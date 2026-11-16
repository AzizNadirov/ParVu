//! Functions applied to one column (the type-aware column menu): text
//! clean-up, case, extraction, and yes/no. Each is a per-row SQL
//! expression, so it stays lazy on large files.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::sql::{quote_ident, quote_literal};
use crate::types::TypeFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Side {
    #[default]
    Both,
    Start,
    End,
}

/// A part of a date or time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DatePart {
    Year,
    Quarter,
    Month,
    Day,
    DayOfYear,
    /// Monday = 1 … Sunday = 7.
    DayOfWeek,
    /// ISO week number.
    Week,
    Hour,
    Minute,
    Second,
}

impl DatePart {
    fn sql(self) -> &'static str {
        match self {
            DatePart::Year => "year",
            DatePart::Quarter => "quarter",
            DatePart::Month => "month",
            DatePart::Day => "day",
            DatePart::DayOfYear => "dayofyear",
            DatePart::DayOfWeek => "isodow",
            DatePart::Week => "week",
            DatePart::Hour => "hour",
            DatePart::Minute => "minute",
            DatePart::Second => "second",
        }
    }
}

/// A unit of time for truncating, adding and differences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    Year,
    Quarter,
    Month,
    Week,
    Day,
    Hour,
    Minute,
    Second,
}

impl TimeUnit {
    pub fn sql(self) -> &'static str {
        match self {
            TimeUnit::Year => "year",
            TimeUnit::Quarter => "quarter",
            TimeUnit::Month => "month",
            TimeUnit::Week => "week",
            TimeUnit::Day => "day",
            TimeUnit::Hour => "hour",
            TimeUnit::Minute => "minute",
            TimeUnit::Second => "second",
        }
    }

    /// `n` of this unit as an INTERVAL (quarters as 3 months).
    pub fn interval(self, n: i64) -> String {
        match self {
            TimeUnit::Quarter => format!("({} * INTERVAL 1 MONTH)", n * 3),
            u => format!("({n} * INTERVAL 1 {})", u.sql().to_uppercase()),
        }
    }
}

/// A function of one column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "fn", rename_all = "snake_case")]
pub enum ColumnFn {
    /// Remove spaces, tabs and line breaks around the text.
    Trim {
        #[serde(default)]
        side: Side,
    },
    /// Remove non-printable characters.
    Clean,
    /// Trim, and turn runs of spaces into one.
    CollapseSpaces,
    Lower,
    Upper,
    /// Capitalize Each Word.
    Proper,
    /// Ç → C, é → e.
    RemoveAccents,
    AddPrefix { text: String },
    AddSuffix { text: String },
    /// Pad to `length` characters with `fill` (longer text is kept).
    Pad {
        length: u32,
        #[serde(default = "zero")]
        fill: String,
        #[serde(default)]
        side: Side,
    },
    /// Number of characters.
    Length,
    /// The first `n` characters.
    First { n: u32 },
    /// The last `n` characters.
    Last { n: u32 },
    /// `length` characters from position `start` (1 is the first).
    Range { start: u32, length: u32 },
    /// Text before the first (or `last`) `delimiter`; empty when absent.
    Before {
        delimiter: String,
        #[serde(default)]
        last: bool,
    },
    /// Text after the first (or `last`) `delimiter`; empty when absent.
    After {
        delimiter: String,
        #[serde(default)]
        last: bool,
    },
    /// Text between the first `start` and the next `end` after it.
    Between { start: String, end: String },
    /// Group `group` (0: the whole match) of the first match of `pattern`.
    RegexExtract {
        pattern: String,
        #[serde(default)]
        group: u32,
    },
    /// Yes ↔ no.
    Invert,
    /// A part of a date or time, as a number.
    DatePart { part: DatePart },
    /// The month's name, from `names` (12, January first), so they come in
    /// the user's language; DuckDB's English names when empty.
    MonthName {
        #[serde(default)]
        names: Vec<String>,
    },
    /// The day's name, from `names` (7, Monday first).
    DayName {
        #[serde(default)]
        names: Vec<String>,
    },
    /// The start of the year, quarter, month, week (Monday), day or hour.
    StartOf { unit: TimeUnit },
    /// The last day of the month, quarter or year.
    EndOf { unit: TimeUnit },
    /// The date of a timestamp.
    DateOnly,
    /// The time of a timestamp.
    TimeOnly,
    /// Whole days, months or years from the date until today.
    Age { unit: TimeUnit },
    /// Add `n` units (negative subtracts).
    AddTime { n: i64, unit: TimeUnit },
    /// The date as text, with a `strftime` format such as `%d.%m.%Y`.
    FormatDate { format: String },
    /// Saturday or Sunday.
    IsWeekend,
}

fn zero() -> String {
    "0".into()
}

impl ColumnFn {
    /// The step label's key (`step.fn.trim`, …).
    pub fn key(&self) -> &'static str {
        match self {
            ColumnFn::Trim { .. } => "step.fn.trim",
            ColumnFn::Clean => "step.fn.clean",
            ColumnFn::CollapseSpaces => "step.fn.collapse_spaces",
            ColumnFn::Lower => "step.fn.lower",
            ColumnFn::Upper => "step.fn.upper",
            ColumnFn::Proper => "step.fn.proper",
            ColumnFn::RemoveAccents => "step.fn.remove_accents",
            ColumnFn::AddPrefix { .. } => "step.fn.add_prefix",
            ColumnFn::AddSuffix { .. } => "step.fn.add_suffix",
            ColumnFn::Pad { .. } => "step.fn.pad",
            ColumnFn::Length => "step.fn.length",
            ColumnFn::First { .. } => "step.fn.first",
            ColumnFn::Last { .. } => "step.fn.last",
            ColumnFn::Range { .. } => "step.fn.range",
            ColumnFn::Before { .. } => "step.fn.before",
            ColumnFn::After { .. } => "step.fn.after",
            ColumnFn::Between { .. } => "step.fn.between",
            ColumnFn::RegexExtract { .. } => "step.fn.regex_extract",
            ColumnFn::Invert => "step.fn.invert",
            ColumnFn::DatePart { .. } => "step.fn.date_part",
            ColumnFn::MonthName { .. } => "step.fn.month_name",
            ColumnFn::DayName { .. } => "step.fn.day_name",
            ColumnFn::StartOf { .. } => "step.fn.start_of",
            ColumnFn::EndOf { .. } => "step.fn.end_of",
            ColumnFn::DateOnly => "step.fn.date_only",
            ColumnFn::TimeOnly => "step.fn.time_only",
            ColumnFn::Age { .. } => "step.fn.age",
            ColumnFn::AddTime { .. } => "step.fn.add_time",
            ColumnFn::FormatDate { .. } => "step.fn.format_date",
            ColumnFn::IsWeekend => "step.fn.is_weekend",
        }
    }

    /// Default name for the new column (`<column>_length`, …).
    pub fn suffix(&self) -> &'static str {
        match self {
            ColumnFn::Length => "length",
            ColumnFn::First { .. } => "first",
            ColumnFn::Last { .. } => "last",
            ColumnFn::Range { .. } => "range",
            ColumnFn::Before { .. } => "before",
            ColumnFn::After { .. } => "after",
            ColumnFn::Between { .. } => "between",
            ColumnFn::RegexExtract { .. } => "match",
            ColumnFn::Invert => "not",
            ColumnFn::DatePart { part } => part.sql(),
            ColumnFn::MonthName { .. } => "month_name",
            ColumnFn::DayName { .. } => "day_name",
            ColumnFn::DateOnly => "date",
            ColumnFn::TimeOnly => "time",
            ColumnFn::Age { .. } => "age",
            ColumnFn::FormatDate { .. } => "text",
            ColumnFn::IsWeekend => "weekend",
            ColumnFn::StartOf { .. } => "start",
            ColumnFn::EndOf { .. } => "end",
            ColumnFn::AddTime { .. } => "shifted",
            _ => "text",
        }
    }

    /// The SQL expression over `column` (of DuckDB type `dtype`).
    pub fn expr(&self, column: &str, dtype: &str) -> Result<String> {
        let c = quote_ident(column);
        if *self == ColumnFn::Invert {
            if !dtype.eq_ignore_ascii_case("BOOLEAN") {
                return Err(CoreError::invalid(format!("\"{column}\" is not a yes/no column")));
            }
            return Ok(format!("(NOT {c})"));
        }
        if let Some(e) = self.date_expr(column, &c, dtype)? {
            return Ok(e);
        }
        // Text functions read any column as text.
        let x = if dtype.eq_ignore_ascii_case("VARCHAR") { c } else { format!("CAST({c} AS VARCHAR)") };
        let lit = |s: &str| quote_literal(s);
        let blanks = "' ' || chr(9) || chr(10) || chr(13)";
        Ok(match self {
            ColumnFn::Trim { side } => {
                let f = match side {
                    Side::Both => "trim",
                    Side::Start => "ltrim",
                    Side::End => "rtrim",
                };
                format!("{f}({x}, {blanks})")
            }
            ColumnFn::Clean => format!("regexp_replace({x}, '[[:cntrl:]]', '', 'g')"),
            ColumnFn::CollapseSpaces => format!("regexp_replace(trim({x}, {blanks}), '\\s+', ' ', 'g')"),
            ColumnFn::Lower => format!("lower({x})"),
            ColumnFn::Upper => format!("upper({x})"),
            ColumnFn::Proper => {
                format!("array_to_string(list_transform(string_split(lower({x}), ' '), lambda w: upper(w[1]) || w[2:]), ' ')")
            }
            ColumnFn::RemoveAccents => format!("strip_accents({x})"),
            ColumnFn::AddPrefix { text } => format!("({} || {x})", lit(text)),
            ColumnFn::AddSuffix { text } => format!("({x} || {})", lit(text)),
            ColumnFn::Pad { length, fill, side } => {
                if fill.is_empty() {
                    return Err(CoreError::invalid("Enter a character to pad with"));
                }
                let f = if *side == Side::End { "rpad" } else { "lpad" };
                // lpad/rpad cut longer text; keep it whole instead.
                format!("CASE WHEN length({x}) >= {length} THEN {x} ELSE {f}({x}, {length}, {}) END", lit(fill))
            }
            ColumnFn::Length => format!("length({x})"),
            ColumnFn::First { n } => format!("left({x}, {n})"),
            ColumnFn::Last { n } => format!("right({x}, {n})"),
            ColumnFn::Range { start, length } => format!("substring({x}, {}, {length})", (*start).max(1)),
            ColumnFn::Before { delimiter, last } | ColumnFn::After { delimiter, last } => {
                if delimiter.is_empty() {
                    return Err(CoreError::invalid("Enter a delimiter"));
                }
                let d = lit(delimiter);
                let before = matches!(self, ColumnFn::Before { .. });
                match (before, last) {
                    (true, false) => format!("CASE WHEN strpos({x}, {d}) > 0 THEN left({x}, strpos({x}, {d}) - 1) END"),
                    (false, false) => format!("CASE WHEN strpos({x}, {d}) > 0 THEN {x}[strpos({x}, {d}) + length({d}):] END"),
                    // The last occurrence, found in the reversed text.
                    (true, true) => format!(
                        "CASE WHEN strpos(reverse({x}), reverse({d})) > 0 \
                         THEN left({x}, length({x}) - strpos(reverse({x}), reverse({d})) - length({d}) + 1) END"
                    ),
                    (false, true) => format!(
                        "CASE WHEN strpos(reverse({x}), reverse({d})) > 0 THEN right({x}, strpos(reverse({x}), reverse({d})) - 1) END"
                    ),
                }
            }
            ColumnFn::Between { start, end } => {
                if start.is_empty() || end.is_empty() {
                    return Err(CoreError::invalid("Enter both delimiters"));
                }
                let (s, e) = (lit(start), lit(end));
                let rest = format!("{x}[strpos({x}, {s}) + length({s}):]");
                format!("CASE WHEN strpos({x}, {s}) > 0 AND strpos({rest}, {e}) > 0 THEN left({rest}, strpos({rest}, {e}) - 1) END")
            }
            ColumnFn::RegexExtract { pattern, group } => {
                if pattern.is_empty() {
                    return Err(CoreError::invalid("Enter a pattern"));
                }
                format!("NULLIF(regexp_extract({x}, {}, {group}), '')", lit(pattern))
            }
            ColumnFn::Invert => unreachable!("handled above"),
            _ => unreachable!("date functions are handled by date_expr"),
        })
    }

    /// The expression of a date function, `None` for other functions.
    fn date_expr(&self, column: &str, c: &str, dtype: &str) -> Result<Option<String>> {
        let family = TypeFamily::of(dtype);
        let is_date = family == TypeFamily::Date;
        let temporal = matches!(family, TypeFamily::Date | TypeFamily::Timestamp | TypeFamily::TimestampTz);
        let has_time = matches!(family, TypeFamily::Timestamp | TypeFamily::TimestampTz | TypeFamily::Time);
        let need = |ok: bool| {
            if ok { Ok(()) } else { Err(CoreError::invalid(format!("\"{column}\" is not a date column"))) }
        };
        // DuckDB returns a TIMESTAMP for date_trunc and DATE + INTERVAL; a
        // date column stays a date.
        let keep = |e: String| if is_date { format!("CAST({e} AS DATE)") } else { e };
        let names = |list: &[String], n: usize, idx: String, fallback: String| -> Result<String> {
            if list.is_empty() {
                return Ok(fallback);
            }
            if list.len() != n {
                return Err(CoreError::invalid(format!("Expected {n} names")));
            }
            let arms: Vec<String> =
                list.iter().enumerate().map(|(i, name)| format!("WHEN {} THEN {}", i + 1, quote_literal(name))).collect();
            Ok(format!("CASE {idx} {} END", arms.join(" ")))
        };
        Ok(Some(match self {
            ColumnFn::DatePart { part } => {
                let time_part = matches!(part, DatePart::Hour | DatePart::Minute | DatePart::Second);
                need(if time_part { has_time } else { temporal })?;
                format!("{}({c})", part.sql())
            }
            ColumnFn::MonthName { names: list } => {
                need(temporal)?;
                names(list, 12, format!("month({c})"), format!("monthname({c})"))?
            }
            ColumnFn::DayName { names: list } => {
                need(temporal)?;
                names(list, 7, format!("isodow({c})"), format!("dayname({c})"))?
            }
            ColumnFn::StartOf { unit } => {
                need(temporal)?;
                keep(format!("date_trunc('{}', {c})", unit.sql()))
            }
            ColumnFn::EndOf { unit } => {
                need(temporal)?;
                let start = format!("date_trunc('{}', {c})", unit.sql());
                match unit {
                    TimeUnit::Month => format!("last_day({c})"),
                    TimeUnit::Quarter | TimeUnit::Year => {
                        format!("CAST({start} + {} - INTERVAL 1 DAY AS DATE)", unit.interval(1))
                    }
                    _ => return Err(CoreError::invalid("End of: month, quarter or year")),
                }
            }
            ColumnFn::DateOnly => {
                need(temporal)?;
                format!("CAST({c} AS DATE)")
            }
            ColumnFn::TimeOnly => {
                need(has_time && family != TypeFamily::Time)?;
                format!("CAST({c} AS TIME)")
            }
            ColumnFn::Age { unit } => {
                need(temporal)?;
                format!("datesub('{}', CAST({c} AS DATE), current_date)", unit.sql())
            }
            ColumnFn::AddTime { n, unit } => {
                need(temporal)?;
                keep(format!("({c} + {})", unit.interval(*n)))
            }
            ColumnFn::FormatDate { format } => {
                need(temporal || family == TypeFamily::Time)?;
                if format.is_empty() {
                    return Err(CoreError::invalid("Enter a format"));
                }
                format!("strftime({c}, {})", quote_literal(format))
            }
            ColumnFn::IsWeekend => {
                need(temporal)?;
                format!("(isodow({c}) >= 6)")
            }
            _ => return Ok(None),
        }))
    }
}

/// A range of dates relative to today, for filtering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RelativeRange {
    /// The last `n` days, weeks, months… up to and including today.
    Last { n: u32, unit: TimeUnit },
    /// This week (from Monday), month, quarter or year.
    This { unit: TimeUnit },
    /// The week, month, quarter or year before this one.
    Previous { unit: TimeUnit },
    /// From 1 January to today.
    YearToDate,
}

impl RelativeRange {
    /// `[from, to)` as SQL expressions. "Today" is evaluated when the query
    /// runs, so the step keeps meaning "the last 7 days".
    pub fn bounds(&self) -> Result<(String, String)> {
        let tomorrow = "(current_date + INTERVAL 1 DAY)".to_string();
        let calendar = |u: TimeUnit| matches!(u, TimeUnit::Day | TimeUnit::Week | TimeUnit::Month | TimeUnit::Quarter | TimeUnit::Year);
        Ok(match self {
            RelativeRange::Last { n, unit } => {
                if *n == 0 || !calendar(*unit) {
                    return Err(CoreError::invalid("Choose a number of days, weeks, months, quarters or years"));
                }
                (format!("(current_date - {} + INTERVAL 1 DAY)", unit.interval(i64::from(*n))), tomorrow)
            }
            RelativeRange::This { unit } | RelativeRange::Previous { unit } => {
                if !calendar(*unit) {
                    return Err(CoreError::invalid("Choose a day, week, month, quarter or year"));
                }
                let start = format!("date_trunc('{}', current_date)", unit.sql());
                if matches!(self, RelativeRange::This { .. }) {
                    (start.clone(), format!("({start} + {})", unit.interval(1)))
                } else {
                    (format!("({start} - {})", unit.interval(1)), start)
                }
            }
            RelativeRange::YearToDate => ("date_trunc('year', current_date)".to_string(), tomorrow),
        })
    }
}

/// Where a column function writes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Output {
    /// Replace the column.
    #[default]
    InPlace,
    /// A new column right after the source; an empty name picks
    /// `<column>_<fn>`.
    New {
        #[serde(default)]
        name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions() {
        let e = |f: ColumnFn| f.expr("a", "VARCHAR").unwrap();
        assert_eq!(e(ColumnFn::Upper), "upper(\"a\")");
        assert_eq!(ColumnFn::Length.expr("n", "BIGINT").unwrap(), "length(CAST(\"n\" AS VARCHAR))");
        assert!(e(ColumnFn::AddPrefix { text: "x'y".into() }).contains("'x''y'"));
        assert!(ColumnFn::Invert.expr("a", "VARCHAR").is_err());
        assert_eq!(ColumnFn::Invert.expr("b", "BOOLEAN").unwrap(), "(NOT \"b\")");
        assert!(ColumnFn::Before { delimiter: String::new(), last: false }.expr("a", "VARCHAR").is_err());
    }
}
