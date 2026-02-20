//! Rig-core provider wrapper
//!
//! Wraps rig-core providers (Anthropic, OpenAI) with a unified interface
//! that integrates with Nika's workflow system.
//!
//! ## Architecture
//!
//! This module provides two main components:
//!
//! 1. **RigProvider** - Enum wrapping Claude/OpenAI provider clients
//! 2. **NikaMcpTool** - Wrapper implementing rig-core's `ToolDyn` for MCP tools
//!
//! ## MCP Integration
//!
//! We use rig-core's `ToolDyn` trait to wrap our MCP tools, avoiding the rmcp
//! version conflict (rig-core uses rmcp 0.13, we use rmcp 0.16).
//!
//! ```text
//! NikaMcpToolDef (our definition)
//!        ↓
//! NikaMcpTool (implements ToolDyn)
//!        ↓
//! rig-core AgentBuilder.tool()
//! ```

use crate::mcp::McpClient;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::{Prompt, PromptError, ToolDefinition};
use rig::providers::{anthropic, openai};
use rig::tool::{ToolDyn, ToolError};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Provider type enum for rig-core providers
#[derive(Debug, Clone)]
pub enum RigProvider {
    /// Claude (Anthropic) provider
    Claude(anthropic::Client),
    /// OpenAI provider
    OpenAI(openai::Client),
}

impl RigProvider {
    /// Create a Claude provider from environment variable ANTHROPIC_API_KEY
    pub fn claude() -> Self {
        let client = anthropic::Client::from_env();
        RigProvider::Claude(client)
    }

    /// Create an OpenAI provider from environment variable OPENAI_API_KEY
    pub fn openai() -> Self {
        let client = openai::Client::from_env();
        RigProvider::OpenAI(client)
    }

    /// Get the provider name
    pub fn name(&self) -> &'static str {
        match self {
            RigProvider::Claude(_) => "claude",
            RigProvider::OpenAI(_) => "openai",
        }
    }

    /// Get the default model for this provider
    ///
    /// Claude: claude-sonnet-4-20250514 (latest stable as of 2025-05)
    /// OpenAI: gpt-4o (latest stable)
    pub fn default_model(&self) -> &'static str {
        match self {
            // Note: rig-core's CLAUDE_3_5_SONNET constant is outdated
            // Using explicit model name for stability
            RigProvider::Claude(_) => "claude-sonnet-4-20250514",
            RigProvider::OpenAI(_) => openai::GPT_4O,
        }
    }

    /// Simple text completion (infer) using rig-core
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send
    /// * `model` - Model identifier (uses default if None)
    ///
    /// # Returns
    /// The completion text from the model
    pub async fn infer(&self, prompt: &str, model: Option<&str>) -> Result<String, RigInferError> {
        let model_id = model.unwrap_or_else(|| self.default_model());

        match self {
            RigProvider::Claude(client) => {
                let agent = client.agent(model_id).build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
            RigProvider::OpenAI(client) => {
                let agent = client.agent(model_id).build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e: PromptError| RigInferError::PromptError(e.to_string()))
            }
        }
    }
}

/// Error type for RigProvider infer operations
#[derive(Debug, thiserror::Error)]
pub enum RigInferError {
    #[error("Completion error: {0}")]
    PromptError(String),
}

// =============================================================================
// NikaMcpTool - Wrapper for MCP tools implementing rig-core's ToolDyn
// =============================================================================

/// Tool definition for Nika MCP tools.
///
/// This is our own definition struct that avoids the rmcp version conflict.
/// We convert MCP tool definitions from rmcp 0.16 into this format.
#[derive(Debug, Clone)]
pub struct NikaMcpToolDef {
    /// Tool name (e.g., "novanet_generate")
    pub name: String,
    /// Tool description for the LLM
    pub description: String,
    /// JSON Schema for input parameters
    pub input_schema: serde_json::Value,
}

/// MCP tool wrapper implementing rig-core's `ToolDyn` trait.
///
/// This allows us to use our MCP tools (rmcp 0.16) with rig-core's
/// agent system without version conflicts.
#[derive(Debug, Clone)]
pub struct NikaMcpTool {
    definition: NikaMcpToolDef,
    /// Optional MCP client for real tool calls
    client: Option<Arc<McpClient>>,
}

impl NikaMcpTool {
    /// Create a new NikaMcpTool from a definition (without client)
    pub fn new(definition: NikaMcpToolDef) -> Self {
        Self {
            definition,
            client: None,
        }
    }

    /// Create a new NikaMcpTool with an MCP client for real tool calls
    pub fn with_client(definition: NikaMcpToolDef, client: Arc<McpClient>) -> Self {
        Self {
            definition,
            client: Some(client),
        }
    }

    /// Get the tool name
    pub fn tool_name(&self) -> &str {
        &self.definition.name
    }
}

/// Type alias for boxed future (required by ToolDyn)
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

impl ToolDyn for NikaMcpTool {
    fn name(&self) -> String {
        self.definition.name.clone()
    }

    fn definition(&self, _prompt: String) -> BoxFuture<'_, ToolDefinition> {
        let def = ToolDefinition {
            name: self.definition.name.clone(),
            description: self.definition.description.clone(),
            parameters: self.definition.input_schema.clone(),
        };
        Box::pin(async move { def })
    }

    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>> {
        let tool_name = self.definition.name.clone();
        let client = self.client.clone();

        Box::pin(async move {
            // Parse the args as JSON
            let params: serde_json::Value = serde_json::from_str(&args).map_err(|e| {
                ToolError::ToolCallError(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid JSON arguments: {}", e),
                )))
            })?;

            // Check if we have a client
            let client = client.ok_or_else(|| {
                ToolError::ToolCallError(Box::new(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "No MCP client configured for this tool",
                )))
            })?;

            // Call the MCP tool
            let result = client.call_tool(&tool_name, params).await.map_err(|e| {
                ToolError::ToolCallError(Box::new(std::io::Error::other(format!(
                    "MCP tool call failed: {}",
                    e
                ))))
            })?;

            // Extract text content from the result
            let output = result.text();

            if output.is_empty() {
                // Return the full result as JSON if no text content
                serde_json::to_string(&result).map_err(|e| {
                    ToolError::ToolCallError(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to serialize result: {}", e),
                    )))
                })
            } else {
                Ok(output)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rig_provider_claude_returns_claude_variant() {
        // This test verifies that RigProvider::claude() creates a Claude variant
        // It will fail initially because we need ANTHROPIC_API_KEY env var
        // In real code, we'll use from_env() which reads the API key

        // For now, we test the name() method which doesn't require API call
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::claude();

        assert_eq!(provider.name(), "claude");
        assert!(matches!(provider, RigProvider::Claude(_)));
    }

    #[test]
    fn test_rig_provider_openai_returns_openai_variant() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::openai();

        assert_eq!(provider.name(), "openai");
        assert!(matches!(provider, RigProvider::OpenAI(_)));
    }

    #[test]
    fn test_rig_provider_default_model_claude() {
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::claude();

        // Using explicit model name instead of rig-core constant
        // rig-core's CLAUDE_3_5_SONNET is outdated
        assert_eq!(provider.default_model(), "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_rig_provider_default_model_openai() {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");
        let provider = RigProvider::openai();

        assert_eq!(provider.default_model(), openai::GPT_4O);
    }

    #[test]
    fn test_rig_infer_error_display() {
        let err = RigInferError::PromptError("Test error message".to_string());
        assert_eq!(err.to_string(), "Completion error: Test error message");
    }

    // =========================================================================
    // NikaMcpTool tests
    // =========================================================================

    #[test]
    fn test_nika_mcp_tool_implements_tool_dyn() {
        // Given: A tool definition from our MCP infrastructure
        let tool_def = NikaMcpToolDef {
            name: "novanet_generate".to_string(),
            description: "Generate native content for an entity".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity": { "type": "string" },
                    "locale": { "type": "string" }
                },
                "required": ["entity", "locale"]
            }),
        };

        // When: We create a NikaMcpTool wrapper
        let tool = NikaMcpTool::new(tool_def);

        // Then: It should have the correct name
        assert_eq!(tool.tool_name(), "novanet_generate");
    }

    #[test]
    fn test_nika_mcp_tool_definition_returns_correct_schema() {
        use rig::tool::ToolDyn;

        // Given: A NikaMcpTool with a specific schema
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Describe an entity from the knowledge graph".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity_key": { "type": "string" }
                },
                "required": ["entity_key"]
            }),
        };
        let tool = NikaMcpTool::new(tool_def);

        // When: We get the tool definition (sync wrapper for test)
        let name = tool.name();

        // Then: The definition should match
        assert_eq!(name, "novanet_describe");
    }

    // =========================================================================
    // RED: NikaMcpTool with McpClient - should FAIL until we wire up McpClient
    // =========================================================================

    #[tokio::test]
    async fn test_nika_mcp_tool_call_uses_mcp_client() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        // Given: A mock MCP client (pre-connected)
        let client = Arc::new(McpClient::mock("novanet"));

        // Given: A NikaMcpTool connected to the client
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Describe an entity".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "entity_key": { "type": "string" }
                },
                "required": ["entity_key"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: We call the tool
        let args = r#"{"entity_key": "qr-code"}"#.to_string();
        let result = tool.call(args).await;

        // Then: The call should succeed (mock returns success)
        assert!(result.is_ok(), "Tool call should succeed with mock client");
        let output = result.unwrap();
        assert!(!output.is_empty(), "Tool should return non-empty output");
    }

    // =========================================================================
    // USE CASE TESTS - Real-world NovaNet MCP tool scenarios
    // =========================================================================

    /// UC1: novanet_generate - Generate native content for an entity
    #[tokio::test]
    async fn test_usecase_novanet_generate_entity_locale() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        // Given: Mock NovaNet MCP client
        let client = Arc::new(McpClient::mock("novanet"));

        // Given: novanet_generate tool with full schema (matching NovaNet MCP spec)
        let tool_def = NikaMcpToolDef {
            name: "novanet_generate".to_string(),
            description: "Full RLM-on-KG context assembly for generation".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "focus_key": { "type": "string", "description": "Entity key to generate for" },
                    "locale": { "type": "string", "description": "BCP-47 locale code" },
                    "mode": { "type": "string", "enum": ["block", "page"], "default": "block" },
                    "token_budget": { "type": "integer", "default": 4000 },
                    "spreading_depth": { "type": "integer", "default": 2 },
                    "forms": {
                        "type": "array",
                        "items": { "type": "string", "enum": ["text", "title", "abbrev", "url"] }
                    }
                },
                "required": ["focus_key", "locale"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Calling for QR code entity in French
        let args = serde_json::json!({
            "focus_key": "qr-code",
            "locale": "fr-FR",
            "mode": "page",
            "forms": ["text", "title", "abbrev"]
        })
        .to_string();

        let result = tool.call(args).await;

        // Then: Should succeed with mock response
        assert!(
            result.is_ok(),
            "novanet_generate should succeed: {:?}",
            result
        );
        let output = result.unwrap();
        assert!(!output.is_empty(), "Should return generation context");
    }

    /// UC2: novanet_describe - Get entity details
    #[tokio::test]
    async fn test_usecase_novanet_describe_entity() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Bootstrap agent understanding of the knowledge graph".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "describe": {
                        "type": "string",
                        "enum": ["schema", "entity", "category", "relations", "locales", "stats"]
                    },
                    "entity_key": { "type": "string" },
                    "category_key": { "type": "string" }
                },
                "required": ["describe"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Describing schema overview
        let args = serde_json::json!({
            "describe": "schema"
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_describe should succeed");
    }

    /// UC3: novanet_traverse - Graph traversal
    #[tokio::test]
    async fn test_usecase_novanet_traverse_graph() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_traverse".to_string(),
            description: "Graph traversal with configurable depth and filters".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "start_key": { "type": "string" },
                    "max_depth": { "type": "integer", "default": 2 },
                    "direction": { "type": "string", "enum": ["outgoing", "incoming", "both"] },
                    "arc_families": { "type": "array", "items": { "type": "string" } },
                    "target_kinds": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["start_key"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Traversing from QR code with HAS_NATIVE arc
        let args = serde_json::json!({
            "start_key": "qr-code",
            "max_depth": 2,
            "direction": "outgoing",
            "arc_families": ["ownership", "localization"]
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_traverse should succeed");
    }

    /// UC4: novanet_search - Hybrid search
    #[tokio::test]
    async fn test_usecase_novanet_search_hybrid() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_search".to_string(),
            description: "Fulltext + property search with hybrid mode".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "mode": { "type": "string", "enum": ["fulltext", "property", "hybrid"] },
                    "kinds": { "type": "array", "items": { "type": "string" } },
                    "realm": { "type": "string", "enum": ["shared", "org"] },
                    "limit": { "type": "integer", "default": 10 }
                },
                "required": ["query"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Searching for QR-related entities
        let args = serde_json::json!({
            "query": "QR code generator",
            "mode": "hybrid",
            "kinds": ["Entity", "Page"],
            "limit": 5
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_search should succeed");
    }

    /// UC5: novanet_atoms - Knowledge atoms retrieval
    #[tokio::test]
    async fn test_usecase_novanet_atoms_locale() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_atoms".to_string(),
            description: "Retrieve knowledge atoms for a specific locale".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "locale": { "type": "string" },
                    "atom_type": {
                        "type": "string",
                        "enum": ["term", "expression", "pattern", "cultureref", "taboo", "audiencetrait", "all"]
                    },
                    "domain": { "type": "string" }
                },
                "required": ["locale"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Getting French terms for QR codes
        let args = serde_json::json!({
            "locale": "fr-FR",
            "atom_type": "term",
            "domain": "qr-code"
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_atoms should succeed");
    }

    /// UC6: novanet_assemble - Context assembly
    #[tokio::test]
    async fn test_usecase_novanet_assemble_context() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));

        let tool_def = NikaMcpToolDef {
            name: "novanet_assemble".to_string(),
            description: "Assemble context for LLM generation (token-aware)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "focus_key": { "type": "string" },
                    "locale": { "type": "string" },
                    "token_budget": { "type": "integer", "default": 4000 },
                    "strategy": {
                        "type": "string",
                        "enum": ["breadth", "depth", "relevance", "custom"]
                    }
                },
                "required": ["focus_key", "locale"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Assembling context for Spanish QR code generation
        let args = serde_json::json!({
            "focus_key": "qr-code",
            "locale": "es-MX",
            "token_budget": 3000,
            "strategy": "relevance"
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok(), "novanet_assemble should succeed");
    }

    // =========================================================================
    // ERROR HANDLING TESTS
    // =========================================================================

    /// Test that calling without client returns proper error
    #[tokio::test]
    async fn test_error_no_client_configured() {
        use rig::tool::ToolDyn;

        // Given: NikaMcpTool WITHOUT client
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let tool = NikaMcpTool::new(tool_def); // No client!

        // When: Calling the tool
        let args = r#"{"entity_key": "test"}"#.to_string();
        let result = tool.call(args).await;

        // Then: Should fail with NotConnected error
        assert!(result.is_err(), "Should fail without client");
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("No MCP client") || err_str.contains("NotConnected"),
            "Error should mention missing client: {}",
            err_str
        );
    }

    /// Test that invalid JSON arguments return proper error
    #[tokio::test]
    async fn test_error_invalid_json_arguments() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Calling with invalid JSON
        let args = "not valid json {{{".to_string();
        let result = tool.call(args).await;

        // Then: Should fail with JSON parsing error
        assert!(result.is_err(), "Should fail with invalid JSON");
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("Invalid JSON") || err_str.contains("JSON"),
            "Error should mention JSON parsing: {}",
            err_str
        );
    }

    /// Test that empty JSON object is valid
    #[tokio::test]
    async fn test_empty_json_object_is_valid() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Calling with empty JSON object
        let args = "{}".to_string();
        let result = tool.call(args).await;

        // Then: Should succeed (empty args are valid)
        assert!(result.is_ok(), "Empty JSON object should be valid");
    }

    // =========================================================================
    // TOOL DEFINITION TESTS
    // =========================================================================

    /// Test async definition method returns correct schema
    #[tokio::test]
    async fn test_tool_definition_async() {
        use rig::tool::ToolDyn;

        let input_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "entity_key": { "type": "string" },
                "locale": { "type": "string" }
            },
            "required": ["entity_key"]
        });

        let tool_def = NikaMcpToolDef {
            name: "test_tool".to_string(),
            description: "A test tool for verification".to_string(),
            input_schema: input_schema.clone(),
        };
        let tool = NikaMcpTool::new(tool_def);

        // When: Getting the tool definition
        let definition = tool.definition("some prompt".to_string()).await;

        // Then: Definition should match
        assert_eq!(definition.name, "test_tool");
        assert_eq!(definition.description, "A test tool for verification");
        assert_eq!(definition.parameters, input_schema);
    }

    /// Test multiple tools can coexist
    #[test]
    fn test_multiple_tools_independent() {
        // Given: Multiple tool definitions
        let tool1 = NikaMcpTool::new(NikaMcpToolDef {
            name: "novanet_generate".to_string(),
            description: "Generate content".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        let tool2 = NikaMcpTool::new(NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Describe entity".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        let tool3 = NikaMcpTool::new(NikaMcpToolDef {
            name: "novanet_traverse".to_string(),
            description: "Traverse graph".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        });

        // Then: Each tool maintains its own identity
        assert_eq!(tool1.tool_name(), "novanet_generate");
        assert_eq!(tool2.tool_name(), "novanet_describe");
        assert_eq!(tool3.tool_name(), "novanet_traverse");
    }

    /// Test tool can be cloned and remains functional
    #[tokio::test]
    async fn test_tool_clone_works() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));
        let tool_def = NikaMcpToolDef {
            name: "novanet_describe".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Cloning the tool
        let cloned_tool = tool.clone();

        // Then: Both should work independently
        let args = r#"{"entity_key": "test"}"#.to_string();
        let result1 = tool.call(args.clone()).await;
        let result2 = cloned_tool.call(args).await;

        assert!(result1.is_ok(), "Original tool should work");
        assert!(result2.is_ok(), "Cloned tool should work");
    }

    // =========================================================================
    // MULTI-LOCALE TESTS (Real-world scenarios)
    // =========================================================================

    /// Test generating for multiple locales (common Nika workflow pattern)
    #[tokio::test]
    async fn test_multi_locale_generation_workflow() {
        use crate::mcp::McpClient;
        use rig::tool::ToolDyn;
        use std::sync::Arc;

        let client = Arc::new(McpClient::mock("novanet"));
        let tool_def = NikaMcpToolDef {
            name: "novanet_generate".to_string(),
            description: "Generate native content".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "focus_key": { "type": "string" },
                    "locale": { "type": "string" },
                    "forms": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["focus_key", "locale"]
            }),
        };
        let tool = NikaMcpTool::with_client(tool_def, client);

        // When: Generating for multiple locales (simulating for_each workflow)
        let locales = ["fr-FR", "es-MX", "de-DE", "ja-JP", "zh-CN"];
        let mut results = Vec::new();

        for locale in locales {
            let args = serde_json::json!({
                "focus_key": "qr-code",
                "locale": locale,
                "forms": ["text", "title"]
            })
            .to_string();

            let result = tool.call(args).await;
            results.push((locale, result.is_ok()));
        }

        // Then: All locales should succeed
        for (locale, success) in &results {
            assert!(success, "Generation for {} should succeed", locale);
        }
        assert_eq!(results.len(), 5, "Should process all 5 locales");
    }
}
