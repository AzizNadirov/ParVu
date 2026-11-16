//! ParVu's built-in AI assistant (plans/AI_PLAN.md §4): model providers, the
//! agent loop, pricing and redaction. No DuckDB and no Tauri: the tools come
//! from a [`ToolBackend`] (in the app, the attached MCP server in-process),
//! and keys are resolved by the caller.

pub mod agent;
pub mod error;
pub mod pricing;
pub mod provider;
pub mod redact;
pub mod types;

pub use agent::{Agent, AgentEvent, ToolBackend, ToolOutput};
pub use error::{AiError, Result};
pub use provider::{Provider, ProviderConfig, ProviderKind, Secret};
pub use types::{Block, Delta, Message, Role, Stop, ToolSpec, Turn, Usage};

/// Install the TLS crypto provider (ring) once per process. Safe to call
/// more than once.
pub fn init_tls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
