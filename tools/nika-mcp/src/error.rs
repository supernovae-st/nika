// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use thiserror::Error;

use crate::types::McpErrorCode;

/// Errors from the MCP client system.
#[derive(Debug, Error)]
pub enum McpError {
    #[error("[NIKA-100] MCP server '{name}' not connected")]
    McpNotConnected { name: String },

    #[error("[NIKA-101] MCP server '{name}' failed to start: {reason}")]
    McpStartError { name: String, reason: String },

    #[error("[NIKA-102] MCP tool '{tool}' call failed{}: {reason}", error_code.map(|c| format!(" ({})", c)).unwrap_or_default())]
    McpToolError {
        tool: String,
        reason: String,
        error_code: Option<McpErrorCode>,
    },

    #[error("[NIKA-103] MCP resource not found: {uri}")]
    McpResourceNotFound { uri: String },

    #[error("[NIKA-104] MCP protocol error: {reason}")]
    McpProtocolError { reason: String },

    #[error("[NIKA-105] MCP server '{name}' not configured in workflow")]
    McpNotConfigured { name: String },

    #[error("[NIKA-106] MCP tool '{tool}' returned invalid response: {reason}")]
    McpInvalidResponse { tool: String, reason: String },

    #[error("[NIKA-107] MCP validation failed for tool '{tool}': {details}")]
    McpValidationFailed {
        tool: String,
        details: String,
        missing: Vec<String>,
        suggestions: Vec<String>,
    },

    #[error("[NIKA-108] MCP schema error for tool '{tool}': {reason}")]
    McpSchemaError { tool: String, reason: String },

    #[error(
        "[NIKA-109] MCP operation timed out for '{name}' ({operation}): exceeded {timeout_secs}s"
    )]
    McpTimeout {
        name: String,
        operation: String,
        timeout_secs: u64,
    },

    #[error("[NIKA-110] MCP tool call failed for '{tool}': {reason}")]
    McpToolCallFailed { tool: String, reason: String },

    #[error("Config error: {reason}")]
    ConfigError { reason: String },

    #[error("Validation error: {reason}")]
    ValidationError { reason: String },

    #[error("Parse error: {details}")]
    ParseError { details: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type for MCP operations.
pub type Result<T> = std::result::Result<T, McpError>;
