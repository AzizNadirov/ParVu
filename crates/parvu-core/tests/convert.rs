//! Converting columns: Convert, ConversionErrors, previews and error counts.

use std::fmt::Write as _;

use parvu_core::Workspace;
use parvu_core::ops::Operation;
use parvu_core::types::{EpochUnit, Reading};
use parvu_core::workspace::conversion_errors;
use tempfile::TempDir;

/// 1,000 rows: `amount` is European-style text with one "n/a" (row 500), so
/// it stays text; `day` is dd.mm.yyyy; `ms` is Unix time in milliseconds.
fn open() -> (TempDir, Workspace, u64) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sales.csv");
    let mut s = String::from("id,amount,day,ms\n");
    for i in 0..1000u32 {
        let amount = if i == 500 { "n/a".to_string() } else { format!("\"{}.{:03},{:02}\"", i % 9 + 1, i % 1000, i % 100) };
        writeln!(s, "{i},{amount},{:02}.{:02}.2024,{}", i % 28 + 1, i % 12 + 1, 1_700_000_000_000u64 + u64::from(i) * 1000).unwrap();
    }
    std::fs::write(&path, s).unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

fn eu() -> Reading {
    Reading::LocaleNumber { thousands: ".".into(), decimal: ",".into() }
}

fn col(ws: &Workspace, id: u64, name: &str) -> Vec<Option<String>> {
    let names: Vec<String> = ws.columns(id).unwrap().into_iter().map(|c| c.name).collect();
    let i = names.iter().position(|n| n == name).unwrap();
    (1..=ws.summary(id).unwrap().total_pages).flat_map(|p| ws.page(id, p).unwrap().rows.into_iter().map(move |r| r[i].clone())).collect()
}

#[test]
fn converts_with_a_format_and_counts_what_failed() {
    let (_d, mut ws, id) = open();
    assert_eq!(ws.columns(id).unwrap()[1].dtype, "VARCHAR", "n/a keeps it text");

    // Preview: first rows, nothing failed among them.
    let preview = ws.preview_conversion(id, "amount", &eu(), 3).unwrap();
    assert_eq!(preview[0], (Some("1.000,00".into()), Some("1000.0".into()), false));

    // Error counts: the first 100 rows have none; all rows have one.
    let sample = ws.conversion_error_query(id, "amount", &eu(), Some(100)).unwrap();
    assert_eq!(conversion_errors(ws.connection(), &sample).unwrap(), (0, 100));
    let all = ws.conversion_error_query(id, "amount", &eu(), None).unwrap();
    assert_eq!(conversion_errors(ws.connection(), &all).unwrap(), (1, 1000));

    ws.apply(id, &Operation::Convert { column: "amount".into(), to: eu(), on_error: None, keep_original: true }).unwrap();
    let cols: Vec<(String, String)> = ws.columns(id).unwrap().into_iter().map(|c| (c.name, c.dtype)).collect();
    assert_eq!(cols[1], ("amount".to_string(), "DOUBLE".to_string()));
    assert_eq!(cols[2], ("amount_raw".to_string(), "VARCHAR".to_string()), "original kept right after");
    let amounts = col(&ws, id, "amount");
    assert_eq!(amounts[0].as_deref(), Some("1000.0"));
    assert_eq!(amounts[500], None, "n/a becomes empty");
    ws.undo(id).unwrap();

    // A value for the cells that don't convert.
    ws.apply(id, &Operation::Convert { column: "amount".into(), to: eu(), on_error: Some("0".into()), keep_original: false })
        .unwrap();
    assert_eq!(col(&ws, id, "amount")[500].as_deref(), Some("0.0"));
    ws.undo(id).unwrap();
    // ... which has to be valid in the new type.
    let bad = Operation::Convert { column: "amount".into(), to: eu(), on_error: Some("zero".into()), keep_original: false };
    assert!(ws.apply(id, &bad).is_err());
    assert_eq!(ws.summary(id).unwrap().steps.len(), 0);
}

#[test]
fn keeps_or_removes_the_rows_that_dont_convert() {
    let (_d, mut ws, id) = open();
    ws.apply(id, &Operation::ConversionErrors { column: "amount".into(), to: eu(), keep: true }).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 1);
    assert_eq!(col(&ws, id, "amount")[0].as_deref(), Some("n/a"));
    ws.undo(id).unwrap();
    ws.apply(id, &Operation::ConversionErrors { column: "amount".into(), to: eu(), keep: false }).unwrap();
    assert_eq!(ws.summary(id).unwrap().total_rows, 999);
}

#[test]
fn unix_time_and_suggestions() {
    let (_d, mut ws, id) = open();
    ws.apply(
        id,
        &Operation::Convert {
            column: "ms".into(),
            to: Reading::Epoch { unit: EpochUnit::Milliseconds },
            on_error: None,
            keep_original: false,
        },
    )
    .unwrap();
    assert_eq!(col(&ws, id, "ms")[0].as_deref(), Some("2023-11-14 22:13:20"));

    // The dialog's suggestion comes from the column's values as text.
    // One "n/a" in 1,000: no reading fits every value, but 1.234,56 fits
    // nearly all of them, so it is still the suggestion.
    assert_eq!(ws.suggest_reading(id, "amount").unwrap(), eu());
    ws.apply(id, &Operation::ConversionErrors { column: "amount".into(), to: eu(), keep: false }).unwrap();
    assert_eq!(ws.suggest_reading(id, "amount").unwrap(), eu(), "without n/a: every value fits");
    // Plain text stays text.
    ws.apply(id, &Operation::AddColumn { name: "word".into(), expression: "'w' || \"sales\".\"id\"".into() }).unwrap();
    assert_eq!(ws.suggest_reading(id, "word").unwrap(), Reading::Text);
    ws.apply(id, &Operation::ChangeType { column: "day".into(), new_type: "VARCHAR".into() }).unwrap();
    assert_eq!(ws.suggest_reading(id, "day").unwrap(), Reading::Date, "dates as ISO text");
}

#[test]
fn formats_and_types_cannot_inject_sql() {
    let (_d, mut ws, id) = open();
    let sneaky = Reading::DateFormat { format: "%d'); DROP TABLE x; --".into() };
    ws.apply(id, &Operation::Convert { column: "amount".into(), to: sneaky, on_error: None, keep_original: false }).unwrap();
    assert!(col(&ws, id, "amount").iter().all(Option::is_none), "the format is just a format");
    let bad_type = Reading::Cast { dtype: "INTEGER); DROP TABLE x; --".into() };
    assert!(ws.apply(id, &Operation::Convert { column: "id".into(), to: bad_type, on_error: None, keep_original: false }).is_err());
}
