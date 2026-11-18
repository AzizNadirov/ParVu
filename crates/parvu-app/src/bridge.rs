//! `parvu mcp --attach`: a stdio bridge between an AI client and the
//! running ParVu window (plans/AI_PLAN.md §19.1).
//!
//! AI clients start their MCP servers when a session opens; Claude Code does
//! so for every window, and first probes the server with `server/discover`.
//! So the bridge must not open ParVu for anything but a tool call:
//!
//! * When ParVu is running with AI clients allowed, the bridge connects at
//!   once and relays every line.
//! * Otherwise a local server with the same tools and no window answers
//!   everything (`server/discover`, `initialize`, `tools/list`, `ping`, the
//!   empty lists and "method not found" for features ParVu doesn't have), so
//!   the answers are exactly the window's.
//! * The first `tools/call` connects to the window, starting ParVu in the
//!   background if needed ([`Launcher`]); the session's handshake is replayed
//!   to the window and its duplicate answers are dropped.
//! * If the window goes away (ParVu quits, AI clients are switched off),
//!   requests waiting for it get an error and the bridge falls back to the
//!   local server; the next tool call connects again.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use parvu_core::Workspace;
use rmcp::ServiceExt;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};

use crate::instance;
use crate::mcp::{self, AttachHost, Decision, ParvuMcp, Proposal, RunInfo};

/// A byte stream to an MCP server (the window's listener, or the local one).
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub type Conn = Box<dyn Stream>;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// How the bridge reaches the window. [`Launcher`] in the app; fakes in tests.
pub trait AppLink: Send + Sync {
    /// Connect if the window listens now; never starts ParVu.
    fn connect(&self) -> BoxFuture<'_, Option<Conn>>;
    /// Connect, starting ParVu when it isn't running.
    fn start(&self) -> BoxFuture<'_, Result<Conn, String>>;
}

const SWITCH_ON: &str = "ParVu is open, but AI clients are not allowed. In ParVu choose AI → Connect an AI client \
                         and switch it on, then try again.";
const SWITCHED_OFF: &str = "AI clients are switched off in ParVu, so it was not started. Open ParVu, choose AI → \
                            Connect an AI client and switch it on, then try again.";
const NOT_IN_TIME: &str = "ParVu did not start in time. Open it yourself, then try again.";
const WINDOW_CLOSED: &str = "ParVu closed the connection (it was closed, or AI clients were switched off).";

/// How long the bridge waits for a ParVu it started (MCP clients give up on
/// a tool call after a minute or more; on startup after about 30 seconds).
const LAUNCH_WAIT: Duration = Duration::from_secs(25);

/// Entry point of `parvu mcp --attach`.
pub async fn run() -> Result<(), String> {
    let launcher = Launcher::new();
    bridge(BufReader::new(tokio::io::stdin()), tokio::io::stdout(), &launcher).await
}

/// One server the bridge talks to.
struct Backend {
    lines: tokio::io::Lines<BufReader<ReadHalf<Conn>>>,
    write: WriteHalf<Conn>,
    /// The window (`false`: the local server).
    window: bool,
    /// Ids of replayed requests the client already has answers to.
    drop: Vec<Value>,
    /// Requests sent here and not answered yet, with their lines.
    pending: Vec<(Value, String)>,
}

impl Backend {
    fn new(conn: Conn, window: bool) -> Self {
        let (read, write) = tokio::io::split(conn);
        Backend { lines: BufReader::new(read).lines(), write, window, drop: Vec::new(), pending: Vec::new() }
    }

    /// A server with ParVu's tools and no window, on an in-memory pipe.
    fn local() -> Self {
        let (ours, theirs) = tokio::io::duplex(1 << 20);
        tokio::spawn(async move {
            let server = ParvuMcp::attached(Arc::new(Offline));
            if let Ok(running) = server.serve(tokio::io::split(theirs)).await {
                let _ = running.waiting().await;
            }
        });
        Backend::new(Box::new(ours), false)
    }

    async fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.write.write_all(line.as_bytes()).await?;
        self.write.write_all(b"\n").await?;
        self.write.flush().await
    }

    /// Bring a new server up to the client's session: the handshake lines
    /// (answers dropped, the client has them) and requests still waiting.
    async fn replay(&mut self, session: &[String], waiting: Vec<(Value, String)>) -> std::io::Result<()> {
        for line in session {
            if let Some(id) = request_id(line)
                && !waiting.iter().any(|(w, _)| *w == id)
            {
                self.drop.push(id);
            }
            self.send(line).await?;
        }
        for (id, line) in waiting {
            if !session.contains(&line) {
                self.send(&line).await?;
            }
            self.pending.push((id, line));
        }
        Ok(())
    }
}

/// The id of a request line (a message with a method and an id).
fn request_id(line: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(line).ok()?;
    v.get("method")?;
    v.get("id").cloned()
}

async fn write_line(out: &mut (impl AsyncWrite + Unpin), v: &Value) -> Result<(), String> {
    let mut text = v.to_string();
    text.push('\n');
    out.write_all(text.as_bytes()).await.map_err(|e| e.to_string())?;
    out.flush().await.map_err(|e| e.to_string())
}

/// A failed tool call, as a tool result the model can read.
fn tool_failure(msg: &Value, text: &str) -> Value {
    let mut result = json!({ "content": [{ "type": "text", "text": text }], "isError": true });
    // Protocol 2026-07-28 tags every result; older versions have no tag.
    if msg.pointer("/params/_meta/io.modelcontextprotocol~1protocolVersion").is_some() {
        result["resultType"] = "complete".into();
    }
    json!({ "jsonrpc": "2.0", "id": msg.get("id"), "result": result })
}

enum Event {
    Client(std::io::Result<Option<String>>),
    Server(std::io::Result<Option<String>>),
}

/// Relay between the client (`input`/`output`) and the window, as described
/// in the module docs.
pub async fn bridge(
    input: impl AsyncBufRead + Unpin,
    mut output: impl AsyncWrite + Unpin,
    link: &dyn AppLink,
) -> Result<(), String> {
    let mut input = input.lines();
    // `initialize` and `notifications/initialized`: replayed to each new server.
    let mut session: Vec<String> = Vec::new();
    let mut backend = link.connect().await.map(|c| Backend::new(c, true));
    loop {
        let event = match backend.as_mut() {
            Some(b) => tokio::select! {
                l = input.next_line() => Event::Client(l),
                l = b.lines.next_line() => Event::Server(l),
            },
            None => Event::Client(input.next_line().await),
        };
        match event {
            Event::Client(Ok(None)) => return Ok(()), // the client ended the session
            Event::Client(Err(e)) => return Err(e.to_string()),
            Event::Client(Ok(Some(line))) => {
                if line.trim().is_empty() {
                    continue;
                }
                let msg: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
                let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
                let id = msg.get("method").and(msg.get("id")).cloned();
                if method == "tools/call" && !backend.as_ref().is_some_and(|b| b.window) {
                    match link.start().await {
                        Ok(conn) => {
                            let waiting = backend.take().map(|b| b.pending).unwrap_or_default();
                            let mut b = Backend::new(conn, true);
                            b.replay(&session, waiting).await.map_err(|e| e.to_string())?;
                            backend = Some(b);
                        }
                        Err(e) => {
                            tracing::warn!("MCP bridge: {e}");
                            write_line(&mut output, &tool_failure(&msg, &e)).await?;
                            continue;
                        }
                    }
                }
                let b = match backend.as_mut() {
                    Some(b) => b,
                    None => {
                        let mut b = Backend::local();
                        b.replay(&session, Vec::new()).await.map_err(|e| e.to_string())?;
                        backend.insert(b)
                    }
                };
                if method == "initialize" || method == "notifications/initialized" {
                    session.push(line.clone());
                }
                if let Some(id) = id {
                    b.pending.push((id, line.clone()));
                }
                if let Err(e) = b.send(&line).await {
                    tracing::warn!("MCP bridge: sending to ParVu: {e}");
                }
            }
            Event::Server(Ok(Some(line))) => {
                let Some(b) = backend.as_mut() else { continue };
                if let Ok(v) = serde_json::from_str::<Value>(&line)
                    && v.get("method").is_none()
                    && let Some(id) = v.get("id")
                {
                    if let Some(i) = b.drop.iter().position(|d| d == id) {
                        b.drop.remove(i);
                        continue;
                    }
                    b.pending.retain(|(p, _)| p != id);
                }
                output.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
                output.write_all(b"\n").await.map_err(|e| e.to_string())?;
                output.flush().await.map_err(|e| e.to_string())?;
            }
            Event::Server(_) => {
                // The window went away: answer what waited for it, and serve
                // the session locally until the next tool call.
                let Some(b) = backend.take() else { continue };
                tracing::info!("MCP bridge: ParVu closed the connection");
                for (id, _) in b.pending {
                    let err = json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": WINDOW_CLOSED } });
                    write_line(&mut output, &err).await?;
                }
            }
        }
    }
}

/// An attached server with no window: answers everything but tool calls,
/// which the bridge sends to the window instead.
struct Offline;

impl AttachHost for Offline {
    fn workspace(&self) -> Result<(String, Arc<Mutex<Workspace>>), String> {
        Err("ParVu is not connected yet".into())
    }
    fn propose(&self, _proposal: Proposal) -> Pin<Box<dyn Future<Output = Decision> + Send>> {
        Box::pin(async { Decision::Rejected })
    }
    fn changed(&self, _window: &str, _tab_id: u64) {}
}

/// Finds or starts the ParVu app of this profile.
pub struct Launcher {
    run_file: PathBuf,
    launch_lock: PathBuf,
    instance_lock: PathBuf,
    /// The user's "Allow AI clients" setting, as saved.
    clients_allowed: Box<dyn Fn() -> bool + Send + Sync>,
    /// Start ParVu (detached, in the background).
    launch: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
    wait: Duration,
}

impl Launcher {
    pub fn new() -> Self {
        Launcher {
            run_file: mcp::run_file(),
            launch_lock: instance::launch_lock_file(),
            instance_lock: instance::instance_lock_file(),
            clients_allowed: Box::new(clients_allowed),
            launch: Box::new(launch_app),
            wait: LAUNCH_WAIT,
        }
    }

    async fn connect_now(&self) -> Option<Conn> {
        let text = tokio::fs::read_to_string(&self.run_file).await.ok()?;
        let info: RunInfo = serde_json::from_str(&text).ok()?;
        let mut sock = tokio::net::TcpStream::connect(("127.0.0.1", info.port)).await.ok()?;
        sock.write_all(format!("{}\n", info.token).as_bytes()).await.ok()?;
        Some(Box::new(sock))
    }

    /// Poll for the listener until `wait` has passed.
    async fn connect_within(&self, wait: Duration) -> Option<Conn> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            if let Some(c) = self.connect_now().await {
                return Some(c);
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    fn app_running(&self) -> bool {
        instance::is_locked(&self.instance_lock)
    }

    async fn start_app(&self) -> Result<Conn, String> {
        if let Some(c) = self.connect_now().await {
            return Ok(c);
        }
        if !(self.clients_allowed)() {
            // The user's choice: don't open ParVu behind their back.
            return Err(if self.app_running() { SWITCH_ON } else { SWITCHED_OFF }.into());
        }
        // One bridge starts ParVu; the others wait here, then connect.
        let path = self.launch_lock.clone();
        let wait = self.wait;
        let lock = tokio::task::spawn_blocking(move || instance::lock_within(&path, wait))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{}: {e}", self.launch_lock.display()))?;
        if lock.is_none() {
            return self.connect_now().await.ok_or_else(|| NOT_IN_TIME.to_string());
        }
        if let Some(c) = self.connect_now().await {
            return Ok(c);
        }
        if self.app_running() {
            // Running, but not listening: it may be starting right now.
            return self.connect_within(Duration::from_secs(5)).await.ok_or_else(|| SWITCH_ON.to_string());
        }
        tracing::info!("MCP bridge: starting ParVu in the background");
        (self.launch)()?;
        let conn = self.connect_within(self.wait).await;
        drop(lock);
        conn.ok_or_else(|| if self.app_running() { SWITCH_ON.to_string() } else { NOT_IN_TIME.to_string() })
    }
}

impl AppLink for Launcher {
    fn connect(&self) -> BoxFuture<'_, Option<Conn>> {
        Box::pin(self.connect_now())
    }
    fn start(&self) -> BoxFuture<'_, Result<Conn, String>> {
        Box::pin(self.start_app())
    }
}

/// The saved "Allow AI clients to connect" setting. No settings file means
/// ParVu never ran, so it is off.
fn clients_allowed() -> bool {
    let paths = parvu_config::AppPaths::default();
    paths.settings_file().exists() && parvu_config::SettingsManager::with_paths(paths).settings().mcp_allow_clients
}

/// Start ParVu in the background (`--background`: hidden in the tray, or
/// minimised without one), detached from this process: its
/// output must not reach stdout (the MCP channel), and it must outlive the
/// bridge.
fn launch_app() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--background");
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("could not start ParVu: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::mcp::{CustomOffer, new_token, serve_tcp, write_run_file};

    const MODERN: &str = "2026-07-28";

    fn meta() -> Value {
        json!({
            "io.modelcontextprotocol/protocolVersion": MODERN,
            "io.modelcontextprotocol/clientInfo": { "name": "claude-code", "version": "2" },
            "io.modelcontextprotocol/clientCapabilities": {}
        })
    }

    /// A window: an attached server over an in-memory pipe, on a workspace
    /// with one table.
    struct Window {
        ws: Arc<Mutex<Workspace>>,
        _dir: tempfile::TempDir,
    }

    impl AttachHost for Window {
        fn workspace(&self) -> Result<(String, Arc<Mutex<Workspace>>), String> {
            Ok(("main".into(), self.ws.clone()))
        }
        fn propose(&self, _p: Proposal) -> Pin<Box<dyn Future<Output = Decision> + Send>> {
            Box::pin(async { Decision::Approved })
        }
        fn changed(&self, _window: &str, _tab_id: u64) {}
        fn offer_custom(&self, _window: &str, _offer: CustomOffer) {}
    }

    fn window() -> Arc<Window> {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("sales.csv");
        std::fs::write(&f, "id,qty\n1,3\n2,5\n").unwrap();
        let mut ws = Workspace::new(100, None).unwrap();
        ws.open(&f).unwrap();
        Arc::new(Window { ws: Arc::new(Mutex::new(ws)), _dir: dir })
    }

    /// A link that counts starts, can refuse them, and serves a window.
    #[derive(Default)]
    struct FakeLink {
        running: Mutex<bool>,
        refuse: Option<String>,
        starts: AtomicUsize,
        /// Close each window connection after this many lines from it.
        hang_up: Option<usize>,
    }

    impl FakeLink {
        fn conn(&self) -> Conn {
            let (ours, theirs) = tokio::io::duplex(1 << 20);
            let host = window();
            let hang_up = self.hang_up;
            tokio::spawn(async move {
                let server = ParvuMcp::attached(host);
                let (read, write) = tokio::io::split(theirs);
                match hang_up {
                    None => {
                        if let Ok(r) = server.serve((read, write)).await {
                            let _ = r.waiting().await;
                        }
                    }
                    Some(_) => {
                        // Serve until the first answer, then drop the connection.
                        let _ = tokio::time::timeout(Duration::from_millis(300), async {
                            if let Ok(r) = server.serve((read, write)).await {
                                let _ = r.waiting().await;
                            }
                        })
                        .await;
                    }
                }
            });
            Box::new(ours)
        }
    }

    impl AppLink for FakeLink {
        fn connect(&self) -> BoxFuture<'_, Option<Conn>> {
            Box::pin(async move { (*self.running.lock().unwrap()).then(|| self.conn()) })
        }
        fn start(&self) -> BoxFuture<'_, Result<Conn, String>> {
            Box::pin(async move {
                self.starts.fetch_add(1, Ordering::SeqCst);
                if let Some(e) = &self.refuse {
                    return Err(e.clone());
                }
                *self.running.lock().unwrap() = true;
                Ok(self.conn())
            })
        }
    }

    /// Run the bridge over `lines` and return what the client got back.
    async fn talk(link: &FakeLink, lines: &[Value]) -> Vec<Value> {
        let (mut client_w, bridge_r) = tokio::io::duplex(1 << 20);
        let (bridge_w, client_r) = tokio::io::duplex(1 << 20);
        let expected = lines.iter().filter(|l| l.get("id").is_some()).count();
        let input: Vec<String> = lines.iter().map(|l| format!("{l}\n")).collect();
        let client = async move {
            let mut out = Vec::new();
            let mut reader = BufReader::new(client_r).lines();
            for l in input {
                let v: Value = serde_json::from_str(&l).unwrap();
                client_w.write_all(l.as_bytes()).await.unwrap();
                // Like a real client: wait for each answer before going on.
                if v.get("id").is_some() {
                    let line = tokio::time::timeout(Duration::from_secs(20), reader.next_line()).await.unwrap().unwrap().unwrap();
                    out.push(serde_json::from_str::<Value>(&line).unwrap());
                }
            }
            drop(client_w);
            while let Ok(Ok(Some(line))) = tokio::time::timeout(Duration::from_millis(200), reader.next_line()).await {
                out.push(serde_json::from_str(&line).unwrap());
            }
            out
        };
        let (out, res) = tokio::join!(client, bridge(BufReader::new(bridge_r), bridge_w, link));
        res.unwrap();
        assert!(out.len() >= expected, "{out:#?}");
        out
    }

    fn req(id: impl Into<Value>, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id.into(), "method": method, "params": params })
    }

    fn modern(id: u64, method: &str, mut params: Value) -> Value {
        params["_meta"] = meta();
        req(id, method, params)
    }

    fn modern_startup() -> Vec<Value> {
        vec![
            req("server-discover-probe-1", "server/discover", json!({ "_meta": meta() })),
            modern(0, "tools/list", json!({})),
            modern(1, "prompts/list", json!({})),
            modern(2, "resources/list", json!({})),
            modern(3, "resources/templates/list", json!({})),
            modern(4, "ping", json!({})),
            modern(5, "logging/setLevel", json!({ "level": "info" })),
            modern(6, "subscriptions/listen", json!({ "notifications": { "toolsListChanged": true } })),
            modern(7, "completion/complete", json!({ "ref": { "type": "ref/prompt", "name": "x" }, "argument": { "name": "a", "value": "" } })),
        ]
    }

    #[tokio::test]
    async fn modern_startup_is_answered_without_starting_parvu() {
        let link = FakeLink::default();
        let out = talk(&link, &modern_startup()).await;
        assert_eq!(link.starts.load(Ordering::SeqCst), 0, "{out:#?}");
        let by_id = |id: Value| out.iter().find(|v| v["id"] == id).unwrap_or_else(|| panic!("no answer to {id}: {out:#?}"));
        let discover = by_id(json!("server-discover-probe-1"));
        assert!(discover["result"]["supportedVersions"].as_array().unwrap().contains(&json!(MODERN)), "{discover}");
        assert!(discover["result"]["capabilities"]["tools"].is_object());
        let tools = by_id(json!(0))["result"]["tools"].as_array().unwrap().clone();
        assert!(tools.iter().any(|t| t["name"] == "apply_operation"));
        assert!(!tools.iter().any(|t| t["name"] == "open_file"));
        assert_eq!(by_id(json!(1))["result"]["prompts"], json!([]));
        assert_eq!(by_id(json!(2))["result"]["resources"], json!([]));
        assert_eq!(by_id(json!(3))["result"]["resourceTemplates"], json!([]));
        // Features ParVu doesn't have (and `ping`, which 2026-07-28 dropped):
        // "method not found", as from the window. Completion gets an empty list.
        assert_eq!(by_id(json!(7))["result"]["completion"]["values"], json!([]));
        for id in [4, 5, 6] {
            assert_eq!(by_id(json!(id))["error"]["code"], json!(-32601), "{out:#?}");
        }

        // Exactly what the window answers.
        let running = FakeLink { running: Mutex::new(true), ..Default::default() };
        let direct = talk(&running, &modern_startup()).await;
        for (a, b) in out.iter().zip(&direct) {
            assert_eq!(a, b);
        }
        assert_eq!(out.len(), direct.len());
    }

    #[tokio::test]
    async fn legacy_handshake_is_answered_without_starting_parvu() {
        let link = FakeLink::default();
        let lines = [
            req(0, "initialize", json!({ "protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": { "name": "c", "version": "1" } })),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            req(1, "tools/list", json!({})),
            req(2, "ping", json!({})),
            req(3, "prompts/list", json!({})),
            req(4, "resources/subscribe", json!({ "uri": "x" })),
        ];
        let out = talk(&link, &lines).await;
        let running = FakeLink { running: Mutex::new(true), ..Default::default() };
        assert_eq!(talk(&running, &lines).await, out);
        assert_eq!(link.starts.load(Ordering::SeqCst), 0);
        assert_eq!(out[0]["result"]["protocolVersion"], json!("2025-11-25"));
        assert!(out[0]["result"]["instructions"].as_str().unwrap().contains("running ParVu window"));
        assert!(!out[1]["result"]["tools"].as_array().unwrap().is_empty());
        assert!(out[2]["result"].is_object());
        assert_eq!(out[3]["result"]["prompts"], json!([]));
        assert_eq!(out[4]["error"]["code"], json!(-32601));
    }

    #[tokio::test]
    async fn only_a_tool_call_starts_parvu_and_the_handshake_is_replayed() {
        let link = FakeLink::default();
        let out = talk(
            &link,
            &[
                req(0, "initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "c", "version": "1" } })),
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                req(1, "tools/list", json!({})),
                req(2, "tools/call", json!({ "name": "query", "arguments": { "sql": "SELECT SUM(qty) AS t FROM sales" } })),
                req(3, "tools/call", json!({ "name": "list_tables", "arguments": {} })),
            ],
        )
        .await;
        assert_eq!(link.starts.load(Ordering::SeqCst), 1);
        // One answer per request: the window's answer to the replayed
        // initialize is dropped.
        assert_eq!(out.len(), 4, "{out:#?}");
        assert_eq!(out[2]["id"], json!(2));
        assert!(out[2]["result"]["content"][0]["text"].as_str().unwrap().contains('8'), "{}", out[2]);
        assert!(out[3]["result"]["content"][0]["text"].as_str().unwrap().contains("sales"));
    }

    #[tokio::test]
    async fn modern_tool_call_reaches_the_window() {
        let link = FakeLink::default();
        let out = talk(
            &link,
            &[
                req("server-discover-probe-1", "server/discover", json!({ "_meta": meta() })),
                modern(0, "tools/list", json!({})),
                modern(1, "tools/call", json!({ "name": "query", "arguments": { "sql": "SELECT COUNT(*) AS n FROM sales" } })),
            ],
        )
        .await;
        assert_eq!(link.starts.load(Ordering::SeqCst), 1);
        assert_eq!(out.len(), 3, "{out:#?}");
        assert_eq!(out[2]["result"]["isError"], json!(false), "{}", out[2]);
        assert_eq!(out[2]["result"]["resultType"], json!("complete"));
    }

    #[tokio::test]
    async fn a_refused_start_is_a_tool_error_and_the_session_goes_on() {
        let link = FakeLink { refuse: Some(SWITCHED_OFF.into()), ..Default::default() };
        let out = talk(
            &link,
            &[
                req("p", "server/discover", json!({ "_meta": meta() })),
                modern(1, "tools/call", json!({ "name": "list_tables", "arguments": {} })),
                modern(2, "tools/list", json!({})),
            ],
        )
        .await;
        assert_eq!(link.starts.load(Ordering::SeqCst), 1);
        assert_eq!(out[1]["result"]["isError"], json!(true));
        assert_eq!(out[1]["result"]["resultType"], json!("complete"));
        assert!(out[1]["result"]["content"][0]["text"].as_str().unwrap().contains("switched off"));
        assert!(out[2]["result"]["tools"].is_array());
    }

    #[tokio::test]
    async fn a_running_window_gets_everything() {
        let link = FakeLink { running: Mutex::new(true), ..Default::default() };
        let out = talk(
            &link,
            &[
                req("p", "server/discover", json!({ "_meta": meta() })),
                modern(0, "tools/list", json!({})),
                modern(1, "tools/call", json!({ "name": "list_tables", "arguments": {} })),
            ],
        )
        .await;
        assert_eq!(link.starts.load(Ordering::SeqCst), 0);
        assert_eq!(out.len(), 3);
        assert!(out[2]["result"]["content"][0]["text"].as_str().unwrap().contains("sales"));
    }

    #[tokio::test]
    async fn the_bridge_reconnects_after_the_window_goes_away() {
        let link = FakeLink { hang_up: Some(1), ..Default::default() };
        let (mut cw, br) = tokio::io::duplex(1 << 20);
        let (bw, cr) = tokio::io::duplex(1 << 20);
        let bridge_task = async { bridge(BufReader::new(br), bw, &link).await };
        let client = async {
            let mut lines = BufReader::new(cr).lines();
            let mut ask = async |v: Value| {
                cw.write_all(format!("{v}\n").as_bytes()).await.unwrap();
                let l = tokio::time::timeout(Duration::from_secs(20), lines.next_line()).await.unwrap().unwrap().unwrap();
                serde_json::from_str::<Value>(&l).unwrap()
            };
            let a = ask(modern(1, "tools/call", json!({ "name": "list_tables", "arguments": {} }))).await;
            assert!(a["result"]["content"][0]["text"].as_str().unwrap().contains("sales"));
            // The fake window hangs up; the session goes on locally...
            tokio::time::sleep(Duration::from_millis(500)).await;
            let a = ask(modern(2, "tools/list", json!({}))).await;
            assert!(a["result"]["tools"].is_array(), "{a}");
            // ...and the next tool call connects again.
            let a = ask(modern(3, "tools/call", json!({ "name": "list_tables", "arguments": {} }))).await;
            assert!(a["result"]["content"][0]["text"].as_str().unwrap().contains("sales"), "{a}");
            drop(cw);
        };
        let (_, r) = tokio::join!(client, bridge_task);
        r.unwrap();
        assert_eq!(link.starts.load(Ordering::SeqCst), 2);
    }

    /// The real launcher, with its files in a temp folder and a fake launch
    /// that starts a listener after a while, as ParVu does.
    fn launcher(dir: &std::path::Path, allowed: bool, launches: Arc<AtomicUsize>) -> Launcher {
        let run_file = dir.join("mcp.json");
        let instance_lock = dir.join("instance.lock");
        let rf = run_file.clone();
        let il = instance_lock.clone();
        Launcher {
            run_file,
            launch_lock: dir.join("launch.lock"),
            instance_lock,
            clients_allowed: Box::new(move || allowed),
            launch: Box::new(move || {
                launches.fetch_add(1, Ordering::SeqCst);
                let (rf, il) = (rf.clone(), il.clone());
                std::thread::spawn(move || {
                    let _held = instance::try_lock(&il).unwrap().expect("one app");
                    std::thread::sleep(Duration::from_millis(400));
                    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                    rt.block_on(async {
                        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
                        let port = listener.local_addr().unwrap().port();
                        let token = new_token();
                        write_run_file(&rf, &RunInfo { port, token: token.clone(), pid: 1 }).unwrap();
                        let host = window();
                        let _ = tokio::time::timeout(
                            Duration::from_secs(10),
                            serve_tcp(listener, token, move || ParvuMcp::attached(host.clone()), |_| {}),
                        )
                        .await;
                    });
                });
                Ok(())
            }),
            wait: Duration::from_secs(10),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_bridges_start_one_parvu() {
        let dir = tempfile::tempdir().unwrap();
        let launches = Arc::new(AtomicUsize::new(0));
        let l = Arc::new(launcher(dir.path(), true, launches.clone()));
        let tasks: Vec<_> = (0..4)
            .map(|_| {
                let l = l.clone();
                tokio::spawn(async move { l.start_app().await.map(|_| ()) })
            })
            .collect();
        for t in tasks {
            t.await.unwrap().unwrap();
        }
        assert_eq!(launches.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn switched_off_clients_never_start_parvu() {
        let dir = tempfile::tempdir().unwrap();
        let launches = Arc::new(AtomicUsize::new(0));
        let l = launcher(dir.path(), false, launches.clone());
        let e = l.start_app().await.err().unwrap();
        assert!(e.contains("switched off"), "{e}");
        // Open, but not listening: the user is told how to switch it on.
        let _held = instance::try_lock(&dir.path().join("instance.lock")).unwrap().unwrap();
        let e = l.start_app().await.err().unwrap();
        assert!(e.contains("not allowed"), "{e}");
        assert_eq!(launches.load(Ordering::SeqCst), 0);
    }
}
