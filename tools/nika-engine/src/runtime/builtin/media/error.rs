//! Media tool error helpers (NIKA-290..297)
//!
//! These are errors specific to media TOOL operations (resize, OCR, SVG render, etc.),
//! distinct from the media PIPELINE errors (NIKA-251..259) in src/media/error.rs.

use crate::error::NikaError;

/// Create a generic media tool error (NIKA-290).
pub fn tool_error(tool: &str, reason: impl Into<String>) -> NikaError {
    NikaError::BuiltinToolError {
        tool: format!("nika:{tool}"),
        reason: format!("[NIKA-290] {}", reason.into()),
    }
}

/// Create an unsupported format error (NIKA-291).
pub fn unsupported_format(tool: &str, mime: &str) -> NikaError {
    NikaError::BuiltinToolError {
        tool: format!("nika:{tool}"),
        reason: format!("[NIKA-291] unsupported format '{mime}' for this tool"),
    }
}

/// Create a dependency missing error (NIKA-292).
#[allow(dead_code)] // Used behind #[cfg(not(feature))] gates
pub fn dependency_missing(tool: &str, feature: &str) -> NikaError {
    NikaError::BuiltinToolError {
        tool: format!("nika:{tool}"),
        reason: format!(
            "[NIKA-292] feature '{feature}' is required but not enabled. \
       Rebuild with: cargo build --features {feature}"
        ),
    }
}

/// Create a timeout error (NIKA-293).
pub fn timeout_error(tool: &str) -> NikaError {
    NikaError::BuiltinToolError {
        tool: format!("nika:{tool}"),
        reason: "[NIKA-293] operation timed out".to_string(),
    }
}

/// Create an invalid args error (NIKA-294).
pub fn invalid_args(tool: &str, reason: impl Into<String>) -> NikaError {
    NikaError::BuiltinInvalidParams {
        tool: format!("nika:{tool}"),
        reason: format!("[NIKA-294] {}", reason.into()),
    }
}

/// Create a pipeline step failed error (NIKA-295).
pub fn pipeline_step_failed(step: usize, reason: impl Into<String>) -> NikaError {
    NikaError::BuiltinToolError {
        tool: "nika:pipeline".to_string(),
        reason: format!("[NIKA-295] step {step} failed: {}", reason.into()),
    }
}

/// Create a pipeline empty error (NIKA-296).
pub fn pipeline_empty() -> NikaError {
    NikaError::BuiltinToolError {
        tool: "nika:pipeline".to_string(),
        reason: "[NIKA-296] pipeline has no steps".to_string(),
    }
}

/// Create a security violation error (NIKA-297).
pub fn security_violation(tool: &str, reason: impl Into<String>) -> NikaError {
    NikaError::BuiltinToolError {
        tool: format!("nika:{tool}"),
        reason: format!("[NIKA-297] security violation: {}", reason.into()),
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
