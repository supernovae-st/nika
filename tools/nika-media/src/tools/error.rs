// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Media tool error types (NIKA-290..297)
//!
//! Errors specific to media TOOL operations (resize, OCR, SVG render, etc.),
//! distinct from the media PIPELINE errors (NIKA-251..259) in crate::error.

/// Errors from media tool operations.
#[derive(Debug, thiserror::Error)]
pub enum MediaToolError {
    /// Generic tool error (NIKA-290)
    #[error("[NIKA-290] nika:{tool}: {reason}")]
    ToolError { tool: String, reason: String },

    /// Unsupported format (NIKA-291)
    #[error("[NIKA-291] nika:{tool}: unsupported format '{mime}'")]
    UnsupportedFormat { tool: String, mime: String },

    /// Missing feature dependency (NIKA-292)
    #[error("[NIKA-292] nika:{tool}: feature '{feature}' required but not enabled")]
    DependencyMissing { tool: String, feature: String },

    /// Operation timed out (NIKA-293)
    #[error("[NIKA-293] nika:{tool}: operation timed out")]
    Timeout { tool: String },

    /// Invalid arguments (NIKA-294)
    #[error("[NIKA-294] nika:{tool}: {reason}")]
    InvalidArgs { tool: String, reason: String },

    /// Pipeline step failed (NIKA-295)
    #[error("[NIKA-295] nika:pipeline: step {step} failed: {reason}")]
    PipelineStepFailed { step: usize, reason: String },

    /// Pipeline has no steps (NIKA-296)
    #[error("[NIKA-296] nika:pipeline: pipeline has no steps")]
    PipelineEmpty,

    /// Security violation (NIKA-297)
    #[error("[NIKA-297] nika:{tool}: security violation: {reason}")]
    SecurityViolation { tool: String, reason: String },

    /// Underlying media pipeline error (NIKA-251..259)
    #[error(transparent)]
    Media(#[from] crate::error::MediaError),
}

/// Convenience Result alias for media tool operations.
pub type Result<T> = std::result::Result<T, MediaToolError>;

// ─── Error constructors ─────────────────────────────────────────────────────

/// Create a generic media tool error (NIKA-290).
pub fn tool_error(tool: &str, reason: impl Into<String>) -> MediaToolError {
    MediaToolError::ToolError {
        tool: tool.to_string(),
        reason: reason.into(),
    }
}

/// Create an unsupported format error (NIKA-291).
pub fn unsupported_format(tool: &str, mime: &str) -> MediaToolError {
    MediaToolError::UnsupportedFormat {
        tool: tool.to_string(),
        mime: mime.to_string(),
    }
}

/// Create a dependency missing error (NIKA-292).
pub fn dependency_missing(tool: &str, feature: &str) -> MediaToolError {
    MediaToolError::DependencyMissing {
        tool: tool.to_string(),
        feature: feature.to_string(),
    }
}

/// Create a timeout error (NIKA-293).
pub fn timeout_error(tool: &str) -> MediaToolError {
    MediaToolError::Timeout {
        tool: tool.to_string(),
    }
}

/// Create an invalid args error (NIKA-294).
pub fn invalid_args(tool: &str, reason: impl Into<String>) -> MediaToolError {
    MediaToolError::InvalidArgs {
        tool: tool.to_string(),
        reason: reason.into(),
    }
}

/// Create a pipeline step failed error (NIKA-295).
pub fn pipeline_step_failed(step: usize, reason: impl Into<String>) -> MediaToolError {
    MediaToolError::PipelineStepFailed {
        step,
        reason: reason.into(),
    }
}

/// Create a pipeline empty error (NIKA-296).
pub fn pipeline_empty() -> MediaToolError {
    MediaToolError::PipelineEmpty
}

/// Create a security violation error (NIKA-297).
pub fn security_violation(tool: &str, reason: impl Into<String>) -> MediaToolError {
    MediaToolError::SecurityViolation {
        tool: tool.to_string(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_error_contains_code() {
        let err = tool_error("thumbnail", "decode failed");
        assert!(err.to_string().contains("NIKA-290"));
        assert!(err.to_string().contains("decode failed"));
    }

    #[test]
    fn unsupported_format_contains_mime() {
        let err = unsupported_format("dimensions", "audio/wav");
        assert!(err.to_string().contains("NIKA-291"));
        assert!(err.to_string().contains("audio/wav"));
    }

    #[test]
    fn dependency_missing_shows_feature() {
        let err = dependency_missing("thumbnail", "media-thumbnail");
        assert!(err.to_string().contains("NIKA-292"));
        assert!(err.to_string().contains("media-thumbnail"));
    }

    #[test]
    fn timeout_error_code() {
        let err = timeout_error("optimize");
        assert!(err.to_string().contains("NIKA-293"));
    }

    #[test]
    fn invalid_args_code() {
        let err = invalid_args("thumbnail", "width must be > 0");
        assert!(err.to_string().contains("NIKA-294"));
        assert!(err.to_string().contains("width must be > 0"));
    }

    #[test]
    fn security_violation_code() {
        let err = security_violation("svg_render", "SVG contains <script>");
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("<script>"));
    }

    #[test]
    fn pipeline_errors() {
        let err = pipeline_step_failed(2, "resize failed");
        assert!(err.to_string().contains("NIKA-295"));
        assert!(err.to_string().contains("step 2"));

        let err = pipeline_empty();
        assert!(err.to_string().contains("NIKA-296"));
    }
}
