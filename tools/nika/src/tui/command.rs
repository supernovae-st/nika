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
    /// /infer <prompt> - Direct LLM inference
    Infer { prompt: String },

    /// /exec <command> - Shell execution
    Exec { command: String },

    /// /fetch <url> [method] - HTTP request
    Fetch { url: String, method: String },

    /// /fetch error - user made a common mistake (v0.8.2)
    FetchError {
        error: String,
        hint: String,
        example: String,
    },

    /// /invoke [server:]tool [json_params] - MCP tool call
    Invoke {
        tool: String,
        server: Option<String>,
        params: serde_json::Value,
    },

    /// /agent <goal> [--max-turns N] [--mcp server1,server2] - Multi-turn agentic loop
    Agent {
        goal: String,
        max_turns: Option<u32>,
        /// MCP servers to use for this agent (v0.5.2)
        mcp_servers: Vec<String>,
    },

    /// Plain chat message (default)
    Chat { message: String },

    /// /help or /? - Show help
    Help,

    /// /model <provider> - Switch LLM provider (openai, claude)
    Model { provider: ModelProvider },

    /// /clear - Clear chat history
    Clear,

    /// /mcp [list|select|toggle] - MCP server management (v0.5.2)
    Mcp { action: McpAction },

    /// /export [path] - Export chat session to JSON file (v0.9)
    Export { path: Option<String> },
}

/// Available LLM providers via rig-core (v0.6: expanded to 6 providers)
#[derive(Debug, Clone, PartialEq)]
pub enum ModelProvider {
    /// OpenAI (gpt-4o, gpt-4, etc.)
    OpenAI,
    /// Anthropic Claude (claude-sonnet-4, etc.)
    Claude,
    /// Mistral AI (mistral-large-latest, etc.)
    Mistral,
    /// Groq (llama-3.3-70b-versatile, etc.)
    Groq,
    /// DeepSeek (deepseek-chat, etc.)
    DeepSeek,
    /// Ollama (local models)
    Ollama,
    /// List available providers
    List,
}

impl Command {
    /// Parse user input into a Command
    ///
    /// # Examples
    ///
    /// ```
    /// use nika::tui::command::Command;
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
                "/export" => Command::Export {
                    path: if args.is_empty() {
                        None
                    } else {
                        Some(args.to_string())
                    },
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

    /// Parse /fetch arguments: /fetch <url> [method]
    /// v0.8.2: Smart error detection with helpful hints
    fn parse_fetch_args(args: &str) -> Command {
        let args = args.trim();

        // Empty args
        if args.is_empty() {
            return Command::FetchError {
                error: "❌ URL manquante".to_string(),
                hint: "Syntaxe: /fetch <url> [method]".to_string(),
                example: "💡 /fetch https://catfact.ninja/fact".to_string(),
            };
        }

        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        let first = parts.first().unwrap_or(&"").to_lowercase();

        // Common mistake: user typed "curl" first
        if first == "curl" || first == "wget" || first == "http" || first == "https" {
            // Check if they typed "curl https://..."
            if first == "curl" || first == "wget" {
                let rest = parts.get(1).map(|s| s.trim()).unwrap_or("");
                if rest.starts_with("http://") || rest.starts_with("https://") {
                    return Command::FetchError {
                        error: format!("❌ '{}' n'est pas nécessaire!", first),
                        hint: "Nika fait le HTTP pour toi. Syntaxe: /fetch <url>".to_string(),
                        example: format!("💡 /fetch {}", rest),
                    };
                }
            }
            // "http" or "https" without "://"
            if (first == "http" || first == "https") && !args.contains("://") {
                return Command::FetchError {
                    error: "❌ URL malformée (manque ://)".to_string(),
                    hint: "L'URL doit inclure le protocole complet".to_string(),
                    example: "💡 /fetch https://api.github.com/zen".to_string(),
                };
            }
        }

        let url = parts.first().unwrap_or(&"").to_string();
        let method_str = parts.get(1).map(|s| s.trim().to_uppercase());

        // Check URL has scheme
        if !url.starts_with("http://") && !url.starts_with("https://") {
            // Maybe they put method first? e.g., "GET https://..."
            if ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"]
                .contains(&url.to_uppercase().as_str())
            {
                if let Some(actual_url) = method_str.as_ref() {
                    if actual_url.starts_with("HTTP://") || actual_url.starts_with("HTTPS://") {
                        // Swap: method was first, URL second
                        return Command::Fetch {
                            url: actual_url.to_string(),
                            method: url.to_uppercase(),
                        };
                    }
                }
                return Command::FetchError {
                    error: format!("❌ Méthode '{}' mais pas d'URL", url.to_uppercase()),
                    hint: "Mets l'URL avant la méthode, ou juste l'URL (GET par défaut)"
                        .to_string(),
                    example: "💡 /fetch https://httpbin.org/get".to_string(),
                };
            }

            return Command::FetchError {
                error: "❌ URL invalide (pas de http:// ou https://)".to_string(),
                hint: format!("Tu as tapé: '{}'", url),
                example: "💡 /fetch https://catfact.ninja/fact".to_string(),
            };
        }

        // Validate method if provided
        let method = method_str.unwrap_or_else(|| "GET".to_string());
        const VALID_METHODS: &[&str] =
            &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

        if !VALID_METHODS.contains(&method.as_str()) {
            return Command::FetchError {
                error: format!("❌ Méthode HTTP '{}' inconnue", method),
                hint: format!("Méthodes valides: {}", VALID_METHODS.join(", ")),
                example: format!("💡 /fetch {} GET", url),
            };
        }

        Command::Fetch { url, method }
    }

    /// Parse /invoke arguments: /invoke [server:]tool [json_params]
    fn parse_invoke_args(args: &str) -> Command {
        let args = args.trim();

        if args.is_empty() {
            return Command::Invoke {
                tool: String::new(),
                server: None,
                params: serde_json::Value::Object(serde_json::Map::new()),
            };
        }

        // Find where the JSON params start (first '{')
        let (tool_spec, json_str) = if let Some(json_start) = args.find('{') {
            let tool_spec = args[..json_start].trim();
            let json_str = &args[json_start..];
            (tool_spec, Some(json_str))
        } else {
            // No JSON params, entire args is tool spec
            let parts: Vec<&str> = args.splitn(2, ' ').collect();
            // Safe: splitn on non-empty string returns at least one element
            (parts.first().copied().unwrap_or(""), None)
        };

        // Parse server:tool or just tool
        let (server, tool) = if tool_spec.contains(':') {
            let tp: Vec<&str> = tool_spec.splitn(2, ':').collect();
            // Safe: splitn(2, ':') on string containing ':' returns at least 2 elements
            let server_part = tp.first().copied().unwrap_or("");
            let tool_part = tp.get(1).copied().unwrap_or("");
            (Some(server_part.to_string()), tool_part.to_string())
        } else {
            (None, tool_spec.to_string())
        };

        // Parse JSON params if present
        let params = json_str
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        Command::Invoke {
            tool,
            server,
            params,
        }
    }

    /// Parse /model arguments: /model <provider> (v0.6: 6 providers)
    fn parse_model_args(args: &str) -> Command {
        let provider = args.trim().to_lowercase();
        match provider.as_str() {
            "openai" | "gpt" | "gpt-4" | "gpt-4o" => Command::Model {
                provider: ModelProvider::OpenAI,
            },
            "claude" | "anthropic" | "sonnet" => Command::Model {
                provider: ModelProvider::Claude,
            },
            "mistral" | "mistral-large" => Command::Model {
                provider: ModelProvider::Mistral,
            },
            "groq" | "llama" | "llama3" => Command::Model {
                provider: ModelProvider::Groq,
            },
            "deepseek" => Command::Model {
                provider: ModelProvider::DeepSeek,
            },
            "ollama" | "local" => Command::Model {
                provider: ModelProvider::Ollama,
            },
            "list" | "" => Command::Model {
                provider: ModelProvider::List,
            },
            _ => Command::Model {
                provider: ModelProvider::List,
            },
        }
    }

    /// Parse /agent arguments: /agent <goal> [--max-turns N] [--mcp server1,server2]
    fn parse_agent_args(args: &str) -> Command {
        let args = args.trim();
        let mut goal = args.to_string();
        let mut max_turns = None;
        let mut mcp_servers = Vec::new();

        // Parse --mcp servers (can be anywhere in the string)
        if let Some(mcp_idx) = args.find("--mcp") {
            let before_mcp = &args[..mcp_idx];
            let after_mcp = &args[mcp_idx + 5..]; // Skip "--mcp"

            // Extract servers until next -- or end
            let servers_str = if let Some(next_flag) = after_mcp.find(" --") {
                &after_mcp[..next_flag]
            } else {
                after_mcp
            };

            // Parse comma-separated servers
            mcp_servers = servers_str
                .trim()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Rebuild goal without --mcp part
            goal = before_mcp.to_string();
            if let Some(next_flag) = after_mcp.find(" --") {
                goal.push_str(&after_mcp[next_flag..]);
            }
        }

        // Parse --max-turns (from potentially modified goal)
        if let Some(turns_idx) = goal.find("--max-turns") {
            let before_turns = &goal[..turns_idx];
            let after_turns = &goal[turns_idx + 11..]; // Skip "--max-turns"

            // Extract number until next -- or end
            let turns_str = if let Some(next_flag) = after_turns.find(" --") {
                &after_turns[..next_flag]
            } else {
                after_turns
            };

            max_turns = turns_str
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok());

            // Rebuild goal without --max-turns part
            goal = before_turns.trim().to_string();
            // Don't append remaining since we've already processed --mcp
        }

        Command::Agent {
            goal: goal.trim().to_string(),
            max_turns,
            mcp_servers,
        }
    }

    /// Parse /mcp arguments: /mcp [list|select|toggle] [servers]
    fn parse_mcp_args(args: &str) -> Command {
        let args = args.trim();

        if args.is_empty() || args == "list" {
            return Command::Mcp {
                action: McpAction::List,
            };
        }

        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        // Safe: splitn on non-empty args returns at least one element
        let action = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
        let server_args = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match action.as_str() {
            "select" => {
                let servers: Vec<String> = server_args
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                Command::Mcp {
                    action: McpAction::Select(servers),
                }
            }
            "toggle" => Command::Mcp {
                action: McpAction::Toggle(server_args.to_string()),
            },
            _ => Command::Mcp {
                action: McpAction::List,
            },
        }
    }

    /// Get the verb name for display
    pub fn verb(&self) -> &'static str {
        match self {
            Command::Infer { .. } => "infer",
            Command::Exec { .. } => "exec",
            Command::Fetch { .. } | Command::FetchError { .. } => "fetch",
            Command::Invoke { .. } => "invoke",
            Command::Agent { .. } => "agent",
            Command::Chat { .. } => "chat",
            Command::Help => "help",
            Command::Model { .. } => "model",
            Command::Clear => "clear",
            Command::Mcp { .. } => "mcp",
            Command::Export { .. } => "export",
        }
    }

    /// Check if this command is an error (v0.8.2)
    pub fn is_error(&self) -> bool {
        matches!(self, Command::FetchError { .. })
    }

    /// Check if this is an empty command (empty input)
    pub fn is_empty(&self) -> bool {
        match self {
            Command::Chat { message } => message.is_empty(),
            Command::Infer { prompt } => prompt.is_empty(),
            Command::Exec { command } => command.is_empty(),
            Command::Fetch { url, .. } => url.is_empty(),
            Command::FetchError { .. } => false, // Errors are not "empty"
            Command::Invoke { tool, .. } => tool.is_empty(),
            Command::Agent { goal, .. } => goal.is_empty(),
            Command::Help => false,
            Command::Model { .. } => false,
            Command::Clear => false,
            Command::Mcp { .. } => false,
            Command::Export { .. } => false, // Export is never "empty"
        }
    }
}

impl ModelProvider {
    /// Get the display name for the provider (v0.6: 6 providers)
    pub fn name(&self) -> &'static str {
        match self {
            ModelProvider::OpenAI => "OpenAI (gpt-4o)",
            ModelProvider::Claude => "Anthropic Claude (claude-sonnet-4)",
            ModelProvider::Mistral => "Mistral AI (mistral-large)",
            ModelProvider::Groq => "Groq (llama-3.3-70b)",
            ModelProvider::DeepSeek => "DeepSeek (deepseek-chat)",
            ModelProvider::Ollama => "Ollama (local)",
            ModelProvider::List => "list",
        }
    }

    /// Get the short command name for the provider (used with /model <name>)
    pub fn command_name(&self) -> &'static str {
        match self {
            ModelProvider::OpenAI => "openai",
            ModelProvider::Claude => "claude",
            ModelProvider::Mistral => "mistral",
            ModelProvider::Groq => "groq",
            ModelProvider::DeepSeek => "deepseek",
            ModelProvider::Ollama => "ollama",
            ModelProvider::List => "list",
        }
    }

    /// Get the environment variable name required for this provider
    pub fn env_var(&self) -> &'static str {
        match self {
            ModelProvider::OpenAI => "OPENAI_API_KEY",
            ModelProvider::Claude => "ANTHROPIC_API_KEY",
            ModelProvider::Mistral => "MISTRAL_API_KEY",
            ModelProvider::Groq => "GROQ_API_KEY",
            ModelProvider::DeepSeek => "DEEPSEEK_API_KEY",
            ModelProvider::Ollama => "OLLAMA_API_BASE_URL",
            ModelProvider::List => "",
        }
    }

    /// Check if the provider is available (env var is set and non-empty)
    pub fn is_available(&self) -> bool {
        match self {
            ModelProvider::List => true,
            ModelProvider::Ollama => true, // Ollama is always "available" (localhost fallback)
            _ => std::env::var(self.env_var()).is_ok_and(|v| !v.is_empty()),
        }
    }

    /// Convert provider name string to ModelProvider enum (v0.12.2)
    ///
    /// Used by Provider Modal SelectProvider action to switch providers.
    /// Returns None for invalid provider names.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "claude" | "anthropic" => Some(ModelProvider::Claude),
            "openai" | "gpt" => Some(ModelProvider::OpenAI),
            "mistral" => Some(ModelProvider::Mistral),
            "groq" => Some(ModelProvider::Groq),
            "deepseek" => Some(ModelProvider::DeepSeek),
            "ollama" => Some(ModelProvider::Ollama),
            _ => None,
        }
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
  /mcp [list|select|toggle] MCP server management (v0.5.2)
  /model <provider>         Switch LLM (openai, claude, mistral, groq, deepseek, ollama)
  /export [path]            Export chat to JSON file (v0.9)
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

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════
    // /infer tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_infer_command() {
        let input = "/infer explain this code";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Infer { prompt } if prompt == "explain this code"));
    }

    #[test]
    fn test_parse_infer_empty_prompt() {
        let input = "/infer";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Infer { prompt } if prompt.is_empty()));
    }

    #[test]
    fn test_parse_infer_with_extra_spaces() {
        let input = "/infer   explain this code  ";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Infer { prompt } if prompt == "explain this code"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // /exec tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_exec_command() {
        let input = "/exec cargo test";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Exec { command } if command == "cargo test"));
    }

    #[test]
    fn test_parse_exec_with_pipes() {
        let input = "/exec ls -la | grep .rs";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Exec { command } if command == "ls -la | grep .rs"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // /fetch tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_fetch_get() {
        let input = "/fetch https://api.example.com";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Fetch { url, method }
            if url == "https://api.example.com" && method == "GET"
        ));
    }

    #[test]
    fn test_parse_fetch_post() {
        let input = "/fetch https://api.example.com POST";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Fetch { url, method }
            if url == "https://api.example.com" && method == "POST"
        ));
    }

    #[test]
    fn test_parse_fetch_lowercase_method() {
        let input = "/fetch https://api.example.com post";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Fetch { url, method }
            if url == "https://api.example.com" && method == "POST"
        ));
    }

    // v0.8.2: FetchError tests for smart error detection
    #[test]
    fn test_parse_fetch_error_curl() {
        let input = "/fetch curl https://api.example.com";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::FetchError { error, .. } if error.contains("curl")));
    }

    #[test]
    fn test_parse_fetch_error_no_scheme() {
        let input = "/fetch api.example.com";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::FetchError { error, .. } if error.contains("http")));
    }

    #[test]
    fn test_parse_fetch_error_empty() {
        let input = "/fetch";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::FetchError { error, .. } if error.contains("manquante")));
    }

    #[test]
    fn test_parse_fetch_error_invalid_method() {
        let input = "/fetch https://api.example.com POAST";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::FetchError { error, .. } if error.contains("POAST")));
    }

    #[test]
    fn test_parse_fetch_method_first_swaps() {
        // User typed method before URL - we handle it!
        let input = "/fetch GET https://api.example.com";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Fetch { url, method }
            if url == "HTTPS://API.EXAMPLE.COM" && method == "GET"
        ));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // /invoke tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_invoke_simple() {
        let input = "/invoke describe";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Invoke { tool, server, params }
            if tool == "describe" && server.is_none() && params.is_object()
        ));
    }

    #[test]
    fn test_parse_invoke_with_server() {
        let input = "/invoke novanet:describe";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Invoke { tool, server, .. }
            if tool == "describe" && server == Some("novanet".to_string())
        ));
    }

    #[test]
    fn test_parse_invoke_with_json_params() {
        let input = r#"/invoke novanet:describe {"entity":"qr-code"}"#;
        let cmd = Command::parse(input);
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

    #[test]
    fn test_parse_invoke_empty() {
        let input = "/invoke";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Invoke { tool, server, params }
            if tool.is_empty() && server.is_none() && params.is_object()
        ));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // /agent tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_agent_simple() {
        let input = "/agent generate a landing page";
        let cmd = Command::parse(input);
        if let Command::Agent {
            goal,
            max_turns,
            mcp_servers,
        } = cmd
        {
            assert_eq!(goal, "generate a landing page");
            assert_eq!(max_turns, None);
            assert!(mcp_servers.is_empty());
        } else {
            panic!("Expected Command::Agent");
        }
    }

    #[test]
    fn test_parse_agent_with_max_turns() {
        let input = "/agent generate a landing page --max-turns 5";
        let cmd = Command::parse(input);
        if let Command::Agent {
            goal,
            max_turns,
            mcp_servers,
        } = cmd
        {
            assert_eq!(goal, "generate a landing page");
            assert_eq!(max_turns, Some(5));
            assert!(mcp_servers.is_empty());
        } else {
            panic!("Expected Command::Agent");
        }
    }

    #[test]
    fn test_parse_agent_max_turns_at_start() {
        let input = "/agent --max-turns 3 do something";
        let cmd = Command::parse(input);
        // The goal should be empty (before --max-turns)
        // "3 do something" parses as 3 using split_whitespace().next()
        if let Command::Agent {
            goal,
            max_turns,
            mcp_servers,
        } = cmd
        {
            assert!(goal.is_empty());
            assert_eq!(max_turns, Some(3));
            assert!(mcp_servers.is_empty());
        } else {
            panic!("Expected Command::Agent");
        }
    }

    #[test]
    fn test_parse_agent_max_turns_only() {
        // When --max-turns is followed by only a number, it should parse correctly
        let input = "/agent --max-turns 10";
        let cmd = Command::parse(input);
        if let Command::Agent {
            goal,
            max_turns,
            mcp_servers,
        } = cmd
        {
            assert!(goal.is_empty());
            assert_eq!(max_turns, Some(10));
            assert!(mcp_servers.is_empty());
        } else {
            panic!("Expected Command::Agent");
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // /help tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_help() {
        let input = "/help";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Help));
    }

    #[test]
    fn test_parse_question_mark_help() {
        let input = "/?";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Help));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Chat message tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_plain_message() {
        let input = "hello world";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Chat { message } if message == "hello world"));
    }

    #[test]
    fn test_parse_empty_message() {
        let input = "";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Chat { message } if message.is_empty()));
    }

    #[test]
    fn test_parse_whitespace_message() {
        let input = "   ";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Chat { message } if message.is_empty()));
    }

    #[test]
    fn test_parse_unknown_command_as_chat() {
        let input = "/unknown some text";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Chat { message } if message == "/unknown some text"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // /model tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_model_openai() {
        let input = "/model openai";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::OpenAI
            }
        ));
    }

    #[test]
    fn test_parse_model_claude() {
        let input = "/model claude";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::Claude
            }
        ));
    }

    #[test]
    fn test_parse_model_gpt_alias() {
        let input = "/model gpt";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::OpenAI
            }
        ));
    }

    #[test]
    fn test_parse_model_anthropic_alias() {
        let input = "/model anthropic";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::Claude
            }
        ));
    }

    #[test]
    fn test_parse_model_list() {
        let input = "/model list";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::List
            }
        ));
    }

    #[test]
    fn test_parse_model_empty() {
        let input = "/model";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::List
            }
        ));
    }

    #[test]
    fn test_model_provider_name() {
        assert_eq!(ModelProvider::OpenAI.name(), "OpenAI (gpt-4o)");
        assert_eq!(
            ModelProvider::Claude.name(),
            "Anthropic Claude (claude-sonnet-4)"
        );
        assert_eq!(ModelProvider::Mistral.name(), "Mistral AI (mistral-large)");
        assert_eq!(ModelProvider::Groq.name(), "Groq (llama-3.3-70b)");
        assert_eq!(ModelProvider::DeepSeek.name(), "DeepSeek (deepseek-chat)");
        assert_eq!(ModelProvider::Ollama.name(), "Ollama (local)");
    }

    #[test]
    fn test_model_provider_env_var() {
        assert_eq!(ModelProvider::OpenAI.env_var(), "OPENAI_API_KEY");
        assert_eq!(ModelProvider::Claude.env_var(), "ANTHROPIC_API_KEY");
        assert_eq!(ModelProvider::Mistral.env_var(), "MISTRAL_API_KEY");
        assert_eq!(ModelProvider::Groq.env_var(), "GROQ_API_KEY");
        assert_eq!(ModelProvider::DeepSeek.env_var(), "DEEPSEEK_API_KEY");
        assert_eq!(ModelProvider::Ollama.env_var(), "OLLAMA_API_BASE_URL");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // v0.6 provider tests (new providers)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_model_mistral() {
        let input = "/model mistral";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::Mistral
            }
        ));
    }

    #[test]
    fn test_parse_model_groq() {
        let input = "/model groq";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::Groq
            }
        ));
    }

    #[test]
    fn test_parse_model_deepseek() {
        let input = "/model deepseek";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::DeepSeek
            }
        ));
    }

    #[test]
    fn test_parse_model_ollama() {
        let input = "/model ollama";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::Ollama
            }
        ));
    }

    #[test]
    fn test_parse_model_llama_alias() {
        // llama maps to Groq
        let input = "/model llama";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::Groq
            }
        ));
    }

    #[test]
    fn test_parse_model_local_alias() {
        // local maps to Ollama
        let input = "/model local";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Model {
                provider: ModelProvider::Ollama
            }
        ));
    }

    #[test]
    fn test_ollama_always_available() {
        // Ollama should always be available (localhost fallback)
        assert!(ModelProvider::Ollama.is_available());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // /clear tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_clear() {
        let input = "/clear";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Clear));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // /agent with --mcp tests (v0.5.2)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_agent_with_mcp_servers() {
        let input = "/agent generate a landing page --mcp novanet,perplexity";
        let cmd = Command::parse(input);
        if let Command::Agent {
            goal,
            max_turns,
            mcp_servers,
        } = cmd
        {
            assert_eq!(goal, "generate a landing page");
            assert_eq!(max_turns, None);
            assert_eq!(mcp_servers, vec!["novanet", "perplexity"]);
        } else {
            panic!("Expected Command::Agent");
        }
    }

    #[test]
    fn test_parse_agent_with_mcp_and_max_turns() {
        let input = "/agent generate a landing page --mcp novanet --max-turns 5";
        let cmd = Command::parse(input);
        if let Command::Agent {
            goal,
            max_turns,
            mcp_servers,
        } = cmd
        {
            assert_eq!(goal, "generate a landing page");
            assert_eq!(max_turns, Some(5));
            assert_eq!(mcp_servers, vec!["novanet"]);
        } else {
            panic!("Expected Command::Agent");
        }
    }

    #[test]
    fn test_parse_agent_with_single_mcp_server() {
        let input = "/agent do something --mcp novanet";
        let cmd = Command::parse(input);
        if let Command::Agent { mcp_servers, .. } = cmd {
            assert_eq!(mcp_servers, vec!["novanet"]);
        } else {
            panic!("Expected Command::Agent");
        }
    }

    #[test]
    fn test_parse_agent_mcp_order_reversed() {
        // --max-turns before --mcp should also work
        let input = "/agent do something --max-turns 3 --mcp novanet,perplexity";
        let cmd = Command::parse(input);
        if let Command::Agent {
            max_turns,
            mcp_servers,
            ..
        } = cmd
        {
            assert_eq!(max_turns, Some(3));
            assert_eq!(mcp_servers, vec!["novanet", "perplexity"]);
        } else {
            panic!("Expected Command::Agent");
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // /mcp command tests (v0.5.2)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_mcp_list() {
        let input = "/mcp";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Mcp {
                action: McpAction::List
            }
        ));
    }

    #[test]
    fn test_parse_mcp_list_explicit() {
        let input = "/mcp list";
        let cmd = Command::parse(input);
        assert!(matches!(
            cmd,
            Command::Mcp {
                action: McpAction::List
            }
        ));
    }

    #[test]
    fn test_parse_mcp_select() {
        let input = "/mcp select novanet,perplexity";
        let cmd = Command::parse(input);
        if let Command::Mcp {
            action: McpAction::Select(servers),
        } = cmd
        {
            assert_eq!(servers, vec!["novanet", "perplexity"]);
        } else {
            panic!("Expected Command::Mcp with Select action");
        }
    }

    #[test]
    fn test_parse_mcp_toggle() {
        let input = "/mcp toggle novanet";
        let cmd = Command::parse(input);
        if let Command::Mcp {
            action: McpAction::Toggle(server),
        } = cmd
        {
            assert_eq!(server, "novanet");
        } else {
            panic!("Expected Command::Mcp with Toggle action");
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Case insensitivity tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_uppercase_infer() {
        let input = "/INFER explain this";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Infer { prompt } if prompt == "explain this"));
    }

    #[test]
    fn test_parse_mixed_case_exec() {
        let input = "/ExEc cargo test";
        let cmd = Command::parse(input);
        assert!(matches!(cmd, Command::Exec { command } if command == "cargo test"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Helper method tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_verb_names() {
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
                mcp_servers: vec![]
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
        assert_eq!(
            Command::Mcp {
                action: McpAction::List
            }
            .verb(),
            "mcp"
        );
    }

    #[test]
    fn test_is_empty() {
        assert!(Command::Chat { message: "".into() }.is_empty());
        assert!(!Command::Chat {
            message: "hi".into()
        }
        .is_empty());
        assert!(Command::Infer { prompt: "".into() }.is_empty());
        assert!(!Command::Help.is_empty());
        assert!(!Command::Model {
            provider: ModelProvider::OpenAI
        }
        .is_empty());
        assert!(!Command::Clear.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // ModelProvider::from_name tests (v0.12.2)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn test_model_provider_from_name_claude() {
        assert_eq!(
            ModelProvider::from_name("claude"),
            Some(ModelProvider::Claude)
        );
        assert_eq!(
            ModelProvider::from_name("anthropic"),
            Some(ModelProvider::Claude)
        );
        assert_eq!(
            ModelProvider::from_name("CLAUDE"),
            Some(ModelProvider::Claude)
        );
    }

    #[test]
    fn test_model_provider_from_name_openai() {
        assert_eq!(
            ModelProvider::from_name("openai"),
            Some(ModelProvider::OpenAI)
        );
        assert_eq!(ModelProvider::from_name("gpt"), Some(ModelProvider::OpenAI));
    }

    #[test]
    fn test_model_provider_from_name_all_providers() {
        assert_eq!(
            ModelProvider::from_name("mistral"),
            Some(ModelProvider::Mistral)
        );
        assert_eq!(ModelProvider::from_name("groq"), Some(ModelProvider::Groq));
        assert_eq!(
            ModelProvider::from_name("deepseek"),
            Some(ModelProvider::DeepSeek)
        );
        assert_eq!(
            ModelProvider::from_name("ollama"),
            Some(ModelProvider::Ollama)
        );
    }

    #[test]
    fn test_model_provider_from_name_invalid() {
        assert_eq!(ModelProvider::from_name("invalid"), None);
        assert_eq!(ModelProvider::from_name(""), None);
        assert_eq!(ModelProvider::from_name("list"), None);
    }
}
