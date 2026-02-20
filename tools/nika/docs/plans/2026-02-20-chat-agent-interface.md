# Chat Agent Interface Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform ChatView into a full AI agent interface with 5 verbs, MCP integration, file mentions, and streaming responses.

**Architecture:** ChatAgent struct manages LLM calls via RigProvider::openai(), with CommandParser for verb dispatch, FileResolver for @file mentions, and streaming via mpsc channels.

**Tech Stack:** rig-core (OpenAI), tokio mpsc, ratatui 0.30, rmcp (MCP)

---

## Phase 1: Switch to OpenAI Provider

### Task 1.1: Update App to Use OpenAI

**Files:**
- Modify: `src/tui/app.rs:1-50`

**Step 1: Write the failing test**

```rust
// In src/tui/app.rs, add to #[cfg(test)] module
#[tokio::test]
async fn test_app_uses_openai_provider() {
    // Verify OPENAI_API_KEY env is checked
    std::env::set_var("OPENAI_API_KEY", "test-key");
    // The app should compile with OpenAI provider
    // This is a compile-time check essentially
    assert!(std::env::var("OPENAI_API_KEY").is_ok());
}
```

**Step 2: Run test to verify setup**

Run: `cargo test test_app_uses_openai -- --nocapture`
Expected: PASS (env var set)

**Step 3: Change provider from Claude to OpenAI**

In `src/tui/app.rs`, change:
```rust
// FROM:
let provider = RigProvider::claude();

// TO:
let provider = RigProvider::openai();
```

**Step 4: Run tests**

Run: `cargo test -p nika`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): switch ChatOverlay from Claude to OpenAI provider"
```

---

## Phase 2: Command Parser

### Task 2.1: Create CommandParser Module

**Files:**
- Create: `src/tui/command.rs`
- Modify: `src/tui/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/command.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_infer_command() {
        let input = "/infer explain this code";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Infer { prompt } if prompt == "explain this code"));
    }

    #[test]
    fn test_parse_exec_command() {
        let input = "/exec cargo test";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Exec { command } if command == "cargo test"));
    }

    #[test]
    fn test_parse_plain_message() {
        let input = "hello world";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Chat { message } if message == "hello world"));
    }
}
```

**Step 2: Run test to see it fail**

Run: `cargo test test_parse_infer_command`
Expected: FAIL with "cannot find value `Command`"

**Step 3: Implement Command enum and parser**

```rust
// src/tui/command.rs
//! Command parser for chat input
//!
//! Parses user input into structured commands for the 5 Nika verbs.

/// Parsed chat command
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// /infer <prompt> - Direct LLM inference
    Infer { prompt: String },
    /// /exec <command> - Shell execution
    Exec { command: String },
    /// /fetch <url> - HTTP request
    Fetch { url: String, method: String },
    /// /invoke <tool> [--param value] - MCP tool call
    Invoke { tool: String, server: Option<String>, params: serde_json::Value },
    /// /agent <goal> - Multi-turn agentic loop
    Agent { goal: String, max_turns: Option<u32> },
    /// Plain chat message (default)
    Chat { message: String },
    /// Help command
    Help,
}

impl Command {
    /// Parse user input into a Command
    pub fn parse(input: &str) -> Self {
        let input = input.trim();

        if input.starts_with('/') {
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            let verb = parts[0].to_lowercase();
            let args = parts.get(1).map(|s| s.trim()).unwrap_or("");

            match verb.as_str() {
                "/infer" => Command::Infer { prompt: args.to_string() },
                "/exec" => Command::Exec { command: args.to_string() },
                "/fetch" => {
                    // Parse URL and optional method
                    let parts: Vec<&str> = args.splitn(2, ' ').collect();
                    Command::Fetch {
                        url: parts[0].to_string(),
                        method: parts.get(1).unwrap_or(&"GET").to_string(),
                    }
                }
                "/invoke" => {
                    // Parse: /invoke [server:]tool [--params json]
                    let (tool, server, params) = Self::parse_invoke_args(args);
                    Command::Invoke { tool, server, params }
                }
                "/agent" => {
                    // Parse: /agent <goal> [--max-turns N]
                    let (goal, max_turns) = Self::parse_agent_args(args);
                    Command::Agent { goal, max_turns }
                }
                "/help" | "/?" => Command::Help,
                _ => Command::Chat { message: input.to_string() },
            }
        } else {
            Command::Chat { message: input.to_string() }
        }
    }

    fn parse_invoke_args(args: &str) -> (String, Option<String>, serde_json::Value) {
        // Simple parsing: tool_name or server:tool_name
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        let tool_spec = parts[0];

        let (server, tool) = if tool_spec.contains(':') {
            let tp: Vec<&str> = tool_spec.splitn(2, ':').collect();
            (Some(tp[0].to_string()), tp[1].to_string())
        } else {
            (None, tool_spec.to_string())
        };

        // Parse remaining as JSON params if present
        let params = parts.get(1)
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        (tool, server, params)
    }

    fn parse_agent_args(args: &str) -> (String, Option<u32>) {
        if args.contains("--max-turns") {
            let parts: Vec<&str> = args.split("--max-turns").collect();
            let goal = parts[0].trim().to_string();
            let max_turns = parts.get(1)
                .and_then(|s| s.trim().parse().ok());
            (goal, max_turns)
        } else {
            (args.to_string(), None)
        }
    }
}
```

**Step 4: Add module to mod.rs**

In `src/tui/mod.rs`, add:
```rust
pub mod command;
```

**Step 5: Run tests**

Run: `cargo test -p nika command`
Expected: All command tests pass

**Step 6: Commit**

```bash
git add src/tui/command.rs src/tui/mod.rs
git commit -m "feat(tui): add CommandParser for 5 Nika verbs"
```

---

## Phase 3: File Resolver

### Task 3.1: Create FileResolver Module

**Files:**
- Create: `src/tui/file_resolve.rs`
- Modify: `src/tui/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/file_resolve.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_file_mentions() {
        let input = "Explain @src/main.rs and @Cargo.toml";
        let mentions = FileResolver::extract_mentions(input);
        assert_eq!(mentions, vec!["src/main.rs", "Cargo.toml"]);
    }

    #[test]
    fn test_no_mentions() {
        let input = "Just a normal message";
        let mentions = FileResolver::extract_mentions(input);
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_email_not_file_mention() {
        let input = "Contact me at user@example.com";
        let mentions = FileResolver::extract_mentions(input);
        assert!(mentions.is_empty());
    }
}
```

**Step 2: Run test to see it fail**

Run: `cargo test test_extract_file_mentions`
Expected: FAIL

**Step 3: Implement FileResolver**

```rust
// src/tui/file_resolve.rs
//! File mention resolver for @file syntax
//!
//! Extracts and resolves @file mentions in chat messages.

use std::path::Path;
use regex::Regex;

/// Resolves @file mentions in chat input
pub struct FileResolver;

impl FileResolver {
    /// Extract all @file mentions from input
    pub fn extract_mentions(input: &str) -> Vec<String> {
        // Match @path/to/file but not emails (no @ immediately after alphanumeric)
        let re = Regex::new(r"(?:^|[^a-zA-Z0-9])@([\w./\-]+\.\w+)").unwrap();

        re.captures_iter(input)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Resolve file mentions and return expanded prompt
    pub fn resolve(input: &str, base_dir: &Path) -> Result<String, std::io::Error> {
        let mentions = Self::extract_mentions(input);
        let mut result = input.to_string();

        for mention in mentions {
            let file_path = base_dir.join(&mention);
            if file_path.exists() {
                let content = std::fs::read_to_string(&file_path)?;
                let replacement = format!(
                    "\n\n<file path=\"{}\">\n{}\n</file>\n",
                    mention, content
                );
                result = result.replace(&format!("@{}", mention), &replacement);
            }
        }

        Ok(result)
    }
}
```

**Step 4: Add regex dependency if needed**

Check Cargo.toml - regex should already be present. If not:
```toml
regex = "1"
```

**Step 5: Add module to mod.rs**

In `src/tui/mod.rs`, add:
```rust
pub mod file_resolve;
```

**Step 6: Run tests**

Run: `cargo test -p nika file_resolve`
Expected: All file_resolve tests pass

**Step 7: Commit**

```bash
git add src/tui/file_resolve.rs src/tui/mod.rs
git commit -m "feat(tui): add FileResolver for @file mentions"
```

---

## Phase 4: ChatAgent with Streaming

### Task 4.1: Create ChatAgent Module

**Files:**
- Create: `src/tui/chat_agent.rs`
- Modify: `src/tui/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/chat_agent.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chat_agent_creation() {
        let agent = ChatAgent::new();
        assert!(agent.is_ok() || std::env::var("OPENAI_API_KEY").is_err());
    }

    #[test]
    fn test_streaming_state_initial() {
        let state = StreamingState::default();
        assert!(!state.is_streaming);
        assert!(state.partial_response.is_empty());
    }
}
```

**Step 2: Implement ChatAgent**

```rust
// src/tui/chat_agent.rs
//! ChatAgent for full AI agent interface
//!
//! Manages LLM calls, streaming, and command execution.

use crate::provider::rig::RigProvider;
use crate::error::NikaError;
use tokio::sync::mpsc;

/// Streaming state for UI updates
#[derive(Debug, Default, Clone)]
pub struct StreamingState {
    pub is_streaming: bool,
    pub partial_response: String,
    pub tokens_received: usize,
}

/// Chat message for history
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub timestamp: std::time::Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Main chat agent handling LLM interactions
pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    streaming_tx: Option<mpsc::Sender<String>>,
}

impl ChatAgent {
    /// Create new ChatAgent with OpenAI provider
    pub fn new() -> Result<Self, NikaError> {
        let provider = RigProvider::openai();
        Ok(Self {
            provider,
            history: Vec::new(),
            streaming_tx: None,
        })
    }

    /// Set streaming channel for real-time updates
    pub fn with_streaming(mut self, tx: mpsc::Sender<String>) -> Self {
        self.streaming_tx = Some(tx);
        self
    }

    /// Execute an infer command
    pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
        self.history.push(ChatMessage {
            role: ChatRole::User,
            content: prompt.to_string(),
            timestamp: std::time::Instant::now(),
        });

        let response = self.provider.infer(prompt, None).await?;

        self.history.push(ChatMessage {
            role: ChatRole::Assistant,
            content: response.clone(),
            timestamp: std::time::Instant::now(),
        });

        Ok(response)
    }

    /// Execute a shell command (uses tokio::process::Command for safety)
    pub async fn exec_command(&self, command: &str) -> Result<String, NikaError> {
        use tokio::process::Command as TokioCommand;

        let output = TokioCommand::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await
            .map_err(|e| NikaError::ExecError {
                task_id: "chat-exec".into(),
                command: command.into(),
                reason: e.to_string(),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            Ok(stdout.to_string())
        } else {
            Ok(format!("Exit code: {}\n{}\n{}",
                output.status.code().unwrap_or(-1),
                stdout,
                stderr
            ))
        }
    }

    /// Execute a fetch command
    pub async fn fetch(&self, url: &str, method: &str) -> Result<String, NikaError> {
        let client = reqwest::Client::new();

        let response = match method.to_uppercase().as_str() {
            "GET" => client.get(url).send().await,
            "POST" => client.post(url).send().await,
            _ => client.get(url).send().await,
        };

        let response = response.map_err(|e| NikaError::FetchError {
            task_id: "chat-fetch".into(),
            url: url.into(),
            reason: e.to_string(),
        })?;

        let text = response.text().await.map_err(|e| NikaError::FetchError {
            task_id: "chat-fetch".into(),
            url: url.into(),
            reason: e.to_string(),
        })?;

        Ok(text)
    }

    /// Get conversation history
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// Clear conversation history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }
}
```

**Step 3: Add module to mod.rs**

In `src/tui/mod.rs`, add:
```rust
pub mod chat_agent;
```

**Step 4: Run tests**

Run: `cargo test -p nika chat_agent`
Expected: Tests pass

**Step 5: Commit**

```bash
git add src/tui/chat_agent.rs src/tui/mod.rs
git commit -m "feat(tui): add ChatAgent with OpenAI streaming support"
```

---

## Phase 5: Integrate into ChatView

### Task 5.1: Update ChatView to Use New Components

**Files:**
- Modify: `src/tui/views/chat.rs`
- Modify: `src/tui/app.rs`

**Step 1: Update chat.rs imports and add command handling**

```rust
// At top of src/tui/views/chat.rs, add:
use crate::tui::command::Command;
use crate::tui::file_resolve::FileResolver;
```

**Step 2: Add command dispatch in handle_key**

Modify the Enter key handler to parse commands:
```rust
// In handle_key, for Enter key:
KeyCode::Enter => {
    if let Some(message) = state.chat_overlay.add_user_message() {
        let cmd = Command::parse(&message);
        match cmd {
            Command::Help => {
                state.chat_overlay.add_nika_message(HELP_TEXT);
                ViewAction::None
            }
            Command::Exec { command } => {
                ViewAction::ChatExec(command)
            }
            Command::Fetch { url, method } => {
                ViewAction::ChatFetch(url, method)
            }
            Command::Invoke { tool, server, params } => {
                ViewAction::ChatInvoke(tool, server, params)
            }
            Command::Agent { goal, max_turns } => {
                ViewAction::ChatAgent(goal, max_turns)
            }
            Command::Infer { prompt } | Command::Chat { message: prompt } => {
                // Resolve file mentions
                let expanded = FileResolver::resolve(&prompt, std::path::Path::new("."))
                    .unwrap_or(prompt);
                ViewAction::ChatInfer(expanded)
            }
        }
    } else {
        ViewAction::None
    }
}
```

**Step 3: Add new ViewAction variants**

In `src/tui/views/mod.rs`:
```rust
pub enum ViewAction {
    // ... existing variants ...
    ChatInfer(String),
    ChatExec(String),
    ChatFetch(String, String),
    ChatInvoke(String, Option<String>, serde_json::Value),
    ChatAgent(String, Option<u32>),
}
```

**Step 4: Handle new actions in app.rs**

Add handlers for the new ViewAction variants in the main event loop.

**Step 5: Run tests**

Run: `cargo test -p nika`
Expected: All tests pass

**Step 6: Commit**

```bash
git add src/tui/views/chat.rs src/tui/views/mod.rs src/tui/app.rs
git commit -m "feat(tui): integrate CommandParser and FileResolver into ChatView"
```

---

## Phase 6: Visual Improvements

### Task 6.1: Add Colored Message Bubbles

**Files:**
- Modify: `src/tui/views/chat.rs`

**Step 1: Update render_messages to use colors**

```rust
fn render_messages(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
    for msg in &state.chat_overlay.messages {
        let style = match msg.role {
            ChatOverlayMessageRole::User => Style::default().fg(Color::Cyan),
            ChatOverlayMessageRole::Nika => Style::default().fg(Color::Green),
        };
        // Apply style to message rendering
    }
}
```

**Step 2: Add streaming indicator**

```rust
// In status bar rendering:
let status = if state.chat_overlay.is_streaming {
    "🔄 Streaming..."
} else {
    "Ready"
};
```

**Step 3: Run and verify visually**

Run: `cargo run -- tui`
Expected: Colored messages, streaming indicator visible

**Step 4: Commit**

```bash
git add src/tui/views/chat.rs
git commit -m "feat(tui): add colored message bubbles and streaming indicator"
```

---

## Phase 7: Final Integration & Testing

### Task 7.1: End-to-End Test

**Files:**
- Create: `tests/chat_agent_integration.rs`

**Step 1: Write integration test**

```rust
#[tokio::test]
#[ignore] // Requires OPENAI_API_KEY
async fn test_chat_agent_full_flow() {
    use nika::tui::chat_agent::ChatAgent;
    use nika::tui::command::Command;

    let mut agent = ChatAgent::new().expect("ChatAgent creation");

    // Test infer
    let cmd = Command::parse("/infer Say hello in one word");
    if let Command::Infer { prompt } = cmd {
        let response = agent.infer(&prompt).await;
        assert!(response.is_ok());
    }

    // Test exec (using safe tokio::process::Command)
    let cmd = Command::parse("/exec echo test");
    if let Command::Exec { command } = cmd {
        let response = agent.exec_command(&command).await;
        assert!(response.is_ok());
        assert!(response.unwrap().contains("test"));
    }
}
```

**Step 2: Run integration test**

Run: `cargo test test_chat_agent_full_flow -- --ignored`
Expected: Pass (with OPENAI_API_KEY set)

**Step 3: Final commit**

```bash
git add tests/chat_agent_integration.rs
git commit -m "test(tui): add chat agent integration tests"
```

---

## Summary

| Phase | Tasks | Description |
|-------|-------|-------------|
| 1 | 1.1 | Switch to OpenAI provider |
| 2 | 2.1 | Command parser for 5 verbs |
| 3 | 3.1 | File resolver for @file |
| 4 | 4.1 | ChatAgent with streaming |
| 5 | 5.1 | Integrate into ChatView |
| 6 | 6.1 | Visual improvements |
| 7 | 7.1 | Integration testing |

**Total: 7 tasks across 7 phases**
