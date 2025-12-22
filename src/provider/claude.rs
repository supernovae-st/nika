//! Claude provider using the Claude CLI
//!
//! Executes prompts via `claude -p "prompt"` command.
//! Supports context passing through conversation history.

use super::{Capabilities, PromptRequest, PromptResponse, Provider, TokenUsage};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

#[cfg(test)]
use std::sync::Arc;

/// Default timeout for Claude CLI execution (5 minutes)
const DEFAULT_EXECUTE_TIMEOUT: Duration = Duration::from_secs(300);

/// Timeout for CLI availability check
const CLI_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Claude provider that uses the Claude CLI
pub struct ClaudeProvider {
    /// Path to the claude CLI binary
    cli_path: String,
    /// Whether to print output format as JSON
    json_output: bool,
    /// Execution timeout
    execute_timeout: Duration,
}

impl ClaudeProvider {
    /// Create a new Claude provider with default settings
    pub fn new() -> Self {
        Self {
            cli_path: "claude".to_string(),
            json_output: false,
            execute_timeout: DEFAULT_EXECUTE_TIMEOUT,
        }
    }

    /// Set a custom CLI path
    pub fn with_cli_path(mut self, path: impl Into<String>) -> Self {
        self.cli_path = path.into();
        self
    }

    /// Enable JSON output mode
    pub fn with_json_output(mut self) -> Self {
        self.json_output = true;
        self
    }

    /// Set execution timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.execute_timeout = timeout;
        self
    }

    /// Build the full prompt including history context
    fn build_prompt(&self, request: &PromptRequest) -> String {
        let mut full_prompt = String::new();

        // Add system prompt if present
        if let Some(ref system) = request.system_prompt {
            full_prompt.push_str(&format!("[System: {}]\n\n", system));
        }

        // Add conversation history for context (agent: tasks)
        if !request.history.is_empty() && !request.is_isolated {
            full_prompt.push_str("Previous conversation:\n");
            for msg in &request.history {
                let role_str = match msg.role {
                    crate::runner::MessageRole::User => "User",
                    crate::runner::MessageRole::Assistant => "Assistant",
                    crate::runner::MessageRole::System => "System",
                };
                full_prompt.push_str(&format!("{}: {}\n", role_str, msg.content));
            }
            full_prompt.push_str("\n---\n\n");
        }

        // Add the main prompt
        full_prompt.push_str(&request.prompt);

        full_prompt
    }

    /// Check if claude CLI is installed (with 5s timeout)
    fn check_cli(&self) -> bool {
        Command::new(&self.cli_path)
            .arg("--version")
            .spawn()
            .and_then(|mut child| {
                match child.wait_timeout(CLI_CHECK_TIMEOUT)? {
                    Some(status) => Ok(status.success()),
                    None => {
                        // Timeout - kill the process
                        let _ = child.kill();
                        Ok(false)
                    }
                }
            })
            .unwrap_or(false)
    }
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::claude()
    }

    async fn execute(&self, request: PromptRequest) -> Result<PromptResponse> {
        let full_prompt = self.build_prompt(&request);
        let prompt_len = full_prompt.len();

        // Capture values for the blocking closure
        let cli_path = self.cli_path.clone();
        let model = request.model.clone();
        let allowed_tools = request.allowed_tools.clone();
        let execute_timeout = self.execute_timeout;

        // Run blocking subprocess operations in a separate thread pool
        let result = tokio::task::spawn_blocking(move || -> Result<(bool, String, usize)> {
            // Build command with piped stdio for capture
            let mut cmd = Command::new(&cli_path);
            cmd.arg("-p")
                .arg(&full_prompt)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            // Add model if specified
            if !model.is_empty() {
                cmd.arg("--model").arg(&model);
            }

            // Add allowed tools if any
            if !allowed_tools.is_empty() {
                cmd.arg("--allowedTools").arg(allowed_tools.join(","));
            }

            // Spawn the process
            let mut child = cmd
                .spawn()
                .context("Failed to spawn claude CLI. Is it installed?")?;

            // Wait with timeout
            match child.wait_timeout(execute_timeout)? {
                Some(status) => {
                    // Process completed within timeout - read outputs
                    let stdout = child
                        .stdout
                        .take()
                        .map(|mut s| {
                            let mut buf = String::new();
                            s.read_to_string(&mut buf).ok();
                            buf
                        })
                        .unwrap_or_default();

                    let stderr = child
                        .stderr
                        .take()
                        .map(|mut s| {
                            let mut buf = String::new();
                            s.read_to_string(&mut buf).ok();
                            buf
                        })
                        .unwrap_or_default();

                    if status.success() {
                        Ok((true, stdout, prompt_len))
                    } else {
                        Ok((false, stderr, prompt_len))
                    }
                }
                None => {
                    // Timeout! Kill the process
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the zombie

                    let error_msg =
                        format!("Claude CLI execution timed out after {:?}", execute_timeout);
                    Ok((false, error_msg, prompt_len))
                }
            }
        })
        .await
        .context("Blocking task panicked")??;

        // Convert result to PromptResponse
        let (success, content, prompt_len) = result;
        if success {
            let trimmed = content.trim().to_string();
            let usage = TokenUsage::estimate(prompt_len, trimmed.len());
            Ok(PromptResponse::success(trimmed).with_usage(usage))
        } else {
            Ok(PromptResponse::failure(content))
        }
    }

    fn is_available(&self) -> bool {
        self.check_cli()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{AgentMessage, MessageRole};

    #[test]
    fn test_claude_provider_name() {
        let provider = ClaudeProvider::new();
        assert_eq!(provider.name(), "claude");
    }

    #[test]
    fn test_claude_supports_tools() {
        let provider = ClaudeProvider::new();
        assert!(provider.supports_tools());
    }

    #[test]
    fn test_build_prompt_simple() {
        let provider = ClaudeProvider::new();
        let request = PromptRequest::new("Hello world", "claude-sonnet-4-5");

        let prompt = provider.build_prompt(&request);
        assert_eq!(prompt, "Hello world");
    }

    #[test]
    fn test_build_prompt_with_system() {
        let provider = ClaudeProvider::new();
        let request =
            PromptRequest::new("Hello", "claude-sonnet-4-5").with_system_prompt("You are helpful");

        let prompt = provider.build_prompt(&request);
        assert!(prompt.contains("[System: You are helpful]"));
        assert!(prompt.contains("Hello"));
    }

    #[test]
    fn test_build_prompt_with_history() {
        let provider = ClaudeProvider::new();
        let history = vec![
            AgentMessage {
                role: MessageRole::User,
                content: Arc::from("What is 2+2?"),
            },
            AgentMessage {
                role: MessageRole::Assistant,
                content: Arc::from("4"),
            },
        ];
        let request = PromptRequest::new("And 3+3?", "claude-sonnet-4-5").with_history(history);

        let prompt = provider.build_prompt(&request);
        assert!(prompt.contains("Previous conversation:"));
        assert!(prompt.contains("User: What is 2+2?"));
        assert!(prompt.contains("Assistant: 4"));
        assert!(prompt.contains("And 3+3?"));
    }

    #[test]
    fn test_build_prompt_isolated_ignores_history() {
        let provider = ClaudeProvider::new();
        let history = vec![AgentMessage {
            role: MessageRole::User,
            content: Arc::from("Previous context"),
        }];
        let request = PromptRequest::new("New isolated task", "claude-sonnet-4-5")
            .with_history(history)
            .isolated();

        let prompt = provider.build_prompt(&request);
        // Isolated should NOT include history
        assert!(!prompt.contains("Previous conversation"));
        assert!(!prompt.contains("Previous context"));
        assert!(prompt.contains("New isolated task"));
    }

    #[test]
    fn test_custom_cli_path() {
        let provider = ClaudeProvider::new().with_cli_path("/custom/path/claude");
        assert_eq!(provider.cli_path, "/custom/path/claude");
    }

    #[test]
    fn test_custom_timeout() {
        let provider = ClaudeProvider::new().with_timeout(Duration::from_secs(60));
        assert_eq!(provider.execute_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_default_timeout_is_5_minutes() {
        let provider = ClaudeProvider::new();
        assert_eq!(provider.execute_timeout, Duration::from_secs(300));
    }

    // Note: Actual CLI execution tests would require mocking or integration tests
    // The check_cli() and execute() methods need real CLI for full testing
}
