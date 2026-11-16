//! The provider-neutral conversation: what the agent loop keeps and what
//! each provider module translates to and from its wire format.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
        /// The raw input when it was not valid JSON (streamed tool input is
        /// not validated by the provider); `input` is then `{}`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        invalid: Option<String>,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    /// A provider-specific block that must be sent back unchanged in later
    /// requests to the same provider (Anthropic thinking and fallback
    /// blocks). Other providers skip it.
    Raw {
        provider: String,
        value: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Block>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Message { role: Role::User, content: vec![Block::Text { text: text.into() }] }
    }

    /// All text blocks joined.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                Block::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema of the input object.
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Stop {
    EndTurn,
    ToolUse,
    MaxTokens,
    /// The provider declined (Anthropic `refusal`, OpenAI `content_filter`).
    Refusal { category: Option<String> },
    Other { reason: String },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, o: &Usage) {
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.cache_read_tokens += o.cache_read_tokens;
        self.cache_write_tokens += o.cache_write_tokens;
    }
}

/// Live pieces of a turn, for showing it as it streams.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    Text { text: String },
    /// Summarised reasoning (Anthropic thinking with `display: "summarized"`).
    Thinking { text: String },
    /// The model started a tool call.
    ToolStart { id: String, name: String },
    /// Anthropic's server-side fallback switched models.
    Fallback { from: String, to: String },
}

/// One model response.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub content: Vec<Block>,
    pub stop: Stop,
    pub usage: Usage,
    /// The model that produced it (after any fallback).
    pub model: String,
}
