// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb Input — Data types only
//!
//! Widget rendering code (`VerbIndicator`, `VerbPrompt`) removed.
//! Data types (`ChatVerb`, `ParsedInput`, `SystemCommand`) are used
//! by `chat/command.rs` and `chat/mod.rs`.

use ratatui::style::Color;

use crate::theme::VerbColor;

/// Chat verb types - maps to Nika's 5 semantic verbs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatVerb {
    #[default]
    Infer, // Default - LLM text generation
    Exec,   // Shell command execution
    Fetch,  // HTTP request
    Invoke, // MCP tool call
    Agent,  // Multi-turn agentic loop
}

impl ChatVerb {
    /// Icon for the verb (matches DAG visualization)
    pub fn icon(&self) -> &'static str {
        match self {
            ChatVerb::Infer => "⚡",
            ChatVerb::Exec => "📟",
            ChatVerb::Fetch => "🛰️",
            ChatVerb::Invoke => "🔌",
            ChatVerb::Agent => "🐔",
        }
    }

    /// Label for the verb
    pub fn label(&self) -> &'static str {
        match self {
            ChatVerb::Infer => "infer",
            ChatVerb::Exec => "exec",
            ChatVerb::Fetch => "fetch",
            ChatVerb::Invoke => "invoke",
            ChatVerb::Agent => "agent",
        }
    }

    /// Color for the verb (delegates to canonical VerbColor)
    pub fn color(&self) -> Color {
        self.verb_color().rgb()
    }

    /// Get the canonical VerbColor for this chat verb
    pub fn verb_color(&self) -> VerbColor {
        match self {
            ChatVerb::Infer => VerbColor::Infer,
            ChatVerb::Exec => VerbColor::Exec,
            ChatVerb::Fetch => VerbColor::Fetch,
            ChatVerb::Invoke => VerbColor::Invoke,
            ChatVerb::Agent => VerbColor::Agent,
        }
    }

    /// Prefix string for the verb
    pub fn prefix(&self) -> &'static str {
        match self {
            ChatVerb::Infer => "/infer",
            ChatVerb::Exec => "/exec",
            ChatVerb::Fetch => "/fetch",
            ChatVerb::Invoke => "/invoke",
            ChatVerb::Agent => "/agent",
        }
    }

    /// Parse verb from input text
    /// Returns (verb, remaining_text)
    pub fn from_input(input: &str) -> (ChatVerb, &str) {
        let trimmed = input.trim_start();

        // Note: These are Nika workflow verb prefixes, not shell commands
        if let Some(rest) = trimmed.strip_prefix("/exec ") {
            (ChatVerb::Exec, rest)
        } else if let Some(rest) = trimmed.strip_prefix("/fetch ") {
            (ChatVerb::Fetch, rest)
        } else if let Some(rest) = trimmed.strip_prefix("/invoke ") {
            (ChatVerb::Invoke, rest)
        } else if let Some(rest) = trimmed.strip_prefix("/agent ") {
            (ChatVerb::Agent, rest)
        } else if let Some(rest) = trimmed.strip_prefix("/infer ") {
            (ChatVerb::Infer, rest)
        } else {
            // Default to infer
            (ChatVerb::Infer, input)
        }
    }

    /// Check if input starts with this verb's prefix
    pub fn matches_prefix(input: &str) -> Option<ChatVerb> {
        let trimmed = input.trim_start();

        if trimmed.starts_with("/exec") {
            Some(ChatVerb::Exec)
        } else if trimmed.starts_with("/fetch") {
            Some(ChatVerb::Fetch)
        } else if trimmed.starts_with("/invoke") {
            Some(ChatVerb::Invoke)
        } else if trimmed.starts_with("/agent") {
            Some(ChatVerb::Agent)
        } else if trimmed.starts_with("/infer") {
            Some(ChatVerb::Infer)
        } else {
            None
        }
    }
}

/// System command types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCommand {
    Clear,
    Help,
    Model,
    Provider,
    Yaml,     // Toggle YAML view
    Thinking, // Toggle thinking display
}

impl SystemCommand {
    /// Parse system command from input
    pub fn from_input(input: &str) -> Option<SystemCommand> {
        let trimmed = input.trim();

        if trimmed == "/clear" || trimmed.starts_with("/clear ") {
            Some(SystemCommand::Clear)
        } else if trimmed == "/help" || trimmed.starts_with("/help ") {
            Some(SystemCommand::Help)
        } else if trimmed == "/model" || trimmed.starts_with("/model ") {
            Some(SystemCommand::Model)
        } else if trimmed == "/provider" || trimmed.starts_with("/provider ") {
            Some(SystemCommand::Provider)
        } else if trimmed == "/yaml" || trimmed.starts_with("/yaml ") {
            Some(SystemCommand::Yaml)
        } else if trimmed == "/thinking" || trimmed.starts_with("/thinking ") {
            Some(SystemCommand::Thinking)
        } else {
            None
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SystemCommand::Clear => "clear",
            SystemCommand::Help => "help",
            SystemCommand::Model => "model",
            SystemCommand::Provider => "provider",
            SystemCommand::Yaml => "yaml",
            SystemCommand::Thinking => "thinking",
        }
    }
}

/// Parsed input result
#[derive(Debug, Clone)]
pub enum ParsedInput {
    /// A verb command with content
    Verb { verb: ChatVerb, content: String },
    /// A system command
    System(SystemCommand),
    /// Empty input
    Empty,
    /// Partial verb prefix (still typing)
    PartialPrefix(String),
}

impl ParsedInput {
    /// Parse input text
    pub fn parse(input: &str) -> ParsedInput {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return ParsedInput::Empty;
        }

        // Check for system commands first
        if let Some(cmd) = SystemCommand::from_input(trimmed) {
            return ParsedInput::System(cmd);
        }

        // Check for partial prefix (user is typing a command)
        if trimmed.starts_with('/') && !trimmed.contains(' ') {
            return ParsedInput::PartialPrefix(trimmed.to_string());
        }

        // Parse as verb command
        let (verb, content) = ChatVerb::from_input(trimmed);
        ParsedInput::Verb {
            verb,
            content: content.to_string(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verb_from_input_default() {
        let (verb, content) = ChatVerb::from_input("Hello world");
        assert_eq!(verb, ChatVerb::Infer);
        assert_eq!(content, "Hello world");
    }

    #[test]
    fn test_verb_from_input_exec() {
        let (verb, content) = ChatVerb::from_input("/exec ls -la");
        assert_eq!(verb, ChatVerb::Exec);
        assert_eq!(content, "ls -la");
    }

    #[test]
    fn test_verb_from_input_fetch() {
        let (verb, content) = ChatVerb::from_input("/fetch https://api.example.com");
        assert_eq!(verb, ChatVerb::Fetch);
        assert_eq!(content, "https://api.example.com");
    }

    #[test]
    fn test_verb_from_input_invoke() {
        let (verb, content) = ChatVerb::from_input("/invoke novanet.describe entity:qr-code");
        assert_eq!(verb, ChatVerb::Invoke);
        assert_eq!(content, "novanet.describe entity:qr-code");
    }

    #[test]
    fn test_verb_from_input_agent() {
        let (verb, content) = ChatVerb::from_input("/agent Research AI papers");
        assert_eq!(verb, ChatVerb::Agent);
        assert_eq!(content, "Research AI papers");
    }

    #[test]
    fn test_verb_from_input_explicit_infer() {
        let (verb, content) = ChatVerb::from_input("/infer Generate a headline");
        assert_eq!(verb, ChatVerb::Infer);
        assert_eq!(content, "Generate a headline");
    }

    #[test]
    fn test_system_command_clear() {
        assert_eq!(
            SystemCommand::from_input("/clear"),
            Some(SystemCommand::Clear)
        );
    }

    #[test]
    fn test_system_command_help() {
        assert_eq!(
            SystemCommand::from_input("/help"),
            Some(SystemCommand::Help)
        );
    }

    #[test]
    fn test_parsed_input_empty() {
        matches!(ParsedInput::parse(""), ParsedInput::Empty);
        matches!(ParsedInput::parse("   "), ParsedInput::Empty);
    }

    #[test]
    fn test_parsed_input_partial_prefix() {
        match ParsedInput::parse("/ex") {
            ParsedInput::PartialPrefix(s) => assert_eq!(s, "/ex"),
            _ => panic!("Expected PartialPrefix"),
        }
    }

    #[test]
    fn test_parsed_input_system() {
        match ParsedInput::parse("/clear") {
            ParsedInput::System(cmd) => assert_eq!(cmd, SystemCommand::Clear),
            _ => panic!("Expected System"),
        }
    }

    #[test]
    fn test_parsed_input_verb() {
        match ParsedInput::parse("/exec npm build") {
            ParsedInput::Verb { verb, content } => {
                assert_eq!(verb, ChatVerb::Exec);
                assert_eq!(content, "npm build");
            }
            _ => panic!("Expected Verb"),
        }
    }

    #[test]
    fn test_verb_colors() {
        assert_eq!(ChatVerb::Infer.color(), VerbColor::Infer.rgb());
        assert_eq!(ChatVerb::Exec.color(), VerbColor::Exec.rgb());
        assert_eq!(ChatVerb::Fetch.color(), VerbColor::Fetch.rgb());
        assert_eq!(ChatVerb::Invoke.color(), VerbColor::Invoke.rgb());
        assert_eq!(ChatVerb::Agent.color(), VerbColor::Agent.rgb());
    }

    #[test]
    fn test_verb_color_mapping() {
        assert_eq!(ChatVerb::Infer.verb_color(), VerbColor::Infer);
        assert_eq!(ChatVerb::Exec.verb_color(), VerbColor::Exec);
        assert_eq!(ChatVerb::Fetch.verb_color(), VerbColor::Fetch);
        assert_eq!(ChatVerb::Invoke.verb_color(), VerbColor::Invoke);
        assert_eq!(ChatVerb::Agent.verb_color(), VerbColor::Agent);
    }

    #[test]
    fn test_verb_icons() {
        assert_eq!(ChatVerb::Infer.icon(), "⚡");
        assert_eq!(ChatVerb::Exec.icon(), "📟");
        assert_eq!(ChatVerb::Fetch.icon(), "🛰️");
        assert_eq!(ChatVerb::Invoke.icon(), "🔌");
        assert_eq!(ChatVerb::Agent.icon(), "🐔");
    }
}
