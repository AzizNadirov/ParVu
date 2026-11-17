//! Parameter types (sets of type families) and the signature checks.

use crate::sql::is_safe_type_name;
use crate::types::TypeFamily;

use super::validate::Issue;
use super::{DefKind, Definition, MAX_PARAMS, ParamKind, ResultKind};

/// A named set of type families, as the signature builder offers them.
pub struct TypeSet {
    pub name: &'static str,
    pub families: &'static [TypeFamily],
}

use TypeFamily as F;

/// Every type set name a signature may use.
pub const TYPE_SETS: &[TypeSet] = &[
    TypeSet {
        name: "any",
        families: &[
            F::Integer, F::Decimal, F::Float, F::Text, F::Boolean, F::Date, F::Timestamp, F::TimestampTz, F::Time,
            F::Interval, F::Json, F::List, F::Struct, F::Map, F::Binary, F::Other,
        ],
    },
    TypeSet { name: "number", families: &[F::Integer, F::Decimal, F::Float] },
    TypeSet { name: "integer", families: &[F::Integer] },
    TypeSet { name: "decimal", families: &[F::Decimal] },
    TypeSet { name: "float", families: &[F::Float] },
    TypeSet { name: "text", families: &[F::Text] },
    TypeSet { name: "boolean", families: &[F::Boolean] },
    TypeSet { name: "date", families: &[F::Date] },
    TypeSet { name: "datetime", families: &[F::Timestamp, F::TimestampTz] },
    TypeSet { name: "date_or_datetime", families: &[F::Date, F::Timestamp, F::TimestampTz] },
    TypeSet { name: "time", families: &[F::Time] },
    TypeSet { name: "interval", families: &[F::Interval] },
    TypeSet { name: "json", families: &[F::Json] },
    TypeSet { name: "list", families: &[F::List] },
    TypeSet { name: "struct", families: &[F::Struct] },
];

/// The families of a list of set names; empty means any. `None` names an
/// unknown set.
pub fn families_of(names: &[String]) -> Result<Vec<TypeFamily>, String> {
    if names.is_empty() {
        return Ok(TYPE_SETS[0].families.to_vec());
    }
    let mut out = Vec::new();
    for n in names {
        let set = TYPE_SETS.iter().find(|s| s.name.eq_ignore_ascii_case(n.trim())).ok_or_else(|| n.clone())?;
        for f in set.families {
            if !out.contains(f) {
                out.push(*f);
            }
        }
    }
    Ok(out)
}

/// The families validation probes for a set: one representative type
/// each. `any` probes a fixed spread instead of every family.
pub fn probe_families(names: &[String]) -> Vec<TypeFamily> {
    let any = names.is_empty() || names.iter().any(|n| n.eq_ignore_ascii_case("any"));
    if any {
        return vec![F::Integer, F::Float, F::Text, F::Date, F::Boolean];
    }
    families_of(names).unwrap_or_default()
}

/// The DuckDB type a probe uses for a family.
pub fn probe_type(f: TypeFamily) -> &'static str {
    match f {
        F::Integer => "BIGINT",
        F::Decimal => "DECIMAL(18,3)",
        F::Float => "DOUBLE",
        F::Text => "VARCHAR",
        F::Boolean => "BOOLEAN",
        F::Date => "DATE",
        F::Timestamp => "TIMESTAMP",
        F::TimestampTz => "TIMESTAMP WITH TIME ZONE",
        F::Time => "TIME",
        F::Interval => "INTERVAL",
        F::Json => "JSON",
        F::List => "VARCHAR[]",
        F::Struct => "STRUCT(a VARCHAR)",
        F::Map => "MAP(VARCHAR, VARCHAR)",
        F::Binary => "BLOB",
        F::Other => "VARCHAR",
    }
}

/// The family's name as the files and the UI spell it.
pub fn family_name(f: TypeFamily) -> String {
    serde_json::to_value(f).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// DuckDB types a value parameter may declare.
pub const VALUE_TYPES: &[&str] =
    &["VARCHAR", "BIGINT", "INTEGER", "DOUBLE", "DECIMAL(18,3)", "BOOLEAN", "DATE", "TIMESTAMP", "TIME", "INTERVAL"];

/// `[a-z][a-z0-9_]{0,39}`, the rule for definition names.
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && name.len() <= 40
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `[a-z_][a-z0-9_]*`, at most 40 characters, the rule for parameter names.
pub fn is_valid_param_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase() || c == '_')
        && name.len() <= 40
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Check 2: the signature on its own (no database). `reserved` tells
/// whether a word is a reserved SQL keyword.
pub fn check_signature(def: &Definition, reserved: &dyn Fn(&str) -> bool) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut fail = |key: &str, args: &[(&str, &str)]| out.push(Issue::new(key, args));
    if def.params.len() > MAX_PARAMS {
        fail("custom.check.too_many_params", &[("max", &MAX_PARAMS.to_string())]);
    }
    let tables = def.tables().count();
    match def.kind {
        DefKind::Function if tables > 0 => fail("custom.check.function_table_param", &[]),
        DefKind::Operation if tables == 0 => fail("custom.check.operation_needs_table", &[]),
        _ => {}
    }
    if def.kind == DefKind::Function && def.params.iter().all(|p| p.kind != ParamKind::Column) {
        fail("custom.check.function_needs_column", &[]);
    }
    match (def.kind, def.result.kind) {
        (DefKind::Function, ResultKind::Table) => fail("custom.check.function_table_result", &[]),
        (DefKind::Operation, k) if k != ResultKind::Table => fail("custom.check.operation_table_result", &[]),
        _ => {}
    }
    if let Err(bad) = families_of(&def.result.types) {
        fail("custom.check.unknown_type", &[("type", &bad), ("param", "→")]);
    }
    let required: Vec<String> =
        def.tables().flat_map(|t| t.columns.iter().map(|c| c.name.to_lowercase())).collect();
    let mut seen: Vec<String> = Vec::new();
    let mut optional_seen = false;
    for p in &def.params {
        let n = p.name.as_str();
        let lower = n.to_lowercase();
        if !is_valid_param_name(n) {
            fail("custom.check.param_name", &[("param", n)]);
        } else if n.starts_with("__") || lower.contains("__pv_") {
            fail("custom.check.param_reserved", &[("param", n)]);
        } else if reserved(&lower) {
            fail("custom.check.param_keyword", &[("param", n)]);
        }
        if seen.contains(&lower) {
            fail("custom.check.param_duplicate", &[("param", n)]);
        }
        seen.push(lower.clone());
        if p.kind != ParamKind::Table && required.contains(&lower) {
            fail("custom.check.param_is_column", &[("param", n)]);
        }
        if p.optional {
            optional_seen = true;
        } else if optional_seen {
            fail("custom.check.optional_order", &[("param", n)]);
        }
        match p.kind {
            ParamKind::Column => {
                if let Err(bad) = families_of(&p.types) {
                    fail("custom.check.unknown_type", &[("type", &bad), ("param", n)]);
                }
                if p.optional {
                    fail("custom.check.only_values_optional", &[("param", n)]);
                }
                if let Some(of) = &p.of
                    && !def.tables().any(|t| t.name.eq_ignore_ascii_case(of))
                {
                    fail("custom.check.unknown_table_param", &[("param", n), ("table", of)]);
                }
            }
            ParamKind::Value => {
                let dtype = p.dtype.as_deref().unwrap_or("");
                if !is_safe_type_name(dtype) || crate::types::TypeFamily::of(dtype) == TypeFamily::Other {
                    fail("custom.check.value_type", &[("param", n), ("type", dtype)]);
                } else if let Some(d) = &p.default
                    && let Err(e) = crate::ops::typed_literal(d, dtype)
                {
                    fail("custom.check.bad_default", &[("param", n), ("error", &e.to_string())]);
                }
            }
            ParamKind::Table => {
                if p.optional {
                    fail("custom.check.only_values_optional", &[("param", n)]);
                }
                for c in &p.columns {
                    if c.name.trim().is_empty() || c.name.contains("__pv_") {
                        fail("custom.check.required_column_name", &[("param", n), ("column", &c.name)]);
                    }
                    if let Err(bad) = families_of(&c.types) {
                        fail("custom.check.unknown_type", &[("type", &bad), ("param", n)]);
                    }
                }
            }
        }
    }
    if let Some(s) = &def.result.same_as {
        match def.param(s) {
            Some(p) if p.kind == ParamKind::Column => {}
            _ => fail("custom.check.same_as", &[("param", s)]),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::{Body, Param, ResultSpec};

    fn func(params: Vec<Param>) -> Definition {
        Definition {
            name: "f".into(),
            params,
            result: ResultSpec::default(),
            body: Body { source: "x".into(), ..Default::default() },
            ..Default::default()
        }
    }

    fn col(name: &str, ty: &str) -> Param {
        Param { name: name.into(), kind: ParamKind::Column, types: vec![ty.into()], ..Default::default() }
    }

    #[test]
    fn sets_expand() {
        assert_eq!(families_of(&["number".into()]).unwrap(), vec![F::Integer, F::Decimal, F::Float]);
        assert_eq!(families_of(&["text".into(), "text".into()]).unwrap(), vec![F::Text]);
        assert!(families_of(&["nope".into()]).is_err());
        assert_eq!(probe_families(&["any".into()]).len(), 5);
        assert_eq!(family_name(F::TimestampTz), "timestamp_tz");
    }

    #[test]
    fn names() {
        assert!(is_valid_name("clean_phone"));
        assert!(!is_valid_name("Clean"));
        assert!(!is_valid_name("_x"));
        assert!(!is_valid_name("1x"));
        assert!(is_valid_param_name("_x"));
        assert!(!is_valid_param_name("a-b"));
    }

    #[test]
    fn signature_checks() {
        let no = |_: &str| false;
        let keys = |d: &Definition| check_signature(d, &|w| w == "group").into_iter().map(|i| i.key).collect::<Vec<_>>();
        assert!(check_signature(&func(vec![col("x", "number")]), &no).is_empty());
        assert_eq!(keys(&func(vec![col("x", "number"), col("x", "text")])), ["custom.check.param_duplicate"]);
        assert_eq!(keys(&func(vec![col("group", "any")])), ["custom.check.param_keyword"]);
        assert_eq!(keys(&func(vec![col("x", "numbr")])), ["custom.check.unknown_type"]);
        let mut v = Param { name: "n".into(), kind: ParamKind::Value, dtype: Some("BIGINT".into()), optional: true, ..Default::default() };
        v.default = Some("abc".into());
        assert_eq!(keys(&func(vec![col("x", "number"), v.clone()])), ["custom.check.bad_default"]);
        v.default = Some("3".into());
        assert_eq!(keys(&func(vec![v.clone(), col("x", "number")])), ["custom.check.optional_order"]);
        let t = Param { name: "t".into(), kind: ParamKind::Table, ..Default::default() };
        assert_eq!(keys(&func(vec![col("x", "any"), t])), ["custom.check.function_table_param"]);
        let mut op = func(vec![col("x", "any")]);
        op.kind = DefKind::Operation;
        op.result.kind = ResultKind::Table;
        assert_eq!(keys(&op), ["custom.check.operation_needs_table"]);
    }
}
