//! Read-only sessions, the SQL guard and operation previews (plans/AI_PLAN.md §3, §6).

use std::time::Duration;

use parvu_core::Workspace;
use parvu_core::clean::{Combine, CondOp, Condition};
use parvu_core::ops::Operation;
use parvu_core::readonly::SampleRows;
use tempfile::TempDir;

/// Two tabs: `sales` (6 rows) and `regions` (3 rows).
fn setup() -> (TempDir, Workspace, u64, u64) {
    let dir = TempDir::new().unwrap();
    let sales = dir.path().join("sales.parquet");
    let regions = dir.path().join("regions.csv");
    let mut ws = Workspace::new(1000, None).unwrap();
    ws.connection()
        .execute_batch(&format!(
            "COPY (SELECT * FROM (VALUES
                (1, 'north', 'apple', 3, 1.5),
                (2, 'south', 'apple', NULL, 2.0),
                (3, 'north', 'pear', 5, 1.0),
                (4, 'south', 'pear', 2, 3.0),
                (5, 'north', 'apple', 7, 1.5),
                (6, 'east', 'plum', NULL, NULL)
             ) t(id, region, product, qty, price)) TO '{}' (FORMAT PARQUET)",
            sales.display()
        ))
        .unwrap();
    std::fs::write(&regions, "region,manager\nnorth,Ann\nsouth,Bob\neast,Cy\n").unwrap();
    let s = ws.open(&sales).unwrap();
    let r = ws.open(&regions).unwrap();
    (dir, ws, s, r)
}

fn cells(rows: &[Vec<Option<String>>]) -> Vec<Vec<String>> {
    rows.iter().map(|r| r.iter().map(|c| c.clone().unwrap_or_else(|| "∅".into())).collect()).collect()
}

fn filter_north() -> Operation {
    Operation::Filter {
        conditions: vec![Condition {
            column: "region".into(),
            op: CondOp::Eq,
            value: "north".into(),
            value2: String::new(),
            case_sensitive: false,
        }],
        combine: Combine::And,
    }
}

#[test]
fn queries_see_each_tab_with_its_steps() {
    let (_d, mut ws, s, _r) = setup();
    ws.apply(s, &filter_north()).unwrap();
    let ro = ws.read_session().unwrap();
    assert_eq!(ro.tables(), ["sales", "regions"]);

    let q = ro.query("SELECT COUNT(*) AS n FROM sales", 10, None).unwrap();
    assert_eq!(cells(&q.rows), [["3"]]);

    // Joins across tabs, CTEs, subqueries and qualified columns.
    let q = ro
        .query(
            "WITH big AS (SELECT * FROM sales WHERE qty > 4)
             SELECT big.product, r.manager FROM big JOIN regions r USING (region)
             WHERE big.id IN (SELECT id FROM sales) ORDER BY big.id",
            10,
            None,
        )
        .unwrap();
    assert_eq!(cells(&q.rows), [["pear", "Ann"], ["apple", "Ann"]]);
    assert_eq!(q.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["product", "manager"]);

    let q = ro.query("SELECT sales.id FROM sales ORDER BY 1 DESC", 2, None).unwrap();
    assert_eq!(cells(&q.rows), [["5"], ["3"]]);
    assert!(q.truncated);

    // Names are matched case-insensitively, like SQL identifiers.
    assert!(ro.query("SELECT * FROM SALES", 1, None).is_ok());

    // The session is a snapshot: later steps on the tab don't change it.
    ws.undo(s).unwrap();
    let q = ro.query("SELECT COUNT(*) FROM sales", 1, None).unwrap();
    assert_eq!(cells(&q.rows), [["3"]]);
}

#[test]
fn guard_refuses_writes_files_and_unknown_tables() {
    let (dir, ws, _s, _r) = setup();
    let ro = ws.read_session().unwrap();
    let secret = dir.path().join("secret.txt");
    std::fs::write(&secret, "token=abc").unwrap();
    let p = secret.display();

    let refused = [
        "INSERT INTO sales VALUES (9, 'x', 'y', 1, 1.0)",
        "DELETE FROM sales",
        "DROP VIEW sales",
        "CREATE TABLE t AS SELECT 1",
        "COPY sales TO 'out.csv'",
        "ATTACH 'other.db'",
        "PRAGMA database_list",
        "SET memory_limit = '1GB'",
        "INSTALL httpfs",
        "SELECT 1; SELECT 2",
        "DESCRIBE sales",
        &format!("SELECT * FROM read_text('{p}')"),
        &format!("SELECT * FROM read_csv('{p}')"),
        &format!("SELECT * FROM '{p}'"),
        &format!("SELECT (SELECT content FROM read_text('{p}')) FROM sales"),
        &format!("SELECT * FROM sales WHERE id IN (SELECT 1 FROM read_text('{p}'))"),
        &format!("WITH x AS (SELECT * FROM read_text('{p}')) SELECT * FROM x"),
        "SELECT * FROM glob('*')",
        "SELECT * FROM range(3), read_text('x')",
        "SELECT * FROM duckdb_settings()",
        "SELECT * FROM information_schema.tables",
        "SELECT * FROM main.sales",
        "SELECT * FROM __pv_src_sales",
        "SELECT * FROM nowhere",
        "SELECT current_setting('temp_directory')",
        "WITH sales AS (SELECT 1) SELECT * FROM sales",
    ];
    for sql in refused {
        let err = ro.query(sql, 10, None).expect_err(sql).to_string();
        assert!(!err.contains("token=abc"), "{sql}: leaked {err}");
    }

    let err = ro.query(&format!("SELECT * FROM read_text('{p}')"), 1, None).unwrap_err().to_string();
    assert!(err.contains("read_text"), "{err}");
    let err = ro.query("SELECT * FROM nowhere", 1, None).unwrap_err().to_string();
    assert!(err.contains("sales, regions"), "{err}");
    let err = ro.query("DELETE FROM sales", 1, None).unwrap_err().to_string();
    assert!(err.contains("Only SELECT"), "{err}");
}

#[test]
fn guard_keeps_quoted_names_comments_and_string_literals() {
    let (dir, mut ws, _s, _r) = setup();
    let odd = dir.path().join("my data.csv");
    std::fs::write(&odd, "a b,c\n1,read_text\n").unwrap();
    ws.open(&odd).unwrap();
    let ro = ws.read_session().unwrap();
    let name = ro.tables()[2].clone();
    let q = ro
        .query(
            &format!("-- a comment\nSELECT \"a b\", c FROM \"{name}\" WHERE c = 'read_text' /* not a call */"),
            10,
            None,
        )
        .unwrap();
    assert_eq!(cells(&q.rows), [["1", "read_text"]]);
}

#[test]
fn helpers_describe_sample_and_count() {
    let (_d, ws, _s, _r) = setup();
    let ro = ws.read_session().unwrap();

    let info = ro.describe_table("sales").unwrap();
    assert_eq!(info.rows, 6);
    assert_eq!(info.columns.len(), 5);
    assert!(info.steps.is_empty());

    let head = ro.sample("sales", SampleRows::Head, 2).unwrap();
    assert_eq!(cells(&head.rows)[0][0], "1");
    let tail = ro.sample("sales", SampleRows::Tail, 2).unwrap();
    assert_eq!(cells(&tail.rows).iter().map(|r| r[0].as_str()).collect::<Vec<_>>(), ["5", "6"]);
    let random = ro.sample("sales", SampleRows::Random, 3).unwrap();
    assert_eq!(random.rows.len(), 3);
    assert_eq!(random.rows, ro.sample("sales", SampleRows::Random, 3).unwrap().rows, "seeded");

    let vc = ro.value_counts("sales", "region", 2).unwrap();
    assert_eq!(cells(&vc.rows), [["north", "3"], ["south", "2"]]);

    let st = ro.column_stats("sales", "qty").unwrap();
    assert_eq!((st.row_count, st.null), (6, 2));
    assert!(ro.column_stats("sales", "nope").is_err());
    assert!(ro.describe_table("nope").is_err());
}

#[test]
fn long_queries_are_interrupted() {
    let (_d, ws, _s, _r) = setup();
    let ro = ws.read_session().unwrap();
    let slow = "SELECT COUNT(*) FROM sales, range(100000000) a, range(100000000) b";
    let started = std::time::Instant::now();
    let err = ro.query(slow, 1, Some(Duration::from_millis(300))).unwrap_err().to_string();
    assert!(err.contains("too long"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(10));
    // The session still works afterwards.
    assert!(ro.query("SELECT 1 AS one FROM sales LIMIT 1", 1, None).is_ok());
}

#[test]
fn operations_are_checked_like_queries() {
    let (dir, ws, s, _r) = setup();
    let ro = ws.read_session().unwrap();
    let p = dir.path().join("x.txt").display().to_string();
    let add = |expression: String| Operation::AddColumn { name: "z".into(), expression };

    assert!(ro.check_operation("sales", &add("\"qty\" * \"price\"".into())).is_ok());
    assert!(ro.check_operation("sales", &add("\"sales\".\"qty\" + 1".into())).is_ok());
    assert!(ro.check_operation("sales", &add(format!("(SELECT content FROM read_text('{p}'))"))).is_err());
    assert!(ro.check_operation("sales", &filter_north()).is_ok());
    let join = |right: &str| Operation::Join {
        right: right.into(),
        join_type: parvu_core::ops::JoinType::Left,
        left_key: "region".into(),
        right_key: "region".into(),
    };
    assert!(ro.check_operation("sales", &join("regions")).is_ok());
    assert!(ro.check_operation("sales", &join("elsewhere")).is_err());
    assert!(ro.check_operation("nowhere", &filter_north()).is_err());
    let _ = s;
}

#[test]
fn preview_shows_the_result_without_applying() {
    let (_d, ws, s, _r) = setup();
    let p = ws.preview_op(s, &filter_north(), 2).unwrap();
    assert_eq!(p.label.key, "step.filter_rows");
    assert_eq!((p.rows_before, p.rows_after), (6, 3));
    assert_eq!(p.rows.len(), 2);
    assert_eq!(p.columns.len(), 5);
    assert!(p.sql.contains("WHERE"));
    // Nothing was applied.
    assert!(ws.summary(s).unwrap().steps.is_empty());
    assert_eq!(ws.summary(s).unwrap().total_rows, 6);

    let bad = Operation::Remove { column: "nope".into() };
    assert!(ws.preview_op(s, &bad, 2).is_err());
}

#[cfg(feature = "schema")]
#[test]
fn operation_schema_covers_every_variant() {
    let schema = parvu_core::ops::operation_schema();
    let text = schema.to_string();
    for op in [
        "sort", "rename", "remove", "change_type", "duplicate", "drop_duplicates", "drop_null", "filter_values",
        "join", "append", "add_column", "replace", "group_by", "pivot", "melt", "transpose", "value_counts",
        "describe", "filter", "fill_null", "sort_by", "select_columns", "slice", "split_column", "bin", "window",
    ] {
        assert!(text.contains(&format!("\"{op}\"")), "schema lacks {op}");
    }
    // Doc comments become descriptions the model reads.
    assert!(text.contains("pandas `value_counts`"));
    assert!(text.contains("cum_sum") && text.contains("count_distinct") && text.contains("starts_with"));
    // An operation written against the schema deserialises.
    let op: Operation = serde_json::from_value(serde_json::json!({
        "op": "group_by", "by": ["region"],
        "aggregations": [{"column": "qty", "func": "sum", "alias": "total"}]
    }))
    .unwrap();
    assert!(matches!(op, Operation::GroupBy { .. }));
}
