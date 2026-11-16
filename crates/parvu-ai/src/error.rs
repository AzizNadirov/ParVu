use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiError {
    /// The provider answered with an error status.
    #[error("{provider} error ({status}): {message}")]
    Api { provider: String, status: u16, message: String },
    /// The provider could not be reached.
    #[error("could not reach {provider}: {message}")]
    Network { provider: String, message: String },
    /// The stream ended or contained something we could not read.
    #[error("unexpected answer from {provider}: {message}")]
    Protocol { provider: String, message: String },
    #[error("{0}")]
    Config(String),
}

impl AiError {
    /// Worth retrying after a pause: rate limits, overload, server errors.
    pub fn retryable(&self) -> bool {
        match self {
            AiError::Api { status, .. } => matches!(status, 408 | 409 | 429 | 500..=599),
            AiError::Network { .. } => true,
            _ => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, AiError>;
