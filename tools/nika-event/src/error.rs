use thiserror::Error;

/// Errors that can occur in the event system.
#[derive(Debug, Error)]
pub enum EventError {
    #[error("Failed to write trace: {0}")]
    TraceWrite(#[from] std::io::Error),

    #[error("Failed to serialize event: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result type for event operations.
pub type Result<T> = std::result::Result<T, EventError>;
