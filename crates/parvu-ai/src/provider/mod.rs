//! Model providers. Two wire formats cover every provider ParVu supports:
//! Anthropic's Messages API ([`anthropic`]) and the OpenAI Chat Completions
//! format ([`openai`]), which OpenAI, Gemini, OpenRouter, Groq, Ollama and
//! LM Studio all speak.

pub mod anthropic;
pub mod openai;

use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::error::{AiError, Result};
use crate::types::{Delta, Message, ToolSpec, Turn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    OpenAiCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    #[default]
    Medium,
    High,
    Xhigh,
    Max,
}

/// A configured way to reach a model. Never holds the key itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Unique id: "anthropic", "ollama-laptop".
    pub id: String,
    pub kind: ProviderKind,
    pub display_name: String,
    pub model: String,
    /// Required for OpenAI-compatible providers ("http://localhost:11434/v1").
    #[serde(default)]
    pub base_url: Option<String>,
    /// Environment variable holding the key when no keychain entry exists.
    #[serde(default)]
    pub key_env: Option<String>,
    #[serde(default = "default_max_output")]
    pub max_output_tokens: u32,
    /// Anthropic: `output_config.effort`.
    #[serde(default)]
    pub effort: Effort,
    /// Anthropic: retry a declined request on Anthropic's fallback model.
    #[serde(default = "yes")]
    pub server_fallback: bool,
    #[serde(default = "default_timeout")]
    pub request_timeout_s: u64,
}

fn default_max_output() -> u32 {
    16_000
}
fn yes() -> bool {
    true
}
fn default_timeout() -> u64 {
    300
}

impl ProviderConfig {
    /// A new Anthropic provider with the recommended defaults.
    pub fn anthropic(id: &str) -> Self {
        Self {
            id: id.into(),
            kind: ProviderKind::Anthropic,
            display_name: "Anthropic".into(),
            model: "claude-opus-5-5".into(),
            base_url: None,
            key_env: Some("ANTHROPIC_API_KEY".into()),
            max_output_tokens: default_max_output(),
            effort: Effort::Medium,
            server_fallback: true,
            request_timeout_s: default_timeout(),
        }
    }

    /// A local Ollama server.
    pub fn ollama(id: &str, model: &str) -> Self {
        Self {
            id: id.into(),
            kind: ProviderKind::OpenAiCompatible,
            display_name: "Ollama".into(),
            model: model.into(),
            base_url: Some("http://localhost:11434/v1".into()),
            key_env: None,
            max_output_tokens: default_max_output(),
            effort: Effort::Medium,
            server_fallback: false,
            request_timeout_s: default_timeout(),
        }
    }
}

/// An API key. Prints as `***` so it can't end up in logs by accident.
#[derive(Clone, Default)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self {
        Secret(s.into())
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

/// What one model call needs.
pub struct Request<'a> {
    pub system: &'a str,
    pub messages: &'a [Message],
    pub tools: &'a [ToolSpec],
}

/// A provider ready to call: its configuration, key and HTTP client.
pub struct Provider {
    pub config: ProviderConfig,
    key: Option<Secret>,
    http: reqwest::Client,
    /// Overrides the endpoint (tests).
    base_override: Option<String>,
}

impl Provider {
    pub fn new(config: ProviderConfig, key: Option<Secret>) -> Result<Self> {
        crate::init_tls();
        if config.kind == ProviderKind::OpenAiCompatible && config.base_url.as_deref().is_none_or(str::is_empty) {
            return Err(AiError::Config(format!("{}: an OpenAI-compatible provider needs a base URL", config.display_name)));
        }
        if config.kind == ProviderKind::Anthropic && key.as_ref().is_none_or(Secret::is_empty) {
            return Err(AiError::Config(format!("{}: no API key", config.display_name)));
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(config.request_timeout_s))
            .user_agent(concat!("ParVu/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AiError::Config(e.to_string()))?;
        Ok(Self { config, key: key.filter(|k| !k.is_empty()), http, base_override: None })
    }

    /// Send requests to `url` instead (tests).
    #[doc(hidden)]
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_override = Some(url.into());
        self
    }

    /// One model call, streamed. `on_delta` sees text, thinking and tool
    /// calls as they arrive. Retries rate limits and server errors (up to
    /// two times) before anything has streamed.
    pub async fn send(&self, req: &Request<'_>, on_delta: &(dyn Fn(Delta) + Send + Sync)) -> Result<Turn> {
        let mut attempt = 0;
        loop {
            let result = match self.config.kind {
                ProviderKind::Anthropic => anthropic::send(self, req, on_delta).await,
                ProviderKind::OpenAiCompatible => openai::send(self, req, on_delta).await,
            };
            match result {
                Err(e) if e.retryable() && attempt < 2 => {
                    attempt += 1;
                    let wait = Duration::from_millis(800 * 2u64.pow(attempt));
                    tracing::warn!("{} failed ({e}); retrying in {wait:?}", self.config.display_name);
                    tokio::time::sleep(wait).await;
                }
                other => return other,
            }
        }
    }

    /// The smallest useful request, to check a provider's settings.
    pub async fn test(&self) -> Result<String> {
        let messages = [Message::user_text("Reply with the single word: ok")];
        let req = Request { system: "You are a connection test.", messages: &messages, tools: &[] };
        let turn = self.send(&req, &|_| {}).await?;
        Ok(turn.model)
    }

    fn base(&self, default: &str) -> String {
        self.base_override
            .clone()
            .or_else(|| self.config.base_url.clone())
            .unwrap_or_else(|| default.to_string())
            .trim_end_matches('/')
            .to_string()
    }
}

/// POST `body` and return the response's server-sent events, or the
/// provider's error.
async fn post_sse(
    p: &Provider,
    url: &str,
    headers: &[(&str, String)],
    body: &serde_json::Value,
) -> Result<impl futures_util::Stream<Item = std::result::Result<eventsource_stream::Event, String>>> {
    use eventsource_stream::Eventsource;
    let name = p.config.display_name.clone();
    let mut rb = p.http.post(url).json(body);
    for (k, v) in headers {
        rb = rb.header(*k, v);
    }
    let resp = rb.send().await.map_err(|e| AiError::Network { provider: name.clone(), message: e.to_string() })?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(AiError::Api { provider: name, status: status.as_u16(), message: error_message(&text) });
    }
    Ok(resp.bytes_stream().eventsource().map(|r| r.map_err(|e| e.to_string())))
}

/// The human part of a provider's error body (`{"error": {"message": …}}`).
fn error_message(body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let m = v["error"]["message"].as_str().or_else(|| v["message"].as_str()).or_else(|| v["error"].as_str());
    let text = m.map(str::to_string).unwrap_or_else(|| body.chars().take(300).collect());
    crate::redact::redact(&text)
}
