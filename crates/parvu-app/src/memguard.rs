//! Memory guard: stops every running query before ParVu takes the whole
//! machine's memory.
//!
//! DuckDB's `memory_limit` does not bound every operator (some aggregates
//! and window functions grow past it), and a runaway query once made the
//! whole system run out of memory. A background thread watches the
//! process's resident memory; when it passes a ceiling and keeps growing,
//! the guard interrupts all DuckDB connections. The interrupted command then
//! fails with the `memory` error kind, which the UI shows as "This operation
//! needed more memory than allowed".

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MB: u64 = 1024 * 1024;
/// How often memory is checked.
const TICK: Duration = Duration::from_millis(200);
/// Growth between two checks that counts as "still growing".
const GROWTH: u64 = 4 * MB;
/// For this long after a stop, interrupted queries report the memory error.
const REPORT_WINDOW_MS: u64 = 10_000;
/// Share of the machine's memory the app may use at most (percent).
const MAX_SHARE: u64 = 65;
/// Above this share the guard stops queries even if memory is not growing.
const HARD_SHARE: u64 = 85;

/// When the guard last stopped queries (ms since the epoch, 0 = never).
static LAST_TRIP_MS: AtomicU64 = AtomicU64::new(0);

/// The memory ParVu may use before running queries are stopped: twice
/// DuckDB's limit plus 1 GB for the app itself, but never more than 65 % of
/// the machine's memory (and never below 512 MB).
pub fn ceiling(memory_limit_mb: u64, total: Option<u64>) -> u64 {
    let wanted = (memory_limit_mb.max(128) * 2 + 1024) * MB;
    let capped = match total {
        Some(t) => wanted.min(t / 100 * MAX_SHARE),
        None => wanted,
    };
    capped.max(512 * MB)
}

/// Whether to stop queries now. `rss` is the current resident memory,
/// `last` the previous reading.
pub fn should_stop(rss: u64, last: u64, ceiling: u64, total: Option<u64>) -> bool {
    if let Some(t) = total
        && rss > t / 100 * HARD_SHARE
    {
        return true;
    }
    rss > ceiling && rss > last.saturating_add(GROWTH)
}

/// Whether an interrupted query was stopped by the guard (recently).
pub fn tripped_recently() -> bool {
    let at = LAST_TRIP_MS.load(Ordering::Relaxed);
    at != 0 && now_ms().saturating_sub(at) < REPORT_WINDOW_MS
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Start the guard thread. `memory_limit_mb` gives DuckDB's current limit
/// (it can change in the settings); `on_stop` runs once per episode with
/// the resident memory in MB. `PARVU_MEMORY_GUARD_MB` overrides the ceiling;
/// `0` turns the guard off.
pub fn start(memory_limit_mb: impl Fn() -> u64 + Send + 'static, on_stop: impl Fn(u64) + Send + 'static) {
    let fixed = std::env::var("PARVU_MEMORY_GUARD_MB").ok().and_then(|v| v.trim().parse::<u64>().ok());
    if fixed == Some(0) {
        tracing::info!("memory guard: off (PARVU_MEMORY_GUARD_MB=0)");
        return;
    }
    if resident_bytes().is_none() {
        tracing::warn!("memory guard: process memory is not readable on this system; guard off");
        return;
    }
    prefer_to_be_killed();
    let total = total_memory();
    let spawned = std::thread::Builder::new().name("memory-guard".into()).spawn(move || {
        let mut last = 0u64;
        let mut in_episode = false;
        loop {
            std::thread::sleep(TICK);
            let Some(rss) = resident_bytes() else { continue };
            let ceiling = fixed.map(|m| m * MB).unwrap_or_else(|| ceiling(memory_limit_mb(), total));
            if should_stop(rss, last, ceiling, total) {
                LAST_TRIP_MS.store(now_ms(), Ordering::Relaxed);
                let reached = parvu_core::interrupts::interrupt_all();
                if !in_episode {
                    in_episode = true;
                    tracing::warn!(
                        "memory guard: {} MB in use, ceiling {} MB; stopped running queries on {reached} connections",
                        rss / MB,
                        ceiling / MB
                    );
                    on_stop(rss / MB);
                }
            } else if rss <= ceiling {
                in_episode = false;
            }
            last = rss;
        }
    });
    match spawned {
        Ok(_) => tracing::info!(
            "memory guard: on, ceiling {} MB",
            fixed.map(|m| m * MB).unwrap_or_else(|| ceiling(1024, total)) / MB
        ),
        Err(e) => tracing::error!("memory guard: {e}"),
    }
}

/// If the system runs out of memory anyway, the kernel should stop ParVu
/// rather than the desktop or another program (Linux only).
fn prefer_to_be_killed() {
    #[cfg(target_os = "linux")]
    if let Err(e) = std::fs::write("/proc/self/oom_score_adj", "300") {
        tracing::warn!("memory guard: oom_score_adj: {e}");
    }
}

/// Resident memory of this process, in bytes.
#[cfg(target_os = "linux")]
pub fn resident_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    kib_field(&status, "VmRSS:")
}

/// The memory ParVu may use, in bytes: the machine's, or less when the
/// process runs under a memory cap (a cgroup: systemd scopes, containers,
/// Flatpak). Without this the guard's ceiling could sit above the cap, and
/// the kernel would kill ParVu before the guard stepped in.
#[cfg(target_os = "linux")]
pub fn total_memory() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let ram = kib_field(&info, "MemTotal:")?;
    Some(cgroup_limit().map_or(ram, |cap| cap.min(ram)))
}

/// The tightest `memory.max` of this process's cgroup and its parents
/// (cgroup v2); `None` without a limit.
#[cfg(target_os = "linux")]
fn cgroup_limit() -> Option<u64> {
    let own = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let path = own.lines().find_map(|l| l.strip_prefix("0::"))?.trim().to_string();
    let mut dir = std::path::PathBuf::from("/sys/fs/cgroup").join(path.trim_start_matches('/'));
    let mut best: Option<u64> = None;
    loop {
        if let Ok(v) = std::fs::read_to_string(dir.join("memory.max"))
            && let Ok(n) = v.trim().parse::<u64>()
        {
            best = Some(best.map_or(n, |b| b.min(n)));
        }
        if !dir.pop() || !dir.starts_with("/sys/fs/cgroup") || dir.as_os_str() == "/sys/fs/cgroup" {
            break;
        }
    }
    best
}

#[cfg(target_os = "linux")]
fn kib_field(text: &str, name: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with(name))?;
    let kib: u64 = line[name.len()..].split_whitespace().next()?.parse().ok()?;
    Some(kib * 1024)
}

#[cfg(windows)]
pub fn resident_bytes() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    c.cb = size;
    // SAFETY: `c` is a properly sized, writable PROCESS_MEMORY_COUNTERS.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, size) };
    (ok != 0).then_some(c.WorkingSetSize as u64)
}

#[cfg(windows)]
pub fn total_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut s: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    s.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `s` is a properly sized, writable MEMORYSTATUSEX.
    let ok = unsafe { GlobalMemoryStatusEx(&mut s) };
    (ok != 0).then_some(s.ullTotalPhys)
}

#[cfg(target_os = "macos")]
pub fn resident_bytes() -> Option<u64> {
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    // SAFETY: `info` is a writable proc_taskinfo of `size` bytes.
    let n = unsafe {
        libc::proc_pidinfo(
            std::process::id() as libc::c_int,
            libc::PROC_PIDTASKINFO,
            0,
            (&mut info as *mut libc::proc_taskinfo).cast(),
            size,
        )
    };
    (n == size).then_some(info.pti_resident_size)
}

#[cfg(target_os = "macos")]
pub fn total_memory() -> Option<u64> {
    let mut v: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: the name is NUL-terminated and `v` is a writable u64.
    let r = unsafe {
        libc::sysctlbyname(c"hw.memsize".as_ptr(), (&mut v as *mut u64).cast(), &mut len, std::ptr::null_mut(), 0)
    };
    (r == 0).then_some(v)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn resident_bytes() -> Option<u64> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub fn total_memory() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * MB;

    #[test]
    fn ceiling_is_twice_the_limit_plus_a_gigabyte_capped_by_the_machine() {
        assert_eq!(ceiling(1024, Some(64 * GB)), 3 * GB);
        assert_eq!(ceiling(1024, Some(4 * GB)), 4 * GB / 100 * 65);
        assert_eq!(ceiling(1024, None), 3 * GB);
        assert_eq!(ceiling(0, Some(GB / 2)), 512 * MB);
    }

    #[test]
    fn stops_only_while_memory_grows_above_the_ceiling() {
        let total = Some(16 * GB);
        let c = 3 * GB;
        assert!(!should_stop(2 * GB, GB, c, total));
        assert!(should_stop(4 * GB, 3 * GB, c, total));
        // Flat above the ceiling (memory kept by the allocator): no stop.
        assert!(!should_stop(4 * GB, 4 * GB, c, total));
        // Close to the whole machine: stop regardless.
        assert!(should_stop(14 * GB, 14 * GB, c, total));
    }

    #[test]
    fn a_memory_cap_lowers_the_ceiling() {
        // Under a 3 GB cap the guard steps in at 65 % of it, not at 3 GB.
        assert_eq!(ceiling(1024, Some(3 * GB)), 3 * GB / 100 * 65);
    }

        #[test]
    fn reads_this_process_memory() {
        if cfg!(any(target_os = "linux", target_os = "macos", windows)) {
            let rss = resident_bytes().unwrap();
            assert!(rss > MB && rss < total_memory().unwrap());
        }
    }
}

/// A query that grows past the ceiling is stopped. DuckDB's own limit is
/// set far above the ceiling here so that only the guard can stop it.
/// Heavy: run it alone, inside a memory cap, e.g.
/// `systemd-run --user --scope -p MemoryMax=800M -p MemorySwapMax=0 <test binary> guard_stops --ignored`
#[cfg(test)]
#[test]
#[ignore]
fn guard_stops_a_runaway_query() {
    let ws = parvu_core::Workspace::new(100, None).unwrap();
    ws.set_memory_limit_mb(4096).unwrap();
    let conn = ws.clone_connection().unwrap();
    let stopped = std::sync::Arc::new(AtomicU64::new(0));
    // SAFETY: set before the guard thread starts; no other thread reads it.
    unsafe { std::env::set_var("PARVU_MEMORY_GUARD_MB", "400") };
    let s = stopped.clone();
    start(|| 4096, move |mb| s.store(mb, Ordering::Relaxed));
    let sql = std::env::var("PARVU_GUARD_TEST_SQL").unwrap_or_else(|_| {
        "SELECT count(*) FROM (SELECT i, count(*) FROM range(100000000) t(i) GROUP BY i)".into()
    });
    let r = conn.query_row(&sql, [], |r| r.get::<_, i64>(0));
    let err = r.expect_err("the query should have been stopped").to_string();
    assert!(err.contains("nterrupt"), "{err}");
    assert!(stopped.load(Ordering::Relaxed) >= 400);
    assert!(tripped_recently());
    eprintln!("stopped at {} MB; {} MB in use after", stopped.load(Ordering::Relaxed), resident_bytes().unwrap() / MB);
}

/// Prints the memory the guard sees; run inside a capped scope to check
/// the cap is detected: `systemd-run --user --scope -p MemoryMax=1G <test binary> shows_total_memory --ignored --nocapture`
#[cfg(test)]
#[test]
#[ignore]
fn shows_total_memory() {
    eprintln!("total_memory = {} MB", total_memory().unwrap() / MB);
}
