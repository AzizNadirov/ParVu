//! Times opening a file and switching pages, for comparing against the
//! Python app (plans/RUST_REWRITE_PLAN.md §9).
//!
//!     cargo run --release -p parvu-core --example page_bench -- FILE [PAGE_SIZE]

use std::time::Instant;

use parvu_core::Workspace;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: page_bench FILE [PAGE_SIZE]");
    let page_size: u64 = args.next().map(|s| s.parse().expect("page size")).unwrap_or(1000);

    let t = Instant::now();
    let mut ws = Workspace::new(page_size, None).expect("workspace");
    let id = ws.open(std::path::Path::new(&path)).expect("open");
    let first = ws.page(id, 1).expect("page");
    println!("open + first page: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);

    let pages = ws.summary(id).expect("summary").total_pages;
    let targets: Vec<u64> = (0..20).map(|i| 1 + (i * 7919) % pages).collect();
    let t = Instant::now();
    for p in &targets {
        let page = ws.page(id, *p).expect("page");
        assert_eq!(page.columns.len(), first.columns.len());
    }
    let per = t.elapsed().as_secs_f64() * 1000.0 / targets.len() as f64;
    println!("page switch ({page_size} rows x {} cols): {per:.1} ms", first.columns.len());
}
