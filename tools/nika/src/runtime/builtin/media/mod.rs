//! Media Builtin Tools — nika:* media operations
//!
//! Provides image/audio/video processing tools accessible via `invoke: nika:thumbnail`, etc.
//!
//! # Architecture
//!
//! ```text
//! invoke: nika:thumbnail → BuiltinToolRouter → MediaToolAdapter → ThumbnailOp
//!                                                    │
//!                                                    ├── Parse JSON args
//!                                                    ├── Timeout wrap (30s)
//!                                                    ├── MediaOp::execute()
//!                                                    ├── CAS write (Binary result)
//!                                                    └── Serialize → JSON String
//! ```
//!
//! Each tool implements `MediaOp`. The `MediaToolAdapter` bridges `MediaOp` → `BuiltinTool`.

pub(crate) mod context;
pub(crate) mod error;
pub(crate) mod safety;

// Tool implementations
mod dimensions;
mod thumbhash_tool;
mod color;
#[cfg(feature = "media-thumbnail")]
mod thumbnail;
#[cfg(feature = "media-metadata")]
mod metadata;
#[cfg(feature = "media-optimize")]
mod optimize;
#[cfg(feature = "media-svg")]
mod svg;
#[cfg(feature = "media-thumbnail")]
mod convert;
#[cfg(feature = "media-thumbnail")]
mod strip;
mod import;
// PR3b tools
#[cfg(feature = "media-chart")]
mod chart;
#[cfg(feature = "media-phash")]
mod phash;
#[cfg(feature = "media-phash")]
mod compare;
#[cfg(feature = "media-pdf")]
mod pdf;
#[cfg(feature = "media-provenance")]
mod provenance;
#[cfg(feature = "media-provenance")]
mod verify;
#[cfg(feature = "media-qr")]
mod qr;
#[cfg(feature = "media-iqa")]
mod quality;
mod pipeline;
#[cfg(test)]
mod tests_integration;
#[cfg(test)]
mod tests_import_integration;
#[cfg(test)]
mod tests_pr3b_tools;
#[cfg(test)]
mod tests_paranoid;
#[cfg(test)]
mod tests_e2e_workflow;
#[cfg(test)]
mod tests_security;
#[cfg(test)]
mod tests_comprehensive;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::error::NikaError;
use super::BuiltinTool;
use context::MediaToolContext;
use error::timeout_error;

/// Default timeout for media operations (30 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Internal trait for media operations.
///
/// Each media tool (thumbnail, metadata, optimize, etc.) implements this.
/// The `MediaToolAdapter` bridges it to the `BuiltinTool` trait.
pub(crate) trait MediaOp: Send + Sync {
  /// Tool name without prefix (e.g., "thumbnail").
  fn name(&self) -> &'static str;

  /// Tool description for LLM discovery.
  fn description(&self) -> &'static str;

  /// JSON Schema for tool parameters.
  fn parameters_schema(&self) -> serde_json::Value;

  /// Execute the media operation.
  fn execute<'a>(
    &'a self,
    args: serde_json::Value,
    ctx: &'a MediaToolContext,
  ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, NikaError>> + Send + 'a>>;
}

/// Result of a media operation.
#[derive(Debug)]
pub(crate) enum MediaOpResult {
  /// Metadata-only result (dimensions, phash, etc.).
  Metadata(serde_json::Value),
  /// Binary output (thumbnail, optimized image, etc.).
  Binary {
    data: Vec<u8>,
    mime_type: String,
    extension: String,
    metadata: serde_json::Value,
  },
}

/// Adapter that bridges `MediaOp` → `BuiltinTool`.
///
/// Handles:
/// 1. JSON arg parsing
/// 2. Timeout wrapping (30s default)
/// 3. CAS write for Binary results
/// 4. Serialization to JSON String
pub(crate) struct MediaToolAdapter {
  op: Arc<dyn MediaOp>,
  ctx: Arc<MediaToolContext>,
}

impl MediaToolAdapter {
  pub fn new(op: Arc<dyn MediaOp>, ctx: Arc<MediaToolContext>) -> Self {
    Self { op, ctx }
  }
}

impl BuiltinTool for MediaToolAdapter {
  fn name(&self) -> &'static str {
    self.op.name()
  }

  fn description(&self) -> &'static str {
    self.op.description()
  }

  fn parameters_schema(&self) -> serde_json::Value {
    self.op.parameters_schema()
  }

  fn call<'a>(
    &'a self,
    args: String,
  ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
    Box::pin(async move {
      // Parse JSON args (outside timeout — parsing is instant)
      let parsed: serde_json::Value =
        serde_json::from_str(&args).map_err(|e| error::invalid_args(self.op.name(), e.to_string()))?;

      // Timeout wraps BOTH execute AND CAS write to prevent unbounded operations
      let tool_name = self.op.name();
      tokio::time::timeout(DEFAULT_TIMEOUT, async {
        let result = self.op.execute(parsed, &self.ctx).await?;

        match result {
          MediaOpResult::Metadata(value) => {
            serde_json::to_string(&value).map_err(|e| {
              error::tool_error(tool_name, format!("serialize metadata: {e}"))
            })
          }
          MediaOpResult::Binary {
            data,
            mime_type,
            extension,
            metadata,
          } => {
            // Store in CAS (inside timeout)
            let store_result = self.ctx.store_media(&data, "media_tool").await?;

            let response = serde_json::json!({
              "hash": store_result.hash,
              "path": store_result.path.to_string_lossy(),
              "size_bytes": store_result.size,
              "mime_type": mime_type,
              "extension": extension,
              "deduplicated": store_result.deduplicated,
              "metadata": metadata,
            });

            serde_json::to_string(&response).map_err(|e| {
              error::tool_error(tool_name, format!("serialize result: {e}"))
            })
          }
        }
      })
      .await
      .map_err(|_| timeout_error(tool_name))?
    })
  }
}

/// Create all media tool adapters for registration in the BuiltinToolRouter.
#[allow(clippy::vec_init_then_push)] // Feature-gated pushes require incremental construction
pub(crate) fn create_media_tool_adapters(ctx: Arc<MediaToolContext>) -> Vec<Box<dyn BuiltinTool>> {
  let mut tools: Vec<Box<dyn BuiltinTool>> = Vec::new();

  // Tier 1 — Always on (zero/tiny deps)
  tools.push(Box::new(MediaToolAdapter::new(
    Arc::new(import::ImportOp),
    Arc::clone(&ctx),
  )));
  tools.push(Box::new(MediaToolAdapter::new(
    Arc::new(dimensions::DimensionsOp),
    Arc::clone(&ctx),
  )));
  tools.push(Box::new(MediaToolAdapter::new(
    Arc::new(thumbhash_tool::ThumbhashOp),
    Arc::clone(&ctx),
  )));
  tools.push(Box::new(MediaToolAdapter::new(
    Arc::new(color::DominantColorOp),
    Arc::clone(&ctx),
  )));

  // Tier 2 — Feature-gated
  #[cfg(feature = "media-thumbnail")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(thumbnail::ThumbnailOp),
      Arc::clone(&ctx),
    )));
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(convert::ConvertOp),
      Arc::clone(&ctx),
    )));
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(strip::StripOp),
      Arc::clone(&ctx),
    )));
  }

  #[cfg(feature = "media-metadata")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(metadata::MetadataOp),
      Arc::clone(&ctx),
    )));
  }

  #[cfg(feature = "media-optimize")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(optimize::OptimizeOp),
      Arc::clone(&ctx),
    )));
  }

  #[cfg(feature = "media-svg")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(svg::SvgRenderOp),
      Arc::clone(&ctx),
    )));
  }

  // Tier 3 — PR3b tools
  #[cfg(feature = "media-chart")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(chart::ChartOp),
      Arc::clone(&ctx),
    )));
  }

  #[cfg(feature = "media-phash")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(phash::PhashOp),
      Arc::clone(&ctx),
    )));
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(compare::CompareOp),
      Arc::clone(&ctx),
    )));
  }

  #[cfg(feature = "media-pdf")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(pdf::PdfExtractOp),
      Arc::clone(&ctx),
    )));
  }

  #[cfg(feature = "media-provenance")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(provenance::ProvenanceOp),
      Arc::clone(&ctx),
    )));
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(verify::VerifyOp),
      Arc::clone(&ctx),
    )));
  }

  #[cfg(feature = "media-qr")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(qr::QrValidateOp),
      Arc::clone(&ctx),
    )));
  }

  #[cfg(feature = "media-iqa")]
  {
    tools.push(Box::new(MediaToolAdapter::new(
      Arc::new(quality::QualityOp),
      Arc::clone(&ctx),
    )));
  }

  // Pipeline — always on (orchestrates other tools)
  tools.push(Box::new(MediaToolAdapter::new(
    Arc::new(pipeline::PipelineOp),
    Arc::clone(&ctx),
  )));

  tools
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::media::CasStore;

  /// Dummy MediaOp for testing the adapter pattern.
  struct DummyOp;

  impl MediaOp for DummyOp {
    fn name(&self) -> &'static str {
      "dummy"
    }
    fn description(&self) -> &'static str {
      "A test tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
      serde_json::json!({
        "type": "object",
        "properties": {
          "value": { "type": "string" }
        }
      })
    }
    fn execute<'a>(
      &'a self,
      args: serde_json::Value,
      _ctx: &'a MediaToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, NikaError>> + Send + 'a>> {
      Box::pin(async move {
        let value = args
          .get("value")
          .and_then(|v| v.as_str())
          .unwrap_or("default");
        Ok(MediaOpResult::Metadata(serde_json::json!({
          "received": value
        })))
      })
    }
  }

  #[test]
  fn media_op_trait_compiles() {
    let op = DummyOp;
    assert_eq!(op.name(), "dummy");
    assert_eq!(op.description(), "A test tool");
  }

  #[tokio::test]
  async fn media_tool_adapter_dispatches() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));
    let adapter = MediaToolAdapter::new(Arc::new(DummyOp), ctx);

    let result = adapter
      .call(r#"{"value":"hello"}"#.to_string())
      .await
      .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["received"], "hello");
  }

  #[test]
  fn media_op_result_metadata_serializes() {
    let result = MediaOpResult::Metadata(serde_json::json!({
      "width": 100,
      "height": 200,
    }));
    if let MediaOpResult::Metadata(v) = result {
      let json = serde_json::to_string(&v).unwrap();
      assert!(json.contains("100"));
      assert!(json.contains("200"));
    }
  }

  #[tokio::test]
  async fn media_op_result_binary_stores_in_cas() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));

    struct BinaryOp;
    impl MediaOp for BinaryOp {
      fn name(&self) -> &'static str { "binary_test" }
      fn description(&self) -> &'static str { "" }
      fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
      fn execute<'a>(
        &'a self,
        _args: serde_json::Value,
        _ctx: &'a MediaToolContext,
      ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, NikaError>> + Send + 'a>> {
        Box::pin(async {
          Ok(MediaOpResult::Binary {
            data: b"fake png data here".to_vec(),
            mime_type: "image/png".to_string(),
            extension: "png".to_string(),
            metadata: serde_json::json!({"width": 256}),
          })
        })
      }
    }

    let adapter = MediaToolAdapter::new(Arc::new(BinaryOp), Arc::clone(&ctx));
    let result = adapter.call("{}".to_string()).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert!(parsed["hash"].as_str().unwrap().starts_with("blake3:"));
    assert_eq!(parsed["mime_type"], "image/png");
    assert_eq!(parsed["extension"], "png");
    assert_eq!(parsed["metadata"]["width"], 256);

    // Verify data is in CAS
    let hash = parsed["hash"].as_str().unwrap();
    let data = ctx.cas.read(hash).await.unwrap();
    assert_eq!(data, b"fake png data here");
  }

  #[tokio::test]
  async fn adapter_invalid_json_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));
    let adapter = MediaToolAdapter::new(Arc::new(DummyOp), ctx);

    let result = adapter.call("not json".to_string()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("NIKA-294"));
  }

  #[tokio::test]
  async fn create_media_tool_adapters_returns_tools() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())));
    let tools = create_media_tool_adapters(ctx);

    // At minimum: import, dimensions, thumbhash, dominant_color (always on)
    assert!(tools.len() >= 4, "expected >= 4 tools, got {}", tools.len());

    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(names.contains(&"import"));
    assert!(names.contains(&"dimensions"));
    assert!(names.contains(&"thumbhash"));
    assert!(names.contains(&"dominant_color"));
  }
}
