//! PR5 cross-feature integration tests.
//!
//! These tests verify that PR4 (Vision/Media) and PR5 (Fetch/Extract) features
//! work together on main. They exercise CAS round-trips, cross-tool pipelines,
//! FetchParams validation, and tool router registration.
//!
//! Run with:
//! ```sh
//! cargo test --lib --features "fetch-extract,fetch-article,media-provenance,media-qr,media-iqa" \
//!   -q -- tests_pr5_integration
//! ```

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::media::CasStore;
    use crate::runtime::builtin::media::context::MediaToolContext;
    #[allow(unused_imports)]
    use crate::runtime::builtin::media::{MediaOp, MediaOpResult};

    // ═══════════════════════════════════════════════════════════════
    // SETUP + FIXTURES
    // ═══════════════════════════════════════════════════════════════

    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    #[allow(dead_code)]
    fn fixture_png(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
        let mut buf = Vec::new();
        let enc = image::codecs::png::PngEncoder::new(&mut buf);
        image::ImageEncoder::write_image(enc, img.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        buf
    }

    #[allow(dead_code)]
    fn fixture_jpeg(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        use image::{ImageBuffer, Rgb};
        let img = ImageBuffer::from_pixel(w, h, Rgb([r, g, b]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        buf.into_inner()
    }

    /// Realistic HTML page fixture with OG tags, nav, article, footer, links.
    const FIXTURE_HTML: &str = r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <title>Nika PR5 Integration Test</title>
            <meta name="description" content="Cross-feature integration test page">
            <meta name="author" content="Nika Team">
            <meta name="robots" content="index, follow">
            <meta property="og:title" content="OG Integration Title">
            <meta property="og:description" content="OG integration description">
            <meta property="og:image" content="https://example.com/og-image.jpg">
            <meta property="og:type" content="article">
            <meta name="twitter:card" content="summary_large_image">
            <meta name="twitter:title" content="Twitter Integration Title">
            <link rel="canonical" href="https://example.com/integration">
            <link rel="icon" href="/favicon.ico">
            <link rel="alternate" type="application/rss+xml" title="RSS" href="/feed.xml">
            <script type="application/ld+json">
            {
                "@context": "https://schema.org",
                "@type": "Article",
                "headline": "Integration Test Article"
            }
            </script>
        </head>
        <body>
            <nav>
                <a href="/">Home</a>
                <a href="/about">About</a>
            </nav>
            <main>
                <article>
                    <h1>Cross-Feature Integration</h1>
                    <p>This article tests that PR4 vision tools and PR5 fetch extraction
                       tools share the same CAS infrastructure and can interoperate.
                       The content-addressable store is the central data plane for all
                       media and HTML content in Nika workflows.</p>
                    <p>When a workflow fetches a web page, stores it in CAS, and then
                       extracts metadata or converts to Markdown, each step reads from
                       and writes to the same CAS. Vision tasks can then reference
                       images by their CAS hash for multimodal LLM calls.</p>
                    <p>This integration ensures no regression across PRs.</p>
                    <a href="https://example.com/internal-link">Internal Link</a>
                    <a href="https://other-site.com/external" rel="nofollow">External Link</a>
                </article>
            </main>
            <footer>
                <a href="/privacy">Privacy</a>
                <p>Copyright 2026</p>
            </footer>
        </body>
        </html>
    "#;

    /// Article-heavy HTML fixture for readability tests.
    const ARTICLE_HTML: &str = r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <title>The State of AI in 2026</title>
            <meta name="author" content="Dr. Nika">
        </head>
        <body>
            <nav><a href="/">Home</a></nav>
            <article>
                <h1>The State of AI in 2026</h1>
                <p>Artificial intelligence has evolved dramatically over the past few years.
                   Large language models, multimodal systems, and agentic workflows are now
                   commonplace in production environments. Companies across every sector
                   rely on AI for content generation, code assistance, data analysis, and
                   customer support automation.</p>
                <p>The open-source ecosystem has flourished. Projects like Nika provide
                   semantic YAML workflow engines that orchestrate AI tasks with clear
                   provenance tracking and safety guarantees. Content-addressable storage
                   ensures reproducibility and auditability of every media artifact.</p>
                <p>Looking ahead, the convergence of vision, language, and tool-use
                   capabilities will enable even more sophisticated workflows. The key
                   challenge remains maintaining safety and transparency as AI systems
                   become more autonomous.</p>
                <p>In conclusion, 2026 represents a maturation point for AI infrastructure.
                   The tools exist. The patterns are established. What matters now is
                   responsible deployment at scale.</p>
            </article>
            <footer><p>Published 2026-03-20</p></footer>
        </body>
        </html>
    "#;

    // ═══════════════════════════════════════════════════════════════
    // SECTION 1: Fetch builtins + CAS round-trip
    //
    // Store HTML in CAS, then call each PR5 tool with the hash.
    // ═══════════════════════════════════════════════════════════════

    #[cfg(feature = "fetch-markdown")]
    mod html_to_md_cas {
        use super::*;
        use crate::runtime::builtin::media::html_to_md::HtmlToMdOp;

        #[tokio::test]
        async fn cas_html_to_md_round_trip() {
            let (_dir, ctx) = setup().await;

            // Store HTML in CAS
            let sr = ctx.cas.store(FIXTURE_HTML.as_bytes()).await.unwrap();
            assert!(
                sr.hash.starts_with("blake3:"),
                "CAS hash must be blake3-prefixed"
            );

            // Read from CAS via hash and convert to Markdown
            let result = HtmlToMdOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .expect("html_to_md should succeed on valid CAS HTML");

            match result {
                MediaOpResult::Metadata(v) => {
                    let md = v["markdown"].as_str().unwrap();
                    assert!(
                        md.contains("Cross-Feature Integration"),
                        "markdown should contain article heading: {md}"
                    );
                    assert!(
                        md.contains("content-addressable store"),
                        "markdown should contain article text: {md}"
                    );
                    assert!(
                        v["char_count"].as_u64().unwrap() > 100,
                        "should produce substantial markdown"
                    );
                }
                other => panic!("expected Metadata from html_to_md, got: {other:?}"),
            }
        }
    }

    #[cfg(feature = "fetch-html")]
    mod css_select_cas {
        use super::*;
        use crate::runtime::builtin::media::css_select::CssSelectOp;

        #[tokio::test]
        async fn cas_css_select_round_trip() {
            let (_dir, ctx) = setup().await;

            let sr = ctx.cas.store(FIXTURE_HTML.as_bytes()).await.unwrap();

            // Extract h1 elements via CSS selector
            let result = CssSelectOp
                .execute(serde_json::json!({"hash": sr.hash, "selector": "h1"}), &ctx)
                .await
                .expect("css_select should succeed on valid CAS HTML");

            match result {
                MediaOpResult::Metadata(v) => {
                    let matches = v["matches"].as_array().unwrap();
                    assert_eq!(matches.len(), 1);
                    assert_eq!(matches[0], "Cross-Feature Integration");
                    assert_eq!(v["count"], 1);
                }
                other => panic!("expected Metadata from css_select, got: {other:?}"),
            }
        }

        #[tokio::test]
        async fn cas_css_select_multiple_matches() {
            let (_dir, ctx) = setup().await;

            let sr = ctx.cas.store(FIXTURE_HTML.as_bytes()).await.unwrap();

            // Select all paragraph elements
            let result = CssSelectOp
                .execute(
                    serde_json::json!({"hash": sr.hash, "selector": "article p"}),
                    &ctx,
                )
                .await
                .unwrap();

            match result {
                MediaOpResult::Metadata(v) => {
                    let count = v["count"].as_u64().unwrap();
                    assert!(
                        count >= 3,
                        "article should have at least 3 paragraphs, got {count}"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }
    }

    #[cfg(feature = "fetch-html")]
    mod extract_metadata_cas {
        use super::*;
        use crate::runtime::builtin::media::extract_metadata::ExtractMetadataOp;

        #[tokio::test]
        async fn cas_extract_metadata_og_tags() {
            let (_dir, ctx) = setup().await;

            let sr = ctx.cas.store(FIXTURE_HTML.as_bytes()).await.unwrap();

            let result = ExtractMetadataOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .expect("extract_metadata should succeed on CAS HTML");

            match result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["title"], "Nika PR5 Integration Test");
                    assert_eq!(v["og"]["title"], "OG Integration Title");
                    assert_eq!(v["og"]["description"], "OG integration description");
                    assert_eq!(v["og"]["image"], "https://example.com/og-image.jpg");
                    assert_eq!(v["og"]["type"], "article");
                    assert_eq!(v["twitter"]["card"], "summary_large_image");
                    assert_eq!(v["canonical"], "https://example.com/integration");
                    assert_eq!(v["favicon"], "/favicon.ico");
                    assert_eq!(v["author"], "Nika Team");

                    // JSON-LD
                    let json_ld = v["json_ld"].as_array().unwrap();
                    assert_eq!(json_ld.len(), 1);
                    assert_eq!(json_ld[0]["@type"], "Article");
                    assert_eq!(json_ld[0]["headline"], "Integration Test Article");

                    // Feeds
                    let feeds = v["feeds"].as_array().unwrap();
                    assert_eq!(feeds.len(), 1);
                    assert_eq!(feeds[0]["href"], "/feed.xml");
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }
    }

    #[cfg(feature = "fetch-html")]
    mod extract_links_cas {
        use super::*;
        use crate::runtime::builtin::media::extract_links::ExtractLinksOp;

        #[tokio::test]
        async fn cas_extract_links_classifies() {
            let (_dir, ctx) = setup().await;

            let sr = ctx.cas.store(FIXTURE_HTML.as_bytes()).await.unwrap();

            let result = ExtractLinksOp
                .execute(
                    serde_json::json!({
                        "hash": sr.hash,
                        "base_url": "https://example.com"
                    }),
                    &ctx,
                )
                .await
                .expect("extract_links should succeed on CAS HTML");

            match result {
                MediaOpResult::Metadata(v) => {
                    let summary = &v["summary"];
                    let total = summary["total"].as_u64().unwrap();
                    assert!(total >= 4, "should find at least 4 links, got {total}");

                    let internal = summary["internal"].as_u64().unwrap();
                    let external = summary["external"].as_u64().unwrap();
                    assert_eq!(total, internal + external);
                    assert!(internal >= 3, "should have internal links");
                    assert!(external >= 1, "should have external links");
                    assert!(
                        summary["nofollow"].as_u64().unwrap() >= 1,
                        "should detect nofollow"
                    );

                    // Verify external link is other-site.com
                    let ext_links = v["external"].as_array().unwrap();
                    let hrefs: Vec<&str> = ext_links
                        .iter()
                        .filter_map(|l| l["href"].as_str())
                        .collect();
                    assert!(
                        hrefs.iter().any(|h| h.contains("other-site.com")),
                        "should find other-site.com external link: {hrefs:?}"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }
    }

    #[cfg(feature = "fetch-article")]
    mod readability_cas {
        use super::*;
        use crate::runtime::builtin::media::readability::ReadabilityOp;

        #[tokio::test]
        async fn cas_readability_extracts_article() {
            let (_dir, ctx) = setup().await;

            let sr = ctx.cas.store(ARTICLE_HTML.as_bytes()).await.unwrap();

            let result = ReadabilityOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .expect("readability should succeed on CAS HTML");

            match result {
                MediaOpResult::Metadata(v) => {
                    let text = v["text_content"].as_str().unwrap();
                    assert!(
                        text.contains("Artificial intelligence"),
                        "should extract article content: {text}"
                    );
                    assert!(
                        text.contains("Nika"),
                        "should contain Nika reference: {text}"
                    );
                    assert!(
                        v["char_count"].as_u64().unwrap() > 200,
                        "should have substantial article content"
                    );

                    // Footer content should be stripped
                    assert!(
                        !text.contains("Published 2026"),
                        "footer should be stripped from article: {text}"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SECTION 2: PR4 tools + PR5 tools combined
    //
    // Cross-tool pipelines that use both PR4 (media) and PR5 (fetch)
    // features in sequence.
    // ═══════════════════════════════════════════════════════════════

    #[cfg(feature = "media-iqa")]
    mod quality_self_compare {
        use super::*;
        use crate::runtime::builtin::media::quality::QualityOp;

        /// Import PNG, then compare it against itself via nika:quality.
        /// DSSIM between identical images must be ~0.
        #[tokio::test]
        async fn import_png_quality_self_compare_dssim_zero() {
            let (_dir, ctx) = setup().await;

            let png = fixture_png(64, 64, 100, 150, 200);
            let sr = ctx.cas.store(&png).await.unwrap();

            let result = QualityOp
                .execute(
                    serde_json::json!({
                        "hash_a": sr.hash,
                        "hash_b": sr.hash,
                    }),
                    &ctx,
                )
                .await
                .expect("quality self-compare should succeed");

            match result {
                MediaOpResult::Metadata(v) => {
                    let dssim = v["dssim"].as_f64().unwrap();
                    assert!(
                        dssim < 0.001,
                        "identical images should have DSSIM ~0, got {dssim}"
                    );
                    assert_eq!(
                        v["quality_grade"], "excellent",
                        "identical images must grade excellent"
                    );

                    let ssim = v["ssim"].as_f64().unwrap();
                    assert!(ssim > 0.99, "identical SSIM should be ~1.0, got {ssim}");
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }
    }

    #[cfg(feature = "media-provenance")]
    mod provenance_verify_pipeline {
        use super::*;
        use crate::runtime::builtin::media::provenance::ProvenanceOp;
        use crate::runtime::builtin::media::verify::VerifyOp;

        /// Import JPEG, sign with provenance, verify, check EU AI Act compliance.
        #[tokio::test]
        async fn import_sign_verify_eu_ai_act() {
            let (_dir, ctx) = setup().await;

            let jpeg = fixture_jpeg(80, 80, 42, 128, 200);
            let sr = ctx.cas.store(&jpeg).await.unwrap();

            // Sign
            let sign_result = ProvenanceOp
                .execute(
                    serde_json::json!({
                        "hash": sr.hash,
                        "assertion": "ai.generated",
                        "title": "PR5 Integration Test"
                    }),
                    &ctx,
                )
                .await
                .expect("provenance signing should succeed");

            let signed_data = match sign_result {
                MediaOpResult::Binary {
                    data,
                    mime_type,
                    metadata,
                    ..
                } => {
                    assert_eq!(mime_type, "image/jpeg");
                    assert_eq!(metadata["signed"], true);
                    assert_eq!(metadata["assertion"], "ai.generated");
                    data
                }
                other => panic!("expected Binary from provenance, got: {other:?}"),
            };

            // Store signed and verify
            let signed_sr = ctx.cas.store(&signed_data).await.unwrap();
            assert_ne!(signed_sr.hash, sr.hash, "signed hash must differ");

            let verify_result = VerifyOp
                .execute(serde_json::json!({"hash": signed_sr.hash}), &ctx)
                .await
                .expect("verify should succeed on signed image");

            match verify_result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["has_manifest"], true);
                    assert_eq!(
                        v["eu_ai_act_compliant"], true,
                        "ai.generated should be EU AI Act compliant"
                    );
                    assert_eq!(v["title"], "PR5 Integration Test");

                    let dst = v["digital_source_type"].as_str().unwrap();
                    assert!(
                        dst.contains("trainedAlgorithmicMedia"),
                        "should map to trainedAlgorithmicMedia, got: {dst}"
                    );

                    let cg = v["claim_generator"].as_str().unwrap();
                    assert!(
                        cg.contains("Nika"),
                        "claim generator should mention Nika: {cg}"
                    );
                }
                other => panic!("expected Metadata from verify, got: {other:?}"),
            }
        }
    }

    #[cfg(feature = "media-qr")]
    mod qr_validate_pipeline {
        use super::*;
        use crate::runtime::builtin::media::qr::QrValidateOp;

        /// Generate a QR code PNG using the `qrcode` crate (dev-dependency).
        fn fixture_qr_png(text: &str) -> Vec<u8> {
            use image::{ImageEncoder, Luma};

            let code = qrcode::QrCode::new(text.as_bytes()).expect("QR encode");
            let module_count = code.width() as u32;
            let scale = 8u32;
            let quiet = 4u32;
            let img_size = (module_count + quiet * 2) * scale;
            let mut img = image::GrayImage::from_pixel(img_size, img_size, Luma([255u8]));

            for y in 0..module_count {
                for x in 0..module_count {
                    if code[(x as usize, y as usize)] == qrcode::types::Color::Dark {
                        for dy in 0..scale {
                            for dx in 0..scale {
                                img.put_pixel(
                                    (x + quiet) * scale + dx,
                                    (y + quiet) * scale + dy,
                                    Luma([0u8]),
                                );
                            }
                        }
                    }
                }
            }

            let mut buf = Vec::new();
            let encoder = image::codecs::png::PngEncoder::new(&mut buf);
            encoder
                .write_image(
                    img.as_raw(),
                    img_size,
                    img_size,
                    image::ExtendedColorType::L8,
                )
                .unwrap();
            buf
        }

        /// Import QR PNG, call nika:qr_validate, check decoded data.
        #[tokio::test]
        async fn import_qr_validate_decoded_data() {
            let (_dir, ctx) = setup().await;

            let qr_text = "https://qrcode-ai.com/pr5-integration";
            let qr_png = fixture_qr_png(qr_text);
            let sr = ctx.cas.store(&qr_png).await.unwrap();

            let result = QrValidateOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .expect("qr_validate should succeed on valid QR");

            match result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["decoded"], true);
                    assert_eq!(v["data"], qr_text, "decoded data should match original");
                    let score = v["scan_score"].as_u64().unwrap();
                    assert!(score > 0, "scan_score should be > 0, got {score}");
                }
                other => panic!("expected Metadata from qr_validate, got: {other:?}"),
            }
        }
    }

    #[cfg(feature = "fetch-markdown")]
    mod html_to_md_cas_round_trip {
        use super::*;
        use crate::runtime::builtin::media::html_to_md::HtmlToMdOp;

        /// Store HTML -> html_to_md -> store markdown in CAS -> read back.
        #[tokio::test]
        async fn html_to_md_then_store_in_cas() {
            let (_dir, ctx) = setup().await;

            // Store HTML in CAS
            let html_sr = ctx.cas.store(FIXTURE_HTML.as_bytes()).await.unwrap();

            // Convert to Markdown
            let result = HtmlToMdOp
                .execute(serde_json::json!({"hash": html_sr.hash}), &ctx)
                .await
                .unwrap();

            let markdown = match result {
                MediaOpResult::Metadata(v) => {
                    let md = v["markdown"].as_str().unwrap().to_string();
                    assert!(!md.is_empty(), "markdown should not be empty");
                    md
                }
                other => panic!("expected Metadata, got: {other:?}"),
            };

            // Store the Markdown in CAS
            let md_sr = ctx.cas.store(markdown.as_bytes()).await.unwrap();
            assert_ne!(
                md_sr.hash, html_sr.hash,
                "markdown hash must differ from HTML hash"
            );

            // Read the Markdown back from CAS
            let read_back = ctx.read_media(&md_sr.hash).await.unwrap();
            let read_md = String::from_utf8(read_back).unwrap();
            assert!(
                read_md.contains("Cross-Feature Integration"),
                "CAS round-trip should preserve content"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SECTION 3: Vision + fetch concepts
    //
    // CAS interop tests that verify vision paths can use same CAS.
    // ═══════════════════════════════════════════════════════════════

    mod cas_vision_interop {
        use super::*;

        /// Store PNG in CAS and verify CAS read works (vision would use this hash).
        #[tokio::test]
        async fn store_png_cas_read_for_vision() {
            let (_dir, ctx) = setup().await;

            let png = fixture_png(32, 32, 255, 128, 0);
            let sr = ctx.cas.store(&png).await.unwrap();
            assert!(sr.hash.starts_with("blake3:"));

            // Read back (this is what vision dispatch does to resolve hash -> base64)
            let data = ctx.read_media(&sr.hash).await.unwrap();
            assert_eq!(data, png, "CAS read should return exact bytes");

            // Verify PNG magic
            assert_eq!(
                &data[..4],
                &[0x89, 0x50, 0x4E, 0x47],
                "read-back data should retain PNG magic"
            );
        }

        /// Store JPEG in CAS, read it back, verify size and magic.
        #[tokio::test]
        async fn store_jpeg_cas_read_for_vision() {
            let (_dir, ctx) = setup().await;

            let jpeg = fixture_jpeg(64, 64, 0, 200, 100);
            let sr = ctx.cas.store(&jpeg).await.unwrap();

            let data = ctx.read_media(&sr.hash).await.unwrap();
            assert_eq!(data.len(), jpeg.len());
            // JPEG magic: FF D8 FF
            assert_eq!(&data[..3], &[0xFF, 0xD8, 0xFF], "should retain JPEG magic");
        }
    }

    mod fetch_params_validation {
        use crate::ast::FetchParams;
        use rustc_hash::FxHashMap;

        /// FetchParams with response: binary validates correctly.
        #[test]
        fn fetch_params_response_binary_valid() {
            let params = FetchParams {
                url: "https://example.com/image.png".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: Some(nika_core::ast::extract::ResponseMode::Binary),
                extract: None,
                selector: None,
                session: None,
                cache: None,
            };
            assert!(
                params.validate().is_ok(),
                "response: binary should be valid"
            );
        }

        /// FetchParams with extract: markdown + selector validates correctly.
        #[test]
        fn fetch_params_extract_markdown_with_selector() {
            let params = FetchParams {
                url: "https://example.com/page".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: Some(nika_core::ast::extract::ExtractMode::Selector),
                selector: Some("div.content".to_string()),
                session: None,
                cache: None,
            };
            assert!(
                params.validate().is_ok(),
                "extract: selector with selector should be valid"
            );
        }

        /// FetchParams with extract: article is valid.
        #[test]
        fn fetch_params_extract_article_valid() {
            let params = FetchParams {
                url: "https://example.com/article".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: Some(nika_core::ast::extract::ExtractMode::Article),
                selector: None,
                session: None,
                cache: None,
            };
            assert!(
                params.validate().is_ok(),
                "extract: article should be valid"
            );
        }

        /// FetchParams with extract: feed is valid.
        #[test]
        fn fetch_params_extract_feed_valid() {
            let params = FetchParams {
                url: "https://example.com/feed.xml".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: Some(nika_core::ast::extract::ExtractMode::Feed),
                selector: None,
                session: None,
                cache: None,
            };
            assert!(params.validate().is_ok(), "extract: feed should be valid");
        }

        /// FetchParams with extract: llm_txt is valid.
        #[test]
        fn fetch_params_extract_llm_txt_valid() {
            let params = FetchParams {
                url: "https://example.com/llm.txt".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: Some(nika_core::ast::extract::ExtractMode::LlmTxt),
                selector: None,
                session: None,
                cache: None,
            };
            assert!(
                params.validate().is_ok(),
                "extract: llm_txt should be valid"
            );
        }

        /// FetchParams with extract: markdown validates correctly.
        #[test]
        fn fetch_params_extract_markdown_valid() {
            let params = FetchParams {
                url: "https://example.com/page".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: Some(nika_core::ast::extract::ExtractMode::Markdown),
                selector: None,
                session: None,
                cache: None,
            };
            assert!(
                params.validate().is_ok(),
                "extract: markdown should be valid"
            );
        }

        // Invalid extract modes are now prevented at the type level (ExtractMode enum).
        // The analyzer catches invalid strings from YAML at parse time.

        /// selector without extract is rejected.
        #[test]
        fn fetch_params_selector_without_extract_rejected() {
            let params = FetchParams {
                url: "https://example.com".to_string(),
                method: "GET".to_string(),
                headers: FxHashMap::default(),
                body: None,
                json: None,
                timeout: None,
                retry: None,
                follow_redirects: None,
                response: None,
                extract: None,
                selector: Some("div".to_string()),
                session: None,
                cache: None,
            };
            let err = params.validate().unwrap_err();
            assert!(
                err.to_string().contains("selector"),
                "error should mention selector: {err}"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SECTION 4: All 26 tool names exist in router
    //
    // Verify create_media_tool_adapters includes all expected tools.
    // ═══════════════════════════════════════════════════════════════

    mod tool_router_registration {
        use super::*;
        use crate::runtime::builtin::media::create_media_tool_adapters;

        #[test]
        fn all_media_tool_adapters_created() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);

            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            // Tier 1 — always on (4)
            assert!(names.contains(&"import"), "missing import: {names:?}");
            assert!(
                names.contains(&"dimensions"),
                "missing dimensions: {names:?}"
            );
            assert!(names.contains(&"thumbhash"), "missing thumbhash: {names:?}");
            assert!(
                names.contains(&"dominant_color"),
                "missing dominant_color: {names:?}"
            );

            // Pipeline — always on
            assert!(names.contains(&"pipeline"), "missing pipeline: {names:?}");

            // Verify tool count is at least tier-1 + pipeline
            assert!(
                tools.len() >= 5,
                "should have at least 5 tools (4 tier-1 + pipeline), got {}",
                tools.len()
            );
        }

        #[cfg(feature = "media-thumbnail")]
        #[test]
        fn tier2_thumbnail_tools_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(names.contains(&"thumbnail"), "missing thumbnail: {names:?}");
            assert!(names.contains(&"convert"), "missing convert: {names:?}");
            assert!(names.contains(&"strip"), "missing strip: {names:?}");
        }

        #[cfg(feature = "media-metadata")]
        #[test]
        fn tier2_metadata_tool_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(names.contains(&"metadata"), "missing metadata: {names:?}");
        }

        #[cfg(feature = "media-optimize")]
        #[test]
        fn tier2_optimize_tool_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(names.contains(&"optimize"), "missing optimize: {names:?}");
        }

        #[cfg(feature = "media-svg")]
        #[test]
        fn tier2_svg_render_tool_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(
                names.contains(&"svg_render"),
                "missing svg_render: {names:?}"
            );
        }

        #[cfg(feature = "media-phash")]
        #[test]
        fn tier3_phash_tools_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(names.contains(&"phash"), "missing phash: {names:?}");
            assert!(names.contains(&"compare"), "missing compare: {names:?}");
        }

        #[cfg(feature = "media-pdf")]
        #[test]
        fn tier3_pdf_extract_tool_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(
                names.contains(&"pdf_extract"),
                "missing pdf_extract: {names:?}"
            );
        }

        #[cfg(feature = "media-chart")]
        #[test]
        fn tier3_chart_tool_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(names.contains(&"chart"), "missing chart: {names:?}");
        }

        #[cfg(feature = "media-provenance")]
        #[test]
        fn tier3_provenance_tools_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(
                names.contains(&"provenance"),
                "missing provenance: {names:?}"
            );
            assert!(names.contains(&"verify"), "missing verify: {names:?}");
        }

        #[cfg(feature = "media-qr")]
        #[test]
        fn tier3_qr_validate_tool_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(
                names.contains(&"qr_validate"),
                "missing qr_validate: {names:?}"
            );
        }

        #[cfg(feature = "media-iqa")]
        #[test]
        fn tier3_quality_tool_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(names.contains(&"quality"), "missing quality: {names:?}");
        }

        #[cfg(feature = "fetch-html")]
        #[test]
        fn pr5_html_tools_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(
                names.contains(&"css_select"),
                "missing css_select: {names:?}"
            );
            assert!(
                names.contains(&"extract_metadata"),
                "missing extract_metadata: {names:?}"
            );
            assert!(
                names.contains(&"extract_links"),
                "missing extract_links: {names:?}"
            );
        }

        #[cfg(feature = "fetch-markdown")]
        #[test]
        fn pr5_html_to_md_tool_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(
                names.contains(&"html_to_md"),
                "missing html_to_md: {names:?}"
            );
        }

        #[cfg(feature = "fetch-article")]
        #[test]
        fn pr5_readability_tool_registered() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);
            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();

            assert!(
                names.contains(&"readability"),
                "missing readability: {names:?}"
            );
        }

        /// Verify all tool names are unique across all tiers.
        #[test]
        fn all_registered_tool_names_unique() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);

            let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
            let mut seen = std::collections::HashSet::new();
            for name in &names {
                assert!(
                    seen.insert(name),
                    "duplicate tool name '{name}' in registry: {names:?}"
                );
            }
        }

        /// Verify all registered tools have valid JSON Schema.
        #[test]
        fn all_registered_tools_have_valid_schema() {
            let dir = tempfile::tempdir().unwrap();
            let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
            let tools = create_media_tool_adapters(ctx);

            for tool in &tools {
                let schema = tool.parameters_schema();
                assert!(
                    schema.is_object(),
                    "tool '{}' schema should be an object",
                    tool.name()
                );
                assert_eq!(
                    schema["type"],
                    "object",
                    "tool '{}' schema type should be 'object'",
                    tool.name()
                );
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // SECTION 5: Error handling
    //
    // Invalid CAS hash, empty HTML, and cancelled context.
    // ═══════════════════════════════════════════════════════════════

    #[cfg(feature = "fetch-markdown")]
    mod html_to_md_errors {
        use super::*;
        use crate::runtime::builtin::media::html_to_md::HtmlToMdOp;

        #[tokio::test]
        async fn invalid_cas_hash_returns_error() {
            let (_dir, ctx) = setup().await;
            let result = HtmlToMdOp
                .execute(
                    serde_json::json!({"hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"}),
                    &ctx,
                )
                .await;
            assert!(result.is_err(), "invalid CAS hash should error");
        }

        /// Empty content cannot be stored in CAS (EmptyMediaContent error).
        /// Test with minimal HTML that produces near-empty output.
        #[tokio::test]
        async fn minimal_html_returns_minimal_markdown() {
            let (_dir, ctx) = setup().await;
            let sr = ctx.cas.store(b"<html></html>").await.unwrap();

            let result = HtmlToMdOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .unwrap();

            match result {
                MediaOpResult::Metadata(v) => {
                    // Minimal HTML should produce very little markdown
                    let char_count = v["char_count"].as_u64().unwrap();
                    assert!(
                        char_count < 10,
                        "minimal html should produce near-empty markdown, got {char_count} chars"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        #[tokio::test]
        async fn cancelled_context_returns_error() {
            let (_dir, ctx) = setup().await;
            ctx.cancel.cancel();

            let result = HtmlToMdOp
                .execute(serde_json::json!({"hash": "blake3:abc"}), &ctx)
                .await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("cancelled"));
        }
    }

    #[cfg(feature = "fetch-html")]
    mod css_select_errors {
        use super::*;
        use crate::runtime::builtin::media::css_select::CssSelectOp;

        #[tokio::test]
        async fn invalid_cas_hash_returns_error() {
            let (_dir, ctx) = setup().await;
            let result = CssSelectOp
                .execute(
                    serde_json::json!({
                        "hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000",
                        "selector": "p"
                    }),
                    &ctx,
                )
                .await;
            assert!(result.is_err(), "invalid CAS hash should error");
        }

        /// Minimal HTML with no matching elements returns zero matches.
        #[tokio::test]
        async fn minimal_html_returns_no_matches() {
            let (_dir, ctx) = setup().await;
            let sr = ctx.cas.store(b"<html><body></body></html>").await.unwrap();

            let result = CssSelectOp
                .execute(serde_json::json!({"hash": sr.hash, "selector": "p"}), &ctx)
                .await
                .unwrap();

            match result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["count"], 0);
                    assert!(v["matches"].as_array().unwrap().is_empty());
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        #[tokio::test]
        async fn cancelled_context_returns_error() {
            let (_dir, ctx) = setup().await;
            ctx.cancel.cancel();

            let result = CssSelectOp
                .execute(
                    serde_json::json!({"hash": "blake3:abc", "selector": "p"}),
                    &ctx,
                )
                .await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("cancelled"));
        }
    }

    #[cfg(feature = "fetch-html")]
    mod extract_metadata_errors {
        use super::*;
        use crate::runtime::builtin::media::extract_metadata::ExtractMetadataOp;

        #[tokio::test]
        async fn invalid_cas_hash_returns_error() {
            let (_dir, ctx) = setup().await;
            let result = ExtractMetadataOp
                .execute(
                    serde_json::json!({
                        "hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"
                    }),
                    &ctx,
                )
                .await;
            assert!(result.is_err(), "invalid CAS hash should error");
        }

        /// Minimal HTML with no metadata returns empty result.
        #[tokio::test]
        async fn minimal_html_returns_empty_metadata() {
            let (_dir, ctx) = setup().await;
            let sr = ctx.cas.store(b"<html><body></body></html>").await.unwrap();

            let result = ExtractMetadataOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await
                .unwrap();

            match result {
                MediaOpResult::Metadata(v) => {
                    assert!(
                        v.get("title").is_none(),
                        "minimal html should have no title"
                    );
                    assert!(v.get("og").is_none(), "minimal html should have no OG tags");
                    assert!(
                        v.get("twitter").is_none(),
                        "minimal html should have no twitter tags"
                    );
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        #[tokio::test]
        async fn cancelled_context_returns_error() {
            let (_dir, ctx) = setup().await;
            ctx.cancel.cancel();

            let result = ExtractMetadataOp
                .execute(serde_json::json!({"hash": "blake3:abc"}), &ctx)
                .await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("cancelled"));
        }
    }

    #[cfg(feature = "fetch-html")]
    mod extract_links_errors {
        use super::*;
        use crate::runtime::builtin::media::extract_links::ExtractLinksOp;

        #[tokio::test]
        async fn invalid_cas_hash_returns_error() {
            let (_dir, ctx) = setup().await;
            let result = ExtractLinksOp
                .execute(
                    serde_json::json!({
                        "hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000",
                        "base_url": "https://example.com"
                    }),
                    &ctx,
                )
                .await;
            assert!(result.is_err(), "invalid CAS hash should error");
        }

        /// Minimal HTML with no links returns zero total.
        #[tokio::test]
        async fn minimal_html_returns_zero_links() {
            let (_dir, ctx) = setup().await;
            let sr = ctx.cas.store(b"<html><body></body></html>").await.unwrap();

            let result = ExtractLinksOp
                .execute(
                    serde_json::json!({
                        "hash": sr.hash,
                        "base_url": "https://example.com"
                    }),
                    &ctx,
                )
                .await
                .unwrap();

            match result {
                MediaOpResult::Metadata(v) => {
                    assert_eq!(v["summary"]["total"], 0);
                }
                other => panic!("expected Metadata, got: {other:?}"),
            }
        }

        #[tokio::test]
        async fn cancelled_context_returns_error() {
            let (_dir, ctx) = setup().await;
            ctx.cancel.cancel();

            let result = ExtractLinksOp
                .execute(
                    serde_json::json!({
                        "hash": "blake3:abc",
                        "base_url": "https://example.com"
                    }),
                    &ctx,
                )
                .await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("cancelled"));
        }
    }

    #[cfg(feature = "fetch-article")]
    mod readability_errors {
        use super::*;
        use crate::runtime::builtin::media::readability::ReadabilityOp;

        #[tokio::test]
        async fn invalid_cas_hash_returns_error() {
            let (_dir, ctx) = setup().await;
            let result = ReadabilityOp
                .execute(
                    serde_json::json!({
                        "hash": "blake3:0000000000000000000000000000000000000000000000000000000000000000"
                    }),
                    &ctx,
                )
                .await;
            assert!(result.is_err(), "invalid CAS hash should error");
        }

        /// Minimal HTML with no article content returns near-empty result.
        #[tokio::test]
        async fn minimal_html_returns_minimal_article() {
            let (_dir, ctx) = setup().await;
            let sr = ctx
                .cas
                .store(b"<html><body><p>x</p></body></html>")
                .await
                .unwrap();

            let result = ReadabilityOp
                .execute(serde_json::json!({"hash": sr.hash}), &ctx)
                .await;

            // Readability may succeed with minimal content or fail to extract
            match result {
                Ok(MediaOpResult::Metadata(v)) => {
                    // If it succeeds, char_count should be very small
                    let char_count = v["char_count"].as_u64().unwrap();
                    assert!(
                        char_count < 50,
                        "minimal html should produce near-empty article, got {char_count}"
                    );
                }
                Err(e) => {
                    // Readability may fail on extremely minimal content -- that is acceptable
                    assert!(!e.to_string().contains("panicked"), "should not panic: {e}");
                }
                other => panic!("unexpected result: {other:?}"),
            }
        }

        #[tokio::test]
        async fn cancelled_context_returns_error() {
            let (_dir, ctx) = setup().await;
            ctx.cancel.cancel();

            let result = ReadabilityOp
                .execute(serde_json::json!({"hash": "blake3:abc"}), &ctx)
                .await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("cancelled"));
        }
    }
}
