// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command parser for chat input
//!
//! Parses user input into structured commands for the 5 Nika verbs.
//!
//! # Supported Commands
//!
//! | Command | Description | Example |
//! |---------|-------------|---------|
//! | `/infer <prompt>` | Direct LLM inference | `/infer explain this code` |
//! | `/exec <command>` | Shell execution | `/exec cargo test` |
//! | `/fetch <url> [method]` | HTTP request | `/fetch https://api.example.com GET` |
//! | `/invoke [server:]tool [json]` | MCP tool call | `/invoke novanet:describe {"entity":"qr-code"}` |
//! | `/agent <goal> [--max-turns N]` | Multi-turn agent | `/agent generate a landing page` |
//! | `/help` or `/?` | Show help | `/help` |
//! | (plain text) | Chat message | `hello world` |

mod model_provider;
mod verb_parser;

#[cfg(test)]
mod tests;

pub use model_provider::ModelProvider;

/// MCP server action for /mcp command
#[derive(Debug, Clone, PartialEq)]
pub enum McpAction {
    /// List available MCP servers
    List,
    /// Select specific servers (replaces current selection)
    Select(Vec<String>),
    /// Toggle a single server on/off
    Toggle(String),
}

/// Parsed chat command
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `/infer <prompt>` - Direct LLM inference
    Infer { prompt: String },

    /// `/exec <command>` - Shell execution
    Exec { command: String },

    /// `/fetch <url> [method]` - HTTP request
    Fetch { url: String, method: String },

    /// /fetch error - user made a common mistake
    FetchError {
        error: String,
        hint: String,
        example: String,
    },

    /// `/invoke [server:]tool [json_params]` - MCP tool call
    Invoke {
        tool: String,
        server: Option<String>,
        params: serde_json::Value,
    },

    /// /invoke error - missing tool name or invalid params
    InvokeError {
        error: String,
        hint: String,
        example: String,
    },

    /// `/agent <goal> [--max-turns N] [--mcp server1,server2]` - Multi-turn agentic loop
    Agent {
        goal: String,
        max_turns: Option<u32>,
        /// MCP servers to use for this agent
        mcp_servers: Vec<String>,
    },

    /// Plain chat message (default)
    Chat { message: String },

    /// /help or /? - Show help
    Help,

    /// `/model <provider>` - Switch LLM provider (openai, claude)
    Model { provider: ModelProvider },

    /// /clear - Clear chat history
    Clear,

    /// /mcp [list|select|toggle] - MCP server management
    Mcp { action: McpAction },

    /// `/export [json|yaml] [path]` - Export chat session
    Export {
        format: ExportFormat,
        path: Option<String>,
    },

    /// `/run <path>` - Run a workflow file
    Run { path: String },
}

/// Export format for chat sessions
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ExportFormat {
    /// JSON format (default)
    #[default]
    Json,
    /// YAML workflow format - exports as runnable .nika.yaml
    Yaml,
}

impl Command {
    /// Parse user input into a Command
    ///
    /// # Examples
    ///
    /// ```
    /// use nika_tui::command::Command;
    ///
    /// let cmd = Command::parse("/infer explain this code");
    /// assert!(matches!(cmd, Command::Infer { prompt } if prompt == "explain this code"));
    ///
    /// let cmd = Command::parse("hello world");
    /// assert!(matches!(cmd, Command::Chat { message } if message == "hello world"));
    /// ```
    pub fn parse(input: &str) -> Self {
        let input = input.trim();

        // Empty input is a chat message
        if input.is_empty() {
            return Command::Chat {
                message: String::new(),
            };
        }

        if input.starts_with('/') {
            let parts: Vec<&str> = input.splitn(2, ' ').collect();
            // Safe: splitn always returns at least one element for non-empty input
            let verb = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
            let args = parts.get(1).map(|s| s.trim()).unwrap_or("");

            match verb.as_str() {
                "/infer" => Command::Infer {
                    prompt: args.to_string(),
                },
                "/exec" => Command::Exec {
                    command: args.to_string(),
                },
                "/fetch" => Self::parse_fetch_args(args),
                "/invoke" => Self::parse_invoke_args(args),
                "/agent" => Self::parse_agent_args(args),
                "/help" | "/?" => Command::Help,
                "/model" => Self::parse_model_args(args),
                "/mcp" => Self::parse_mcp_args(args),
                "/clear" => Command::Clear,
                "/export" => Self::parse_export_args(args),
                "/run" => Command::Run {
                    path: args.to_string(),
                },
                _ => {
                    // Unknown command, treat as chat message
                    Command::Chat {
                        message: input.to_string(),
                    }
                }
            }
        } else {
            Command::Chat {
                message: input.to_string(),
            }
        }
    }

    /// Get the verb name for display
    pub fn verb(&self) -> &'static str {
        match self {
            Command::Infer { .. } => "infer",
            Command::Exec { .. } => "exec",
            Command::Fetch { .. } | Command::FetchError { .. } => "fetch",
            Command::Invoke { .. } | Command::InvokeError { .. } => "invoke",
            Command::Agent { .. } => "agent",
            Command::Chat { .. } => "chat",
            Command::Help => "help",
            Command::Model { .. } => "model",
            Command::Clear => "clear",
            Command::Mcp { .. } => "mcp",
            Command::Export { .. } => "export",
            Command::Run { .. } => "run",
        }
    }

    /// Check if this command is an error
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            Command::FetchError { .. } | Command::InvokeError { .. }
        )
    }

    /// Check if this is an empty command (empty input)
    pub fn is_empty(&self) -> bool {
        match self {
            Command::Chat { message } => message.is_empty(),
            Command::Infer { prompt } => prompt.is_empty(),
            Command::Exec { command } => command.is_empty(),
            Command::Fetch { url, .. } => url.is_empty(),
            Command::FetchError { .. } | Command::InvokeError { .. } => false, // Errors are not "empty"
            Command::Invoke { tool, .. } => tool.is_empty(),
            Command::Agent { goal, .. } => goal.is_empty(),
            Command::Help => false,
            Command::Model { .. } => false,
            Command::Clear => false,
            Command::Mcp { .. } => false,
            Command::Export { .. } => false, // Export is never "empty"
            Command::Run { path } => path.is_empty(),
        }
    }

    /// Check if this command is a verb command that creates a TaskBox widget
    /// (Infer, Exec, Fetch, Invoke, Agent) - these should NOT add a user message bubble
    /// because the TaskBox already displays the command visually
    pub fn is_verb_command(&self) -> bool {
        matches!(
            self,
            Command::Infer { .. }
                | Command::Exec { .. }
                | Command::Fetch { .. }
                | Command::FetchError { .. }
                | Command::Invoke { .. }
                | Command::InvokeError { .. }
                | Command::Agent { .. }
        )
    }
}

/// Help text for the chat interface
pub const HELP_TEXT: &str = r#"
Nika Chat Commands:

  /infer <prompt>           Direct LLM inference
  /exec <command>           Shell command execution
  /fetch <url> [method]     HTTP request (default: GET)
  /invoke [server:]tool     MCP tool call (params as JSON)
  /agent <goal>             Multi-turn agent (--max-turns N) (--mcp servers)
  /mcp [list|select|toggle] MCP server management
  /model <provider>         Switch LLM (openai, claude, mistral, groq, deepseek, native)
  /run <path>               Run a workflow file (.nika.yaml)
  /export [path]            Export chat to JSON file
  /clear                    Clear chat history
  /help or /?               Show this help

Keyboard Shortcuts:
  Ctrl+K                    Open command palette
  Ctrl+T                    Toggle deep thinking (🧠)
  Ctrl+M                    Toggle Infer/Agent mode
  Ctrl+S                    Open MCP server picker

Modes:
  ⚡ Infer                  Simple inference (default)
  🐔 Agent                  Multi-turn with MCP tools
  🧠 Think                  Extended thinking (Claude only)

MCP Server Management:
  /mcp                      List available MCP servers
  /mcp list                 List available MCP servers
  /mcp select novanet       Select specific servers
  /mcp toggle novanet       Toggle server on/off

Model Switching:
  /model openai             Switch to OpenAI (gpt-4o)
  /model claude             Switch to Anthropic Claude
  /model list               Show available providers

File Mentions:
  @src/main.rs              Include file content in prompt

Examples:
  /infer explain this code
  /exec cargo test
  /fetch https://api.example.com GET
  /invoke novanet:describe {"entity":"qr-code"}
  /agent generate a landing page --max-turns 5 --mcp novanet
  /agent research topic --mcp novanet,perplexity
  Explain @src/main.rs      Include file content

Plain text is treated as chat messages for the current model.
"#;
