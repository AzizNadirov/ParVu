//! Column types detected when a CSV file opens.

use std::fmt::Write as _;
use std::path::Path;

use parvu_core::Workspace;
use parvu_core::ops::Operation;
use parvu_core::types::{DateOrder, DetectOptions, Reading, TypeFamily};
use tempfile::TempDir;

/// 25,000 rows; row 24,990 (past both ParVu's 10,000-row sample and DuckDB's
/// 20,480-row sniff) has a bad `qty`.
fn write_messy(path: &Path) {
    let mut s = String::from("zip,qty,eu,us,yn,dmy,amb,iso,ts,clock,payload,note,empty\n");
    for i in 0..25_000u32 {
        let qty = if i == 24_990 { "oops".to_string() } else { (i % 50).to_string() };
        let day = i % 28 + 1;
        let month = i % 12 + 1;
        writeln!(
            s,
            "{zip:05},{qty},\"{eu_int}.{eu_th:03},{c:02}\",\"{us_int},{eu_th:03}.5\",{yn},{d13:02}/{month:02}/2024,{amb_d:02}/{month:02}/2024,2024-{month:02}-{day:02},2024-{month:02}-{day:02}T10:{min:02}:00,{h:02}:{min:02},\"{{\"\"a\"\": {i}}}\",note {i},",
            zip = i % 100_000,
            eu_int = i % 9 + 1,
            eu_th = i % 1000,
            c = i % 100,
            us_int = i % 90 + 10,
            yn = if i % 2 == 0 { "yes" } else { "No" },
            // Always above 12 somewhere in the sample, so day-first is certain.
            d13 = i % 28 + 1,
            // Never above 12, and not equal to the month: both orders read
            // every value, and give different dates.
            amb_d = (i + 3) % 12 + 1,
            min = i % 60,
            h = i % 24,
        )
        .unwrap();
    }
    std::fs::write(path, s).unwrap();
}

fn open(options: DetectOptions) -> (TempDir, Workspace, u64) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("messy.csv");
    write_messy(&path);
    let mut ws = Workspace::new(100, None).unwrap();
    ws.set_detect_options(options);
    let id = ws.open(&path).unwrap();
    (dir, ws, id)
}

fn dtypes(ws: &Workspace, id: u64) -> Vec<(String, String)> {
    ws.columns(id).unwrap().into_iter().map(|c| (c.name, c.dtype)).collect()
}

fn first(ws: &Workspace, id: u64, col: &str) -> Option<String> {
    let page = ws.page(id, 1).unwrap();
    let i = page.columns.iter().position(|c| c.name == col).unwrap();
    page.rows[0][i].clone()
}

#[test]
fn csv_columns_get_their_types() {
    let (_d, ws, id) = open(DetectOptions::default());
    let types = dtypes(&ws, id);
    let want = [
        ("zip", "VARCHAR"),
        ("qty", "BIGINT"),
        ("eu", "DOUBLE"),
        ("us", "DOUBLE"),
        ("yn", "BOOLEAN"),
        ("dmy", "DATE"),
        ("amb", "DATE"),
        ("iso", "DATE"),
        ("ts", "TIMESTAMP"),
        ("clock", "TIME"),
        ("payload", "JSON"),
        ("note", "VARCHAR"),
        ("empty", "VARCHAR"),
    ];
    let want: Vec<(String, String)> = want.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    assert_eq!(types, want);

    assert_eq!(first(&ws, id, "zip").as_deref(), Some("00000"), "leading zeros stay");
    assert_eq!(first(&ws, id, "eu").as_deref(), Some("1000.0"), "1.000,00");
    assert_eq!(first(&ws, id, "us").as_deref(), Some("10000.5"), "10,000.5");
    assert_eq!(first(&ws, id, "yn").as_deref(), Some("true"));
    assert_eq!(first(&ws, id, "dmy").as_deref(), Some("2024-01-01"));

    let cols = ws.columns(id).unwrap();
    assert_eq!(cols.iter().find(|c| c.name == "payload").unwrap().family, TypeFamily::Json);

    let source = ws.source_columns(id).unwrap();
    let amb = source.iter().find(|c| c.name == "amb").unwrap();
    assert!(amb.ambiguous);
    assert_eq!(amb.reading, Reading::DateFormat { format: "%d/%m/%Y".into() });
    assert!(!source.iter().find(|c| c.name == "dmy").unwrap().ambiguous);
}

#[test]
fn a_bad_value_past_the_sample_is_empty_not_an_error() {
    let (_d, mut ws, id) = open(DetectOptions::default());
    assert_eq!(ws.summary(id).unwrap().total_rows, 25_000);
    let qty = |ws: &Workspace| ws.source_columns(id).unwrap().iter().find(|c| c.name == "qty").unwrap().clone();
    assert!(qty(&ws).native, "DuckDB's fast typed reader until a value fails");

    // Reading every value of qty meets "oops" at row 24,990: DuckDB fails
    // naming the column, and healing re-reads just that column as text.
    let sum = |ws: &mut Workspace| {
        Ok(ws.connection().query_row("SELECT count(*) FILTER (WHERE qty IS NULL), max(qty) FROM messy", [], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?)
    };
    assert!(sum(&mut ws).is_err(), "without healing the query fails");
    assert_eq!(ws.healing(sum).unwrap(), (1, 49));
    assert!(!qty(&ws).native);
    assert_eq!(qty(&ws).reading, Reading::Cast { dtype: "BIGINT".into() });
    assert_eq!(ws.columns(id).unwrap()[1].dtype, "BIGINT", "the type stays");

    ws.apply(id, &Operation::Sort { column: "qty".into(), ascending: true }).unwrap();
    let last = ws.summary(id).unwrap().total_pages;
    ws.page(id, last).unwrap();
}

#[test]
fn try_cast_over_csv_text_never_fails() {
    // DuckDB's type_pushdown rule turned this TRY_CAST into a strict parse
    // inside the CSV reader; the workspace switches the rule off, on every
    // connection.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("codes.csv");
    let mut text = String::from("id,code\n");
    for i in 0..500 {
        writeln!(text, "{i},{i}").unwrap();
    }
    text.push_str("500,oops\n");
    std::fs::write(&path, text).unwrap();
    let mut ws = Workspace::new(100, None).unwrap();
    let id = ws.open(&path).unwrap();
    ws.apply(id, &Operation::ChangeType { column: "code".into(), new_type: "INTEGER".into() }).unwrap();
    let through_tab = format!("SELECT max(code), count(code) FROM ({})", ws.current_query(id).unwrap());
    // The shape that triggered the rule: a TRY_CAST in a subquery over read_csv.
    let direct = format!(
        "SELECT max(code), count(code) FROM (SELECT TRY_CAST(code AS INTEGER) AS code, id FROM read_csv('{}'))",
        path.display()
    );
    for sql in [&through_tab, &direct] {
        let read = |c: &parvu_core::duckdb::Connection| {
            c.query_row(sql, [], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).unwrap()
        };
        assert_eq!(read(ws.connection()), (499, 500));
        assert_eq!(read(&ws.clone_connection().unwrap()), (499, 500));
    }
}

#[test]
fn ambiguous_dates_follow_the_date_order() {
    let (_d, ws, id) = open(DetectOptions { enabled: true, date_order: DateOrder::MonthFirst });
    let amb = ws.source_columns(id).unwrap().iter().find(|c| c.name == "amb").unwrap().clone();
    assert_eq!(amb.reading, Reading::DateFormat { format: "%m/%d/%Y".into() });
    // Day above 12: no choice, whatever the order.
    let dmy = ws.source_columns(id).unwrap().iter().find(|c| c.name == "dmy").unwrap().clone();
    assert_eq!(dmy.reading, Reading::DateFormat { format: "%d/%m/%Y".into() });
}

#[test]
fn detection_off_keeps_text() {
    let (_d, ws, id) = open(DetectOptions { enabled: false, date_order: DateOrder::DayFirst });
    assert!(dtypes(&ws, id).iter().all(|(_, t)| t == "VARCHAR"));
}

/// Opening speed on a large CSV: `cargo test -p parvu-core --test types --
/// --ignored --nocapture`. The file goes under `target/` (on disk), not the
/// system temp folder, which may live in RAM.
#[test]
#[ignore]
fn open_speed() {
    use std::time::Instant;
    let rows: u64 = std::env::var("PARVU_BENCH_ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(2_000_000);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("parvu-bench");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("bench_{rows}.csv"));
    if !path.exists() {
        let ws = Workspace::new(100, None).unwrap();
        ws.connection()
            .execute_batch(&format!(
                "COPY (SELECT i AS id, (i * 7919 % 100000) / 100.0 AS amount, DATE '2020-01-01' + CAST(i % 2000 AS INTEGER) AS day, \
                 strftime(DATE '2020-01-01' + CAST(i % 2000 AS INTEGER), '%d.%m.%Y') AS dmy, 'name_' || (i % 5000) AS name, \
                 CASE WHEN i % 3 = 0 THEN 'yes' ELSE 'no' END AS yn FROM range({rows}) t(i)) TO '{}' (HEADER)",
                path.display()
            ))
            .unwrap();
    }
    let size = std::fs::metadata(&path).unwrap().len() / 1_000_000;
    for (label, enabled) in [("detect", true), ("text only", false)] {
        let mut ws = Workspace::new(100, None).unwrap();
        ws.connection().execute_batch("SET threads = 2; SET memory_limit = '1GB'").unwrap();
        ws.set_detect_options(DetectOptions { enabled, date_order: DateOrder::DayFirst });
        let t = Instant::now();
        let id = ws.open(&path).unwrap();
        let open = t.elapsed();
        if enabled {
            for c in ws.source_columns(id).unwrap() {
                println!("  {:8} native={} {:?}", c.name, c.native, c.reading);
            }
            let t = Instant::now();
            parvu_core::types::detect(ws.connection(), &path.to_string_lossy(), DetectOptions::default(), &Default::default()).unwrap();
            println!("  detection alone: {:?}", t.elapsed());
        }
        let t = Instant::now();
        ws.apply(id, &Operation::Sort { column: "amount".into(), ascending: false }).unwrap();
        ws.page(id, 1).unwrap();
        println!("{label:10} {rows} rows / {size} MB: open {open:?}, sort + first page {:?}", t.elapsed());
    }
    // DuckDB's own typed reader, as ParVu opened CSV files before.
    let ws = Workspace::new(100, None).unwrap();
    let c = ws.connection();
    c.execute_batch("SET threads = 2; SET memory_limit = '1GB'").unwrap();
    let t = Instant::now();
    c.execute_batch(&format!("CREATE VIEW v AS SELECT * FROM read_csv('{}')", path.display())).unwrap();
    let n: i64 = c.query_row("SELECT count(*) FROM v", [], |r| r.get(0)).unwrap();
    c.execute_batch("SELECT * FROM v LIMIT 100").unwrap();
    let open = t.elapsed();
    // What applying a sort costs in ParVu: validate, count, first page.
    let t = Instant::now();
    c.execute_batch("SELECT * FROM v ORDER BY amount DESC LIMIT 0").unwrap();
    let _: i64 = c.query_row("SELECT count(*) FROM (SELECT * FROM v ORDER BY amount DESC)", [], |r| r.get(0)).unwrap();
    c.execute_batch("SELECT * FROM v ORDER BY amount DESC LIMIT 100").unwrap();
    println!("{:10} {n} rows / {size} MB: open {open:?}, sort + first page {:?}", "duckdb", t.elapsed());
}

#[test]
fn a_column_can_be_read_another_way_and_undone() {
    let (_d, mut ws, id) = open(DetectOptions::default());
    // Row 1 is "04/01/2024": 4 January day first, 1 April month first.
    assert_eq!(first(&ws, id, "amb").as_deref(), Some("2024-01-04"));
    ws.set_source_reading(id, "amb", Reading::DateFormat { format: "%m/%d/%Y".into() }).unwrap();
    assert_eq!(first(&ws, id, "amb").as_deref(), Some("2024-04-01"));
    let amb = ws.source_columns(id).unwrap().iter().find(|c| c.name == "amb").unwrap().clone();
    assert!(!amb.ambiguous && !amb.native);
    assert!(ws.summary(id).unwrap().is_base_query, "no transform: cells stay editable");
    ws.edit_cell(id, 0, "note", Some("edited".into())).unwrap();

    ws.undo(id).unwrap(); // the cell edit
    ws.undo(id).unwrap(); // the reading
    assert_eq!(first(&ws, id, "amb").as_deref(), Some("2024-01-04"));
    let amb = ws.source_columns(id).unwrap().iter().find(|c| c.name == "amb").unwrap().clone();
    assert_eq!(amb.reading, Reading::DateFormat { format: "%d/%m/%Y".into() });
    assert!(amb.ambiguous, "undo restores the detected reading as it was");

    // Text where a later step multiplies the column: refused, nothing changes.
    ws.apply(id, &Operation::AddColumn { name: "double".into(), expression: "\"messy\".\"qty\" * 2".into() }).unwrap();
    assert!(ws.set_source_reading(id, "qty", Reading::Text).is_err());
    assert_eq!(dtypes(&ws, id)[1], ("qty".to_string(), "BIGINT".to_string()));
    assert!(ws.page(id, 1).is_ok());
}

#[test]
fn readings_chosen_before_apply_without_a_step() {
    let (_d, mut ws, id) = open(DetectOptions::default());
    ws.apply_source_readings(id, &[("zip".into(), Reading::Integer), ("missing".into(), Reading::Text)]).unwrap();
    assert_eq!(dtypes(&ws, id)[0], ("zip".to_string(), "BIGINT".to_string()));
    assert!(ws.summary(id).unwrap().is_base_query);
    assert!(!ws.has_unsaved_changes(id).unwrap());
}
