// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Media Builtin Tools — thin adapter layer
//!
//! Tool implementations live in `nika-media::tools`. This module provides:
//! - `MediaToolAdapter`: bridges `MediaOp` → `BuiltinTool`
//! - `create_media_tool_adapters()`: factory for all media tools
//! - Re-exports for test compatibility

// Re-export context (used by router.rs and executor/mod.rs)
pub(crate) use nika_media::tools::context;

// Re-export tool modules from nika-media for test compatibility
#[cfg(all(test, feature = "media-chart"))]
pub(crate) use nika_media::tools::chart;
#[cfg(all(test, feature = "fetch-markdown"))]
pub(crate) use nika_media::tools::html_to_md;
#[cfg(all(test, feature = "media-metadata"))]
pub(crate) use nika_media::tools::metadata;
#[cfg(all(test, feature = "media-optimize"))]
pub(crate) use nika_media::tools::optimize;
#[cfg(all(test, feature = "media-pdf"))]
pub(crate) use nika_media::tools::pdf;
#[cfg(all(test, feature = "media-qr"))]
pub(crate) use nika_media::tools::qr;
#[cfg(all(test, feature = "media-iqa"))]
pub(crate) use nika_media::tools::quality;
#[cfg(all(test, feature = "fetch-article"))]
pub(crate) use nika_media::tools::readability;
#[cfg(all(test, feature = "media-svg"))]
pub(crate) use nika_media::tools::svg;
#[cfg(test)]
pub(crate) use nika_media::tools::{color, dimensions, import, safety, thumbhash_tool};
#[cfg(all(test, feature = "media-phash"))]
pub(crate) use nika_media::tools::{compare, phash};
#[cfg(all(test, feature = "media-thumbnail"))]
pub(crate) use nika_media::tools::{convert, strip, thumbnail};
#[cfg(all(test, feature = "fetch-html"))]
pub(crate) use nika_media::tools::{css_select, extract_links, extract_metadata};
#[cfg(all(test, feature = "media-provenance"))]
pub(crate) use nika_media::tools::{provenance, verify};

// Re-export core types (used by adapter + tests)
pub(crate) use nika_media::tools::{MediaOp, MediaOpResult, MediaToolContext};

// Test modules (remain in nika-engine — they test integration with engine types)
#[cfg(test)]
mod tests_comprehensive;
#[cfg(test)]
mod tests_e2e_workflow;
#[cfg(test)]
mod tests_import_integration;
#[cfg(test)]
mod tests_integration;
#[cfg(test)]
mod tests_paranoid;
#[cfg(test)]
mod tests_pr3b_tools;
#[cfg(test)]
mod tests_pr4_pipelines;
#[cfg(test)]
mod tests_pr5_integration;
#[cfg(test)]
mod tests_security;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::BuiltinTool;
use crate::error::NikaError;
use nika_media::tools::error::{invalid_args, timeout_error, tool_error};

/// Default timeout for media operations (30 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Adapter that bridges `MediaOp` → `BuiltinTool`.
///
/// Handles: JSON arg parsing, timeout wrapping, CAS write, serialization.
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
            let parsed: serde_json::Value = serde_json::from_str(&args)
                .map_err(|e| NikaError::from(invalid_args(self.op.name(), e.to_string())))?;

            let tool_name = self.op.name();
            tokio::time::timeout(DEFAULT_TIMEOUT, async {
                let result = self
                    .op
                    .execute(parsed, &self.ctx)
                    .await
                    .map_err(NikaError::from)?;

                match result {
                    MediaOpResult::Metadata(value) => serde_json::to_string(&value).map_err(|e| {
                        NikaError::from(tool_error(tool_name, format!("serialize metadata: {e}")))
                    }),
                    MediaOpResult::Binary {
                        data,
                        mime_type,
                        extension,
                        metadata,
                    } => {
                        let store_result = self
                            .ctx
                            .store_media(&data, "media_tool")
                            .await
                            .map_err(NikaError::from)?;

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
                            NikaError::from(tool_error(tool_name, format!("serialize result: {e}")))
                        })
                    }
                }
            })
            .await
            .map_err(|_| NikaError::from(timeout_error(tool_name)))?
        })
    }
}

/// Create all media tool adapters for registration in the BuiltinToolRouter.
pub(crate) fn create_media_tool_adapters(ctx: Arc<MediaToolContext>) -> Vec<Box<dyn BuiltinTool>> {
    nika_media::tools::create_all_media_ops()
        .into_iter()
        .map(|op| {
            Box::new(MediaToolAdapter::new(Arc::from(op), Arc::clone(&ctx))) as Box<dyn BuiltinTool>
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::CasStore;
    use nika_media::tools::error::MediaToolError;

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
        ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, MediaToolError>> + Send + 'a>>
        {
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
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
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
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());

        struct BinaryOp;
        impl MediaOp for BinaryOp {
            fn name(&self) -> &'static str {
                "binary_test"
            }
            fn description(&self) -> &'static str {
                ""
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            fn execute<'a>(
                &'a self,
                _args: serde_json::Value,
                _ctx: &'a MediaToolContext,
            ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, MediaToolError>> + Send + 'a>>
            {
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

        let hash = parsed["hash"].as_str().unwrap();
        let data = ctx.cas.read(hash).await.unwrap();
        assert_eq!(data, b"fake png data here");
    }

    #[tokio::test]
    async fn adapter_invalid_json_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let adapter = MediaToolAdapter::new(Arc::new(DummyOp), ctx);

        let result = adapter.call("not json".to_string()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-294"));
    }

    #[tokio::test]
    async fn create_media_tool_adapters_returns_tools() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Arc::new(MediaToolContext::new(CasStore::new(dir.path())).unwrap());
        let tools = create_media_tool_adapters(ctx);

        assert!(tools.len() >= 4, "expected >= 4 tools, got {}", tools.len());

        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"import"));
        assert!(names.contains(&"dimensions"));
        assert!(names.contains(&"thumbhash"));
        assert!(names.contains(&"dominant_color"));
    }
}
