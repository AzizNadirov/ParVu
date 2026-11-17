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

/// A query over all rows of `source` with the aggregates `items`, written
/// so that DuckDB keeps it within its memory limit.
///
/// DuckDB 2.0 computes some aggregates over a whole table (`MODE`,
/// `COUNT(DISTINCT)`) without spilling, so on a big file they can take all
/// the machine's memory. Grouped by a constant they go through the hash
/// aggregate, which spills to disk. `alias` names the rows for the items.
///
/// An empty `source` gives no row at all (there is no group); callers that
/// need one row fall back to [`whole_table_plain`], which is cheap then.
pub fn whole_table(items: &str, source: &str, alias: &str) -> String {
    format!("SELECT {items} FROM (SELECT *, 0 AS __pv_one FROM ({source}) AS {alias}) AS {alias} GROUP BY __pv_one")
}

/// The same query without the grouping: one row even for an empty source.
pub fn whole_table_plain(items: &str, source: &str, alias: &str) -> String {
    format!("SELECT {items} FROM ({source}) AS {alias}")
}

/// [`whole_table`] for a view: exactly one row, also for an empty source.
/// The second branch aggregates nothing unless the source is empty.
pub fn whole_table_view(items: &str, source: &str, alias: &str) -> String {
    format!(
        "SELECT * FROM ({}) UNION ALL SELECT {items} FROM ({source}) AS {alias} \
         WHERE NOT EXISTS (SELECT 1 FROM ({source})) HAVING NOT EXISTS (SELECT 1 FROM ({source}))",
        whole_table(items, source, alias)
    )
}

/// Run a one-row aggregate over a whole table within DuckDB's memory limit
/// (see [`whole_table`]), mapping the row with `f`.
pub fn query_whole_table<T>(
    conn: &duckdb::Connection,
    items: &str,
    source: &str,
    alias: &str,
    f: impl Fn(&duckdb::Row<'_>) -> duckdb::Result<T>,
) -> duckdb::Result<T> {
    match conn.query_row(&whole_table(items, source, alias), [], &f) {
        Err(duckdb::Error::QueryReturnedNoRows) => conn.query_row(&whole_table_plain(items, source, alias), [], &f),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_table_aggregates_group_by_a_constant() {
        let conn = duckdb::Connection::open_in_memory().unwrap();
        let items = "COUNT(*), COUNT(DISTINCT x), MODE(x)";
        assert!(whole_table(items, "SELECT 1 AS x", "t").ends_with("GROUP BY __pv_one"));
        let full = "SELECT * FROM (VALUES (1), (2), (2)) v(x)";
        let empty = "SELECT * FROM (VALUES (1)) v(x) WHERE false";
        let read = |source: &str| {
            query_whole_table(&conn, items, source, "t", |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<i32>>(2)?))
            })
            .unwrap()
        };
        assert_eq!(read(full), (3, 2, Some(2)));
        assert_eq!(read(empty), (0, 0, None));
        let view = |source: &str| {
            let mut st = conn.prepare(&whole_table_view(items, source, "t")).unwrap();
            st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i32>>(2)?))).unwrap().map(Result::unwrap).collect::<Vec<_>>()
        };
        assert_eq!(view(full), vec![(3, Some(2))]);
        assert_eq!(view(empty), vec![(0, None)]);
    }

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
