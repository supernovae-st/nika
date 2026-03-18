//! Media pipeline error types (NIKA-251..259)
//!
//! Derives both `thiserror::Error` and `miette::Diagnostic` so that
//! NikaError can use `#[error(transparent)] #[diagnostic(transparent)]`.

use std::path::PathBuf;

/// Media pipeline errors.
///
/// Error codes NIKA-251 through NIKA-259 cover the media extraction,
/// detection, storage, and processing pipeline.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum MediaError {
    /// NIKA-251: Declared MIME type conflicts with detected magic bytes
    #[error("[NIKA-251] MIME detection failed: {reason}")]
    #[diagnostic(code(nika::mime_detection_failed))]
    MimeDetectionFailed {
        reason: String,
    },

    /// NIKA-252: Media type is recognized but not supported for processing
    #[error("[NIKA-252] unsupported media type '{mime_type}': {reason}")]
    #[diagnostic(code(nika::unsupported_media_type))]
    UnsupportedMediaType {
        mime_type: String,
        reason: String,
    },

    /// NIKA-253: Referenced media hash not found in CAS store
    #[error("[NIKA-253] media not found in store: {hash}")]
    #[diagnostic(code(nika::media_not_found))]
    MediaNotFound {
        hash: String,
    },

    /// NIKA-254: CAS read-back verification failed
    #[error("[NIKA-254] CAS hash mismatch (expected {expected}, got {actual})")]
    #[diagnostic(code(nika::hash_mismatch))]
    HashMismatch {
        expected: String,
        actual: String,
    },

    /// NIKA-255: I/O error during CAS store read or write
    #[error("[NIKA-255] media store I/O error: {}: {source}", path.display())]
    #[diagnostic(code(nika::media_store_io))]
    MediaStoreIo {
        path: PathBuf,
        source: std::io::Error,
    },

    /// NIKA-256: Base64 decoding failed for media content block
    #[error("[NIKA-256] base64 decode failed for {source_desc}: {reason}")]
    #[diagnostic(code(nika::base64_decode_failed))]
    Base64DecodeFailed {
        source_desc: String,
        reason: String,
    },

    /// NIKA-257: Base64 input exceeds maximum allowed size before decoding
    #[error("[NIKA-257] base64 input too large ({size} bytes, max {max} bytes)")]
    #[diagnostic(code(nika::base64_input_too_large))]
    Base64InputTooLarge {
        size: usize,
        max: usize,
    },

    /// NIKA-258: Content block decoded to zero bytes
    #[error("[NIKA-258] empty media content from task {task_id}")]
    #[diagnostic(code(nika::empty_media_content))]
    EmptyMediaContent {
        task_id: String,
    },

    /// NIKA-259: Per-run media budget exceeded
    #[error("[NIKA-259] run media budget exceeded ({current} bytes, max {max} bytes)")]
    #[diagnostic(code(nika::run_budget_exceeded))]
    RunBudgetExceeded {
        current: u64,
        max: u64,
    },
}

impl MediaError {
    /// Get the NIKA error code for this error.
    ///
    /// Returns `&'static str` to match `NikaError::code()` signature.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MimeDetectionFailed { .. } => "NIKA-251",
            Self::UnsupportedMediaType { .. } => "NIKA-252",
            Self::MediaNotFound { .. } => "NIKA-253",
            Self::HashMismatch { .. } => "NIKA-254",
            Self::MediaStoreIo { .. } => "NIKA-255",
            Self::Base64DecodeFailed { .. } => "NIKA-256",
            Self::Base64InputTooLarge { .. } => "NIKA-257",
            Self::EmptyMediaContent { .. } => "NIKA-258",
            Self::RunBudgetExceeded { .. } => "NIKA-259",
        }
    }

    /// Check if this error is potentially recoverable through retry.
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::MediaStoreIo { .. })
    }

    /// Construct MimeDetectionFailed with optional server hint.
    pub fn mime_detection_failed(inspected_bytes: usize, server_hint: Option<String>) -> Self {
        let hint_display = match &server_hint {
            Some(hint) => format!(", server hint: {hint}"),
            None => String::new(),
        };
        Self::MimeDetectionFailed {
            reason: format!("{inspected_bytes} bytes inspected{hint_display}"),
        }
    }
}
