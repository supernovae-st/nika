// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb-specific argument parsers
//!
//! Each Nika verb (`/fetch`, `/invoke`, `/agent`, `/model`, `/mcp`, `/export`)
//! has its own argument parser that returns a `Command` variant.

use super::{Command, ExportFormat, McpAction, ModelProvider};

impl Command {
    /// Parse /fetch arguments: /fetch <url> [method]
    /// Smart error detection with helpful hints
    pub(super) fn parse_fetch_args(args: &str) -> Command {
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
                        error: format!("❌ '{}' is not needed!", first),
                        hint: "Nika handles HTTP for you. Syntax: /fetch <url>".to_string(),
                        example: format!("💡 /fetch {}", rest),
                    };
                }
            }
            // "http" or "https" without "://"
            if (first == "http" || first == "https") && !args.contains("://") {
                return Command::FetchError {
                    error: "❌ Malformed URL (missing ://)".to_string(),
                    hint: "URL must include the full protocol".to_string(),
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
                    error: format!("❌ Method '{}' but no URL", url.to_uppercase()),
                    hint: "Put the URL before the method, or just the URL (GET by default)"
                        .to_string(),
                    example: "💡 /fetch https://httpbin.org/get".to_string(),
                };
            }

            return Command::FetchError {
                error: "❌ Invalid URL (no http:// or https://)".to_string(),
                hint: format!("You typed: '{}'", url),
                example: "💡 /fetch https://catfact.ninja/fact".to_string(),
            };
        }

        // Validate method if provided
        let method = method_str.unwrap_or_else(|| "GET".to_string());
        const VALID_METHODS: &[&str] =
            &["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"];

        if !VALID_METHODS.contains(&method.as_str()) {
            return Command::FetchError {
                error: format!("❌ Unknown HTTP method '{}'", method),
                hint: format!("Valid methods: {}", VALID_METHODS.join(", ")),
                example: format!("💡 /fetch {} GET", url),
            };
        }

        Command::Fetch { url, method }
    }

    /// Parse /invoke arguments: /invoke [server:]tool [json_params]
    pub(super) fn parse_invoke_args(args: &str) -> Command {
        let args = args.trim();

        if args.is_empty() {
            return Command::InvokeError {
                error: "Missing tool name".to_string(),
                hint: "Usage: /invoke [server:]tool [json_params]".to_string(),
                example: "/invoke novanet:describe {\"entity\":\"qr-code\"}".to_string(),
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

    /// Parse /model arguments: /model <provider>
    pub(super) fn parse_model_args(args: &str) -> Command {
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
            "native" | "local" => Command::Model {
                provider: ModelProvider::Native,
            },
            "list" | "" => Command::Model {
                provider: ModelProvider::List,
            },
            _ => Command::Model {
                provider: ModelProvider::List,
            },
        }
    }

    /// Parse /export arguments: /export [json|yaml] [path]
    /// Added yaml format for workflow export
    pub(super) fn parse_export_args(args: &str) -> Command {
        let parts: Vec<&str> = args.split_whitespace().collect();

        match parts.as_slice() {
            // /export (no args) -> JSON to auto-generated path
            [] => Command::Export {
                format: ExportFormat::Json,
                path: None,
            },
            // /export json or /export yaml
            [format] if format.eq_ignore_ascii_case("json") => Command::Export {
                format: ExportFormat::Json,
                path: None,
            },
            [format] if format.eq_ignore_ascii_case("yaml") => Command::Export {
                format: ExportFormat::Yaml,
                path: None,
            },
            // /export path.json (infer format from extension)
            [path] if path.ends_with(".json") => Command::Export {
                format: ExportFormat::Json,
                path: Some((*path).to_string()),
            },
            // /export path.yaml or path.nika.yaml
            [path] if path.ends_with(".yaml") || path.ends_with(".yml") => Command::Export {
                format: ExportFormat::Yaml,
                path: Some((*path).to_string()),
            },
            // /export path (no extension) -> JSON default
            [path] => Command::Export {
                format: ExportFormat::Json,
                path: Some((*path).to_string()),
            },
            // /export json path or /export yaml path
            [format, path] if format.eq_ignore_ascii_case("json") => Command::Export {
                format: ExportFormat::Json,
                path: Some((*path).to_string()),
            },
            [format, path] if format.eq_ignore_ascii_case("yaml") => Command::Export {
                format: ExportFormat::Yaml,
                path: Some((*path).to_string()),
            },
            // Default: treat everything as path, JSON format
            _ => Command::Export {
                format: ExportFormat::Json,
                path: Some(args.to_string()),
            },
        }
    }

    /// Parse /agent arguments: /agent <goal> [--max-turns N] [--mcp server1,server2]
    pub(super) fn parse_agent_args(args: &str) -> Command {
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
    pub(super) fn parse_mcp_args(args: &str) -> Command {
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
}
