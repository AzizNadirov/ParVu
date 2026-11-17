//! Every DuckDB connection ParVu opens, so that one call can stop all
//! running statements (the app's memory guard uses it).

use std::sync::{Arc, Mutex, Weak};

use duckdb::{Connection, InterruptHandle};

/// A set of connections that can be interrupted together.
#[derive(Default)]
pub struct Interrupts {
    handles: Mutex<Vec<Weak<InterruptHandle>>>,
}

impl Interrupts {
    pub const fn new() -> Self {
        Self { handles: Mutex::new(Vec::new()) }
    }

    /// Remember `conn`. Closed connections drop out on their own.
    pub fn register(&self, conn: &Connection) {
        let mut list = self.handles.lock().unwrap_or_else(|p| p.into_inner());
        list.retain(|w| w.strong_count() > 0);
        list.push(Arc::downgrade(&conn.interrupt_handle()));
    }

    /// Interrupt the running statement of every open connection; an idle
    /// connection ignores it. Returns how many connections were reached.
    pub fn interrupt_all(&self) -> usize {
        let live: Vec<Arc<InterruptHandle>> = {
            let mut list = self.handles.lock().unwrap_or_else(|p| p.into_inner());
            list.retain(|w| w.strong_count() > 0);
            list.iter().filter_map(Weak::upgrade).collect()
        };
        for h in &live {
            h.interrupt();
        }
        live.len()
    }
}

/// Every connection of this process.
static ALL: Interrupts = Interrupts::new();

/// Remember `conn` so [`interrupt_all`] reaches it.
pub fn register(conn: &Connection) {
    ALL.register(conn);
}

/// Interrupt the running statement of every connection of this process.
pub fn interrupt_all() -> usize {
    ALL.interrupt_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stops_running_queries_and_forgets_closed_connections() {
        let set = Interrupts::new();
        let idle = Connection::open_in_memory().unwrap();
        let busy = idle.try_clone().unwrap();
        set.register(&idle);
        set.register(&busy);
        let t = std::thread::spawn(move || {
            busy.query_row("SELECT count(*) FROM range(10000000000) a, range(1000) b", [], |r| r.get::<_, i64>(0))
        });
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(set.interrupt_all(), 2);
        for _ in 0..100 {
            if t.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
            set.interrupt_all();
        }
        let err = t.join().unwrap().unwrap_err().to_string();
        assert!(err.contains("nterrupt"), "{err}");
        // The query's connection is gone; the idle one still works.
        assert_eq!(set.interrupt_all(), 1);
        assert_eq!(idle.query_row("SELECT 1", [], |r| r.get::<_, i32>(0)).unwrap(), 1);
    }
}
