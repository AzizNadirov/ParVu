//! Anthropic Messages API (`POST /v1/messages`, streamed).
//!
//! * API version `2026-10-21`: no sampling parameters.
//! * Adaptive thinking with summarized display; thinking blocks are kept as
//!   [`Block::Raw`] and sent back unchanged.
//! * `output_config.effort` from the provider settings.
//! * Tools stream their input eagerly (`eager_input_streaming`), so the
//!   input is parsed here and invalid JSON is reported to the agent.
//! * Top-level automatic prompt caching.
//! * Server-side refusal fallback (`fallbacks: "default"`) on models that
//!   support it, when enabled.

use futures_util::StreamExt;
use serde_json::{Value, json};

use super::{Provider, Request, post_sse};
use crate::error::{AiError, Result};
use crate::types::{Block, Delta, Message, Role, Stop, Turn, Usage};

pub const API_VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const PROVIDER: &str = "anthropic";

/// Models that accept `fallbacks: "default"`.
pub fn supports_fallbacks(model: &str) -> bool {
    ["claude-opus-5-5", "claude-opus-5", "claude-sonnet-5-5", "claude-fable-", "claude-mythos-"]
        .iter()
        .any(|m| if m.ends_with('-') { model.starts_with(m) } else { model == *m })
}

/// The request body (also used by tests).
pub fn request_body(p: &Provider, req: &Request<'_>) -> Value {
    let cfg = &p.config;
    let mut body = json!({
        "model": cfg.model,
        "max_tokens": cfg.max_output_tokens,
        "stream": true,
        "system": [{ "type": "text", "text": req.system }],
        "messages": req.messages.iter().map(message_json).collect::<Vec<_>>(),
        "thinking": { "type": "adaptive", "display": "summarized" },
        "output_config": { "effort": cfg.effort },
        "cache_control": { "type": "ephemeral" },
    });
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                    "eager_input_streaming": true,
                })
            })
            .collect();
    }
    if use_fallbacks(p) {
        body["fallbacks"] = json!("default");
    }
    body
}

fn use_fallbacks(p: &Provider) -> bool {
    p.config.server_fallback && supports_fallbacks(&p.config.model)
}

fn message_json(m: &Message) -> Value {
    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    // After a mid-output fallback, thinking and tool_use blocks before the
    // last `fallback` block are not sent back; the fallback block itself
    // stays in place.
    let last_fallback = m
        .content
        .iter()
        .rposition(|b| matches!(b, Block::Raw { provider, value } if provider == PROVIDER && value["type"] == "fallback"));
    let content: Vec<Value> = m
        .content
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let before_fallback = last_fallback.is_some_and(|f| i < f);
            match b {
                Block::Text { text } => Some(json!({ "type": "text", "text": text })),
                Block::ToolUse { .. } if before_fallback => None,
                Block::ToolUse { id, name, input, .. } => {
                    Some(json!({ "type": "tool_use", "id": id, "name": name, "input": input }))
                }
                Block::ToolResult { tool_use_id, content, is_error } => Some(json!({
                    "type": "tool_result",
                    "tool_use_id": tool_use_id,
                    "content": content,
                    "is_error": is_error,
                })),
                Block::Raw { provider, value } if provider == PROVIDER => {
                    let t = value["type"].as_str().unwrap_or("");
                    if before_fallback && t != "fallback" && t != "text" {
                        None
                    } else {
                        Some(value.clone())
                    }
                }
                Block::Raw { .. } => None,
            }
        })
        .collect();
    json!({ "role": role, "content": content })
}

enum Part {
    Text(String),
    Tool { id: String, name: String, json: String },
    /// thinking / redacted_thinking / fallback / anything else: kept raw.
    Other(Value),
}

pub(super) async fn send(p: &Provider, req: &Request<'_>, on_delta: &(dyn Fn(Delta) + Send + Sync)) -> Result<Turn> {
    let key = p.key.as_ref().map(|k| k.expose().to_string()).unwrap_or_default();
    let mut headers = vec![("x-api-key", key), ("anthropic-version", API_VERSION.to_string())];
    if use_fallbacks(p) {
        headers.push(("anthropic-beta", FALLBACK_BETA.to_string()));
    }
    let url = format!("{}/v1/messages", p.base("https://api.anthropic.com"));
    let body = request_body(p, req);
    let mut events = Box::pin(post_sse(p, &url, &headers, &body).await?);

    let protocol = |m: String| AiError::Protocol { provider: p.config.display_name.clone(), message: m };
    let mut parts: Vec<Option<Part>> = Vec::new();
    let mut usage = Usage::default();
    let mut model = p.config.model.clone();
    let mut stop = None;
    let mut finished = false;

    while let Some(ev) = events.next().await {
        let ev = ev.map_err(protocol)?;
        if ev.data.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(&ev.data).map_err(|e| protocol(e.to_string()))?;
        match v["type"].as_str().unwrap_or(&ev.event) {
            "message_start" => {
                let m = &v["message"];
                if let Some(name) = m["model"].as_str() {
                    model = name.to_string();
                }
                read_usage(&m["usage"], &mut usage);
            }
            "content_block_start" => {
                let i = v["index"].as_u64().unwrap_or(parts.len() as u64) as usize;
                let b = &v["content_block"];
                let part = match b["type"].as_str().unwrap_or("") {
                    "text" => Part::Text(b["text"].as_str().unwrap_or("").to_string()),
                    "tool_use" => {
                        let id = b["id"].as_str().unwrap_or("").to_string();
                        let name = b["name"].as_str().unwrap_or("").to_string();
                        on_delta(Delta::ToolStart { id: id.clone(), name: name.clone() });
                        Part::Tool { id, name, json: String::new() }
                    }
                    "fallback" => {
                        on_delta(Delta::Fallback {
                            from: b["from"]["model"].as_str().unwrap_or("").to_string(),
                            to: b["to"]["model"].as_str().unwrap_or("").to_string(),
                        });
                        Part::Other(b.clone())
                    }
                    _ => Part::Other(b.clone()),
                };
                if parts.len() <= i {
                    parts.resize_with(i + 1, || None);
                }
                parts[i] = Some(part);
            }
            "content_block_delta" => {
                let i = v["index"].as_u64().unwrap_or(0) as usize;
                let d = &v["delta"];
                match (parts.get_mut(i).and_then(Option::as_mut), d["type"].as_str().unwrap_or("")) {
                    (Some(Part::Text(t)), "text_delta") => {
                        let s = d["text"].as_str().unwrap_or("");
                        t.push_str(s);
                        on_delta(Delta::Text { text: s.to_string() });
                    }
                    (Some(Part::Tool { json, .. }), "input_json_delta") => {
                        json.push_str(d["partial_json"].as_str().unwrap_or(""));
                    }
                    (Some(Part::Other(b)), "thinking_delta") => {
                        let s = d["thinking"].as_str().unwrap_or("");
                        let cur = b["thinking"].as_str().unwrap_or("").to_string();
                        b["thinking"] = Value::String(cur + s);
                        if !s.is_empty() {
                            on_delta(Delta::Thinking { text: s.to_string() });
                        }
                    }
                    (Some(Part::Other(b)), "signature_delta") => {
                        let cur = b["signature"].as_str().unwrap_or("").to_string();
                        b["signature"] = Value::String(cur + d["signature"].as_str().unwrap_or(""));
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                read_usage(&v["usage"], &mut usage);
                if let Some(r) = v["delta"]["stop_reason"].as_str() {
                    stop = Some(match r {
                        "end_turn" | "stop_sequence" => Stop::EndTurn,
                        "tool_use" => Stop::ToolUse,
                        "max_tokens" => Stop::MaxTokens,
                        "refusal" => Stop::Refusal {
                            category: v["delta"]["stop_details"]["category"].as_str().map(str::to_string),
                        },
                        other => Stop::Other { reason: other.to_string() },
                    });
                }
            }
            "message_stop" => {
                finished = true;
                break;
            }
            "error" => {
                let message = v["error"]["message"].as_str().unwrap_or("stream error").to_string();
                let status = match v["error"]["type"].as_str() {
                    Some("overloaded_error") => 529,
                    Some("rate_limit_error") => 429,
                    Some("api_error") => 500,
                    _ => 400,
                };
                return Err(AiError::Api { provider: p.config.display_name.clone(), status, message });
            }
            _ => {} // ping and future event types
        }
    }
    if !finished {
        return Err(protocol("the stream ended early".into()));
    }

    let content = parts
        .into_iter()
        .flatten()
        .map(|part| match part {
            Part::Text(text) => Block::Text { text },
            Part::Tool { id, name, json } => {
                let src = if json.trim().is_empty() { "{}" } else { json.as_str() };
                match serde_json::from_str::<Value>(src) {
                    Ok(input @ Value::Object(_)) => Block::ToolUse { id, name, input, invalid: None },
                    _ => Block::ToolUse { id, name, input: json!({}), invalid: Some(json) },
                }
            }
            Part::Other(value) => Block::Raw { provider: PROVIDER.into(), value },
        })
        .collect();
    Ok(Turn { content, stop: stop.unwrap_or(Stop::Other { reason: "none".into() }), usage, model })
}

/// Usage fields seen so far (message_start carries input, message_delta the
/// running output count).
fn read_usage(u: &Value, usage: &mut Usage) {
    let get = |k: &str| u[k].as_u64();
    if let Some(n) = get("input_tokens") {
        usage.input_tokens = n;
    }
    if let Some(n) = get("output_tokens") {
        usage.output_tokens = n;
    }
    if let Some(n) = get("cache_read_input_tokens") {
        usage.cache_read_tokens = n;
    }
    if let Some(n) = get("cache_creation_input_tokens") {
        usage.cache_write_tokens = n;
    }
}
