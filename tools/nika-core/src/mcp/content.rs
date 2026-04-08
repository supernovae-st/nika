// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ContentBlock and ResourceContent — MCP content protocol types.
//!
//! These types represent content exchanged via MCP tool calls.
//! Extracted from nika-mcp to break the nika-media → nika-mcp dependency.

use serde::{Deserialize, Serialize};

/// Content block from MCP tool call result.
///
/// Tagged by `type` field for JSON serialization. Each variant represents
/// a different content type that an MCP server can return.
///
/// Uses `#[serde(tag = "type")]` rather than a newtype wrapper because
/// the tag must appear as a field in the JSON map for deserialization.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Plain text content
    Text { text: String },

    /// Base64-encoded image with MIME type
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },

    /// Base64-encoded audio with MIME type
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },

    /// Embedded resource content (text or blob)
    Resource(ResourceContent),

    /// Resource link (URI reference without inline data)
    ResourceLink {
        uri: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
        mime_type: Option<String>,
    },
}

impl ContentBlock {
    /// Create a text content block.
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Create an image content block.
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Image {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }

    /// Create an audio content block.
    pub fn audio(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self::Audio {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }

    /// Create a resource content block.
    pub fn resource(resource: ResourceContent) -> Self {
        Self::Resource(resource)
    }

    /// Create a resource link content block.
    pub fn resource_link(
        uri: impl Into<String>,
        name: Option<String>,
        mime_type: Option<String>,
    ) -> Self {
        Self::ResourceLink {
            uri: uri.into(),
            name,
            mime_type,
        }
    }

    /// Check if this is a text block.
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }

    /// Check if this is an image block.
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image { .. })
    }

    /// Check if this is an audio block.
    pub fn is_audio(&self) -> bool {
        matches!(self, Self::Audio { .. })
    }

    /// Check if this is a resource block.
    pub fn is_resource(&self) -> bool {
        matches!(self, Self::Resource(_))
    }

    /// Check if this is a resource link block.
    pub fn is_resource_link(&self) -> bool {
        matches!(self, Self::ResourceLink { .. })
    }
}

/// Resource content from MCP server.
///
/// Represents a resource that can be read from the MCP server.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ResourceContent {
    /// Resource URI (e.g., "file:///path/to/file", "neo4j://entity/qr-code")
    pub uri: String,

    /// MIME type of the resource content
    #[serde(default, rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,

    /// Resource text content (if loaded)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Resource binary content as base64 (if loaded)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

impl ResourceContent {
    /// Create a new resource content with URI.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            mime_type: None,
            text: None,
            blob: None,
        }
    }

    /// Set the MIME type.
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Set the text content.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Set the blob content (base64-encoded binary).
    pub fn with_blob(mut self, blob: impl Into<String>) -> Self {
        self.blob = Some(blob.into());
        self
    }

    /// Set the MIME type if value is Some.
    pub fn with_optional_mime(mut self, mime_type: Option<String>) -> Self {
        if mime_type.is_some() {
            self.mime_type = mime_type;
        }
        self
    }
}
