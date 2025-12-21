//! Claude provider using the Claude CLI
//!
//! Executes prompts via `claude -p "prompt"` command.
//! Supports context passing through conversation history.

use super::{PromptRequest, PromptResponse, Provider, TokenUsage};
use anyhow::{Context, Result};
use std::process::Command;

/// Claude provider that uses the Claude CLI
pub struct ClaudeProvider {
    /// Path to the claude CLI binary
    cli_path: String,
    /// Whether to print output format as JSON
    json_output: bool,
}

impl ClaudeProvider {
    /// Create a new Claude provider with default settings
    pub fn new() -> Self {
        Self {
            cli_path: "claude".to_string(),
            json_output: false,
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

    /// Check if claude CLI is installed
    fn check_cli(&self) -> bool {
        Command::new(&self.cli_path)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn execute(&self, request: PromptRequest) -> Result<PromptResponse> {
        let full_prompt = self.build_prompt(&request);

        // Build command
        let mut cmd = Command::new(&self.cli_path);
        cmd.arg("-p").arg(&full_prompt);

        // Add model if specified
        if !request.model.is_empty() {
            cmd.arg("--model").arg(&request.model);
        }

        // Add allowed tools if any
        if !request.allowed_tools.is_empty() {
            cmd.arg("--allowedTools")
                .arg(request.allowed_tools.join(","));
        }

        // Execute
        let output = cmd
            .output()
            .context("Failed to execute claude CLI. Is it installed?")?;

        if output.status.success() {
            let content = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let usage = TokenUsage::estimate(full_prompt.len(), content.len());

            Ok(PromptResponse::success(content).with_usage(usage))
        } else {
            let error = String::from_utf8_lossy(&output.stderr).to_string();
            Ok(PromptResponse::failure(error))
        }
    }

    fn supports_tools(&self) -> bool {
        true // Claude CLI supports tool execution
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
        let request = PromptRequest::new("Hello", "claude-sonnet-4-5")
            .with_system_prompt("You are helpful");

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
                content: "What is 2+2?".to_string(),
            },
            AgentMessage {
                role: MessageRole::Assistant,
                content: "4".to_string(),
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
            content: "Previous context".to_string(),
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

    // Note: Actual CLI execution tests would require mocking or integration tests
    // The check_cli() and execute() methods need real CLI for full testing
}
