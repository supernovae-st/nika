//! Claude provider chat tests
//!
//! Tests for message serialization and tool definition formatting
//! compatible with Claude's Messages API.

use nika::provider::{
    ChatResponse, ContentBlock, Message, MessageContent, MessageRole, StopReason, ToolCall,
    ToolDefinition, Usage,
};
use serde_json::json;

// ============================================================================
// ToolDefinition Tests
// ============================================================================

#[test]
fn test_tool_definition_serialization() {
    let tool = ToolDefinition {
        name: "novanet_generate".to_string(),
        description: "Generate content context".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string"},
                "entity": {"type": "string"}
            },
            "required": ["mode"]
        }),
    };

    let json = serde_json::to_value(&tool).unwrap();
    assert_eq!(json["name"], "novanet_generate");
    assert_eq!(json["description"], "Generate content context");
    assert!(json["input_schema"]["properties"].is_object());
    assert_eq!(json["input_schema"]["required"][0], "mode");
}

#[test]
fn test_tool_definition_constructor() {
    let tool = ToolDefinition::new(
        "novanet_traverse",
        "Traverse knowledge graph",
        json!({
            "type": "object",
            "properties": {
                "start": {"type": "string"},
                "arc": {"type": "string"}
            }
        }),
    );

    assert_eq!(tool.name, "novanet_traverse");
    assert_eq!(tool.description, "Traverse knowledge graph");
}

// ============================================================================
// Message Serialization Tests
// ============================================================================

#[test]
fn test_message_serialization_for_claude_api() {
    let messages = vec![Message::user("Hello"), Message::assistant("Hi there!")];

    let json = serde_json::to_value(&messages).unwrap();
    assert_eq!(json[0]["role"], "user");
    assert_eq!(json[0]["content"], "Hello");
    assert_eq!(json[1]["role"], "assistant");
    assert_eq!(json[1]["content"], "Hi there!");
}

#[test]
fn test_tool_result_message() {
    let msg = Message::tool_result("call_123", "The result data");

    assert_eq!(msg.role, MessageRole::Tool);
    assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
    assert_eq!(
        msg.content.as_text(),
        Some("The result data".to_string())
    );
}

#[test]
fn test_system_message() {
    let msg = Message::system("You are a helpful assistant.");

    assert_eq!(msg.role, MessageRole::System);
    assert_eq!(
        msg.content.as_text(),
        Some("You are a helpful assistant.".to_string())
    );
}

// ============================================================================
// ContentBlock Tests
// ============================================================================

#[test]
fn test_content_block_tool_use_serialization() {
    let block = ContentBlock::ToolUse {
        id: "toolu_123".to_string(),
        name: "novanet_generate".to_string(),
        input: json!({"entity": "qr-code"}),
    };

    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "tool_use");
    assert_eq!(json["id"], "toolu_123");
    assert_eq!(json["name"], "novanet_generate");
    assert_eq!(json["input"]["entity"], "qr-code");
}

#[test]
fn test_content_block_tool_result_serialization() {
    let block = ContentBlock::ToolResult {
        tool_use_id: "toolu_123".to_string(),
        content: "Generated content here".to_string(),
        is_error: None,
    };

    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "tool_result");
    assert_eq!(json["tool_use_id"], "toolu_123");
    assert_eq!(json["content"], "Generated content here");
    // is_error should be skipped when None
    assert!(json.get("is_error").is_none());
}

#[test]
fn test_content_block_tool_result_with_error_serialization() {
    let block = ContentBlock::ToolResult {
        tool_use_id: "toolu_456".to_string(),
        content: "Error: tool execution failed".to_string(),
        is_error: Some(true),
    };

    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "tool_result");
    assert_eq!(json["is_error"], true);
}

#[test]
fn test_message_with_blocks() {
    let msg = Message::assistant_blocks(vec![
        ContentBlock::Text {
            text: "Let me help you.".to_string(),
        },
        ContentBlock::ToolUse {
            id: "toolu_001".to_string(),
            name: "novanet_generate".to_string(),
            input: json!({"mode": "block"}),
        },
    ]);

    assert_eq!(msg.role, MessageRole::Assistant);
    match &msg.content {
        MessageContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
            match &blocks[0] {
                ContentBlock::Text { text } => assert_eq!(text, "Let me help you."),
                _ => panic!("Expected text block"),
            }
            match &blocks[1] {
                ContentBlock::ToolUse { id, name, .. } => {
                    assert_eq!(id, "toolu_001");
                    assert_eq!(name, "novanet_generate");
                }
                _ => panic!("Expected tool_use block"),
            }
        }
        _ => panic!("Expected blocks content"),
    }
}

// ============================================================================
// StopReason Tests
// ============================================================================

#[test]
fn test_stop_reason_variants() {
    assert_ne!(StopReason::EndTurn, StopReason::ToolUse);
    assert_ne!(StopReason::MaxTokens, StopReason::StopSequence);
    assert_ne!(StopReason::EndTurn, StopReason::Unknown);
}

#[test]
fn test_stop_reason_serialization_snake_case() {
    // Claude API uses snake_case for stop reasons
    let json = serde_json::to_value(&StopReason::EndTurn).unwrap();
    assert_eq!(json, "end_turn");

    let json = serde_json::to_value(&StopReason::ToolUse).unwrap();
    assert_eq!(json, "tool_use");

    let json = serde_json::to_value(&StopReason::MaxTokens).unwrap();
    assert_eq!(json, "max_tokens");

    let json = serde_json::to_value(&StopReason::StopSequence).unwrap();
    assert_eq!(json, "stop_sequence");
}

// ============================================================================
// Usage Tests
// ============================================================================

#[test]
fn test_usage_accumulation() {
    let usage1 = Usage {
        input_tokens: 100,
        output_tokens: 50,
        cache_creation_input_tokens: 10,
        cache_read_input_tokens: 20,
    };

    let usage2 = Usage {
        input_tokens: 200,
        output_tokens: 100,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 30,
    };

    let total = usage1 + usage2;
    assert_eq!(total.input_tokens, 300);
    assert_eq!(total.output_tokens, 150);
    assert_eq!(total.cache_creation_input_tokens, 10);
    assert_eq!(total.cache_read_input_tokens, 50);
}

#[test]
fn test_usage_total_tokens() {
    let usage = Usage::new(500, 150);
    assert_eq!(usage.total_tokens(), 650);
}

// ============================================================================
// ChatResponse Tests
// ============================================================================

#[test]
fn test_chat_response_with_tool_calls() {
    // Simulate a response from Claude with tool calls
    let response = ChatResponse {
        content: MessageContent::Text("I'll generate that content for you.".to_string()),
        tool_calls: vec![ToolCall {
            id: "toolu_001".to_string(),
            name: "novanet_generate".to_string(),
            arguments: json!({"entity": "qr-code", "locale": "fr-FR"}),
        }],
        stop_reason: StopReason::ToolUse,
        usage: Usage {
            input_tokens: 500,
            output_tokens: 150,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 100,
        },
    };

    assert_eq!(response.stop_reason, StopReason::ToolUse);
    assert!(response.has_tool_calls());
    assert_eq!(response.tool_calls.len(), 1);
    assert_eq!(response.tool_calls[0].name, "novanet_generate");
    assert_eq!(response.tool_calls[0].arguments["entity"], "qr-code");
}

#[test]
fn test_chat_response_without_tool_calls() {
    let response = ChatResponse {
        content: MessageContent::Text("Hello! How can I help you today?".to_string()),
        tool_calls: vec![],
        stop_reason: StopReason::EndTurn,
        usage: Usage::new(100, 50),
    };

    assert_eq!(response.stop_reason, StopReason::EndTurn);
    assert!(!response.has_tool_calls());
    assert_eq!(
        response.text(),
        Some("Hello! How can I help you today?".to_string())
    );
}

#[test]
fn test_chat_response_multiple_tool_calls() {
    // Claude can request multiple tool calls in one response
    let response = ChatResponse {
        content: MessageContent::Text("I'll fetch both pieces of information.".to_string()),
        tool_calls: vec![
            ToolCall {
                id: "toolu_001".to_string(),
                name: "novanet_generate".to_string(),
                arguments: json!({"entity": "qr-code"}),
            },
            ToolCall {
                id: "toolu_002".to_string(),
                name: "novanet_traverse".to_string(),
                arguments: json!({"start": "entity:qr-code", "arc": "HAS_NATIVE"}),
            },
        ],
        stop_reason: StopReason::ToolUse,
        usage: Usage::new(300, 200),
    };

    assert_eq!(response.tool_calls.len(), 2);
    assert_eq!(response.tool_calls[0].id, "toolu_001");
    assert_eq!(response.tool_calls[1].id, "toolu_002");
}

// ============================================================================
// ToolCall Tests
// ============================================================================

#[test]
fn test_tool_call_serialization() {
    let call = ToolCall {
        id: "toolu_abc123".to_string(),
        name: "novanet_generate".to_string(),
        arguments: json!({
            "entity": "landing-page",
            "locale": "en-US",
            "forms": ["title", "description"]
        }),
    };

    let json = serde_json::to_value(&call).unwrap();
    assert_eq!(json["id"], "toolu_abc123");
    assert_eq!(json["name"], "novanet_generate");
    assert_eq!(json["arguments"]["entity"], "landing-page");
    assert_eq!(json["arguments"]["forms"][0], "title");
}

#[test]
fn test_tool_call_deserialization() {
    let json_str = r#"{
        "id": "toolu_xyz789",
        "name": "novanet_traverse",
        "arguments": {"start": "entity:qr-code"}
    }"#;

    let call: ToolCall = serde_json::from_str(json_str).unwrap();
    assert_eq!(call.id, "toolu_xyz789");
    assert_eq!(call.name, "novanet_traverse");
    assert_eq!(call.arguments["start"], "entity:qr-code");
}

// ============================================================================
// Message Role Tests
// ============================================================================

#[test]
fn test_message_role_serialization_lowercase() {
    // Claude API expects lowercase role names
    assert_eq!(
        serde_json::to_string(&MessageRole::User).unwrap(),
        "\"user\""
    );
    assert_eq!(
        serde_json::to_string(&MessageRole::Assistant).unwrap(),
        "\"assistant\""
    );
    assert_eq!(
        serde_json::to_string(&MessageRole::Tool).unwrap(),
        "\"tool\""
    );
    assert_eq!(
        serde_json::to_string(&MessageRole::System).unwrap(),
        "\"system\""
    );
}

// ============================================================================
// Full Conversation Round-trip Test
// ============================================================================

#[test]
fn test_conversation_serialization_roundtrip() {
    let conversation = vec![
        Message::system("You are a helpful assistant with access to NovaNet."),
        Message::user("Generate content for the qr-code entity in French."),
        Message::assistant_blocks(vec![
            ContentBlock::Text {
                text: "I'll generate that content for you.".to_string(),
            },
            ContentBlock::ToolUse {
                id: "toolu_001".to_string(),
                name: "novanet_generate".to_string(),
                input: json!({"entity": "qr-code", "locale": "fr-FR"}),
            },
        ]),
        Message::tool_result("toolu_001", r#"{"title": "Code QR", "description": "..."}"#),
    ];

    // Serialize
    let json = serde_json::to_string(&conversation).unwrap();

    // Deserialize
    let parsed: Vec<Message> = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.len(), 4);
    assert_eq!(parsed[0].role, MessageRole::System);
    assert_eq!(parsed[1].role, MessageRole::User);
    assert_eq!(parsed[2].role, MessageRole::Assistant);
    assert_eq!(parsed[3].role, MessageRole::Tool);
    assert_eq!(parsed[3].tool_call_id, Some("toolu_001".to_string()));
}
