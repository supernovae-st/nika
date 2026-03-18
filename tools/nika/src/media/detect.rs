//! MIME detection pipeline: magic bytes -> server hint fallback
//!
//! When both magic bytes AND server hint fail, returns
//! `Err(MimeDetectionFailed)` instead of a silent fallback.
//!
//! Server MIME is case-normalized before any comparison.
//!
//! Declare-then-verify: when both magic bytes and server hint
//! are available, cross-validates at the category level.

use super::error::MediaError;

/// Result of MIME detection.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedMime {
    /// Detected MIME type (e.g., "image/png")
    pub mime_type: String,

    /// File extension without dot (e.g., "png")
    pub extension: String,

    /// How the MIME type was determined
    pub source: DetectionSource,
}

/// How the MIME type was determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectionSource {
    /// Determined via magic byte inspection (highest confidence)
    MagicBytes,

    /// Determined via file extension lookup
    Extension,

    /// Accepted from server-provided Content-Type hint
    ServerHint,
}

/// Detect MIME type from binary data with optional server hint.
///
/// Detection pipeline (in order):
/// 1. Magic byte inspection via `infer` crate (first 8192 bytes)
/// 2. If server_mime provided and magic bytes fail, accept server hint
/// 3. If both fail, return `Err(MimeDetectionFailed)`
pub fn detect_mime(
    data: &[u8],
    server_mime: Option<&str>,
) -> Result<DetectedMime, MediaError> {
    // Normalize server MIME to lowercase for case-insensitive comparison
    let server_mime_normalized: Option<String> =
        server_mime.map(|m| m.to_ascii_lowercase());
    let server_mime_ref = server_mime_normalized.as_deref();

    let inspect_len = data.len().min(8192);
    let sample = &data[..inspect_len];

    // SVG special handling: magic bytes won't detect SVG (it's XML-based text)
    if sample.len() >= 5 {
        let text_start = std::str::from_utf8(&sample[..sample.len().min(512)]);
        if let Ok(text) = text_start {
            let trimmed = text.trim_start();
            if trimmed.starts_with("<?xml") || trimmed.starts_with("<svg") {
                if trimmed.contains("<svg")
                    || trimmed.contains("xmlns=\"http://www.w3.org/2000/svg\"")
                {
                    return Ok(DetectedMime {
                        mime_type: "image/svg+xml".to_string(),
                        extension: "svg".to_string(),
                        source: DetectionSource::MagicBytes,
                    });
                }
            }
        }
    }

    // Layer 1: Magic byte detection
    if let Some(kind) = infer::get(sample) {
        let mime_type = kind.mime_type().to_string();
        let extension = kind.extension().to_string();

        // Declare-then-verify: cross-validate with server hint
        if let Some(server) = server_mime_ref {
            let detected_category = mime_type.split('/').next();
            let server_category = server.split('/').next();
            if detected_category != server_category {
                tracing::warn!(
                    detected = %mime_type,
                    server = %server,
                    "MIME category mismatch: server declared {}, magic bytes detected {}",
                    server,
                    mime_type,
                );
            } else if *server != mime_type {
                tracing::debug!(
                    detected = %mime_type,
                    server = %server,
                    "MIME subtype mismatch: magic bytes disagree with server hint, using magic bytes"
                );
            }
        }

        return Ok(DetectedMime {
            mime_type,
            extension,
            source: DetectionSource::MagicBytes,
        });
    }

    // Layer 2: Accept server hint if magic bytes failed
    if let Some(server) = server_mime_ref {
        if server != "application/octet-stream" {
            let extension = mime_to_extension(server);
            return Ok(DetectedMime {
                mime_type: server.to_string(),
                extension,
                source: DetectionSource::ServerHint,
            });
        }
    }

    // Layer 3: Both magic bytes and server hint failed
    Err(MediaError::mime_detection_failed(
        inspect_len,
        server_mime.map(|s| s.to_string()),
    ))
}

/// Convert a MIME type to a file extension.
pub fn mime_to_extension(mime: &str) -> String {
    // Try mime_guess first
    let guesses = mime_guess::get_mime_extensions_str(mime);
    if let Some(exts) = guesses {
        if let Some(ext) = exts.first() {
            return sanitize_extension(ext);
        }
    }

    // Manual fallback for common types
    let ext = match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "application/pdf" => "pdf",
        "application/json" => "json",
        "text/plain" => "txt",
        "text/html" => "html",
        _ => "bin",
    };

    sanitize_extension(ext)
}

/// Sanitize extension to prevent path traversal.
/// Only allows alphanumeric characters and hyphens.
fn sanitize_extension(ext: &str) -> String {
    ext.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase()
}
