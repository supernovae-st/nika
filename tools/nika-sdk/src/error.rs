//! SDK error types.
//!
//! `SdkError` is the only error type exposed by this crate.
//! Internal dependencies (`NikaError`, `reqwest::Error`) are converted
//! explicitly in each transport — never via blanket `#[from]`.

use std::time::Duration;

#[derive(thiserror::Error, Debug)]
#[must_use]
pub enum SdkError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("HTTP error {status}: {message}")]
    Http {
        status: u16,
        message: String,
        body: Option<String>,
    },

    #[error("connection failed: {0}")]
    Connection(String),

    #[error("request timeout after {0:?}")]
    Timeout(Duration),

    #[error("engine error: {message}")]
    Engine {
        message: String,
        code: Option<String>,
    },

    #[error("job not found: {0}")]
    NotFound(String),

    #[error("server queue full")]
    QueueFull,

    #[error("invalid workflow: {0}")]
    InvalidWorkflow(String),

    #[error("authentication failed")]
    Unauthorized,

    #[error("job cancelled")]
    Cancelled,

    #[error("event stream closed unexpectedly")]
    StreamClosed,

    #[error("event deserialization failed: {0}")]
    EventParse(String),

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_http() {
        let err = SdkError::Http {
            status: 429,
            message: "rate limited".into(),
            body: None,
        };
        assert_eq!(err.to_string(), "HTTP error 429: rate limited");
    }

    #[test]
    fn error_display_engine() {
        let err = SdkError::Engine {
            message: "DAG cycle".into(),
            code: Some("NIKA-020".into()),
        };
        assert_eq!(err.to_string(), "engine error: DAG cycle");
    }

    #[test]
    fn error_display_timeout() {
        let err = SdkError::Timeout(Duration::from_secs(30));
        assert_eq!(err.to_string(), "request timeout after 30s");
    }

    #[test]
    fn error_display_not_found() {
        let err = SdkError::NotFound("job-abc".into());
        assert_eq!(err.to_string(), "job not found: job-abc");
    }

    #[test]
    fn error_display_cancelled() {
        let err = SdkError::Cancelled;
        assert_eq!(err.to_string(), "job cancelled");
    }

    #[test]
    fn error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        // SdkError contains Box<dyn Error + Send + Sync> so it's Send + Sync
        assert_send_sync::<SdkError>();
    }
}
