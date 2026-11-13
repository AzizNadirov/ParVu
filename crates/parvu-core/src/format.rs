//! Supported file formats and how each one is registered with DuckDB.
//!
//! Parquet, CSV and JSON are read lazily by DuckDB's own readers, so even huge
//! files are never loaded into memory. Excel has no built-in DuckDB reader
//! without downloading an extension, so it is read with `calamine` into a
//! DuckDB table once, when the file is opened.

use std::path::Path;

use calamine::{Data, Reader, open_workbook_auto};
use duckdb::Connection;
use duckdb::types::{TimeUnit, Value};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::sql::{quote_ident, quote_literal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileFormat {
    Parquet,
    Csv,
    Json,
    Excel,
}

impl FileFormat {
    pub const ALL: [FileFormat; 4] =
        [FileFormat::Parquet, FileFormat::Csv, FileFormat::Json, FileFormat::Excel];

    /// Extensions (lowercase, with dot) that map to this format.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            FileFormat::Parquet => &[".parquet", ".pq"],
            FileFormat::Csv => &[".csv", ".tsv"],
            FileFormat::Json => &[".json", ".jsonl", ".ndjson"],
            FileFormat::Excel => &[".xlsx", ".xls"],
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FileFormat::Parquet => "Parquet",
            FileFormat::Csv => "CSV",
            FileFormat::Json => "JSON",
            FileFormat::Excel => "Excel",
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        let dotted = format!(".{ext}");
        Self::ALL.into_iter().find(|f| f.extensions().contains(&dotted.as_str()))
    }

    /// Like [`FileFormat::from_path`] but returns an error naming the file.
    pub fn require(path: &Path) -> Result<Self> {
        Self::from_path(path).ok_or_else(|| {
            CoreError::UnsupportedFormat(
                path.extension().and_then(|e| e.to_str()).unwrap_or("(none)").to_string(),
            )
        })
    }

    /// Lazy reader SQL for formats DuckDB reads directly. `None` for Excel.
    pub fn reader_sql(self, path: &Path) -> Option<String> {
        let p = quote_literal(&path.to_string_lossy());
        match self {
            FileFormat::Parquet => Some(format!("SELECT * FROM read_parquet({p})")),
            FileFormat::Csv => Some(format!("SELECT * FROM read_csv({p})")),
            FileFormat::Json => Some(format!("SELECT * FROM read_json({p})")),
            FileFormat::Excel => None,
        }
    }
}

/// All extensions the app can open, sorted.
pub fn supported_extensions() -> Vec<&'static str> {
    let mut v: Vec<&str> = FileFormat::ALL.iter().flat_map(|f| f.extensions().iter().copied()).collect();
    v.sort_unstable();
    v
}

/// Make `relation` (a view name) read from `path`.
///
/// For Excel files the data is first copied into the table `excel_table`,
/// which must be a name the caller owns; the view then reads from it.
pub fn register_source(conn: &Connection, relation: &str, excel_table: &str, path: &Path) -> Result<()> {
    if !path.is_file() {
        return Err(CoreError::FileNotFound(path.display().to_string()));
    }
    let format = FileFormat::require(path)?;
    let select = match format.reader_sql(path) {
        Some(sql) => sql,
        None => {
            load_excel(conn, excel_table, path)?;
            format!("SELECT * FROM {}", quote_ident(excel_table))
        }
    };
    conn.execute_batch(&format!("CREATE OR REPLACE VIEW {} AS {select}", quote_ident(relation)))?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExcelType {
    Unknown,
    Bool,
    Int,
    Float,
    Date,
    Timestamp,
    Text,
}

impl ExcelType {
    fn sql(self) -> &'static str {
        match self {
            ExcelType::Bool => "BOOLEAN",
            ExcelType::Int => "BIGINT",
            ExcelType::Float => "DOUBLE",
            ExcelType::Date => "DATE",
            ExcelType::Timestamp => "TIMESTAMP",
            ExcelType::Unknown | ExcelType::Text => "VARCHAR",
        }
    }

    fn of(cell: &Data) -> ExcelType {
        match cell {
            Data::Empty | Data::Error(_) => ExcelType::Unknown,
            Data::Bool(_) => ExcelType::Bool,
            Data::Int(_) => ExcelType::Int,
            Data::Float(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => ExcelType::Int,
            Data::Float(_) => ExcelType::Float,
            Data::DateTime(dt) => match dt.as_datetime() {
                Some(t) if t.time() == chrono::NaiveTime::MIN => ExcelType::Date,
                Some(_) => ExcelType::Timestamp,
                None => ExcelType::Text,
            },
            _ => ExcelType::Text,
        }
    }

    fn merge(self, other: ExcelType) -> ExcelType {
        use ExcelType::*;
        match (self, other) {
            (a, Unknown) => a,
            (Unknown, b) => b,
            (a, b) if a == b => a,
            (Int, Float) | (Float, Int) => Float,
            (Date, Timestamp) | (Timestamp, Date) => Timestamp,
            _ => Text,
        }
    }
}

fn cell_text(cell: &Data) -> Option<String> {
    match cell {
        Data::Empty | Data::Error(_) => None,
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => Some(s.clone()),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => Some(format!("{}", *f as i64)),
        Data::DateTime(dt) => match dt.as_datetime() {
            Some(t) => Some(t.format("%Y-%m-%d %H:%M:%S").to_string()),
            None => Some(dt.as_f64().to_string()),
        },
        other => Some(other.to_string()),
    }
}

fn cell_value(cell: &Data, ty: ExcelType) -> Value {
    match (ty, cell) {
        (_, Data::Empty | Data::Error(_)) => Value::Null,
        (ExcelType::Bool, Data::Bool(b)) => Value::Boolean(*b),
        (ExcelType::Int, Data::Int(i)) => Value::BigInt(*i),
        (ExcelType::Int, Data::Float(f)) => Value::BigInt(*f as i64),
        (ExcelType::Float, Data::Int(i)) => Value::Double(*i as f64),
        (ExcelType::Float, Data::Float(f)) => Value::Double(*f),
        (ExcelType::Date, Data::DateTime(dt)) => match dt.as_datetime() {
            Some(t) => Value::Date32((t.date() - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid")).num_days() as i32),
            None => Value::Null,
        },
        (ExcelType::Timestamp, Data::DateTime(dt)) => match dt.as_datetime() {
            Some(t) => Value::Timestamp(TimeUnit::Microsecond, t.and_utc().timestamp_micros()),
            None => Value::Null,
        },
        _ => cell_text(cell).map(Value::Text).unwrap_or(Value::Null),
    }
}

/// Read the first worksheet of an Excel file into table `table`.
/// The first row is the header.
pub fn load_excel(conn: &Connection, table: &str, path: &Path) -> Result<()> {
    let mut wb = open_workbook_auto(path).map_err(|e| CoreError::Excel(e.to_string()))?;
    let range = wb
        .worksheet_range_at(0)
        .ok_or_else(|| CoreError::Excel("workbook has no sheets".into()))?
        .map_err(|e| CoreError::Excel(e.to_string()))?;

    let mut rows = range.rows();
    let header = rows.next().unwrap_or(&[]);
    let width = range.width();
    let mut names: Vec<String> = Vec::with_capacity(width);
    for i in 0..width {
        let base = header.get(i).and_then(cell_text).map(|s| s.trim().to_string()).unwrap_or_default();
        let base = if base.is_empty() { format!("column{i}") } else { base };
        let mut name = base.clone();
        let mut n = 1;
        while names.contains(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        names.push(name);
    }

    let body: Vec<&[Data]> = rows.collect();
    let mut types = vec![ExcelType::Unknown; width];
    for row in &body {
        for (i, cell) in row.iter().enumerate().take(width) {
            types[i] = types[i].merge(ExcelType::of(cell));
        }
    }

    let cols: Vec<String> =
        names.iter().zip(&types).map(|(n, t)| format!("{} {}", quote_ident(n), t.sql())).collect();
    conn.execute_batch(&format!(
        "CREATE OR REPLACE TABLE {} ({})",
        quote_ident(table),
        if cols.is_empty() { "column0 VARCHAR".to_string() } else { cols.join(", ") }
    ))?;

    if width > 0 {
        let mut app = conn.appender(table)?;
        for row in body {
            let values: Vec<Value> = (0..width)
                .map(|i| row.get(i).map(|c| cell_value(c, types[i])).unwrap_or(Value::Null))
                .collect();
            app.append_row(duckdb::appender_params_from_iter(values))?;
        }
        app.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_formats() {
        assert_eq!(FileFormat::from_path(Path::new("a.PARQUET")), Some(FileFormat::Parquet));
        assert_eq!(FileFormat::from_path(Path::new("a.pq")), Some(FileFormat::Parquet));
        assert_eq!(FileFormat::from_path(Path::new("a.tsv")), Some(FileFormat::Csv));
        assert_eq!(FileFormat::from_path(Path::new("a.ndjson")), Some(FileFormat::Json));
        assert_eq!(FileFormat::from_path(Path::new("a.xls")), Some(FileFormat::Excel));
        assert_eq!(FileFormat::from_path(Path::new("a.txt")), None);
        assert_eq!(FileFormat::from_path(Path::new("noext")), None);
    }

    #[test]
    fn reader_sql_escapes_quotes() {
        let sql = FileFormat::Csv.reader_sql(&PathBuf::from("/tmp/it's.csv")).unwrap();
        assert_eq!(sql, "SELECT * FROM read_csv('/tmp/it''s.csv')");
        assert!(FileFormat::Excel.reader_sql(Path::new("x.xlsx")).is_none());
    }

    #[test]
    fn type_merge() {
        use ExcelType::*;
        assert_eq!(Int.merge(Float), Float);
        assert_eq!(Int.merge(Unknown), Int);
        assert_eq!(Date.merge(Timestamp), Timestamp);
        assert_eq!(Bool.merge(Int), Text);
        assert_eq!(Unknown.merge(Unknown), Unknown);
    }

    #[test]
    fn extensions_sorted() {
        let e = supported_extensions();
        assert!(e.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(e.len(), 9);
    }
}
