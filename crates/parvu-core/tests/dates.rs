//! Date functions, differences between dates and relative date filters.

use parvu_core::Workspace;
use parvu_core::colfn::{ColumnFn, DatePart, Output, RelativeRange, TimeUnit};
use parvu_core::ops::Operation;
use tempfile::TempDir;

fn open() -> (TempDir, Workspace, u64) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("events.csv");
    std::fs::write(
        &path,
        "day,at,born,ordered,shipped\n\
         2024-05-18,2024-05-18 14:35:20,2000-06-15,2024-01-31,2024-03-01\n\
         2024-12-31,2024-12-31 23:59:59,1990-01-01,2024-02-29,2024-02-29\n",
    )
    .unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

fn col(ws: &Workspace, id: u64, name: &str) -> Vec<Option<String>> {
    let page = ws.page(id, 1).unwrap();
    let i = page.columns.iter().position(|c| c.name == name).unwrap_or_else(|| panic!("no column {name}"));
    page.rows.into_iter().map(|r| r[i].clone()).collect()
}

fn new(ws: &mut Workspace, id: u64, column: &str, func: ColumnFn) -> String {
    ws.apply(id, &Operation::ColumnTransform { column: column.into(), func, output: Output::New { name: String::new() } })
        .unwrap();
    ws.columns(id).unwrap().into_iter().map(|c| c.name).find(|n| n.starts_with(&format!("{column}_"))).unwrap()
}

fn s(v: &[&str]) -> Vec<Option<String>> {
    v.iter().map(|x| Some(x.to_string())).collect()
}

#[test]
fn parts_and_names() {
    let (_d, mut ws, id) = open();
    let part = |p| ColumnFn::DatePart { part: p };
    let c = new(&mut ws, id, "day", part(DatePart::Quarter));
    assert_eq!(col(&ws, id, &c), s(&["2", "4"]));
    ws.undo(id).unwrap();
    let c = new(&mut ws, id, "day", part(DatePart::DayOfWeek));
    assert_eq!(col(&ws, id, &c), s(&["6", "2"]), "Saturday = 6, Tuesday = 2");
    ws.undo(id).unwrap();
    let c = new(&mut ws, id, "at", part(DatePart::Hour));
    assert_eq!(col(&ws, id, &c), s(&["14", "23"]));
    ws.undo(id).unwrap();
    // Hours of a plain date make no sense.
    let hour_of_date = Operation::ColumnTransform { column: "day".into(), func: part(DatePart::Hour), output: Output::InPlace };
    assert!(ws.apply(id, &hour_of_date).is_err());

    // Names in the user's language.
    let months: Vec<String> = (1..=12).map(|m| format!("m{m}")).collect();
    let c = new(&mut ws, id, "day", ColumnFn::MonthName { names: months });
    assert_eq!(col(&ws, id, &c), s(&["m5", "m12"]));
    ws.undo(id).unwrap();
    let days: Vec<String> = ["B.e.", "Ç.a.", "Ç.", "C.a.", "C.", "Ş.", "B."].iter().map(|d| d.to_string()).collect();
    let c = new(&mut ws, id, "day", ColumnFn::DayName { names: days });
    assert_eq!(col(&ws, id, &c), s(&["Ş.", "Ç.a."]));
    ws.undo(id).unwrap();
    let c = new(&mut ws, id, "day", ColumnFn::MonthName { names: Vec::new() });
    assert_eq!(col(&ws, id, &c), s(&["May", "December"]), "English without names");
    ws.undo(id).unwrap();
    let c = new(&mut ws, id, "day", ColumnFn::IsWeekend);
    assert_eq!(col(&ws, id, &c), s(&["true", "false"]));
}

#[test]
fn start_end_shift_and_format() {
    let (_d, mut ws, id) = open();
    let in_place = |ws: &mut Workspace, column: &str, func: ColumnFn| {
        ws.apply(id, &Operation::ColumnTransform { column: column.into(), func, output: Output::InPlace }).unwrap();
    };
    in_place(&mut ws, "day", ColumnFn::StartOf { unit: TimeUnit::Quarter });
    assert_eq!(col(&ws, id, "day"), s(&["2024-04-01", "2024-10-01"]));
    assert_eq!(ws.columns(id).unwrap()[0].dtype, "DATE", "a date column stays a date");
    ws.undo(id).unwrap();
    in_place(&mut ws, "day", ColumnFn::StartOf { unit: TimeUnit::Week });
    assert_eq!(col(&ws, id, "day")[0].as_deref(), Some("2024-05-13"), "weeks start on Monday");
    ws.undo(id).unwrap();
    in_place(&mut ws, "ordered", ColumnFn::EndOf { unit: TimeUnit::Month });
    assert_eq!(col(&ws, id, "ordered"), s(&["2024-01-31", "2024-02-29"]));
    ws.undo(id).unwrap();
    in_place(&mut ws, "day", ColumnFn::EndOf { unit: TimeUnit::Quarter });
    assert_eq!(col(&ws, id, "day"), s(&["2024-06-30", "2024-12-31"]));
    ws.undo(id).unwrap();
    in_place(&mut ws, "ordered", ColumnFn::AddTime { n: 1, unit: TimeUnit::Month });
    assert_eq!(col(&ws, id, "ordered"), s(&["2024-02-29", "2024-03-29"]), "month ends are clamped");
    ws.undo(id).unwrap();
    in_place(&mut ws, "at", ColumnFn::StartOf { unit: TimeUnit::Hour });
    assert_eq!(col(&ws, id, "at")[0].as_deref(), Some("2024-05-18 14:00:00"));
    ws.undo(id).unwrap();
    in_place(&mut ws, "at", ColumnFn::TimeOnly);
    assert_eq!(col(&ws, id, "at")[0].as_deref(), Some("14:35:20"));
    ws.undo(id).unwrap();
    let c = new(&mut ws, id, "day", ColumnFn::FormatDate { format: "%d.%m.%Y".into() });
    assert_eq!(col(&ws, id, &c), s(&["18.05.2024", "31.12.2024"]));
}

#[test]
fn ages_and_differences() {
    let (_d, mut ws, id) = open();
    let c = new(&mut ws, id, "born", ColumnFn::Age { unit: TimeUnit::Year });
    let ages: Vec<i64> = col(&ws, id, &c).into_iter().map(|v| v.unwrap().parse().unwrap()).collect();
    let expected: Vec<i64> = ["2000-06-15", "1990-01-01"]
        .iter()
        .map(|b| {
            ws.connection().query_row(&format!("SELECT datesub('year', DATE '{b}', current_date)"), [], |r| r.get(0)).unwrap()
        })
        .collect();
    assert_eq!(ages, expected);

    let diff = |unit| Operation::DateDiff { start: "ordered".into(), end: "shipped".into(), unit, name: "gap".into() };
    ws.apply(id, &diff(TimeUnit::Day)).unwrap();
    let names: Vec<String> = ws.columns(id).unwrap().into_iter().map(|c| c.name).collect();
    assert_eq!(names[names.iter().position(|n| n == "shipped").unwrap() + 1], "gap", "right after the end column");
    assert_eq!(col(&ws, id, "gap"), s(&["30", "0"]));
    ws.undo(id).unwrap();
    ws.apply(id, &diff(TimeUnit::Month)).unwrap();
    assert_eq!(col(&ws, id, "gap"), s(&["1", "0"]), "31 Jan → 1 Mar is one whole month");
    let text = Operation::DateDiff { start: "day".into(), end: "born".into(), unit: TimeUnit::Day, name: "x".into() };
    assert!(ws.apply(id, &text).is_ok());
}

#[test]
fn relative_filters() {
    // Dates around today, written by DuckDB so "today" is the same clock.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("recent.csv");
    let ws0 = Workspace::new(100, None).unwrap();
    ws0.connection()
        .execute_batch(&format!(
            "COPY (SELECT i, current_date - CAST(i AS INTEGER) AS d FROM range(0, 800) t(i)) TO '{}' (HEADER)",
            path.display()
        ))
        .unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    let rows = |ws: &mut Workspace, range: RelativeRange| {
        ws.apply(id, &Operation::DateRange { column: "d".into(), range }).unwrap();
        let n = ws.summary(id).unwrap().total_rows;
        ws.undo(id).unwrap();
        n
    };
    assert_eq!(rows(&mut ws, RelativeRange::Last { n: 7, unit: TimeUnit::Day }), 7, "today and the 6 days before");
    let in_sql = |ws: &Workspace, sql: &str| -> u64 {
        ws.connection().query_row(&format!("SELECT count(*) FROM range(0, 800) t(i) WHERE {sql}"), [], |r| r.get(0)).unwrap()
    };
    let d = "(current_date - CAST(i AS INTEGER))";
    let this_month = in_sql(&ws, &format!("date_trunc('month', {d}) = date_trunc('month', current_date)"));
    assert_eq!(rows(&mut ws, RelativeRange::This { unit: TimeUnit::Month }), this_month);
    let last_year = in_sql(&ws, &format!("year({d}) = year(current_date) - 1"));
    assert_eq!(rows(&mut ws, RelativeRange::Previous { unit: TimeUnit::Year }), last_year);
    let ytd = in_sql(&ws, &format!("year({d}) = year(current_date)"));
    assert_eq!(rows(&mut ws, RelativeRange::YearToDate), ytd);
    assert!(ws.apply(id, &Operation::DateRange { column: "i".into(), range: RelativeRange::YearToDate }).is_err());
}
