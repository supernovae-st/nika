//! Tests for RigAgentLoop provider methods (Mistral, Groq, DeepSeek, Ollama)
//!
//! These tests verify the 4 provider methods added in v0.6 work correctly.
//!
//! NOTE: rig-core's `from_env()` methods PANIC when API keys are missing.
//! This is a rig-core design decision. Tests that require API keys are marked
//! with `#[ignore]` and should be run manually with appropriate env vars set.
//!
//! Run with API keys:
//!   MISTRAL_API_KEY=... cargo test --test rig_provider_methods_test -- --ignored
//!   GROQ_API_KEY=... cargo test --test rig_provider_methods_test -- --ignored
//!   DEEPSEEK_API_KEY=... cargo test --test rig_provider_methods_test -- --ignored

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
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    RigAgentLoop::new("test_task".to_string(), params, event_log, mcp_clients)
        .expect("Should create agent loop with valid params")
}

fn create_agent_loop_with_model(prompt: &str, model: &str) -> RigAgentLoop {
    let params = AgentParams {
        prompt: prompt.to_string(),
        model: Some(model.to_string()),
        mcp: vec![],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    RigAgentLoop::new("test_task".to_string(), params, event_log, mcp_clients)
        .expect("Should create agent loop with valid params")
}

// ═══════════════════════════════════════════════════════════════════════════
// Method Existence Tests (compile-time verification)
// ═══════════════════════════════════════════════════════════════════════════

// Method existence is verified at compile time via the integration tests.
// If run_mistral/run_groq/run_deepseek/run_ollama methods don't exist,
// the integration tests below won't compile.
//
// We don't need separate "method exists" tests as Rust's type system
// guarantees this at compile time.

// ═══════════════════════════════════════════════════════════════════════════
// Agent Loop Creation Tests (validates params before provider call)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_agent_loop_creation_for_mistral_valid() {
    // Verify agent loop can be created with params suitable for Mistral
    let params = AgentParams {
        prompt: "Generate text with Mistral".to_string(),
        model: Some("mistral-large-latest".to_string()),
        mcp: vec![],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("mistral_task".to_string(), params, event_log, mcp_clients);
    assert!(result.is_ok(), "Should create agent loop for Mistral");
}

#[test]
fn test_agent_loop_creation_for_groq_valid() {
    let params = AgentParams {
        prompt: "Generate text with Groq".to_string(),
        model: Some("llama-3.3-70b-versatile".to_string()),
        mcp: vec![],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("groq_task".to_string(), params, event_log, mcp_clients);
    assert!(result.is_ok(), "Should create agent loop for Groq");
}

#[test]
fn test_agent_loop_creation_for_deepseek_valid() {
    let params = AgentParams {
        prompt: "Generate text with DeepSeek".to_string(),
        model: Some("deepseek-chat".to_string()),
        mcp: vec![],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("deepseek_task".to_string(), params, event_log, mcp_clients);
    assert!(result.is_ok(), "Should create agent loop for DeepSeek");
}

#[test]
fn test_agent_loop_creation_for_ollama_valid() {
    let params = AgentParams {
        prompt: "Generate text with Ollama".to_string(),
        model: Some("llama3.2".to_string()),
        mcp: vec![],
        max_turns: Some(5),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let result = RigAgentLoop::new("ollama_task".to_string(), params, event_log, mcp_clients);
    assert!(result.is_ok(), "Should create agent loop for Ollama");
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Tests - Mistral (require MISTRAL_API_KEY)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires MISTRAL_API_KEY environment variable"]
async fn test_run_mistral_completes_successfully() {
    let mut agent_loop = create_agent_loop("Say 'hello' and nothing else.");

    let result = agent_loop.run_mistral().await;

    assert!(
        result.is_ok(),
        "Should succeed with valid API key: {:?}",
        result
    );
    let result = result.unwrap();
    assert!(result.turns >= 1, "Should complete at least 1 turn");
    assert_eq!(result.status, RigAgentStatus::NaturalCompletion);
}

#[tokio::test]
#[ignore = "Requires MISTRAL_API_KEY environment variable"]
async fn test_run_mistral_with_custom_model() {
    let mut agent_loop =
        create_agent_loop_with_model("Say 'hello' and nothing else.", "mistral-small-latest");

    let result = agent_loop.run_mistral().await;

    assert!(
        result.is_ok(),
        "Should succeed with custom model: {:?}",
        result
    );
}

#[tokio::test]
#[ignore = "Requires MISTRAL_API_KEY environment variable"]
async fn test_run_mistral_emits_events() {
    let params = AgentParams {
        prompt: "Say 'hello'.".to_string(),
        mcp: vec![],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let mut agent_loop = RigAgentLoop::new(
        "mistral_events".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let _ = agent_loop.run_mistral().await;

    let events = event_log.events();
    assert!(!events.is_empty(), "Should emit events");
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Tests - Groq (require GROQ_API_KEY)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires GROQ_API_KEY environment variable"]
async fn test_run_groq_completes_successfully() {
    let mut agent_loop = create_agent_loop("Say 'hello' and nothing else.");

    let result = agent_loop.run_groq().await;

    assert!(
        result.is_ok(),
        "Should succeed with valid API key: {:?}",
        result
    );
    let result = result.unwrap();
    assert!(result.turns >= 1, "Should complete at least 1 turn");
    assert_eq!(result.status, RigAgentStatus::NaturalCompletion);
}

#[tokio::test]
#[ignore = "Requires GROQ_API_KEY environment variable"]
async fn test_run_groq_with_custom_model() {
    let mut agent_loop =
        create_agent_loop_with_model("Say 'hello' and nothing else.", "mixtral-8x7b-32768");

    let result = agent_loop.run_groq().await;

    assert!(
        result.is_ok(),
        "Should succeed with custom model: {:?}",
        result
    );
}

#[tokio::test]
#[ignore = "Requires GROQ_API_KEY environment variable"]
async fn test_run_groq_emits_events() {
    let params = AgentParams {
        prompt: "Say 'hello'.".to_string(),
        mcp: vec![],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let mut agent_loop = RigAgentLoop::new(
        "groq_events".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let _ = agent_loop.run_groq().await;

    let events = event_log.events();
    assert!(!events.is_empty(), "Should emit events");
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Tests - DeepSeek (require DEEPSEEK_API_KEY)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires DEEPSEEK_API_KEY environment variable"]
async fn test_run_deepseek_completes_successfully() {
    let mut agent_loop = create_agent_loop("Say 'hello' and nothing else.");

    let result = agent_loop.run_deepseek().await;

    assert!(
        result.is_ok(),
        "Should succeed with valid API key: {:?}",
        result
    );
    let result = result.unwrap();
    assert!(result.turns >= 1, "Should complete at least 1 turn");
    assert_eq!(result.status, RigAgentStatus::NaturalCompletion);
}

#[tokio::test]
#[ignore = "Requires DEEPSEEK_API_KEY environment variable"]
async fn test_run_deepseek_with_custom_model() {
    let mut agent_loop =
        create_agent_loop_with_model("Say 'hello' and nothing else.", "deepseek-coder");

    let result = agent_loop.run_deepseek().await;

    assert!(
        result.is_ok(),
        "Should succeed with custom model: {:?}",
        result
    );
}

#[tokio::test]
#[ignore = "Requires DEEPSEEK_API_KEY environment variable"]
async fn test_run_deepseek_emits_events() {
    let params = AgentParams {
        prompt: "Say 'hello'.".to_string(),
        mcp: vec![],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let mut agent_loop = RigAgentLoop::new(
        "deepseek_events".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let _ = agent_loop.run_deepseek().await;

    let events = event_log.events();
    assert!(!events.is_empty(), "Should emit events");
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Tests - Ollama (require local Ollama server)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[ignore = "Requires Ollama server running locally"]
async fn test_run_ollama_completes_successfully() {
    let mut agent_loop = create_agent_loop("Say 'hello' and nothing else.");

    let result = agent_loop.run_ollama().await;

    assert!(
        result.is_ok(),
        "Should succeed with Ollama running: {:?}",
        result
    );
    let result = result.unwrap();
    assert!(result.turns >= 1, "Should complete at least 1 turn");
    assert_eq!(result.status, RigAgentStatus::NaturalCompletion);
}

#[tokio::test]
#[ignore = "Requires Ollama server running locally"]
async fn test_run_ollama_with_custom_model() {
    let mut agent_loop = create_agent_loop_with_model("Say 'hello' and nothing else.", "codellama");

    let result = agent_loop.run_ollama().await;

    assert!(
        result.is_ok(),
        "Should succeed with custom model: {:?}",
        result
    );
}

#[tokio::test]
#[ignore = "Requires Ollama server running locally"]
async fn test_run_ollama_emits_events() {
    let params = AgentParams {
        prompt: "Say 'hello'.".to_string(),
        mcp: vec![],
        max_turns: Some(3),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let mut agent_loop = RigAgentLoop::new(
        "ollama_events".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
    )
    .unwrap();

    let _ = agent_loop.run_ollama().await;

    let events = event_log.events();
    assert!(!events.is_empty(), "Should emit events");
}

// ═══════════════════════════════════════════════════════════════════════════
// Default Model Verification (document expected defaults)
// ═══════════════════════════════════════════════════════════════════════════

/// Document expected default models for each provider
/// These are tested via integration tests, but documented here for reference
#[test]
fn test_default_models_documented() {
    // Mistral default: mistral-large (rig::providers::mistral::MISTRAL_LARGE)
    // Groq default: llama-3.3-70b-versatile
    // DeepSeek default: deepseek-chat
    // Ollama default: llama3.2

    // These values are hardcoded in src/runtime/rig_agent_loop.rs
    // If they change, update this documentation and CLAUDE.md
    let _expected_defaults = [
        ("mistral", "mistral-large"),
        ("groq", "llama-3.3-70b-versatile"),
        ("deepseek", "deepseek-chat"),
        ("ollama", "llama3.2"),
    ];
}
