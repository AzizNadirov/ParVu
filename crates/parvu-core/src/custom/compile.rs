//! Body → SQL template (plans/CUSTOM_TOOLS_PLAN.md §5.1).
//!
//! The body is parsed by DuckDB (`json_serialize_sql`), checked by the
//! read-only guard with only the definition's table parameters as tables,
//! then walked: parameter references become the slots `__pv_arg_N`,
//! functions are checked for determinism, and order or memory risks become
//! warnings. `json_deserialize_sql` prints the template.

use std::collections::{BTreeSet, HashMap};

use duckdb::Connection;
use serde_json::Value;

use super::validate::Issue;
use super::{DefKind, Definition, MAX_BODY, ParamKind};
use crate::readonly::guard_select;

/// The table a function body is compiled against (its parameters).
pub(crate) const ARGS_TABLE: &str = "__pv_args";

/// Slot name of parameter `i` in a template.
pub fn arg_slot(i: usize) -> String {
    format!("__pv_arg_{i}")
}

/// Name of table parameter `k` in a template.
pub fn table_slot(k: usize) -> String {
    format!("__pv_tab_{k}")
}

/// What compiling a body gives.
#[derive(Debug, Clone, Default)]
pub struct CompileOutput {
    pub template: String,
    /// A window: the value of a row depends on other rows.
    pub reads_other_rows: bool,
    /// Uses today's date or the current time.
    pub uses_current_time: bool,
    pub warnings: Vec<Issue>,
}

/// How stable a DuckDB function is: 0 always the same, 1 the same within
/// one query (`now()`), 2 volatile or with side effects.
pub type Stability = HashMap<String, u8>;

/// Stability of every function in the database, from `duckdb_functions()`.
pub fn function_stability(conn: &Connection) -> crate::Result<Stability> {
    let mut st = conn.prepare(
        "SELECT lower(function_name), max(CASE WHEN has_side_effects OR stability = 'VOLATILE' THEN 2 \
         WHEN stability = 'CONSISTENT_WITHIN_QUERY' THEN 1 ELSE 0 END)::INTEGER \
         FROM duckdb_functions() GROUP BY 1",
    )?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i32>(1)?)))?;
    let mut out = HashMap::new();
    for r in rows {
        let (name, s) = r?;
        out.insert(name, s.clamp(0, 2) as u8);
    }
    Ok(out)
}

/// Words SQL reserves (`group`, `order`…), lower-cased.
pub fn reserved_words(conn: &Connection) -> crate::Result<BTreeSet<String>> {
    let mut st = conn.prepare("SELECT lower(keyword_name) FROM duckdb_keywords() WHERE keyword_category = 'reserved'")?;
    let rows = st.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Keywords that read the clock but parse as column references.
const CLOCK_WORDS: &[&str] = &["current_date", "current_time", "current_timestamp", "localtime", "localtimestamp"];

/// Window functions whose value depends on the order of rows.
const ORDER_DEPENDENT_WINDOWS: &[&str] = &[
    "row_number", "rank", "dense_rank", "percent_rank", "cume_dist", "ntile", "lag", "lead", "first_value",
    "last_value", "nth_value", "first", "last", "any_value", "string_agg", "list", "array_agg", "group_concat",
];

/// Aggregates whose value depends on the order of rows unless ordered.
const ORDER_DEPENDENT_AGGREGATES: &[&str] =
    &["first", "last", "any_value", "string_agg", "list", "array_agg", "group_concat", "arbitrary", "listagg"];

/// Aggregates that keep every value (or every distinct value) in memory.
const HOLISTIC: &[&str] = &[
    "mode", "median", "quantile", "quantile_cont", "quantile_disc", "list", "array_agg", "string_agg", "group_concat",
    "approx_quantile", "histogram", "listagg",
];

fn parse_error(conn: &Connection, sql: &str) -> Option<String> {
    let json: String = match conn.query_row("SELECT json_serialize_sql(?)", [sql], |r| r.get(0)) {
        Ok(j) => j,
        Err(e) => return Some(e.to_string()),
    };
    let tree: Value = serde_json::from_str(&json).ok()?;
    if tree["error"].as_bool() == Some(true) {
        let msg = tree["error_message"].as_str().unwrap_or("invalid SQL");
        return Some(if msg.contains("Only SELECT") { "only a SELECT is allowed".into() } else { msg.to_string() });
    }
    None
}

fn serialize(conn: &Connection, sql: &str) -> crate::Result<Value> {
    let json: String = conn.query_row("SELECT json_serialize_sql(?)", [sql], |r| r.get(0))?;
    serde_json::from_str(&json).map_err(|e| crate::CoreError::invalid(e.to_string()))
}

fn deserialize(conn: &Connection, tree: &Value) -> crate::Result<String> {
    let text = serde_json::to_string(tree).map_err(|e| crate::CoreError::invalid(e.to_string()))?;
    Ok(conn.query_row("SELECT json_deserialize_sql(?::JSON)", [text], |r| r.get(0))?)
}

/// Problems with the body text itself (check 3, before parsing).
pub fn check_text(source: &str) -> Option<Issue> {
    let s = source.trim();
    if s.is_empty() {
        return Some(Issue::new("custom.check.body_empty", &[]));
    }
    if s.len() > MAX_BODY {
        return Some(Issue::new("custom.check.body_long", &[("max", &MAX_BODY.to_string())]));
    }
    if s.to_lowercase().contains("__pv_") {
        return Some(Issue::new("custom.check.body_reserved", &[]));
    }
    if s.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) {
        return Some(Issue::new("custom.check.body_control", &[]));
    }
    None
}

/// The SQL the body is parsed as: a function's expression is selected from
/// the parameters' table.
fn source_sql(def: &Definition) -> String {
    let body = def.body.source.trim().trim_end_matches(';').trim();
    match def.kind {
        DefKind::Function => format!("SELECT {body}\nFROM {ARGS_TABLE}"),
        DefKind::Operation => body.to_string(),
    }
}

/// Compile `def`'s body into a template. Errors are the failing check.
pub fn compile(conn: &Connection, def: &Definition, stability: &Stability) -> Result<CompileOutput, Issue> {
    if let Some(issue) = check_text(&def.body.source) {
        return Err(issue);
    }
    let src = source_sql(def);
    if let Some(msg) = parse_error(conn, &src) {
        return Err(Issue::new("custom.check.parse", &[("error", &msg)]));
    }
    // The read-only guard: one SELECT, no files, no other tables.
    let table_names: Vec<String> = def.tables().map(|t| t.name.to_lowercase()).collect();
    let resolve = |name: &str| -> Option<String> {
        let lower = name.to_lowercase();
        match def.kind {
            DefKind::Function => (lower == ARGS_TABLE).then(|| ARGS_TABLE.to_string()),
            DefKind::Operation => table_names.iter().position(|t| *t == lower).map(table_slot),
        }
    };
    let guarded = guard_select(conn, &src, &resolve).map_err(|e| Issue::new("custom.check.safety", &[("error", &e.to_string())]))?;
    let mut tree = serialize(conn, &guarded).map_err(|e| Issue::new("custom.check.parse", &[("error", &e.to_string())]))?;
    let statements = tree["statements"].as_array().map_or(0, Vec::len);
    if statements != 1 {
        return Err(Issue::new("custom.check.one_statement", &[]));
    }

    let mut walk = Walk {
        params: def
            .params
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind != ParamKind::Table)
            .map(|(i, p)| (p.name.to_lowercase(), i))
            .collect(),
        tables: table_names.clone(),
        stability,
        used: BTreeSet::new(),
        out: CompileOutput::default(),
        failure: None,
        function: def.kind == DefKind::Function,
    };
    let node = &mut tree["statements"][0]["node"];
    if def.kind == DefKind::Function {
        check_function_root(node)?;
    }
    walk.visit(node);
    if let Some(f) = walk.failure {
        return Err(f);
    }
    if walk.out.uses_current_time && !def.flags.uses_current_time {
        return Err(Issue::new("custom.check.current_time", &[]));
    }
    for (i, p) in def.params.iter().enumerate() {
        if p.kind != ParamKind::Table && !walk.used.contains(&i) {
            walk.out.warnings.push(Issue::new("custom.check.unused_param", &[("param", &p.name)]));
        }
    }
    if def.kind == DefKind::Function {
        // Unwrap the expression: no alias, no FROM.
        node["select_list"][0]["alias"] = Value::String(String::new());
        node["from_table"] = serde_json::json!({
            "alias": "", "query_location": u64::MAX, "query_location_length": 0, "sample": null, "type": "EMPTY"
        });
    }
    let text = deserialize(conn, &tree).map_err(|e| Issue::new("custom.check.parse", &[("error", &e.to_string())]))?;
    walk.out.template = match def.kind {
        DefKind::Function => text
            .strip_prefix("SELECT ")
            .map(|s| s.trim().to_string())
            .ok_or_else(|| Issue::new("custom.check.parse", &[("error", &text)]))?,
        DefKind::Operation => text.trim().to_string(),
    };
    Ok(walk.out)
}

/// A function body is one expression: no clauses besides it.
fn check_function_root(node: &Value) -> Result<(), Issue> {
    let one = node["select_list"].as_array().map_or(0, Vec::len) == 1;
    let plain = node["type"].as_str() == Some("SELECT_NODE")
        && node["where_clause"].is_null()
        && node["having"].is_null()
        && node["qualify"].is_null()
        && node["group_expressions"].as_array().is_none_or(Vec::is_empty)
        && node["modifiers"].as_array().is_none_or(Vec::is_empty)
        && node["cte_map"]["map"].as_array().is_none_or(Vec::is_empty)
        && node["from_table"]["type"].as_str() == Some("BASE_TABLE");
    if !one || !plain {
        return Err(Issue::new("custom.check.one_expression", &[]));
    }
    if node["select_list"][0]["class"].as_str() == Some("STAR") {
        return Err(Issue::new("custom.check.one_expression", &[]));
    }
    Ok(())
}

struct Walk<'a> {
    /// lower-cased parameter name → its index in the signature.
    params: HashMap<String, usize>,
    tables: Vec<String>,
    stability: &'a Stability,
    used: BTreeSet<usize>,
    out: CompileOutput,
    failure: Option<Issue>,
    function: bool,
}

impl Walk<'_> {
    fn warn(&mut self, key: &str, args: &[(&str, &str)]) {
        let issue = Issue::new(key, args);
        if !self.out.warnings.contains(&issue) {
            self.out.warnings.push(issue);
        }
    }

    fn fail(&mut self, key: &str, args: &[(&str, &str)]) {
        if self.failure.is_none() {
            self.failure = Some(Issue::new(key, args));
        }
    }

    fn check_function_name(&mut self, name: &str) {
        let lower = name.to_lowercase();
        match self.stability.get(&lower) {
            Some(2) => self.fail("custom.check.volatile", &[("function", name)]),
            Some(1) => self.out.uses_current_time = true,
            _ => {}
        }
    }

    fn visit(&mut self, v: &mut Value) {
        match v {
            Value::Object(map) => {
                let class = map.get("class").and_then(Value::as_str).unwrap_or("").to_string();
                let ty = map.get("type").and_then(Value::as_str).unwrap_or("").to_string();
                let other_table = ty == "BASE_TABLE"
                    && map.get("table_name").and_then(Value::as_str).is_none_or(|t| t != ARGS_TABLE);
                if self.function && (class == "SUBQUERY" || ty == "SUBQUERY" || ty == "TABLE_FUNCTION" || other_table) {
                    self.fail("custom.check.function_subquery", &[]);
                }
                match class.as_str() {
                    "COLUMN_REF" => self.column_ref(map),
                    "FUNCTION" => {
                        let name = map.get("function_name").and_then(Value::as_str).unwrap_or("").to_string();
                        self.check_function_name(&name);
                        let lower = name.to_lowercase();
                        let unordered = map
                            .get("order_bys")
                            .and_then(|o| o["orders"].as_array())
                            .is_none_or(Vec::is_empty);
                        if ORDER_DEPENDENT_AGGREGATES.contains(&lower.as_str()) && unordered {
                            self.warn("custom.check.row_order", &[("function", &name)]);
                        }
                    }
                    "WINDOW" => {
                        let name = map.get("function_name").and_then(Value::as_str).unwrap_or("").to_string();
                        self.check_function_name(&name);
                        self.out.reads_other_rows = true;
                        let unordered = map.get("orders").and_then(Value::as_array).is_none_or(Vec::is_empty);
                        if ORDER_DEPENDENT_WINDOWS.contains(&name.to_lowercase().as_str()) && unordered {
                            self.warn("custom.check.row_order", &[("function", &name)]);
                        }
                    }
                    _ => {}
                }
                if ty == "SELECT_NODE" {
                    self.select_node(map);
                }
                for child in map.values_mut() {
                    self.visit(child);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|c| self.visit(c)),
            _ => {}
        }
    }

    fn select_node(&mut self, map: &serde_json::Map<String, Value>) {
        let modifiers = map.get("modifiers").and_then(Value::as_array).cloned().unwrap_or_default();
        let has = |t: &str| modifiers.iter().any(|m| m["type"].as_str() == Some(t));
        if has("LIMIT_MODIFIER") && !has("ORDER_MODIFIER") {
            self.warn("custom.check.limit_order", &[]);
        }
        let grouped = map.get("group_expressions").and_then(Value::as_array).is_some_and(|g| !g.is_empty());
        if !grouped && !self.function {
            let mut heavy = None;
            if let Some(list) = map.get("select_list") {
                find_holistic(list, &mut heavy);
            }
            if let Some(f) = heavy {
                self.warn("custom.check.memory", &[("function", &f)]);
            }
        }
    }

    fn column_ref(&mut self, map: &mut serde_json::Map<String, Value>) {
        let Some(names) = map.get_mut("column_names").and_then(Value::as_array_mut) else { return };
        let parts: Vec<String> = names.iter().map(|n| n.as_str().unwrap_or("").to_lowercase()).collect();
        // `p` or `p.field`: the parameter itself (or a field of a struct
        // argument); `t.p`: the parameter qualified by a table parameter.
        let at = match parts.as_slice() {
            [first, ..] if self.params.contains_key(first) => Some(0),
            [table, second, ..] if self.tables.contains(table) && self.params.contains_key(second) => Some(1),
            _ => None,
        };
        match at {
            Some(i) => {
                let idx = self.params[&parts[i]];
                self.used.insert(idx);
                names[i] = Value::String(arg_slot(idx));
            }
            None if parts.len() == 1 && CLOCK_WORDS.contains(&parts[0].as_str()) => self.out.uses_current_time = true,
            None => {}
        }
    }
}

/// The first holistic aggregate or `count(DISTINCT …)` in `v`, not looking
/// into nested queries.
fn find_holistic(v: &Value, out: &mut Option<String>) {
    if out.is_some() {
        return;
    }
    match v {
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("SELECT_NODE") {
                return;
            }
            if map.get("class").and_then(Value::as_str) == Some("FUNCTION") {
                let name = map.get("function_name").and_then(Value::as_str).unwrap_or("").to_lowercase();
                let distinct = map.get("distinct").and_then(Value::as_bool).unwrap_or(false);
                if HOLISTIC.contains(&name.as_str()) || (distinct && name == "count") {
                    *out = Some(name);
                    return;
                }
            }
            map.values().for_each(|c| find_holistic(c, out));
        }
        Value::Array(items) => items.iter().for_each(|c| find_holistic(c, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::{Body, Param, ResultKind, ResultSpec};

    fn conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    fn param(name: &str, kind: ParamKind) -> Param {
        Param {
            name: name.into(),
            kind,
            dtype: (kind == ParamKind::Value).then(|| "BIGINT".to_string()),
            ..Default::default()
        }
    }

    fn function(body: &str, params: &[&str]) -> Definition {
        Definition {
            name: "f".into(),
            params: params.iter().map(|p| param(p, ParamKind::Column)).collect(),
            result: ResultSpec::default(),
            body: Body { source: body.into(), ..Default::default() },
            ..Default::default()
        }
    }

    fn operation(body: &str, params: Vec<Param>) -> Definition {
        Definition {
            kind: DefKind::Operation,
            name: "op".into(),
            params,
            result: ResultSpec { kind: ResultKind::Table, ..Default::default() },
            body: Body { source: body.into(), ..Default::default() },
            ..Default::default()
        }
    }

    fn run(def: &Definition) -> Result<CompileOutput, Issue> {
        let c = conn();
        let st = function_stability(&c).unwrap();
        compile(&c, def, &st)
    }

    #[test]
    fn function_template_has_slots() {
        let out = run(&function("CASE WHEN length(phone) < 7 THEN NULL ELSE prefix || phone END", &["phone", "prefix"])).unwrap();
        assert!(out.template.contains("length(__pv_arg_0)"), "{}", out.template);
        assert!(out.template.contains("__pv_arg_1 || __pv_arg_0"), "{}", out.template);
        assert!(!out.template.contains("FROM"));
        assert!(!out.reads_other_rows);
        let w = run(&function("x / sum(x) OVER ()", &["x"])).unwrap();
        assert!(w.reads_other_rows);
        assert_eq!(w.template, "(__pv_arg_0 / sum(__pv_arg_0) OVER ())");
    }

    #[test]
    fn regex_braces_stay_text() {
        let out = run(&function("regexp_matches(s, '\\d{2}')", &["s"])).unwrap();
        assert!(out.template.contains("'\\d{2}'"), "{}", out.template);
    }

    #[test]
    fn refusals() {
        let key = |d: Definition| run(&d).unwrap_err().key;
        assert_eq!(key(function("", &["x"])), "custom.check.body_empty");
        assert_eq!(key(function("x + __pv_arg_0", &["x"])), "custom.check.body_reserved");
        assert_eq!(key(function("x +", &["x"])), "custom.check.parse");
        assert_eq!(key(function("x, x", &["x"])), "custom.check.one_expression");
        assert_eq!(key(function("(SELECT 1) + x", &["x"])), "custom.check.function_subquery");
        assert_eq!(key(function("x + random()", &["x"])), "custom.check.volatile");
        assert_eq!(key(function("x + length(getenv('HOME'))", &["x"])), "custom.check.safety");
        assert_eq!(key(function("x > current_date", &["x"])), "custom.check.current_time");
        assert_eq!(key(function("x > now()", &["x"])), "custom.check.current_time");
        let t = || param("t", ParamKind::Table);
        assert_eq!(key(operation("SELECT * FROM read_csv('/etc/passwd')", vec![t()])), "custom.check.safety");
        assert_eq!(key(operation("SELECT * FROM other", vec![t()])), "custom.check.safety");
        assert_eq!(key(operation("SELECT * FROM '/tmp/x.csv'", vec![t()])), "custom.check.safety");
        assert_eq!(key(operation("DELETE FROM t", vec![t()])), "custom.check.parse");
        assert_eq!(key(operation("SELECT 1 FROM t; SELECT 2 FROM t", vec![t()])), "custom.check.safety");
        assert_eq!(key(operation("CREATE TABLE x AS SELECT 1", vec![t()])), "custom.check.parse");
    }

    #[test]
    fn operation_template_and_warnings() {
        let mut ps = vec![param("t", ParamKind::Table), param("grp", ParamKind::Column), param("score", ParamKind::Column)];
        ps.push(param("n", ParamKind::Value));
        let def = operation("SELECT * FROM t QUALIFY row_number() OVER (PARTITION BY grp ORDER BY t.score DESC) <= n", ps);
        let out = run(&def).unwrap();
        assert!(out.template.contains("FROM __pv_tab_0 AS t"), "{}", out.template);
        assert!(out.template.contains("PARTITION BY __pv_arg_1 ORDER BY t.__pv_arg_2 DESC"), "{}", out.template);
        assert!(out.template.contains("<= __pv_arg_3"), "{}", out.template);
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);

        let warns = |body: &str| run(&operation(body, vec![param("t", ParamKind::Table)])).unwrap().warnings;
        assert_eq!(warns("SELECT * FROM t LIMIT 3")[0].key, "custom.check.limit_order");
        assert_eq!(warns("SELECT *, row_number() OVER () AS n FROM t")[0].key, "custom.check.row_order");
        assert_eq!(warns("SELECT median(1) AS m FROM t")[0].key, "custom.check.memory");
        assert!(warns("SELECT * FROM t ORDER BY 1 LIMIT 3").is_empty());
        let unused = run(&operation("SELECT * FROM t", vec![param("t", ParamKind::Table), param("n", ParamKind::Value)])).unwrap();
        assert_eq!(unused.warnings[0].key, "custom.check.unused_param");
    }

    #[test]
    fn compiling_twice_gives_the_same_template() {
        let def = function("coalesce(x, 0) * 2", &["x"]);
        assert_eq!(run(&def).unwrap().template, run(&def).unwrap().template);
    }
}
