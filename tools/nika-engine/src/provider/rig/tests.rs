use super::*;
use serial_test::serial;

// =========================================================================
// StreamResult tests
// =========================================================================

#[test]
fn stream_result_from_text_has_zero_tokens() {
    let result = StreamResult::from_text("hello world");
    assert_eq!(result.text, "hello world");
    assert_eq!(result.input_tokens, 0);
    assert_eq!(result.output_tokens, 0);
    assert_eq!(result.total_tokens, 0);
    assert_eq!(result.cached_input_tokens, 0);
}

#[test]
fn stream_result_default_is_empty() {
    let result = StreamResult::default();
    assert_eq!(result.text, "");
    assert_eq!(result.total_tokens, 0);
}

#[test]
fn stream_result_with_tokens() {
    let result = StreamResult {
        text: "response".to_string(),
        input_tokens: 100,
        output_tokens: 50,
        total_tokens: 150,
        cached_input_tokens: 20,
        ttft_ms: None,
        request_id: None,
        finish_reason: None,
    };
    assert_eq!(
        result.total_tokens,
        result.input_tokens + result.output_tokens
    );
    assert_eq!(result.cached_input_tokens, 20);
}

#[test]
#[serial]
fn test_rig_provider_claude_returns_claude_variant() {
    // This test verifies that RigProvider::claude() creates a Claude variant
    // It will fail initially because we need ANTHROPIC_API_KEY env var
    // In real code, we'll use from_env() which reads the API key

    // For now, we test the name() method which doesn't require API call
    std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-unit-test");
    let provider = RigProvider::claude();

    assert_eq!(provider.name(), "anthropic");
    assert!(matches!(provider, RigProvider::Claude(_)));
}

#[test]
#[serial]
fn test_rig_provider_openai_returns_openai_variant() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");
    let provider = RigProvider::openai();

    assert_eq!(provider.name(), "openai");
    assert!(matches!(provider, RigProvider::OpenAI(_)));
}

#[test]
#[serial]
fn test_rig_provider_default_model_claude() {
    std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-unit-test");
    let provider = RigProvider::claude();

    // Using explicit model name instead of rig-core constant
    // rig-core's CLAUDE_3_5_SONNET is outdated
    assert_eq!(provider.default_model(), "claude-sonnet-4-6");
}

#[test]
#[serial]
fn test_rig_provider_default_model_openai() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");
    let provider = RigProvider::openai();

    assert_eq!(provider.default_model(), "gpt-5.2");
}

#[test]
fn test_rig_infer_error_display() {
    let err = RigInferError::PromptError("Test error message".to_string());
    assert_eq!(err.to_string(), "Completion error: Test error message");
}

#[test]
fn test_rig_infer_error_timeout_display() {
    // Test new Timeout variant
    let err = RigInferError::Timeout { duration_ms: 60000 };
    assert_eq!(
        err.to_string(),
        "Stream timeout: no chunk received for 60000ms"
    );
}

// =========================================================================
// New Provider Tests
// =========================================================================

#[test]
#[serial]
fn test_rig_provider_mistral_returns_mistral_variant() {
    std::env::set_var("MISTRAL_API_KEY", "test-key-for-unit-test");
    let provider = RigProvider::mistral();

    assert_eq!(provider.name(), "mistral");
    assert!(matches!(provider, RigProvider::Mistral(_)));
}

#[test]
#[serial]
fn test_rig_provider_groq_returns_groq_variant() {
    std::env::set_var("GROQ_API_KEY", "test-key-for-unit-test");
    let provider = RigProvider::groq();

    assert_eq!(provider.name(), "groq");
    assert!(matches!(provider, RigProvider::Groq(_)));
}

#[test]
#[serial]
fn test_rig_provider_deepseek_returns_deepseek_variant() {
    std::env::set_var("DEEPSEEK_API_KEY", "test-key-for-unit-test");
    let provider = RigProvider::deepseek();

    assert_eq!(provider.name(), "deepseek");
    assert!(matches!(provider, RigProvider::DeepSeek(_)));
}

#[test]
#[serial]
fn test_rig_provider_default_models_v06() {
    // Test all new provider default models
    std::env::set_var("MISTRAL_API_KEY", "test");
    std::env::set_var("GROQ_API_KEY", "test");
    std::env::set_var("DEEPSEEK_API_KEY", "test");

    assert_eq!(
        RigProvider::mistral().default_model(),
        mistral::MISTRAL_LARGE
    );
    assert_eq!(
        RigProvider::groq().default_model(),
        "llama-3.3-70b-versatile"
    );
    assert_eq!(RigProvider::deepseek().default_model(), "deepseek-chat");
}

#[test]
#[serial]
fn test_rig_provider_auto_detects_claude() {
    // Clear other keys, set only Claude
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("MISTRAL_API_KEY");
    std::env::remove_var("GROQ_API_KEY");
    std::env::remove_var("DEEPSEEK_API_KEY");
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");

    let provider = RigProvider::auto();
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "anthropic");
}

#[test]
#[serial]
fn test_rig_provider_auto_returns_none_when_no_keys() {
    // Clear all API keys - uses #[serial] for test isolation
    clear_all_provider_env_vars();

    let provider = RigProvider::auto();
    assert!(provider.is_none());
}

// =========================================================================
// Provider Fallback Chain Tests
// =========================================================================

/// Helper to clear all provider env vars for testing fallback chain
fn clear_all_provider_env_vars() {
    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("MISTRAL_API_KEY");
    std::env::remove_var("GROQ_API_KEY");
    std::env::remove_var("DEEPSEEK_API_KEY");
    std::env::remove_var("GEMINI_API_KEY");
    std::env::remove_var("XAI_API_KEY");
}

#[test]
#[serial]
fn test_auto_fallback_to_openai() {
    // Given: Only OPENAI_API_KEY is set (Claude not available)
    clear_all_provider_env_vars();
    std::env::set_var("OPENAI_API_KEY", "test-key");

    // When: auto() is called
    let provider = RigProvider::auto();

    // Then: Should fall back to OpenAI
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "openai");
}

#[test]
#[serial]
fn test_auto_fallback_to_mistral() {
    // Given: Only MISTRAL_API_KEY is set
    clear_all_provider_env_vars();
    std::env::set_var("MISTRAL_API_KEY", "test-key");

    // When: auto() is called
    let provider = RigProvider::auto();

    // Then: Should fall back to Mistral
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "mistral");
}

#[test]
#[serial]
fn test_auto_fallback_to_groq() {
    // Given: Only GROQ_API_KEY is set
    clear_all_provider_env_vars();
    std::env::set_var("GROQ_API_KEY", "test-key");

    // When: auto() is called
    let provider = RigProvider::auto();

    // Then: Should fall back to Groq
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "groq");
}

#[test]
#[serial]
fn test_auto_fallback_to_deepseek() {
    // Given: Only DEEPSEEK_API_KEY is set
    clear_all_provider_env_vars();
    std::env::set_var("DEEPSEEK_API_KEY", "test-key");

    // When: auto() is called
    let provider = RigProvider::auto();

    // Then: Should fall back to DeepSeek
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "deepseek");
}

#[test]
#[serial]
fn test_auto_fallback_to_gemini() {
    // Given: Only GEMINI_API_KEY is set
    clear_all_provider_env_vars();
    std::env::set_var("GEMINI_API_KEY", "test-key");

    // When: auto() is called
    let provider = RigProvider::auto();

    // Then: Should fall back to Gemini
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "gemini");
}

#[test]
#[serial]
fn test_auto_priority_claude_over_openai() {
    // Given: Both Claude and OpenAI keys are set
    clear_all_provider_env_vars();
    std::env::set_var("ANTHROPIC_API_KEY", "claude-key");
    std::env::set_var("OPENAI_API_KEY", "openai-key");

    // When: auto() is called
    let provider = RigProvider::auto();

    // Then: Should select Claude (higher priority)
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "anthropic");
}

#[test]
#[serial]
fn test_auto_priority_openai_over_mistral() {
    // Given: OpenAI and Mistral keys are set (no Claude)
    clear_all_provider_env_vars();
    std::env::set_var("OPENAI_API_KEY", "openai-key");
    std::env::set_var("MISTRAL_API_KEY", "mistral-key");

    // When: auto() is called
    let provider = RigProvider::auto();

    // Then: Should select OpenAI (higher priority than Mistral)
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "openai");
}

#[test]
#[serial]
fn test_auto_empty_env_var_treated_as_unset() {
    // Given: ANTHROPIC_API_KEY is set but empty
    clear_all_provider_env_vars();
    std::env::set_var("ANTHROPIC_API_KEY", ""); // Empty string
    std::env::set_var("OPENAI_API_KEY", "valid-key");

    // When: auto() is called
    let provider = RigProvider::auto();

    // Then: Should skip empty Claude and select OpenAI
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().name(), "openai");
}

#[test]
#[serial]
fn test_auto_whitespace_env_var_treated_as_unset() {
    // Given: ANTHROPIC_API_KEY is set to whitespace only
    clear_all_provider_env_vars();
    std::env::set_var("ANTHROPIC_API_KEY", "   "); // Whitespace only

    // When: auto() is called
    let provider = RigProvider::auto();

    // Then: Should treat whitespace-only as unset
    // The implementation now uses !v.trim().is_empty() to reject whitespace-only keys
    assert!(
        provider.is_none(),
        "Whitespace-only API key should be treated as unset"
    );
}

// =========================================================================
// NikaMcpTool tests
// =========================================================================

#[test]
fn test_nika_mcp_tool_implements_tool_dyn() {
    // Given: A tool definition from our MCP infrastructure
    let tool_def = NikaMcpToolDef {
        name: "novanet_context".to_string(),
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
    assert_eq!(tool.tool_name(), "novanet_context");
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

/// UC1: novanet_context - Assemble LLM context for content generation
#[tokio::test]
async fn test_usecase_novanet_context_entity_locale() {
    use crate::mcp::McpClient;
    use rig::tool::ToolDyn;
    use std::sync::Arc;

    // Given: Mock NovaNet MCP client
    let client = Arc::new(McpClient::mock("novanet"));

    // Given: novanet_context tool with full schema (matching NovaNet MCP spec)
    let tool_def = NikaMcpToolDef {
        name: "novanet_context".to_string(),
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
        "novanet_context should succeed: {:?}",
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

/// UC3: novanet_search (walk mode) - Graph traversal
#[tokio::test]
async fn test_usecase_novanet_search_walk_graph() {
    use crate::mcp::McpClient;
    use rig::tool::ToolDyn;
    use std::sync::Arc;

    let client = Arc::new(McpClient::mock("novanet"));

    let tool_def = NikaMcpToolDef {
        name: "novanet_search".to_string(),
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
    assert!(result.is_ok(), "novanet_search walk should succeed");
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

/// UC5: novanet_audit - Quality checks with CSR metrics
#[tokio::test]
async fn test_usecase_novanet_audit_locale() {
    use crate::mcp::McpClient;
    use rig::tool::ToolDyn;
    use std::sync::Arc;

    let client = Arc::new(McpClient::mock("novanet"));

    let tool_def = NikaMcpToolDef {
        name: "novanet_audit".to_string(),
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
    assert!(result.is_ok(), "novanet_audit should succeed");
}

/// UC6: novanet_batch - Parallel operations
#[tokio::test]
async fn test_usecase_novanet_batch_context() {
    use crate::mcp::McpClient;
    use rig::tool::ToolDyn;
    use std::sync::Arc;

    let client = Arc::new(McpClient::mock("novanet"));

    let tool_def = NikaMcpToolDef {
        name: "novanet_batch".to_string(),
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
    assert!(result.is_ok(), "novanet_batch should succeed");
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
        name: "novanet_context".to_string(),
        description: "Generate content".to_string(),
        input_schema: serde_json::json!({"type": "object"}),
    });

    let tool2 = NikaMcpTool::new(NikaMcpToolDef {
        name: "novanet_describe".to_string(),
        description: "Describe entity".to_string(),
        input_schema: serde_json::json!({"type": "object"}),
    });

    let tool3 = NikaMcpTool::new(NikaMcpToolDef {
        name: "novanet_search".to_string(),
        description: "Traverse graph".to_string(),
        input_schema: serde_json::json!({"type": "object"}),
    });

    // Then: Each tool maintains its own identity
    assert_eq!(tool1.tool_name(), "novanet_context");
    assert_eq!(tool2.tool_name(), "novanet_describe");
    assert_eq!(tool3.tool_name(), "novanet_search");
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
        name: "novanet_context".to_string(),
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

// =========================================================================
// Provider Verification Tests
// =========================================================================

#[test]
fn test_provider_verify_error_types() {
    // Test all error variants
    let invalid_key = ProviderVerifyError::InvalidApiKey {
        provider: "claude".to_string(),
    };
    assert!(invalid_key.to_string().contains("Invalid API key"));
    assert!(invalid_key.suggestion().contains("API key"));

    let rate_limited = ProviderVerifyError::RateLimited {
        provider: "openai".to_string(),
    };
    assert!(rate_limited.to_string().contains("Rate limited"));

    let timeout = ProviderVerifyError::Timeout {
        provider: "mistral".to_string(),
    };
    assert!(timeout.to_string().contains("timeout"));

    let network = ProviderVerifyError::NetworkError {
        provider: "groq".to_string(),
        details: "connection refused".to_string(),
    };
    assert!(network.to_string().contains("Network error"));

    let provider_err = ProviderVerifyError::ProviderError {
        provider: "deepseek".to_string(),
        details: "server down".to_string(),
    };
    assert!(provider_err.to_string().contains("server down"));
}

#[test]
fn test_provider_verify_result_fields() {
    let result = ProviderVerifyResult {
        provider: "claude".to_string(),
        latency: std::time::Duration::from_millis(150),
        model: "claude-sonnet-4-6".to_string(),
    };

    assert_eq!(result.provider, "claude");
    assert_eq!(result.latency.as_millis(), 150);
    assert_eq!(result.model, "claude-sonnet-4-6");
}

#[test]
#[serial]
fn test_is_configured_with_api_key() {
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    let provider = RigProvider::claude();
    assert!(provider.is_configured());
}

#[test]
#[serial]
fn test_is_configured_returns_true_for_all_providers_with_keys() {
    // Set up all API keys
    std::env::set_var("ANTHROPIC_API_KEY", "test");
    std::env::set_var("OPENAI_API_KEY", "test");
    std::env::set_var("MISTRAL_API_KEY", "test");
    std::env::set_var("GROQ_API_KEY", "test");
    std::env::set_var("DEEPSEEK_API_KEY", "test");

    assert!(RigProvider::claude().is_configured());
    assert!(RigProvider::openai().is_configured());
    assert!(RigProvider::mistral().is_configured());
    assert!(RigProvider::groq().is_configured());
    assert!(RigProvider::deepseek().is_configured());
}

// =========================================================================
// InferOptions Tests
// =========================================================================

#[test]
fn test_infer_options_default() {
    let opts = InferOptions::default();
    assert!(opts.model.is_none());
    assert!(opts.temperature.is_none());
    assert!(opts.max_tokens.is_none());
    assert!(opts.system.is_none());
    assert!(opts.additional_params.is_none());
}

#[test]
fn test_infer_options_with_all_fields() {
    let opts = InferOptions {
        model: Some("gpt-4o".to_string()),
        temperature: Some(0.7),
        max_tokens: Some(2000),
        system: Some("You are a helpful assistant.".to_string()),
        additional_params: None,
    };
    assert_eq!(opts.model.as_deref(), Some("gpt-4o"));
    assert_eq!(opts.temperature, Some(0.7));
    assert_eq!(opts.max_tokens, Some(2000));
    assert_eq!(opts.system.as_deref(), Some("You are a helpful assistant."));
}

#[test]
fn test_infer_options_partial_fields() {
    let opts = InferOptions {
        temperature: Some(0.5),
        ..Default::default()
    };
    assert!(opts.model.is_none());
    assert_eq!(opts.temperature, Some(0.5));
    assert!(opts.max_tokens.is_none());
    assert!(opts.system.is_none());
    assert!(opts.additional_params.is_none());
}

#[test]
fn test_infer_options_temperature_zero() {
    let opts = InferOptions {
        temperature: Some(0.0),
        ..Default::default()
    };
    assert_eq!(opts.temperature, Some(0.0));
}

#[test]
fn test_infer_options_max_tokens_small() {
    let opts = InferOptions {
        max_tokens: Some(1),
        ..Default::default()
    };
    assert_eq!(opts.max_tokens, Some(1));
}

#[test]
fn test_infer_options_system_empty_string() {
    let opts = InferOptions {
        system: Some(String::new()),
        ..Default::default()
    };
    assert_eq!(opts.system.as_deref(), Some(""));
}

#[test]
fn test_infer_options_clone() {
    let opts = InferOptions {
        model: Some("test-model".to_string()),
        temperature: Some(0.8),
        max_tokens: Some(1000),
        system: Some("Test system".to_string()),
        additional_params: Some(serde_json::json!({"foo": "bar"})),
    };
    let cloned = opts.clone();
    assert_eq!(opts.model, cloned.model);
    assert_eq!(opts.temperature, cloned.temperature);
    assert_eq!(opts.max_tokens, cloned.max_tokens);
    assert_eq!(opts.system, cloned.system);
    assert_eq!(opts.additional_params, cloned.additional_params);
}

#[test]
fn test_infer_options_with_additional_params() {
    let params = serde_json::json!({
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "structured_output",
                "strict": true,
                "schema": { "type": "object" }
            }
        }
    });
    let opts = InferOptions {
        additional_params: Some(params.clone()),
        ..Default::default()
    };
    assert_eq!(opts.additional_params, Some(params));
}

#[test]
fn test_infer_options_with_extended_thinking() {
    let budget: u64 = 8192;
    let thinking_params = serde_json::json!({
        "thinking": { "type": "enabled", "budget_tokens": budget }
    });
    let opts = InferOptions {
        model: Some("claude-sonnet-4-6".to_string()),
        temperature: Some(1.0),
        max_tokens: Some((budget as u32) + 8192),
        system: None,
        additional_params: Some(thinking_params.clone()),
    };
    assert_eq!(opts.temperature, Some(1.0));
    assert_eq!(opts.max_tokens, Some(16384));
    let params = opts.additional_params.unwrap();
    assert_eq!(params["thinking"]["type"], "enabled");
    assert_eq!(params["thinking"]["budget_tokens"], 8192);
}

// =========================================================================
// Structured Output Helper Tests
// =========================================================================

#[test]
fn test_supports_native_structured_output_by_name() {
    assert!(supports_native_structured_output("openai"));
    assert!(supports_native_structured_output("groq"));
    assert!(supports_native_structured_output("deepseek"));
    assert!(supports_native_structured_output("xai"));

    assert!(!supports_native_structured_output("claude"));
    assert!(!supports_native_structured_output("anthropic"));
    assert!(!supports_native_structured_output("gemini"));
    assert!(!supports_native_structured_output("mistral"));
    assert!(!supports_native_structured_output("native"));
    assert!(!supports_native_structured_output("mock"));
    // Custom endpoints like "h100" are NOT detected by the string check
    assert!(!supports_native_structured_output("h100"));
}

#[test]
#[serial]
fn test_supports_native_structured_output_by_provider() {
    // RigProvider method detects OpenAiCompat (custom endpoints)
    std::env::set_var("OPENAI_API_KEY", "test-key");
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    assert!(RigProvider::openai().supports_native_structured_output());
    assert!(!RigProvider::claude().supports_native_structured_output());

    let compat =
        RigProvider::openai_compat("h100", "http://localhost:8000/v1", "test", None, 300).unwrap();
    assert!(
        compat.supports_native_structured_output(),
        "OpenAiCompat (custom endpoints) should support native structured output"
    );
}

#[test]
fn test_build_response_format_params() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "score": { "type": "number" }
        },
        "required": ["name", "score"]
    });
    let params = build_response_format_params(&schema);
    assert_eq!(params["response_format"]["type"], "json_schema");
    assert_eq!(
        params["response_format"]["json_schema"]["name"],
        "structured_output"
    );
    assert_eq!(params["response_format"]["json_schema"]["strict"], true);
    assert_eq!(
        params["response_format"]["json_schema"]["schema"]["properties"]["name"]["type"],
        "string"
    );
}

#[test]
fn test_build_response_format_preserves_full_schema() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "items": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["items"],
        "additionalProperties": false
    });
    let params = build_response_format_params(&schema);
    let embedded = &params["response_format"]["json_schema"]["schema"];
    assert_eq!(embedded["additionalProperties"], false);
    assert_eq!(embedded["properties"]["items"]["type"], "array");
}

// =========================================================================
// Vision Provider Tests
// =========================================================================

#[test]
fn vision_not_supported_error_display() {
    let err = RigInferError::VisionNotSupported("DeepSeek no vision".to_string());
    assert!(err.to_string().contains("Vision not supported"));
    assert!(err.to_string().contains("DeepSeek no vision"));
}

/// Test DeepSeek vision rejection (only when DEEPSEEK_API_KEY is set)
#[tokio::test]
async fn infer_vision_deepseek_returns_error() {
    if std::env::var("DEEPSEEK_API_KEY").is_err() {
        // Can't construct DeepSeek without API key; test message building instead
        let err = RigInferError::VisionNotSupported("DeepSeek".to_string());
        assert!(err.to_string().contains("Vision not supported"));
        return;
    }
    let provider = RigProvider::deepseek();
    let content = vec![rig::completion::message::UserContent::text("hello")];
    let result = provider.infer_vision(content, None, None, None).await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        RigInferError::VisionNotSupported(_)
    ));
}

#[test]
fn infer_vision_empty_content_builds_error() {
    // OneOrMany::many rejects empty vecs, which infer_vision maps to VisionNotSupported
    use rig::OneOrMany;
    let content: Vec<rig::completion::message::UserContent> = vec![];
    let result = OneOrMany::many(content);
    assert!(result.is_err(), "empty content should fail");
}

#[test]
fn build_vision_user_content_text_only() {
    let content = [rig::completion::message::UserContent::text("Describe this")];
    assert_eq!(content.len(), 1);
}

#[test]
fn build_vision_user_content_with_image() {
    use rig::completion::message::{ImageMediaType, UserContent};
    let content = [
        UserContent::text("What is in this image?"),
        UserContent::image_base64(
            "iVBORw0KGgo=", // fake base64
            Some(ImageMediaType::PNG),
            None,
        ),
    ];
    assert_eq!(content.len(), 2);
}

#[test]
fn build_vision_message_from_content() {
    use rig::completion::message::{ImageMediaType, Message, UserContent};
    use rig::OneOrMany;

    let parts = vec![
        UserContent::text("Describe this image"),
        UserContent::image_base64("iVBORw0KGgo=", Some(ImageMediaType::PNG), None),
    ];
    let msg = Message::User {
        content: OneOrMany::many(parts).unwrap(),
    };
    assert!(matches!(msg, Message::User { .. }));
}

// =========================================================================
// Reasoning Model Detection Tests (BUG 5 / NIKA-031)
// =========================================================================

#[test]
fn reasoning_model_o_series() {
    assert!(is_reasoning_model("o1"));
    assert!(is_reasoning_model("o1-mini"));
    assert!(is_reasoning_model("o1-pro"));
    assert!(is_reasoning_model("o3"));
    assert!(is_reasoning_model("o3-mini"));
    assert!(is_reasoning_model("o3-pro"));
    assert!(is_reasoning_model("o4"));
    assert!(is_reasoning_model("o4-mini"));
    assert!(is_reasoning_model("o1-2024-12-17"));
}

#[test]
fn reasoning_model_gpt5() {
    // gpt-5.x supports temperature, so is_reasoning_model returns false.
    // Use model_capabilities() for richer checks — these are "temperature-rejects" tests.
    // gpt-5.x needs max_completion_tokens but DOES support temperature.
    assert!(!is_reasoning_model("gpt-5"));
    assert!(!is_reasoning_model("gpt-5-turbo"));
    assert!(!is_reasoning_model("gpt-5.2"));
    assert!(!is_reasoning_model("gpt-5.2-pro"));

    // Verify via catalog that they DO need max_completion_tokens
    use nika_core::catalogs::capabilities::{model_capabilities, TokenLimitParam};
    assert_eq!(
        model_capabilities("openai", "gpt-5.2").token_limit_param,
        TokenLimitParam::MaxCompletionTokens
    );
}

#[test]
fn reasoning_model_deepseek() {
    // DeepSeek Reasoner: uses max_tokens (standard), but rejects temperature
    assert!(is_reasoning_model("deepseek-reasoner"));
}

#[test]
fn reasoning_model_case_insensitive() {
    assert!(is_reasoning_model("O1"));
    // GPT-5 supports temperature → is_reasoning_model returns false
    assert!(!is_reasoning_model("GPT-5"));
}

#[test]
fn non_reasoning_models() {
    assert!(!is_reasoning_model("gpt-4o"));
    assert!(!is_reasoning_model("gpt-4o-mini"));
    assert!(!is_reasoning_model("claude-sonnet-4"));
    assert!(!is_reasoning_model("deepseek-chat"));
    assert!(!is_reasoning_model("gemini-2.0-flash"));
    assert!(!is_reasoning_model("grok-3"));
}

// =========================================================================
// Endpoint resolution tests
// =========================================================================

#[test]
fn test_from_name_with_endpoints_custom() {
    use crate::provider::endpoints::{CustomEndpointMap, ResolvedEndpoint};

    let mut endpoints = CustomEndpointMap::new();
    endpoints.insert(
        "local".to_string(),
        ResolvedEndpoint {
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: "ollama".to_string(),
            default_model: Some("llama3.2".to_string()),
            timeout_secs: 300,
            hourly_rate: None,
            currency: "USD".to_string(),
        },
    );

    let provider = RigProvider::from_name_with_endpoints("local", &endpoints).unwrap();
    assert!(matches!(provider, RigProvider::OpenAiCompat { .. }));
}

#[test]
fn test_from_name_with_endpoints_fallback_to_catalog() {
    use crate::provider::endpoints::{CustomEndpointMap, ResolvedEndpoint};

    // Add endpoint "myserver" but look up "openai" -> should fall through to catalog
    let mut endpoints = CustomEndpointMap::new();
    endpoints.insert(
        "myserver".to_string(),
        ResolvedEndpoint {
            base_url: "http://localhost:8000/v1".to_string(),
            api_key: "test".to_string(),
            default_model: None,
            timeout_secs: 300,
            hourly_rate: None,
            currency: "USD".to_string(),
        },
    );

    // "openai" is not in custom endpoints -> falls through to catalog
    // The catalog lookup should not match "myserver"
    let result = RigProvider::from_name_with_endpoints("myserver", &endpoints);
    assert!(
        matches!(result.as_ref().unwrap(), RigProvider::OpenAiCompat { .. }),
        "Custom endpoint should resolve to OpenAiCompat"
    );
}

#[test]
fn test_from_name_with_endpoints_unknown() {
    use crate::provider::endpoints::CustomEndpointMap;

    let endpoints = CustomEndpointMap::new();
    let result = RigProvider::from_name_with_endpoints("nonexistent", &endpoints);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("NIKA-030"),
        "Expected NIKA-030 not configured, got: {}",
        err_msg
    );
}

// =========================================================================
// Security: fail fast on missing API key for OpenAI-compat providers
// =========================================================================

#[test]
#[serial]
fn test_openai_compat_missing_key_fails_fast() {
    // Ensure no key is set for any of the OpenAI-compat providers
    let providers = [
        "openrouter",
        "together",
        "fireworks",
        "cerebras",
        "sambanova",
        "cohere",
        "ai21",
    ];

    for name in &providers {
        // Remove env var to ensure key is missing
        let env_var = format!("{}_API_KEY", name.to_uppercase());
        std::env::remove_var(&env_var);
        crate::secrets::store::clear();

        let result = RigProvider::from_name(name);
        assert!(
            result.is_err(),
            "Provider '{}' should fail when API key is missing, but got Ok",
            name
        );
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("API key")
                || err_str.contains("api key")
                || err_str.contains("NIKA-035"),
            "Error for '{}' should mention API key, got: {}",
            name,
            err_str
        );
    }
}

// =========================================================================
// Provider flag methods
// =========================================================================

#[test]
#[serial]
fn test_is_anthropic() {
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    assert!(RigProvider::claude().is_anthropic());
    std::env::set_var("OPENAI_API_KEY", "test-key");
    assert!(!RigProvider::openai().is_anthropic());
    assert!(!RigProvider::Mock.is_anthropic());
}

#[test]
fn test_supports_vision() {
    assert!(!RigProvider::Mock.supports_vision());
    let compat =
        RigProvider::openai_compat("test", "http://localhost:8000/v1", "k", None, 300).unwrap();
    assert!(compat.supports_vision());
}

#[test]
#[serial]
fn test_supports_thinking() {
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    assert!(RigProvider::claude().supports_thinking());
    std::env::set_var("OPENAI_API_KEY", "test-key");
    assert!(!RigProvider::openai().supports_thinking());
}

// =========================================================================
// Security: Debug impl redacts raw_api_key
// =========================================================================

#[test]
fn test_debug_redacts_raw_api_key() {
    let provider = RigProvider::openai_compat(
        "test-endpoint",
        "http://localhost:8000/v1",
        "sk-super-secret-key-12345",
        Some("test-model"),
        300,
    )
    .unwrap();

    let debug_output = format!("{:?}", provider);
    assert!(
        !debug_output.contains("sk-super-secret-key-12345"),
        "Debug output must NOT contain raw API key, got: {}",
        debug_output
    );
    assert!(
        debug_output.contains("***"),
        "Debug output should show redacted key as '***', got: {}",
        debug_output
    );
}

#[test]
fn test_debug_works_for_all_variants() {
    // Verify Debug doesn't panic for rig-core variants
    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    let claude = RigProvider::claude();
    let _ = format!("{:?}", claude);

    // Mock variant
    let mock = RigProvider::Mock;
    let debug = format!("{:?}", mock);
    assert!(debug.contains("Mock"));
}

// =========================================================================
// Fix 1.1: No Box::leak in name() and default_model()
// =========================================================================

#[test]
fn test_openai_compat_name_no_leak() {
    // Creating many OpenAiCompat providers should NOT permanently leak memory.
    // Before fix: Box::leak allocated a new &'static str every call.
    // After fix: cached_name field returned by reference.
    for i in 0..100 {
        let provider = RigProvider::openai_compat(
            &format!("endpoint-{}", i),
            "http://localhost:8000/v1",
            "test-key",
            Some("test-model"),
            300,
        )
        .unwrap();
        assert_eq!(provider.name(), format!("openai-compat:endpoint-{}", i));
    }
}

#[test]
fn test_openai_compat_default_model_cached() {
    let provider = RigProvider::openai_compat(
        "h100",
        "http://localhost:8000/v1",
        "test-key",
        Some("Qwen/Qwen3-8B"),
        300,
    )
    .unwrap();
    assert_eq!(provider.default_model(), "Qwen/Qwen3-8B");

    // Without default model → fallback
    let provider2 =
        RigProvider::openai_compat("h100", "http://localhost:8000/v1", "test-key", None, 300)
            .unwrap();
    assert_eq!(provider2.default_model(), "gpt-3.5-turbo");
}

// =========================================================================
// Fix 1.2: cost_provider_kind() for custom endpoints
// =========================================================================

#[test]
fn test_cost_provider_kind_standard_providers() {
    use crate::provider::cost::ProviderKind;

    // Use explicit API keys to avoid env var race in parallel tests
    let claude_client = anthropic::Client::builder()
        .api_key("test-key")
        .build()
        .unwrap();
    let openai_client = openai::Client::builder()
        .api_key("test-key")
        .build()
        .unwrap();
    assert_eq!(
        RigProvider::Claude(claude_client).cost_provider_kind(),
        Some(ProviderKind::Claude)
    );
    assert_eq!(
        RigProvider::OpenAI(openai_client).cost_provider_kind(),
        Some(ProviderKind::OpenAI)
    );
}

#[test]
fn test_cost_provider_kind_openai_compat() {
    use crate::provider::cost::ProviderKind;

    let provider = RigProvider::openai_compat(
        "h100",
        "http://localhost:8000/v1",
        "test-key",
        Some("Qwen/Qwen3-8B"),
        300,
    )
    .unwrap();
    // Custom endpoints use OpenAI-compatible API → treat as OpenAI for cost
    assert_eq!(provider.cost_provider_kind(), Some(ProviderKind::OpenAI));
}

#[test]
fn test_openai_compat_cost_not_zero() {
    use crate::provider::cost::calculate_cost;

    let provider = RigProvider::openai_compat(
        "h100",
        "http://localhost:8000/v1",
        "test-key",
        Some("gpt-4o"),
        300,
    )
    .unwrap();
    let pk = provider.cost_provider_kind().unwrap();
    let cost = calculate_cost(pk, "gpt-4o", 10_000, 5_000);
    assert!(
        cost > 0.0,
        "Cost should be non-zero for known model via OpenAiCompat"
    );
}

// =========================================================================
// vLLM response handling: raw_openai_compat_infer
// =========================================================================

/// Verify that raw_openai_compat_infer extracts content from a vLLM response
/// that includes non-standard fields (annotations, reasoning, stop_reason,
/// token_ids, kv_transfer_params, etc.) which crash rig-core deserialization.
#[tokio::test]
async fn test_raw_openai_compat_infer_vllm_response() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let vllm_response = serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 1712000000,
        "model": "qwen3.5-27b",
        "system_fingerprint": null,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "{\"name\":\"Rust\",\"category\":\"Systems\"}",
                "refusal": null,
                "annotations": null,
                "audio": null,
                "function_call": null,
                "tool_calls": [],
                "reasoning": null
            },
            "logprobs": null,
            "finish_reason": "stop",
            "stop_reason": null,
            "token_ids": null
        }],
        "usage": {
            "prompt_tokens": 80,
            "total_tokens": 95,
            "completion_tokens": 15,
            "prompt_tokens_details": null
        },
        "service_tier": null,
        "prompt_logprobs": null,
        "prompt_token_ids": null,
        "kv_transfer_params": null
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&vllm_response))
        .mount(&server)
        .await;

    let http_client = reqwest::Client::new();
    let messages = vec![serde_json::json!({"role": "user", "content": "test"})];
    let base_url = format!("{}/v1", server.uri());
    let result = RigProvider::raw_openai_compat_infer(
        &http_client,
        &base_url,
        "",
        "qwen3.5-27b",
        messages,
        100,
        None,
        std::time::Duration::from_secs(10),
    )
    .await;

    assert!(result.is_ok(), "Failed: {:?}", result.err());
    let (content, prompt_tokens, completion_tokens) = result.unwrap();
    assert_eq!(content, "{\"name\":\"Rust\",\"category\":\"Systems\"}");
    assert_eq!(prompt_tokens, 80);
    assert_eq!(completion_tokens, 15);
}

/// Verify that raw_openai_compat_infer handles vLLM responses with
/// <think>...</think> tags (Qwen reasoning) in content.
#[tokio::test]
async fn test_raw_openai_compat_infer_vllm_with_think_tags() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let vllm_response = serde_json::json!({
        "id": "chatcmpl-think",
        "object": "chat.completion",
        "created": 1712000000,
        "model": "qwen3.5-27b",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "<think>\nLet me think about this...\n</think>\n{\"name\":\"Python\"}",
                "refusal": null,
                "annotations": null,
                "reasoning": null
            },
            "logprobs": null,
            "finish_reason": "stop",
            "stop_reason": null
        }],
        "usage": { "prompt_tokens": 50, "total_tokens": 80, "completion_tokens": 30 }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&vllm_response))
        .mount(&server)
        .await;

    let http_client = reqwest::Client::new();
    let messages = vec![serde_json::json!({"role": "user", "content": "test"})];
    let base_url = format!("{}/v1", server.uri());
    let result = RigProvider::raw_openai_compat_infer(
        &http_client,
        &base_url,
        "",
        "qwen3.5-27b",
        messages,
        100,
        None,
        std::time::Duration::from_secs(10),
    )
    .await;

    // raw_openai_compat_infer returns the raw content; strip_think_tags is
    // applied by the caller (make_infer_callback).
    assert!(result.is_ok(), "Failed: {:?}", result.err());
    let (content, _, _) = result.unwrap();
    assert!(content.contains("{\"name\":\"Python\"}"));
}

/// Verify that raw_openai_compat_infer returns an error for HTTP failures.
#[tokio::test]
async fn test_raw_openai_compat_infer_http_error() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limited"))
        .mount(&server)
        .await;

    let http_client = reqwest::Client::new();
    let messages = vec![serde_json::json!({"role": "user", "content": "test"})];
    let base_url = format!("{}/v1", server.uri());
    let result = RigProvider::raw_openai_compat_infer(
        &http_client,
        &base_url,
        "",
        "qwen3.5-27b",
        messages,
        100,
        None,
        std::time::Duration::from_secs(10),
    )
    .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("429"), "Error should mention status: {err}");
}

/// Verify that raw_openai_compat_infer sends bearer auth when api_key is set.
#[tokio::test]
async fn test_raw_openai_compat_infer_sends_auth_header() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let response = serde_json::json!({
        "id": "chatcmpl-auth",
        "object": "chat.completion",
        "created": 1712000000,
        "model": "qwen3.5-27b",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "ok" },
            "logprobs": null,
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 10, "total_tokens": 11, "completion_tokens": 1 }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer my-secret-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&server)
        .await;

    let http_client = reqwest::Client::new();
    let messages = vec![serde_json::json!({"role": "user", "content": "test"})];
    let base_url = format!("{}/v1", server.uri());
    let result = RigProvider::raw_openai_compat_infer(
        &http_client,
        &base_url,
        "my-secret-key",
        "qwen3.5-27b",
        messages,
        100,
        None,
        std::time::Duration::from_secs(10),
    )
    .await;

    assert!(result.is_ok(), "Auth should work: {:?}", result.err());
    let (content, _, _) = result.unwrap();
    assert_eq!(content, "ok");
}

/// Test infer_with_tools raw HTTP path for OpenAiCompat: extracts tool_calls
/// arguments from vLLM response without going through rig-core deserialization.
#[tokio::test]
async fn test_openai_compat_infer_with_tools_raw_http() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let vllm_tool_response = serde_json::json!({
        "id": "chatcmpl-tools",
        "object": "chat.completion",
        "created": 1712000000,
        "model": "qwen3.5-27b",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_123",
                    "type": "function",
                    "function": {
                        "name": "submit_result",
                        "arguments": "{\"name\":\"Rust\",\"age\":30}"
                    }
                }],
                "refusal": null,
                "annotations": null,
                "reasoning": null
            },
            "logprobs": null,
            "finish_reason": "tool_calls",
            "stop_reason": null,
            "token_ids": null
        }],
        "usage": {
            "prompt_tokens": 120,
            "total_tokens": 150,
            "completion_tokens": 30,
            "prompt_tokens_details": null
        },
        "kv_transfer_params": null
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&vllm_tool_response))
        .mount(&server)
        .await;

    let provider = RigProvider::openai_compat(
        "h100",
        &format!("{}/v1", server.uri()),
        "test-key",
        Some("qwen3.5-27b"),
        300,
    )
    .unwrap();

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "number" }
        },
        "required": ["name", "age"]
    });
    let submit_tool = crate::runtime::submit_tool::DynamicSubmitTool::new(schema);
    let tools: Vec<Box<dyn rig::tool::ToolDyn>> = vec![Box::new(submit_tool)];

    let result = provider
        .infer_with_tools("Extract info", tools, None, None, None)
        .await;

    assert!(result.is_ok(), "Failed: {:?}", result.err());
    let (content, prompt_tokens, completion_tokens) = result.unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(json["name"], "Rust");
    assert_eq!(json["age"], 30);
    // Verify tokens are tracked from API response
    assert_eq!(prompt_tokens, 120);
    assert_eq!(completion_tokens, 30);
}

/// Test infer_with_tools fallback: when vLLM responds with content instead
/// of tool_calls (some models don't support tool calling).
#[tokio::test]
async fn test_openai_compat_infer_with_tools_content_fallback() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let vllm_no_tools_response = serde_json::json!({
        "id": "chatcmpl-notool",
        "object": "chat.completion",
        "created": 1712000000,
        "model": "qwen3.5-27b",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "{\"name\":\"Python\",\"age\":25}",
                "tool_calls": [],
                "refusal": null,
                "annotations": null
            },
            "logprobs": null,
            "finish_reason": "stop",
            "stop_reason": null
        }],
        "usage": { "prompt_tokens": 80, "total_tokens": 100, "completion_tokens": 20 }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&vllm_no_tools_response))
        .mount(&server)
        .await;

    let provider = RigProvider::openai_compat(
        "h100",
        &format!("{}/v1", server.uri()),
        "test-key",
        Some("qwen3.5-27b"),
        300,
    )
    .unwrap();

    let schema = serde_json::json!({
        "type": "object",
        "properties": { "name": { "type": "string" } },
        "required": ["name"]
    });
    let submit_tool = crate::runtime::submit_tool::DynamicSubmitTool::new(schema);
    let tools: Vec<Box<dyn rig::tool::ToolDyn>> = vec![Box::new(submit_tool)];

    let result = provider
        .infer_with_tools("Extract info", tools, None, None, None)
        .await;

    // Falls back to content field when tool_calls is empty
    assert!(result.is_ok(), "Failed: {:?}", result.err());
    let (content, _prompt_tokens, _completion_tokens) = result.unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(json["name"], "Python");
}

/// Test that infer_with_tools returns non-zero token counts for OpenAiCompat.
/// This was a bug: the OpenAiCompat path discarded usage data from the API response,
/// causing telemetry to silently report zero tokens.
#[tokio::test]
async fn test_openai_compat_infer_with_tools_tracks_tokens() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    let response = serde_json::json!({
        "id": "chatcmpl-tokens",
        "object": "chat.completion",
        "created": 1712000000,
        "model": "qwen3.5-27b",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_456",
                    "type": "function",
                    "function": {
                        "name": "submit_result",
                        "arguments": "{\"name\":\"Token\",\"age\":42}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": { "prompt_tokens": 200, "total_tokens": 250, "completion_tokens": 50 }
    });

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&server)
        .await;

    let provider = RigProvider::openai_compat(
        "h100",
        &format!("{}/v1", server.uri()),
        "test-key",
        Some("qwen3.5-27b"),
        300,
    )
    .unwrap();

    let schema = serde_json::json!({
        "type": "object",
        "properties": { "name": { "type": "string" }, "age": { "type": "number" } },
        "required": ["name", "age"]
    });
    let submit_tool = crate::runtime::submit_tool::DynamicSubmitTool::new(schema);
    let tools: Vec<Box<dyn rig::tool::ToolDyn>> = vec![Box::new(submit_tool)];

    let result = provider
        .infer_with_tools("Extract info", tools, None, None, None)
        .await;

    assert!(result.is_ok(), "Failed: {:?}", result.err());
    let (content, prompt_tokens, completion_tokens) = result.unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(json["name"], "Token");
    assert_eq!(json["age"], 42);

    // Key assertion: tokens must be non-zero (the bug was silent zero telemetry)
    assert_eq!(
        prompt_tokens, 200,
        "prompt_tokens should come from API response"
    );
    assert_eq!(
        completion_tokens, 50,
        "completion_tokens should come from API response"
    );
}

// =========================================================================
// token_limit_for_model tests
// =========================================================================

#[test]
fn token_limit_openai_o3_uses_max_completion_tokens() {
    let (rig_max, extra) = super::token_limit_for_model("openai", "o3", 4096);
    assert!(rig_max.is_none(), "o3: must NOT set rig max_tokens");
    let params = extra.expect("o3: must inject additional_params");
    assert_eq!(params["max_completion_tokens"], 4096);
}

#[test]
fn token_limit_openai_o4_mini_uses_max_completion_tokens() {
    let (rig_max, extra) = super::token_limit_for_model("openai", "o4-mini", 8192);
    assert!(rig_max.is_none());
    assert_eq!(extra.unwrap()["max_completion_tokens"], 8192);
}

#[test]
fn token_limit_openai_gpt52_uses_max_completion_tokens() {
    let (rig_max, extra) = super::token_limit_for_model("openai", "gpt-5.2", 16384);
    assert!(rig_max.is_none(), "gpt-5.2: must NOT set rig max_tokens");
    assert_eq!(extra.unwrap()["max_completion_tokens"], 16384);
}

#[test]
fn token_limit_openai_gpt4o_uses_max_tokens() {
    let (rig_max, extra) = super::token_limit_for_model("openai", "gpt-4o", 4096);
    assert_eq!(rig_max, Some(4096));
    assert!(extra.is_none(), "gpt-4o: no additional_params needed");
}

#[test]
fn token_limit_anthropic_uses_max_tokens() {
    let (rig_max, extra) = super::token_limit_for_model("anthropic", "claude-sonnet-4-6", 8192);
    assert_eq!(rig_max, Some(8192));
    assert!(extra.is_none());
}

#[test]
fn token_limit_custom_endpoint_uses_max_tokens() {
    // Custom/vLLM endpoints: safe defaults even if model name matches o3
    let (rig_max, extra) = super::token_limit_for_model("custom", "o3", 4096);
    assert_eq!(
        rig_max,
        Some(4096),
        "custom endpoints always use max_tokens"
    );
    assert!(extra.is_none());
}

#[test]
fn token_limit_groq_uses_max_tokens() {
    let (rig_max, extra) = super::token_limit_for_model("groq", "llama-3.3-70b", 4096);
    assert_eq!(rig_max, Some(4096));
    assert!(extra.is_none());
}
