// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Content part types for multi-modal infer prompts.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A part of a multi-modal content message.
///
/// Used in infer tasks to mix text, images, and other content types
/// within a single prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Plain text content.
    Text {
        /// The text content.
        text: String,
    },
    /// An image referenced by URL or base64.
    Image {
        /// Image source: URL or base64-encoded data.
        source: String,
        /// Optional media type (e.g. `image/png`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
    },
    /// A file attachment.
    File {
        /// File path (resolved relative to workflow).
        path: String,
        /// Optional media type override.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
    },
}

impl ContentPart {
    /// Create a text content part.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Create an image content part.
    #[must_use]
    pub fn image(source: impl Into<String>) -> Self {
        Self::Image {
            source: source.into(),
            media_type: None,
        }
    }

    /// Create a file content part.
    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self::File {
            path: path.into(),
            media_type: None,
        }
    }
}

impl fmt::Display for ContentPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text { text } => write!(f, "[text: {}]", truncate(text, 40)),
            Self::Image { source, .. } => write!(f, "[image: {}]", truncate(source, 40)),
            Self::File { path, .. } => write!(f, "[file: {path}]"),
        }
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..s.floor_char_boundary(max)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_constructor() {
        let part = ContentPart::text("Hello world");
        assert!(matches!(part, ContentPart::Text { text } if text == "Hello world"));
    }

    #[test]
    fn image_constructor() {
        let part = ContentPart::image("https://example.com/photo.png");
        assert!(
            matches!(part, ContentPart::Image { source, media_type } if source == "https://example.com/photo.png" && media_type.is_none())
        );
    }

    #[test]
    fn serde_roundtrip_text() {
        let part = ContentPart::text("Hello");
        let json = serde_json::to_string(&part).expect("serialize");
        assert!(json.contains(r#""type":"text""#));
        let back: ContentPart = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, part);
    }

    #[test]
    fn serde_roundtrip_image_with_media_type() {
        let part = ContentPart::Image {
            source: "data:image/png;base64,ABC".into(),
            media_type: Some("image/png".into()),
        };
        let json = serde_json::to_string(&part).expect("serialize");
        let back: ContentPart = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, part);
    }

    #[test]
    fn display_truncates() {
        let long = "a".repeat(100);
        let part = ContentPart::text(&long);
        let display = part.to_string();
        assert!(display.len() < 60);
    }
}
