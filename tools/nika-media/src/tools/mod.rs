//! Media Builtin Tools — nika:* media operations
//!
//! Each tool implements `MediaOp`. The engine wraps these with an adapter
//! that bridges to its `BuiltinTool` trait.

pub mod context;
pub mod error;
pub mod safety;

// Tool implementations
#[cfg(feature = "media-chart")]
pub mod chart;
pub mod color;
#[cfg(feature = "media-phash")]
pub mod compare;
pub mod decode;
#[cfg(feature = "media-thumbnail")]
pub mod convert;
#[cfg(feature = "fetch-html")]
pub mod css_select;
pub mod dimensions;
#[cfg(feature = "fetch-html")]
pub mod extract_links;
#[cfg(feature = "fetch-html")]
pub mod extract_metadata;
#[cfg(feature = "fetch-markdown")]
pub mod html_to_md;
pub mod import;
#[cfg(feature = "media-metadata")]
pub mod metadata;
#[cfg(feature = "media-optimize")]
pub mod optimize;
#[cfg(feature = "media-pdf")]
pub mod pdf;
#[cfg(feature = "media-phash")]
pub mod phash;
pub mod pipeline;
#[cfg(feature = "media-provenance")]
pub mod provenance;
#[cfg(feature = "media-qr")]
pub mod qr;
#[cfg(feature = "media-iqa")]
pub mod quality;
#[cfg(feature = "fetch-article")]
pub mod readability;
#[cfg(feature = "media-thumbnail")]
pub mod strip;
#[cfg(feature = "media-svg")]
pub mod svg;
pub mod thumbhash_tool;
#[cfg(feature = "media-thumbnail")]
pub mod thumbnail;
#[cfg(feature = "media-provenance")]
pub mod verify;

use std::future::Future;
use std::pin::Pin;

pub use context::MediaToolContext;
use error::MediaToolError;

/// Internal trait for media operations.
///
/// Each media tool (thumbnail, metadata, optimize, etc.) implements this.
pub trait MediaOp: Send + Sync {
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
    ) -> Pin<Box<dyn Future<Output = Result<MediaOpResult, MediaToolError>> + Send + 'a>>;
}

/// Result of a media operation.
#[derive(Debug)]
pub enum MediaOpResult {
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

/// Create all media tool operations (feature-gated).
///
/// The engine wraps each in a `MediaToolAdapter` that bridges to `BuiltinTool`.
#[allow(clippy::vec_init_then_push)]
pub fn create_all_media_ops() -> Vec<Box<dyn MediaOp>> {
    let mut ops: Vec<Box<dyn MediaOp>> = Vec::new();

    // Tier 1 — Always on
    ops.push(Box::new(import::ImportOp));
    ops.push(Box::new(decode::DecodeOp));
    ops.push(Box::new(dimensions::DimensionsOp));
    ops.push(Box::new(thumbhash_tool::ThumbhashOp));
    ops.push(Box::new(color::DominantColorOp));

    // Tier 2 — Feature-gated
    #[cfg(feature = "media-thumbnail")]
    {
        ops.push(Box::new(thumbnail::ThumbnailOp));
        ops.push(Box::new(convert::ConvertOp));
        ops.push(Box::new(strip::StripOp));
    }

    #[cfg(feature = "media-metadata")]
    ops.push(Box::new(metadata::MetadataOp));

    #[cfg(feature = "media-optimize")]
    ops.push(Box::new(optimize::OptimizeOp));

    #[cfg(feature = "media-svg")]
    ops.push(Box::new(svg::SvgRenderOp));

    // Tier 3
    #[cfg(feature = "media-chart")]
    ops.push(Box::new(chart::ChartOp));

    #[cfg(feature = "media-phash")]
    {
        ops.push(Box::new(phash::PhashOp));
        ops.push(Box::new(compare::CompareOp));
    }

    #[cfg(feature = "media-pdf")]
    ops.push(Box::new(pdf::PdfExtractOp));

    #[cfg(feature = "media-provenance")]
    {
        ops.push(Box::new(provenance::ProvenanceOp));
        ops.push(Box::new(verify::VerifyOp));
    }

    #[cfg(feature = "media-qr")]
    ops.push(Box::new(qr::QrValidateOp));

    #[cfg(feature = "media-iqa")]
    ops.push(Box::new(quality::QualityOp));

    // Web extraction builtins
    #[cfg(feature = "fetch-html")]
    {
        ops.push(Box::new(css_select::CssSelectOp));
        ops.push(Box::new(extract_metadata::ExtractMetadataOp));
        ops.push(Box::new(extract_links::ExtractLinksOp));
    }

    #[cfg(feature = "fetch-markdown")]
    ops.push(Box::new(html_to_md::HtmlToMdOp));

    #[cfg(feature = "fetch-article")]
    ops.push(Box::new(readability::ReadabilityOp));

    // Pipeline — always on (orchestrates other tools)
    ops.push(Box::new(pipeline::PipelineOp));

    ops
}
