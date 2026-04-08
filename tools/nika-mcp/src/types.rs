// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP Protocol Types
//!
//! Core types for MCP (Model Context Protocol) integration:
//! - [`McpConfig`]: Server configuration (name, command, args, env, cwd)
//! - [`ToolCallRequest`]: Request to invoke an MCP tool
//! - [`ToolCallResult`]: Result from a tool invocation
//! - [`ContentBlock`]: Content block in tool results (text, image, resource)
//! - [`ResourceContent`]: Resource content from MCP server
//! - [`ToolDefinition`]: Tool schema from MCP server
//! - [`McpErrorCode`]: JSON-RPC error codes

use rustc_hash::FxHashMap;

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// MCP JSON-RPC Error Codes
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
/// use nika_mcp::McpErrorCode;
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

    /// Check if this error is potentially recoverable through retry.
    ///
    /// Returns `true` for transient errors that might succeed on retry:
    /// - Internal errors (might be temporary server issues)
    /// - Server errors (implementation-defined, often transient)
    ///
    /// Returns `false` for client errors that require fixing the request:
    /// - Parse errors (invalid JSON won't become valid)
    /// - Invalid request (malformed request structure)
    /// - Invalid params (wrong parameters)
    /// - Method not found (method doesn't exist)
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::InternalError | Self::ServerError(_) | Self::Unknown(_)
        )
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

/// MCP server configuration.
///
/// Defines how to spawn and connect to an MCP server process.
///
/// # Example YAML
///
/// ```yaml
/// mcp:
///   novanet:
///     command: "npx"
///     args: ["-y", "@novanet/mcp-server"]
///     env:
///       NEO4J_URI: "bolt://localhost:7687"
///     cwd: "/path/to/project"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct McpConfig {
    /// Server name (key in mcp: block)
    #[serde(skip)]
    pub name: String,

    /// Command to execute (e.g., "npx", "node", "python")
    pub command: String,

    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables for the process
    #[serde(default)]
    pub env: FxHashMap<String, String>,

    /// Working directory for the process
    #[serde(default)]
    pub cwd: Option<String>,
}

impl McpConfig {
    /// Create a new McpConfig with the given name and command.
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            args: Vec::new(),
            env: FxHashMap::default(),
            cwd: None,
        }
    }

    /// Add an argument to the command.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments to the command.
    pub fn with_args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set the working directory.
    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Expand environment variables and tilde in all config strings.
    ///
    /// Applies shellexpand to:
    /// - `command` - the executable path
    /// - `args` - each argument
    /// - `env` values (NOT keys - keys are literal identifiers)
    /// - `cwd` - working directory
    ///
    /// # Syntax Supported
    /// - `$VAR` or `${VAR}` - environment variable
    /// - `~/path` - home directory expansion
    /// - `${VAR:-default}` - with default value (shell-compatible)
    ///
    /// # Errors
    /// Returns error if a referenced variable is not set.
    ///
    /// # Example
    /// ```rust,ignore
    /// let config = McpConfig::new("test", "$HOME/bin/server")
    ///     .with_arg("--config=$XDG_CONFIG_HOME/test.json")
    ///     .with_env("DATA_DIR", "${XDG_DATA_HOME}/test")
    ///     .expand_env_vars()?;
    /// ```
    pub fn expand_env_vars(mut self) -> Result<Self, String> {
        // Expand command
        self.command = shellexpand::full(&self.command)
            .map_err(|e| format!("Failed to expand command '{}': {}", self.command, e))?
            .into_owned();

        // Expand args
        let mut expanded_args = Vec::with_capacity(self.args.len());
        for arg in &self.args {
            let expanded = shellexpand::full(arg)
                .map_err(|e| format!("Failed to expand arg '{}': {}", arg, e))?
                .into_owned();
            expanded_args.push(expanded);
        }
        self.args = expanded_args;

        // Expand env VALUES (not keys - keys are identifiers)
        let mut expanded_env = FxHashMap::default();
        for (key, value) in self.env.drain() {
            let expanded_value = shellexpand::full(&value)
                .map_err(|e| format!("Failed to expand env '{}={}': {}", key, value, e))?
                .into_owned();
            expanded_env.insert(key, expanded_value);
        }
        self.env = expanded_env;

        // Expand cwd
        if let Some(cwd) = self.cwd.as_mut() {
            *cwd = shellexpand::full(cwd)
                .map_err(|e| format!("Failed to expand cwd '{}': {}", cwd, e))?
                .into_owned();
        }

        Ok(self)
    }
}

/// Request to call an MCP tool.
///
/// Sent to an MCP server to invoke a specific tool with arguments.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ToolCallRequest {
    /// Tool name (e.g., "novanet_context", "read_file")
    pub name: String,

    /// Tool arguments as JSON object
    #[serde(default)]
    pub arguments: serde_json::Value,
}

impl ToolCallRequest {
    /// Create a new tool call request.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            arguments: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Set the arguments from a JSON value.
    pub fn with_arguments(mut self, args: serde_json::Value) -> Self {
        self.arguments = args;
        self
    }
}

/// Result from an MCP tool call.
///
/// Contains one or more content blocks with the tool's output.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ToolCallResult {
    /// Content blocks returned by the tool
    pub content: Vec<ContentBlock>,

    /// Whether the tool call resulted in an error
    #[serde(default)]
    pub is_error: bool,

    /// Whether this result was served from cache (not from the MCP server).
    /// Per-result field eliminates the race condition of a shared AtomicBool.
    #[serde(default)]
    pub was_cached: bool,
}

impl ToolCallResult {
    /// Create a successful result with the given content blocks.
    pub fn success(content: Vec<ContentBlock>) -> Self {
        Self {
            content,
            is_error: false,
            was_cached: false,
        }
    }

    /// Create an error result with a text message.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::text(message)],
            is_error: true,
            was_cached: false,
        }
    }

    /// Extract all text content from the result.
    ///
    /// Joins all text blocks with newlines.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract the first text block, if any.
    pub fn first_text(&self) -> Option<&str> {
        self.content.iter().find_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }

    /// Check if result contains any non-text content (images, audio, resources).
    pub fn has_media(&self) -> bool {
        self.content.iter().any(|b| !b.is_text())
    }

    /// Get all image content blocks.
    pub fn images(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| b.is_image()).collect()
    }

    /// Get all audio content blocks.
    pub fn audio_blocks(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| b.is_audio()).collect()
    }

    /// Get all non-text content blocks (images, audio, resources, resource links).
    pub fn media_blocks(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| !b.is_text()).collect()
    }

    /// Estimate total content size in bytes across all content blocks.
    pub fn content_size_bytes(&self) -> usize {
        self.content
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text } => text.len(),
                ContentBlock::Image { data, .. } => data.len(),
                ContentBlock::Audio { data, .. } => data.len(),
                ContentBlock::Resource(rc) => {
                    rc.text.as_ref().map(|t| t.len()).unwrap_or(0)
                        + rc.blob.as_ref().map(|b| b.len()).unwrap_or(0)
                }
                ContentBlock::ResourceLink { .. } => 128, // Small metadata
            })
            .sum()
    }
}

/// Content block in MCP tool results.
// ARCH-5: ContentBlock and ResourceContent are now defined in nika-core::mcp
// and re-exported here for backwards compatibility.
pub use nika_core::mcp::{ContentBlock, ResourceContent};

/// Tool definition from MCP server.
///
/// Describes a tool that can be called, including its JSON Schema.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ToolDefinition {
    /// Tool name (e.g., "novanet_context")
    pub name: String,

    /// Human-readable description
    #[serde(default)]
    pub description: Option<String>,

    /// JSON Schema for the tool's input parameters
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Option<serde_json::Value>,
}

impl ToolDefinition {
    /// Create a new tool definition.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: None,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the input schema.
    pub fn with_input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = Some(schema);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_saphyr as serde_yaml;
    use serial_test::serial;

    // ═══════════════════════════════════════════════════════════════
    // McpConfig Tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_mcp_config_deserialize() {
        let yaml = r#"
            command: "npx"
            args:
              - "-y"
              - "@novanet/mcp-server"
            env:
              NEO4J_URI: "bolt://localhost:7687"
              NEO4J_USER: "neo4j"
            cwd: "/home/user/project"
        "#;

        let mut config: McpConfig = serde_yaml::from_str(yaml).unwrap();
        config.name = "novanet".to_string();

        assert_eq!(config.name, "novanet");
        assert_eq!(config.command, "npx");
        assert_eq!(config.args, vec!["-y", "@novanet/mcp-server"]);
        assert_eq!(
            config.env.get("NEO4J_URI"),
            Some(&"bolt://localhost:7687".to_string())
        );
        assert_eq!(config.env.get("NEO4J_USER"), Some(&"neo4j".to_string()));
        assert_eq!(config.cwd, Some("/home/user/project".to_string()));
    }

    #[test]
    fn test_mcp_config_deserialize_minimal() {
        let yaml = r#"
            command: "node"
        "#;

        let config: McpConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.command, "node");
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
        assert!(config.cwd.is_none());
    }

    #[test]
    fn test_mcp_config_builder() {
        let config = McpConfig::new("test", "npx")
            .with_args(["-y", "@test/server"])
            .with_env("API_KEY", "secret")
            .with_cwd("/tmp");

        assert_eq!(config.name, "test");
        assert_eq!(config.command, "npx");
        assert_eq!(config.args, vec!["-y", "@test/server"]);
        assert_eq!(config.env.get("API_KEY"), Some(&"secret".to_string()));
        assert_eq!(config.cwd, Some("/tmp".to_string()));
    }

    #[test]
    fn test_mcp_config_serialize_roundtrip() {
        let config = McpConfig::new("test", "python")
            .with_arg("server.py")
            .with_env("DEBUG", "true");

        let json = serde_json::to_string(&config).unwrap();
        let parsed: McpConfig = serde_json::from_str(&json).unwrap();

        // Note: name is skipped in serialization
        assert_eq!(config.command, parsed.command);
        assert_eq!(config.args, parsed.args);
        assert_eq!(config.env, parsed.env);
    }

    // ═══════════════════════════════════════════════════════════════
    // ToolCallRequest Tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_tool_call_request_new() {
        let request = ToolCallRequest::new("novanet_context");

        assert_eq!(request.name, "novanet_context");
        assert!(request.arguments.is_object());
        assert!(request.arguments.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_tool_call_request_with_arguments() {
        let args = serde_json::json!({
            "entity": "qr-code",
            "locale": "fr-FR"
        });

        let request = ToolCallRequest::new("novanet_context").with_arguments(args.clone());

        assert_eq!(request.name, "novanet_context");
        assert_eq!(request.arguments, args);
    }

    #[test]
    fn test_tool_call_request_deserialize() {
        let json = r#"{
            "name": "read_file",
            "arguments": {
                "path": "/tmp/test.txt"
            }
        }"#;

        let request: ToolCallRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.name, "read_file");
        assert_eq!(request.arguments["path"], "/tmp/test.txt");
    }

    // ═══════════════════════════════════════════════════════════════
    // ToolCallResult Tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_tool_result_text_extraction() {
        let result = ToolCallResult::success(vec![
            ContentBlock::text("First line"),
            ContentBlock::image("base64data", "image/png"),
            ContentBlock::text("Second line"),
        ]);

        assert_eq!(result.text(), "First line\nSecond line");
        assert_eq!(result.first_text(), Some("First line"));
        assert!(!result.is_error);
    }

    #[test]
    fn test_tool_result_text_extraction_empty() {
        let result = ToolCallResult::success(vec![ContentBlock::image("data", "image/png")]);

        assert_eq!(result.text(), "");
        assert_eq!(result.first_text(), None);
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolCallResult::error("Something went wrong");

        assert!(result.is_error);
        assert_eq!(result.text(), "Something went wrong");
    }

    #[test]
    fn test_tool_result_deserialize() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Hello, world!"}
            ],
            "is_error": false
        }"#;

        let result: ToolCallResult = serde_json::from_str(json).unwrap();

        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.first_text(), Some("Hello, world!"));
    }

    // ═══════════════════════════════════════════════════════════════
    // ContentBlock Tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_content_block_text() {
        let block = ContentBlock::text("Hello");

        assert!(block.is_text());
        assert!(!block.is_image());
        assert!(!block.is_resource());
        assert!(matches!(block, ContentBlock::Text { ref text } if text == "Hello"));
    }

    #[test]
    fn test_content_block_image() {
        let block = ContentBlock::image("SGVsbG8=", "image/png");

        assert!(block.is_image());
        assert!(!block.is_text());
        assert!(matches!(
            block,
            ContentBlock::Image { ref data, ref mime_type }
            if data == "SGVsbG8=" && mime_type == "image/png"
        ));
    }

    #[test]
    fn test_content_block_resource() {
        let resource = ResourceContent::new("file:///tmp/test.txt").with_text("File content");
        let block = ContentBlock::resource(resource);

        assert!(block.is_resource());
        assert!(!block.is_text());
        assert!(
            matches!(block, ContentBlock::Resource(ref rc) if rc.uri == "file:///tmp/test.txt")
        );
    }

    #[test]
    fn test_content_block_deserialize() {
        let json = r#"{
            "type": "text",
            "text": "Hello from MCP"
        }"#;

        let block: ContentBlock = serde_json::from_str(json).unwrap();

        assert!(block.is_text());
        assert!(matches!(block, ContentBlock::Text { ref text } if text == "Hello from MCP"));
    }

    // ═══════════════════════════════════════════════════════════════
    // ResourceContent Tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_resource_content_builder() {
        let resource = ResourceContent::new("neo4j://entity/qr-code")
            .with_mime_type("application/json")
            .with_text(r#"{"name": "QR Code"}"#);

        assert_eq!(resource.uri, "neo4j://entity/qr-code");
        assert_eq!(resource.mime_type, Some("application/json".to_string()));
        assert_eq!(resource.text, Some(r#"{"name": "QR Code"}"#.to_string()));
    }

    #[test]
    fn test_resource_content_deserialize() {
        let json = r#"{
            "uri": "file:///tmp/data.json",
            "mimeType": "application/json",
            "text": "{\"key\": \"value\"}"
        }"#;

        let resource: ResourceContent = serde_json::from_str(json).unwrap();

        assert_eq!(resource.uri, "file:///tmp/data.json");
        assert_eq!(resource.mime_type, Some("application/json".to_string()));
    }

    // ═══════════════════════════════════════════════════════════════
    // ToolDefinition Tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_tool_definition_builder() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "entity": {"type": "string"},
                "locale": {"type": "string"}
            },
            "required": ["entity"]
        });

        let tool = ToolDefinition::new("novanet_context")
            .with_description("Generate native content for an entity")
            .with_input_schema(schema.clone());

        assert_eq!(tool.name, "novanet_context");
        assert_eq!(
            tool.description,
            Some("Generate native content for an entity".to_string())
        );
        assert_eq!(tool.input_schema, Some(schema));
    }

    #[test]
    fn test_tool_definition_deserialize() {
        let json = r#"{
            "name": "read_resource",
            "description": "Read a resource from the server",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "uri": {"type": "string"}
                }
            }
        }"#;

        let tool: ToolDefinition = serde_json::from_str(json).unwrap();

        assert_eq!(tool.name, "read_resource");
        assert_eq!(
            tool.description,
            Some("Read a resource from the server".to_string())
        );
        assert!(tool.input_schema.is_some());
    }

    #[test]
    fn test_tool_definition_minimal() {
        let json = r#"{"name": "ping"}"#;

        let tool: ToolDefinition = serde_json::from_str(json).unwrap();

        assert_eq!(tool.name, "ping");
        assert!(tool.description.is_none());
        assert!(tool.input_schema.is_none());
    }

    // ═══════════════════════════════════════════════════════════════
    // McpErrorCode Tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_mcp_error_code_standard_codes() {
        assert_eq!(McpErrorCode::from_code(-32700), McpErrorCode::ParseError);
        assert_eq!(
            McpErrorCode::from_code(-32600),
            McpErrorCode::InvalidRequest
        );
        assert_eq!(
            McpErrorCode::from_code(-32601),
            McpErrorCode::MethodNotFound
        );
        assert_eq!(McpErrorCode::from_code(-32602), McpErrorCode::InvalidParams);
        assert_eq!(McpErrorCode::from_code(-32603), McpErrorCode::InternalError);
    }

    #[test]
    fn test_mcp_error_code_server_error_range() {
        let code = McpErrorCode::from_code(-32050);
        assert!(matches!(code, McpErrorCode::ServerError(-32050)));
        assert!(code.is_server_error());
        assert!(!code.is_client_error());
    }

    #[test]
    fn test_mcp_error_code_unknown() {
        let code = McpErrorCode::from_code(42);
        assert!(matches!(code, McpErrorCode::Unknown(42)));
        assert!(!code.is_server_error());
        assert!(!code.is_client_error());
    }

    #[test]
    fn test_mcp_error_code_client_errors() {
        assert!(McpErrorCode::ParseError.is_client_error());
        assert!(McpErrorCode::InvalidRequest.is_client_error());
        assert!(McpErrorCode::InvalidParams.is_client_error());
        assert!(!McpErrorCode::MethodNotFound.is_client_error());
        assert!(!McpErrorCode::InternalError.is_client_error());
    }

    #[test]
    fn test_mcp_error_code_server_errors() {
        assert!(McpErrorCode::MethodNotFound.is_server_error());
        assert!(McpErrorCode::InternalError.is_server_error());
        assert!(McpErrorCode::ServerError(-32050).is_server_error());
        assert!(!McpErrorCode::ParseError.is_server_error());
    }

    #[test]
    fn test_mcp_error_code_display() {
        let code = McpErrorCode::InvalidParams;
        let display = format!("{}", code);
        assert!(display.contains("-32602"));
        assert!(display.contains("Invalid method parameter"));
    }

    #[test]
    fn test_mcp_error_code_serde_roundtrip() {
        let original = McpErrorCode::InvalidParams;
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(json, "-32602");

        let parsed: McpErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_mcp_error_code_into_i32() {
        let code = McpErrorCode::ParseError;
        let num: i32 = code.into();
        assert_eq!(num, -32700);
    }

    // ==========================================================================
    // Tests for is_retryable()
    // ==========================================================================

    #[test]
    fn test_mcp_error_code_retryable_internal_error() {
        let code = McpErrorCode::InternalError;
        assert!(code.is_retryable());
    }

    #[test]
    fn test_mcp_error_code_retryable_server_error() {
        let code = McpErrorCode::ServerError(-32050);
        assert!(code.is_retryable());
    }

    #[test]
    fn test_mcp_error_code_retryable_unknown() {
        let code = McpErrorCode::Unknown(-999);
        assert!(code.is_retryable());
    }

    #[test]
    fn test_mcp_error_code_not_retryable_parse_error() {
        let code = McpErrorCode::ParseError;
        assert!(!code.is_retryable());
    }

    #[test]
    fn test_mcp_error_code_not_retryable_invalid_request() {
        let code = McpErrorCode::InvalidRequest;
        assert!(!code.is_retryable());
    }

    #[test]
    fn test_mcp_error_code_not_retryable_invalid_params() {
        let code = McpErrorCode::InvalidParams;
        assert!(!code.is_retryable());
    }

    #[test]
    fn test_mcp_error_code_not_retryable_method_not_found() {
        let code = McpErrorCode::MethodNotFound;
        assert!(!code.is_retryable());
    }

    // ==========================================================================
    // Tests for expand_env_vars()
    // ==========================================================================

    #[test]
    #[serial]
    fn test_expand_env_vars_command() {
        std::env::set_var("NIKA_TEST_BIN", "/usr/local/bin");
        let config = McpConfig::new("test", "$NIKA_TEST_BIN/server")
            .expand_env_vars()
            .unwrap();

        assert_eq!(config.command, "/usr/local/bin/server");
        std::env::remove_var("NIKA_TEST_BIN");
    }

    #[test]
    #[serial]
    fn test_expand_env_vars_args() {
        std::env::set_var("NIKA_TEST_CONFIG", "/etc/mcp");
        let config = McpConfig::new("test", "server")
            .with_arg("--config=$NIKA_TEST_CONFIG/config.json")
            .expand_env_vars()
            .unwrap();

        assert_eq!(config.args[0], "--config=/etc/mcp/config.json");
        std::env::remove_var("NIKA_TEST_CONFIG");
    }

    #[test]
    #[serial]
    fn test_expand_env_vars_env_values() {
        std::env::set_var("NIKA_TEST_ROOT", "/var/lib");
        let config = McpConfig::new("test", "server")
            .with_env("DATA_DIR", "$NIKA_TEST_ROOT/mcp")
            .expand_env_vars()
            .unwrap();

        assert_eq!(config.env.get("DATA_DIR").unwrap(), "/var/lib/mcp");
        std::env::remove_var("NIKA_TEST_ROOT");
    }

    #[test]
    fn test_expand_env_vars_tilde() {
        // shellexpand::full expands ~ only at the START of a string
        let config = McpConfig::new("test", "~/bin/server")
            .expand_env_vars()
            .unwrap();

        // Command should have ~ expanded (it's at the start)
        assert!(!config.command.contains('~'));
        assert!(config.command.contains("/bin/server"));
        // Check it expanded to actual home
        assert!(config.command.starts_with('/'));
    }

    #[test]
    #[serial]
    fn test_expand_env_vars_curly_brace_syntax() {
        std::env::set_var("NIKA_TEST_PATH", "/opt/mcp");
        let config = McpConfig::new("test", "${NIKA_TEST_PATH}/server")
            .expand_env_vars()
            .unwrap();

        assert_eq!(config.command, "/opt/mcp/server");
        std::env::remove_var("NIKA_TEST_PATH");
    }

    #[test]
    fn test_expand_env_vars_no_expansion_needed() {
        let config = McpConfig::new("test", "/usr/bin/server")
            .with_arg("--port=8080")
            .with_env("LOG_LEVEL", "debug")
            .expand_env_vars()
            .unwrap();

        assert_eq!(config.command, "/usr/bin/server");
        assert_eq!(config.args[0], "--port=8080");
        assert_eq!(config.env.get("LOG_LEVEL").unwrap(), "debug");
    }

    #[test]
    #[serial]
    fn test_expand_env_vars_cwd() {
        std::env::set_var("NIKA_TEST_DIR", "/home/user/projects");
        let config = McpConfig::new("test", "server")
            .with_cwd("$NIKA_TEST_DIR/mcp")
            .expand_env_vars()
            .unwrap();

        assert_eq!(config.cwd.unwrap(), "/home/user/projects/mcp");
        std::env::remove_var("NIKA_TEST_DIR");
    }

    // ═══════════════════════════════════════════════════════════════
    // ContentBlock Enum Tests (PR1)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_content_block_audio_enum() {
        let block = ContentBlock::audio("b64audio", "audio/wav");
        assert!(block.is_audio());
        assert!(!block.is_text());
        assert!(!block.is_image());
        assert!(matches!(
            block,
            ContentBlock::Audio { ref data, ref mime_type }
            if data == "b64audio" && mime_type == "audio/wav"
        ));
    }

    #[test]
    fn test_content_block_resource_link_enum() {
        let block = ContentBlock::resource_link(
            "file:///tmp/test.txt",
            Some("test.txt".to_string()),
            Some("text/plain".to_string()),
        );
        assert!(block.is_resource_link());
        assert!(!block.is_text());
        assert!(!block.is_resource());
    }

    #[test]
    fn test_content_block_serde_roundtrip_all_variants() {
        let blocks = [
            ContentBlock::text("hello"),
            ContentBlock::image("b64", "image/png"),
            ContentBlock::audio("b64", "audio/wav"),
            ContentBlock::resource(ResourceContent::new("file:///test").with_text("content")),
            ContentBlock::resource_link("file:///link", None, None),
        ];
        for (i, block) in blocks.iter().enumerate() {
            let json = serde_json::to_string(block)
                .unwrap_or_else(|e| panic!("variant {i} failed to serialize: {e}"));
            let back: ContentBlock = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("variant {i} failed to deserialize: {e}\nJSON: {json}"));
            assert_eq!(*block, back, "variant {i} roundtrip mismatch");
        }
    }

    #[test]
    fn test_debug_print_all_variants_json() {
        let blocks = vec![
            ("text", ContentBlock::text("hello")),
            ("image", ContentBlock::image("b64", "image/png")),
            ("audio", ContentBlock::audio("b64", "audio/wav")),
            (
                "resource",
                ContentBlock::resource(ResourceContent::new("file:///test").with_text("content")),
            ),
            (
                "resource_link_none",
                ContentBlock::resource_link("file:///link", None, None),
            ),
            (
                "resource_link_full",
                ContentBlock::resource_link(
                    "file:///link",
                    Some("test.txt".into()),
                    Some("text/plain".into()),
                ),
            ),
        ];
        for (label, block) in &blocks {
            let json = serde_json::to_string_pretty(block).unwrap();
            println!("=== {label} ===\n{json}\n");
        }
    }

    #[test]
    fn test_content_block_text_json_format() {
        let block = ContentBlock::text("hello world");
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "hello world");
    }

    #[test]
    fn test_content_block_image_json_has_mime_type_camel_case() {
        let block = ContentBlock::image("b64data", "image/png");
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "image");
        assert_eq!(json["mimeType"], "image/png");
        assert!(
            json.get("mime_type").is_none(),
            "should use mimeType not mime_type"
        );
    }

    #[test]
    fn test_content_block_resource_link_skips_none_fields() {
        let block = ContentBlock::resource_link("file:///link", None, None);
        let json = serde_json::to_string(&block).unwrap();
        assert!(!json.contains("name"), "None name should be skipped");
        assert!(
            !json.contains("mimeType"),
            "None mimeType should be skipped"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // ToolCallResult Media Helper Tests (PR1)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_has_media_false_for_text_only() {
        let result = ToolCallResult::success(vec![ContentBlock::text("hello")]);
        assert!(!result.has_media());
        assert!(result.images().is_empty());
        assert!(result.audio_blocks().is_empty());
        assert!(result.media_blocks().is_empty());
    }

    #[test]
    fn test_has_media_true_for_mixed_content() {
        let result = ToolCallResult::success(vec![
            ContentBlock::text("description"),
            ContentBlock::image("b64", "image/png"),
            ContentBlock::audio("b64", "audio/wav"),
        ]);
        assert!(result.has_media());
        assert_eq!(result.images().len(), 1);
        assert_eq!(result.audio_blocks().len(), 1);
        assert_eq!(result.media_blocks().len(), 2);
    }

    #[test]
    fn test_text_preserved_with_media() {
        let result = ToolCallResult::success(vec![
            ContentBlock::text("First line"),
            ContentBlock::image("b64data", "image/png"),
            ContentBlock::text("Second line"),
        ]);
        assert_eq!(result.text(), "First line\nSecond line");
        assert_eq!(result.first_text(), Some("First line"));
        assert!(result.has_media());
    }

    #[test]
    fn test_empty_content_vec() {
        let result = ToolCallResult::success(vec![]);
        assert!(!result.has_media());
        assert_eq!(result.text(), "");
        assert!(result.images().is_empty());
        assert!(result.media_blocks().is_empty());
    }

    #[test]
    fn test_with_blob_builder() {
        let rc = ResourceContent::new("file:///test")
            .with_blob("base64data")
            .with_mime_type("application/pdf");
        assert_eq!(rc.blob, Some("base64data".to_string()));
        assert_eq!(rc.mime_type, Some("application/pdf".to_string()));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Exhaustive ContentBlock Serde Tests
    //
    // These tests simulate real MCP server JSON responses to catch silent
    // serialization/deserialization bugs (field naming, missing fields,
    // variant tagging, Option skipping, etc.).
    // ═══════════════════════════════════════════════════════════════════════

    // ---------------------------------------------------------------
    // 1. Deserialization from external JSON (MCP server responses)
    // ---------------------------------------------------------------

    #[test]
    fn test_deser_text_from_mcp_json() {
        let json = r#"{"type": "text", "text": "hello"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(block.is_text());
        assert_eq!(
            block,
            ContentBlock::Text {
                text: "hello".into()
            }
        );
    }

    #[test]
    fn test_deser_image_from_mcp_json() {
        let json = r#"{"type": "image", "data": "b64data", "mimeType": "image/png"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(block.is_image());
        assert_eq!(
            block,
            ContentBlock::Image {
                data: "b64data".into(),
                mime_type: "image/png".into(),
            }
        );
    }

    #[test]
    fn test_deser_audio_from_mcp_json() {
        let json = r#"{"type": "audio", "data": "b64data", "mimeType": "audio/wav"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(block.is_audio());
        assert_eq!(
            block,
            ContentBlock::Audio {
                data: "b64data".into(),
                mime_type: "audio/wav".into(),
            }
        );
    }

    #[test]
    fn test_deser_resource_from_mcp_json() {
        let json = r#"{"type": "resource", "uri": "file:///test", "text": "content"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(block.is_resource());
        let expected =
            ContentBlock::Resource(ResourceContent::new("file:///test").with_text("content"));
        assert_eq!(block, expected);
    }

    #[test]
    fn test_deser_resource_link_from_mcp_json() {
        let json = r#"{"type": "resource_link", "uri": "file:///link"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(block.is_resource_link());
        assert_eq!(
            block,
            ContentBlock::ResourceLink {
                uri: "file:///link".into(),
                name: None,
                mime_type: None,
            }
        );
    }

    #[test]
    fn test_deser_resource_link_with_optional_fields() {
        let json = r#"{
            "type": "resource_link",
            "uri": "file:///link",
            "name": "report.pdf",
            "mimeType": "application/pdf"
        }"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert_eq!(
            block,
            ContentBlock::ResourceLink {
                uri: "file:///link".into(),
                name: Some("report.pdf".into()),
                mime_type: Some("application/pdf".into()),
            }
        );
    }

    // ---------------------------------------------------------------
    // 2. Error cases — malformed / invalid JSON
    // ---------------------------------------------------------------

    #[test]
    fn test_deser_image_missing_mime_type_fails() {
        let json = r#"{"type": "image", "data": "x"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(
            result.is_err(),
            "image without mimeType should fail to deserialize"
        );
    }

    #[test]
    fn test_deser_image_missing_data_fails() {
        let json = r#"{"type": "image", "mimeType": "image/png"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(
            result.is_err(),
            "image without data should fail to deserialize"
        );
    }

    #[test]
    fn test_deser_audio_missing_mime_type_fails() {
        let json = r#"{"type": "audio", "data": "x"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(
            result.is_err(),
            "audio without mimeType should fail to deserialize"
        );
    }

    #[test]
    fn test_deser_audio_missing_data_fails() {
        let json = r#"{"type": "audio", "mimeType": "audio/wav"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(
            result.is_err(),
            "audio without data should fail to deserialize"
        );
    }

    #[test]
    fn test_deser_text_missing_text_field_fails() {
        let json = r#"{"type": "text"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(
            result.is_err(),
            "text without text field should fail to deserialize"
        );
    }

    #[test]
    fn test_deser_resource_missing_uri_fails() {
        let json = r#"{"type": "resource", "text": "content"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(
            result.is_err(),
            "resource without uri should fail to deserialize"
        );
    }

    #[test]
    fn test_deser_resource_link_missing_uri_fails() {
        let json = r#"{"type": "resource_link", "name": "test.txt"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(
            result.is_err(),
            "resource_link without uri should fail to deserialize"
        );
    }

    #[test]
    fn test_deser_extra_unknown_fields_succeeds() {
        // serde default: unknown fields are ignored (no deny_unknown_fields)
        let json = r#"{"type": "text", "text": "hi", "extra": true, "count": 42}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert_eq!(block, ContentBlock::text("hi"));
    }

    #[test]
    fn test_deser_invalid_type_value_fails() {
        let json = r#"{"type": "video", "data": "x"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(result.is_err(), "unknown type 'video' should fail");
    }

    #[test]
    fn test_deser_empty_string_type_fails() {
        let json = r#"{"type": "", "text": "hello"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(result.is_err(), "empty string type should fail");
    }

    #[test]
    fn test_deser_missing_type_field_fails() {
        let json = r#"{"text": "hello"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(result.is_err(), "missing type field should fail");
    }

    #[test]
    fn test_deser_null_type_fails() {
        let json = r#"{"type": null, "text": "hello"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(result.is_err(), "null type should fail");
    }

    #[test]
    fn test_deser_numeric_type_fails() {
        let json = r#"{"type": 42, "text": "hello"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(result.is_err(), "numeric type should fail");
    }

    // ---------------------------------------------------------------
    // 3. Resource variant: flat JSON structure + skip_serializing_if
    // ---------------------------------------------------------------

    #[test]
    fn test_resource_serializes_flat_not_nested() {
        let block = ContentBlock::resource(
            ResourceContent::new("file:///test")
                .with_text("hello")
                .with_mime_type("text/plain"),
        );
        let json = serde_json::to_value(&block).unwrap();

        // Fields should be at top level, NOT nested under a "resource" key
        assert_eq!(json["type"], "resource");
        assert_eq!(json["uri"], "file:///test");
        assert_eq!(json["text"], "hello");
        assert_eq!(json["mimeType"], "text/plain");
        assert!(
            json.get("resource").is_none(),
            "should NOT have nested 'resource' key"
        );
    }

    #[test]
    fn test_resource_content_none_fields_omitted_in_serialization() {
        let rc = ResourceContent::new("file:///bare");
        let json = serde_json::to_value(&rc).unwrap();

        assert_eq!(json["uri"], "file:///bare");
        assert!(
            json.get("mimeType").is_none(),
            "None mimeType should be omitted"
        );
        assert!(json.get("text").is_none(), "None text should be omitted");
        assert!(json.get("blob").is_none(), "None blob should be omitted");
    }

    #[test]
    fn test_resource_content_some_fields_present_in_serialization() {
        let rc = ResourceContent::new("file:///full")
            .with_mime_type("application/json")
            .with_text("{}")
            .with_blob("YmluYXJ5");
        let json = serde_json::to_value(&rc).unwrap();

        assert_eq!(json["uri"], "file:///full");
        assert_eq!(json["mimeType"], "application/json");
        assert_eq!(json["text"], "{}");
        assert_eq!(json["blob"], "YmluYXJ5");
    }

    #[test]
    fn test_resource_block_none_fields_omitted_via_content_block() {
        // When wrapped in ContentBlock, the skip_serializing_if should still apply
        let block = ContentBlock::resource(ResourceContent::new("file:///bare"));
        let json_str = serde_json::to_string(&block).unwrap();

        assert!(
            !json_str.contains("mimeType"),
            "None mimeType should be omitted"
        );
        assert!(
            !json_str.contains("\"text\""),
            "None text should be omitted"
        );
        assert!(!json_str.contains("blob"), "None blob should be omitted");
        assert!(json_str.contains("\"type\""));
        assert!(json_str.contains("\"uri\""));
    }

    #[test]
    fn test_resource_content_roundtrip_with_all_fields() {
        let original = ResourceContent::new("file:///test")
            .with_mime_type("application/octet-stream")
            .with_text("textual fallback")
            .with_blob("YmxvYg==");

        let json = serde_json::to_string(&original).unwrap();
        let parsed: ResourceContent = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_resource_content_roundtrip_with_no_optional_fields() {
        let original = ResourceContent::new("file:///minimal");
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ResourceContent = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    // ---------------------------------------------------------------
    // 4. ToolCallResult with mixed content
    // ---------------------------------------------------------------

    #[test]
    fn test_tool_call_result_deser_mixed_content() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Analysis complete"},
                {"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"},
                {"type": "audio", "data": "UklGRg==", "mimeType": "audio/wav"},
                {"type": "text", "text": "See attached media"},
                {"type": "resource", "uri": "file:///data.json", "text": "{\"key\": 1}"},
                {"type": "resource_link", "uri": "file:///extra"}
            ],
            "is_error": false
        }"#;

        let result: ToolCallResult = serde_json::from_str(json).unwrap();

        assert!(!result.is_error);
        assert_eq!(result.content.len(), 6);

        // text() joins only text blocks
        assert_eq!(result.text(), "Analysis complete\nSee attached media");

        // has_media() should be true (image, audio, resource, resource_link)
        assert!(result.has_media());

        // media_blocks() = everything except text = 4 blocks
        assert_eq!(result.media_blocks().len(), 4);

        // images() = 1
        assert_eq!(result.images().len(), 1);
        assert!(result.images()[0].is_image());

        // audio_blocks() = 1
        assert_eq!(result.audio_blocks().len(), 1);
        assert!(result.audio_blocks()[0].is_audio());

        // first_text()
        assert_eq!(result.first_text(), Some("Analysis complete"));
    }

    #[test]
    fn test_tool_call_result_deser_error_with_mixed_content() {
        let json = r#"{
            "content": [
                {"type": "text", "text": "Partial failure"},
                {"type": "image", "data": "corrupt", "mimeType": "image/jpeg"}
            ],
            "is_error": true
        }"#;

        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        assert!(result.is_error);
        assert!(result.has_media());
        assert_eq!(result.text(), "Partial failure");
    }

    #[test]
    fn test_tool_call_result_deser_is_error_defaults_false() {
        // is_error has #[serde(default)], so omitting it should default to false
        let json = r#"{
            "content": [{"type": "text", "text": "ok"}]
        }"#;

        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        assert!(!result.is_error);
    }

    #[test]
    fn test_tool_call_result_roundtrip_mixed() {
        let original = ToolCallResult::success(vec![
            ContentBlock::text("output"),
            ContentBlock::image("aW1n", "image/webp"),
            ContentBlock::audio("YXVk", "audio/mp3"),
            ContentBlock::resource(ResourceContent::new("file:///r").with_text("resource text")),
            ContentBlock::resource_link("file:///rl", Some("name".into()), None),
        ]);

        let json = serde_json::to_string(&original).unwrap();
        let parsed: ToolCallResult = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn test_tool_call_result_empty_content_array() {
        let json = r#"{"content": [], "is_error": false}"#;
        let result: ToolCallResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.content.len(), 0);
        assert!(!result.has_media());
        assert_eq!(result.text(), "");
        assert!(result.first_text().is_none());
    }

    // ---------------------------------------------------------------
    // 5. Unicode and special characters
    // ---------------------------------------------------------------

    #[test]
    fn test_text_block_with_emoji() {
        let json = r#"{"type": "text", "text": "Hello 🌍"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert_eq!(block, ContentBlock::text("Hello 🌍"));
    }

    #[test]
    fn test_text_block_with_newlines() {
        let json = "{\"type\": \"text\", \"text\": \"line1\\nline2\"}";
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert_eq!(block, ContentBlock::text("line1\nline2"));
    }

    #[test]
    fn test_text_block_with_unicode_escapes() {
        // JSON unicode escape for e-acute: \u00e9
        let json = r#"{"type": "text", "text": "caf\u00e9"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert_eq!(block, ContentBlock::text("caf\u{00e9}"));
    }

    #[test]
    fn test_text_block_with_json_special_chars() {
        // Text containing quotes, backslashes, tabs
        let json = r#"{"type": "text", "text": "quote: \" backslash: \\ tab: \t"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert_eq!(block, ContentBlock::text("quote: \" backslash: \\ tab: \t"));
    }

    #[test]
    fn test_text_block_empty_string() {
        let json = r#"{"type": "text", "text": ""}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert_eq!(block, ContentBlock::text(""));
    }

    #[test]
    fn test_image_data_with_base64_special_chars() {
        // Base64 alphabet includes +, /, and = (padding)
        let b64 = "abc+def/ghi=";
        let json = format!(
            r#"{{"type": "image", "data": "{}", "mimeType": "image/png"}}"#,
            b64
        );
        let block: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(
            block,
            ContentBlock::Image {
                data: b64.into(),
                mime_type: "image/png".into(),
            }
        );
    }

    #[test]
    fn test_image_data_with_double_padding() {
        let b64 = "YQ==";
        let json = format!(
            r#"{{"type": "image", "data": "{}", "mimeType": "image/gif"}}"#,
            b64
        );
        let block: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(
            block,
            ContentBlock::Image {
                data: b64.into(),
                mime_type: "image/gif".into(),
            }
        );
    }

    #[test]
    fn test_resource_uri_with_special_chars() {
        let json = r#"{"type": "resource", "uri": "file:///path/to/my%20file.txt", "text": "ok"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(block.is_resource());
        if let ContentBlock::Resource(rc) = &block {
            assert_eq!(rc.uri, "file:///path/to/my%20file.txt");
        }
    }

    // ---------------------------------------------------------------
    // 6. Field naming invariants (camelCase in JSON, snake_case in Rust)
    // ---------------------------------------------------------------

    #[test]
    fn test_image_serialization_uses_camel_case_mime_type() {
        let block = ContentBlock::image("data", "image/jpeg");
        let json = serde_json::to_value(&block).unwrap();

        // Must be "mimeType" (camelCase), NOT "mime_type" (snake_case)
        assert!(
            json.get("mimeType").is_some(),
            "should serialize as mimeType"
        );
        assert!(
            json.get("mime_type").is_none(),
            "should NOT serialize as mime_type"
        );
    }

    #[test]
    fn test_audio_serialization_uses_camel_case_mime_type() {
        let block = ContentBlock::audio("data", "audio/ogg");
        let json = serde_json::to_value(&block).unwrap();

        assert!(
            json.get("mimeType").is_some(),
            "should serialize as mimeType"
        );
        assert!(
            json.get("mime_type").is_none(),
            "should NOT serialize as mime_type"
        );
    }

    #[test]
    fn test_resource_link_serialization_uses_camel_case_mime_type() {
        let block = ContentBlock::resource_link("file:///x", None, Some("text/html".into()));
        let json = serde_json::to_value(&block).unwrap();

        assert!(
            json.get("mimeType").is_some(),
            "should serialize as mimeType"
        );
        assert!(
            json.get("mime_type").is_none(),
            "should NOT serialize as mime_type"
        );
    }

    #[test]
    fn test_resource_content_serialization_uses_camel_case_mime_type() {
        let rc = ResourceContent::new("file:///x").with_mime_type("text/plain");
        let json = serde_json::to_value(&rc).unwrap();

        assert!(
            json.get("mimeType").is_some(),
            "should serialize as mimeType"
        );
        assert!(
            json.get("mime_type").is_none(),
            "should NOT serialize as mime_type"
        );
    }

    #[test]
    fn test_image_deser_rejects_snake_case_mime_type() {
        // MCP spec uses camelCase; snake_case should NOT work
        let json = r#"{"type": "image", "data": "x", "mime_type": "image/png"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(
            result.is_err(),
            "snake_case mime_type should fail — serde renames to mimeType"
        );
    }

    #[test]
    fn test_audio_deser_rejects_snake_case_mime_type() {
        let json = r#"{"type": "audio", "data": "x", "mime_type": "audio/wav"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(
            result.is_err(),
            "snake_case mime_type should fail — serde renames to mimeType"
        );
    }

    // ---------------------------------------------------------------
    // 7. Type tag value invariants (snake_case)
    // ---------------------------------------------------------------

    #[test]
    fn test_type_tag_values_are_snake_case() {
        let cases: Vec<(ContentBlock, &str)> = vec![
            (ContentBlock::text("t"), "text"),
            (ContentBlock::image("d", "image/png"), "image"),
            (ContentBlock::audio("d", "audio/wav"), "audio"),
            (
                ContentBlock::resource(ResourceContent::new("file:///x")),
                "resource",
            ),
            (
                ContentBlock::resource_link("file:///x", None, None),
                "resource_link",
            ),
        ];

        for (block, expected_tag) in cases {
            let json = serde_json::to_value(&block).unwrap();
            assert_eq!(
                json["type"].as_str().unwrap(),
                expected_tag,
                "wrong type tag for {:?}",
                block
            );
        }
    }

    #[test]
    fn test_camel_case_type_tag_fails() {
        // "resourceLink" (camelCase) should NOT work, only "resource_link" (snake_case)
        let json = r#"{"type": "resourceLink", "uri": "file:///x"}"#;
        let result = serde_json::from_str::<ContentBlock>(json);
        assert!(result.is_err(), "camelCase type tag should fail");
    }

    // ---------------------------------------------------------------
    // 8. Boundary and regression cases
    // ---------------------------------------------------------------

    #[test]
    fn test_deser_resource_with_blob_no_text() {
        let json = r#"{"type": "resource", "uri": "file:///bin", "blob": "AQID"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        if let ContentBlock::Resource(rc) = &block {
            assert_eq!(rc.uri, "file:///bin");
            assert_eq!(rc.blob, Some("AQID".into()));
            assert!(rc.text.is_none());
            assert!(rc.mime_type.is_none());
        } else {
            panic!("expected Resource variant");
        }
    }

    #[test]
    fn test_deser_resource_uri_only() {
        let json = r#"{"type": "resource", "uri": "file:///bare"}"#;
        let block: ContentBlock = serde_json::from_str(json).unwrap();
        let expected = ContentBlock::resource(ResourceContent::new("file:///bare"));
        assert_eq!(block, expected);
    }

    #[test]
    fn test_content_block_clone_eq() {
        let blocks = vec![
            ContentBlock::text("t"),
            ContentBlock::image("d", "image/png"),
            ContentBlock::audio("d", "audio/wav"),
            ContentBlock::resource(ResourceContent::new("file:///x")),
            ContentBlock::resource_link("file:///x", Some("n".into()), Some("m".into())),
        ];
        for block in &blocks {
            let cloned = block.clone();
            assert_eq!(*block, cloned);
        }
    }

    #[test]
    fn test_large_text_block_roundtrip() {
        let large_text = "a".repeat(100_000);
        let block = ContentBlock::text(&large_text);
        let json = serde_json::to_string(&block).unwrap();
        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, parsed);
    }

    #[test]
    fn test_large_base64_data_roundtrip() {
        let large_data = "A".repeat(1_000_000); // ~750KB decoded
        let block = ContentBlock::image(&large_data, "image/tiff");
        let json = serde_json::to_string(&block).unwrap();
        let parsed: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(block, parsed);
    }

    #[test]
    fn test_tool_call_result_many_blocks_roundtrip() {
        let blocks: Vec<ContentBlock> = (0..100)
            .map(|i| {
                if i % 3 == 0 {
                    ContentBlock::text(format!("block-{i}"))
                } else if i % 3 == 1 {
                    ContentBlock::image(format!("data-{i}"), "image/png")
                } else {
                    ContentBlock::audio(format!("audio-{i}"), "audio/ogg")
                }
            })
            .collect();

        let original = ToolCallResult::success(blocks);
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ToolCallResult = serde_json::from_str(&json).unwrap();
        assert_eq!(original, parsed);
        assert_eq!(parsed.content.len(), 100);
    }
}
