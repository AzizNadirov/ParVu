//! Custom functions in SQL mode and in read-only queries (MCP `query`, the
//! assistant).
//!
//! Custom functions are not registered with DuckDB (no macros: they would
//! be catalog state that cloned connections, steps and Refresh depend on,
//! and the read-only guard couldn't see inside them; plans §5.5). Instead a
//! query is rewritten before it runs: DuckDB parses it
//! (`json_serialize_sql`), every call of a usable custom function that is
//! not a DuckDB function is replaced by the function's compiled template
//! with each argument's expression tree in its slot, and the tree is printed
//! back (`json_deserialize_sql`). The result is self-contained SQL, so a
//! step keeps working when the definition later changes.
//!
//! - Column parameters take any expression; value parameters are cast to
//!   their declared type; a left-out optional value takes its default.
//! - Arguments are positional or named (`clean_phone(phone := p)`).
//! - A summary function with `OVER (…)` or `FILTER (WHERE …)`: the window or
//!   the filter is given to every aggregate in its template, so
//!   `weighted_average(x, w) OVER (PARTITION BY g)` — `sum(x * w) / sum(w)` —
//!   becomes `sum(x * w) OVER (PARTITION BY g) / sum(w) OVER (PARTITION BY g)`.
//! - `columns` parameters, custom operations, `DISTINCT` and `ORDER BY`
//!   inside the call, and `OVER` on a row function are refused with a
//!   message naming the function.
//! - A query with no custom call is returned unchanged, byte for byte.
//! - An unaliased select item that calls a custom function keeps the name
//!   DuckDB would give the call (`clean_phone(Phone)`), not the expansion.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use duckdb::Connection;
use serde_json::{Value, json};

use super::fill::value_literal;
use super::{DefKind, Definition, Library, ParamKind, Shape};
use crate::error::{CoreError, Result};
use crate::sql::{is_safe_type_name, quote_literal};

/// A custom function a query called.
#[derive(Debug, Clone)]
pub struct Used {
    pub def: Arc<Definition>,
    /// Lower-cased names of the DuckDB functions in its template, to tell
    /// whether an error comes from inside it.
    functions: BTreeSet<String>,
}

/// A query with its custom calls expanded.
#[derive(Debug, Clone)]
pub struct Expanded {
    /// The SQL to run: the input itself when it calls no custom function.
    pub sql: String,
    /// The custom functions it called, in name order.
    pub used: Vec<Used>,
}

impl Expanded {
    fn unchanged(sql: &str) -> Self {
        Self { sql: sql.to_string(), used: Vec::new() }
    }

    /// Whether the query called a custom function.
    pub fn is_expanded(&self) -> bool {
        !self.used.is_empty()
    }

    /// An error from running the expanded SQL, made to name the custom
    /// function it most likely comes from. DuckDB's "LINE 1: …" excerpt is
    /// dropped: it would show the expansion, not what the user typed.
    pub fn explain(&self, err: CoreError) -> CoreError {
        match err {
            CoreError::Db(e) if self.is_expanded() => self.explain_message(&crate::readonly::clean_db_error(&e.to_string())),
            other => other,
        }
    }

    /// [`Self::explain`] for an error message.
    pub fn explain_message(&self, msg: &str) -> CoreError {
        let lower = msg.to_lowercase();
        let culprit = self.used.iter().find(|u| u.functions.iter().any(|f| lower.contains(&format!("{f}("))));
        CoreError::invalid(match culprit {
            Some(u) => format!(
                "{}: an argument has a type this function doesn't take. It is {}\n\n{msg}",
                u.def.name,
                u.def.signature()
            ),
            None if lower.contains("referenced column") || lower.contains("catalog error") => msg.to_string(),
            None => {
                let names: Vec<&str> = self.used.iter().map(|u| u.def.name.as_str()).collect();
                let s = if names.len() > 1 { "s" } else { "" };
                format!("{msg}\n(The query calls the custom function{s} {}.)", names.join(", "))
            }
        })
    }
}

fn err(msg: impl Into<String>) -> CoreError {
    CoreError::invalid(msg)
}

fn serialize(conn: &Connection, sql: &str) -> Option<Value> {
    let json: String = conn.query_row("SELECT json_serialize_sql(?)", [sql], |r| r.get(0)).ok()?;
    let tree: Value = serde_json::from_str(&json).ok()?;
    (tree["error"].as_bool() != Some(true) && tree["statements"].as_array().is_some_and(|s| s.len() == 1)).then_some(tree)
}

fn deserialize(conn: &Connection, tree: &Value) -> Result<String> {
    let text = serde_json::to_string(tree).map_err(|e| err(e.to_string()))?;
    Ok(conn.query_row("SELECT json_deserialize_sql(?::JSON)", [text], |r| r.get(0))?)
}

/// The tree of one SQL expression.
fn expression(conn: &Connection, expr: &str) -> Result<Value> {
    let tree = serialize(conn, &format!("SELECT {expr}")).ok_or_else(|| err(format!("could not read {expr}")))?;
    Ok(tree["statements"][0]["node"]["select_list"][0].clone())
}

fn str_of<'a>(map: &'a serde_json::Map<String, Value>, key: &str) -> &'a str {
    map.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The lower-cased name of a custom call: a function or window node with no
/// schema or catalog whose name is in the library.
fn call_name(map: &serde_json::Map<String, Value>, lib: &Library) -> Option<String> {
    let class = str_of(map, "class");
    if class != "FUNCTION" && class != "WINDOW" {
        return None;
    }
    if !str_of(map, "schema").is_empty() || !str_of(map, "catalog").is_empty() {
        return None;
    }
    let name = str_of(map, "function_name").to_lowercase();
    lib.get(&name).is_some().then_some(name)
}

fn collect_calls(v: &Value, lib: &Library, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(map) => {
            if let Some(name) = call_name(map, lib) {
                out.insert(name);
            }
            map.values().for_each(|c| collect_calls(c, lib, out));
        }
        Value::Array(items) => items.iter().for_each(|c| collect_calls(c, lib, out)),
        _ => {}
    }
}

fn contains_call(v: &Value, names: &BTreeSet<String>) -> bool {
    match v {
        Value::Object(map) => {
            let class = str_of(map, "class");
            ((class == "FUNCTION" || class == "WINDOW")
                && str_of(map, "schema").is_empty()
                && names.contains(&str_of(map, "function_name").to_lowercase()))
                || map.values().any(|c| contains_call(c, names))
        }
        Value::Array(items) => items.iter().any(|c| contains_call(c, names)),
        _ => false,
    }
}

/// Lower-cased names among `names` that DuckDB knows, with whether any of
/// their overloads is an aggregate.
fn duckdb_functions(conn: &Connection, names: &BTreeSet<String>) -> Result<HashMap<String, bool>> {
    if names.is_empty() {
        return Ok(HashMap::new());
    }
    let list: Vec<String> = names.iter().map(|n| quote_literal(n)).collect();
    let sql = format!(
        "SELECT lower(function_name), bool_or(function_type = 'aggregate') FROM duckdb_functions() \
         WHERE lower(function_name) IN ({}) GROUP BY 1",
        list.join(", ")
    );
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// Lower-cased names of the functions called in `v`.
fn function_names(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(map) => {
            let class = str_of(map, "class");
            if class == "FUNCTION" || class == "WINDOW" {
                out.insert(str_of(map, "function_name").to_lowercase());
            }
            map.values().for_each(|c| function_names(c, out));
        }
        Value::Array(items) => items.iter().for_each(|c| function_names(c, out)),
        _ => {}
    }
}

/// Expand the custom function calls in `sql` with `lib`'s definitions.
///
/// A query that doesn't parse, isn't one SELECT, or calls no custom function
/// comes back as it is (DuckDB reports its own errors when it runs).
pub fn expand(conn: &Connection, sql: &str, lib: &Library) -> Result<Expanded> {
    if lib.is_empty() {
        return Ok(Expanded::unchanged(sql));
    }
    // Cheap test first: no name of the library appears in the text.
    let lower = sql.to_lowercase();
    if !lib.definitions().iter().any(|d| lower.contains(&d.name.to_lowercase())) {
        return Ok(Expanded::unchanged(sql));
    }
    let Some(mut tree) = serialize(conn, sql) else { return Ok(Expanded::unchanged(sql)) };
    let mut names = BTreeSet::new();
    collect_calls(&tree, lib, &mut names);
    // A DuckDB function of the same name wins (validation refuses such
    // names, but DuckDB may have gained one since).
    let builtins = duckdb_functions(conn, &names)?;
    names.retain(|n| !builtins.contains_key(n));
    if names.is_empty() {
        return Ok(Expanded::unchanged(sql));
    }
    let mut x = Expander { conn, lib, names, used: Vec::new(), templates: HashMap::new(), plain: None };
    let node = &mut tree["statements"][0]["node"];
    x.name_items(node)?;
    x.visit(node)?;
    let out = deserialize(conn, &tree)?;
    let mut used = x.used;
    used.sort_by(|a, b| a.def.name.cmp(&b.def.name));
    Ok(Expanded { sql: out, used })
}

struct Expander<'a> {
    conn: &'a Connection,
    lib: &'a Library,
    /// Lower-cased names of the custom functions to expand.
    names: BTreeSet<String>,
    used: Vec<Used>,
    /// Lower-cased name → (template tree, aggregate function names in it).
    templates: HashMap<String, (Value, BTreeSet<String>)>,
    /// `SELECT 1`, the statement select items are printed in.
    plain: Option<Value>,
}

impl Expander<'_> {
    /// Give each unaliased select item that calls a custom function the
    /// name DuckDB would give it, so the result column is `clean_phone(p)`
    /// rather than the text of the expansion.
    fn name_items(&mut self, v: &mut Value) -> Result<()> {
        match v {
            Value::Object(map) => {
                if str_of(map, "type") == "SELECT_NODE"
                    && let Some(Value::Array(items)) = map.get_mut("select_list")
                {
                    for item in items.iter_mut() {
                        let unnamed = item["alias"].as_str().is_none_or(str::is_empty);
                        let star = matches!(item["class"].as_str(), Some("STAR"));
                        if unnamed && !star && contains_call(item, &self.names) {
                            let name = self.text_of(item)?;
                            item["alias"] = Value::String(name);
                        }
                    }
                }
                for c in map.values_mut() {
                    self.name_items(c)?;
                }
            }
            Value::Array(items) => {
                for c in items {
                    self.name_items(c)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// An expression printed as DuckDB prints it.
    fn text_of(&mut self, item: &Value) -> Result<String> {
        if self.plain.is_none() {
            self.plain = Some(serialize(self.conn, "SELECT 1").ok_or_else(|| err("could not read SELECT 1"))?);
        }
        let mut tree = self.plain.clone().unwrap_or_default();
        let mut item = item.clone();
        item["alias"] = Value::String(String::new());
        tree["statements"][0]["node"]["select_list"] = json!([item]);
        let text = deserialize(self.conn, &tree)?;
        Ok(text.strip_prefix("SELECT ").unwrap_or(&text).trim().to_string())
    }

    fn visit(&mut self, v: &mut Value) -> Result<()> {
        match v {
            Value::Object(map) => {
                if str_of(map, "type") == "TABLE_FUNCTION"
                    && let Some(f) = map.get("function").and_then(Value::as_object)
                    && let Some(name) = call_name(f, self.lib)
                    && self.names.contains(&name)
                {
                    let def = self.lib.get(&name).ok_or_else(|| err(name.clone()))?;
                    return Err(err(match def.kind {
                        DefKind::Operation => format!(
                            "{} is a custom operation: SQL mode can't call it. Apply it from the Custom tools menu.",
                            def.name
                        ),
                        DefKind::Function => {
                            format!("{} is a custom function: call it on columns (SELECT {}(…) FROM …), not as a table.", def.name, def.name)
                        }
                    }));
                }
                // Arguments first, so calls nested in arguments are expanded.
                for c in map.values_mut() {
                    self.visit(c)?;
                }
                if let Some(name) = call_name(map, self.lib)
                    && self.names.contains(&name)
                {
                    let call = std::mem::take(map);
                    *v = self.expand_call(&name, call)?;
                }
            }
            Value::Array(items) => {
                for c in items {
                    self.visit(c)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The template of `def` as a tree, and the aggregates it calls.
    fn template(&mut self, def: &Arc<Definition>) -> Result<(Value, BTreeSet<String>)> {
        let key = def.name.to_lowercase();
        if let Some(t) = self.templates.get(&key) {
            return Ok(t.clone());
        }
        let compiled = def.compiled.as_ref().ok_or_else(|| needs_attention(def))?;
        let tree = expression(self.conn, &compiled.template)?;
        let mut functions = BTreeSet::new();
        function_names(&tree, &mut functions);
        let aggregates: BTreeSet<String> =
            duckdb_functions(self.conn, &functions)?.into_iter().filter(|(_, agg)| *agg).map(|(n, _)| n).collect();
        self.used.push(Used { def: def.clone(), functions });
        self.templates.insert(key, (tree.clone(), aggregates.clone()));
        Ok((tree, aggregates))
    }

    fn expand_call(&mut self, name: &str, call: serde_json::Map<String, Value>) -> Result<Value> {
        let def = self.lib.get(name).ok_or_else(|| err(name.to_string()))?;
        let fname = def.name.clone();
        if def.kind == DefKind::Operation {
            return Err(err(format!(
                "{fname} is a custom operation: SQL mode can't call it. Apply it from the Custom tools menu."
            )));
        }
        if !def.is_usable() {
            return Err(needs_attention(&def));
        }
        if let Some(p) = def.params.iter().find(|p| p.kind == ParamKind::Columns) {
            return Err(err(format!(
                "{fname} takes several columns ({}), which SQL mode can't pass. Use it from the Custom tools menu.",
                p.name
            )));
        }
        let shape = def.compiled.as_ref().map(|c| c.shape).unwrap_or_default();
        let window = str_of(&call, "class") == "WINDOW";
        let summary = shape == Shape::Summary;
        if window && !summary {
            return Err(err(format!("{fname} is a row function: OVER (…) can't be used with it.")));
        }
        if call.get("distinct").and_then(Value::as_bool) == Some(true) {
            return Err(err(format!("{fname}: DISTINCT can't be used in a custom function call.")));
        }
        let ordered = if window {
            call.get("arg_orders").and_then(Value::as_array).is_some_and(|o| !o.is_empty())
        } else {
            call.get("order_bys").and_then(|o| o["orders"].as_array()).is_some_and(|o| !o.is_empty())
        };
        if ordered {
            return Err(err(format!("{fname}: ORDER BY can't be used inside a custom function call.")));
        }
        if call.get("ignore_nulls").and_then(Value::as_bool) == Some(true) {
            return Err(err(format!("{fname}: IGNORE NULLS can't be used with a custom function.")));
        }
        let filter_key = if window { "filter_expr" } else { "filter" };
        let filter = call.get(filter_key).filter(|f| !f.is_null()).cloned();
        if filter.is_some() && !summary {
            return Err(err(format!("{fname} is a row function: FILTER (WHERE …) can't be used with it.")));
        }

        let args = self.bind(&def, &call)?;
        let (mut tree, aggregates) = self.template(&def)?;
        if window || filter.is_some() {
            let mut n = 0;
            distribute(&mut tree, &aggregates, window.then_some(&call), filter.as_ref(), &fname, &mut n)?;
            if n == 0 {
                return Err(err(format!("{fname}: its body has no aggregate to apply OVER or FILTER to.")));
            }
        }
        fill(&mut tree, &args);
        tree["alias"] = Value::String(str_of(&call, "alias").to_string());
        Ok(tree)
    }

    /// The expression of each parameter, by index: the argument (a value
    /// argument cast to its type) or an optional value's default.
    fn bind(&self, def: &Definition, call: &serde_json::Map<String, Value>) -> Result<Vec<Value>> {
        let fname = &def.name;
        let params = &def.params;
        let given: Vec<(String, Value)> = call
            .get("arguments")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|x| (x["name"].as_str().unwrap_or("").to_string(), x["expression"].clone()))
                    .collect()
            })
            .unwrap_or_default();
        let mut slots: Vec<Option<Value>> = vec![None; params.len()];
        let mut next = 0;
        for (name, mut expr) in given {
            let i = if name.is_empty() {
                if next >= params.len() {
                    return Err(arity(def));
                }
                next += 1;
                next - 1
            } else {
                params
                    .iter()
                    .position(|p| p.name.eq_ignore_ascii_case(&name))
                    .ok_or_else(|| err(format!("{fname} has no parameter {name}. It is {}", def.signature())))?
            };
            if slots[i].is_some() {
                return Err(err(format!("{fname}: {} is given twice.", params[i].name)));
            }
            if let Some(m) = expr.as_object_mut() {
                m.insert("alias".into(), Value::String(String::new()));
            }
            slots[i] = Some(expr);
        }
        let mut out = Vec::with_capacity(params.len());
        for (p, slot) in params.iter().zip(slots) {
            let expr = match (slot, p.kind) {
                (Some(e), ParamKind::Value) => self.cast(p.dtype.as_deref().unwrap_or("VARCHAR"), e)?,
                (Some(e), _) => e,
                (None, ParamKind::Value) if p.optional => {
                    let lit = value_literal(p, p.default.as_deref())?;
                    self.cast(p.dtype.as_deref().unwrap_or("VARCHAR"), expression(self.conn, &lit)?)?
                }
                (None, _) => return Err(arity(def)),
            };
            out.push(expr);
        }
        Ok(out)
    }

    /// `CAST(expr AS dtype)`.
    fn cast(&self, dtype: &str, expr: Value) -> Result<Value> {
        if !is_safe_type_name(dtype) {
            return Err(err(format!("Invalid type: {dtype}")));
        }
        let mut node = expression(self.conn, &format!("CAST(NULL AS {dtype})"))?;
        node["child"] = expr;
        Ok(node)
    }
}

fn needs_attention(def: &Definition) -> CoreError {
    err(format!("Custom function {} needs attention: open it in Custom tools and validate it.", def.name))
}

fn arity(def: &Definition) -> CoreError {
    let total = def.params.len();
    let required = def.params.iter().filter(|p| !(p.kind == ParamKind::Value && p.optional)).count();
    let count = if required == total { format!("{total}") } else { format!("{required} to {total}") };
    let s = if total == 1 { "" } else { "s" };
    err(format!("{} takes {count} argument{s}: {}", def.name, def.signature()))
}

/// Give the call's window (`over`) or filter to each aggregate in a
/// summary template (not inside a window already). Counts them in `n`.
fn distribute(
    v: &mut Value,
    aggregates: &BTreeSet<String>,
    over: Option<&serde_json::Map<String, Value>>,
    filter: Option<&Value>,
    fname: &str,
    n: &mut usize,
) -> Result<()> {
    match v {
        Value::Object(map) => {
            let class = str_of(map, "class");
            if class == "WINDOW" || class == "SUBQUERY" {
                return Ok(());
            }
            if class == "FUNCTION" && aggregates.contains(&str_of(map, "function_name").to_lowercase()) {
                *n += 1;
                let own_filter = map.get("filter").filter(|f| !f.is_null()).cloned();
                let filter = match (own_filter, filter) {
                    (Some(_), Some(_)) => {
                        return Err(err(format!("{fname}: its body already filters an aggregate, so FILTER can't be added.")));
                    }
                    (own, None) => own,
                    (None, Some(f)) => Some(f.clone()),
                };
                match over {
                    Some(window) => {
                        let mut w = window.clone();
                        for key in ["function_name", "qualified_name", "schema", "catalog", "arguments", "distinct"] {
                            w.insert(key.into(), map.get(key).cloned().unwrap_or(Value::Null));
                        }
                        w.insert("arg_orders".into(), map.get("order_bys").map(|o| o["orders"].clone()).unwrap_or(json!([])));
                        w.insert("filter_expr".into(), filter.unwrap_or(Value::Null));
                        w.insert("alias".into(), Value::String(String::new()));
                        w.insert("type".into(), Value::String("WINDOW_AGGREGATE".into()));
                        *map = w;
                    }
                    None => {
                        map.insert("filter".into(), filter.unwrap_or(Value::Null));
                    }
                }
                return Ok(());
            }
            for c in map.values_mut() {
                distribute(c, aggregates, over, filter, fname, n)?;
            }
        }
        Value::Array(items) => {
            for c in items {
                distribute(c, aggregates, over, filter, fname, n)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// The slot index of a template column reference `__pv_arg_N[.field…]`.
fn slot_of(names: &[Value]) -> Option<usize> {
    let first = names.first()?.as_str()?.to_lowercase();
    first.strip_prefix("__pv_arg_")?.parse().ok()
}

/// Put each argument's tree in place of its slots.
fn fill(v: &mut Value, args: &[Value]) {
    match v {
        Value::Object(map) => {
            if str_of(map, "class") == "COLUMN_REF"
                && let Some(names) = map.get("column_names").and_then(Value::as_array)
                && let Some(i) = slot_of(names)
                && i < args.len()
            {
                // `slot.field.sub`: the fields of a struct argument.
                let fields: Vec<String> = names.iter().skip(1).filter_map(|n| n.as_str().map(str::to_string)).collect();
                let mut node = args[i].clone();
                for f in fields {
                    node = json!({
                        "alias": "", "class": "OPERATOR", "type": "STRUCT_EXTRACT",
                        "query_location": u64::MAX, "query_location_length": 0,
                        "children": [node, {
                            "alias": "", "class": "CONSTANT", "type": "VALUE_CONSTANT",
                            "literal": { "kind": "STRING", "text": f },
                            "query_location": u64::MAX, "query_location_length": 0
                        }]
                    });
                }
                *v = node;
                return;
            }
            map.values_mut().for_each(|c| fill(c, args));
        }
        Value::Array(items) => items.iter_mut().for_each(|c| fill(c, args)),
        _ => {}
    }
}
