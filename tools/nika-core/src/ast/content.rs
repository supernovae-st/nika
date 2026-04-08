// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Content parts for multimodal vision support.
//!
//! Three-phase types following the AST pipeline convention:
//! - `RawContentPart` — parsed from YAML with span tracking
//! - `AnalyzedContentPart` — validated, spans stripped
//! - `ContentPart` — runtime type used in `InferParams`
//!
//! # YAML Syntax
//!
//! ```yaml
//! content:
//!   - type: text
//!     text: "Describe this image"
//!   - type: image
//!     source: "{{with.photo.media[0].hash}}"
//!     detail: high
//!   - type: image_url
//!     url: "https://example.com/photo.jpg"
//!     detail: low
//! ```

use serde::{Deserialize, Serialize};

use crate::source::{Span, Spanned};

// ═══════════════════════════════════════════════════════════════
// Phase 1: Raw (from YAML parser, with spans)
// ═══════════════════════════════════════════════════════════════

/// Raw content part parsed from YAML with full span tracking.
#[derive(Debug, Clone)]
pub enum RawContentPart {
    /// Text content: `{ type: text, text: "..." }`
    Text { text: Spanned<String> },
    /// CAS image reference: `{ type: image, source: "blake3:...", detail: auto }`
    Image {
        source: Spanned<String>,
        detail: Option<Spanned<String>>,
    },
    /// External image URL: `{ type: image_url, url: "https://...", detail: low }`
    ImageUrl {
        url: Spanned<String>,
        detail: Option<Spanned<String>>,
    },
}

// ═══════════════════════════════════════════════════════════════
// Phase 2: Analyzed (validated, spans stripped)
// ═══════════════════════════════════════════════════════════════

/// Analyzed content part — validated and span-free.
#[derive(Debug, Clone)]
pub enum AnalyzedContentPart {
    Text { text: String },
    Image { source: String, detail: ImageDetail },
    ImageUrl { url: String, detail: ImageDetail },
}

// ═══════════════════════════════════════════════════════════════
// Phase 3: Runtime (used in InferParams, serde-enabled)
// ═══════════════════════════════════════════════════════════════

/// Runtime content part for multimodal inference.
///
/// Used in `InferParams.content` to specify text + image parts
/// sent to vision-capable LLMs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Text content
    Text { text: String },
    /// CAS image reference (resolved to base64 at runtime)
    Image {
        source: String,
        #[serde(default)]
        detail: ImageDetail,
    },
    /// External image URL (passed directly to provider)
    ImageUrl {
        url: String,
        #[serde(default)]
        detail: ImageDetail,
    },
}

/// Image detail level for vision models.
///
/// - `Auto`: Let the model decide (default)
/// - `Low`: Faster, cheaper, lower resolution
/// - `High`: Full resolution analysis
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail {
    #[default]
    Auto,
    Low,
    High,
}

impl std::fmt::Display for ImageDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageDetail::Auto => write!(f, "auto"),
            ImageDetail::Low => write!(f, "low"),
            ImageDetail::High => write!(f, "high"),
        }
    }
}

impl ImageDetail {
    /// Parse from string, defaulting to Auto for unknown values.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "low" => ImageDetail::Low,
            "high" => ImageDetail::High,
            _ => ImageDetail::Auto,
        }
    }
}

impl std::fmt::Display for ContentPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentPart::Text { text } => write!(f, "text({} chars)", text.len()),
            ContentPart::Image { source, detail } => {
                write!(f, "image(source={}, detail={})", source, detail)
            }
            ContentPart::ImageUrl { url, detail } => {
                write!(f, "image_url(url={}, detail={})", url, detail)
            }
        }
    }
}

/// Convert an analyzed content part to a runtime content part.
impl From<AnalyzedContentPart> for ContentPart {
    fn from(part: AnalyzedContentPart) -> Self {
        match part {
            AnalyzedContentPart::Text { text } => ContentPart::Text { text },
            AnalyzedContentPart::Image { source, detail } => ContentPart::Image { source, detail },
            AnalyzedContentPart::ImageUrl { url, detail } => ContentPart::ImageUrl { url, detail },
        }
    }
}

/// Convert a raw detail string to ImageDetail.
pub fn parse_detail(detail: Option<&Spanned<String>>) -> ImageDetail {
    detail
        .map(|s| ImageDetail::from_str_lossy(&s.value))
        .unwrap_or_default()
}

/// Analyze a raw content part into an analyzed content part.
pub fn analyze_content_part(raw: &RawContentPart) -> AnalyzedContentPart {
    match raw {
        RawContentPart::Text { text } => AnalyzedContentPart::Text {
            text: text.value.clone(),
        },
        RawContentPart::Image { source, detail } => AnalyzedContentPart::Image {
            source: source.value.clone(),
            detail: parse_detail(detail.as_ref()),
        },
        RawContentPart::ImageUrl { url, detail } => AnalyzedContentPart::ImageUrl {
            url: url.value.clone(),
            detail: parse_detail(detail.as_ref()),
        },
    }
}

/// Get the span of a raw content part (for error reporting).
impl RawContentPart {
    pub fn span(&self) -> Span {
        match self {
            RawContentPart::Text { text } => text.span,
            RawContentPart::Image { source, .. } => source.span,
            RawContentPart::ImageUrl { url, .. } => url.span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_part_serde_text_round_trip() {
        let part = ContentPart::Text {
            text: "Hello world".to_string(),
        };
        let json = serde_json::to_string(&part).unwrap();
        let parsed: ContentPart = serde_json::from_str(&json).unwrap();
        assert_eq!(part, parsed);
        assert!(json.contains(r#""type":"text""#));
    }

    #[test]
    fn content_part_serde_image_round_trip() {
        let part = ContentPart::Image {
            source: "blake3:abc123".to_string(),
            detail: ImageDetail::High,
        };
        let json = serde_json::to_string(&part).unwrap();
        let parsed: ContentPart = serde_json::from_str(&json).unwrap();
        assert_eq!(part, parsed);
        assert!(json.contains(r#""type":"image""#));
        assert!(json.contains(r#""detail":"high""#));
    }

    #[test]
    fn content_part_serde_image_url_round_trip() {
        let part = ContentPart::ImageUrl {
            url: "https://example.com/photo.jpg".to_string(),
            detail: ImageDetail::Low,
        };
        let json = serde_json::to_string(&part).unwrap();
        let parsed: ContentPart = serde_json::from_str(&json).unwrap();
        assert_eq!(part, parsed);
        assert!(json.contains(r#""type":"image_url""#));
    }

    #[test]
    fn content_part_serde_default_detail() {
        let json = r#"{"type":"image","source":"blake3:xyz"}"#;
        let part: ContentPart = serde_json::from_str(json).unwrap();
        match part {
            ContentPart::Image { detail, .. } => assert_eq!(detail, ImageDetail::Auto),
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn image_detail_display() {
        assert_eq!(ImageDetail::Auto.to_string(), "auto");
        assert_eq!(ImageDetail::Low.to_string(), "low");
        assert_eq!(ImageDetail::High.to_string(), "high");
    }

    #[test]
    fn image_detail_from_str_lossy() {
        assert_eq!(ImageDetail::from_str_lossy("low"), ImageDetail::Low);
        assert_eq!(ImageDetail::from_str_lossy("high"), ImageDetail::High);
        assert_eq!(ImageDetail::from_str_lossy("auto"), ImageDetail::Auto);
        assert_eq!(ImageDetail::from_str_lossy("unknown"), ImageDetail::Auto);
        assert_eq!(ImageDetail::from_str_lossy(""), ImageDetail::Auto);
    }

    #[test]
    fn image_detail_default_is_auto() {
        assert_eq!(ImageDetail::default(), ImageDetail::Auto);
    }

    #[test]
    fn content_part_display() {
        let text = ContentPart::Text {
            text: "hello".to_string(),
        };
        assert_eq!(text.to_string(), "text(5 chars)");

        let img = ContentPart::Image {
            source: "blake3:abc".to_string(),
            detail: ImageDetail::High,
        };
        assert_eq!(img.to_string(), "image(source=blake3:abc, detail=high)");
    }

    #[test]
    fn content_part_serde_vec_round_trip() {
        let parts = vec![
            ContentPart::Text {
                text: "Describe this:".to_string(),
            },
            ContentPart::Image {
                source: "blake3:deadbeef".to_string(),
                detail: ImageDetail::High,
            },
            ContentPart::ImageUrl {
                url: "https://example.com/img.png".to_string(),
                detail: ImageDetail::Auto,
            },
        ];
        let json = serde_json::to_string(&parts).unwrap();
        let parsed: Vec<ContentPart> = serde_json::from_str(&json).unwrap();
        assert_eq!(parts, parsed);
    }

    #[test]
    fn analyze_content_part_text() {
        let raw = RawContentPart::Text {
            text: Spanned::dummy("hello".to_string()),
        };
        let analyzed = analyze_content_part(&raw);
        match analyzed {
            AnalyzedContentPart::Text { text } => assert_eq!(text, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn analyze_content_part_image_with_detail() {
        let raw = RawContentPart::Image {
            source: Spanned::dummy("blake3:abc".to_string()),
            detail: Some(Spanned::dummy("high".to_string())),
        };
        let analyzed = analyze_content_part(&raw);
        match analyzed {
            AnalyzedContentPart::Image { source, detail } => {
                assert_eq!(source, "blake3:abc");
                assert_eq!(detail, ImageDetail::High);
            }
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn analyze_content_part_image_no_detail_defaults_auto() {
        let raw = RawContentPart::Image {
            source: Spanned::dummy("blake3:xyz".to_string()),
            detail: None,
        };
        let analyzed = analyze_content_part(&raw);
        match analyzed {
            AnalyzedContentPart::Image { detail, .. } => {
                assert_eq!(detail, ImageDetail::Auto);
            }
            _ => panic!("expected Image"),
        }
    }

    #[test]
    fn analyzed_to_runtime_conversion() {
        let analyzed = AnalyzedContentPart::Image {
            source: "blake3:test".to_string(),
            detail: ImageDetail::Low,
        };
        let runtime: ContentPart = analyzed.into();
        assert_eq!(
            runtime,
            ContentPart::Image {
                source: "blake3:test".to_string(),
                detail: ImageDetail::Low,
            }
        );
    }

    #[test]
    fn raw_content_part_span() {
        let span = Span::new(crate::source::FileId(0), 10, 20);
        let raw = RawContentPart::Text {
            text: Spanned::new("test".to_string(), span),
        };
        assert_eq!(raw.span(), span);
    }
}
