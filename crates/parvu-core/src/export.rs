//! Export of a query result to a file (port of `core/exporter.py` from the
//! export-dialog branch).
//!
//! Column selection, casts and date formatting are pushed into SQL. CSV,
//! Parquet and JSON are written by DuckDB's `COPY`, which streams and is
//! fast; the caller can watch progress and cancel through the connection's
//! progress and interrupt handles. Excel is written with `rust_xlsxwriter`
//! from Arrow batches in constant memory.
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
use crate::page::format_options;
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
}

impl ExportFormat {
    /// Format for a path's extension; CSV when unknown (as in Python).
    pub fn for_path(path: &Path) -> Self {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        match ext.as_str() {
            "parquet" | "pq" => ExportFormat::Parquet,
            "json" | "jsonl" | "ndjson" => ExportFormat::Json,
            "xlsx" => ExportFormat::Excel,
            _ => ExportFormat::Csv,
        }
    }

    pub fn default_extension(self) -> &'static str {
        match self {
            ExportFormat::Csv => "csv",
            ExportFormat::Parquet => "parquet",
            ExportFormat::Json => "json",
            ExportFormat::Excel => "xlsx",
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
            format!("CAST({ident} AS {})", spec.cast_to)
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
    let result = match o.format {
        ExportFormat::Excel => write_excel(conn, &query, &tmp),
        _ => {
            let sql = format!(
                "COPY ({query}) TO {} ({})",
                quote_literal(&tmp.to_string_lossy()),
                copy_options(o)?
            );
            conn.execute(&sql, []).map(|n| n as u64).map_err(CoreError::from)
        }
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

fn xlsx_err(e: rust_xlsxwriter::XlsxError) -> CoreError {
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
    use duckdb::arrow::datatypes::{Date32Type, TimestampMicrosecondType};
    use chrono::Datelike;
    use rust_xlsxwriter::{Format, Workbook};

    let total = crate::engine::count_rows(conn, query)?;
    if total + 1 > EXCEL_MAX_ROWS {
        return Err(CoreError::invalid(format!(
            "Excel files hold at most {} rows; this result has {total}",
            EXCEL_MAX_ROWS - 1
        )));
    }
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet_with_constant_memory().map_err(xlsx_err)?;
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
    wb.save(path).map_err(xlsx_err)?;
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
            "SELECT \"a\" AS \"a\", strftime(\"d\", '%d/%m/%Y') AS \"d\", CAST(\"t\" AS DATE) AS \"t\", \
             CAST(\"n\" AS DOUBLE) AS \"n\" FROM (Q)"
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
}
