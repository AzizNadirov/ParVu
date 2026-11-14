//! The pandas-style operations (reshape, clean, window) run against DuckDB.

use parvu_core::Workspace;
use parvu_core::chart::{ChartKind, ChartSpec};
use parvu_core::clean::{BinMethod, Combine, CondOp, Condition, FillMethod, SliceMode, SortKey};
use parvu_core::ops::Operation;
use parvu_core::reshape::{AggFunc, Aggregation};
use parvu_core::window::{RankMethod, WindowFunc, WindowSpec};
use tempfile::TempDir;

/// sales: id, region, product, qty (INTEGER, NULLs), price (DOUBLE), day (DATE), note (VARCHAR).
fn setup() -> (TempDir, Workspace, u64) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sales.parquet");
    let mut ws = Workspace::new(1000, None).unwrap();
    ws.connection()
        .execute_batch(&format!(
            "COPY (SELECT * FROM (VALUES
                (1, 'north', 'apple', 3, 1.5, DATE '2024-01-01', 'a-b-c'),
                (2, 'south', 'apple', NULL, 2.0, DATE '2024-01-02', NULL),
                (3, 'north', 'pear', 5, 1.0, DATE '2024-01-03', 'x-y'),
                (4, 'south', 'pear', 2, 3.0, DATE '2024-01-04', ''),
                (5, 'north', 'apple', 7, 1.5, DATE '2024-01-05', 'p'),
                (6, 'east', 'plum', NULL, NULL, NULL, 'q-r')
             ) t(id, region, product, qty, price, day, note)) TO '{}' (FORMAT PARQUET)",
            path.display()
        ))
        .unwrap();
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

fn rows(ws: &Workspace, id: u64) -> Vec<Vec<Option<String>>> {
    ws.page(id, 1).unwrap().rows
}

fn names(ws: &Workspace, id: u64) -> Vec<String> {
    ws.columns(id).unwrap().into_iter().map(|c| c.name).collect()
}

/// One column of the result as text ("∅" for NULL).
fn col(ws: &Workspace, id: u64, name: &str) -> Vec<String> {
    let i = names(ws, id).iter().position(|n| n == name).unwrap_or_else(|| panic!("no column {name}"));
    rows(ws, id).into_iter().map(|r| r[i].clone().unwrap_or_else(|| "∅".into())).collect()
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn agg(column: &str, func: AggFunc) -> Aggregation {
    Aggregation { column: column.into(), func, alias: None }
}

#[test]
fn group_by_aggregates_in_key_order() {
    let (_d, mut ws, id) = setup();
    ws.apply(
        id,
        &Operation::GroupBy {
            by: s(&["region"]),
            aggregations: vec![
                agg("", AggFunc::Rows),
                agg("qty", AggFunc::Count),
                agg("qty", AggFunc::Sum),
                agg("price", AggFunc::Mean),
                agg("product", AggFunc::First),
                agg("product", AggFunc::Last),
                agg("product", AggFunc::CountDistinct),
            ],
        },
    )
    .unwrap();
    assert_eq!(
        names(&ws, id),
        s(&["region", "rows", "qty_count", "qty_sum", "price_mean", "product_first", "product_last", "product_count_distinct"])
    );
    assert_eq!(col(&ws, id, "region"), s(&["east", "north", "south"]));
    assert_eq!(col(&ws, id, "rows"), s(&["1", "3", "2"]));
    assert_eq!(col(&ws, id, "qty_count"), s(&["0", "3", "1"]));
    assert_eq!(col(&ws, id, "qty_sum"), s(&["∅", "15", "2"]));
    assert_eq!(col(&ws, id, "product_first"), s(&["plum", "apple", "apple"]));
    assert_eq!(col(&ws, id, "product_last"), s(&["plum", "apple", "pear"]));
    assert_eq!(col(&ws, id, "product_count_distinct"), s(&["1", "2", "2"]));
    // Undo returns to the file.
    ws.undo(id).unwrap();
    assert_eq!(names(&ws, id).len(), 7);
}

#[test]
fn group_by_without_keys_is_one_row_and_aliases_are_unique() {
    let (_d, mut ws, id) = setup();
    ws.apply(
        id,
        &Operation::GroupBy {
            by: vec![],
            aggregations: vec![
                Aggregation { column: "qty".into(), func: AggFunc::Max, alias: Some("top".into()) },
                Aggregation { column: "qty".into(), func: AggFunc::Min, alias: Some("top".into()) },
            ],
        },
    )
    .unwrap();
    assert_eq!(names(&ws, id), s(&["top", "top_2"]));
    assert_eq!(rows(&ws, id), vec![vec![Some("7".into()), Some("2".into())]]);
}

#[test]
fn pivot_makes_a_column_per_value() {
    let (_d, mut ws, id) = setup();
    ws.apply(
        id,
        &Operation::Pivot { index: s(&["region"]), columns: "product".into(), values: Some("qty".into()), func: AggFunc::Sum },
    )
    .unwrap();
    assert_eq!(names(&ws, id), s(&["region", "apple", "pear", "plum"]));
    assert_eq!(col(&ws, id, "apple"), s(&["∅", "10", "∅"]));
    assert_eq!(col(&ws, id, "pear"), s(&["∅", "5", "2"]));
    // Crosstab: no values column counts rows.
    ws.undo(id).unwrap();
    ws.apply(id, &Operation::Pivot { index: s(&["product"]), columns: "region".into(), values: None, func: AggFunc::Count })
        .unwrap();
    assert_eq!(names(&ws, id), s(&["product", "east", "north", "south"]));
    assert_eq!(col(&ws, id, "north"), s(&["2", "1", "0"]));
}

#[test]
fn melt_turns_columns_into_rows() {
    let (_d, mut ws, id) = setup();
    ws.apply(
        id,
        &Operation::Melt {
            id_columns: s(&["id"]),
            value_columns: s(&["qty", "price"]),
            variable_name: "variable".into(),
            value_name: "value".into(),
        },
    )
    .unwrap();
    assert_eq!(names(&ws, id), s(&["id", "variable", "value"]));
    let r = rows(&ws, id);
    assert_eq!(r.len(), 12, "NULLs are kept, as in pandas");
    // Mixed types (INTEGER, DOUBLE) become text.
    assert!(r.iter().any(|row| row[1].as_deref() == Some("qty") && row[2].as_deref() == Some("3")));
    assert!(r.iter().any(|row| row[1].as_deref() == Some("price") && row[2].is_none()));
    // A clash with a kept column is refused.
    assert!(
        ws.apply(
            id,
            &Operation::Melt { id_columns: s(&["id"]), value_columns: vec![], variable_name: "id".into(), value_name: "v".into() }
        )
        .is_err()
    );
}

#[test]
fn transpose_with_and_without_header() {
    let (_d, mut ws, id) = setup();
    ws.apply(id, &Operation::SelectColumns { columns: s(&["product", "qty"]) }).unwrap();
    ws.apply(id, &Operation::Slice { mode: SliceMode::Head, n: 3, seed: None }).unwrap();
    ws.apply(id, &Operation::Transpose { header_column: Some("product".into()) }).unwrap();
    // Duplicate header values get suffixes.
    assert_eq!(names(&ws, id), s(&["column", "apple", "apple_2", "pear"]));
    assert_eq!(rows(&ws, id), vec![vec![Some("qty".into()), Some("3".into()), None, Some("5".into())]]);
    ws.undo(id).unwrap();
    ws.apply(id, &Operation::Transpose { header_column: None }).unwrap();
    assert_eq!(names(&ws, id), s(&["column", "row_1", "row_2", "row_3"]));
    assert_eq!(col(&ws, id, "column"), s(&["product", "qty"]));
}

#[test]
fn value_counts_and_describe() {
    let (_d, mut ws, id) = setup();
    ws.apply(id, &Operation::ValueCounts { columns: s(&["region"]), normalize: true, dropna: false }).unwrap();
    assert_eq!(names(&ws, id), s(&["region", "count", "proportion"]));
    assert_eq!(col(&ws, id, "region"), s(&["north", "south", "east"]));
    assert_eq!(col(&ws, id, "count"), s(&["3", "2", "1"]));
    assert_eq!(col(&ws, id, "proportion")[0], "0.5");
    ws.undo(id).unwrap();
    ws.apply(id, &Operation::Describe).unwrap();
    let n = names(&ws, id);
    assert!(n.contains(&"column_name".to_string()) && n.contains(&"null_percentage".to_string()), "{n:?}");
    assert_eq!(rows(&ws, id).len(), 7);
}

fn cond(column: &str, op: CondOp, value: &str) -> Condition {
    Condition { column: column.into(), op, value: value.into(), value2: String::new(), case_sensitive: false }
}

#[test]
fn filter_conditions() {
    let (_d, mut ws, id) = setup();
    let mut check = |conditions: Vec<Condition>, combine: Combine, expected: &[&str]| {
        ws.apply(id, &Operation::Filter { conditions, combine }).unwrap();
        assert_eq!(col(&ws, id, "id"), s(expected));
        ws.undo(id).unwrap();
    };
    check(vec![cond("qty", CondOp::Gt, "2")], Combine::And, &["1", "3", "5"]);
    check(vec![cond("qty", CondOp::Ge, "3"), cond("region", CondOp::Eq, "NORTH")], Combine::And, &["1", "3", "5"]);
    check(vec![cond("region", CondOp::Eq, "east"), cond("qty", CondOp::Lt, "3")], Combine::Or, &["4", "6"]);
    check(
        vec![Condition { value2: "2024-01-03".into(), ..cond("day", CondOp::Between, "2024-01-02") }],
        Combine::And,
        &["2", "3"],
    );
    check(vec![cond("note", CondOp::Contains, "-")], Combine::And, &["1", "3", "6"]);
    check(vec![Condition { case_sensitive: true, ..cond("product", CondOp::StartsWith, "P") }], Combine::And, &[]);
    check(vec![cond("product", CondOp::StartsWith, "P")], Combine::And, &["3", "4", "6"]);
    check(vec![cond("note", CondOp::Matches, "^[a-c]")], Combine::And, &["1"]);
    check(vec![cond("region", CondOp::In, "north, east")], Combine::And, &["1", "3", "5", "6"]);
    check(vec![cond("qty", CondOp::NotIn, "3,5")], Combine::And, &["4", "5"]);
    check(vec![cond("qty", CondOp::IsNull, "")], Combine::And, &["2", "6"]);
    check(vec![cond("note", CondOp::IsEmpty, "")], Combine::And, &["2", "4"]);
    // A non-number for a numeric column is refused before running.
    assert!(ws.apply(id, &Operation::Filter { conditions: vec![cond("qty", CondOp::Gt, "abc")], combine: Combine::And }).is_err());
}

#[test]
fn fill_null_methods() {
    let (_d, mut ws, id) = setup();
    let mut check = |columns: &[&str], method: FillMethod, value: &str, column: &str, expected: &[&str]| {
        ws.apply(id, &Operation::FillNull { columns: s(columns), method, value: value.into() }).unwrap();
        assert_eq!(col(&ws, id, column), s(expected), "{method:?}");
        assert_eq!(col(&ws, id, "id"), s(&["1", "2", "3", "4", "5", "6"]), "row order is kept");
        ws.undo(id).unwrap();
    };
    check(&["qty"], FillMethod::Value, "0", "qty", &["3", "0", "5", "2", "7", "0"]);
    check(&["qty"], FillMethod::Forward, "", "qty", &["3", "3", "5", "2", "7", "7"]);
    check(&["qty"], FillMethod::Backward, "", "qty", &["3", "5", "5", "2", "7", "∅"]);
    check(&["qty"], FillMethod::Mean, "", "qty", &["3.0", "4.25", "5.0", "2.0", "7.0", "4.25"]);
    check(&["note"], FillMethod::Value, "n/a", "note", &["a-b-c", "n/a", "x-y", "", "p", "q-r"]);
    check(&["day"], FillMethod::Forward, "", "day", &["2024-01-01", "2024-01-02", "2024-01-03", "2024-01-04", "2024-01-05", "2024-01-05"]);
}

#[test]
fn sort_by_several_columns_replaces_previous_sort() {
    let (_d, mut ws, id) = setup();
    ws.apply(id, &Operation::Sort { column: "id".into(), ascending: false }).unwrap();
    ws.apply(
        id,
        &Operation::SortBy {
            keys: vec![
                SortKey { column: "region".into(), ascending: true, nulls_first: false },
                SortKey { column: "qty".into(), ascending: false, nulls_first: false },
            ],
        },
    )
    .unwrap();
    assert_eq!(col(&ws, id, "id"), s(&["6", "5", "3", "1", "4", "2"]));
    let q = ws.current_query(id).unwrap();
    assert_eq!(q.matches("ORDER BY").count(), 1, "the earlier sort is replaced: {q}");
}

#[test]
fn select_slice_split_bin() {
    let (_d, mut ws, id) = setup();
    ws.apply(id, &Operation::SelectColumns { columns: s(&["note", "id"]) }).unwrap();
    assert_eq!(names(&ws, id), s(&["note", "id"]));
    ws.undo(id).unwrap();

    ws.apply(id, &Operation::Slice { mode: SliceMode::Tail, n: 2, seed: None }).unwrap();
    assert_eq!(col(&ws, id, "id"), s(&["5", "6"]));
    ws.undo(id).unwrap();
    ws.apply(id, &Operation::Slice { mode: SliceMode::Sample, n: 3, seed: Some(7) }).unwrap();
    let sample = col(&ws, id, "id");
    assert_eq!(sample.len(), 3);
    let mut sorted = sample.iter().map(|x| x.parse::<i32>().unwrap()).collect::<Vec<_>>();
    sorted.sort();
    assert_eq!(sample, sorted.iter().map(|x| x.to_string()).collect::<Vec<_>>(), "sample keeps table order");
    ws.undo(id).unwrap();

    ws.apply(
        id,
        &Operation::SplitColumn { column: "note".into(), separator: "-".into(), regex: false, parts: None, keep_original: false },
    )
    .unwrap();
    assert_eq!(names(&ws, id), s(&["id", "region", "product", "qty", "price", "day", "note_1", "note_2", "note_3"]));
    assert_eq!(col(&ws, id, "note_3"), s(&["c", "∅", "∅", "∅", "∅", "∅"]));
    assert_eq!(col(&ws, id, "note_1"), s(&["a", "∅", "x", "", "p", "q"]));
    ws.undo(id).unwrap();

    ws.apply(
        id,
        &Operation::Bin {
            column: "qty".into(),
            method: BinMethod::EqualWidth,
            bins: 5,
            edges: vec![],
            labels: vec![],
            output: "qty_bin".into(),
        },
    )
    .unwrap();
    assert_eq!(col(&ws, id, "qty_bin"), s(&["[2, 3]", "∅", "(4, 5]", "[2, 3]", "(6, 7]", "∅"]));
    ws.undo(id).unwrap();
    ws.apply(
        id,
        &Operation::Bin {
            column: "price".into(),
            method: BinMethod::Edges,
            bins: 0,
            edges: vec![0.0, 1.5, 10.0],
            labels: s(&["cheap", "dear"]),
            output: "band".into(),
        },
    )
    .unwrap();
    assert_eq!(col(&ws, id, "band"), s(&["cheap", "dear", "cheap", "dear", "cheap", "∅"]));
    ws.undo(id).unwrap();
    ws.apply(
        id,
        &Operation::Bin {
            column: "id".into(),
            method: BinMethod::Quantile,
            bins: 2,
            edges: vec![],
            labels: vec![],
            output: "half".into(),
        },
    )
    .unwrap();
    assert_eq!(col(&ws, id, "half"), s(&["[1, 3.5]", "[1, 3.5]", "[1, 3.5]", "(3.5, 6]", "(3.5, 6]", "(3.5, 6]"]));
}

fn win(func: WindowFunc, column: &str) -> WindowSpec {
    WindowSpec {
        column: column.into(),
        func,
        periods: None,
        window: None,
        rank_method: RankMethod::Average,
        ascending: true,
        partition_by: vec![],
        order_by: None,
        output: "out".into(),
    }
}

#[test]
fn window_functions() {
    let (_d, mut ws, id) = setup();
    let mut check = |spec: WindowSpec, expected: &[&str]| {
        ws.apply(id, &Operation::Window(spec.clone())).unwrap();
        assert_eq!(col(&ws, id, "out"), s(expected), "{:?}", spec.func);
        assert_eq!(col(&ws, id, "id"), s(&["1", "2", "3", "4", "5", "6"]), "row order is kept");
        ws.undo(id).unwrap();
    };
    check(win(WindowFunc::CumSum, "qty"), &["3", "3", "8", "10", "17", "17"]);
    check(win(WindowFunc::CumMax, "qty"), &["3", "3", "5", "5", "7", "7"]);
    check(win(WindowFunc::CumCount, ""), &["0", "1", "2", "3", "4", "5"]);
    check(win(WindowFunc::Shift, "id"), &["∅", "1", "2", "3", "4", "5"]);
    check(WindowSpec { periods: Some(-2), ..win(WindowFunc::Shift, "id") }, &["3", "4", "5", "6", "∅", "∅"]);
    check(win(WindowFunc::Diff, "price"), &["∅", "0.5", "-1.0", "2.0", "-1.5", "∅"]);
    check(win(WindowFunc::PctChange, "id"), &["∅", "1.0", "0.5", "0.33333333333333326", "0.25", "0.19999999999999996"]);
    check(WindowSpec { window: Some(2), ..win(WindowFunc::RollingSum, "id") }, &["∅", "3", "5", "7", "9", "11"]);
    check(WindowSpec { window: Some(2), ..win(WindowFunc::RollingMean, "qty") }, &["∅", "∅", "∅", "3.5", "4.5", "∅"]);
    // price: 1.5, 2.0, 1.0, 3.0, 1.5, NULL; ties at 1.5.
    check(win(WindowFunc::Rank, "price"), &["2.5", "4.0", "1.0", "5.0", "2.5", "∅"]);
    check(WindowSpec { rank_method: RankMethod::Min, ..win(WindowFunc::Rank, "price") }, &["2", "4", "1", "5", "2", "∅"]);
    check(WindowSpec { rank_method: RankMethod::Max, ..win(WindowFunc::Rank, "price") }, &["3", "4", "1", "5", "3", "∅"]);
    check(WindowSpec { rank_method: RankMethod::Dense, ..win(WindowFunc::Rank, "price") }, &["2", "3", "1", "4", "2", "∅"]);
    check(WindowSpec { rank_method: RankMethod::First, ..win(WindowFunc::Rank, "price") }, &["2", "4", "1", "5", "3", "∅"]);
    check(
        WindowSpec { ascending: false, rank_method: RankMethod::Min, ..win(WindowFunc::Rank, "price") },
        &["3", "2", "5", "1", "3", "∅"],
    );
    // Per group: running total of qty within each region.
    check(
        WindowSpec { partition_by: s(&["region"]), ..win(WindowFunc::CumSum, "qty") },
        &["3", "∅", "8", "2", "15", "∅"],
    );
    // Ordered by another column.
    check(
        WindowSpec { order_by: Some("price".into()), ..win(WindowFunc::RowNumber, "") },
        &["2", "4", "1", "5", "3", "6"],
    );
}

#[test]
fn operations_from_ui_json() {
    let op: Operation = serde_json::from_str(
        r#"{"op":"window","column":"qty","func":"rolling_mean","window":3,"partition_by":["region"],"output":"r"}"#,
    )
    .unwrap();
    assert!(matches!(op, Operation::Window(WindowSpec { func: WindowFunc::RollingMean, ascending: true, .. })));
    let op: Operation = serde_json::from_str(r#"{"op":"describe"}"#).unwrap();
    assert_eq!(op, Operation::Describe);
    let op: Operation = serde_json::from_str(
        r#"{"op":"filter","conditions":[{"column":"a","op":"between","value":"1","value2":"2"}]}"#,
    )
    .unwrap();
    assert!(matches!(op, Operation::Filter { combine: Combine::And, .. }));
    let op: Operation =
        serde_json::from_str(r#"{"op":"group_by","by":["a"],"aggregations":[{"func":"rows"},{"column":"b","func":"mean"}]}"#)
            .unwrap();
    assert!(matches!(op, Operation::GroupBy { .. }));
}

/// The SQL names the expression language compiles to exist in DuckDB with these arguments.
#[test]
fn expression_function_names_exist_in_duckdb() {
    let ws = Workspace::new(100, None).unwrap();
    let sql = "SELECT IF(true, 1, 2), IFNULL(NULL, 1), GREATEST(1, 2, 3), LEAST(1, 2), \
               SPLIT_PART('a@b', '@', 2), SUBSTRING('abc', 1, 2), LEFT('abc', 2), RIGHT('abc', 2), \
               LPAD('1', 3, '0'), RPAD('1', 3, '_'), REVERSE('ab'), REGEXP_EXTRACT('a123', '(\\d+)', 1), \
               REGEXP_MATCHES('abc', 'b'), STRFTIME(DATE '2024-03-05', '%Y-%m'), \
               STRPTIME('05.03.2024', '%d.%m.%Y'), QUARTER(DATE '2024-03-05'), WEEK(DATE '2024-03-05'), \
               DAYOFWEEK(DATE '2024-03-05'), HOUR(TIMESTAMP '2024-03-05 13:45:00'), \
               MINUTE(TIMESTAMP '2024-03-05 13:45:00'), DATE_TRUNC('month', DATE '2024-03-05'), \
               LN(1), LOG10(100), EXP(0), MOD(7, 3), SIGN(-2)";
    let row: Vec<String> = ws
        .connection()
        .prepare(&format!("SELECT * FROM ({sql}) t"))
        .unwrap()
        .query_row([], |r| (0..26).map(|i| r.get::<_, duckdb::types::Value>(i).map(|v| format!("{v:?}"))).collect())
        .unwrap();
    assert_eq!(row.len(), 26);
    assert!(row[4].contains('b') && row[11].contains("123") && row[13].contains("2024-03"), "{row:?}");
}

#[test]
fn batch_edits_are_one_step_and_undo_restores_earlier_edits() {
    let (_d, mut ws, id) = setup();
    // An earlier single edit on a cell the batch will overwrite.
    ws.edit_cell(id, 2, "region", Some("west".into())).unwrap();
    let fill: Vec<(u64, String, Option<String>)> =
        (1..4).map(|r| (r, "region".to_string(), Some("north".to_string()))).collect();
    // Rows 1-3 become "north": 1 and 3 were "south", 2 was edited to "west".
    let n = ws.edit_cells(id, &fill).unwrap();
    assert_eq!(n, 3);
    assert_eq!(col(&ws, id, "region"), s(&["north", "north", "north", "north", "north", "east"]));
    assert_eq!(ws.summary(id).unwrap().steps.len(), 2);
    ws.undo(id).unwrap();
    assert_eq!(col(&ws, id, "region"), s(&["north", "south", "west", "south", "north", "east"]), "the earlier edit is back");
    // Invalid values are refused and change nothing.
    assert!(ws.edit_cells(id, &[(0, "qty".into(), Some("x".into())), (1, "qty".into(), Some("1".into()))]).is_err());
    assert_eq!(col(&ws, id, "qty"), s(&["3", "∅", "5", "2", "7", "∅"]));
    // Unchanged values make no step.
    assert_eq!(ws.edit_cells(id, &[(0, "qty".into(), Some("3".into()))]).unwrap(), 0);
}

#[test]
fn numeric_range_for_colour_scales() {
    let (_d, ws, id) = setup();
    assert_eq!(ws.numeric_range(id, "qty").unwrap(), Some((2.0, 7.0)));
    assert_eq!(ws.numeric_range(id, "region").unwrap(), None);
    assert!(ws.numeric_range(id, "nope").is_err());
}

fn chart(kind: ChartKind, x: &str, y: Option<&str>, agg: AggFunc) -> ChartSpec {
    ChartSpec { kind, x: x.into(), y: y.map(String::from), agg, bins: None, sort_by_value: false }
}

#[test]
fn chart_data_is_aggregated_in_duckdb() {
    let (_d, ws, id) = setup();
    let bar = ws.chart(id, &chart(ChartKind::Bar, "region", Some("qty"), AggFunc::Sum)).unwrap();
    assert_eq!(bar.labels, s(&["east", "north", "south"]));
    assert_eq!(bar.values, vec![None, Some(15.0), Some(2.0)]);
    let bar = ws.chart(id, &ChartSpec { sort_by_value: true, ..chart(ChartKind::Bar, "region", None, AggFunc::Sum) }).unwrap();
    assert_eq!(bar.labels, s(&["north", "south", "east"]), "row counts, largest first");
    assert_eq!(bar.values, vec![Some(3.0), Some(2.0), Some(1.0)]);
    let line = ws.chart(id, &chart(ChartKind::Line, "day", Some("price"), AggFunc::Mean)).unwrap();
    assert_eq!(line.labels.first().map(String::as_str), Some("2024-01-01"));
    assert_eq!(line.labels.last().map(String::as_str), Some("NULL"));
    let sc = ws.chart(id, &chart(ChartKind::Scatter, "qty", Some("price"), AggFunc::Sum)).unwrap();
    assert_eq!(sc.xs.len(), 6);
    assert!(!sc.truncated);
    let h = ws.chart(id, &ChartSpec { bins: Some(5), ..chart(ChartKind::Histogram, "qty", None, AggFunc::Sum) }).unwrap();
    assert_eq!(h.labels, s(&["2", "3", "4", "5", "6"]));
    assert_eq!(h.values.iter().flatten().sum::<f64>(), 4.0, "four non-null values");
    assert_eq!(h.values.last(), Some(&Some(1.0)), "the maximum lands in the last bin");
    assert!(ws.chart(id, &chart(ChartKind::Histogram, "region", None, AggFunc::Sum)).is_err());
    assert!(ws.chart(id, &chart(ChartKind::Bar, "region", Some("qty"), AggFunc::List)).is_err());
}
