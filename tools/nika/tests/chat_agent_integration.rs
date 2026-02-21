//! End-to-end integration tests for the Chat Agent interface
//!
//! These tests verify:
//! - ChatAgent creation with OpenAI provider
//! - Command parsing for all 5 verbs + /model + /clear
//! - FileResolver @file expansion
//! - ChatAgent.exec_command() execution
//! - ChatAgent.infer() (requires API key - marked #[ignore])
//! - Full flow: parse command -> resolve files -> execute
//!
//! Run all tests: `cargo test --test chat_agent_integration --features tui`
//! Run ignored tests: `cargo test --test chat_agent_integration --features tui -- --ignored`

#![cfg(feature = "tui")]

use nika::tui::chat_agent::{ChatAgent, ChatMessage, ChatRole, StreamingState};
use nika::tui::command::{Command, ModelProvider, HELP_TEXT};
use nika::tui::file_resolve::FileResolver;
use std::fs;
use tempfile::TempDir;
use tokio::sync::mpsc;

// ═══════════════════════════════════════════════════════════════════════════
// CHAT AGENT CREATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

/// Test that ChatAgent can be created successfully
#[tokio::test]
async fn test_chat_agent_creation_succeeds() {
    // ChatAgent::new() should always succeed, even without API keys
    // (errors happen on actual API calls, not on creation)
    let result = ChatAgent::new();
    assert!(result.is_ok(), "ChatAgent creation should succeed");
}

/// Test ChatAgent auto-detection picks valid provider
#[tokio::test]
async fn test_chat_agent_auto_detection() {
    // Set a key to ensure at least one provider is available
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");

    // RigProvider::auto() picks first available provider in priority order:
    // 1. Claude, 2. OpenAI, 3. Mistral, 4. Groq, 5. DeepSeek, 6. Ollama
    // Due to parallel tests and user env, any provider may be selected
    let valid_providers = ["claude", "openai", "mistral", "groq", "deepseek", "ollama"];
    assert!(
        valid_providers.contains(&agent.provider_name()),
        "Expected valid provider, got: {}",
        agent.provider_name()
    );
    assert!(agent.history().is_empty());
    assert!(!agent.is_streaming());
}

/// Test ChatAgent with Claude fallback
#[tokio::test]
async fn test_chat_agent_claude_fallback() {
    // Set both keys to verify creation works
    std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new();
    assert!(agent.is_ok());
}

/// Test provider switching
#[tokio::test]
async fn test_chat_agent_provider_switching() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");
    std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-integration-test");

    let mut agent = ChatAgent::new().expect("Should create agent");

    // Switch to Claude
    let result = agent.set_provider(ModelProvider::Claude);
    assert!(result.is_ok());
    assert_eq!(agent.provider_name(), "claude");

    // Switch back to OpenAI
    let result = agent.set_provider(ModelProvider::OpenAI);
    assert!(result.is_ok());
    assert_eq!(agent.provider_name(), "openai");

    // List doesn't change provider
    let prev_provider = agent.provider_name();
    let result = agent.set_provider(ModelProvider::List);
    assert!(result.is_ok());
    assert_eq!(agent.provider_name(), prev_provider);
}

// ═══════════════════════════════════════════════════════════════════════════
// COMMAND PARSING TESTS - ALL 5 VERBS
// ═══════════════════════════════════════════════════════════════════════════

/// Test parsing /infer command
#[test]
fn test_command_parse_infer() {
    let cmd = Command::parse("/infer explain this code");
    assert!(matches!(cmd, Command::Infer { prompt } if prompt == "explain this code"));

    // Empty prompt
    let cmd = Command::parse("/infer");
    assert!(matches!(cmd, Command::Infer { prompt } if prompt.is_empty()));

    // Case insensitive
    let cmd = Command::parse("/INFER test");
    assert!(matches!(cmd, Command::Infer { prompt } if prompt == "test"));
}

/// Test parsing /exec command
#[test]
fn test_command_parse_exec() {
    let cmd = Command::parse("/exec cargo test");
    assert!(matches!(cmd, Command::Exec { command } if command == "cargo test"));

    // With pipes
    let cmd = Command::parse("/exec ls -la | grep rs");
    assert!(matches!(cmd, Command::Exec { command } if command == "ls -la | grep rs"));

    // Case insensitive
    let cmd = Command::parse("/EXEC echo hello");
    assert!(matches!(cmd, Command::Exec { command } if command == "echo hello"));
}

/// Test parsing /fetch command
#[test]
fn test_command_parse_fetch() {
    // Default method (GET)
    let cmd = Command::parse("/fetch https://example.com");
    assert!(
        matches!(cmd, Command::Fetch { url, method } if url == "https://example.com" && method == "GET")
    );

    // Explicit POST
    let cmd = Command::parse("/fetch https://api.example.com POST");
    assert!(
        matches!(cmd, Command::Fetch { url, method } if url == "https://api.example.com" && method == "POST")
    );

    // Case insensitive method
    let cmd = Command::parse("/fetch https://api.example.com delete");
    assert!(matches!(cmd, Command::Fetch { url, method } if method == "DELETE"));
}

/// Test parsing /invoke command
#[test]
fn test_command_parse_invoke() {
    // Simple tool name
    let cmd = Command::parse("/invoke describe");
    assert!(
        matches!(cmd, Command::Invoke { tool, server, .. } if tool == "describe" && server.is_none())
    );

    // With server
    let cmd = Command::parse("/invoke novanet:describe");
    assert!(
        matches!(cmd, Command::Invoke { tool, server, .. } if tool == "describe" && server == Some("novanet".to_string()))
    );

    // With JSON params
    let cmd = Command::parse(r#"/invoke novanet:describe {"entity":"qr-code"}"#);
    if let Command::Invoke {
        tool,
        server,
        params,
    } = cmd
    {
        assert_eq!(tool, "describe");
        assert_eq!(server, Some("novanet".to_string()));
        assert_eq!(params["entity"], "qr-code");
    } else {
        panic!("Expected Command::Invoke");
    }
}

/// Test parsing /agent command
#[test]
fn test_command_parse_agent() {
    // Simple goal
    let cmd = Command::parse("/agent generate a landing page");
    assert!(
        matches!(cmd, Command::Agent { goal, max_turns, mcp_servers: _ } if goal == "generate a landing page" && max_turns.is_none())
    );

    // With max turns
    let cmd = Command::parse("/agent generate content --max-turns 5");
    assert!(
        matches!(cmd, Command::Agent { goal, max_turns, mcp_servers: _ } if goal == "generate content" && max_turns == Some(5))
    );
}

/// Test parsing /model command
#[test]
fn test_command_parse_model() {
    // OpenAI
    let cmd = Command::parse("/model openai");
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::OpenAI
        }
    ));

    // Claude
    let cmd = Command::parse("/model claude");
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::Claude
        }
    ));

    // Aliases
    let cmd = Command::parse("/model gpt");
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::OpenAI
        }
    ));

    let cmd = Command::parse("/model anthropic");
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::Claude
        }
    ));

    // List
    let cmd = Command::parse("/model list");
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::List
        }
    ));

    // Empty defaults to list
    let cmd = Command::parse("/model");
    assert!(matches!(
        cmd,
        Command::Model {
            provider: ModelProvider::List
        }
    ));
}

/// Test parsing /clear command
#[test]
fn test_command_parse_clear() {
    let cmd = Command::parse("/clear");
    assert!(matches!(cmd, Command::Clear));
}

/// Test parsing /help command
#[test]
fn test_command_parse_help() {
    let cmd = Command::parse("/help");
    assert!(matches!(cmd, Command::Help));

    let cmd = Command::parse("/?");
    assert!(matches!(cmd, Command::Help));
}

/// Test parsing plain chat messages
#[test]
fn test_command_parse_chat() {
    let cmd = Command::parse("hello world");
    assert!(matches!(cmd, Command::Chat { message } if message == "hello world"));

    // Empty message
    let cmd = Command::parse("");
    assert!(matches!(cmd, Command::Chat { message } if message.is_empty()));

    // Whitespace
    let cmd = Command::parse("   ");
    assert!(matches!(cmd, Command::Chat { message } if message.is_empty()));

    // Unknown command becomes chat
    let cmd = Command::parse("/unknown something");
    assert!(matches!(cmd, Command::Chat { message } if message == "/unknown something"));
}

/// Test Command helper methods
#[test]
fn test_command_helper_methods() {
    // verb() method
    assert_eq!(Command::Infer { prompt: "x".into() }.verb(), "infer");
    assert_eq!(
        Command::Exec {
            command: "x".into()
        }
        .verb(),
        "exec"
    );
    assert_eq!(
        Command::Fetch {
            url: "x".into(),
            method: "GET".into()
        }
        .verb(),
        "fetch"
    );
    assert_eq!(
        Command::Invoke {
            tool: "x".into(),
            server: None,
            params: serde_json::json!({})
        }
        .verb(),
        "invoke"
    );
    assert_eq!(
        Command::Agent {
            goal: "x".into(),
            max_turns: None,
            mcp_servers: vec![],
        }
        .verb(),
        "agent"
    );
    assert_eq!(
        Command::Chat {
            message: "x".into()
        }
        .verb(),
        "chat"
    );
    assert_eq!(Command::Help.verb(), "help");
    assert_eq!(
        Command::Model {
            provider: ModelProvider::OpenAI
        }
        .verb(),
        "model"
    );
    assert_eq!(Command::Clear.verb(), "clear");

    // is_empty() method
    assert!(Command::Chat { message: "".into() }.is_empty());
    assert!(!Command::Chat {
        message: "hi".into()
    }
    .is_empty());
    assert!(Command::Infer { prompt: "".into() }.is_empty());
    assert!(!Command::Help.is_empty());
    assert!(!Command::Clear.is_empty());
}

/// Test HELP_TEXT exists and contains expected commands
#[test]
fn test_help_text_content() {
    assert!(HELP_TEXT.contains("/infer"));
    assert!(HELP_TEXT.contains("/exec"));
    assert!(HELP_TEXT.contains("/fetch"));
    assert!(HELP_TEXT.contains("/invoke"));
    assert!(HELP_TEXT.contains("/agent"));
    assert!(HELP_TEXT.contains("/model"));
    assert!(HELP_TEXT.contains("/clear"));
    assert!(HELP_TEXT.contains("/help"));
}

// ═══════════════════════════════════════════════════════════════════════════
// FILE RESOLVER TESTS
// ═══════════════════════════════════════════════════════════════════════════

/// Test extracting @file mentions
#[test]
fn test_file_resolver_extract_mentions() {
    // Single file
    let mentions = FileResolver::extract_mentions("Explain @src/main.rs");
    assert_eq!(mentions, vec!["src/main.rs"]);

    // Multiple files
    let mentions = FileResolver::extract_mentions("Compare @a.rs and @b.rs");
    assert_eq!(mentions, vec!["a.rs", "b.rs"]);

    // Nested path
    let mentions = FileResolver::extract_mentions("Check @path/to/file.yaml");
    assert_eq!(mentions, vec!["path/to/file.yaml"]);

    // No mentions
    let mentions = FileResolver::extract_mentions("Just text");
    assert!(mentions.is_empty());
}

/// Test that emails are NOT treated as file mentions
#[test]
fn test_file_resolver_excludes_emails() {
    let mentions = FileResolver::extract_mentions("Contact user@example.com");
    assert!(mentions.is_empty());

    let mentions = FileResolver::extract_mentions("Email support@company.io for help");
    assert!(mentions.is_empty());

    // But file mentions are still captured alongside emails
    let mentions = FileResolver::extract_mentions("See @file.rs for user@example.com");
    assert_eq!(mentions, vec!["file.rs"]);
}

/// Test file resolution with real files
#[test]
fn test_file_resolver_resolve_existing_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "Hello, World!").unwrap();

    let input = "Explain @test.txt";
    let resolved = FileResolver::resolve(input, temp_dir.path());

    assert!(resolved.contains("<file path=\"test.txt\">"));
    assert!(resolved.contains("Hello, World!"));
    assert!(resolved.contains("</file>"));
    assert!(!resolved.contains("@test.txt"));
}

/// Test file resolution with missing files
#[test]
fn test_file_resolver_resolve_missing_file() {
    let temp_dir = TempDir::new().unwrap();

    let input = "Explain @missing.txt";
    let resolved = FileResolver::resolve(input, temp_dir.path());

    // Missing file should be left as-is
    assert_eq!(resolved, "Explain @missing.txt");
}

/// Test file resolution with nested directories
#[test]
fn test_file_resolver_resolve_nested_path() {
    let temp_dir = TempDir::new().unwrap();
    let nested = temp_dir.path().join("src/nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("file.rs"), "// nested content").unwrap();

    let input = "Check @src/nested/file.rs";
    let resolved = FileResolver::resolve(input, temp_dir.path());

    assert!(resolved.contains("<file path=\"src/nested/file.rs\">"));
    assert!(resolved.contains("// nested content"));
}

/// Test file resolution preserves emails
#[test]
fn test_file_resolver_preserves_emails() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("code.rs"), "fn main() {}").unwrap();

    let input = "Here is @code.rs for user@example.com";
    let resolved = FileResolver::resolve(input, temp_dir.path());

    // File should be resolved
    assert!(resolved.contains("<file path=\"code.rs\">"));
    // Email should be preserved
    assert!(resolved.contains("user@example.com"));
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT AGENT EXEC COMMAND TESTS (NO API KEY REQUIRED)
// ═══════════════════════════════════════════════════════════════════════════

/// Test exec_command with simple echo
#[tokio::test]
async fn test_chat_agent_exec_echo() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent.exec_command("echo hello").await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "hello");
}

/// Test exec_command with multiple arguments
#[tokio::test]
async fn test_chat_agent_exec_with_args() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent.exec_command("echo -n 'test output'").await;

    assert!(result.is_ok());
    assert!(result.unwrap().contains("test output"));
}

/// Test exec_command with pipe
#[tokio::test]
async fn test_chat_agent_exec_pipe() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent
        .exec_command("echo 'hello world' | tr 'a-z' 'A-Z'")
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "HELLO WORLD");
}

/// Test exec_command with failed command
#[tokio::test]
async fn test_chat_agent_exec_failure() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent.exec_command("exit 1").await;

    // Command failure returns Ok with exit code info
    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.contains("Exit code: 1"));
}

/// Test exec_command with environment variable
#[tokio::test]
async fn test_chat_agent_exec_env_var() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent.exec_command("echo $HOME").await;

    assert!(result.is_ok());
    // Should output the home directory path
    let output = result.unwrap();
    assert!(!output.is_empty());
    assert!(output.starts_with('/'));
}

/// Test exec_command with working directory command
#[tokio::test]
async fn test_chat_agent_exec_pwd() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent.exec_command("pwd").await;

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(output.starts_with('/'));
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT MESSAGE AND HISTORY TESTS
// ═══════════════════════════════════════════════════════════════════════════

/// Test ChatMessage creation
#[test]
fn test_chat_message_creation() {
    let user_msg = ChatMessage::user("Hello");
    assert_eq!(user_msg.role, ChatRole::User);
    assert_eq!(user_msg.content, "Hello");

    let assistant_msg = ChatMessage::assistant("Hi there!");
    assert_eq!(assistant_msg.role, ChatRole::Assistant);
    assert_eq!(assistant_msg.content, "Hi there!");

    let system_msg = ChatMessage::system("You are helpful.");
    assert_eq!(system_msg.role, ChatRole::System);

    let tool_msg = ChatMessage::tool(r#"{"result": "ok"}"#);
    assert_eq!(tool_msg.role, ChatRole::Tool);
}

/// Test ChatRole display names
#[test]
fn test_chat_role_display_names() {
    assert_eq!(ChatRole::User.display_name(), "You");
    assert_eq!(ChatRole::Assistant.display_name(), "Nika");
    assert_eq!(ChatRole::System.display_name(), "System");
    assert_eq!(ChatRole::Tool.display_name(), "Tool");
}

/// Test history management
#[tokio::test]
async fn test_chat_agent_history_management() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let mut agent = ChatAgent::new().expect("Should create agent");

    // History starts empty
    assert!(agent.history().is_empty());

    // Execute a command (which doesn't add to history)
    let _ = agent.exec_command("echo test").await;
    assert!(agent.history().is_empty());

    // Clear history (should work even when empty)
    agent.clear_history();
    assert!(agent.history().is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// STREAMING STATE TESTS
// ═══════════════════════════════════════════════════════════════════════════

/// Test StreamingState default values
#[test]
fn test_streaming_state_default() {
    let state = StreamingState::default();
    assert!(!state.is_streaming);
    assert!(state.partial_response.is_empty());
    assert_eq!(state.tokens_received, 0);
}

/// Test StreamingState lifecycle
#[test]
fn test_streaming_state_lifecycle() {
    let mut state = StreamingState::default();

    // Start streaming
    state.start();
    assert!(state.is_streaming);
    assert!(state.partial_response.is_empty());

    // Append chunks
    state.append("Hello");
    state.append(", ");
    state.append("world!");
    assert_eq!(state.partial_response, "Hello, world!");
    assert_eq!(state.tokens_received, 3);

    // Finish streaming
    let result = state.finish();
    assert_eq!(result, "Hello, world!");
    assert!(!state.is_streaming);
    assert!(state.partial_response.is_empty());
}

/// Test StreamingState with agent
#[tokio::test]
async fn test_chat_agent_streaming_state() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");

    assert!(!agent.is_streaming());
    assert!(!agent.streaming_state().is_streaming);
    assert!(agent.streaming_state().partial_response.is_empty());
}

/// Test ChatAgent with streaming channel
#[tokio::test]
async fn test_chat_agent_with_streaming_channel() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let (tx, _rx) = mpsc::channel::<String>(10);
    let _agent = ChatAgent::new()
        .expect("Should create agent")
        .with_streaming(tx);

    // Agent created with streaming channel
    // The streaming_tx field is private, so we just verify creation succeeds
}

// ═══════════════════════════════════════════════════════════════════════════
// MODEL PROVIDER TESTS
// ═══════════════════════════════════════════════════════════════════════════

/// Test ModelProvider properties
#[test]
fn test_model_provider_properties() {
    // Names
    assert_eq!(ModelProvider::OpenAI.name(), "OpenAI (gpt-4o)");
    assert_eq!(
        ModelProvider::Claude.name(),
        "Anthropic Claude (claude-sonnet-4)"
    );
    assert_eq!(ModelProvider::List.name(), "list");

    // Environment variables
    assert_eq!(ModelProvider::OpenAI.env_var(), "OPENAI_API_KEY");
    assert_eq!(ModelProvider::Claude.env_var(), "ANTHROPIC_API_KEY");
    assert_eq!(ModelProvider::List.env_var(), "");
}

/// Test ModelProvider availability
#[test]
fn test_model_provider_availability() {
    // List is always available
    assert!(ModelProvider::List.is_available());

    // OpenAI availability depends on env var
    std::env::set_var("OPENAI_API_KEY", "test-key");
    assert!(ModelProvider::OpenAI.is_available());

    std::env::set_var("ANTHROPIC_API_KEY", "test-key");
    assert!(ModelProvider::Claude.is_available());
}

// ═══════════════════════════════════════════════════════════════════════════
// FULL FLOW INTEGRATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

/// Test full flow: parse command -> resolve files -> execute (exec)
#[tokio::test]
async fn test_full_flow_exec() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let input = "/exec echo 'integration test'";

    // 1. Parse command
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Exec { .. }));

    // 2. Execute
    if let Command::Exec { command } = cmd {
        let agent = ChatAgent::new().expect("Should create agent");
        let result = agent.exec_command(&command).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("integration test"));
    }
}

/// Test full flow: parse command -> resolve files -> execute (with file mention)
#[tokio::test]
async fn test_full_flow_with_file_mention() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("code.rs"), "fn main() {}").unwrap();

    let input = "Explain @code.rs";

    // 1. Parse command (plain chat)
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Chat { .. }));

    // 2. Resolve file mentions
    if let Command::Chat { message } = cmd {
        let resolved = FileResolver::resolve(&message, temp_dir.path());

        // Verify file was resolved
        assert!(resolved.contains("<file path=\"code.rs\">"));
        assert!(resolved.contains("fn main() {}"));
    }
}

/// Test full flow: help command
#[test]
fn test_full_flow_help() {
    let input = "/help";

    // Parse command
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Help));

    // Help would display HELP_TEXT
    assert!(!HELP_TEXT.is_empty());
}

/// Test full flow: clear command
#[tokio::test]
async fn test_full_flow_clear() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let input = "/clear";

    // Parse command
    let cmd = Command::parse(input);
    assert!(matches!(cmd, Command::Clear));

    // Execute clear
    let mut agent = ChatAgent::new().expect("Should create agent");
    agent.clear_history();
    assert!(agent.history().is_empty());
}

/// Test full flow: model switch
#[tokio::test]
async fn test_full_flow_model_switch() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");
    std::env::set_var("ANTHROPIC_API_KEY", "test-key-for-integration-test");

    let input = "/model claude";

    // Parse command
    let cmd = Command::parse(input);

    if let Command::Model { provider } = cmd {
        let mut agent = ChatAgent::new().expect("Should create agent");
        let result = agent.set_provider(provider);
        assert!(result.is_ok());
        assert_eq!(agent.provider_name(), "claude");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EDGE CASES AND ERROR HANDLING
// ═══════════════════════════════════════════════════════════════════════════

/// Test command parsing edge cases
#[test]
fn test_command_parse_edge_cases() {
    // Multiple spaces
    let cmd = Command::parse("/infer    multiple   spaces");
    assert!(matches!(cmd, Command::Infer { prompt } if prompt == "multiple   spaces"));

    // Newlines in prompt
    let cmd = Command::parse("/infer line1\nline2");
    assert!(matches!(cmd, Command::Infer { prompt } if prompt.contains('\n')));

    // Special characters
    let cmd = Command::parse("/exec echo '!@#$%^&*()'");
    assert!(matches!(cmd, Command::Exec { command } if command.contains("!@#$%^&*()")));

    // Unicode
    let cmd = Command::parse("/infer explain 日本語");
    assert!(matches!(cmd, Command::Infer { prompt } if prompt.contains("日本語")));
}

/// Test file resolver edge cases
#[test]
fn test_file_resolver_edge_cases() {
    // Multiple @ in path (should only match the file mention)
    let mentions = FileResolver::extract_mentions("Check @path/file.rs at user@domain.com");
    assert_eq!(mentions, vec!["path/file.rs"]);

    // @ at end of line with file
    let mentions = FileResolver::extract_mentions("See @README.md");
    assert_eq!(mentions, vec!["README.md"]);

    // @ in parentheses
    let mentions = FileResolver::extract_mentions("See (@file.rs) for details");
    assert_eq!(mentions, vec!["file.rs"]);

    // Hyphenated filename
    let mentions = FileResolver::extract_mentions("Check @my-config-file.json");
    assert_eq!(mentions, vec!["my-config-file.json"]);
}

// ═══════════════════════════════════════════════════════════════════════════
// API-DEPENDENT TESTS (REQUIRE REAL API KEY)
// ═══════════════════════════════════════════════════════════════════════════

/// Test infer() call - requires OPENAI_API_KEY
#[tokio::test]
#[ignore = "requires OPENAI_API_KEY environment variable with valid key"]
async fn test_chat_agent_infer() {
    let mut agent = ChatAgent::new().expect("Should create agent");

    // Make a simple inference call
    let result = agent.infer("Say hello in one word").await;

    assert!(result.is_ok(), "Infer should succeed: {:?}", result.err());

    let response = result.unwrap();
    assert!(!response.is_empty(), "Response should not be empty");

    // History should be updated
    assert_eq!(agent.history().len(), 2); // User + Assistant
    assert_eq!(agent.history()[0].role, ChatRole::User);
    assert_eq!(agent.history()[1].role, ChatRole::Assistant);
}

/// Test full agent flow with real API - requires OPENAI_API_KEY
#[tokio::test]
#[ignore = "requires OPENAI_API_KEY environment variable with valid key"]
async fn test_full_agent_flow_with_api() {
    let mut agent = ChatAgent::new().expect("Should create agent");

    // Test multiple interactions
    let result1 = agent
        .infer("What is 2+2? Answer with just the number.")
        .await;
    assert!(result1.is_ok());
    let response1 = result1.unwrap();
    assert!(response1.contains('4') || response1.contains("four"));

    // Second interaction
    let result2 = agent
        .infer("What was my previous question about? One sentence.")
        .await;
    assert!(result2.is_ok());

    // History should have 4 messages (2 user + 2 assistant)
    assert_eq!(agent.history().len(), 4);

    // Clear and verify
    agent.clear_history();
    assert!(agent.history().is_empty());
}

/// Test fetch with real endpoint
#[tokio::test]
async fn test_chat_agent_fetch_real_endpoint() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");

    // Fetch from httpbin (a test HTTP service)
    let result = agent.fetch("https://httpbin.org/get", "GET").await;

    // This might fail if the network is unavailable, but should work in most cases
    if let Ok(response) = result {
        assert!(response.contains("httpbin") || response.contains("headers"));
    }
}

/// Test fetch with POST method
#[tokio::test]
#[ignore = "requires network access to httpbin.org"]
async fn test_chat_agent_fetch_post() {
    std::env::set_var("OPENAI_API_KEY", "test-key-for-integration-test");

    let agent = ChatAgent::new().expect("Should create agent");

    let result = agent.fetch("https://httpbin.org/post", "POST").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(response.contains("httpbin") || response.contains("origin"));
}

// ═══════════════════════════════════════════════════════════════════════════
// COMMAND PARSING COMPREHENSIVE TESTS
// ═══════════════════════════════════════════════════════════════════════════

/// Test all command verbs comprehensively
#[test]
fn test_all_command_verbs_comprehensive() {
    // All 5 Nika verbs
    assert_eq!(Command::parse("/infer test").verb(), "infer", "infer verb");
    assert_eq!(Command::parse("/exec test").verb(), "exec", "exec verb");
    assert_eq!(
        Command::parse("/fetch http://x.com").verb(),
        "fetch",
        "fetch verb"
    );
    assert_eq!(
        Command::parse("/invoke tool").verb(),
        "invoke",
        "invoke verb"
    );
    assert_eq!(Command::parse("/agent goal").verb(), "agent", "agent verb");

    // Additional commands
    assert_eq!(
        Command::parse("/model openai").verb(),
        "model",
        "model verb"
    );
    assert_eq!(Command::parse("/clear").verb(), "clear", "clear verb");
    assert_eq!(Command::parse("/help").verb(), "help", "help verb");
    assert_eq!(Command::parse("chat message").verb(), "chat", "chat verb");
}
