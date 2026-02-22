//! MCP Protocol Types shared with nika-core
//!
//! These types are defined in nika-core because they're needed by error.rs.
//! The nika-mcp crate re-exports these types.

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// MCP JSON-RPC Error Codes (v0.5.3)
// ═══════════════════════════════════════════════════════════════════════════

/// MCP JSON-RPC error codes per MCP specification.
///
/// These error codes follow the JSON-RPC 2.0 specification and are preserved
/// from rmcp errors for better debugging and error handling.
///
/// # Error Code Ranges
///
/// - `-32700`: Parse error (invalid JSON)
/// - `-32600`: Invalid request
/// - `-32601`: Method not found
/// - `-32602`: Invalid params
/// - `-32603`: Internal error
/// - `-32000` to `-32099`: Server errors (implementation-defined)
///
/// # Example
///
/// ```rust
/// use nika_core::mcp_types::McpErrorCode;
///
/// let code = McpErrorCode::from_code(-32602);
/// assert_eq!(code, McpErrorCode::InvalidParams);
/// assert!(code.is_client_error());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "i32", try_from = "i32")]
pub enum McpErrorCode {
    /// Parse error: Invalid JSON was received by the server (-32700)
    ParseError,
    /// Invalid request: The JSON sent is not a valid Request object (-32600)
    InvalidRequest,
    /// Method not found: The method does not exist / is not available (-32601)
    MethodNotFound,
    /// Invalid params: Invalid method parameter(s) (-32602)
    InvalidParams,
    /// Internal error: Internal JSON-RPC error (-32603)
    InternalError,
    /// Server error: Implementation-defined server errors (-32000 to -32099)
    ServerError(i32),
    /// Unknown error code (not in JSON-RPC spec)
    Unknown(i32),
}

impl McpErrorCode {
    /// Create an error code from a numeric JSON-RPC error code.
    pub fn from_code(code: i32) -> Self {
        match code {
            -32700 => Self::ParseError,
            -32600 => Self::InvalidRequest,
            -32601 => Self::MethodNotFound,
            -32602 => Self::InvalidParams,
            -32603 => Self::InternalError,
            c if (-32099..=-32000).contains(&c) => Self::ServerError(c),
            c => Self::Unknown(c),
        }
    }

    /// Get the numeric error code.
    pub fn code(&self) -> i32 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            Self::ServerError(c) | Self::Unknown(c) => *c,
        }
    }

    /// Check if this is a client-side error (invalid request/params).
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::ParseError | Self::InvalidRequest | Self::InvalidParams
        )
    }

    /// Check if this is a server-side error.
    pub fn is_server_error(&self) -> bool {
        matches!(
            self,
            Self::InternalError | Self::MethodNotFound | Self::ServerError(_)
        )
    }

    /// Get a human-readable description of the error code.
    pub fn description(&self) -> &'static str {
        match self {
            Self::ParseError => "Invalid JSON was received",
            Self::InvalidRequest => "The JSON sent is not a valid Request object",
            Self::MethodNotFound => "The method does not exist or is not available",
            Self::InvalidParams => "Invalid method parameter(s)",
            Self::InternalError => "Internal JSON-RPC error",
            Self::ServerError(_) => "Server error",
            Self::Unknown(_) => "Unknown error",
        }
    }
}

impl std::fmt::Display for McpErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.description(), self.code())
    }
}

impl From<McpErrorCode> for i32 {
    fn from(code: McpErrorCode) -> Self {
        code.code()
    }
}

impl From<i32> for McpErrorCode {
    fn from(code: i32) -> Self {
        Self::from_code(code)
    }
}
