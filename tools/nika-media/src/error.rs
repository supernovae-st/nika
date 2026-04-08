// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Media pipeline error types (NIKA-251..259)
//!
//! Derives `miette::Diagnostic` so that
//! NikaError can use `#[diagnostic(transparent)]`.
//! Display is implemented manually for human-friendly error messages
//! (with human-readable byte sizes, clear field labels, etc.).

use std::path::PathBuf;

/// Media pipeline errors.
///
/// Error codes NIKA-251 through NIKA-259 cover the media extraction,
/// detection, storage, and processing pipeline.
#[derive(Debug, miette::Diagnostic)]
pub enum MediaError {
    /// NIKA-251: Declared MIME type conflicts with detected magic bytes
    #[diagnostic(code(nika::mime_detection_failed))]
    MimeDetectionFailed { reason: String },

    /// NIKA-252: Media type is recognized but not supported for processing
    #[diagnostic(code(nika::unsupported_media_type))]
    UnsupportedMediaType { mime_type: String, reason: String },

    /// NIKA-253: Referenced media hash not found in CAS store
    #[diagnostic(code(nika::media_not_found))]
    MediaNotFound { hash: String },

    /// NIKA-254: CAS read-back verification failed
    #[diagnostic(code(nika::hash_mismatch))]
    HashMismatch { expected: String, actual: String },

    /// NIKA-255: I/O error during CAS store read or write
    #[diagnostic(code(nika::media_store_io))]
    MediaStoreIo {
        path: PathBuf,
        source: std::io::Error,
    },

    /// NIKA-256: Base64 decoding failed for media content block
    #[diagnostic(code(nika::base64_decode_failed))]
    Base64DecodeFailed { source_desc: String, reason: String },

    /// NIKA-257: Media content exceeds maximum allowed size
    #[diagnostic(code(nika::media_too_large))]
    Base64InputTooLarge { size: usize, max: usize },

    /// NIKA-258: Content block decoded to zero bytes
    #[diagnostic(code(nika::empty_media_content))]
    EmptyMediaContent { task_id: String },

    /// NIKA-259: Per-run media budget exceeded
    #[diagnostic(code(nika::run_budget_exceeded))]
    RunBudgetExceeded { current: u64, max: u64 },
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MimeDetectionFailed { reason } => {
                write!(f, "[NIKA-251] MIME detection failed: {reason}")
            }

            Self::UnsupportedMediaType { mime_type, reason } => {
                write!(
                    f,
                    "[NIKA-252] unsupported media type '{mime_type}': {reason}"
                )
            }

            Self::MediaNotFound { hash } => {
                write!(f, "[NIKA-253] media not found in store: {hash}")
            }

            Self::HashMismatch { expected, actual } => {
                write!(
                    f,
                    "[NIKA-254] CAS hash mismatch (expected {expected}, got {actual})"
                )
            }

            Self::MediaStoreIo { path, source } => {
                // Show only the filename or last 2 components to avoid leaking
                // full filesystem paths in user-facing output.
                let display_path = sanitize_path_for_display(path);
                write!(
                    f,
                    "[NIKA-255] media store I/O error at {display_path}: {source}"
                )
            }

            Self::Base64DecodeFailed {
                source_desc,
                reason,
            } => {
                write!(
                    f,
                    "[NIKA-256] base64 decode failed for {source_desc}: {reason}"
                )
            }

            Self::Base64InputTooLarge { size, max } => {
                write!(
                    f,
                    "[NIKA-257] media content too large ({}, limit is {})",
                    format_size(*size as u64),
                    format_size(*max as u64),
                )
            }

            Self::EmptyMediaContent { task_id } => {
                if task_id == "(cas-direct)" {
                    write!(
                        f,
                        "[NIKA-258] empty media content received by CAS store \
                         (internal guard: data was empty before storage)"
                    )
                } else {
                    write!(f, "[NIKA-258] empty media content from task '{task_id}'")
                }
            }

            Self::RunBudgetExceeded { current, max } => {
                // `current` is actually the would-be total (existing + requested).
                // Show it as the attempted total vs the limit, with remaining info.
                let used = if *current > *max {
                    // The budget was already partially used; show how much was used
                    // before this attempt. We know: current = prev + requested_size,
                    // and prev <= max (otherwise we'd have rejected earlier).
                    // We can't recover `prev` here, so just show the attempted total.
                    format!(
                        "attempted total: {}, limit: {}",
                        format_size(*current),
                        format_size(*max),
                    )
                } else {
                    format!("at {}, limit: {}", format_size(*current), format_size(*max),)
                };
                write!(f, "[NIKA-259] run media budget exceeded ({used})")
            }
        }
    }
}

impl std::error::Error for MediaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MediaStoreIo { source, .. } => Some(source),
            _ => None,
        }
    }
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
        let reason = match &server_hint {
            Some(hint) => format!(
                "could not identify file type from {inspected_bytes} bytes inspected \
                 (server hint '{hint}' was not usable)"
            ),
            None => format!(
                "could not identify file type from {inspected_bytes} bytes inspected \
                 and no server MIME hint was provided"
            ),
        };
        Self::MimeDetectionFailed { reason }
    }
}

/// Format bytes as human-readable size for error messages.
///
/// Inlined here to avoid a dependency on `crate::util::fs::format_size`
/// (media module should remain low-dependency).
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} bytes")
    }
}

/// Sanitize a filesystem path for user-facing display.
///
/// Shows only the last 2 path components (shard dir + filename) to avoid
/// leaking the full workspace path in error messages or logs.
fn sanitize_path_for_display(path: &std::path::Path) -> String {
    let components: Vec<_> = path.components().rev().take(2).collect();
    match components.len() {
        0 => "<unknown>".to_string(),
        1 => components[0].as_os_str().to_string_lossy().to_string(),
        _ => format!(
            "{}/{}",
            components[1].as_os_str().to_string_lossy(),
            components[0].as_os_str().to_string_lossy(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_human_readable() {
        assert_eq!(format_size(0), "0 bytes");
        assert_eq!(format_size(512), "512 bytes");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1_048_576), "1.0 MB");
        assert_eq!(format_size(104_857_600), "100.0 MB");
        assert_eq!(format_size(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn sanitize_path_shows_last_two_components() {
        let path = PathBuf::from("/home/user/.nika/media/store/af/1349b9abc");
        let display = sanitize_path_for_display(&path);
        assert_eq!(display, "af/1349b9abc");
    }

    #[test]
    fn sanitize_path_single_component() {
        let path = PathBuf::from("filename");
        let display = sanitize_path_for_display(&path);
        assert_eq!(display, "filename");
    }

    #[test]
    fn display_mime_detection_failed_no_hint() {
        let err = MediaError::mime_detection_failed(8192, None);
        let msg = err.to_string();
        assert!(msg.contains("NIKA-251"), "missing code: {msg}");
        assert!(msg.contains("8192 bytes"), "missing byte count: {msg}");
        assert!(
            msg.contains("no server MIME hint"),
            "missing guidance: {msg}"
        );
    }

    #[test]
    fn display_mime_detection_failed_with_hint() {
        let err = MediaError::mime_detection_failed(100, Some("application/octet-stream".into()));
        let msg = err.to_string();
        assert!(msg.contains("NIKA-251"), "missing code: {msg}");
        assert!(
            msg.contains("application/octet-stream"),
            "missing hint: {msg}"
        );
        assert!(msg.contains("not usable"), "missing guidance: {msg}");
    }

    #[test]
    fn display_mime_cross_category_conflict() {
        let err = MediaError::MimeDetectionFailed {
            reason: "MIME category conflict: server declared 'audio/wav' but magic bytes detected 'image/png'".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-251"), "missing code: {msg}");
        assert!(msg.contains("audio/wav"), "missing server type: {msg}");
        assert!(msg.contains("image/png"), "missing detected type: {msg}");
    }

    #[test]
    fn display_base64_input_too_large_human_readable() {
        let err = MediaError::Base64InputTooLarge {
            size: 150 * 1024 * 1024,
            max: 100 * 1024 * 1024,
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-257"), "missing code: {msg}");
        assert!(msg.contains("150.0 MB"), "missing human size: {msg}");
        assert!(msg.contains("100.0 MB"), "missing human max: {msg}");
        assert!(
            msg.contains("media content too large"),
            "wrong label: {msg}"
        );
        // Must NOT say "base64 input" since CAS uses this for raw bytes too
        assert!(
            !msg.contains("base64 input"),
            "should not say 'base64 input': {msg}"
        );
    }

    #[test]
    fn display_base64_input_too_large_small_sizes() {
        let err = MediaError::Base64InputTooLarge {
            size: 200,
            max: 100,
        };
        let msg = err.to_string();
        assert!(msg.contains("200 bytes"), "missing size: {msg}");
        assert!(msg.contains("100 bytes"), "missing max: {msg}");
    }

    #[test]
    fn display_empty_media_cas_direct() {
        let err = MediaError::EmptyMediaContent {
            task_id: "(cas-direct)".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-258"), "missing code: {msg}");
        assert!(msg.contains("CAS store"), "should mention CAS: {msg}");
        assert!(
            msg.contains("internal guard"),
            "should say internal guard: {msg}"
        );
        // Should NOT show the raw "(cas-direct)" sentinel to the user
        assert!(
            !msg.contains("(cas-direct)"),
            "should not show sentinel: {msg}"
        );
    }

    #[test]
    fn display_empty_media_with_task_id() {
        let err = MediaError::EmptyMediaContent {
            task_id: "generate_image".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-258"), "missing code: {msg}");
        assert!(msg.contains("generate_image"), "missing task_id: {msg}");
    }

    #[test]
    fn display_run_budget_exceeded_human_readable() {
        let err = MediaError::RunBudgetExceeded {
            current: 600 * 1024 * 1024,
            max: 500 * 1024 * 1024,
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-259"), "missing code: {msg}");
        assert!(msg.contains("600.0 MB"), "missing attempted total: {msg}");
        assert!(msg.contains("500.0 MB"), "missing limit: {msg}");
        assert!(
            msg.contains("attempted total"),
            "should say attempted: {msg}"
        );
    }

    #[test]
    fn display_media_store_io_sanitized_path() {
        let err = MediaError::MediaStoreIo {
            path: PathBuf::from("/Users/secret/.nika/media/store/af/1349b9"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-255"), "missing code: {msg}");
        assert!(msg.contains("af/1349b9"), "missing path tail: {msg}");
        assert!(msg.contains("denied"), "missing OS error: {msg}");
        // Must NOT expose the full path
        assert!(!msg.contains("/Users/secret"), "leaking full path: {msg}");
    }

    #[test]
    fn display_hash_mismatch_shows_both() {
        let err = MediaError::HashMismatch {
            expected: "blake3:aaaa".into(),
            actual: "blake3:bbbb".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("blake3:aaaa"), "missing expected: {msg}");
        assert!(msg.contains("blake3:bbbb"), "missing actual: {msg}");
    }

    #[test]
    fn all_variants_contain_error_code() {
        let errors: Vec<MediaError> = vec![
            MediaError::mime_detection_failed(0, None),
            MediaError::UnsupportedMediaType {
                mime_type: "video/mp4".into(),
                reason: "not supported".into(),
            },
            MediaError::MediaNotFound {
                hash: "blake3:xxx".into(),
            },
            MediaError::HashMismatch {
                expected: "blake3:aaa".into(),
                actual: "blake3:bbb".into(),
            },
            MediaError::MediaStoreIo {
                path: "/tmp/fail".into(),
                source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
            },
            MediaError::Base64DecodeFailed {
                source_desc: "test".into(),
                reason: "bad".into(),
            },
            MediaError::Base64InputTooLarge {
                size: 200,
                max: 100,
            },
            MediaError::EmptyMediaContent {
                task_id: "t1".into(),
            },
            MediaError::RunBudgetExceeded {
                current: 600,
                max: 500,
            },
        ];

        let expected_codes = [
            "NIKA-251", "NIKA-252", "NIKA-253", "NIKA-254", "NIKA-255", "NIKA-256", "NIKA-257",
            "NIKA-258", "NIKA-259",
        ];

        for (i, (err, code)) in errors.iter().zip(expected_codes.iter()).enumerate() {
            let display = err.to_string();
            assert!(!display.is_empty(), "Error {i} Display is empty");
            assert!(
                display.contains(code),
                "Error {i} Display missing code: {display}"
            );
            assert_eq!(err.code(), *code, "Error {i} code mismatch");
        }
    }
}
