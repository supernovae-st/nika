//! MCP Protocol Types (v0.2)
//!
//! Core types for MCP (Model Context Protocol) integration:
//! - [`McpConfig`]: Server configuration (name, command, args, env, cwd)
//! - [`ToolCallRequest`]: Request to invoke an MCP tool
//! - [`ToolCallResult`]: Result from a tool invocation
//! - [`ContentBlock`]: Content block in tool results (text, image, resource)
//! - [`ResourceContent`]: Resource content from MCP server
//! - [`ToolDefinition`]: Tool schema from MCP server

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
    pub env: HashMap<String, String>,

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
            env: HashMap::new(),
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
}

/// Request to call an MCP tool.
///
/// Sent to an MCP server to invoke a specific tool with arguments.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ToolCallRequest {
    /// Tool name (e.g., "novanet_generate", "read_file")
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
}

impl ToolCallResult {
    /// Create a successful result with the given content blocks.
    pub fn success(content: Vec<ContentBlock>) -> Self {
        Self {
            content,
            is_error: false,
        }
    }

    /// Create an error result with a text message.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ContentBlock::text(message)],
            is_error: true,
        }
    }

    /// Extract all text content from the result.
    ///
    /// Joins all text blocks with newlines.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| block.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract the first text block, if any.
    pub fn first_text(&self) -> Option<&str> {
        self.content
            .iter()
            .find_map(|block| block.text.as_deref())
    }
}

/// Content block in MCP tool results.
///
/// Can be text, image (base64), or embedded resource.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ContentBlock {
    /// Content type: "text", "image", or "resource"
    #[serde(rename = "type")]
    pub content_type: String,

    /// Text content (for type="text")
    #[serde(default)]
    pub text: Option<String>,

    /// Base64-encoded data (for type="image")
    #[serde(default)]
    pub data: Option<String>,

    /// MIME type (for type="image", e.g., "image/png")
    #[serde(default)]
    pub mime_type: Option<String>,

    /// Embedded resource (for type="resource")
    #[serde(default)]
    pub resource: Option<ResourceContent>,
}

impl ContentBlock {
    /// Create a text content block.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content_type: "text".to_string(),
            text: Some(text.into()),
            data: None,
            mime_type: None,
            resource: None,
        }
    }

    /// Create an image content block.
    pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
        Self {
            content_type: "image".to_string(),
            text: None,
            data: Some(data.into()),
            mime_type: Some(mime_type.into()),
            resource: None,
        }
    }

    /// Create a resource content block.
    pub fn resource(resource: ResourceContent) -> Self {
        Self {
            content_type: "resource".to_string(),
            text: None,
            data: None,
            mime_type: None,
            resource: Some(resource),
        }
    }

    /// Check if this is a text block.
    pub fn is_text(&self) -> bool {
        self.content_type == "text"
    }

    /// Check if this is an image block.
    pub fn is_image(&self) -> bool {
        self.content_type == "image"
    }

    /// Check if this is a resource block.
    pub fn is_resource(&self) -> bool {
        self.content_type == "resource"
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
    #[serde(default, rename = "mimeType")]
    pub mime_type: Option<String>,

    /// Resource text content (if loaded)
    #[serde(default)]
    pub text: Option<String>,

    /// Resource binary content as base64 (if loaded)
    #[serde(default)]
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
}

/// Tool definition from MCP server.
///
/// Describes a tool that can be called, including its JSON Schema.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ToolDefinition {
    /// Tool name (e.g., "novanet_generate")
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
        assert_eq!(config.env.get("NEO4J_URI"), Some(&"bolt://localhost:7687".to_string()));
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
        let request = ToolCallRequest::new("novanet_generate");

        assert_eq!(request.name, "novanet_generate");
        assert!(request.arguments.is_object());
        assert!(request.arguments.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_tool_call_request_with_arguments() {
        let args = serde_json::json!({
            "entity": "qr-code",
            "locale": "fr-FR"
        });

        let request = ToolCallRequest::new("novanet_generate")
            .with_arguments(args.clone());

        assert_eq!(request.name, "novanet_generate");
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
        let result = ToolCallResult::success(vec![
            ContentBlock::image("data", "image/png"),
        ]);

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
        assert_eq!(block.text, Some("Hello".to_string()));
    }

    #[test]
    fn test_content_block_image() {
        let block = ContentBlock::image("SGVsbG8=", "image/png");

        assert!(block.is_image());
        assert!(!block.is_text());
        assert_eq!(block.data, Some("SGVsbG8=".to_string()));
        assert_eq!(block.mime_type, Some("image/png".to_string()));
    }

    #[test]
    fn test_content_block_resource() {
        let resource = ResourceContent::new("file:///tmp/test.txt")
            .with_text("File content");
        let block = ContentBlock::resource(resource);

        assert!(block.is_resource());
        assert!(!block.is_text());
        assert!(block.resource.is_some());
        assert_eq!(block.resource.unwrap().uri, "file:///tmp/test.txt");
    }

    #[test]
    fn test_content_block_deserialize() {
        let json = r#"{
            "type": "text",
            "text": "Hello from MCP"
        }"#;

        let block: ContentBlock = serde_json::from_str(json).unwrap();

        assert_eq!(block.content_type, "text");
        assert_eq!(block.text, Some("Hello from MCP".to_string()));
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

        let tool = ToolDefinition::new("novanet_generate")
            .with_description("Generate native content for an entity")
            .with_input_schema(schema.clone());

        assert_eq!(tool.name, "novanet_generate");
        assert_eq!(tool.description, Some("Generate native content for an entity".to_string()));
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
        assert_eq!(tool.description, Some("Read a resource from the server".to_string()));
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
}
