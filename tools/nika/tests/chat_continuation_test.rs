//! Tests for RigAgentLoop chat continuation methods (v0.6)
//!
//! These tests verify the chat history management and multi-turn conversation
//! capabilities added in v0.6.
//!
//! History management methods (add_to_history, clear_history, etc.) are tested
//! without API keys. The chat_continue method requires API keys and is marked
//! with `#[ignore]`.
//!
//! Run with API keys:
//!   ANTHROPIC_API_KEY=... cargo test --test chat_continuation_test -- --ignored
//!   OPENAI_API_KEY=... cargo test --test chat_continuation_test -- --ignored

use rig::message::Message;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use nika::ast::AgentParams;
use nika::event::EventLog;
use nika::mcp::McpClient;
use nika::runtime::{RigAgentLoop, RigAgentStatus};

// ═══════════════════════════════════════════════════════════════════════════
// Test Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn create_agent_loop(prompt: &str) -> RigAgentLoop {
    let params = AgentParams {
        prompt: prompt.to_string(),
        mcp: vec![],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    RigAgentLoop::new("test_chat".to_string(), params, event_log, mcp_clients)
        .expect("Should create agent loop")
}

// ═══════════════════════════════════════════════════════════════════════════
// History Management Unit Tests (no API keys needed)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_history_starts_empty() {
    let agent_loop = create_agent_loop("Initial prompt");

    assert_eq!(agent_loop.history_len(), 0, "History should start empty");
    assert!(agent_loop.history().is_empty());
}

#[test]
fn test_add_to_history_creates_two_messages() {
    let mut agent_loop = create_agent_loop("Initial prompt");

    agent_loop.add_to_history("User question", "Assistant answer");

    assert_eq!(
        agent_loop.history_len(),
        2,
        "Should add both user and assistant messages"
    );
}

#[test]
fn test_add_to_history_preserves_content() {
    let mut agent_loop = create_agent_loop("Initial prompt");

    agent_loop.add_to_history("What is 2+2?", "4");

    let history = agent_loop.history();
    assert_eq!(history.len(), 2);

    // First message should be user
    match &history[0] {
        Message::User { content } => {
            let text: String = content
                .iter()
                .filter_map(|part| {
                    if let rig::message::UserContent::Text(rig::message::Text { text }) = part {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(text, "What is 2+2?");
        }
        _ => panic!("First message should be User"),
    }

    // Second message should be assistant
    match &history[1] {
        Message::Assistant { content, .. } => {
            let text: String = content
                .iter()
                .filter_map(|part| {
                    if let rig::message::AssistantContent::Text(rig::message::Text { text }) = part
                    {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(text, "4");
        }
        _ => panic!("Second message should be Assistant"),
    }
}

#[test]
fn test_add_to_history_multiple_turns() {
    let mut agent_loop = create_agent_loop("Initial prompt");

    agent_loop.add_to_history("First question", "First answer");
    agent_loop.add_to_history("Second question", "Second answer");
    agent_loop.add_to_history("Third question", "Third answer");

    assert_eq!(
        agent_loop.history_len(),
        6,
        "Should have 6 messages (3 turns × 2 messages)"
    );
}

#[test]
fn test_push_message_adds_single_message() {
    let mut agent_loop = create_agent_loop("Initial prompt");

    agent_loop.push_message(Message::user("Single user message"));

    assert_eq!(agent_loop.history_len(), 1, "Should have exactly 1 message");
}

#[test]
fn test_push_message_allows_mixed_order() {
    let mut agent_loop = create_agent_loop("Initial prompt");

    // Unusual order (assistant first) should still work
    agent_loop.push_message(Message::assistant("System context"));
    agent_loop.push_message(Message::user("User query"));

    assert_eq!(agent_loop.history_len(), 2);
}

#[test]
fn test_clear_history_removes_all_messages() {
    let mut agent_loop = create_agent_loop("Initial prompt");

    // Add some history
    agent_loop.add_to_history("Q1", "A1");
    agent_loop.add_to_history("Q2", "A2");
    assert_eq!(agent_loop.history_len(), 4);

    // Clear it
    agent_loop.clear_history();

    assert_eq!(
        agent_loop.history_len(),
        0,
        "History should be empty after clear"
    );
    assert!(agent_loop.history().is_empty());
}

#[test]
fn test_history_returns_slice() {
    let mut agent_loop = create_agent_loop("Initial prompt");

    agent_loop.add_to_history("Q", "A");

    let history = agent_loop.history();

    // Should be a slice, not owned
    assert_eq!(history.len(), 2);
    // Verify it's a reference (compile-time check)
    let _: &[Message] = history;
}

#[test]
fn test_with_history_sets_initial_history() {
    let params = AgentParams {
        prompt: "Continue conversation".to_string(),
        mcp: vec![],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    // Create with pre-existing history
    let pre_history = vec![
        Message::user("Previous question"),
        Message::assistant("Previous answer"),
    ];

    let agent_loop = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients)
        .unwrap()
        .with_history(pre_history);

    assert_eq!(
        agent_loop.history_len(),
        2,
        "Should have pre-existing history"
    );
}

#[test]
fn test_with_history_chaining() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        mcp: vec![],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    // Verify with_history returns Self for chaining
    let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients)
        .unwrap()
        .with_history(vec![Message::user("Hello")]);

    assert_eq!(agent.history_len(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// chat_continue Error Handling Tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_chat_continue_without_api_key_returns_error() {
    // Save current API keys
    let saved_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();
    let saved_openai = std::env::var("OPENAI_API_KEY").ok();
    let saved_mistral = std::env::var("MISTRAL_API_KEY").ok();
    let saved_groq = std::env::var("GROQ_API_KEY").ok();
    let saved_deepseek = std::env::var("DEEPSEEK_API_KEY").ok();
    let saved_ollama = std::env::var("OLLAMA_API_BASE_URL").ok();

    // Remove all provider API keys
    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("MISTRAL_API_KEY");
    std::env::remove_var("GROQ_API_KEY");
    std::env::remove_var("DEEPSEEK_API_KEY");
    std::env::remove_var("OLLAMA_API_BASE_URL");

    let mut agent_loop = create_agent_loop("Initial prompt");

    let result = agent_loop.chat_continue("Follow-up").await;

    // Restore API keys before assertions (in case of panic)
    if let Some(key) = saved_anthropic {
        std::env::set_var("ANTHROPIC_API_KEY", key);
    }
    if let Some(key) = saved_openai {
        std::env::set_var("OPENAI_API_KEY", key);
    }
    if let Some(key) = saved_mistral {
        std::env::set_var("MISTRAL_API_KEY", key);
    }
    if let Some(key) = saved_groq {
        std::env::set_var("GROQ_API_KEY", key);
    }
    if let Some(key) = saved_deepseek {
        std::env::set_var("DEEPSEEK_API_KEY", key);
    }
    if let Some(key) = saved_ollama {
        std::env::set_var("OLLAMA_API_BASE_URL", key);
    }

    assert!(result.is_err(), "Should fail without any API key");

    let err = result.unwrap_err();
    let err_string = err.to_string();
    assert!(
        err_string.contains("NIKA-113") || err_string.contains("chat_continue"),
        "Error should mention chat_continue requirement: {err_string}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Tests - chat_continue with Claude (require ANTHROPIC_API_KEY)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY environment variable"]
async fn test_chat_continue_claude_updates_history() {
    let mut agent_loop = create_agent_loop("You are a helpful assistant.");

    // First turn via run_claude
    let result1 = agent_loop.run_claude().await;
    assert!(result1.is_ok(), "First turn should succeed: {:?}", result1);

    // Extract response and add to history
    let response1 = result1.unwrap().final_output["response"]
        .as_str()
        .unwrap_or("")
        .to_string();
    agent_loop.add_to_history("You are a helpful assistant.", &response1);

    // Continue conversation
    let result2 = agent_loop.chat_continue("What did I just say?").await;
    assert!(
        result2.is_ok(),
        "chat_continue should succeed: {:?}",
        result2
    );

    // History should now have 4 messages (2 turns × 2 messages each)
    assert_eq!(
        agent_loop.history_len(),
        4,
        "Should have 4 messages after 2 turns"
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY environment variable"]
async fn test_chat_continue_claude_multi_turn() {
    let mut agent_loop = create_agent_loop("Answer briefly.");

    // First turn
    let r1 = agent_loop.run_claude().await.unwrap();
    let resp1 = r1.final_output["response"].as_str().unwrap_or("");
    agent_loop.add_to_history("Answer briefly.", resp1);

    // Second turn
    let r2 = agent_loop.chat_continue("What is 2+2?").await.unwrap();
    let resp2 = r2.final_output["response"].as_str().unwrap_or("");
    assert!(
        resp2.contains('4') || resp2.to_lowercase().contains("four"),
        "Should answer math question: {resp2}"
    );

    // Third turn
    let r3 = agent_loop.chat_continue("And 3+3?").await.unwrap();
    let resp3 = r3.final_output["response"].as_str().unwrap_or("");
    assert!(
        resp3.contains('6') || resp3.to_lowercase().contains("six"),
        "Should answer follow-up: {resp3}"
    );

    // History should have all turns
    assert!(
        agent_loop.history_len() >= 6,
        "Should have at least 6 messages"
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY environment variable"]
async fn test_chat_continue_emits_events() {
    let params = AgentParams {
        prompt: "Test events.".to_string(),
        mcp: vec![],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let mut agent_loop = RigAgentLoop::new(
        "test_events".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    // Run initial turn
    let _ = agent_loop.run_claude().await;

    // Clear events from first turn
    let initial_events = event_log.events().len();

    // Continue conversation
    let _ = agent_loop.chat_continue("Follow-up").await;

    // Should have new events
    let total_events = event_log.events().len();
    assert!(
        total_events > initial_events,
        "Should emit events for chat_continue"
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY environment variable"]
async fn test_chat_continue_respects_stop_conditions() {
    let params = AgentParams {
        prompt: "Say DONE when asked to stop.".to_string(),
        mcp: vec![],
        max_turns: Some(5),
        stop_conditions: vec!["DONE".to_string()],
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let mut agent_loop =
        RigAgentLoop::new("test_stop".to_string(), params, event_log, mcp_clients).unwrap();

    // First turn
    let r1 = agent_loop.run_claude().await.unwrap();
    let resp1 = r1.final_output["response"].as_str().unwrap_or("");
    agent_loop.add_to_history("Say DONE when asked to stop.", resp1);

    // Ask to stop
    let r2 = agent_loop.chat_continue("Please stop now.").await.unwrap();

    // If response contains DONE, status should be StopConditionMet
    let resp2 = r2.final_output["response"].as_str().unwrap_or("");
    if resp2.contains("DONE") {
        assert_eq!(
            r2.status,
            RigAgentStatus::StopConditionMet,
            "Should detect stop condition"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Tests - chat_continue with OpenAI (require OPENAI_API_KEY)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY environment variable"]
async fn test_chat_continue_openai_multi_turn() {
    let mut agent_loop = create_agent_loop("Answer briefly.");

    // First turn
    let r1 = agent_loop.run_openai().await.unwrap();
    let resp1 = r1.final_output["response"].as_str().unwrap_or("");
    agent_loop.add_to_history("Answer briefly.", resp1);

    // Second turn
    let r2 = agent_loop.chat_continue("What is 2+2?").await.unwrap();
    let resp2 = r2.final_output["response"].as_str().unwrap_or("");
    assert!(
        resp2.contains('4') || resp2.to_lowercase().contains("four"),
        "Should answer math question: {resp2}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// History Persistence Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_history_persists_across_operations() {
    let mut agent_loop = create_agent_loop("Test");

    // Add history
    agent_loop.add_to_history("Q1", "A1");

    // Push individual message
    agent_loop.push_message(Message::user("Q2"));

    // Add more history
    agent_loop.add_to_history("Q3", "A3");

    // Should have 5 messages total
    assert_eq!(
        agent_loop.history_len(),
        5,
        "All operations should accumulate"
    );
}

#[test]
fn test_history_immutable_reference() {
    let mut agent_loop = create_agent_loop("Test");
    agent_loop.add_to_history("Q", "A");

    // Get history reference
    let history = agent_loop.history();
    let len = history.len();

    // history() returns immutable reference, so this compiles
    assert_eq!(len, 2);

    // After dropping the reference, we can mutate again
    agent_loop.add_to_history("Q2", "A2");
    assert_eq!(agent_loop.history_len(), 4);
}
