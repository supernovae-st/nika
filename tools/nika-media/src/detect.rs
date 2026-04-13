// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

    /// Accepted from server-provided Content-Type hint
    ServerHint,
}

/// Detect MIME type from binary data with optional server hint.
///
/// Detection pipeline (in order):
/// 1. Magic byte inspection via `infer` crate (first 8192 bytes)
/// 2. If server_mime provided and magic bytes fail, accept server hint
/// 3. If both fail, return `Err(MimeDetectionFailed)`
pub fn detect_mime(data: &[u8], server_mime: Option<&str>) -> Result<DetectedMime, MediaError> {
    // Normalize server MIME to lowercase for case-insensitive comparison
    let server_mime_normalized: Option<String> = server_mime.map(|m| m.to_ascii_lowercase());
    let server_mime_ref = server_mime_normalized.as_deref();

    let inspect_len = data.len().min(8192);
    let sample = &data[..inspect_len];

    // SVG special handling: magic bytes won't detect SVG (it's XML-based text).
    // Uses word-boundary check after `<svg` to avoid false positives on
    // tags like `<svg-report>` or `<svg-data>`.
    if sample.len() >= 5 {
        let text_start = std::str::from_utf8(&sample[..sample.len().min(512)]);
        if let Ok(text) = text_start {
            let trimmed = text.trim_start();
            let has_xml_prefix = trimmed.starts_with("<?xml");
            let has_svg_tag = has_svg_element(trimmed);
            let has_svg_ns = trimmed.contains("xmlns=\"http://www.w3.org/2000/svg\"")
                || trimmed.contains("xmlns='http://www.w3.org/2000/svg'");
            if has_svg_tag || (has_xml_prefix && has_svg_ns) {
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
                            "category conflict: server declared '{server}' \
                             but magic bytes detected '{mime_type}'"
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
/// - `audio/flac` ↔ `audio/x-flac`
/// - `audio/aiff` ↔ `audio/x-aiff`
/// - `image/vnd.microsoft.icon` ↔ `image/x-icon`
/// - `audio/mp4` ↔ `video/mp4` (container format ambiguity)
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
            | ("audio/flac", "audio/x-flac")
            | ("audio/aiff", "audio/x-aiff")
            | ("image/vnd.microsoft.icon", "image/x-icon")
            | ("audio/mp4", "video/mp4")
    )
}

/// Convert a MIME type to a file extension.
///
/// Checks the curated manual table FIRST (correct human-expected extensions),
/// then falls back to `mime_guess` for uncommon types.
/// This avoids mime_guess returning obscure extensions like "jfif" for JPEG,
/// "m2a" for MP3, or "asm" for text/plain.
pub fn mime_to_extension(mime: &str) -> String {
    // Manual table first — curated, human-expected extensions
    let manual = match mime {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "image/vnd.microsoft.icon" | "image/x-icon" => Some("ico"),
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/ogg" => Some("ogg"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/aiff" | "audio/x-aiff" => Some("aiff"),
        "audio/mp4" | "audio/x-m4a" => Some("m4a"),
        "video/mp4" => Some("mp4"),
        "video/webm" => Some("webm"),
        "application/pdf" => Some("pdf"),
        "application/json" => Some("json"),
        "text/plain" => Some("txt"),
        "text/html" => Some("html"),
        "text/csv" => Some("csv"),
        _ => None,
    };

    if let Some(ext) = manual {
        return ext.to_string();
    }

    // Fallback to mime_guess for uncommon types
    if let Some(exts) = mime_guess::get_mime_extensions_str(mime) {
        if let Some(ext) = exts.first() {
            return sanitize_extension(ext);
        }
    }

    "bin".to_string()
}

/// Check if text contains an `<svg` element (not `<svg-something>`).
///
/// Requires `<svg` to be followed by a word boundary: space, `>`, `/`, tab, newline.
/// This prevents false positives on tags like `<svg-report>`, `<svg-data>`, etc.
fn has_svg_element(text: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find("<svg") {
        let abs_pos = search_from + pos;
        let after = abs_pos + 4; // length of "<svg"
        if after >= text.len() {
            // `<svg` at end of string — could be start of valid SVG
            return true;
        }
        let next_char = text.as_bytes()[after];
        // Word boundary: space, >, /, tab, newline, carriage return
        if matches!(next_char, b' ' | b'>' | b'/' | b'\t' | b'\n' | b'\r') {
            return true;
        }
        search_from = abs_pos + 4;
    }
    false
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
        assert!(
            result.mime_type.contains("wav"),
            "expected wav, got {}",
            result.mime_type
        );
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
        assert_eq!(mime_to_extension("image/jpeg"), "jpg");
        assert_eq!(mime_to_extension("audio/mpeg"), "mp3");
        assert_eq!(mime_to_extension("audio/wav"), "wav");
        assert_eq!(mime_to_extension("audio/x-wav"), "wav");
        assert_eq!(mime_to_extension("audio/flac"), "flac");
        assert_eq!(mime_to_extension("text/plain"), "txt");
        assert_eq!(mime_to_extension("application/pdf"), "pdf");
        assert_eq!(mime_to_extension("application/json"), "json");
    }

    #[test]
    fn is_mime_alias_known_pairs() {
        assert!(is_mime_alias("audio/mp3", "audio/mpeg"));
        assert!(is_mime_alias("audio/mpeg", "audio/mp3"));
        assert!(is_mime_alias("image/jpeg", "image/jpg"));
        assert!(is_mime_alias("image/jpg", "image/jpeg"));
        assert!(is_mime_alias("audio/wav", "audio/x-wav"));
        assert!(is_mime_alias("audio/x-wav", "audio/wav"));
        assert!(is_mime_alias("audio/flac", "audio/x-flac"));
        assert!(is_mime_alias("audio/x-flac", "audio/flac"));
        assert!(is_mime_alias("audio/aiff", "audio/x-aiff"));
        assert!(is_mime_alias("image/vnd.microsoft.icon", "image/x-icon"));
        assert!(is_mime_alias("audio/mp4", "video/mp4"));
        assert!(is_mime_alias("video/mp4", "audio/mp4"));
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
        assert!(
            result.is_err(),
            "Cross-category mismatch should be rejected"
        );
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

    // ═══════════════════════════════════════════════════════════════
    // GAP 4: mime_to_extension for alias MIME types
    // Non-standard MIME aliases must map to the correct extension
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn mime_to_extension_alias_image_jpg() {
        // "image/jpg" is non-standard (standard is "image/jpeg")
        // Must still return "jpg", not fall through to mime_guess
        assert_eq!(mime_to_extension("image/jpg"), "jpg");
    }

    #[test]
    fn mime_to_extension_alias_audio_mp3() {
        // "audio/mp3" is non-standard (standard is "audio/mpeg")
        // Must return "mp3", not some obscure extension like "m2a"
        assert_eq!(mime_to_extension("audio/mp3"), "mp3");
    }

    #[test]
    fn mime_to_extension_alias_audio_x_flac() {
        // "audio/x-flac" is non-standard (standard is "audio/flac")
        // Must return "flac"
        assert_eq!(mime_to_extension("audio/x-flac"), "flac");
    }

    #[test]
    fn mime_to_extension_alias_audio_x_wav() {
        // "audio/x-wav" is non-standard (standard is "audio/wav")
        assert_eq!(mime_to_extension("audio/x-wav"), "wav");
    }

    #[test]
    fn mime_to_extension_alias_audio_x_aiff() {
        // "audio/x-aiff" is non-standard (standard is "audio/aiff")
        assert_eq!(mime_to_extension("audio/x-aiff"), "aiff");
    }

    #[test]
    fn mime_to_extension_alias_image_x_icon() {
        // "image/x-icon" is non-standard (standard is "image/vnd.microsoft.icon")
        assert_eq!(mime_to_extension("image/x-icon"), "ico");
    }

    #[test]
    fn mime_to_extension_alias_audio_mp4() {
        // "audio/mp4" should return "m4a" (audio in MP4 container)
        assert_eq!(mime_to_extension("audio/mp4"), "m4a");
    }

    // ═══════════════════════════════════════════════════════════════
    // GAP 5: is_mime_alias reverse order for aiff and icon
    // Lexicographic ordering in (a.min(b), a.max(b)) must work
    // regardless of argument order
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn is_mime_alias_aiff_reverse_order() {
        // Test that (audio/x-aiff, audio/aiff) matches — reverse of table order
        assert!(
            is_mime_alias("audio/x-aiff", "audio/aiff"),
            "reverse order must match: audio/x-aiff -> audio/aiff"
        );
        // Confirm canonical order also works
        assert!(
            is_mime_alias("audio/aiff", "audio/x-aiff"),
            "canonical order must match: audio/aiff -> audio/x-aiff"
        );
    }

    #[test]
    fn is_mime_alias_icon_reverse_order() {
        // Test that (image/x-icon, image/vnd.microsoft.icon) matches
        assert!(
            is_mime_alias("image/x-icon", "image/vnd.microsoft.icon"),
            "reverse order must match: image/x-icon -> image/vnd.microsoft.icon"
        );
        // Confirm canonical order
        assert!(
            is_mime_alias("image/vnd.microsoft.icon", "image/x-icon"),
            "canonical order must match: image/vnd.microsoft.icon -> image/x-icon"
        );
    }

    #[test]
    fn is_mime_alias_mp4_cross_category_reverse() {
        // Test both orderings of the cross-category mp4 alias
        assert!(
            is_mime_alias("video/mp4", "audio/mp4"),
            "reverse order must match: video/mp4 -> audio/mp4"
        );
        assert!(
            is_mime_alias("audio/mp4", "video/mp4"),
            "canonical order must match: audio/mp4 -> video/mp4"
        );
    }

    #[test]
    fn is_mime_alias_flac_reverse_order() {
        assert!(
            is_mime_alias("audio/x-flac", "audio/flac"),
            "reverse order must match: audio/x-flac -> audio/flac"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // GAP 7: detect_mime with cross-category alias (audio/mp4 <> video/mp4)
    // MP4 container is detected as video/mp4 by magic bytes. When
    // server declares audio/mp4, the alias should prevent rejection.
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn detect_mime_mp4_container_with_audio_mp4_server_hint() {
        // MP4 container header (ftyp box): detected as video/mp4 by infer
        let mp4_header: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x20, // box size = 32
            0x66, 0x74, 0x79, 0x70, // "ftyp"
            0x69, 0x73, 0x6F, 0x6D, // major brand "isom"
            0x00, 0x00, 0x02, 0x00, // minor version
            0x69, 0x73, 0x6F, 0x6D, // compatible brands: "isom"
            0x69, 0x73, 0x6F, 0x32, // "iso2"
            0x61, 0x76, 0x63, 0x31, // "avc1"
            0x6D, 0x70, 0x34, 0x31, // "mp41"
        ];

        // Magic bytes -> video/mp4, server -> audio/mp4
        // Without the alias, this would be a cross-category mismatch (video != audio)
        // With the alias, it should be accepted
        let result = detect_mime(&mp4_header, Some("audio/mp4"));
        assert!(
            result.is_ok(),
            "audio/mp4 <> video/mp4 alias should prevent cross-category rejection, got: {:?}",
            result.err()
        );

        let detected = result.unwrap();
        assert_eq!(
            detected.mime_type, "video/mp4",
            "magic bytes should win (video/mp4), not server hint"
        );
        assert_eq!(detected.source, DetectionSource::MagicBytes);
    }

    #[test]
    fn detect_mime_mp4_container_with_video_mp4_server_matches() {
        // MP4 container header: same as above
        let mp4_header: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x20, 0x66, 0x74, 0x79, 0x70, 0x69, 0x73, 0x6F, 0x6D, 0x00, 0x00,
            0x02, 0x00, 0x69, 0x73, 0x6F, 0x6D, 0x69, 0x73, 0x6F, 0x32, 0x61, 0x76, 0x63, 0x31,
            0x6D, 0x70, 0x34, 0x31,
        ];

        // Magic bytes -> video/mp4, server -> video/mp4 -> exact match
        let result = detect_mime(&mp4_header, Some("video/mp4"));
        assert!(result.is_ok());
        let detected = result.unwrap();
        assert_eq!(detected.mime_type, "video/mp4");
    }

    #[test]
    fn sanitize_extension_rejects_traversal() {
        assert_eq!(sanitize_extension("../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_extension("png;rm -rf /"), "pngrm-rf");
    }

    // ---------------------------------------------------------------
    // SVG heuristic edge cases (lines 57-73)
    // ---------------------------------------------------------------

    #[test]
    fn svg_heuristic_rejects_svg_prefixed_tag() {
        // `<svg-report>` starts with `<svg` but is NOT followed by a word boundary.
        // The tightened heuristic (has_svg_element) requires space/>/\t/\n after `<svg`.
        // `<svg-report>` has `-` after `<svg`, which is NOT a word boundary → rejected.
        let data = b"<svg-report><item>data</item></svg-report>";
        let result = detect_mime(data, None);
        assert!(
            result.is_err(),
            "<svg-report> should NOT be detected as SVG"
        );
    }

    #[test]
    fn svg_heuristic_leading_whitespace() {
        // Leading whitespace/tabs/newlines should be stripped by trim_start()
        let data = b"  \t\n  <svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        let result = detect_mime(data, None).unwrap();
        assert_eq!(result.mime_type, "image/svg+xml");
        assert_eq!(result.extension, "svg");
        assert_eq!(result.source, DetectionSource::MagicBytes);
    }

    #[test]
    fn svg_heuristic_xml_declaration_plus_svg_no_xmlns() {
        // `<?xml` prefix passes first guard, `<svg>` substring found by contains("<svg").
        // Even without xmlns, the heuristic fires.
        let data = b"<?xml version=\"1.0\"?>\n<svg></svg>";
        let result = detect_mime(data, None).unwrap();
        assert_eq!(result.mime_type, "image/svg+xml");
        assert_eq!(result.extension, "svg");
    }

    #[test]
    fn svg_heuristic_false_positive_svg_in_xml_comment() {
        // `<?xml` passes starts_with guard. The comment text `<svg>` makes
        // `contains("<svg")` return true. This is a known false positive:
        // the heuristic cannot distinguish commented-out SVG from real SVG.
        let data = b"<?xml version=\"1.0\"?><!-- comment with <svg> in it --><root/>";
        let result = detect_mime(data, None);
        assert!(
            result.is_ok(),
            "heuristic fires on <svg> inside XML comment (known false positive)"
        );
        let detected = result.unwrap();
        assert_eq!(detected.mime_type, "image/svg+xml");
    }

    #[test]
    fn svg_heuristic_false_negative_beyond_512_byte_window() {
        // The heuristic only inspects the first 512 bytes of the sample.
        // If the `<svg` tag starts beyond byte 512, it won't be found.
        // Build: 620 bytes of non-whitespace padding + SVG tag.
        let mut data = Vec::new();
        // Use valid XML-like padding that is NOT whitespace (trim_start won't remove it)
        data.extend_from_slice(b"<data>");
        // 610 bytes of 'x' filler (total prefix = 6 + 610 = 616 bytes)
        data.extend(std::iter::repeat_n(b'x', 610));
        data.extend_from_slice(b"</data><svg xmlns=\"http://www.w3.org/2000/svg\"></svg>");

        let result = detect_mime(&data, None);
        // The first 512 bytes are `<data>xxxx...` which starts_with neither
        // `<svg` nor `<?xml`, so the heuristic does not fire.
        assert!(
            result.is_err(),
            "SVG beyond 512-byte window should not be detected (false negative)"
        );
    }

    #[test]
    fn svg_heuristic_single_quotes_xmlns() {
        // The code checks `xmlns=\"http://www.w3.org/2000/svg\"` (double quotes).
        // Single-quoted xmlns does NOT match that literal.
        // However, `starts_with("<svg")` is true and `contains("<svg")` is also
        // true (the tag itself is `<svg`), so the first OR branch fires.
        // Result: still detected despite single quotes, because `contains("<svg")`
        // is the condition that matches, not the xmlns check.
        let data = b"<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'></svg>";
        let result = detect_mime(data, None).unwrap();
        assert_eq!(result.mime_type, "image/svg+xml");
        assert_eq!(result.extension, "svg");
    }

    #[test]
    fn svg_heuristic_uppercase_tag_not_detected() {
        // `<SVG` does NOT match `starts_with("<svg")` (case-sensitive).
        // It also does not match `starts_with("<?xml")`.
        // The heuristic does not fire. Falls through to infer crate which
        // also won't detect it, then to server hint, then to error.
        let data = b"<SVG xmlns=\"http://www.w3.org/2000/svg\"></SVG>";
        let result = detect_mime(data, None);
        assert!(
            result.is_err(),
            "uppercase <SVG> not detected by case-sensitive heuristic"
        );
    }

    #[test]
    fn svg_heuristic_self_closing_no_xmlns() {
        // `<svg/>` starts with `<svg`, and `contains("<svg")` is true.
        // The heuristic fires even though this is a bare self-closing tag.
        let data = b"<svg/>";
        let result = detect_mime(data, None).unwrap();
        assert_eq!(result.mime_type, "image/svg+xml");
        assert_eq!(result.extension, "svg");
    }

    #[test]
    fn svg_heuristic_binary_with_svg_utf8_prefix() {
        // Binary data whose first 4 bytes happen to be `<svg` in UTF-8,
        // but followed by invalid UTF-8 byte 0xFF within the first 512 bytes.
        // from_utf8 on the first 512 bytes will fail, so the heuristic skips.
        let mut data = Vec::new();
        data.extend_from_slice(b"<svg");
        data.push(0xFF); // invalid UTF-8
        data.extend(std::iter::repeat_n(0x00u8, 20));
        let result = detect_mime(&data, None);
        // from_utf8 fails on the 512-byte window → SVG heuristic skipped.
        // infer crate won't recognize it → error (no server hint).
        assert!(
            result.is_err(),
            "binary data with <svg prefix but invalid UTF-8 should not be detected as SVG"
        );
    }

    #[test]
    fn svg_heuristic_exactly_five_bytes() {
        // `<svg>` is exactly 5 bytes. The `sample.len() >= 5` guard passes.
        // from_utf8 succeeds. trim_start is a no-op. starts_with("<svg") is true.
        // contains("<svg") is true. Detected as SVG.
        let data = b"<svg>";
        assert_eq!(data.len(), 5);
        let result = detect_mime(data, None).unwrap();
        assert_eq!(result.mime_type, "image/svg+xml");
        assert_eq!(result.extension, "svg");
        assert_eq!(result.source, DetectionSource::MagicBytes);
    }
}
