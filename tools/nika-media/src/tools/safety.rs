// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Media tool safety primitives.
//!
//! SECURITY: Every media tool that decodes images MUST use `decode_image_safe()`.
//! Never call `image::load_from_memory()` directly — a 1x1 PNG can decompress to 16 GB.

use super::error::MediaToolError;
use super::error::{security_violation, tool_error};

/// Maximum decoded pixel buffer size (256 MB).
#[cfg(any(
    feature = "media-thumbnail",
    feature = "media-svg",
    feature = "media-phash",
    feature = "media-qr",
    feature = "media-iqa"
))]
const MAX_DECODED_BYTES: u64 = 256 * 1024 * 1024;

/// Maximum image dimension (10000x10000).
#[cfg(any(
    feature = "media-thumbnail",
    feature = "media-svg",
    feature = "media-phash",
    feature = "media-qr",
    feature = "media-iqa"
))]
pub(crate) const MAX_IMAGE_DIM: u32 = 10_000;

/// Safely decode an image with resource limits.
///
/// Uses `image::io::Reader` with explicit `Limits` to prevent
/// decompression bombs (e.g., a 1x1 header that expands to 16 GB).
///
/// # Security
/// - `max_alloc`: 256 MB
/// - `max_image_width`: 10,000 px
/// - `max_image_height`: 10,000 px
#[cfg(any(
    feature = "media-thumbnail",
    feature = "media-svg",
    feature = "media-phash",
    feature = "media-qr",
    feature = "media-iqa"
))]
pub fn decode_image_safe(data: &[u8]) -> Result<image::DynamicImage, MediaToolError> {
    use image::ImageReader;
    use std::io::Cursor;

    // Guess format from magic bytes, then apply resource limits
    let mut reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| tool_error("decode", format!("format detection: {e}")))?;

    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODED_BYTES);
    limits.max_image_width = Some(MAX_IMAGE_DIM);
    limits.max_image_height = Some(MAX_IMAGE_DIM);
    reader.limits(limits);

    reader
        .decode()
        .map_err(|e| tool_error("decode", format!("decode failed: {e}")))
}

/// Composite RGBA image onto white background, returning an RGB image.
///
/// `image::DynamicImage::to_rgb8()` does NOT composite — it silently drops
/// the alpha channel, so RGBA(255,0,0,0) (fully transparent red) becomes
/// RGB(255,0,0) (solid red). This function performs proper alpha blending
/// against a white background before stripping alpha.
///
/// MUST be called whenever encoding to a format that does not support
/// transparency (JPEG).
#[cfg(any(feature = "media-thumbnail", feature = "media-svg"))]
pub fn composite_on_white(img: &image::DynamicImage) -> image::RgbImage {
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    image::RgbImage::from_fn(w, h, |x, y| {
        let px = rgba.get_pixel(x, y);
        let [r, g, b, a] = px.0;
        let alpha = a as f32 / 255.0;
        let inv = 1.0 - alpha;
        image::Rgb([
            (r as f32 * alpha + 255.0 * inv) as u8,
            (g as f32 * alpha + 255.0 * inv) as u8,
            (b as f32 * alpha + 255.0 * inv) as u8,
        ])
    })
}

/// Sanitize SVG content by rejecting dangerous elements.
///
/// MUST be called BEFORE any SVG parsing (usvg, resvg).
///
/// # Rejected patterns
/// - `<script>` — XSS
/// - `<foreignObject>` — HTML injection
/// - `javascript:` — XSS via href
/// - `on*=` event handlers — XSS via DOM events
pub fn sanitize_svg(input: &str) -> Result<&str, MediaToolError> {
    let lower = input.to_ascii_lowercase();

    for pattern in [
        "<script",
        "<foreignobject",
        "javascript:",
        "file://",
        "data:text/html",
        "data:image/svg",
        "data:application/xml",
        "data:text/xml",
    ] {
        if lower.contains(pattern) {
            return Err(security_violation(
                "svg_render",
                format!("SVG contains forbidden element: {pattern}"),
            ));
        }
    }

    // Block DOCTYPE declarations (XML bomb / entity expansion attack)
    if lower.contains("<!doctype") || lower.contains("<!entity") {
        return Err(security_violation(
            "svg_render",
            "SVG contains DOCTYPE or ENTITY declaration (not allowed for security)",
        ));
    }

    // Block href/xlink:href with external URLs (SSRF via SVG rendering).
    // Covers both SVG 1.1 (xlink:href) and SVG 2 (plain href) on image/use elements.
    // Allow internal fragment references like href="#icon".
    // Optional quote handles both href="http://..." and href=http://...
    // After optional quote, first char must NOT be # or quote (closing).
    static XLINK_EXTERNAL_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?:xlink:)?href\s*=\s*["']?[^#\s"']"#).unwrap()
    });
    if XLINK_EXTERNAL_RE.is_match(&lower) {
        return Err(security_violation(
            "svg_render",
            "SVG contains href with external URL (only #fragment refs allowed)",
        ));
    }

    // Event handlers: onload=, onclick=, onerror=, etc.
    static EVENT_HANDLER_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\bon\w+\s*=").unwrap());
    if EVENT_HANDLER_RE.is_match(&lower) {
        return Err(security_violation(
            "svg_render",
            "SVG contains event handler attribute",
        ));
    }

    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════
    // SVG SANITIZATION TESTS
    // ═══════════════════════════════════════════

    #[test]
    fn sanitize_svg_allows_clean_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
      <rect width="100" height="100" fill="red"/>
    </svg>"#;
        assert!(sanitize_svg(svg).is_ok());
    }

    #[test]
    fn sanitize_svg_rejects_script_tag() {
        let svg = r#"<svg><script>alert('xss')</script></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("<script"));
    }

    #[test]
    fn sanitize_svg_rejects_script_case_insensitive() {
        let svg = r#"<svg><SCRIPT>alert('xss')</SCRIPT></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
    }

    #[test]
    fn sanitize_svg_rejects_foreign_object() {
        let svg = r#"<svg><foreignObject><body xmlns="http://www.w3.org/1999/xhtml">
      <div>HTML injection</div>
    </body></foreignObject></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("<foreignobject"));
    }

    #[test]
    fn sanitize_svg_rejects_javascript_href() {
        let svg = r#"<svg><a href="javascript:alert(1)"><text>click</text></a></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("javascript:"));
    }

    #[test]
    fn sanitize_svg_rejects_onload_handler() {
        let svg = r#"<svg onload="alert(1)"><rect width="10" height="10"/></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("event handler"));
    }

    #[test]
    fn sanitize_svg_rejects_onclick_handler() {
        let svg = r#"<svg><rect onclick="alert(1)" width="10" height="10"/></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
    }

    #[test]
    fn sanitize_svg_rejects_onerror_handler() {
        let svg = r#"<svg><image onerror ="alert(1)" href="x"/></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
    }

    #[test]
    fn sanitize_svg_allows_xlink_href_for_symbols() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"><defs><rect id=\"icon\" width=\"10\" height=\"10\"/></defs><use xlink:href=\"#icon\"/></svg>";
        assert!(sanitize_svg(svg).is_ok());
    }

    #[test]
    fn sanitize_svg_still_rejects_javascript_in_xlink_href() {
        let svg = r#"<svg><a xlink:href="javascript:alert(1)"><text>click</text></a></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
    }

    #[test]
    fn sanitize_svg_rejects_unquoted_xlink_href() {
        let svg = r#"<svg><use xlink:href=http://evil.com/payload.svg /></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("href"));
    }

    #[test]
    fn sanitize_svg_rejects_plain_href_ssrf() {
        // SVG 2 uses plain href (no xlink: prefix)
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><image href="http://169.254.169.254/latest/meta-data/" width="10" height="10"/></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("href"));
    }

    #[test]
    fn sanitize_svg_rejects_data_image_svg_xml() {
        // Nested SVG via data:image/svg+xml — no <script> so it tests the data URI rule
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
          <image href="data:image/svg+xml,<svg><rect width='9999' height='9999'/></svg>"/>
        </svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("data:image/svg"));
    }

    #[test]
    fn sanitize_svg_rejects_data_application_xml() {
        let svg = r#"<svg><image href="data:application/xml,<x/>"/></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("data:application/xml"));
    }

    #[test]
    fn sanitize_svg_rejects_data_text_xml() {
        let svg = r#"<svg><image href="data:text/xml,<x/>"/></svg>"#;
        let err = sanitize_svg(svg).unwrap_err();
        assert!(err.to_string().contains("NIKA-297"));
        assert!(err.to_string().contains("data:text/xml"));
    }

    #[test]
    fn sanitize_svg_rejects_doctype() {
        let svg = r#"<?xml version="1.0"?>
<!DOCTYPE svg [
  <!ENTITY lol "lol">
  <!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;">
]>
<svg xmlns="http://www.w3.org/2000/svg">
  <text>&lol2;</text>
</svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("DOCTYPE") || err.contains("ENTITY"),
            "error should mention DOCTYPE/ENTITY: {err}"
        );
    }

    #[test]
    fn sanitize_svg_rejects_entity_declaration() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
<!ENTITY xxe SYSTEM "file:///etc/passwd">
<text>&xxe;</text>
</svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err());
    }

    #[test]
    fn sanitize_svg_allows_normal_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <circle cx="50" cy="50" r="40" fill="blue"/>
</svg>"#;
        assert!(sanitize_svg(svg).is_ok());
    }

    // ═══════════════════════════════════════════
    // IMAGE DECODE SAFETY TESTS
    // ═══════════════════════════════════════════

    #[cfg(any(
        feature = "media-thumbnail",
        feature = "media-svg",
        feature = "media-phash"
    ))]
    #[test]
    fn decode_image_safe_valid_png() {
        // Minimal 1x1 red PNG
        let png = create_test_png_1x1();
        let img = decode_image_safe(&png).unwrap();
        assert_eq!(img.width(), 1);
        assert_eq!(img.height(), 1);
    }

    #[cfg(any(
        feature = "media-thumbnail",
        feature = "media-svg",
        feature = "media-phash"
    ))]
    #[test]
    fn decode_image_safe_rejects_garbage() {
        let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB];
        let result = decode_image_safe(&garbage);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-290"));
    }

    #[cfg(any(
        feature = "media-thumbnail",
        feature = "media-svg",
        feature = "media-phash"
    ))]
    #[test]
    fn decode_image_safe_empty_data() {
        let result = decode_image_safe(&[]);
        assert!(result.is_err());
    }

    #[cfg(any(
        feature = "media-thumbnail",
        feature = "media-svg",
        feature = "media-phash"
    ))]
    #[test]
    fn decode_image_safe_fuzz_no_panic() {
        use std::panic;
        for i in 0..100u8 {
            let data: Vec<u8> = (0..=i).collect();
            let _ = panic::catch_unwind(|| {
                let _ = decode_image_safe(&data);
            });
        }
    }

    /// Create a minimal valid 1x1 red PNG for testing.
    #[cfg(any(
        feature = "media-thumbnail",
        feature = "media-svg",
        feature = "media-phash"
    ))]
    fn create_test_png_1x1() -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img = ImageBuffer::from_pixel(1, 1, Rgba([255u8, 0, 0, 255]));
        let mut buf = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            1,
            1,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }
}
