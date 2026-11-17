//! Filling a template's slots with arguments, and the SQL an application
//! of a definition builds (plans/CUSTOM_TOOLS_PLAN.md §5.2, §5.3, §6.8).
//!
//! Column arguments become quoted identifiers, values typed literals in
//! parentheses, tables the CTEs `__pv_tab_N`. Bodies may not contain the
//! text `__pv_`, so every slot in a template is one the compiler wrote.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::signature::families_of;
use super::{DefKind, Definition, ParamKind, Placement, ResultKind};
use crate::clean::{RowFilter, label_where, only_where};
use crate::colfn::Output;
use crate::error::{CoreError, Result};
use crate::ops::{OpContext, StepLabel, typed_literal, unique_name};
use crate::page::ColumnMeta;
use crate::sql::{is_safe_type_name, quote_ident, quote_literal};
use crate::types::TypeFamily;

/// An argument of a custom function or operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CustomArg {
    /// A column of the table, by name.
    Column(String),
    /// A constant, as text (`"3"`, `"2024-01-31"`, `"+"`).
    Value(String),
    /// Another open table, by name.
    Table(String),
}

fn slot_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""?__pv_arg_([0-9]+)"?"#).expect("slot pattern"))
}

/// Replace each `__pv_arg_N` in `template` with `slot(N)`.
pub fn fill_slots(template: &str, slot: &dyn Fn(usize) -> String) -> String {
    slot_re()
        .replace_all(template, |c: &regex::Captures| c[1].parse::<usize>().map(slot).unwrap_or_default())
        .into_owned()
}

/// `WITH "__pv_tab_0" AS (q0), … SELECT * FROM (<template>)`: a table
/// template over its tables. The template may have CTEs of its own, so it
/// is nested rather than prefixed.
pub fn with_tables(tables: &[String], body: &str) -> String {
    if tables.is_empty() {
        return body.to_string();
    }
    let ctes: Vec<String> =
        tables.iter().enumerate().map(|(k, q)| format!("{} AS ({q})", quote_ident(&super::compile::table_slot(k)))).collect();
    format!("WITH {} SELECT * FROM ({body})", ctes.join(", "))
}

/// SQL literal for a value argument of type `dtype`, checked against the
/// parameter's choices and bounds; `None` is an empty value.
pub fn value_literal(p: &super::Param, raw: Option<&str>) -> Result<String> {
    let dtype = p.dtype.as_deref().unwrap_or("VARCHAR");
    if !is_safe_type_name(dtype) {
        return Err(CoreError::invalid(format!("Invalid type: {dtype}")));
    }
    let Some(raw) = raw else { return Ok(format!("CAST(NULL AS {dtype})")) };
    if !p.choices.is_empty() && !p.choices.iter().any(|c| c == raw) {
        return Err(CoreError::invalid(format!("{}: choose one of {}", p.name, p.choices.join(", "))));
    }
    let family = TypeFamily::of(dtype);
    if family == TypeFamily::Integer && raw.trim().parse::<i64>().is_err() {
        return Err(CoreError::invalid(format!("{}: '{raw}' is not a whole number", p.name)));
    }
    if matches!(family, TypeFamily::Integer | TypeFamily::Decimal | TypeFamily::Float) {
        let n: f64 = raw.trim().parse().map_err(|_| CoreError::invalid(format!("{}: '{raw}' is not a number", p.name)))?;
        if p.min.is_some_and(|m| n < m) || p.max.is_some_and(|m| n > m) {
            let range = format!("{}…{}", p.min.map(crate::ops::fmt_number).unwrap_or_default(), p.max.map(crate::ops::fmt_number).unwrap_or_default());
            return Err(CoreError::invalid(format!("{}: {raw} is outside {range}", p.name)));
        }
        let lit = typed_literal(raw, dtype)?;
        return Ok(if family == TypeFamily::Decimal { format!("CAST({lit} AS {dtype})") } else { lit });
    }
    if family == TypeFamily::Text {
        return Ok(quote_literal(raw));
    }
    typed_literal(raw, dtype)
}

/// "clean_phone (Phone)": the definition's name and the column and value
/// arguments, cut at 60 characters like a query step.
pub fn call_label(name: &str, def: Option<&Definition>, args: &BTreeMap<String, CustomArg>) -> StepLabel {
    let order = |k: &str| def.and_then(|d| d.params.iter().position(|p| p.name.eq_ignore_ascii_case(k))).unwrap_or(usize::MAX);
    let mut items: Vec<(&String, &CustomArg)> = args.iter().collect();
    items.sort_by_key(|(k, _)| order(k));
    let text: Vec<String> = items
        .iter()
        .map(|(_, a)| match a {
            CustomArg::Column(c) | CustomArg::Value(c) | CustomArg::Table(c) => c.clone(),
        })
        .collect();
    let joined = text.join(", ");
    let short = if joined.chars().count() > 60 { format!("{}…", joined.chars().take(60).collect::<String>()) } else { joined };
    StepLabel::new("step.custom", &[("name", name), ("args", &short)])
}

/// The definition `name` from the workspace's library, if it can be used.
fn usable(ctx: &OpContext, name: &str, kind: DefKind) -> Result<std::sync::Arc<Definition>> {
    let what = if kind == DefKind::Function { "Custom function" } else { "Custom operation" };
    let def = (ctx.custom)(name).ok_or_else(|| CoreError::invalid(format!("{what} {name} no longer exists")))?;
    if def.kind != kind {
        return Err(CoreError::invalid(format!("{name} is not a {}", what.to_lowercase())));
    }
    if !def.is_usable() {
        return Err(CoreError::invalid(format!("{what} {name} needs attention: open it in Custom tools and validate it")));
    }
    Ok(def)
}

/// Columns of a query, from DESCRIBE through the context.
fn describe(ctx: &OpContext, query: &str) -> Result<Vec<ColumnMeta>> {
    let rows = (ctx.query)(&format!("DESCRIBE SELECT * FROM ({query})"))?;
    Ok(rows
        .into_iter()
        .filter_map(|r| Some(ColumnMeta::new(r.first()?.clone()?, r.get(1)?.clone()?)))
        .collect())
}

fn check_family(param: &str, column: &ColumnMeta, types: &[String]) -> Result<()> {
    let fams = families_of(types).map_err(|t| CoreError::invalid(format!("Unknown type {t}")))?;
    if !fams.contains(&column.family) {
        return Err(CoreError::invalid(format!(
            "{param}: column \"{}\" is {}, expected {}",
            column.name,
            column.dtype,
            if types.is_empty() { "any".to_string() } else { types.join(" or ") }
        )));
    }
    Ok(())
}

/// Arguments bound to a definition: the text of each slot, and the query
/// of each table parameter.
struct Bound {
    slots: Vec<String>,
    tables: Vec<String>,
    /// The column argument of the first column parameter.
    first_column: Option<String>,
}

fn bind(def: &Definition, args: &BTreeMap<String, CustomArg>, ctx: &OpContext) -> Result<Bound> {
    for k in args.keys() {
        if def.param(k).is_none() {
            return Err(CoreError::invalid(format!("{} has no parameter {k}", def.name)));
        }
    }
    let arg = |name: &str| args.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, a)| a);
    // Tables: the first is this table; the others are open tabs by name.
    let mut tables = Vec::new();
    let mut table_columns: Vec<Vec<ColumnMeta>> = Vec::new();
    for (k, t) in def.tables().enumerate() {
        let (query, columns) = if k == 0 {
            (ctx.current_query.to_string(), ctx.columns.to_vec())
        } else {
            let Some(CustomArg::Table(name)) = arg(&t.name) else {
                return Err(CoreError::invalid(format!("{}: choose a table", t.name)));
            };
            let q = (ctx.other_query)(name).ok_or_else(|| CoreError::invalid(format!("No open table named {name}")))?;
            let cols = describe(ctx, &q)?;
            (q, cols)
        };
        for req in &t.columns {
            let c = columns.iter().find(|c| c.name == req.name).ok_or_else(|| {
                CoreError::invalid(format!("{} needs a column named \"{}\" in {}", def.name, req.name, t.name))
            })?;
            check_family(&req.name, c, &req.types)?;
        }
        tables.push(query);
        table_columns.push(columns);
    }
    let mut slots = vec![String::new(); def.params.len()];
    let mut first_column = None;
    for (i, p) in def.params.iter().enumerate() {
        match p.kind {
            ParamKind::Table => {}
            ParamKind::Column => {
                let Some(CustomArg::Column(c)) = arg(&p.name) else {
                    return Err(CoreError::invalid(format!("{}: choose a column", p.name)));
                };
                let columns: &[ColumnMeta] = match def.kind {
                    DefKind::Function => ctx.columns,
                    DefKind::Operation => {
                        let k = def.table_of(p).and_then(|t| def.tables().position(|x| x.name == t.name)).unwrap_or(0);
                        &table_columns[k]
                    }
                };
                let meta = columns.iter().find(|m| &m.name == c).ok_or_else(|| CoreError::UnknownColumn(c.clone()))?;
                check_family(&p.name, meta, &p.types)?;
                slots[i] = quote_ident(c);
                first_column.get_or_insert_with(|| c.clone());
            }
            ParamKind::Value => {
                let raw = match arg(&p.name) {
                    Some(CustomArg::Value(v)) => Some(v.as_str()),
                    Some(_) => return Err(CoreError::invalid(format!("{}: expected a value", p.name))),
                    None if p.optional => p.default.as_deref(),
                    None => return Err(CoreError::invalid(format!("{}: enter a value", p.name))),
                };
                slots[i] = format!("({})", value_literal(p, raw)?);
            }
        }
    }
    Ok(Bound { slots, tables, first_column })
}

/// The SQL and label of a row function applied to the tab: in place of
/// the first column argument, or into a new column right after it.
pub(crate) fn build_function(
    ctx: &OpContext,
    name: &str,
    args: &BTreeMap<String, CustomArg>,
    output: &Output,
    filter: Option<&RowFilter>,
) -> Result<(String, StepLabel)> {
    let def = usable(ctx, name, DefKind::Function)?;
    let compiled = def.compiled.as_ref().ok_or_else(|| CoreError::invalid("not compiled"))?;
    if def.result.kind != ResultKind::Column {
        return Err(CoreError::invalid(format!("{name} gives one value, not a column; run it from the Column menu")));
    }
    let bound = bind(&def, args, ctx)?;
    let column = bound.first_column.clone().ok_or_else(|| CoreError::invalid("No column argument"))?;
    let expr = fill_slots(&compiled.template, &|i| bound.slots.get(i).cloned().unwrap_or_default());
    // The result's type, checked against the declaration (binds the SQL too).
    let t = quote_ident(ctx.table);
    let cur = ctx.current_query;
    let probe = describe(ctx, &format!("SELECT {expr} AS v FROM ({cur}) AS {t}"))?;
    if let Some(meta) = probe.first() {
        check_result_family(&def, &bound, ctx, meta)?;
    }
    let label = label_where(call_label(name, Some(&def), args), filter);
    let q = quote_ident;
    let sql = match output {
        Output::InPlace => {
            let new = only_where(ctx, filter, &expr, &q(&column))?;
            let items: Vec<String> =
                ctx.columns.iter().map(|c| if c.name == column { format!("{new} AS {}", q(&c.name)) } else { q(&c.name) }).collect();
            format!("SELECT {} FROM ({cur}) AS {t}", items.join(", "))
        }
        Output::New { name: wanted } => {
            let new = only_where(ctx, filter, &expr, "NULL")?;
            let taken: Vec<String> = ctx.columns.iter().map(|c| c.name.clone()).collect();
            let wanted = if wanted.trim().is_empty() { format!("{column}_{name}") } else { wanted.trim().to_string() };
            let out = unique_name(&taken, &wanted);
            let items: Vec<String> = ctx
                .columns
                .iter()
                .flat_map(|c| {
                    if c.name == column { vec![q(&c.name), format!("{new} AS {}", q(&out))] } else { vec![q(&c.name)] }
                })
                .collect();
            format!("SELECT {} FROM ({cur}) AS {t}", items.join(", "))
        }
    };
    Ok((sql, label))
}

fn check_result_family(def: &Definition, bound: &Bound, ctx: &OpContext, meta: &ColumnMeta) -> Result<()> {
    let ok = match &def.result.same_as {
        Some(p) => {
            let i = def.params.iter().position(|x| x.name.eq_ignore_ascii_case(p)).unwrap_or(0);
            let col = bound.slots.get(i).and_then(|s| ctx.columns.iter().find(|c| quote_ident(&c.name) == *s));
            col.is_none_or(|c| c.family == meta.family)
        }
        None => families_of(&def.result.types).is_ok_and(|f| f.contains(&meta.family)),
    };
    if ok {
        Ok(())
    } else {
        Err(CoreError::invalid(format!("{} gave a {} result here, which its definition does not allow", def.name, meta.dtype)))
    }
}

/// The SQL that computes a summary function's value over the tab, as
/// items for [`crate::sql::whole_table`]: `(<value> AS v, source, alias)`.
pub fn summary_parts(ctx: &OpContext, name: &str, args: &BTreeMap<String, CustomArg>) -> Result<(String, String, String)> {
    let def = usable(ctx, name, DefKind::Function)?;
    let compiled = def.compiled.as_ref().ok_or_else(|| CoreError::invalid("not compiled"))?;
    if def.result.kind != ResultKind::Value {
        return Err(CoreError::invalid(format!("{name} gives a column, not one value")));
    }
    let bound = bind(&def, args, ctx)?;
    let expr = fill_slots(&compiled.template, &|i| bound.slots.get(i).cloned().unwrap_or_default());
    let items = format!("CAST({expr} AS VARCHAR) AS v");
    let alias = quote_ident(ctx.table);
    // Binds now: errors show before the job starts.
    (ctx.query)(&format!("DESCRIBE {}", crate::sql::whole_table_plain(&items, ctx.current_query, &alias)))?;
    Ok((items, ctx.current_query.to_string(), alias))
}

/// The SQL and label of a custom operation on the tab.
pub(crate) fn build_operation(
    ctx: &OpContext,
    name: &str,
    args: &BTreeMap<String, CustomArg>,
    new_table: bool,
) -> Result<(String, StepLabel)> {
    let def = usable(ctx, name, DefKind::Operation)?;
    let compiled = def.compiled.as_ref().ok_or_else(|| CoreError::invalid("not compiled"))?;
    if new_table != (def.placement() == Placement::NewTable) {
        return Err(CoreError::invalid(if new_table {
            format!("{name} changes this table; it doesn't open a new one")
        } else {
            format!("{name} opens a new table")
        }));
    }
    let bound = bind(&def, args, ctx)?;
    let body = fill_slots(&compiled.template, &|i| bound.slots.get(i).cloned().unwrap_or_default());
    let sql = with_tables(&bound.tables, &body);
    let columns = describe(ctx, &sql)?;
    check_table_result(&def, &columns)?;
    Ok((sql, call_label(name, Some(&def), args)))
}

/// A table result's columns: unique names, no reserved names, and the
/// declared columns with their types.
pub fn check_table_result(def: &Definition, columns: &[ColumnMeta]) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for c in columns {
        if c.name.to_lowercase().starts_with("__pv_") {
            return Err(CoreError::invalid(format!("{}: the result has a reserved column name {}", def.name, c.name)));
        }
        if !seen.insert(c.name.to_lowercase()) {
            return Err(CoreError::invalid(format!("{}: the result has two columns named {}", def.name, c.name)));
        }
    }
    for req in &def.result.columns {
        let c = columns
            .iter()
            .find(|c| c.name == req.name)
            .ok_or_else(|| CoreError::invalid(format!("{}: the result has no column {}", def.name, req.name)))?;
        check_family(&req.name, c, &req.types)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::Param;

    #[test]
    fn slots_fill_exactly() {
        let t = "f(__pv_arg_1, __pv_arg_10, \"__pv_arg_0\", t.__pv_arg_0)";
        let out = fill_slots(t, &|i| format!("<{i}>"));
        assert_eq!(out, "f(<1>, <10>, <0>, t.<0>)");
    }

    #[test]
    fn values_are_typed_and_checked() {
        let p = |dtype: &str| Param { name: "n".into(), kind: ParamKind::Value, dtype: Some(dtype.into()), ..Default::default() };
        assert_eq!(value_literal(&p("BIGINT"), Some("3")).unwrap(), "3");
        assert!(value_literal(&p("BIGINT"), Some("3.5")).is_err());
        assert!(value_literal(&p("BIGINT"), Some("1; DROP TABLE x")).is_err());
        assert_eq!(value_literal(&p("VARCHAR"), Some("it's")).unwrap(), "'it''s'");
        assert_eq!(value_literal(&p("DATE"), Some("2024-01-31")).unwrap(), "CAST('2024-01-31' AS DATE)");
        assert_eq!(value_literal(&p("DOUBLE"), None).unwrap(), "CAST(NULL AS DOUBLE)");
        let mut bounded = p("DOUBLE");
        bounded.min = Some(1.0);
        assert!(value_literal(&bounded, Some("0.5")).is_err());
        let mut choices = p("VARCHAR");
        choices.choices = vec!["a".into(), "b".into()];
        assert!(value_literal(&choices, Some("c")).is_err());
        assert!(value_literal(&choices, Some("a")).is_ok());
    }

    #[test]
    fn labels_follow_the_signature() {
        let mut args = BTreeMap::new();
        args.insert("b".to_string(), CustomArg::Value("3".into()));
        args.insert("a".to_string(), CustomArg::Column("Phone".into()));
        let def = Definition {
            params: vec![
                Param { name: "a".into(), ..Default::default() },
                Param { name: "b".into(), kind: ParamKind::Value, ..Default::default() },
            ],
            ..Default::default()
        };
        let l = call_label("f", Some(&def), &args);
        assert_eq!(l.key, "step.custom");
        assert_eq!(l.args["args"], "Phone, 3");
    }

    #[test]
    fn tables_nest_the_template() {
        assert_eq!(with_tables(&["Q".into()], "SELECT 1"), "WITH \"__pv_tab_0\" AS (Q) SELECT * FROM (SELECT 1)");
    }
}
