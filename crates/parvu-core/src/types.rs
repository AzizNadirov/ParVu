//! Column types: the family of a DuckDB type, and detecting the types of a
//! text file's columns.
//!
//! CSV files: DuckDB's typed reader is the fast path (it parses straight
//! into typed values; reading text and converting costs 50–170 % more on a
//! full scan). ParVu checks DuckDB's guess against its own detectors on the
//! first [`SAMPLE_ROWS`] rows (taken with `LIMIT`, so only the start of the
//! file is read):
//!
//! - where they agree, the column stays **native** (DuckDB's type);
//! - where ParVu reads more (numbers like `1.234,5`, yes/no words, dates in
//!   another order, JSON), the column is read as text and converted with an
//!   expression that never fails (`TRY_CAST`, `try_strptime`, a `CASE`).
//!
//! A native column can still meet a value it can't parse past the sample;
//! DuckDB then fails the query naming the column, and [`heal_target`] lets
//! the workspace switch just that column to text + `TRY_CAST` and retry.

use duckdb::Connection;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::sql::{quote_ident, quote_literal};

/// Rows read to decide the column types.
pub const SAMPLE_ROWS: u32 = 10_000;

/// What kind of values a column holds, for menus, icons and alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeFamily {
    Integer,
    Decimal,
    Float,
    Text,
    Boolean,
    Date,
    Timestamp,
    TimestampTz,
    Time,
    Interval,
    Json,
    List,
    Struct,
    Map,
    Binary,
    Other,
}

impl TypeFamily {
    /// The family of a DuckDB type name such as `BIGINT`, `DECIMAL(18,2)`,
    /// `VARCHAR[]` or `STRUCT(a INTEGER)`.
    pub fn of(dtype: &str) -> Self {
        let t = dtype.trim().to_uppercase();
        if t.ends_with(']') {
            return TypeFamily::List;
        }
        let base = t.split('(').next().unwrap_or("").trim();
        match base {
            "TINYINT" | "SMALLINT" | "INTEGER" | "INT" | "BIGINT" | "HUGEINT" | "UTINYINT" | "USMALLINT"
            | "UINTEGER" | "UBIGINT" | "UHUGEINT" | "INT1" | "INT2" | "INT4" | "INT8" => TypeFamily::Integer,
            "DECIMAL" | "NUMERIC" => TypeFamily::Decimal,
            "FLOAT" | "DOUBLE" | "REAL" | "FLOAT4" | "FLOAT8" => TypeFamily::Float,
            "VARCHAR" | "TEXT" | "STRING" | "CHAR" | "BPCHAR" | "UUID" | "ENUM" => TypeFamily::Text,
            "BOOLEAN" | "BOOL" => TypeFamily::Boolean,
            "DATE" => TypeFamily::Date,
            "TIMESTAMP WITH TIME ZONE" | "TIMESTAMPTZ" => TypeFamily::TimestampTz,
            "TIMESTAMP" | "DATETIME" | "TIMESTAMP_S" | "TIMESTAMP_MS" | "TIMESTAMP_NS" => TypeFamily::Timestamp,
            "TIME" | "TIME WITH TIME ZONE" | "TIMETZ" => TypeFamily::Time,
            "INTERVAL" => TypeFamily::Interval,
            "JSON" => TypeFamily::Json,
            "STRUCT" | "UNION" => TypeFamily::Struct,
            "MAP" => TypeFamily::Map,
            "LIST" => TypeFamily::List,
            "BLOB" | "BYTEA" | "BIT" => TypeFamily::Binary,
            _ => TypeFamily::Other,
        }
    }
}

/// Which way to read dates such as `03/04/2024` when both readings fit
/// every value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateOrder {
    #[default]
    DayFirst,
    MonthFirst,
}

/// How files are typed when they open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectOptions {
    /// Off: every CSV column stays text.
    pub enabled: bool,
    pub date_order: DateOrder,
}

impl Default for DetectOptions {
    fn default() -> Self {
        Self { enabled: true, date_order: DateOrder::DayFirst }
    }
}

/// How a column of text is turned into its type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reading {
    /// Kept as text.
    Text,
    Integer,
    /// Plain numbers with a `.` decimal point.
    Number,
    /// Numbers with thousands separators and/or a `,` decimal comma, such as
    /// `1,234.5` (thousands `,`), `1.234,5` (thousands `.`, decimal `,`) or
    /// `1 234,5`.
    LocaleNumber { thousands: String, decimal: String },
    /// Words such as yes/no, y/n, true/false, on/off.
    Boolean,
    /// ISO dates (`2024-03-05`).
    Date,
    /// Dates in another format, e.g. `%d.%m.%Y`.
    DateFormat { format: String },
    /// ISO timestamps (with `T`, fractions or a zone).
    Timestamp,
    TimestampFormat { format: String },
    Time,
    Json,
    /// Any other DuckDB type, with `TRY_CAST` (used when a natively typed
    /// column has to fall back to text).
    Cast { dtype: String },
}

/// Words read as true / false (lower case). English, Russian, Azerbaijani.
const TRUE_WORDS: &[&str] = &["true", "yes", "y", "t", "on", "1", "да", "д", "bəli", "beli", "hə"];
const FALSE_WORDS: &[&str] = &["false", "no", "n", "f", "off", "0", "нет", "н", "xeyr", "yox"];

impl Reading {
    /// DuckDB type the column gets.
    pub fn dtype(&self) -> &str {
        match self {
            Reading::Cast { dtype } => dtype,
            Reading::Text => "VARCHAR",
            Reading::Integer => "BIGINT",
            Reading::Number | Reading::LocaleNumber { .. } => "DOUBLE",
            Reading::Boolean => "BOOLEAN",
            Reading::Date | Reading::DateFormat { .. } => "DATE",
            Reading::Timestamp | Reading::TimestampFormat { .. } => "TIMESTAMP",
            Reading::Time => "TIME",
            Reading::Json => "JSON",
        }
    }

    /// Expression turning the text column `column` into this type. Never
    /// fails: a value that doesn't fit gives NULL.
    pub fn expr(&self, column: &str) -> String {
        let c = quote_ident(column);
        // No trim(): it costs as much as the conversion itself, and
        // TRY_CAST already ignores spaces around numbers.
        let x = c.clone();
        match self {
            Reading::Text => c,
            Reading::Integer => format!("TRY_CAST({x} AS BIGINT)"),
            Reading::Number => format!("TRY_CAST({x} AS DOUBLE)"),
            Reading::LocaleNumber { thousands, decimal } => {
                let mut e = x;
                for sep in thousands.chars() {
                    e = format!("replace({e}, {}, '')", quote_literal(&sep.to_string()));
                }
                if decimal != "." {
                    e = format!("replace({e}, {}, '.')", quote_literal(decimal));
                }
                format!("TRY_CAST({e} AS DOUBLE)")
            }
            Reading::Boolean => format!(
                "CASE WHEN lower({x}) IN ({}) THEN true WHEN lower({x}) IN ({}) THEN false END",
                word_list(TRUE_WORDS),
                word_list(FALSE_WORDS)
            ),
            Reading::Date => format!("TRY_CAST({x} AS DATE)"),
            Reading::DateFormat { format } => format!("CAST(try_strptime({x}, {}) AS DATE)", quote_literal(format)),
            Reading::Timestamp => format!("TRY_CAST({x} AS TIMESTAMP)"),
            Reading::TimestampFormat { format } => format!("try_strptime({x}, {})", quote_literal(format)),
            Reading::Time => format!("TRY_CAST({x} AS TIME)"),
            Reading::Json => format!("TRY_CAST({x} AS JSON)"),
            Reading::Cast { dtype } if crate::sql::is_safe_type_name(dtype) => format!("TRY_CAST({x} AS {dtype})"),
            Reading::Cast { .. } => c,
        }
    }
}

fn word_list(words: &[&str]) -> String {
    words.iter().map(|w| quote_literal(w)).collect::<Vec<_>>().join(", ")
}

/// A column of a file and how it is read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceColumn {
    pub name: String,
    pub reading: Reading,
    /// Read by DuckDB's typed reader (fast). Otherwise read as text and
    /// converted with `reading`.
    pub native: bool,
    /// The sample allowed more than one reading (e.g. `03/04/2024`), and
    /// the date order setting chose this one.
    pub ambiguous: bool,
}

/// `SELECT` reading the CSV file at `path`: native columns as DuckDB types
/// them, the others as text converted by their reading.
pub fn csv_select(path: &str, columns: &[SourceColumn]) -> String {
    let text: Vec<String> =
        columns.iter().filter(|c| !c.native).map(|c| format!("{}: 'VARCHAR'", quote_literal(&c.name))).collect();
    let items: Vec<String> = columns
        .iter()
        .map(|c| match (&c.reading, c.native) {
            (_, true) | (Reading::Text, false) => quote_ident(&c.name),
            (r, false) => format!("{} AS {}", r.expr(&c.name), quote_ident(&c.name)),
        })
        .collect();
    let p = quote_literal(path);
    let reader = if text.is_empty() {
        format!("read_csv({p})")
    } else {
        format!("read_csv({p}, types = {{{}}})", text.join(", "))
    };
    format!("SELECT {} FROM {reader}", items.join(", "))
}

/// The column a failed query names when DuckDB couldn't parse one of the
/// CSV file's values: `(file, column)`.
pub fn heal_target(message: &str) -> Option<(String, String)> {
    if !message.contains("CSV Error") {
        return None;
    }
    let column = message.split("Error when converting column \"").nth(1)?.split("\".").next()?.to_string();
    let file = message.split("\n  file = ").nth(1)?.lines().next()?.trim().to_string();
    Some((file, column))
}

// ---------- detection ----------

const SAMPLE_TABLE: &str = "__pv_detect_sample";

/// Value shapes counted for every column, in this order. Each is a
/// condition on `x` (the trimmed, non-empty value).
const SHAPES: &[&str] = &[
    // 0: identifiers that look numeric but must stay text (00123, very long)
    r"regexp_full_match($x, '[+-]?0\d+') OR regexp_full_match($x, '[+-]?\d{19,}')",
    // 1: integer
    r"regexp_full_match($x, '[+-]?\d+')",
    // 2: number with a decimal point (not an integer)
    r"regexp_full_match($x, '[+-]?(\d+\.\d*|\.\d+|\d+)([eE][+-]?\d+)?') AND NOT regexp_full_match($x, '[+-]?\d+')",
    // 3: thousands with commas, US style (1,234 or 1,234.5)
    r"regexp_full_match($x, '[+-]?\d{1,3}(,\d{3})+(\.\d+)?')",
    // 4: European style: decimal comma and/or dotted thousands (2,5 / 1.234,5 / 1.234)
    r"regexp_full_match($x, '[+-]?\d+,\d+') OR regexp_full_match($x, '[+-]?\d{1,3}(\.\d{3})+(,\d+)?')",
    // 5: dotted thousands that also read as a decimal point (1.234)
    r"regexp_full_match($x, '[+-]?\d{1,3}(\.\d{3})+')",
    // 6: space thousands (1 234 / 1 234,5), including no-break spaces
    "regexp_full_match($x, '[+-]?\\d{1,3}([ \u{a0}\u{202f}]\\d{3})+(,\\d+)?')",
    // 7: yes/no words
    "", // filled in by shape_sql
    // 8: looks like a date (or date and time)
    r"regexp_full_match($x, '\d{1,4}[-./]\d{1,2}[-./]\d{1,4}([ T].*)?') OR regexp_full_match($x, '\d{1,2}[ -][A-Za-z]{3,9}[ -]\d{2,4}( .*)?') OR regexp_full_match($x, '[A-Za-z]{3,9} \d{1,2}, \d{4}( .*)?')",
    // 9: time of day
    r"regexp_full_match($x, '\d{1,2}:\d{2}(:\d{2}(\.\d+)?)?')",
    // 10: JSON object or array
    "(starts_with($x, '{') OR starts_with($x, '[')) AND json_valid($x)",
    // 11: has a four-digit year
    r"regexp_matches($x, '\d{4}')",
];

fn shape_sql(i: usize) -> String {
    if i == 7 {
        let words: Vec<&str> = TRUE_WORDS.iter().chain(FALSE_WORDS).copied().collect();
        format!("lower($x) IN ({})", word_list(&words))
    } else {
        SHAPES[i].to_string()
    }
}

/// Date formats tried on date-like columns, in order of preference. Pairs
/// that differ only in day/month order are resolved with [`DateOrder`].
const DATE_FORMATS: &[&str] = &[
    "%d.%m.%Y", "%d/%m/%Y", "%m/%d/%Y", "%d-%m-%Y", "%m-%d-%Y", "%Y/%m/%d", "%Y.%m.%d",
    "%d.%m.%y", "%d/%m/%y", "%m/%d/%y", "%d %b %Y", "%d %B %Y", "%d-%b-%Y", "%d-%b-%y", "%b %d, %Y", "%B %d, %Y",
];
const TIME_SUFFIXES: &[&str] = &[" %H:%M:%S", " %H:%M"];

/// Decide how to read each column of the CSV file at `path`.
pub fn detect(conn: &Connection, path: &str, options: DetectOptions) -> Result<Vec<SourceColumn>> {
    let p = quote_literal(path);
    let duck = describe(conn, &format!("SELECT * FROM read_csv({p})"))?;
    if !options.enabled {
        return Ok(duck
            .into_iter()
            .map(|(name, _)| SourceColumn { name, reading: Reading::Text, native: false, ambiguous: false })
            .collect());
    }
    conn.execute_batch(&format!(
        "CREATE OR REPLACE TEMP TABLE {SAMPLE_TABLE} AS SELECT * FROM read_csv({p}, all_varchar = true) LIMIT {SAMPLE_ROWS}"
    ))?;
    let result = (|| {
        let names: Vec<String> = describe(conn, &format!("SELECT * FROM {SAMPLE_TABLE}"))?.into_iter().map(|(n, _)| n).collect();
        let duck_names: Vec<&String> = duck.iter().map(|(n, _)| n).collect();
        if names.iter().collect::<Vec<_>>() != duck_names {
            return Err(crate::error::CoreError::invalid("the text and typed readers see different columns"));
        }
        let ours = detect_sample(conn, &names, options)?;
        Ok(ours.into_iter().zip(duck).map(|(c, (_, dtype))| merge(c, &dtype)).collect())
    })();
    let _ = conn.execute_batch(&format!("DROP TABLE IF EXISTS {SAMPLE_TABLE}"));
    result
}

/// Combine ParVu's reading of a column with DuckDB's type for it.
fn merge(mut c: SourceColumn, duck: &str) -> SourceColumn {
    let fam = TypeFamily::of(duck);
    let r = &c.reading;
    let agrees = match fam {
        TypeFamily::Text => matches!(r, Reading::Text),
        TypeFamily::Integer => matches!(r, Reading::Integer),
        TypeFamily::Float | TypeFamily::Decimal => matches!(r, Reading::Number | Reading::Integer),
        TypeFamily::Boolean => matches!(r, Reading::Boolean),
        TypeFamily::Date => matches!(r, Reading::Date | Reading::DateFormat { .. }) && !c.ambiguous,
        TypeFamily::Timestamp => matches!(r, Reading::Timestamp | Reading::TimestampFormat { .. }) && !c.ambiguous,
        TypeFamily::Time => matches!(r, Reading::Time),
        // ParVu found no type, DuckDB did (e.g. a zone offset): trust it.
        _ => matches!(r, Reading::Text),
    } || (matches!(r, Reading::Text) && fam != TypeFamily::Text);
    if agrees {
        c.native = true;
        // How the column is converted if it ever has to fall back to text.
        c.reading = match (fam, &c.reading) {
            (TypeFamily::Text, _) => Reading::Text,
            (TypeFamily::Date | TypeFamily::Timestamp | TypeFamily::Boolean, r) if !matches!(r, Reading::Text) => r.clone(),
            _ => Reading::Cast { dtype: duck.to_string() },
        };
    }
    c
}

/// `(name, type)` of each column of a query.
fn describe(conn: &Connection, query: &str) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(&format!("DESCRIBE {query}"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Runs `sql` (one row of integer counts) and returns the counts.
fn counts(conn: &Connection, sql: &str, n: usize) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([])?;
    let row = rows.next()?.ok_or_else(|| crate::error::CoreError::invalid("no result"))?;
    (0..n).map(|i| Ok(row.get::<_, Option<i64>>(i)?.unwrap_or(0))).collect()
}

/// `x` for column `c`: the value, NULL when empty.
fn x_of(c: &str) -> String {
    format!("NULLIF({}, '')", quote_ident(c))
}

fn detect_sample(conn: &Connection, names: &[String], options: DetectOptions) -> Result<Vec<SourceColumn>> {
    // Pass 1: count each value shape per column.
    let per = SHAPES.len() + 1;
    let mut items = Vec::with_capacity(names.len() * per);
    for c in names {
        let x = x_of(c);
        items.push(format!("count({x})"));
        for i in 0..SHAPES.len() {
            let cond = shape_sql(i).replace("$x", &x);
            items.push(format!("count(*) FILTER (WHERE {x} IS NOT NULL AND ({cond}))"));
        }
    }
    let n_items = items.len();
    let shapes = counts(conn, &format!("SELECT {} FROM {SAMPLE_TABLE}", items.join(", ")), n_items)?;

    let mut out = Vec::with_capacity(names.len());
    let mut date_like = Vec::new();
    for (k, name) in names.iter().enumerate() {
        let s = &shapes[k * per..(k + 1) * per];
        let (n, guard, int, dec, us, eu, dotted, space, words, date, time, json) =
            (s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7], s[8], s[9], s[10], s[11]);
        let reading = if n == 0 || guard > 0 {
            Reading::Text
        } else if int == n {
            Reading::Integer
        } else if int + dec == n {
            Reading::Number
        } else if us > 0 && int + dec + us == n {
            Reading::LocaleNumber { thousands: ",".into(), decimal: ".".into() }
        } else if eu > dotted && int + eu == n {
            Reading::LocaleNumber { thousands: ".".into(), decimal: ",".into() }
        } else if space > 0 && int + space + eu == n {
            Reading::LocaleNumber { thousands: " \u{a0}\u{202f}".into(), decimal: ",".into() }
        } else if words == n {
            Reading::Boolean
        } else if time == n {
            Reading::Time
        } else if json == n {
            Reading::Json
        } else if date == n {
            date_like.push((k, n, s[12] == n));
            Reading::Text
        } else {
            Reading::Text
        };
        out.push(SourceColumn { name: name.clone(), reading, native: false, ambiguous: false });
    }
    if !date_like.is_empty() {
        detect_dates(conn, names, &date_like, options, &mut out)?;
    }
    Ok(out)
}

/// Pass 2, only for date-like columns: which formats read every value.
fn detect_dates(
    conn: &Connection,
    names: &[String],
    date_like: &[(usize, i64, bool)],
    options: DetectOptions,
    out: &mut [SourceColumn],
) -> Result<()> {
    // ISO dates and timestamps, plus one example value per column.
    let mut items = Vec::new();
    for &(k, _, _) in date_like {
        let x = x_of(&names[k]);
        items.push(format!("count(TRY_CAST({x} AS DATE)) FILTER (WHERE regexp_full_match({x}, '\\d{{4}}-\\d{{2}}-\\d{{2}}'))"));
        items.push(format!("count(TRY_CAST({x} AS TIMESTAMP)) FILTER (WHERE regexp_matches({x}, '^\\d{{4}}-\\d{{2}}-\\d{{2}}[ T]'))"));
        items.push(format!("CAST(any_value({x}) AS VARCHAR)"));
    }
    let mut iso = Vec::new();
    {
        let mut stmt = conn.prepare(&format!("SELECT {} FROM {SAMPLE_TABLE}", items.join(", ")))?;
        let mut rows = stmt.query([])?;
        let row = rows.next()?.ok_or_else(|| crate::error::CoreError::invalid("no result"))?;
        for j in 0..date_like.len() {
            let date: i64 = row.get::<_, Option<i64>>(j * 3)?.unwrap_or(0);
            let ts: i64 = row.get::<_, Option<i64>>(j * 3 + 1)?.unwrap_or(0);
            let example: Option<String> = row.get(j * 3 + 2)?;
            iso.push((date, ts, example.unwrap_or_default()));
        }
    }
    // Formats worth trying: only those with the example's outline (digits,
    // letters and separators in the same places), so a column is tested
    // against two to four formats, not all of them.
    let all: Vec<String> = DATE_FORMATS
        .iter()
        .map(|f| f.to_string())
        .chain(DATE_FORMATS.iter().flat_map(|f| TIME_SUFFIXES.iter().map(move |t| format!("{f}{t}"))))
        .collect();
    let mut candidates: Vec<Vec<String>> = Vec::new();
    let mut items = Vec::new();
    for (j, &(k, n, _)) in date_like.iter().enumerate() {
        let (date, ts, example) = &iso[j];
        let fits: Vec<String> = if *date == n || *ts == n {
            Vec::new()
        } else {
            let shape = outline(example);
            all.iter().filter(|f| outline(&format_example(f)) == shape).cloned().collect()
        };
        let x = x_of(&names[k]);
        for f in &fits {
            items.push(format!("count(try_strptime({x}, {}))", quote_literal(f)));
        }
        candidates.push(fits);
    }
    let got = if items.is_empty() {
        Vec::new()
    } else {
        counts(conn, &format!("SELECT {} FROM {SAMPLE_TABLE}", items.join(", ")), items.len())?
    };
    let mut at = 0;
    for (j, &(k, n, four_digit_year)) in date_like.iter().enumerate() {
        let tried = &candidates[j];
        let s = &got[at..at + tried.len()];
        at += tried.len();
        if iso[j].0 == n {
            out[k].reading = Reading::Date;
            continue;
        }
        if iso[j].1 == n {
            out[k].reading = Reading::Timestamp;
            continue;
        }
        // Formats that read every value; a two-digit year format only when
        // the values have no four-digit year (`%Y` would read "24" as 0024).
        let fits: Vec<&String> = tried
            .iter()
            .enumerate()
            .filter(|(i, f)| s[*i] == n && (f.contains("%Y") == four_digit_year || !f.contains('%')))
            .map(|(_, f)| f)
            .collect();
        let Some(first) = fits.first() else { continue };
        let swapped = swap_day_month(first);
        let both = swapped.as_ref().is_some_and(|w| fits.contains(&w));
        let chosen = if both {
            let day_first = first.find("%d") < first.find("%m");
            match (options.date_order, day_first) {
                (DateOrder::DayFirst, true) | (DateOrder::MonthFirst, false) => (*first).clone(),
                _ => swapped.clone().unwrap_or_else(|| (*first).clone()),
            }
        } else {
            (*first).clone()
        };
        out[k].ambiguous = both;
        out[k].reading = if chosen.contains("%H") {
            Reading::TimestampFormat { format: chosen }
        } else {
            Reading::DateFormat { format: chosen }
        };
    }
    Ok(())
}

/// The outline of a value: runs of digits become `9`, runs of letters `a`,
/// everything else stays (`25.08.2024 10:00` → `9.9.9 9:9`).
fn outline(value: &str) -> String {
    let mut out = String::new();
    for ch in value.trim().chars() {
        let class = if ch.is_ascii_digit() {
            '9'
        } else if ch.is_alphabetic() {
            'a'
        } else {
            ch
        };
        if !((class == '9' || class == 'a') && out.ends_with(class)) {
            out.push(class);
        }
    }
    out
}

/// A value written in format `f`, for [`outline`] (`%d.%m.%Y` → `1.1.1`).
fn format_example(f: &str) -> String {
    f.replace("%b", "Jan").replace("%B", "January").replace(['%'], "").replace(['d', 'm', 'Y', 'y', 'H', 'M', 'S'], "1")
}

/// `%d/%m/%Y` ↔ `%m/%d/%Y`; `None` when the format has no day and month.
fn swap_day_month(f: &str) -> Option<String> {
    if !(f.contains("%d") && f.contains("%m")) {
        return None;
    }
    Some(f.replace("%d", "\u{1}").replace("%m", "%d").replace('\u{1}', "%m"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families() {
        assert_eq!(TypeFamily::of("BIGINT"), TypeFamily::Integer);
        assert_eq!(TypeFamily::of("DECIMAL(18,2)"), TypeFamily::Decimal);
        assert_eq!(TypeFamily::of("VARCHAR[]"), TypeFamily::List);
        assert_eq!(TypeFamily::of("STRUCT(a INTEGER)"), TypeFamily::Struct);
        assert_eq!(TypeFamily::of("TIMESTAMP WITH TIME ZONE"), TypeFamily::TimestampTz);
        assert_eq!(TypeFamily::of("JSON"), TypeFamily::Json);
        assert_eq!(TypeFamily::of("MAP(VARCHAR, INTEGER)"), TypeFamily::Map);
    }

    #[test]
    fn readings_are_safe_sql() {
        assert_eq!(Reading::Integer.expr("a b"), "TRY_CAST(\"a b\" AS BIGINT)");
        assert_eq!(
            Reading::LocaleNumber { thousands: ".".into(), decimal: ",".into() }.expr("x"),
            "TRY_CAST(replace(replace(\"x\", '.', ''), ',', '.') AS DOUBLE)"
        );
        assert_eq!(Reading::DateFormat { format: "%d'%m".into() }.expr("d"), "CAST(try_strptime(\"d\", '%d''%m') AS DATE)");
        assert_eq!(outline("25.08.2024 10:00"), "9.9.9 9:9");
        assert_eq!(outline(&format_example("%d.%m.%Y %H:%M")), "9.9.9 9:9");
        assert_eq!(outline(&format_example("%b %d, %Y")), outline("Aug 5, 2024"));
        assert_eq!(swap_day_month("%d/%m/%Y %H:%M").as_deref(), Some("%m/%d/%Y %H:%M"));
        assert_eq!(swap_day_month("%Y-%m"), None);
    }
}
