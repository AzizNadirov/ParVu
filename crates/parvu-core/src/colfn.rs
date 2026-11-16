//! Functions applied to one column (the type-aware column menu): text
//! clean-up, case, extraction, and yes/no. Each is a per-row SQL
//! expression, so it stays lazy on large files.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::sql::{quote_ident, quote_literal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Side {
    #[default]
    Both,
    Start,
    End,
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
