//! Memory use of a typical session: open, page, sort, filter, statistics.
//!
//!     cargo run --release -p parvu-core --example memory_bench -- FILE ["SET ...; SET ..."]
//!
//! The optional second argument is SQL run on the connection first, for
//! comparing DuckDB settings.

use std::time::Instant;

use parvu_core::Workspace;
use parvu_core::ops::Operation;

fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).map(str::to_string))
        .and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
        .unwrap_or(0.0)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: memory_bench FILE [SQL]");
    let setup = args.next();

    let mut ws = Workspace::new(500, None).expect("workspace");
    if let Some(sql) = &setup {
        ws.connection().execute_batch(sql).expect("setup SQL");
    }
    let step = |name: &str, t: Instant| println!("{name:<28} {:>7.0} MB  {:>6.0} ms", rss_mb(), t.elapsed().as_secs_f64() * 1000.0);
    step("start", Instant::now());

    let t = Instant::now();
    let id = ws.open(std::path::Path::new(&path)).expect("open");
    ws.page(id, 1).expect("page");
    step("open + page 1", t);

    let t = Instant::now();
    let pages = ws.summary(id).unwrap().total_pages;
    for i in 0..20u64 {
        ws.page(id, 1 + (i * 7919) % pages).unwrap();
    }
    step("20 random pages", t);

    let t = Instant::now();
    ws.apply(id, &Operation::Sort { column: "price".into(), ascending: false }).unwrap();
    ws.page(id, 1).unwrap();
    ws.page(id, pages / 2).unwrap();
    step("sort by price + 2 pages", t);

    let t = Instant::now();
    ws.apply(id, &Operation::FilterValues { column: "region".into(), values: vec![Some("East".into())] }).unwrap();
    ws.page(id, 3).unwrap();
    step("filter + page", t);

    let t = Instant::now();
    ws.column_stats(id, "price").unwrap();
    ws.unique_values(id, "product").unwrap();
    step("stats + unique values", t);

    let t = Instant::now();
    ws.reset(id).unwrap();
    ws.page(id, 1).unwrap();
    step("reset", t);

    let t = Instant::now();
    ws.close(id).unwrap();
    step("close tab", t);
}
