//! Error types for nika-init crate.

/// Errors from the init/course/showcase subsystem.
#[derive(Debug, thiserror::Error)]
pub enum NikaInitError {
    /// I/O error during file operations (NIKA-093 equivalent)
    #[error("NIKA-093: I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Configuration error — TOML parse/serialize failures (NIKA-135 equivalent)
    #[error("NIKA-135: Config error: {reason}")]
    ConfigError { reason: String },

    /// Validation error — e.g. "course already exists" (NIKA-004 equivalent)
    #[error("NIKA-004: Validation error: {reason}")]
    ValidationError { reason: String },
}

/// Convenience Result alias.
pub type Result<T> = std::result::Result<T, NikaInitError>;
