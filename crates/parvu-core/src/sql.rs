//! Small SQL text helpers. All identifiers and literals that reach DuckDB go
//! through these, so user-provided names can never break a statement.

/// Quote an identifier: `my "col"` -> `"my ""col"""`.
pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Quote a string literal: `it's` -> `'it''s'`.
pub fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Turn a file stem into a SQL-friendly table name (port of `slugify_name`).
///
/// Lowercase, runs of anything outside `[a-z0-9_]` become `_`, leading and
/// trailing underscores are trimmed, a leading digit gets a `_` prefix, and an
/// empty result becomes `data`.
pub fn slugify_name(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut in_run = false;
    for ch in lower.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' {
            out.push(ch);
            in_run = false;
        } else if !in_run {
            out.push('_');
            in_run = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        return "data".to_string();
    }
    if trimmed.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

/// Wrap a query so LIMIT/OFFSET or other clauses can be appended safely.
pub fn wrap(query: &str) -> String {
    format!("SELECT * FROM ({query})")
}

/// A DuckDB type name is used verbatim inside `CAST(... AS <type>)`, so only
/// accept characters that can appear in a type spelling.
pub fn is_safe_type_name(t: &str) -> bool {
    !t.trim().is_empty()
        && t.chars().all(|c| c.is_ascii_alphanumeric() || " _(),[]".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(quote_ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(quote_literal("it's"), "'it''s'");
    }

    #[test]
    fn slugify_matches_python() {
        assert_eq!(slugify_name("My Data-File (1)"), "my_data_file_1");
        assert_eq!(slugify_name("2024 sales"), "_2024_sales");
        assert_eq!(slugify_name("___"), "data");
        assert_eq!(slugify_name(""), "data");
        assert_eq!(slugify_name("already_ok"), "already_ok");
        assert_eq!(slugify_name("Ünïcode"), "n_code");
    }

    #[test]
    fn type_names() {
        assert!(is_safe_type_name("DECIMAL(18,4)"));
        assert!(is_safe_type_name("TIMESTAMP WITH TIME ZONE"));
        assert!(is_safe_type_name("INTEGER[]"));
        assert!(!is_safe_type_name("INT); DROP TABLE x; --"));
        assert!(!is_safe_type_name(""));
    }
}
