//! Long work outside the window's workspace lock: one mechanism for unique
//! values, statistics, charts, colour-scale ranges, search, samples and
//! full counts.
//!
//! A job runs on a cloned DuckDB connection, so the grid keeps paging while
//! it works. It reports progress as `job-progress` events to its window and
//! can be cancelled: the flag stops work between statements and batches, and
//! the connection's interrupt stops the running statement. A newer job in
//! the same slot of a window (say, the chart's next spec) cancels the older
//! one, so heavy queries don't pile up.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use parvu_core::duckdb::{Connection, InterruptHandle};
use parvu_core::job::Cancel;
use parvu_core::{CoreError, TabId, Workspace};
use serde::Serialize;
use tauri::{Emitter, State, WebviewWindow};

use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, lock};

/// How often progress is sent, and a cancelled statement re-interrupted.
const TICK: Duration = Duration::from_millis(250);

/// A CSV value deep in the file that doesn't parse is healed (the column is
/// read as text) and the work runs again, at most this many times.
const MAX_HEALS: usize = 8;

struct Job {
    tab: Option<TabId>,
    slot: &'static str,
    cancel: Cancel,
    interrupt: Arc<InterruptHandle>,
}

impl Job {
    fn stop(&self) {
        self.cancel.cancel();
        self.interrupt.interrupt();
    }
}

/// Running jobs by (window label, job id).
#[derive(Default)]
pub struct Jobs(Mutex<HashMap<(String, u64), Job>>);

impl Jobs {
    /// Add a job; an older job in the same slot of the window is cancelled.
    fn register(&self, window: &str, job: u64, j: Job) {
        let mut jobs = lock(&self.0);
        for ((w, _), other) in jobs.iter() {
            if w == window && other.slot == j.slot {
                other.stop();
            }
        }
        jobs.insert((window.to_string(), job), j);
    }

    fn finish(&self, window: &str, job: u64) {
        lock(&self.0).remove(&(window.to_string(), job));
    }

    pub fn cancel(&self, window: &str, job: u64) {
        if let Some(j) = lock(&self.0).get(&(window.to_string(), job)) {
            j.stop();
        }
    }

    /// Cancel the jobs of a tab (it is closing).
    pub fn cancel_tab(&self, window: &str, tab: TabId) {
        for ((w, _), j) in lock(&self.0).iter() {
            if w == window && j.tab == Some(tab) {
                j.stop();
            }
        }
    }

    /// Cancel every job of a window (it is closing).
    pub fn cancel_window(&self, window: &str) {
        for ((w, _), j) in lock(&self.0).iter() {
            if w == window {
                j.stop();
            }
        }
    }
}

/// A `job-progress` event. `percent` comes from DuckDB's progress (when it
/// knows it), `rows` and `data` from the work itself. `restart` tells the
/// UI to drop partial data: the work starts over after a heal.
#[derive(Debug, Clone, Default, Serialize)]
pub struct JobProgress {
    pub job: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub restart: bool,
}

/// What the work of a job gets: its own connection, the cancel flag, and a
/// way to report rows done and partial results.
pub struct JobCtx<'a> {
    pub conn: &'a Connection,
    pub cancel: &'a Cancel,
    report: &'a (dyn Fn(JobProgress) + Sync),
    job: u64,
}

impl JobCtx<'_> {
    pub fn progress(&self, rows: Option<u64>, data: Option<serde_json::Value>) {
        (self.report)(JobProgress { job: self.job, rows, data, ..Default::default() });
    }
}

/// Run `work` as job `job` of the calling window.
///
/// `prepare` runs under the workspace lock and must be cheap (SQL text,
/// column types); `work` runs on a cloned connection without the lock. The
/// result is the command's result; a cancelled job ends with kind
/// `"cancelled"`.
pub async fn run_job<P, T>(
    state: State<'_, AppState>,
    window: WebviewWindow,
    job: u64,
    slot: &'static str,
    tab: Option<TabId>,
    prepare: impl FnOnce(&Workspace) -> parvu_core::Result<P>,
    work: impl Fn(&JobCtx, &P) -> parvu_core::Result<T> + Send + 'static,
) -> ApiResult<T>
where
    P: Send + 'static,
    T: Send + 'static,
{
    let ws_arc = crate::commands::ws_of(&state, &window)?;
    let (conn, input) = {
        let ws = lock(&ws_arc);
        (ws.clone_connection()?, prepare(&ws)?)
    };
    conn.set_query_progress_tracking(true).map_err(CoreError::from)?;
    let interrupt = conn.interrupt_handle();
    let cancel = Cancel::new();
    let label = window.label().to_string();
    state.jobs.register(&label, job, Job { tab, slot, cancel: cancel.clone(), interrupt: interrupt.clone() });

    let send = {
        let window = window.clone();
        let label = label.clone();
        move |p: JobProgress| {
            let _ = window.emit_to(label.as_str(), "job-progress", p);
        }
    };

    // Progress ticks, and a cancel that outlives one statement: an
    // interrupt sent between two statements of the work would be lost.
    let done = Arc::new(AtomicBool::new(false));
    {
        let (done, cancel, send) = (done.clone(), cancel.clone(), send.clone());
        let progress = conn.progress_handle();
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                if cancel.is_cancelled() {
                    interrupt.interrupt();
                } else {
                    if let Some(p) = progress.query_progress().percentage().filter(|p| *p >= 0.0) {
                        send(JobProgress { job, percent: Some(p), ..Default::default() });
                    }
                }
                std::thread::sleep(TICK);
            }
        });
    }

    let result = {
        let cancel = cancel.clone();
        let send = send.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let ctx = JobCtx { conn: &conn, cancel: &cancel, report: &send, job };
            let mut heals = 0;
            loop {
                match work(&ctx, &input) {
                    Err(e)
                        if !cancel.is_cancelled()
                            && heals < MAX_HEALS
                            && lock(&ws_arc).heal(&e.to_string()) =>
                    {
                        heals += 1;
                        send(JobProgress { job, restart: true, ..Default::default() });
                    }
                    other => return other,
                }
            }
        })
        .await
    };
    done.store(true, Ordering::Relaxed);
    state.jobs.finish(&label, job);
    match result {
        Err(e) => Err(ApiError::new("app", e.to_string())),
        Ok(_) if cancel.is_cancelled() => Err(CoreError::Cancelled.into()),
        Ok(r) => r.map_err(ApiError::from),
    }
}

/// Stop a job of the calling window.
#[tauri::command(rename_all = "snake_case")]
pub fn cancel_job(state: State<'_, AppState>, window: WebviewWindow, job: u64) {
    state.jobs.cancel(window.label(), job);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(slot: &'static str, tab: TabId) -> (Job, Cancel) {
        let conn = Connection::open_in_memory().unwrap();
        let cancel = Cancel::new();
        (Job { tab: Some(tab), slot, cancel: cancel.clone(), interrupt: conn.interrupt_handle() }, cancel)
    }

    #[test]
    fn a_newer_job_in_the_same_slot_cancels_the_older() {
        let jobs = Jobs::default();
        let (a, ca) = job("chart", 1);
        let (b, cb) = job("chart", 1);
        let (c, cc) = job("stats", 1);
        let (d, cd) = job("chart", 1);
        jobs.register("main", 1, a);
        jobs.register("main", 2, c);
        jobs.register("other", 3, d);
        jobs.register("main", 4, b);
        assert!(ca.is_cancelled());
        assert!(!cb.is_cancelled());
        assert!(!cc.is_cancelled(), "another slot");
        assert!(!cd.is_cancelled(), "another window");
    }

    #[test]
    fn finish_removes_only_its_own_job() {
        let jobs = Jobs::default();
        let (a, ca) = job("unique", 1);
        let (b, cb) = job("stats", 1);
        jobs.register("main", 1, a);
        jobs.register("main", 2, b);
        jobs.finish("main", 1);
        jobs.cancel("main", 1);
        assert!(!ca.is_cancelled(), "finished: nothing to cancel");
        jobs.cancel("main", 2);
        assert!(cb.is_cancelled());
    }

    #[test]
    fn closing_a_tab_or_window_cancels_its_jobs() {
        let jobs = Jobs::default();
        let (a, ca) = job("unique", 1);
        let (b, cb) = job("stats", 2);
        let (c, cc) = job("stats", 2);
        jobs.register("main", 1, a);
        jobs.register("main", 2, b);
        jobs.register("w2", 3, c);
        jobs.cancel_tab("main", 1);
        assert!(ca.is_cancelled() && !cb.is_cancelled());
        jobs.cancel_window("main");
        assert!(cb.is_cancelled() && !cc.is_cancelled());
    }
}
