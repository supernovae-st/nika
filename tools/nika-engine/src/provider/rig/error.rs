// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error types for the rig provider layer.

use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════
// TOOL ERROR TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// MCP tool call error with semantic error kinds
///
/// Provides proper error semantics instead of wrapping in std::io::Error.
#[derive(Debug)]
pub struct McpToolError {
    kind: McpToolErrorKind,
    message: String,
}

/// Error kinds for MCP tool calls
#[derive(Debug, Clone, Copy)]
pub enum McpToolErrorKind {
    /// Invalid JSON arguments
    InvalidArguments,
    /// MCP client not configured
    NotConfigured,
    /// MCP tool call failed
    CallFailed,
    /// Failed to serialize/deserialize result
    SerializationError,
}

impl McpToolError {
    /// Create an invalid arguments error
    pub fn invalid_args(msg: impl Into<String>) -> Self {
        Self {
            kind: McpToolErrorKind::InvalidArguments,
            message: msg.into(),
        }
    }

    /// Create a not configured error
    pub fn not_configured(msg: impl Into<String>) -> Self {
        Self {
            kind: McpToolErrorKind::NotConfigured,
            message: msg.into(),
        }
    }

    /// Create a call failed error
    pub fn call_failed(msg: impl Into<String>) -> Self {
        Self {
            kind: McpToolErrorKind::CallFailed,
            message: msg.into(),
        }
    }

    /// Create a serialization error
    pub fn serialization(msg: impl Into<String>) -> Self {
        Self {
            kind: McpToolErrorKind::SerializationError,
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for McpToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind_str = match self.kind {
            McpToolErrorKind::InvalidArguments => "InvalidArguments",
            McpToolErrorKind::NotConfigured => "NotConfigured",
            McpToolErrorKind::CallFailed => "CallFailed",
            McpToolErrorKind::SerializationError => "SerializationError",
        };
        write!(f, "[{}] {}", kind_str, self.message)
    }
}

impl std::error::Error for McpToolError {}

// ═══════════════════════════════════════════════════════════════════════════
// Provider Verification Types
// ═══════════════════════════════════════════════════════════════════════════

/// Result of a successful provider verification
#[derive(Debug, Clone)]
pub struct ProviderVerifyResult {
    /// Provider name (claude, openai, etc.)
    pub provider: String,
    /// Round-trip latency for the test call
    pub latency: Duration,
    /// Model used for verification
    pub model: String,
}

/// Error during provider verification
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProviderVerifyError {
    #[error("Invalid API key for {provider}")]
    InvalidApiKey { provider: String },

    #[error("Rate limited by {provider}")]
    RateLimited { provider: String },

    #[error("Connection timeout to {provider}")]
    Timeout { provider: String },

    #[error("Network error connecting to {provider}: {details}")]
    NetworkError { provider: String, details: String },

    #[error("Provider error from {provider}: {details}")]
    ProviderError { provider: String, details: String },
}

impl ProviderVerifyError {
    /// Get a user-friendly suggestion for fixing the error
    pub fn suggestion(&self) -> &'static str {
        match self {
            ProviderVerifyError::InvalidApiKey { .. } => {
                "Check your API key in environment variables"
            }
            ProviderVerifyError::RateLimited { .. } => {
                "Wait a moment and try again, or check your plan limits"
            }
            ProviderVerifyError::Timeout { .. } => "Check your network connection or try again",
            ProviderVerifyError::NetworkError { .. } => {
                "Check your internet connection and firewall settings"
            }
            ProviderVerifyError::ProviderError { .. } => {
                "The provider service may be experiencing issues"
            }
        }
    }
}

/// Error type for RigProvider infer operations
#[derive(Debug, thiserror::Error)]
pub enum RigInferError {
    #[error("Completion error: {0}")]
    PromptError(String),

    /// Stream timeout - no chunk received within timeout period
    #[error("Stream timeout: no chunk received for {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    /// Provider does not support vision/multimodal content
    #[error("Vision not supported: {0}")]
    VisionNotSupported(String),
}
