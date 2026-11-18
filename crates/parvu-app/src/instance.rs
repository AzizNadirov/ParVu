//! One ParVu process per profile (`~/.ParVu`), background starts, and the
//! lock that lets only one `parvu mcp --attach` bridge start ParVu at a time.
//!
//! * The running app holds an exclusive lock on `~/.ParVu/run/instance.lock`
//!   for its whole life; the OS releases it when the process ends, even after
//!   a crash. A second `parvu` (menu, file manager, `parvu file.csv`) finds the
//!   lock taken and hands its files to the running app over a local control
//!   connection (`~/.ParVu/run/app.json`: port and token, readable only by the
//!   user), then exits. `parvu mcp …` never takes part: `main` handles it first.
//! * Bridges that need ParVu take `~/.ParVu/run/launch.lock` before starting
//!   it, so several AI sessions opening at once start one ParVu, not several.
//! * `parvu --background` starts with the window minimised. Until the user
//!   brings it up, it quits by itself a while after the last AI client leaves
//!   ([`IdleClock`]).

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::mcp::{self, RunInfo};

/// Command-line options of the app (not of `parvu mcp`).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct AppArgs {
    /// `--background`: start minimised, for AI clients (see `bridge.rs`).
    pub background: bool,
    pub files: Vec<PathBuf>,
}

impl AppArgs {
    /// Parse the arguments after the program name. Unknown `--options` are
    /// ignored, so a desktop launcher's extra flags never stop ParVu.
    pub fn parse(args: impl IntoIterator<Item = std::ffi::OsString>) -> Self {
        let mut out = AppArgs::default();
        for a in args {
            match a.to_str() {
                Some("--background") => out.background = true,
                Some(s) if s.starts_with("--") => tracing::warn!("ignoring unknown option {s}"),
                _ => out.files.push(PathBuf::from(a)),
            }
        }
        out
    }
}

fn run_dir() -> PathBuf {
    parvu_config::paths::legacy_app_dir().join("run")
}

pub fn instance_lock_file() -> PathBuf {
    run_dir().join("instance.lock")
}

pub fn launch_lock_file() -> PathBuf {
    run_dir().join("launch.lock")
}

/// Where the running app publishes its control connection.
pub fn control_file() -> PathBuf {
    run_dir().join("app.json")
}

/// Try to take the exclusive lock on `path`: `Ok(Some(file))` holds it until
/// the file is dropped, `Ok(None)` means another process holds it.
pub fn try_lock(path: &Path) -> std::io::Result<Option<File>> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

/// Wait up to `wait` for the lock on `path`. `None` when it stays taken.
pub fn lock_within(path: &Path, wait: Duration) -> std::io::Result<Option<File>> {
    let deadline = Instant::now() + wait;
    loop {
        if let Some(f) = try_lock(path)? {
            return Ok(Some(f));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether another process holds the lock on `path` (for the instance
/// lock: a ParVu app of this profile is running). Holds a free lock for an
/// instant; an app starting at that moment retries (see [`claim`]).
pub fn is_locked(path: &Path) -> bool {
    matches!(try_lock(path), Ok(None))
}

/// What a second `parvu` asks of the running one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Files to open (absolute paths).
    #[serde(default)]
    pub files: Vec<PathBuf>,
    /// Bring the window to the front. `false` for `--background` starts,
    /// which only need ParVu to be running.
    #[serde(default)]
    pub show: bool,
}

/// How this process starts.
pub enum Startup {
    /// This is the profile's app. Keep the lock (if any) until exit.
    Primary(Option<File>),
    /// The running app took the request; exit.
    Forwarded,
}

/// Become the profile's app, or hand `request` to the one that runs.
pub fn claim(request: &Request) -> Startup {
    claim_at(&instance_lock_file(), &control_file(), request, Duration::from_secs(10))
}

fn claim_at(lock: &Path, control: &Path, request: &Request, wait: Duration) -> Startup {
    let deadline = Instant::now() + wait;
    loop {
        match try_lock(lock) {
            Ok(Some(f)) => return Startup::Primary(Some(f)),
            Ok(None) => match forward(control, request) {
                Ok(()) => return Startup::Forwarded,
                // Still starting (no control file yet), or a bridge holds the
                // lock for an instant: try again.
                Err(e) => tracing::debug!("single instance: {e}"),
            },
            Err(e) => {
                tracing::warn!("single instance: cannot lock {}: {e}; starting anyway", lock.display());
                return Startup::Primary(None);
            }
        }
        if Instant::now() >= deadline {
            tracing::warn!("single instance: the running ParVu does not answer; starting another one");
            return Startup::Primary(None);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Send `request` to the app listening per `control`.
fn forward(control: &Path, request: &Request) -> Result<(), String> {
    let text = std::fs::read_to_string(control).map_err(|e| format!("{}: {e}", control.display()))?;
    let info: RunInfo = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], info.port));
    let mut sock = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
    let body = serde_json::to_string(request).map_err(|e| e.to_string())?;
    sock.write_all(format!("{}\n{body}\n", info.token).as_bytes()).map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(sock).read_line(&mut reply).map_err(|e| e.to_string())?;
    if reply.trim() == "ok" { Ok(()) } else { Err(format!("unexpected reply {reply:?}")) }
}

/// Listen for other `parvu` starts and pass their requests to `handle`
/// (called on the listener's thread). Writes the control file.
pub fn serve(handle: impl Fn(Request) + Send + 'static) -> std::io::Result<()> {
    serve_at(&control_file(), handle)
}

fn serve_at(control: &Path, handle: impl Fn(Request) + Send + 'static) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let token = mcp::new_token();
    mcp::write_run_file(control, &RunInfo { port, token: token.clone(), pid: std::process::id() })?;
    std::thread::Builder::new().name("parvu-instance".into()).spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut sock) = conn else { continue };
            if !sock.peer_addr().is_ok_and(|a| a.ip().is_loopback()) {
                continue;
            }
            let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
            let Ok(read) = sock.try_clone() else { continue };
            let mut read = BufReader::new(read.take(64 * 1024));
            let (mut first, mut body) = (String::new(), String::new());
            if read.read_line(&mut first).is_err() || !mcp::same_token(first.trim_end(), &token) {
                tracing::warn!("single instance: refused a connection with a wrong token");
                continue;
            }
            if read.read_line(&mut body).is_err() {
                continue;
            }
            match serde_json::from_str::<Request>(body.trim()) {
                Ok(req) => {
                    handle(req);
                    let _ = sock.write_all(b"ok\n");
                }
                Err(e) => tracing::warn!("single instance: bad request: {e}"),
            }
        }
    })?;
    Ok(())
}

/// Remove the control file at exit, if it is this process's.
pub fn clear_control_file() {
    let path = control_file();
    if std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<RunInfo>(&t).ok())
        .is_some_and(|info| info.pid == std::process::id())
    {
        let _ = std::fs::remove_file(&path);
    }
}

/// How long a background ParVu nobody opened stays after the last AI client
/// leaves: `PARVU_BACKGROUND_IDLE_SECS`, 10 minutes by default.
pub fn idle_grace() -> Duration {
    let secs = std::env::var("PARVU_BACKGROUND_IDLE_SECS").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(600);
    Duration::from_secs(secs)
}

/// Decides when a background ParVu quits: once it has been without AI
/// clients for `grace`, as long as the user never brought the window up.
#[derive(Debug)]
pub struct IdleClock {
    grace: Duration,
    idle_since: Option<Instant>,
}

impl IdleClock {
    pub fn new(grace: Duration) -> Self {
        Self { grace, idle_since: None }
    }

    /// Called every few seconds; `true` means quit now.
    pub fn tick(&mut self, now: Instant, untouched: bool, clients: usize) -> bool {
        if !untouched || clients > 0 {
            self.idle_since = None;
            return false;
        }
        let since = *self.idle_since.get_or_insert(now);
        now.duration_since(since) >= self.grace
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::{Arc, Mutex};

    #[test]
    fn parses_app_args() {
        let parse = |a: &[&str]| AppArgs::parse(a.iter().map(OsString::from));
        assert_eq!(parse(&[]), AppArgs::default());
        let a = parse(&["--background"]);
        assert!(a.background && a.files.is_empty());
        let a = parse(&["a.csv", "--background", "--some-launcher-flag", "dir/b.parquet"]);
        assert!(a.background);
        assert_eq!(a.files, [PathBuf::from("a.csv"), PathBuf::from("dir/b.parquet")]);
        assert!(!parse(&["--backgroundx"]).background);
    }

    #[test]
    fn idle_clock_quits_only_untouched_and_idle() {
        let grace = Duration::from_secs(600);
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let mut c = IdleClock::new(grace);
        // No client ever connected: quits a grace period after start.
        assert!(!c.tick(at(0), true, 0));
        assert!(!c.tick(at(599), true, 0));
        assert!(c.tick(at(600), true, 0));

        // A client resets the clock; the grace counts from when it leaves.
        let mut c = IdleClock::new(grace);
        assert!(!c.tick(at(0), true, 0));
        assert!(!c.tick(at(500), true, 1));
        assert!(!c.tick(at(1000), true, 2));
        assert!(!c.tick(at(1100), true, 0));
        assert!(!c.tick(at(1699), true, 0));
        assert!(c.tick(at(1700), true, 0));

        // Once the user used the window, it never quits by itself.
        let mut c = IdleClock::new(grace);
        assert!(!c.tick(at(0), true, 0));
        assert!(!c.tick(at(10_000), false, 0));
        assert!(!c.tick(at(100_000), false, 0));
    }

    #[test]
    fn second_start_is_forwarded_to_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let (lock, control) = (dir.path().join("instance.lock"), dir.path().join("app.json"));
        let Startup::Primary(Some(held)) = claim_at(&lock, &control, &Request::default(), Duration::ZERO) else {
            panic!("the first start must own the lock")
        };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        serve_at(&control, move |r| s.lock().unwrap().push(r)).unwrap();

        let req = Request { files: vec![PathBuf::from("/data/a.csv")], show: true };
        assert!(matches!(claim_at(&lock, &control, &req, Duration::from_secs(5)), Startup::Forwarded));
        assert_eq!(*seen.lock().unwrap(), [req]);

        // A wrong token is refused.
        let mut info: RunInfo = serde_json::from_str(&std::fs::read_to_string(&control).unwrap()).unwrap();
        info.token = mcp::new_token();
        let bad = dir.path().join("bad.json");
        mcp::write_run_file(&bad, &info).unwrap();
        assert!(forward(&bad, &Request::default()).is_err());
        assert_eq!(seen.lock().unwrap().len(), 1);

        // When the first one ends, the next start owns the lock.
        drop(held);
        assert!(matches!(claim_at(&lock, &control, &Request::default(), Duration::ZERO), Startup::Primary(Some(_))));
    }

    #[test]
    fn a_lock_without_an_answer_starts_anyway() {
        let dir = tempfile::tempdir().unwrap();
        let lock = dir.path().join("instance.lock");
        let _held = try_lock(&lock).unwrap().unwrap();
        let started = Instant::now();
        let s = claim_at(&lock, &dir.path().join("none.json"), &Request::default(), Duration::from_millis(300));
        assert!(matches!(s, Startup::Primary(None)));
        assert!(started.elapsed() >= Duration::from_millis(300));
    }

    #[test]
    fn launch_lock_admits_one_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("launch.lock");
        let inside = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let most = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let threads: Vec<_> = (0..6)
            .map(|_| {
                let (path, inside, most) = (path.clone(), inside.clone(), most.clone());
                std::thread::spawn(move || {
                    let _l = lock_within(&path, Duration::from_secs(20)).unwrap().expect("lock");
                    let n = inside.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    most.fetch_max(n, std::sync::atomic::Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(30));
                    inside.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(most.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Held elsewhere, a short wait gives up.
        let _held = try_lock(&path).unwrap().unwrap();
        assert!(lock_within(&path, Duration::from_millis(100)).unwrap().is_none());
    }
}
