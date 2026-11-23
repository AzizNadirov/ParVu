//! Export of a query result to a file (port of `core/exporter.py` from the
//! export-dialog branch).
//!
//! Column selection, casts and date formatting are pushed into SQL. CSV,
//! Parquet and JSON are written by DuckDB's `COPY`, which streams and is
//! fast; the caller can watch progress and cancel through the connection's
//! progress and interrupt handles. Excel is written with `rust_xlsxwriter`
//! from Arrow batches in constant memory, and Markdown tables from streamed
//! Arrow batches, a line at a time.
//!
//! Output always goes to a temporary file next to the target and is renamed
//! into place at the end, so a failed or cancelled export leaves nothing
//! behind and the source file can safely be the target.

use std::path::{Path, PathBuf};

use duckdb::Connection;
use duckdb::arrow::array::{Array, ArrayRef, AsArray};
use duckdb::arrow::datatypes::{DataType, Float64Type};
use duckdb::arrow::util::display::ArrayFormatter;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::page::{Cell, format_batches, format_options};
use crate::sql::{is_safe_type_name, quote_ident, quote_literal};

/// Excel's hard row limit, including the header row.
pub const EXCEL_MAX_ROWS: u64 = 1_048_576;

/// Cast targets offered per column (`""` keeps the source type).
pub const CAST_TYPES: [&str; 9] =
    ["", "VARCHAR", "BIGINT", "INTEGER", "DOUBLE", "DECIMAL(18,4)", "BOOLEAN", "DATE", "TIMESTAMP"];

pub const PARQUET_COMPRESSIONS: [&str; 6] = ["snappy", "zstd", "gzip", "lz4", "brotli", "uncompressed"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Parquet,
    Json,
    Excel,
    Markdown,
}

impl ExportFormat {
    /// Format for a path's extension; CSV when unknown (as in Python).
    pub fn for_path(path: &Path) -> Self {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        match ext.as_str() {
            "parquet" | "pq" => ExportFormat::Parquet,
            "json" | "jsonl" | "ndjson" => ExportFormat::Json,
            "xlsx" => ExportFormat::Excel,
            "md" | "markdown" => ExportFormat::Markdown,
            _ => ExportFormat::Csv,
        }
    }

    pub fn default_extension(self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Parquet => "parquet",
            ExportFormat::Json => "json",
            ExportFormat::Excel => "xlsx",
            ExportFormat::Markdown => "md",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Quoting {
    /// Quote only when needed.
    #[default]
    Needed,
    /// Quote every value.
    AllValid,
    /// Never quote.
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnSpec {
    pub name: String,
    pub source_type: String,
    #[serde(default = "yes")]
    pub include: bool,
    /// DuckDB type to cast to; empty keeps the source type.
    #[serde(default)]
    pub cast_to: String,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    pub path: PathBuf,
    pub format: ExportFormat,
    /// Empty means "all columns, unchanged".
    pub columns: Vec<ColumnSpec>,
    /// strftime pattern for DATE columns.
    pub date_format: String,
    /// strftime pattern for TIMESTAMP and TIME columns.
    pub timestamp_format: String,
    pub delimiter: String,
    pub include_header: bool,
    pub quoting: Quoting,
    pub compression: String,
    /// JSON lines (one object per line) instead of a JSON array.
    pub json_lines: bool,
    /// Markdown: a first column `#` with the 1-based row number.
    pub row_numbers: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            format: ExportFormat::Csv,
            columns: Vec::new(),
            date_format: String::new(),
            timestamp_format: String::new(),
            delimiter: ",".into(),
            include_header: true,
            quoting: Quoting::Needed,
            compression: "snappy".into(),
            json_lines: false,
            row_numbers: false,
        }
    }
}

impl ExportOptions {
    /// Defaults for `path`, with the format taken from its extension.
    pub fn for_path(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let format = ExportFormat::for_path(&path);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        Self {
            delimiter: if ext == "tsv" { "\t".into() } else { ",".into() },
            json_lines: matches!(ext.as_str(), "jsonl" | "ndjson"),
            path,
            format,
            ..Self::default()
        }
    }
}

/// True for DuckDB DATE, TIME and TIMESTAMP types.
pub fn is_temporal(dtype: &str) -> bool {
    let t = dtype.to_uppercase();
    t.starts_with("DATE") || t.starts_with("TIMESTAMP") || t.starts_with("TIME")
}

/// Wrap `source` with column selection, casts and date formatting.
///
/// An explicit cast wins over a date format, except a cast to VARCHAR, which
/// is what formatting produces anyway.
pub fn build_export_query(source: &str, o: &ExportOptions) -> Result<String> {
    if o.columns.is_empty() {
        return Ok(format!("SELECT * FROM ({source})"));
    }
    let included: Vec<&ColumnSpec> = o.columns.iter().filter(|c| c.include).collect();
    if included.is_empty() {
        return Err(CoreError::invalid("Select at least one column to export"));
    }
    let mut parts = Vec::with_capacity(included.len());
    for spec in included {
        let ident = quote_ident(&spec.name);
        let fmt = if !is_temporal(&spec.source_type) {
            ""
        } else if spec.source_type.to_uppercase().starts_with("DATE") {
            o.date_format.as_str()
        } else {
            o.timestamp_format.as_str()
        };
        let expr = if !fmt.is_empty() && (spec.cast_to.is_empty() || spec.cast_to == "VARCHAR") {
            format!("strftime({ident}, {})", quote_literal(fmt))
        } else if !spec.cast_to.is_empty() {
            if !is_safe_type_name(&spec.cast_to) {
                return Err(CoreError::invalid(format!("Invalid type: {}", spec.cast_to)));
            }
            // TRY_CAST so one bad value can't stop the export halfway and
            // leave a partial file; it is written as empty instead.
            format!("TRY_CAST({ident} AS {})", spec.cast_to)
        } else {
            ident.clone()
        };
        parts.push(format!("{expr} AS {ident}"));
    }
    Ok(format!("SELECT {} FROM ({source})", parts.join(", ")))
}

/// The `COPY ... TO ... (options)` option list for `o`.
pub fn copy_options(o: &ExportOptions) -> Result<String> {
    Ok(match o.format {
        ExportFormat::Csv => {
            if o.delimiter.is_empty() {
                return Err(CoreError::invalid("Delimiter must not be empty"));
            }
            let mut opts = vec![
                "FORMAT CSV".to_string(),
                format!("HEADER {}", o.include_header),
                format!("DELIMITER {}", quote_literal(&o.delimiter)),
            ];
            match o.quoting {
                Quoting::Needed => {}
                Quoting::AllValid => opts.push("FORCE_QUOTE *".into()),
                Quoting::None => {
                    opts.push("QUOTE ''".into());
                    opts.push("ESCAPE ''".into());
                }
            }
            opts.join(", ")
        }
        ExportFormat::Parquet => {
            let c = o.compression.to_lowercase();
            if !PARQUET_COMPRESSIONS.contains(&c.as_str()) {
                return Err(CoreError::invalid(format!("Unknown compression: {}", o.compression)));
            }
            format!("FORMAT PARQUET, COMPRESSION {c}")
        }
        ExportFormat::Json => format!("FORMAT JSON, ARRAY {}", !o.json_lines),
        ExportFormat::Excel => return Err(CoreError::invalid("Excel is not written with COPY")),
        ExportFormat::Markdown => return Err(CoreError::invalid("Markdown is not written with COPY")),
    })
}

/// A temporary path in the same directory as `target`.
pub fn temp_path_for(target: &Path) -> PathBuf {
    let dir = target.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("export");
    let ext = target.extension().and_then(|e| e.to_str()).unwrap_or("tmp");
    dir.join(format!(".{name}.parvu-{}.{ext}", std::process::id()))
}

/// Export `source` (a query) to `o.path`. Returns the number of rows written.
pub fn export(conn: &Connection, source: &str, o: &ExportOptions) -> Result<u64> {
    if o.path.as_os_str().is_empty() {
        return Err(CoreError::invalid("No output path"));
    }
    let query = build_export_query(source, o)?;
    let tmp = temp_path_for(&o.path);
    let copy = |query: &str| -> Result<u64> {
        let sql = format!("COPY ({query}) TO {} ({})", quote_literal(&tmp.to_string_lossy()), copy_options(o)?);
        conn.execute(&sql, []).map(|n| n as u64).map_err(CoreError::from)
    };
    let result = match o.format {
        ExportFormat::Excel => write_excel(conn, &query, &tmp),
        ExportFormat::Markdown => write_markdown(conn, &query, &tmp, o),
        ExportFormat::Parquet => match hugeint_columns(conn, &query)? {
            huge if huge.is_empty() => copy(&query),
            // Parquet has no 128-bit integers: DuckDB would write them as
            // doubles (a SUM of integers, 6, would read back as 6.0). Whole
            // numbers stay whole: BIGINT when every value fits, otherwise
            // DECIMAL(38,0).
            huge => match copy(&cast_columns(&query, &huge, "BIGINT")) {
                Err(e) if e.to_string().contains("out of range") || e.to_string().contains("Conversion Error") => {
                    let _ = std::fs::remove_file(&tmp);
                    copy(&cast_columns(&query, &huge, "DECIMAL(38,0)"))
                }
                other => other,
            },
        },
        _ => copy(&query),
    };
    match result {
        Ok(rows) => {
            std::fs::rename(&tmp, &o.path)?;
            tracing::info!("exported {rows} rows to {}", o.path.display());
            Ok(rows)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Columns of `query` typed HUGEINT or UHUGEINT (128-bit integers).
fn hugeint_columns(conn: &Connection, query: &str) -> Result<Vec<String>> {
    Ok(crate::engine::describe(conn, query)?
        .into_iter()
        .filter(|c| matches!(c.dtype.to_uppercase().as_str(), "HUGEINT" | "UHUGEINT"))
        .map(|c| c.name)
        .collect())
}

/// `query` with `columns` cast to `ty`, in place.
fn cast_columns(query: &str, columns: &[String], ty: &str) -> String {
    let items: Vec<String> = columns.iter().map(|c| format!("CAST({0} AS {ty}) AS {0}", quote_ident(c))).collect();
    format!("SELECT * REPLACE ({}) FROM ({query})", items.join(", "))
}

fn is_number(dt: &DataType) -> bool {
    matches!(
        dt,
        DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float16
            | DataType::Float32
            | DataType::Float64
            | DataType::Decimal32(_, _)
            | DataType::Decimal64(_, _)
            | DataType::Decimal128(_, _)
            | DataType::Decimal256(_, _)
    )
}

// ---------------------------------------------------------------- Markdown
//
// A GitHub-flavoured pipe table. The UI's "Copy as → Markdown" builds the
// same text in `ui/src/lib/copy.ts`; both are tested against
// `tests/golden/markdown.json`, so keep them in step.

/// Header of the row-number column.
pub const MARKDOWN_ROW_NUMBER_LABEL: &str = "#";

/// A cell's text made safe inside a pipe table: `|` is escaped as `\|`
/// (backslashes right before it are doubled, so they stay visible and don't
/// escape the escape), and line breaks become `<br>`. Nothing else changes.
pub fn markdown_cell(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 4);
    let mut backslashes = 0usize;
    let mut chars = v.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                backslashes += 1;
                out.push('\\');
                continue;
            }
            '|' => {
                out.extend(std::iter::repeat_n('\\', backslashes));
                out.push_str("\\|");
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str("<br>");
            }
            '\n' => out.push_str("<br>"),
            _ => out.push(c),
        }
        backslashes = 0;
    }
    out
}

/// One table line from already escaped cells: `| a | b |`.
fn markdown_line<S: AsRef<str>>(cells: &[S]) -> String {
    let mut line = String::from("|");
    for c in cells {
        line.push(' ');
        line.push_str(c.as_ref());
        line.push_str(" |");
    }
    line
}

/// The header line and the delimiter line. Number columns (and the row
/// numbers) are right-aligned.
///
/// A pipe table cannot do without its header line, so with
/// `include_header` off the header line is there but blank (`|  |  |`).
pub fn markdown_head(names: &[String], numeric: &[bool], include_header: bool, row_numbers: bool) -> [String; 2] {
    let mut head: Vec<String> = Vec::with_capacity(names.len() + 1);
    let mut delim: Vec<&str> = Vec::with_capacity(names.len() + 1);
    if row_numbers {
        head.push(if include_header { MARKDOWN_ROW_NUMBER_LABEL.to_string() } else { String::new() });
        delim.push("---:");
    }
    for (i, n) in names.iter().enumerate() {
        head.push(if include_header { markdown_cell(n) } else { String::new() });
        delim.push(if numeric.get(i).copied().unwrap_or(false) { "---:" } else { "---" });
    }
    [markdown_line(&head), markdown_line(&delim)]
}

/// One data line; NULL is an empty cell. `row_number` is 1-based.
pub fn markdown_row(cells: &[Cell], row_number: Option<u64>) -> String {
    let mut out: Vec<String> = Vec::with_capacity(cells.len() + 1);
    if let Some(n) = row_number {
        out.push(n.to_string());
    }
    out.extend(cells.iter().map(|c| c.as_deref().map(markdown_cell).unwrap_or_default()));
    markdown_line(&out)
}

/// Stream `query` into a Markdown table at `path`, batch by batch, with the
/// cells formatted as the grid shows them.
fn write_markdown(conn: &Connection, query: &str, path: &Path, o: &ExportOptions) -> Result<u64> {
    use std::io::Write;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut stmt = conn.prepare(query)?;
    let stream = stmt.stream_arrow([])?;
    let schema = stream.get_schema();
    let names: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let numeric: Vec<bool> = schema.fields().iter().map(|f| is_number(f.data_type())).collect();
    for line in markdown_head(&names, &numeric, o.include_header, o.row_numbers) {
        writeln!(out, "{line}")?;
    }
    let mut rows: u64 = 0;
    for batch in stream {
        let batch = batch?;
        for cells in format_batches(std::slice::from_ref(&batch))? {
            rows += 1;
            writeln!(out, "{}", markdown_row(&cells, o.row_numbers.then_some(rows)))?;
        }
    }
    out.flush()?;
    Ok(rows)
}

pub(crate) fn xlsx_err(e: rust_xlsxwriter::XlsxError) -> CoreError {
    CoreError::Excel(e.to_string())
}

/// How one column is written to Excel.
enum XlsxKind {
    Number(ArrayRef),
    Bool,
    Date(ArrayRef),
    DateTime(ArrayRef),
    Text,
}

fn xlsx_kind(arr: &ArrayRef) -> XlsxKind {
    use duckdb::arrow::compute::cast;
    use duckdb::arrow::datatypes::TimeUnit;
    let dt = arr.data_type();
    if is_number(dt)
        && let Ok(a) = cast(arr, &DataType::Float64)
    {
        return XlsxKind::Number(a);
    }
    match dt {
        DataType::Boolean => XlsxKind::Bool,
        DataType::Date32 | DataType::Date64 => {
            cast(arr, &DataType::Date32).map(XlsxKind::Date).unwrap_or(XlsxKind::Text)
        }
        DataType::Timestamp(_, _) => cast(arr, &DataType::Timestamp(TimeUnit::Microsecond, None))
            .map(XlsxKind::DateTime)
            .unwrap_or(XlsxKind::Text),
        _ => XlsxKind::Text,
    }
}

fn write_excel(conn: &Connection, query: &str, path: &Path) -> Result<u64> {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet_with_constant_memory().map_err(xlsx_err)?;
    let rows = write_query_sheet(ws, conn, query)?;
    wb.save(path).map_err(xlsx_err)?;
    Ok(rows)
}

/// Write the result of `query` into `ws`, a header row and then the rows.
pub(crate) fn write_query_sheet(ws: &mut rust_xlsxwriter::Worksheet, conn: &Connection, query: &str) -> Result<u64> {
    use duckdb::arrow::datatypes::{Date32Type, TimestampMicrosecondType};
    use chrono::Datelike;
    use rust_xlsxwriter::Format;

    let total = crate::engine::count_rows(conn, query)?;
    if total + 1 > EXCEL_MAX_ROWS {
        return Err(CoreError::invalid(format!(
            "Excel files hold at most {} rows; this result has {total}",
            EXCEL_MAX_ROWS - 1
        )));
    }
    let header = Format::new().set_bold();
    let date_fmt = Format::new().set_num_format("yyyy-mm-dd");
    let datetime_fmt = Format::new().set_num_format("yyyy-mm-dd hh:mm:ss");
    let opts = format_options();
    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid date");

    let mut stmt = conn.prepare(query)?;
    let arrow = stmt.query_arrow([])?;
    let schema = arrow.get_schema();
    for (c, field) in schema.fields().iter().enumerate() {
        ws.write_string_with_format(0, c as u16, field.name(), &header).map_err(xlsx_err)?;
    }
    let mut row: u32 = 1;
    for batch in arrow {
        let cols: Vec<&ArrayRef> = batch.columns().iter().collect();
        let kinds: Vec<XlsxKind> = cols.iter().map(|a| xlsx_kind(a)).collect();
        let text = cols
            .iter()
            .map(|a| ArrayFormatter::try_new(a.as_ref(), &opts))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| CoreError::invalid(e.to_string()))?;
        for r in 0..batch.num_rows() {
            for (c, arr) in cols.iter().enumerate() {
                if arr.is_null(r) {
                    continue;
                }
                let col = c as u16;
                // Dates Excel cannot represent (before 1900) fall back to text.
                let written = match &kinds[c] {
                    XlsxKind::Number(n) => {
                        ws.write_number(row, col, n.as_primitive::<Float64Type>().value(r)).map_err(xlsx_err)?;
                        true
                    }
                    XlsxKind::Bool => {
                        ws.write_boolean(row, col, arr.as_boolean().value(r)).map_err(xlsx_err)?;
                        true
                    }
                    XlsxKind::Date(d) => {
                        let days = d.as_primitive::<Date32Type>().value(r);
                        epoch
                            .checked_add_signed(chrono::Duration::days(i64::from(days)))
                            .filter(|date| date.year() >= 1900)
                            .is_some_and(|date| ws.write_date_with_format(row, col, date, &date_fmt).is_ok())
                    }
                    XlsxKind::DateTime(t) => {
                        let micros = t.as_primitive::<TimestampMicrosecondType>().value(r);
                        chrono::DateTime::from_timestamp_micros(micros)
                            .map(|v| v.naive_utc())
                            .filter(|dt| dt.year() >= 1900)
                            .is_some_and(|dt| ws.write_datetime_with_format(row, col, dt, &datetime_fmt).is_ok())
                    }
                    XlsxKind::Text => false,
                };
                if !written {
                    ws.write_string(row, col, text[c].value(r).to_string()).map_err(xlsx_err)?;
                }
            }
            row += 1;
        }
    }
    Ok(u64::from(row - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, t: &str, include: bool, cast: &str) -> ColumnSpec {
        ColumnSpec { name: name.into(), source_type: t.into(), include, cast_to: cast.into() }
    }

    #[test]
    fn formats_from_path() {
        assert_eq!(ExportFormat::for_path(Path::new("a.PQ")), ExportFormat::Parquet);
        assert_eq!(ExportFormat::for_path(Path::new("a.ndjson")), ExportFormat::Json);
        assert_eq!(ExportFormat::for_path(Path::new("a.xlsx")), ExportFormat::Excel);
        assert_eq!(ExportFormat::for_path(Path::new("a.weird")), ExportFormat::Csv);
        assert_eq!(ExportFormat::for_path(Path::new("a.md")), ExportFormat::Markdown);
        assert_eq!(ExportFormat::for_path(Path::new("a.Markdown")), ExportFormat::Markdown);
        assert_eq!(ExportFormat::Markdown.default_extension(), "md");
        let o = ExportOptions::for_path("x.tsv");
        assert_eq!(o.delimiter, "\t");
        assert!(ExportOptions::for_path("x.jsonl").json_lines);
    }

    #[test]
    fn query_building() {
        let o = ExportOptions {
            columns: vec![
                spec("a", "BIGINT", true, ""),
                spec("b", "VARCHAR", false, ""),
                spec("d", "DATE", true, ""),
                spec("t", "TIMESTAMP", true, "DATE"),
                spec("n", "INTEGER", true, "DOUBLE"),
            ],
            date_format: "%d/%m/%Y".into(),
            timestamp_format: "%H".into(),
            ..ExportOptions::default()
        };
        assert_eq!(
            build_export_query("Q", &o).unwrap(),
            "SELECT \"a\" AS \"a\", strftime(\"d\", '%d/%m/%Y') AS \"d\", TRY_CAST(\"t\" AS DATE) AS \"t\", \
             TRY_CAST(\"n\" AS DOUBLE) AS \"n\" FROM (Q)"
        );
    }

    #[test]
    fn nothing_selected_is_an_error() {
        let o = ExportOptions { columns: vec![spec("a", "BIGINT", false, "")], ..ExportOptions::default() };
        assert!(build_export_query("Q", &o).is_err());
        assert_eq!(build_export_query("Q", &ExportOptions::default()).unwrap(), "SELECT * FROM (Q)");
    }

    #[test]
    fn rejects_bad_cast_and_compression() {
        let o = ExportOptions { columns: vec![spec("a", "BIGINT", true, "INT); --")], ..ExportOptions::default() };
        assert!(build_export_query("Q", &o).is_err());
        let o = ExportOptions { format: ExportFormat::Parquet, compression: "nope".into(), ..ExportOptions::default() };
        assert!(copy_options(&o).is_err());
    }

    #[test]
    fn copy_option_text() {
        let o = ExportOptions { quoting: Quoting::AllValid, include_header: false, ..ExportOptions::default() };
        assert_eq!(copy_options(&o).unwrap(), "FORMAT CSV, HEADER false, DELIMITER ',', FORCE_QUOTE *");
        let o = ExportOptions { format: ExportFormat::Json, json_lines: true, ..ExportOptions::default() };
        assert_eq!(copy_options(&o).unwrap(), "FORMAT JSON, ARRAY false");
    }

    #[test]
    fn temporal() {
        assert!(is_temporal("DATE"));
        assert!(is_temporal("TIMESTAMP WITH TIME ZONE"));
        assert!(is_temporal("TIME"));
        assert!(!is_temporal("VARCHAR"));
    }

    /// The cases shared with the UI's "Copy as → Markdown" tests.
    fn golden_markdown() -> Vec<serde_json::Value> {
        let v: serde_json::Value = serde_json::from_str(include_str!("../tests/golden/markdown.json")).unwrap();
        v["cases"].as_array().unwrap().clone()
    }

    #[test]
    fn markdown_golden_cases() {
        for case in golden_markdown() {
            let strs = |k: &str| -> Vec<String> {
                case[k].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
            };
            let names = strs("columns");
            let numeric: Vec<bool> = case["numeric"].as_array().unwrap().iter().map(|x| x.as_bool().unwrap()).collect();
            let header = case["header"].as_bool().unwrap();
            let row_numbers = case["row_numbers"].as_bool().unwrap();
            let first = case["first_row"].as_u64().unwrap();
            let mut lines: Vec<String> = markdown_head(&names, &numeric, header, row_numbers).into();
            for (i, row) in case["rows"].as_array().unwrap().iter().enumerate() {
                let cells: Vec<Cell> = row.as_array().unwrap().iter().map(|x| x.as_str().map(String::from)).collect();
                lines.push(markdown_row(&cells, row_numbers.then_some(first + i as u64)));
            }
            assert_eq!(lines.join("\n"), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn markdown_escaping() {
        assert_eq!(markdown_cell("a|b"), "a\\|b");
        assert_eq!(markdown_cell("a\\|b"), "a\\\\\\|b");
        assert_eq!(markdown_cell("a\\b"), "a\\b");
        assert_eq!(markdown_cell("1\r\n2\r3\n"), "1<br>2<br>3<br>");
        assert_eq!(markdown_cell("日本 🎉"), "日本 🎉");
        assert_eq!(markdown_row(&[None, Some(String::new())], None), "|  |  |");
    }

    fn markdown_export(conn: &Connection, query: &str, header: bool, row_numbers: bool) -> (u64, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.md");
        let o = ExportOptions { include_header: header, row_numbers, ..ExportOptions::for_path(&path) };
        assert_eq!(o.format, ExportFormat::Markdown);
        let rows = export(conn, query, &o).unwrap();
        (rows, std::fs::read_to_string(&path).unwrap())
    }

    #[test]
    fn markdown_export_matches_the_copy() {
        let conn = Connection::open_in_memory().unwrap();
        let q = "SELECT * FROM (VALUES (1, 'a|b', 2.5::DOUBLE), (2, NULL, 10.0::DOUBLE)) t(id, name, price)";
        let cases = golden_markdown();
        let (rows, text) = markdown_export(&conn, q, true, false);
        assert_eq!(rows, 2);
        assert_eq!(text, format!("{}\n", cases[0]["expected"].as_str().unwrap()));
        let (_, text) = markdown_export(&conn, q, false, false);
        assert_eq!(text, format!("{}\n", cases[2]["expected"].as_str().unwrap()));
        // Grid formatting: 15 significant digits, ISO dates and timestamps.
        let q = "SELECT 0.1::DOUBLE + 0.2::DOUBLE AS f, DATE '2024-03-01' AS d, TIMESTAMP '2024-03-01 10:20:30' AS ts, \
                 12.50::DECIMAL(10,2) AS amount";
        let (_, text) = markdown_export(&conn, q, true, false);
        assert_eq!(text, "| f | d | ts | amount |\n| ---: | --- | --- | ---: |\n| 0.3 | 2024-03-01 | 2024-03-01 10:20:30 | 12.50 |\n");
    }

    #[test]
    fn markdown_export_streams_many_batches() {
        let conn = Connection::open_in_memory().unwrap();
        // Several Arrow batches (2048 rows each); row numbers carry on across them.
        let (rows, text) = markdown_export(&conn, "SELECT range AS n FROM range(5000)", true, true);
        assert_eq!(rows, 5000);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5002);
        assert_eq!(lines[0], "| # | n |");
        assert_eq!(lines[1], "| ---: | ---: |");
        assert_eq!(lines[2], "| 1 | 0 |");
        assert_eq!(lines[2050], "| 2049 | 2048 |");
        assert_eq!(lines[5001], "| 5000 | 4999 |");
    }
}
