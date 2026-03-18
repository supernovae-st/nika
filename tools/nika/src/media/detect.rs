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
            if (trimmed.starts_with("<?xml") || trimmed.starts_with("<svg"))
                && (trimmed.contains("<svg")
                    || trimmed.contains("xmlns=\"http://www.w3.org/2000/svg\""))
            {
                    return Ok(DetectedMime {
                        mime_type: "image/svg+xml".to_string(),
                        extension: "svg".to_string(),
                        source: DetectionSource::MagicBytes,
                    });
            }
        }
    }

    // Layer 1: Magic byte detection
    if let Some(kind) = infer::get(sample) {
        let mime_type = kind.mime_type().to_string();
        let extension = kind.extension().to_string();

        // Declare-then-verify (D25): cross-validate with server hint
        if let Some(server) = server_mime_ref {
            if !is_mime_alias(&mime_type, server) {
                let detected_category = mime_type.split('/').next();
                let server_category = server.split('/').next();
                if detected_category != server_category {
                    // Cross-category mismatch: REJECT (e.g., server=audio/*, magic=image/*)
                    return Err(MediaError::MimeDetectionFailed {
                        reason: format!(
                            "MIME category conflict: server declared '{}' but magic bytes detected '{}'",
                            server, mime_type
                        ),
                    });
                } else {
                    // Same category, different subtype: warn but accept magic bytes
                    tracing::debug!(
                        detected = %mime_type,
                        server = %server,
                        "MIME subtype mismatch: magic bytes disagree with server hint, using magic bytes"
                    );
                }
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

/// Check if two MIME types are known aliases of each other.
///
/// Covers common non-standard MIME variants sent by MCP servers:
/// - `audio/mp3` ↔ `audio/mpeg`
/// - `image/jpg` ↔ `image/jpeg`
/// - `audio/wav` ↔ `audio/x-wav`
pub fn is_mime_alias(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let pair = (a.min(b), a.max(b));
    matches!(
        pair,
        ("audio/mp3", "audio/mpeg")
            | ("audio/wav", "audio/x-wav")
            | ("image/jpeg", "image/jpg")
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    // PNG magic bytes: 89 50 4E 47
    const PNG_HEADER: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
    // JPEG magic bytes: FF D8 FF
    const JPEG_HEADER: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0];
    // WAV: RIFF....WAVE
    const WAV_HEADER: &[u8] = &[
        0x52, 0x49, 0x46, 0x46, // RIFF
        0x00, 0x00, 0x00, 0x00, // size
        0x57, 0x41, 0x56, 0x45, // WAVE
    ];

    #[test]
    fn detect_png_magic_bytes() {
        let result = detect_mime(PNG_HEADER, None).unwrap();
        assert_eq!(result.mime_type, "image/png");
        assert_eq!(result.extension, "png");
        assert_eq!(result.source, DetectionSource::MagicBytes);
    }

    #[test]
    fn detect_jpeg_magic_bytes() {
        let result = detect_mime(JPEG_HEADER, None).unwrap();
        assert_eq!(result.mime_type, "image/jpeg");
        assert_eq!(result.source, DetectionSource::MagicBytes);
    }

    #[test]
    fn detect_wav_magic_bytes() {
        let result = detect_mime(WAV_HEADER, None).unwrap();
        assert!(result.mime_type.contains("wav"), "expected wav, got {}", result.mime_type);
        assert_eq!(result.source, DetectionSource::MagicBytes);
    }

    #[test]
    fn unknown_bytes_returns_error() {
        let data = &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        let result = detect_mime(data, None);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_bytes_with_octet_stream_returns_error() {
        let data = &[0x00, 0x01, 0x02, 0x03];
        let result = detect_mime(data, Some("application/octet-stream"));
        assert!(result.is_err());
    }

    #[test]
    fn unknown_bytes_with_server_hint_accepted() {
        let data = &[0x00, 0x01, 0x02, 0x03];
        let result = detect_mime(data, Some("image/png")).unwrap();
        assert_eq!(result.mime_type, "image/png");
        assert_eq!(result.source, DetectionSource::ServerHint);
    }

    #[test]
    fn magic_bytes_preferred_over_same_category_hint() {
        // PNG bytes + wrong server hint (same category: image)
        let result = detect_mime(PNG_HEADER, Some("image/webp")).unwrap();
        assert_eq!(result.mime_type, "image/png");
        assert_eq!(result.source, DetectionSource::MagicBytes);
    }

    #[test]
    fn uppercase_server_mime_normalized() {
        let data = &[0x00, 0x01, 0x02, 0x03];
        let result = detect_mime(data, Some("IMAGE/PNG")).unwrap();
        assert_eq!(result.mime_type, "image/png");
    }

    #[test]
    fn svg_detection() {
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\"></svg>";
        let result = detect_mime(svg, None).unwrap();
        assert_eq!(result.mime_type, "image/svg+xml");
        assert_eq!(result.extension, "svg");
    }

    #[test]
    fn mime_to_extension_common_types() {
        assert_eq!(mime_to_extension("image/png"), "png");
        // mime_guess may return "jfif" or "jpg" for image/jpeg
        let jpeg_ext = mime_to_extension("image/jpeg");
        assert!(jpeg_ext == "jpg" || jpeg_ext == "jfif", "got: {jpeg_ext}");
        assert_eq!(mime_to_extension("application/pdf"), "pdf");
    }

    #[test]
    fn is_mime_alias_known_pairs() {
        assert!(is_mime_alias("audio/mp3", "audio/mpeg"));
        assert!(is_mime_alias("audio/mpeg", "audio/mp3"));
        assert!(is_mime_alias("image/jpeg", "image/jpg"));
        assert!(is_mime_alias("image/jpg", "image/jpeg"));
        assert!(is_mime_alias("audio/wav", "audio/x-wav"));
        assert!(is_mime_alias("audio/x-wav", "audio/wav"));
    }

    #[test]
    fn is_mime_alias_identity() {
        assert!(is_mime_alias("image/png", "image/png"));
        assert!(is_mime_alias("audio/mpeg", "audio/mpeg"));
    }

    #[test]
    fn is_mime_alias_non_aliases() {
        assert!(!is_mime_alias("image/png", "image/jpeg"));
        assert!(!is_mime_alias("audio/mp3", "image/png"));
        assert!(!is_mime_alias("audio/ogg", "audio/flac"));
    }

    #[test]
    fn cross_category_mismatch_is_rejected() {
        // PNG bytes + server declares audio/wav → should fail with NIKA-251
        let result = detect_mime(PNG_HEADER, Some("audio/wav"));
        assert!(result.is_err(), "Cross-category mismatch should be rejected");
        assert_eq!(result.unwrap_err().code(), "NIKA-251");
    }

    #[test]
    fn same_category_alias_is_accepted() {
        // WAV bytes + server declares audio/x-wav → alias, should accept
        let result = detect_mime(WAV_HEADER, Some("audio/x-wav"));
        assert!(result.is_ok());
    }

    #[test]
    fn same_category_subtype_mismatch_uses_magic_bytes() {
        // JPEG bytes + server declares image/webp → same category, accept magic bytes
        let result = detect_mime(JPEG_HEADER, Some("image/webp")).unwrap();
        assert_eq!(result.mime_type, "image/jpeg");
        assert_eq!(result.source, DetectionSource::MagicBytes);
    }

    #[test]
    fn sanitize_extension_rejects_traversal() {
        assert_eq!(sanitize_extension("../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_extension("png;rm -rf /"), "pngrm-rf");
    }
}
