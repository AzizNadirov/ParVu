//! Long work piece by piece: unique values stop early, stay small, and stop
//! promptly when cancelled. Every source here is a lazy `range()`: no files,
//! next to no memory.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parvu_core::CoreError;
use parvu_core::duckdb::Connection;
use parvu_core::job::Cancel;
use parvu_core::stats::{UNIQUE_LIMIT, UniqueStop, column_stats, compare_values, unique_values};

fn conn() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("SET threads = 2; SET memory_limit = '256MB'").unwrap();
    c
}

#[test]
fn many_values_stop_early() {
    let c = conn();
    let u = unique_values(&c, "SELECT i FROM range(1000000) t(i)", "i", &Cancel::new(), &mut |_, _| {}).unwrap();
    assert!(!u.complete);
    assert_eq!(u.stopped, Some(UniqueStop::TooMany));
    assert_eq!(u.values.len(), UNIQUE_LIMIT);
    assert!(u.rows_scanned < 1_000_000, "read {} rows", u.rows_scanned);
}

#[test]
fn few_values_read_every_row_and_sort_numbers_by_value() {
    let c = conn();
    let sql = "SELECT CASE WHEN i = 5 THEN NULL ELSE i % 11 END AS v FROM range(300000) t(i)";
    let mut reports = 0;
    let u = unique_values(&c, sql, "v", &Cancel::new(), &mut |_, _| reports += 1).unwrap();
    assert!(u.complete);
    assert_eq!(u.rows_scanned, 300_000);
    let shown: Vec<_> = u.values.iter().map(|v| v.as_deref().unwrap_or("NULL")).collect();
    assert_eq!(shown, ["NULL", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]);
    let _ = reports;
}

#[test]
fn long_texts_stop_at_the_size_cap() {
    let c = conn();
    let sql = "SELECT repeat('x', 100000) || i AS v FROM range(1000) t(i)";
    let u = unique_values(&c, sql, "v", &Cancel::new(), &mut |_, _| {}).unwrap();
    assert_eq!(u.stopped, Some(UniqueStop::TooLarge));
    assert!(u.values.len() < 100);
}

#[test]
fn cancelled_before_start_returns_at_once() {
    let c = conn();
    let cancel = Cancel::new();
    cancel.cancel();
    let r = unique_values(&c, "SELECT i FROM range(10) t(i)", "i", &cancel, &mut |_, _| {});
    assert!(matches!(r, Err(CoreError::Cancelled)));
}

/// A scan of two billion rows with few values would take minutes; cancel
/// plus interrupt must end it within a couple of seconds.
#[test]
fn cancelled_scan_returns_promptly() {
    let c = conn();
    let interrupt = c.interrupt_handle();
    let cancel = Cancel::new();
    let flag = cancel.clone();
    let worker = std::thread::spawn(move || {
        unique_values(&c, "SELECT (i % 100) AS v FROM range(2000000000) t(i)", "v", &flag, &mut |_, _| {})
    });
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    cancel.cancel();
    interrupt.interrupt();
    let r = worker.join().unwrap();
    assert!(t.elapsed() < Duration::from_secs(2), "took {:?}", t.elapsed());
    assert!(r.is_err(), "a cancelled scan must not look complete");
}

/// The interrupt handle stops a blocking aggregate on another thread.
#[test]
fn interrupt_stops_a_running_aggregate() {
    let c = conn();
    let interrupt: Arc<_> = c.interrupt_handle();
    let worker = std::thread::spawn(move || {
        c.query_row("SELECT COUNT(DISTINCT i % 1000) FROM range(3000000000) t(i)", [], |r| r.get::<_, i64>(0))
    });
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    interrupt.interrupt();
    let r = worker.join().unwrap();
    assert!(t.elapsed() < Duration::from_secs(2), "took {:?}", t.elapsed());
    assert!(r.is_err());
}

#[test]
fn values_sort_null_numbers_text() {
    let mut v = vec![Some("b"), Some("10"), None, Some("9"), Some("a")];
    v.sort_by(|a, b| compare_values(*a, *b));
    assert_eq!(v, [None, Some("9"), Some("10"), Some("a"), Some("b")]);
}

#[test]
fn big_tables_get_an_estimated_distinct_count() {
    let c = conn();
    let sql = "SELECT (i % 5000)::VARCHAR AS v FROM range(1000000) t(i)";
    let exact = column_stats(&c, sql, "v", "VARCHAR", false).unwrap();
    let approx = column_stats(&c, sql, "v", "VARCHAR", true).unwrap();
    assert_eq!((exact.distinct, exact.distinct_approx), (5000, false));
    assert!(approx.distinct_approx);
    assert!((4500..5500).contains(&approx.distinct), "{}", approx.distinct);
}
