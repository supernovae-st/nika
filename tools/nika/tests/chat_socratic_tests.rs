//! Socratic Tests for Chat Agent Interface
//!
//! These tests use the Socratic method - asking questions about what SHOULD happen
//! and verifying that behavior. Each test poses a question and validates the answer.
//!
//! # Test Categories
//!
//! 1. **Conversation Flow** - How do multi-turn conversations work?
//! 2. **Command Parsing** - How are user inputs interpreted?
//! 3. **File Mentions** - How are @file references handled?
//! 4. **Security** - What happens with malicious inputs?
//! 5. **UX Consistency** - Is the interface intuitive?
//! 6. **Edge Cases** - What happens at the boundaries?

use nika::tui::chat_agent::{ChatAgent, ChatRole, StreamingState};
use nika::tui::command::{Command, ModelProvider, HELP_TEXT};
use nika::tui::file_resolve::FileResolver;
use std::fs;
use tempfile::TempDir;

// ═══════════════════════════════════════════════════════════════════════════════
// CATEGORY 1: Conversation Flow
// "How do multi-turn conversations work?"
// ═══════════════════════════════════════════════════════════════════════════════

/// Q: When a user sends a message, is it added to history?
#[test]
fn socratic_message_added_to_history() {
    // Hypothesis: User messages should be tracked in conversation history
    let mut agent = ChatAgent::new().expect("Agent creation should succeed");

    // The user sends a message
    // Note: We can't actually call infer without an API key, but we can verify structure
    assert_eq!(agent.history().len(), 0, "History should start empty");

    // Clear should work on empty history
    agent.clear_history();
    assert_eq!(agent.history().len(), 0, "Clear on empty should not panic");
}

/// Q: Can the user clear conversation history?
#[test]
fn socratic_history_can_be_cleared() {
    let mut agent = ChatAgent::new().expect("Agent creation");
    agent.clear_history();
    assert_eq!(agent.history().len(), 0);
}

/// Q: Does streaming state track partial responses correctly?
#[test]
fn socratic_streaming_tracks_partial_responses() {
    let mut state = StreamingState::default();

    // Initially not streaming
    assert!(!state.is_streaming, "Should not be streaming initially");
    assert!(
        state.partial_response.is_empty(),
        "Partial response should be empty"
    );

    // Start streaming
    state.is_streaming = true;
    state.partial_response = "Hello".to_string();

    // Append more content
    state.partial_response.push_str(", world!");
    assert_eq!(state.partial_response, "Hello, world!");

    // Finish streaming
    state.is_streaming = false;
    assert!(!state.is_streaming);
}

// ═══════════════════════════════════════════════════════════════════════════════
// CATEGORY 2: Command Parsing
// "How are user inputs interpreted?"
// ═══════════════════════════════════════════════════════════════════════════════

/// Q: Does /infer correctly extract the prompt?
#[test]
fn socratic_infer_extracts_prompt() {
    let cmd = Command::parse("/infer What is 2+2?");
    match cmd {
        Command::Infer { prompt } => assert_eq!(prompt, "What is 2+2?"),
        _ => panic!("Expected Infer command"),
    }
}

/// Q: Does /exec preserve the entire command string?
#[test]
fn socratic_exec_preserves_command() {
    let cmd = Command::parse("/exec ls -la | grep .rs | wc -l");
    match cmd {
        Command::Exec { command } => assert_eq!(command, "ls -la | grep .rs | wc -l"),
        _ => panic!("Expected Exec command"),
    }
}

/// Q: Does /fetch default to GET method?
#[test]
fn socratic_fetch_defaults_to_get() {
    let cmd = Command::parse("/fetch https://api.example.com");
    match cmd {
        Command::Fetch { url, method } => {
            assert_eq!(url, "https://api.example.com");
            assert_eq!(method, "GET");
        }
        _ => panic!("Expected Fetch command"),
    }
}

/// Q: Does /fetch accept POST method?
#[test]
fn socratic_fetch_accepts_post() {
    let cmd = Command::parse("/fetch https://api.example.com POST");
    match cmd {
        Command::Fetch { method, .. } => assert_eq!(method, "POST"),
        _ => panic!("Expected Fetch command"),
    }
}

/// Q: Does /invoke parse server:tool format?
#[test]
fn socratic_invoke_parses_server_tool() {
    let cmd = Command::parse(r#"/invoke novanet:describe {"entity":"qr-code"}"#);
    match cmd {
        Command::Invoke {
            tool,
            server,
            params,
        } => {
            assert_eq!(tool, "describe");
            assert_eq!(server, Some("novanet".to_string()));
            assert_eq!(params["entity"], "qr-code");
        }
        _ => panic!("Expected Invoke command"),
    }
}

/// Q: Does /agent accept --max-turns option?
#[test]
fn socratic_agent_parses_max_turns() {
    let cmd = Command::parse("/agent Generate a landing page --max-turns 10");
    match cmd {
        Command::Agent { goal, max_turns } => {
            assert_eq!(goal, "Generate a landing page");
            assert_eq!(max_turns, Some(10));
        }
        _ => panic!("Expected Agent command"),
    }
}

/// Q: Does /model recognize provider aliases?
#[test]
fn socratic_model_recognizes_aliases() {
    let aliases = vec![
        ("openai", ModelProvider::OpenAI),
        ("gpt", ModelProvider::OpenAI),
        ("gpt-4", ModelProvider::OpenAI),
        ("claude", ModelProvider::Claude),
        ("anthropic", ModelProvider::Claude),
        ("sonnet", ModelProvider::Claude),
    ];

    for (alias, expected) in aliases {
        let cmd = Command::parse(&format!("/model {}", alias));
        match cmd {
            Command::Model { provider } => {
                assert_eq!(provider, expected, "Alias '{}' should map correctly", alias);
            }
            _ => panic!("Expected Model command for alias '{}'", alias),
        }
    }
}

/// Q: Does plain text become a Chat command?
#[test]
fn socratic_plain_text_is_chat() {
    let cmd = Command::parse("Hello, how are you?");
    match cmd {
        Command::Chat { message } => assert_eq!(message, "Hello, how are you?"),
        _ => panic!("Expected Chat command"),
    }
}

/// Q: Are unknown commands treated as chat?
#[test]
fn socratic_unknown_commands_are_chat() {
    let cmd = Command::parse("/unknown some text");
    match cmd {
        Command::Chat { message } => assert_eq!(message, "/unknown some text"),
        _ => panic!("Unknown commands should be Chat"),
    }
}

/// Q: Is /help recognized?
#[test]
fn socratic_help_is_recognized() {
    let cmd = Command::parse("/help");
    assert!(matches!(cmd, Command::Help));

    let cmd = Command::parse("/?");
    assert!(matches!(cmd, Command::Help));
}

/// Q: Is /clear recognized?
#[test]
fn socratic_clear_is_recognized() {
    let cmd = Command::parse("/clear");
    assert!(matches!(cmd, Command::Clear));
}

/// Q: Does help text contain all commands?
#[test]
fn socratic_help_text_is_complete() {
    assert!(HELP_TEXT.contains("/infer"), "Help should document /infer");
    assert!(HELP_TEXT.contains("/exec"), "Help should document /exec");
    assert!(HELP_TEXT.contains("/fetch"), "Help should document /fetch");
    assert!(
        HELP_TEXT.contains("/invoke"),
        "Help should document /invoke"
    );
    assert!(HELP_TEXT.contains("/agent"), "Help should document /agent");
    assert!(HELP_TEXT.contains("/model"), "Help should document /model");
    assert!(HELP_TEXT.contains("/clear"), "Help should document /clear");
    assert!(HELP_TEXT.contains("/help"), "Help should document /help");
}

// ═══════════════════════════════════════════════════════════════════════════════
// CATEGORY 3: File Mentions
// "How are @file references handled?"
// ═══════════════════════════════════════════════════════════════════════════════

/// Q: Are @file mentions extracted correctly?
#[test]
fn socratic_file_mentions_extracted() {
    let mentions = FileResolver::extract_mentions("Check @src/main.rs and @Cargo.toml");
    assert_eq!(mentions, vec!["src/main.rs", "Cargo.toml"]);
}

/// Q: Are emails NOT treated as file mentions?
#[test]
fn socratic_emails_not_file_mentions() {
    let mentions = FileResolver::extract_mentions("Contact user@example.com for help");
    assert!(
        mentions.is_empty(),
        "Email addresses should not be treated as file mentions"
    );
}

/// Q: Are nested paths handled?
#[test]
fn socratic_nested_paths_handled() {
    let mentions = FileResolver::extract_mentions("See @path/to/deep/nested/file.rs");
    assert_eq!(mentions, vec!["path/to/deep/nested/file.rs"]);
}

/// Q: Does file resolution inject content?
#[test]
fn socratic_file_resolution_injects_content() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("test.txt"), "Hello, World!").unwrap();

    let input = "Explain @test.txt";
    let resolved = FileResolver::resolve(input, temp_dir.path());

    assert!(resolved.contains("<file path=\"test.txt\">"));
    assert!(resolved.contains("Hello, World!"));
    assert!(resolved.contains("</file>"));
}

/// Q: Are missing files left as-is?
#[test]
fn socratic_missing_files_preserved() {
    let temp_dir = TempDir::new().unwrap();
    let input = "Check @nonexistent.txt";
    let resolved = FileResolver::resolve(input, temp_dir.path());

    assert_eq!(resolved, "Check @nonexistent.txt");
}

// ═══════════════════════════════════════════════════════════════════════════════
// CATEGORY 4: Security
// "What happens with malicious inputs?"
// ═══════════════════════════════════════════════════════════════════════════════

/// Q: Is path traversal blocked?
#[test]
fn socratic_path_traversal_blocked() {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path().join("workspace");
    fs::create_dir_all(&base).unwrap();

    // Create a "secret" file outside workspace
    fs::write(temp_dir.path().join("secret.txt"), "SECRET DATA").unwrap();

    // Try to escape with ../
    let input = "Read @../secret.txt";
    let resolved = FileResolver::resolve(input, &base);

    // Should NOT contain secret data
    assert!(
        !resolved.contains("SECRET DATA"),
        "Path traversal should be blocked"
    );
}

/// Q: Are large files rejected?
#[test]
fn socratic_large_files_rejected() {
    let temp_dir = TempDir::new().unwrap();
    // Create a file > 1MB
    let large_content = "x".repeat(1_100_000);
    fs::write(temp_dir.path().join("large.txt"), &large_content).unwrap();

    let input = "Read @large.txt";
    let resolved = FileResolver::resolve(input, temp_dir.path());

    assert!(
        resolved.contains("error=\"too_large\""),
        "Large files should show error"
    );
    assert!(
        !resolved.contains(&large_content[0..100]),
        "Large file content should not be included"
    );
}

/// Q: Can shell metacharacters in /exec cause issues?
#[test]
fn socratic_shell_metacharacters_preserved() {
    // This test verifies that special characters are preserved in parsing
    // (actual security depends on sandboxing, which is documented)
    let cmd = Command::parse("/exec echo $HOME && rm -rf /");
    match cmd {
        Command::Exec { command } => {
            assert_eq!(command, "echo $HOME && rm -rf /");
            // Note: Execution would be dangerous - this is a parsing test only
        }
        _ => panic!("Expected Exec command"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// CATEGORY 5: UX Consistency
// "Is the interface intuitive?"
// ═══════════════════════════════════════════════════════════════════════════════

/// Q: Are commands case-insensitive?
#[test]
fn socratic_commands_case_insensitive() {
    let variations = vec!["/INFER test", "/Infer test", "/iNfEr test"];
    for input in variations {
        let cmd = Command::parse(input);
        assert!(
            matches!(cmd, Command::Infer { .. }),
            "Command '{}' should be case-insensitive",
            input
        );
    }
}

/// Q: Is whitespace handled gracefully?
#[test]
fn socratic_whitespace_handled() {
    let cmd = Command::parse("  /infer   multiple   spaces   ");
    match cmd {
        Command::Infer { prompt } => assert_eq!(prompt, "multiple   spaces"),
        _ => panic!("Whitespace should be trimmed from edges"),
    }
}

/// Q: Is empty input handled?
#[test]
fn socratic_empty_input_handled() {
    let cmd = Command::parse("");
    assert!(matches!(cmd, Command::Chat { message } if message.is_empty()));

    let cmd = Command::parse("   ");
    assert!(matches!(cmd, Command::Chat { message } if message.is_empty()));
}

/// Q: Does verb() return correct names?
#[test]
fn socratic_verb_names_consistent() {
    assert_eq!(Command::Infer { prompt: "".into() }.verb(), "infer");
    assert_eq!(Command::Exec { command: "".into() }.verb(), "exec");
    assert_eq!(
        Command::Fetch {
            url: "".into(),
            method: "".into()
        }
        .verb(),
        "fetch"
    );
    assert_eq!(
        Command::Invoke {
            tool: "".into(),
            server: None,
            params: serde_json::json!({})
        }
        .verb(),
        "invoke"
    );
    assert_eq!(
        Command::Agent {
            goal: "".into(),
            max_turns: None
        }
        .verb(),
        "agent"
    );
    assert_eq!(
        Command::Model {
            provider: ModelProvider::OpenAI
        }
        .verb(),
        "model"
    );
    assert_eq!(Command::Clear.verb(), "clear");
    assert_eq!(Command::Help.verb(), "help");
}

// ═══════════════════════════════════════════════════════════════════════════════
// CATEGORY 6: Edge Cases
// "What happens at the boundaries?"
// ═══════════════════════════════════════════════════════════════════════════════

/// Q: What happens with very long inputs?
#[test]
fn socratic_long_input_handled() {
    let long_prompt = "x".repeat(10_000);
    let cmd = Command::parse(&format!("/infer {}", long_prompt));
    match cmd {
        Command::Infer { prompt } => assert_eq!(prompt.len(), 10_000),
        _ => panic!("Long inputs should be handled"),
    }
}

/// Q: What happens with unicode input?
#[test]
fn socratic_unicode_handled() {
    let cmd = Command::parse("/infer Expliquez-moi les émojis: 🎉🚀💻");
    match cmd {
        Command::Infer { prompt } => {
            assert!(prompt.contains("émojis"));
            assert!(prompt.contains("🎉"));
        }
        _ => panic!("Unicode should be preserved"),
    }
}

/// Q: What happens with newlines in input?
#[test]
fn socratic_newlines_in_input() {
    let cmd = Command::parse("/infer Line 1\nLine 2\nLine 3");
    match cmd {
        Command::Infer { prompt } => {
            assert!(prompt.contains('\n'), "Newlines should be preserved");
        }
        _ => panic!("Multiline input should work"),
    }
}

/// Q: What happens with JSON containing quotes?
#[test]
fn socratic_json_with_quotes() {
    let cmd = Command::parse(r#"/invoke tool {"text":"hello \"world\""}"#);
    match cmd {
        Command::Invoke { params, .. } => {
            assert_eq!(params["text"], r#"hello "world""#);
        }
        _ => panic!("JSON with quotes should parse"),
    }
}

/// Q: What happens with empty JSON?
#[test]
fn socratic_empty_json() {
    let cmd = Command::parse("/invoke tool {}");
    match cmd {
        Command::Invoke { params, .. } => {
            assert!(params.is_object());
            assert!(params.as_object().unwrap().is_empty());
        }
        _ => panic!("Empty JSON should parse"),
    }
}

/// Q: What happens with invalid JSON?
#[test]
fn socratic_invalid_json_fallback() {
    let cmd = Command::parse("/invoke tool {invalid json}");
    match cmd {
        Command::Invoke { params, .. } => {
            // Invalid JSON should fallback to empty object
            assert!(params.is_object());
        }
        _ => panic!("Invalid JSON should not crash"),
    }
}

/// Q: What happens with max_turns = 0?
#[test]
fn socratic_zero_max_turns() {
    let cmd = Command::parse("/agent do something --max-turns 0");
    match cmd {
        Command::Agent { max_turns, .. } => {
            assert_eq!(max_turns, Some(0));
        }
        _ => panic!("Zero max_turns should parse"),
    }
}

/// Q: ModelProvider availability check works?
#[test]
fn socratic_model_provider_availability() {
    // List should always be "available"
    assert!(ModelProvider::List.is_available());

    // Provider availability depends on env vars (may or may not be set)
    let openai_available = ModelProvider::OpenAI.is_available();
    let claude_available = ModelProvider::Claude.is_available();

    // Just verify the method doesn't panic
    println!("OpenAI available: {}", openai_available);
    println!("Claude available: {}", claude_available);
}

/// Q: ChatRole display names are correct?
#[test]
fn socratic_chat_role_display_names() {
    assert_eq!(ChatRole::User.display_name(), "You");
    assert_eq!(ChatRole::Assistant.display_name(), "Nika"); // Brand name!
    assert_eq!(ChatRole::System.display_name(), "System");
    assert_eq!(ChatRole::Tool.display_name(), "Tool");
}

// ═══════════════════════════════════════════════════════════════════════════════
// CATEGORY 7: Integration Scenarios
// "Do real-world workflows work?"
// ═══════════════════════════════════════════════════════════════════════════════

/// Q: Can we parse a typical developer workflow?
#[test]
fn socratic_developer_workflow_parsing() {
    // Scenario: Developer exploring code
    let commands = vec![
        ("Check @src/main.rs", "Chat", "check file"),
        ("/exec cargo test", "Exec", "run tests"),
        ("/infer Explain this error", "Infer", "ask question"),
        ("/model claude", "Model", "switch model"),
        ("/clear", "Clear", "clear history"),
    ];

    for (input, expected_verb, scenario) in commands {
        let cmd = Command::parse(input);
        assert_eq!(
            cmd.verb(),
            expected_verb.to_lowercase(),
            "Scenario '{}' should parse correctly",
            scenario
        );
    }
}

/// Q: Can we parse a data fetching workflow?
#[test]
fn socratic_data_workflow_parsing() {
    let commands = [
        "/fetch https://api.github.com/repos/rust-lang/rust",
        "/invoke novanet:describe {\"entity\":\"rust\"}",
        "/infer Summarize this data",
        "/agent Generate documentation --max-turns 5",
    ];

    for (i, input) in commands.iter().enumerate() {
        let cmd = Command::parse(input);
        assert!(
            !cmd.is_empty(),
            "Step {} should parse to non-empty command",
            i
        );
    }
}

/// Q: Multiple file mentions in one message?
#[test]
fn socratic_multiple_files_in_message() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("a.rs"), "// file a").unwrap();
    fs::write(temp_dir.path().join("b.rs"), "// file b").unwrap();
    fs::write(temp_dir.path().join("c.rs"), "// file c").unwrap();

    let input = "Compare @a.rs with @b.rs and @c.rs";
    let resolved = FileResolver::resolve(input, temp_dir.path());

    assert!(resolved.contains("// file a"));
    assert!(resolved.contains("// file b"));
    assert!(resolved.contains("// file c"));
}
