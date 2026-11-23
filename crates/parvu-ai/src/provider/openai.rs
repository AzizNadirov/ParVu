//! OpenAI Chat Completions format (`POST {base}/chat/completions`, streamed):
//! OpenAI, Gemini's OpenAI endpoint, OpenRouter, Groq, Ollama, LM Studio and
//! llama.cpp. Anthropic-specific blocks in the history are skipped.

use std::collections::BTreeMap;

use futures_util::StreamExt;
use serde_json::{Value, json};

use super::{Provider, Request, post_sse};
use crate::error::{AiError, Result};
use crate::types::{Block, Delta, Message, Role, Stop, Turn, Usage};

/// The request body (also used by tests).
pub fn request_body(p: &Provider, req: &Request<'_>) -> Value {
    let mut messages = vec![json!({ "role": "system", "content": req.system })];
    for m in req.messages {
        messages.extend(message_json(m));
    }
    let mut body = json!({
        "model": p.config.model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": { "name": t.name, "description": t.description, "parameters": t.input_schema },
                })
            })
            .collect();
    }
    body
}

/// One neutral message becomes one or more chat messages (tool results are
/// separate `tool` messages).
fn message_json(m: &Message) -> Vec<Value> {
    match m.role {
        Role::User => {
            let mut out = Vec::new();
            let mut text = String::new();
            for b in &m.content {
                match b {
                    Block::Text { text: t } => text.push_str(t),
                    Block::ToolResult { tool_use_id, content, is_error } => out.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_use_id,
                        "content": if *is_error { format!("Error: {content}") } else { content.clone() },
                    })),
                    _ => {}
                }
            }
            if !text.is_empty() {
                out.push(json!({ "role": "user", "content": text }));
            }
            out
        }
        Role::Assistant => {
            let text = m.text();
            let calls: Vec<Value> = m
                .content
                .iter()
                .filter_map(|b| match b {
                    Block::ToolUse { id, name, input, extra, .. } => {
                        let mut call = json!({
                            "id": id,
                            "type": "function",
                            "function": { "name": name, "arguments": input.to_string() },
                        });
                        // Gemini 3 refuses a history whose function calls lost
                        // their `thought_signature` (in `extra_content`).
                        if let Some(extra) = extra {
                            call["extra_content"] = extra.clone();
                        }
                        Some(call)
                    }
                    _ => None,
                })
                .collect();
            let mut msg = json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { Value::String(text) } });
            if !calls.is_empty() {
                msg["tool_calls"] = Value::Array(calls);
            }
            vec![msg]
        }
    }
}

#[derive(Default)]
struct Call {
    id: String,
    name: String,
    args: String,
    /// `extra_content` of the call (Gemini: `{"google": {"thought_signature": …}}`).
    extra: Option<Value>,
}

pub(super) async fn send(p: &Provider, req: &Request<'_>, on_delta: &(dyn Fn(Delta) + Send + Sync)) -> Result<Turn> {
    let mut headers = Vec::new();
    if let Some(k) = &p.key {
        headers.push(("authorization", format!("Bearer {}", k.expose())));
    }
    let url = format!("{}/chat/completions", p.base("https://api.openai.com/v1"));
    let body = request_body(p, req);
    let mut events = Box::pin(post_sse(p, &url, &headers, &body).await?);
    let protocol = |m: String| AiError::Protocol { provider: p.config.display_name.clone(), message: m };

    let mut text = String::new();
    let mut calls: BTreeMap<u64, Call> = BTreeMap::new();
    let mut usage = Usage::default();
    let mut model = p.config.model.clone();
    let mut finish: Option<String> = None;
    let mut done = false;

    while let Some(ev) = events.next().await {
        let ev = ev.map_err(protocol)?;
        let data = ev.data.trim();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            done = true;
            break;
        }
        let v: Value = serde_json::from_str(data).map_err(|e| protocol(e.to_string()))?;
        if let Some(msg) = v["error"]["message"].as_str() {
            return Err(AiError::Api { provider: p.config.display_name.clone(), status: 500, message: msg.to_string() });
        }
        if let Some(m) = v["model"].as_str() {
            model = m.to_string();
        }
        if v["usage"].is_object() {
            usage.input_tokens = v["usage"]["prompt_tokens"].as_u64().unwrap_or(usage.input_tokens);
            usage.output_tokens = v["usage"]["completion_tokens"].as_u64().unwrap_or(usage.output_tokens);
            usage.cache_read_tokens =
                v["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or(usage.cache_read_tokens);
        }
        let Some(choice) = v["choices"].get(0) else { continue };
        let d = &choice["delta"];
        if let Some(s) = d["content"].as_str()
            && !s.is_empty()
        {
            text.push_str(s);
            on_delta(Delta::Text { text: s.to_string() });
        }
        if let Some(tcs) = d["tool_calls"].as_array() {
            for tc in tcs {
                let idx = tc["index"].as_u64().unwrap_or(calls.len() as u64);
                let c = calls.entry(idx).or_default();
                if let Some(id) = tc["id"].as_str() {
                    c.id = id.to_string();
                }
                if let Some(n) = tc["function"]["name"].as_str() {
                    let first = c.name.is_empty();
                    c.name.push_str(n);
                    if first {
                        on_delta(Delta::ToolStart { id: c.id.clone(), name: c.name.clone() });
                    }
                }
                c.args.push_str(tc["function"]["arguments"].as_str().unwrap_or(""));
                if let Some(e) = tc.get("extra_content").filter(|e| !e.is_null()) {
                    merge(c.extra.get_or_insert_with(|| json!({})), e);
                }
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            finish = Some(f.to_string());
        }
    }
    if !done && finish.is_none() {
        return Err(protocol("the stream ended early".into()));
    }

    let mut content = Vec::new();
    if !text.is_empty() {
        content.push(Block::Text { text });
    }
    for (i, c) in calls {
        let id = if c.id.is_empty() { format!("call_{i}") } else { c.id };
        let src = if c.args.trim().is_empty() { "{}" } else { c.args.as_str() };
        content.push(match serde_json::from_str::<Value>(src) {
            Ok(input @ Value::Object(_)) => Block::ToolUse { id, name: c.name, input, invalid: None, extra: c.extra },
            _ => Block::ToolUse { id, name: c.name, input: json!({}), invalid: Some(c.args), extra: c.extra },
        });
    }
    let has_calls = content.iter().any(|b| matches!(b, Block::ToolUse { .. }));
    let stop = match finish.as_deref() {
        Some("tool_calls") | Some("function_call") => Stop::ToolUse,
        _ if has_calls => Stop::ToolUse,
        Some("length") => Stop::MaxTokens,
        Some("content_filter") => Stop::Refusal { category: None },
        Some("stop") | None => Stop::EndTurn,
        Some(other) => Stop::Other { reason: other.to_string() },
    };
    Ok(Turn { content, stop, usage, model })
}

/// Merge a streamed `extra_content` piece into what was received so far
/// (objects key by key; anything else replaces).
fn merge(into: &mut Value, piece: &Value) {
    match (into, piece) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (slot, v) => *slot = v.clone(),
    }
}
