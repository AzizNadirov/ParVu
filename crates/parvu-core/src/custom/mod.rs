//! Custom functions and custom operations (plans/CUSTOM_TOOLS_PLAN.md).
//!
//! A [`Definition`] has a typed signature, a body and examples. Saving it
//! runs [`validate::validate`] on a small in-memory DuckDB of its own: the
//! body is parsed with DuckDB's parser, checked by the read-only guard
//! ([`crate::readonly::guard_select`]) and compiled once into a SQL template
//! owned by ParVu, in which `__pv_arg_N` stands for parameter `N` and
//! `__pv_tab_N` for table parameter `N`. Applying a definition fills the
//! slots with quoted identifiers and typed literals ([`fill`]); no user text
//! reaches SQL unquoted.
//!
//! Function and operation bodies are SQL in this version: a SQL expression
//! for a function, one SELECT for an operation. [`BodyLanguage`] is the seam
//! for expression-language bodies later.

pub mod compile;
pub mod examples;
pub mod fill;
pub mod record;
pub mod signature;
pub mod sqlmode;
pub mod validate;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::page::Cell;

pub use fill::{CustomArg, call_label};
pub use record::{StepNote, StepsCapture, StepsDraft};
pub use signature::{TypeSet, families_of};
pub use validate::{Check, CheckStatus, Report, ValidateEnv, validate};

/// The file format version this build writes and reads.
pub const SCHEMA_VERSION: u32 = 1;

/// Longest body accepted.
pub const MAX_BODY: usize = 20_000;
/// Most parameters a definition may have.
pub const MAX_PARAMS: usize = 12;
/// Most examples, rows per example table and columns per example table.
pub const MAX_EXAMPLES: usize = 20;
pub const MAX_EXAMPLE_ROWS: usize = 200;
pub const MAX_EXAMPLE_COLUMNS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefKind {
    /// Columns and values in, a column or one value out.
    #[default]
    Function,
    /// A table in, a table out.
    Operation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    /// One column whose type is in the parameter's type set.
    #[default]
    Column,
    /// Several columns, each of a type in the parameter's type set, between
    /// `min` and `max` of them. In a body it stands for a comma-separated
    /// list, so it may appear only where SQL takes one (see
    /// [`compile`]): the select list, `GROUP BY`, `PARTITION BY`, `ORDER BY`
    /// without a direction, `DISTINCT ON`, and function arguments.
    Columns,
    /// One constant of a fixed DuckDB type.
    Value,
    /// A table (operations only); the first is the table the operation runs on.
    Table,
}

/// A column a table parameter (or an operation's result) must have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequiredColumn {
    pub name: String,
    /// Type set names (`number`, `text`…); empty means any.
    #[serde(default, rename = "type", skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Param {
    pub name: String,
    pub kind: ParamKind,
    /// Column and columns parameters: type set names (`number`, `text`, `any`…).
    #[serde(default, rename = "type", skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    /// Value parameters: the DuckDB type of the value (`BIGINT`, `DOUBLE`,
    /// `VARCHAR`, `DATE`, `TIMESTAMP`, `BOOLEAN`…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dtype: Option<String>,
    /// Value parameters may be left out; they then take `default` (or are empty).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Value parameters: the only values allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    /// Number value parameters: bounds. Columns parameters: how many
    /// columns (`min` defaults to 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Column and columns parameters of an operation: the table parameter
    /// the column belongs to (default: the first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<String>,
    /// Table parameters: columns the table must have, by exact name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<RequiredColumn>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResultKind {
    /// One value per row (a row function).
    #[default]
    Column,
    /// One value for the whole column (a summary function, an aggregate).
    Value,
    /// A table (operations).
    Table,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResultSpec {
    pub kind: ResultKind,
    /// Type set names of a column or value result; empty means any.
    #[serde(default, rename = "type", skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    /// The result has the type family of this column parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_as: Option<String>,
    /// Table results: columns the result must have.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<RequiredColumn>,
}

/// Where an operation's result goes. The definition decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// Changes this table: one step, undoable.
    #[default]
    Step,
    /// Opens a new table (a result tab), like Group By.
    NewTable,
}

/// The language a body is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BodyLanguage {
    /// A SQL expression (functions) or one SELECT (operations).
    #[default]
    Sql,
    /// ParVu's expression language, `[x]` meaning parameter `x` (functions
    /// only). The app translates it to SQL (`parvu-dsl`), which then goes
    /// through the same checks ([`ValidateEnv::expression_sql`]).
    Expression,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Body {
    #[serde(default)]
    pub language: BodyLanguage,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Flags {
    /// The body may use today's date or the current time (`now()`,
    /// `current_date`), so its result changes from day to day.
    #[serde(default)]
    pub uses_current_time: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Validated: usable from menus.
    #[default]
    Ok,
    /// Kept while incomplete or failing; not usable.
    Draft,
    /// Was valid but failed again after an update; not usable.
    Broken,
}

/// A column of an example table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExampleColumn {
    pub name: String,
    /// DuckDB type of the column (inputs). Expected outputs may leave it empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dtype: String,
}

/// A small table: cells as text, `null` for empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ExampleTable {
    pub columns: Vec<ExampleColumn>,
    #[serde(default)]
    pub rows: Vec<Vec<Cell>>,
}

/// An argument of an example: a value (or a column name, for an
/// operation's column parameter), or a list of column names for a columns
/// parameter. In the file: `"3"`, `null` or `["region", "city"]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExampleArg {
    Columns(Vec<String>),
    Cell(Cell),
}

impl From<Cell> for ExampleArg {
    fn from(c: Cell) -> Self {
        ExampleArg::Cell(c)
    }
}

impl ExampleArg {
    /// The value or column name; `None` for an empty value or a list.
    pub fn cell(&self) -> Option<&str> {
        match self {
            ExampleArg::Cell(c) => c.as_deref(),
            ExampleArg::Columns(_) => None,
        }
    }
}

/// A test case: inputs and the expected output.
///
/// Functions: `input` has one column per column parameter, named after it;
/// `expected` has one column (a row function: one value per input row; a
/// summary function: one row). Operations: `input` is the first table
/// parameter, `tables` the others by parameter name, `args` names the
/// column argument of each column parameter, and `expected` is the result
/// table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Example {
    #[serde(default)]
    pub name: String,
    pub input: ExampleTable,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tables: BTreeMap<String, ExampleTable>,
    /// Value arguments (text, `null` for empty) and, for operations, the
    /// column chosen for each column parameter; the columns of a columns
    /// parameter as a list.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, ExampleArg>,
    pub expected: ExampleTable,
    /// Compare table rows in order (otherwise as a multiset).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ordered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Row,
    Summary,
    Table,
}

/// The compiled form of a definition, a cache trusted only while `hash`,
/// `app` and `duckdb` match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Compiled {
    /// SQL with `__pv_arg_N` / `__pv_tab_N` slots.
    pub template: String,
    pub hash: String,
    pub shape: Shape,
    /// A row function that reads other rows (a window).
    #[serde(default)]
    pub reads_other_rows: bool,
    /// Probe description (`number=BIGINT`) → the result type DuckDB inferred.
    #[serde(default)]
    pub result_types: BTreeMap<String, String>,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub duckdb: String,
}

/// A custom function or custom operation, as stored in
/// `~/.ParVu/custom/<name>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Definition {
    #[serde(default = "schema_version")]
    pub schema: u32,
    pub kind: DefKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// Shipped with ParVu (the user may delete it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sample: bool,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub params: Vec<Param>,
    pub result: ResultSpec,
    /// Operations: where the result goes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    pub body: Body,
    #[serde(default)]
    pub flags: Flags,
    #[serde(default)]
    pub examples: Vec<Example>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compiled: Option<Compiled>,
    /// The last validation report of a draft or broken definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<Report>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub created: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub modified: String,
}

/// ` × 1…3`, ` × 2+`: a columns parameter's count, when it isn't "one or more".
fn count_text(p: &Param) -> String {
    let (min, max) = p.column_count();
    match max {
        Some(max) if max == min => format!(" × {min}"),
        Some(max) => format!(" × {min}…{max}"),
        None if min > 1 => format!(" × {min}+"),
        None => String::new(),
    }
}

impl Param {
    /// A columns parameter's bounds: at least `min` (1 by default), at most `max`.
    pub fn column_count(&self) -> (usize, Option<usize>) {
        let min = self.min.map_or(1, |m| m.max(0.0) as usize);
        (min, self.max.map(|m| m.max(0.0) as usize))
    }

    /// A column or columns parameter.
    pub fn takes_columns(&self) -> bool {
        matches!(self.kind, ParamKind::Column | ParamKind::Columns)
    }
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Definition {
    /// Where an operation's result goes (a step unless it says otherwise;
    /// several table parameters always make a new table).
    pub fn placement(&self) -> Placement {
        if self.params.iter().filter(|p| p.kind == ParamKind::Table).count() > 1 {
            return Placement::NewTable;
        }
        self.placement.unwrap_or_default()
    }

    pub fn param(&self, name: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Table parameters, in order.
    pub fn tables(&self) -> impl Iterator<Item = &Param> {
        self.params.iter().filter(|p| p.kind == ParamKind::Table)
    }

    /// The table parameter a column parameter belongs to.
    pub fn table_of<'a>(&'a self, p: &Param) -> Option<&'a Param> {
        match &p.of {
            Some(t) => self.tables().find(|x| x.name.eq_ignore_ascii_case(t)),
            None => self.tables().next(),
        }
    }

    /// `clean_phone(phone: column<text>, prefix: value<VARCHAR> = "+") → column<text>`,
    /// as the editor shows it.
    pub fn signature(&self) -> String {
        let types = |t: &[String]| if t.is_empty() { "any".to_string() } else { t.join(" | ") };
        let params: Vec<String> = self
            .params
            .iter()
            .map(|p| match p.kind {
                ParamKind::Table => {
                    let cols: Vec<String> = p.columns.iter().map(|c| format!("{}: {}", c.name, types(&c.types))).collect();
                    if cols.is_empty() { format!("{}: table", p.name) } else { format!("{}: table{{{}}}", p.name, cols.join(", ")) }
                }
                ParamKind::Column => format!("{}: column<{}>", p.name, types(&p.types)),
                ParamKind::Columns => format!("{}: columns<{}>{}", p.name, types(&p.types), count_text(p)),
                ParamKind::Value => {
                    let default = match (&p.default, p.optional) {
                        (Some(d), true) => format!(" = {}", serde_json::Value::String(d.clone())),
                        (None, true) => " = empty".into(),
                        _ => String::new(),
                    };
                    format!("{}: value<{}>{default}", p.name, p.dtype.as_deref().unwrap_or("VARCHAR"))
                }
            })
            .collect();
        let result = match self.result.kind {
            ResultKind::Table => "table".to_string(),
            k => {
                let kind = if k == ResultKind::Value { "value" } else { "column" };
                match &self.result.same_as {
                    Some(s) => format!("{kind}<same as {s}>"),
                    None => format!("{kind}<{}>", types(&self.result.types)),
                }
            }
        };
        format!("{}({}) → {result}", self.name, params.join(", "))
    }

    /// Ready to use: validated and compiled.
    pub fn is_usable(&self) -> bool {
        self.status == Status::Ok && self.compiled.is_some()
    }

    /// The source parts that decide the compiled template, hashed: name,
    /// parameters, result, placement, body and flags.
    pub fn source_hash(&self) -> String {
        let parts = serde_json::json!({
            "kind": self.kind,
            "name": self.name.to_lowercase(),
            "params": self.params,
            "result": self.result,
            "placement": self.placement(),
            "body": self.body,
            "flags": self.flags,
        });
        format!("{:016x}", fnv1a(parts.to_string().as_bytes()))
    }

    /// The same definition without what a receiver must compute itself
    /// (export, import).
    pub fn for_export(&self) -> Definition {
        let mut d = self.clone();
        d.compiled = None;
        d.report = None;
        d.status = Status::Ok;
        d
    }
}

/// The sample definitions shipped with ParVu (seeded into the user's
/// folder once; the user may delete them).
pub fn samples() -> Vec<Definition> {
    [
        include_str!("../../samples/clean_phone.json"),
        include_str!("../../samples/title_case.json"),
        include_str!("../../samples/percent_of_total.json"),
        include_str!("../../samples/weighted_average.json"),
        include_str!("../../samples/top_n_per_group.json"),
        include_str!("../../samples/outliers_iqr.json"),
        include_str!("../../samples/running_total.json"),
        include_str!("../../samples/dedupe_by.json"),
        include_str!("../../samples/percentile_value.json"),
    ]
    .iter()
    .filter_map(|text| serde_json::from_str(text).map_err(|e| tracing::error!("bad sample definition: {e}")).ok())
    .collect()
}

/// FNV-1a, 64 bits: stable across builds, enough for a cache key.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
}

/// The usable definitions, by lower-cased name. Workspaces hold a snapshot.
#[derive(Debug, Clone, Default)]
pub struct Library {
    defs: HashMap<String, Arc<Definition>>,
}

impl Library {
    pub fn new(defs: impl IntoIterator<Item = Definition>) -> Self {
        Self { defs: defs.into_iter().map(|d| (d.name.to_lowercase(), Arc::new(d))).collect() }
    }

    /// A definition by name, usable or not.
    pub fn get(&self, name: &str) -> Option<Arc<Definition>> {
        self.defs.get(&name.to_lowercase()).cloned()
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// Every definition, in name order.
    pub fn definitions(&self) -> Vec<Arc<Definition>> {
        let mut v: Vec<Arc<Definition>> = self.defs.values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

/// A file holding one definition or several (a bundle), for import and
/// export: `{"schema": 1, "definitions": [ … ]}`. A file with a single
/// definition object is read too.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub schema: u32,
    pub definitions: Vec<Definition>,
}

impl Bundle {
    pub fn new(definitions: Vec<Definition>) -> Self {
        Self { schema: SCHEMA_VERSION, definitions: definitions.iter().map(Definition::for_export).collect() }
    }

    /// Read a bundle, or a single definition, from JSON text.
    pub fn parse(text: &str) -> crate::Result<Bundle> {
        let v: serde_json::Value =
            serde_json::from_str(text).map_err(|e| crate::CoreError::invalid(format!("Not a JSON file: {e}")))?;
        let schema = v.get("schema").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
        if schema > SCHEMA_VERSION {
            return Err(crate::CoreError::invalid(format!(
                "This file was made by a newer ParVu (format {schema}); this version reads format {SCHEMA_VERSION}."
            )));
        }
        let read = |v: serde_json::Value| {
            serde_json::from_value::<Definition>(v).map_err(|e| crate::CoreError::invalid(format!("Not a custom tool: {e}")))
        };
        let definitions = match v.get("definitions") {
            Some(serde_json::Value::Array(items)) => items.iter().cloned().map(read).collect::<crate::Result<Vec<_>>>()?,
            Some(_) => return Err(crate::CoreError::invalid("\"definitions\" must be a list")),
            None => vec![read(v)?],
        };
        Ok(Bundle { schema: SCHEMA_VERSION, definitions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_ignores_examples() {
        let mut d = Definition { name: "f".into(), body: Body { source: "x + 1".into(), ..Default::default() }, ..Default::default() };
        let h = d.source_hash();
        assert_eq!(h, d.source_hash());
        d.examples.push(Example::default());
        d.title = "t".into();
        assert_eq!(h, d.source_hash(), "examples and texts are not compiled");
        d.body.source = "x + 2".into();
        assert_ne!(h, d.source_hash());
    }

    #[test]
    fn bundles_read_one_or_many() {
        let one = r#"{"schema":1,"kind":"function","name":"f","result":{"kind":"column"},"body":{"source":"1"}}"#;
        assert_eq!(Bundle::parse(one).unwrap().definitions.len(), 1);
        let many = format!(r#"{{"schema":1,"definitions":[{one},{one}]}}"#);
        assert_eq!(Bundle::parse(&many).unwrap().definitions.len(), 2);
        assert!(Bundle::parse(r#"{"schema":99,"definitions":[]}"#).unwrap_err().to_string().contains("newer"));
        assert!(Bundle::parse("nope").is_err());
    }
}
