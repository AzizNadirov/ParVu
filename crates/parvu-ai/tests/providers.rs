//! Providers and the agent loop against a local fake HTTP server that
//! replays recorded server-sent event streams.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use parvu_ai::provider::{Request, anthropic};
use parvu_ai::{
    Agent, AgentEvent, Block, Delta, Message, Provider, ProviderConfig, Role, Secret, Stop, ToolBackend, ToolOutput,
    ToolSpec,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Requests the fake server saw: lower-cased headers and the JSON body.
type Seen = Arc<Mutex<Vec<(Vec<(String, String)>, Value)>>>;

/// A scripted HTTP server: answers each request with the next (status, body)
/// and records the request (headers lower-cased, JSON body).
struct Fake {
    url: String,
    seen: Seen,
}

async fn fake(responses: Vec<(u16, String)>) -> Fake {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        for (status, body) in responses {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let (head_end, len) = loop {
                let n = s.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..i]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                        .unwrap_or(0);
                    break (i + 4, len);
                }
            };
            while buf.len() < head_end + len {
                let n = s.read(&mut chunk).await.unwrap();
                buf.extend_from_slice(&chunk[..n]);
            }
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let headers = head
                .lines()
                .skip(1)
                .filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string())))
                .collect();
            let body_json: Value = serde_json::from_slice(&buf[head_end..head_end + len]).unwrap_or(Value::Null);
            log.lock().unwrap().push((headers, body_json));
            let ctype = if status == 200 { "text/event-stream" } else { "application/json" };
            let resp = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            s.write_all(resp.as_bytes()).await.unwrap();
            let _ = s.shutdown().await;
        }
    });
    Fake { url, seen }
}

fn sse(events: &[Value]) -> String {
    events.iter().map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap_or("message"))).collect()
}

fn claude(url: &str, model: &str) -> Provider {
    let mut cfg = ProviderConfig::anthropic("anthropic");
    cfg.model = model.into();
    Provider::new(cfg, Some(Secret::new("sk-ant-test-key-123456"))).unwrap().with_base_url(url)
}

fn start(model: &str) -> Value {
    json!({"type": "message_start", "message": {"model": model, "usage": {"input_tokens": 120, "cache_read_input_tokens": 100, "cache_creation_input_tokens": 0, "output_tokens": 1}}})
}

fn end(reason: &str, out: u64) -> [Value; 2] {
    [json!({"type": "message_delta", "delta": {"stop_reason": reason}, "usage": {"output_tokens": out}}), json!({"type": "message_stop"})]
}

#[tokio::test]
async fn anthropic_streams_text_thinking_and_tool_calls() {
    let mut ev = vec![
        start("claude-opus-5-5"),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": "", "signature": ""}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "Need totals."}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "sig1"}}),
        json!({"type": "content_block_stop", "index": 0}),
        json!({"type": "ping"}),
        json!({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}),
        json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "Let me "}}),
        json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "check."}}),
        json!({"type": "content_block_stop", "index": 1}),
        json!({"type": "content_block_start", "index": 2, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "query", "input": {}}}),
        json!({"type": "content_block_delta", "index": 2, "delta": {"type": "input_json_delta", "partial_json": "{\"sql\": \"SELECT "}}),
        json!({"type": "content_block_delta", "index": 2, "delta": {"type": "input_json_delta", "partial_json": "1\"}"}}),
        json!({"type": "content_block_stop", "index": 2}),
    ];
    ev.extend(end("tool_use", 42));
    let f = fake(vec![(200, sse(&ev))]).await;
    let p = claude(&f.url, "claude-opus-5-5");
    let deltas = Mutex::new(Vec::new());
    let tools = [ToolSpec { name: "query".into(), description: "Run SQL".into(), input_schema: json!({"type": "object"}) }];
    let msgs = [Message::user_text("totals?")];
    let turn = p
        .send(&Request { system: "sys", messages: &msgs, tools: &tools }, &|d| deltas.lock().unwrap().push(d))
        .await
        .unwrap();

    assert_eq!(turn.stop, Stop::ToolUse);
    assert_eq!(turn.model, "claude-opus-5-5");
    assert_eq!((turn.usage.input_tokens, turn.usage.output_tokens, turn.usage.cache_read_tokens), (120, 42, 100));
    assert_eq!(turn.content[1], Block::Text { text: "Let me check.".into() });
    assert_eq!(
        turn.content[2],
        Block::ToolUse { id: "toolu_1".into(), name: "query".into(), input: json!({"sql": "SELECT 1"}), invalid: None }
    );
    let Block::Raw { value, .. } = &turn.content[0] else { panic!() };
    assert_eq!((value["thinking"].as_str(), value["signature"].as_str()), (Some("Need totals."), Some("sig1")));
    let d = deltas.lock().unwrap();
    assert!(d.contains(&Delta::Thinking { text: "Need totals.".into() }));
    assert!(d.contains(&Delta::ToolStart { id: "toolu_1".into(), name: "query".into() }));

    // The request: headers, adaptive thinking, effort, caching, eager tools,
    // fallbacks, and no sampling parameters.
    let (headers, body) = f.seen.lock().unwrap()[0].clone();
    let h = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    assert_eq!(h("x-api-key").as_deref(), Some("sk-ant-test-key-123456"));
    assert_eq!(h("anthropic-version").as_deref(), Some("2026-10-21"));
    assert_eq!(h("anthropic-beta").as_deref(), Some("server-side-fallback-2026-07-01"));
    assert_eq!(body["thinking"], json!({"type": "adaptive", "display": "summarized"}));
    assert_eq!(body["output_config"]["effort"], "medium");
    assert_eq!(body["cache_control"]["type"], "ephemeral");
    assert_eq!(body["fallbacks"], "default");
    assert_eq!(body["tools"][0]["eager_input_streaming"], true);
    assert!(body.get("temperature").is_none() && body["stream"] == true);
}

#[tokio::test]
async fn anthropic_invalid_tool_json_refusal_and_errors() {
    let mut bad = vec![
        start("claude-haiku-5-5"),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": "t", "name": "query", "input": {}}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"sql\": \"SEL"}}),
        json!({"type": "content_block_stop", "index": 0}),
    ];
    bad.extend(end("tool_use", 5));
    let mut refusal = vec![start("claude-haiku-5-5")];
    refusal.push(json!({"type": "message_delta", "delta": {"stop_reason": "refusal", "stop_details": {"type": "refusal", "category": "cyber"}}, "usage": {"output_tokens": 0}}));
    refusal.push(json!({"type": "message_stop"}));
    let f = fake(vec![
        (200, sse(&bad)),
        (200, sse(&refusal)),
        (401, json!({"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key: sk-ant-test-key-123456"}}).to_string()),
    ])
    .await;
    let p = claude(&f.url, "claude-haiku-5-5");
    let msgs = [Message::user_text("q")];
    let req = Request { system: "s", messages: &msgs, tools: &[] };

    let t = p.send(&req, &|_| {}).await.unwrap();
    let Block::ToolUse { invalid, input, .. } = &t.content[0] else { panic!() };
    assert_eq!(invalid.as_deref(), Some("{\"sql\": \"SEL"));
    assert_eq!(input, &json!({}));
    // Haiku 5.5 has no server-side fallback: no beta header, no field.
    assert!(f.seen.lock().unwrap()[0].1.get("fallbacks").is_none());

    let t = p.send(&req, &|_| {}).await.unwrap();
    assert_eq!(t.stop, Stop::Refusal { category: Some("cyber".into()) });

    let err = p.send(&req, &|_| {}).await.unwrap_err().to_string();
    assert!(err.contains("401") && err.contains("invalid x-api-key") && !err.contains("sk-ant-test"), "{err}");
}

#[tokio::test]
async fn anthropic_retries_overload_and_echoes_fallback_history() {
    let mut ok = vec![start("claude-opus-4-8")];
    ok.push(json!({"type": "content_block_start", "index": 0, "content_block": {"type": "fallback", "from": {"model": "claude-opus-5-5"}, "to": {"model": "claude-opus-4-8"}}}));
    ok.push(json!({"type": "content_block_stop", "index": 0}));
    ok.push(json!({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": "Hi"}}));
    ok.push(json!({"type": "content_block_stop", "index": 1}));
    ok.extend(end("end_turn", 3));
    let f = fake(vec![(529, json!({"error": {"type": "overloaded_error", "message": "Overloaded"}}).to_string()), (200, sse(&ok))]).await;
    let p = claude(&f.url, "claude-opus-5-5");
    let deltas = Mutex::new(Vec::new());
    let history = [
        Message::user_text("q1"),
        Message {
            role: Role::Assistant,
            content: vec![
                Block::Raw { provider: "anthropic".into(), value: json!({"type": "thinking", "thinking": "a", "signature": "s"}) },
                Block::Text { text: "partial".into() },
                Block::ToolUse { id: "t0".into(), name: "x".into(), input: json!({}), invalid: None },
                Block::Raw { provider: "anthropic".into(), value: json!({"type": "fallback", "from": {"model": "a"}, "to": {"model": "b"}}) },
                Block::Raw { provider: "anthropic".into(), value: json!({"type": "thinking", "thinking": "b", "signature": "t"}) },
                Block::Text { text: "after".into() },
            ],
        },
        Message::user_text("q2"),
    ];
    let t = p.send(&Request { system: "s", messages: &history, tools: &[] }, &|d| deltas.lock().unwrap().push(d)).await.unwrap();
    assert_eq!(t.model, "claude-opus-4-8");
    assert!(deltas.lock().unwrap().contains(&Delta::Fallback { from: "claude-opus-5-5".into(), to: "claude-opus-4-8".into() }));
    assert_eq!(f.seen.lock().unwrap().len(), 2, "the overload was retried");

    // Before the fallback block: text kept, thinking and tool_use dropped;
    // the fallback block and everything after it kept in place.
    let sent = f.seen.lock().unwrap()[1].1["messages"][1]["content"].clone();
    let types: Vec<_> = sent.as_array().unwrap().iter().map(|b| b["type"].as_str().unwrap().to_string()).collect();
    assert_eq!(types, ["text", "fallback", "thinking", "text"]);
    assert!(anthropic::supports_fallbacks("claude-sonnet-5-5") && !anthropic::supports_fallbacks("claude-haiku-5-5"));
}

#[tokio::test]
async fn openai_compatible_streams_tool_calls_and_skips_anthropic_blocks() {
    let chunks = [
        json!({"model": "llama3.1", "choices": [{"index": 0, "delta": {"role": "assistant", "content": "Checking"}}]}),
        json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "call_a", "type": "function", "function": {"name": "query", "arguments": ""}}]}}]}),
        json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "{\"sql\":"}}]}}]}),
        json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "\"SELECT 2\"}"}}]}}]}),
        json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
        json!({"choices": [], "usage": {"prompt_tokens": 50, "completion_tokens": 9}}),
    ];
    let body: String = chunks.iter().map(|c| format!("data: {c}\n\n")).collect::<String>() + "data: [DONE]\n\n";
    let f = fake(vec![(200, body)]).await;
    let cfg = ProviderConfig::ollama("ollama", "llama3.1");
    let p = Provider::new(cfg, None).unwrap().with_base_url(format!("{}/v1", f.url));
    let history = [
        Message::user_text("first"),
        Message {
            role: Role::Assistant,
            content: vec![
                Block::Raw { provider: "anthropic".into(), value: json!({"type": "thinking"}) },
                Block::ToolUse { id: "c1".into(), name: "list_tables".into(), input: json!({}), invalid: None },
            ],
        },
        Message { role: Role::User, content: vec![Block::ToolResult { tool_use_id: "c1".into(), content: "[]".into(), is_error: false }] },
    ];
    let tools = [ToolSpec { name: "query".into(), description: "SQL".into(), input_schema: json!({"type": "object"}) }];
    let t = p.send(&Request { system: "sys", messages: &history, tools: &tools }, &|_| {}).await.unwrap();
    assert_eq!(t.stop, Stop::ToolUse);
    assert_eq!((t.usage.input_tokens, t.usage.output_tokens), (50, 9));
    assert_eq!(t.content[0], Block::Text { text: "Checking".into() });
    assert_eq!(t.content[1], Block::ToolUse { id: "call_a".into(), name: "query".into(), input: json!({"sql": "SELECT 2"}), invalid: None });

    let (headers, body) = f.seen.lock().unwrap()[0].clone();
    assert!(!headers.iter().any(|(k, _)| k == "authorization"), "local servers get no key");
    let roles: Vec<_> = body["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap().to_string()).collect();
    assert_eq!(roles, ["system", "user", "assistant", "tool"]);
    assert_eq!(body["messages"][2]["tool_calls"][0]["function"]["name"], "list_tables");
    assert_eq!(body["messages"][3]["tool_call_id"], "c1");
    assert_eq!(body["tools"][0]["function"]["name"], "query");
}

/// A tool backend that answers `query` and records calls.
struct Tools(Mutex<Vec<(String, Value)>>);

impl ToolBackend for Tools {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![ToolSpec { name: "query".into(), description: "SQL".into(), input_schema: json!({"type": "object"}) }]
    }
    fn call<'a>(&'a self, name: &'a str, input: Value) -> Pin<Box<dyn Future<Output = ToolOutput> + Send + 'a>> {
        self.0.lock().unwrap().push((name.to_string(), input));
        Box::pin(async { ToolOutput { text: "{\"rows\":[[\"15\"]]}".into(), is_error: false } })
    }
}

fn tool_turn(id: &str) -> String {
    let mut ev = vec![
        start("claude-opus-5-5"),
        json!({"type": "content_block_start", "index": 0, "content_block": {"type": "tool_use", "id": id, "name": "query", "input": {}}}),
        json!({"type": "content_block_delta", "index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"sql\":\"SELECT 1\"}"}}),
        json!({"type": "content_block_stop", "index": 0}),
    ];
    ev.extend(end("tool_use", 10));
    sse(&ev)
}

#[tokio::test]
async fn agent_runs_tools_until_the_answer() {
    let mut answer = vec![start("claude-opus-5-5")];
    answer.push(json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": "North sold 15."}}));
    answer.push(json!({"type": "content_block_stop", "index": 0}));
    answer.extend(end("end_turn", 6));
    let f = fake(vec![(200, tool_turn("t1")), (200, sse(&answer))]).await;
    let p = claude(&f.url, "claude-opus-5-5");
    let tools = Tools(Mutex::new(Vec::new()));
    let agent = Agent { provider: &p, tools: &tools, system: "You help.".into(), max_tool_calls: 5 };
    let events = Mutex::new(Vec::new());
    let mut history = Vec::new();
    let stop = agent.ask(&mut history, Message::user_text("North total?"), &|e| events.lock().unwrap().push(e)).await.unwrap();

    assert_eq!(stop, Stop::EndTurn);
    assert_eq!(tools.0.lock().unwrap()[0], ("query".to_string(), json!({"sql": "SELECT 1"})));
    assert_eq!(history.len(), 4, "question, tool call, tool result, answer");
    assert_eq!(history[3].text(), "North sold 15.");
    // The tool result went back to the model in the second request.
    let second = f.seen.lock().unwrap()[1].1.clone();
    assert_eq!(second["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(second["messages"][2]["content"][0]["tool_use_id"], "t1");
    let ev = events.lock().unwrap();
    assert!(ev.iter().any(|e| matches!(e, AgentEvent::ToolCall { name, .. } if name == "query")));
    assert!(ev.iter().any(|e| matches!(e, AgentEvent::Usage { cost_usd: Some(_), .. })));
    assert!(matches!(ev.last(), Some(AgentEvent::Done { stop: Stop::EndTurn })));
}

#[tokio::test]
async fn agent_stops_at_the_tool_limit() {
    let f = fake(vec![(200, tool_turn("a")), (200, tool_turn("b")), (200, tool_turn("c"))]).await;
    let p = claude(&f.url, "claude-opus-5-5");
    let tools = Tools(Mutex::new(Vec::new()));
    let agent = Agent { provider: &p, tools: &tools, system: "s".into(), max_tool_calls: 1 };
    let mut history = Vec::new();
    let stop = agent.ask(&mut history, Message::user_text("loop"), &|_| {}).await.unwrap();
    assert_eq!(stop, Stop::Other { reason: "tool_limit".into() });
    assert_eq!(tools.0.lock().unwrap().len(), 1, "only the allowed call ran");
}

#[test]
fn secrets_never_print() {
    let s = Secret::new("sk-ant-very-secret-1234");
    assert_eq!(format!("{s:?}"), "***");
    let p = Provider::new(ProviderConfig::anthropic("a"), None);
    assert!(p.is_err(), "Anthropic needs a key");
    let mut ollama = ProviderConfig::ollama("o", "m");
    ollama.base_url = None;
    assert!(Provider::new(ollama, None).is_err(), "OpenAI-compatible needs a base URL");
}
