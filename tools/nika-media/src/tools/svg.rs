//! nika:svg_render — SVG to PNG rasterization.
//!
//! Uses resvg + usvg + tiny-skia. SECURITY: sanitize_svg() BEFORE parsing.
//! Resources_dir is ALWAYS None (no external resource loading).

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use super::context::MediaToolContext;
use super::error::MediaToolError;
use super::error::{invalid_args, tool_error};
use super::safety::sanitize_svg;
use super::{MediaOp, MediaOpResult};

/// Lazy-loaded fontdb database (expensive first-time init).
static FONTDB: OnceLock<Arc<fontdb::Database>> = OnceLock::new();

fn get_fontdb() -> Arc<fontdb::Database> {
    FONTDB
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            if db.is_empty() {
                tracing::warn!(
                    "svg_render: no system fonts found — text in SVGs will not render. \
         Install fonts or use a container with font packages."
                );
            }
            Arc::new(db)
        })
        .clone()
}

pub struct SvgRenderOp;

impl MediaOp for SvgRenderOp {
    fn name(&self) -> &'static str {
        "svg_render"
    }

    fn description(&self) -> &'static str {
        "Render SVG to PNG (rasterize vector graphics)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
          "type": "object",
          "properties": {
            "hash": { "type": "string", "description": "CAS hash of the SVG file" },
            "width": { "type": "integer", "description": "Output width (optional)" },
            "height": { "type": "integer", "description": "Output height (optional)" }
          },
          "required": ["hash"],
          "additionalProperties": false
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a MediaToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, MediaToolError>> + Send + 'a>> {
        Box::pin(async move {
            ctx.check_cancelled()?;
            let hash = args
                .get("hash")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid_args("svg_render", "missing 'hash'"))?;
            let req_width = args.get("width").and_then(|v| v.as_u64()).map(|w| w as u32);
            let req_height = args
                .get("height")
                .and_then(|v| v.as_u64())
                .map(|h| h as u32);

            let data = ctx.read_media(hash).await?;

            let svg_str = String::from_utf8(data)
                .map_err(|_| tool_error("svg_render", "SVG is not valid UTF-8"))?;

            // SECURITY: sanitize BEFORE any parsing
            sanitize_svg(&svg_str)?;

            let png_data = ctx
                .compute
                .compute(move || -> Result<Vec<u8>, MediaToolError> {
                    let fontdb = get_fontdb();

                    let opts = usvg::Options {
                        resources_dir: None, // SECURITY: never load external resources
                        fontdb,
                        ..Default::default()
                    };

                    let tree = usvg::Tree::from_str(&svg_str, &opts)
                        .map_err(|e| tool_error("svg_render", format!("SVG parse: {e}")))?;

                    let svg_size = tree.size();

                    // Guard: reject degenerate 0x0 viewBox SVGs (division by zero)
                    if svg_size.width() == 0.0 || svg_size.height() == 0.0 {
                        return Err(invalid_args(
                            "svg_render",
                            "SVG has zero-dimension viewBox",
                        ));
                    }

                    let (w, h) = match (req_width, req_height) {
                        (Some(w), Some(h)) => (w, h),
                        (Some(w), None) => {
                            let ratio = svg_size.height() / svg_size.width();
                            (w, (w as f32 * ratio).round().max(1.0) as u32)
                        }
                        (None, Some(h)) => {
                            let ratio = svg_size.width() / svg_size.height();
                            ((h as f32 * ratio).round().max(1.0) as u32, h)
                        }
                        (None, None) => (
                            svg_size.width().round().max(1.0) as u32,
                            svg_size.height().round().max(1.0) as u32,
                        ),
                    };

                    // Reject out-of-range dimensions (consistent with thumbnail)
                    if w == 0 || w > 10_000 || h == 0 || h > 10_000 {
                        return Err(invalid_args(
                            "svg_render",
                            format!("dimensions out of range: {w}x{h} (max 10000x10000)"),
                        ));
                    }

                    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or_else(|| {
                        tool_error("svg_render", format!("failed to create {w}x{h} pixmap"))
                    })?;

                    let transform = tiny_skia::Transform::from_scale(
                        w as f32 / svg_size.width(),
                        h as f32 / svg_size.height(),
                    );

                    resvg::render(&tree, transform, &mut pixmap.as_mut());

                    pixmap
                        .encode_png()
                        .map_err(|e| tool_error("svg_render", format!("PNG encode: {e}")))
                })
                .await??;

            Ok(MediaOpResult::Binary {
                data: png_data,
                mime_type: "image/png".to_string(),
                extension: "png".to_string(),
                metadata: serde_json::json!({
                  "source_format": "svg",
                }),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CasStore;

    async fn setup() -> (tempfile::TempDir, Arc<MediaToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        (dir, ctx)
    }

    #[tokio::test]
    async fn svg_render_basic_shape() {
        let (_dir, ctx) = setup().await;
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
      <rect width="100" height="100" fill="blue"/>
    </svg>"#;
        let sr = ctx.cas.store(svg.as_bytes()).await.unwrap();

        let op = SvgRenderOp;
        let result = op
            .execute(serde_json::json!({"hash": sr.hash}), &ctx)
            .await
            .unwrap();

        if let MediaOpResult::Binary {
            data, mime_type, ..
        } = result
        {
            assert_eq!(mime_type, "image/png");
            assert_eq!(&data[..4], &[137, 80, 78, 71]); // PNG magic
        }
    }

    #[tokio::test]
    async fn svg_render_custom_dimensions() {
        let (_dir, ctx) = setup().await;
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100">
      <circle cx="100" cy="50" r="50" fill="red"/>
    </svg>"#;
        let sr = ctx.cas.store(svg.as_bytes()).await.unwrap();

        let op = SvgRenderOp;
        let result = op
            .execute(
                serde_json::json!({
                  "hash": sr.hash, "width": 400, "height": 200
                }),
                &ctx,
            )
            .await
            .unwrap();

        if let MediaOpResult::Binary { data, .. } = result {
            assert!(!data.is_empty());
        }
    }

    #[tokio::test]
    async fn svg_render_script_tag_rejected() {
        let (_dir, ctx) = setup().await;
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        let sr = ctx.cas.store(svg.as_bytes()).await.unwrap();

        let op = SvgRenderOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    #[tokio::test]
    async fn svg_render_foreign_object_rejected() {
        let (_dir, ctx) = setup().await;
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
      <foreignObject width="100" height="100">
        <body xmlns="http://www.w3.org/1999/xhtml"><div>html</div></body>
      </foreignObject>
    </svg>"#;
        let sr = ctx.cas.store(svg.as_bytes()).await.unwrap();

        let op = SvgRenderOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    #[tokio::test]
    async fn svg_render_event_handler_rejected() {
        let (_dir, ctx) = setup().await;
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)">
      <rect width="10" height="10"/>
    </svg>"#;
        let sr = ctx.cas.store(svg.as_bytes()).await.unwrap();

        let op = SvgRenderOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-297"));
    }

    #[tokio::test]
    async fn svg_render_invalid_svg() {
        let (_dir, ctx) = setup().await;
        let sr = ctx.cas.store(b"not valid xml at all").await.unwrap();

        let op = SvgRenderOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn svg_render_rejects_oversized_dimensions() {
        let (_dir, ctx) = setup().await;
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
      <rect width="100" height="100" fill="blue"/>
    </svg>"#;
        let sr = ctx.cas.store(svg.as_bytes()).await.unwrap();

        let op = SvgRenderOp;
        let result = op
            .execute(
                serde_json::json!({"hash": sr.hash, "width": 99999}),
                &ctx,
            )
            .await;
        assert!(result.is_err(), "width=99999 must be rejected, not clamped");
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn svg_render_zero_viewbox_rejected() {
        let (_dir, ctx) = setup().await;
        // viewBox 0 0 0 0 is degenerate — would cause div-by-zero
        let svg =
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 0 0" width="0" height="0"></svg>"#;
        let sr = ctx.cas.store(svg.as_bytes()).await.unwrap();

        let op = SvgRenderOp;
        let result = op.execute(serde_json::json!({"hash": sr.hash}), &ctx).await;
        // Either parse error or our zero-dimension guard
        assert!(result.is_err(), "0x0 viewBox must be rejected");
    }
}
