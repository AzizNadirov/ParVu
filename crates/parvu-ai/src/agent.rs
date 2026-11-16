//! The agent loop: send the conversation, run the tools the model asks for,
//! send the results back, repeat until the model answers (plans/AI_PLAN.md
//! §4.3). Changes to data go through the tools themselves (in the app: an
//! approval card), so the loop has no special cases for them.

use std::future::Future;
use std::pin::Pin;

use serde::Serialize;
use serde_json::{Value, json};

use crate::error::Result;
use crate::provider::{Provider, Request};
use crate::types::{Block, Delta, Message, Role, Stop, ToolSpec, Usage};

/// A tool's answer: text for the model, and whether it is an error.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

/// Where tools come from (in the app: the attached MCP server).
pub trait ToolBackend: Send + Sync {
    fn specs(&self) -> Vec<ToolSpec>;
    fn call<'a>(&'a self, name: &'a str, input: Value) -> Pin<Box<dyn Future<Output = ToolOutput> + Send + 'a>>;
}

/// What the UI hears while the agent works.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Delta { delta: Delta },
    ToolCall { id: String, name: String, input: Value },
    ToolResult { id: String, preview: String, is_error: bool },
    /// After each model call: its usage and the running cost (None when the
    /// model has no known price).
    Usage { usage: Usage, model: String, cost_usd: Option<f64> },
    /// The turn ended; `stop` says how.
    Done { stop: Stop },
}

pub struct Agent<'a> {
    pub provider: &'a Provider,
    pub tools: &'a dyn ToolBackend,
    pub system: String,
    /// Tool calls allowed for one question.
    pub max_tool_calls: usize,
}

/// Longest tool result sent to the model, in characters.
const MAX_RESULT_CHARS: usize = 24_000;

impl Agent<'_> {
    /// Answer `question`, appending everything to `history`. Returns how the
    /// final turn stopped.
    pub async fn ask(
        &self,
        history: &mut Vec<Message>,
        question: Message,
        on_event: &(dyn Fn(AgentEvent) + Send + Sync),
    ) -> Result<Stop> {
        history.push(question);
        let specs = self.tools.specs();
        let mut calls = 0usize;
        let mut nudged = false;
        loop {
            let req = Request { system: &self.system, messages: history, tools: &specs };
            let turn = self.provider.send(&req, &|d| on_event(AgentEvent::Delta { delta: d })).await?;
            on_event(AgentEvent::Usage {
                usage: turn.usage,
                cost_usd: crate::pricing::cost_usd(&turn.model, &turn.usage),
                model: turn.model.clone(),
            });
            let tool_uses: Vec<(String, String, Value, Option<String>)> = turn
                .content
                .iter()
                .filter_map(|b| match b {
                    Block::ToolUse { id, name, input, invalid } => Some((id.clone(), name.clone(), input.clone(), invalid.clone())),
                    _ => None,
                })
                .collect();
            let stop = turn.stop.clone();
            history.push(Message { role: Role::Assistant, content: turn.content });

            // A refusal or a cut-off turn can end mid tool call: never run
            // that turn's tools.
            if stop != Stop::ToolUse || tool_uses.is_empty() {
                let stop = if stop == Stop::ToolUse { Stop::EndTurn } else { stop };
                on_event(AgentEvent::Done { stop: stop.clone() });
                return Ok(stop);
            }

            let mut results = Vec::new();
            for (id, name, input, invalid) in tool_uses {
                on_event(AgentEvent::ToolCall { id: id.clone(), name: name.clone(), input: input.clone() });
                calls += 1;
                let out = if let Some(raw) = invalid {
                    ToolOutput { text: json!({ "INVALID_JSON": raw }).to_string(), is_error: true }
                } else if calls > self.max_tool_calls {
                    ToolOutput {
                        text: format!(
                            "Tool call limit reached ({} calls for this question). Answer with what you have, \
                             or ask the user to continue.",
                            self.max_tool_calls
                        ),
                        is_error: true,
                    }
                } else {
                    self.tools.call(&name, input).await
                };
                let text = clip(&out.text, MAX_RESULT_CHARS);
                on_event(AgentEvent::ToolResult { id: id.clone(), preview: clip(&text, 400), is_error: out.is_error });
                results.push(Block::ToolResult { tool_use_id: id, content: text, is_error: out.is_error });
            }
            // Every result goes back in one message.
            history.push(Message { role: Role::User, content: results });
            if calls > self.max_tool_calls {
                if nudged {
                    on_event(AgentEvent::Done { stop: Stop::Other { reason: "tool_limit".into() } });
                    return Ok(Stop::Other { reason: "tool_limit".into() });
                }
                nudged = true;
            }
        }
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push_str(" …[truncated]");
        out
    }
}
