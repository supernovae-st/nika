// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Semantic token handler — syntax coloring.

#[derive(Debug, Clone)]
pub struct RawToken {
    pub line: u32,
    pub start_char: u32,
    pub length: u32,
    pub token_type: TokenType,
    pub modifiers: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Keyword,
    Verb,
    Property,
    Variable,
    Function,
    Type,
    Comment,
}

impl TokenType {
    pub fn index(self) -> u32 {
        match self {
            Self::Keyword => 0,
            Self::Verb => 1,
            Self::Property => 2,
            Self::Variable => 3,
            Self::Function => 4,
            Self::Type => 5,
            Self::Comment => 6,
        }
    }
}

const VERBS: &[&str] = &["infer", "exec", "fetch", "invoke", "agent"];
const TOP_KEYS: &[&str] = &[
    "schema",
    "workflow",
    "tasks",
    "mcp",
    "context",
    "inputs",
    "include",
    "edges",
    "skills",
    "agents",
    "artifacts",
    "log",
];
const TASK_KEYS: &[&str] = &[
    "id",
    "with",
    "depends_on",
    "content",
    "for_each",
    "retry",
    "timeout",
    "guardrails",
    "guard",
    "limits",
    "provider",
    "model",
    "description",
    "structured",
    "output",
    "on_error",
    "as",
    "concurrency",
    "fail_fast",
    "artifact",
    "decompose",
    "skills",
    "completion",
    "log",
];
const VERB_SUBKEYS: &[&str] = &[
    "prompt",
    "system",
    "temperature",
    "max_tokens",
    "thinking",
    "extended_thinking",
    "thinking_budget",
    "command",
    "shell",
    "cwd",
    "working_dir",
    "url",
    "method",
    "headers",
    "body",
    "extract",
    "selector",
    "response",
    "follow_redirects",
    "mcp",
    "server",
    "tool",
    "params",
    "resource",
    "max_turns",
    "max_iterations",
    "depth_limit",
    "tools",
    "goal",
    "detail",
    "source",
    "json",
    "env",
    "response_format",
    "tool_choice",
    "scope",
    "from",
    "token_budget",
    "stop_sequences",
    // Retry sub-fields
    "max_attempts",
    "delay",
    "delay_ms",
    "backoff",
    "backoff_multiplier",
    // Decompose sub-fields
    "strategy",
    "traverse",
    "mcp_server",
    "max_items",
    "max_depth",
    // Guardrails sub-fields
    "judge_prompt",
    "judge_model",
    "pass_pattern",
    "on_failure",
    "negate",
    "pattern",
    "signal",
    "confidence",
    "mode",
    "on_limit_reached",
    "max_cost_usd",
    "max_duration_secs",
    "save_progress",
    // Content part fields (note: "type" has special handling as TokenType::Type)
    "text",
];

pub fn semantic_tokens(text: &str) -> Vec<RawToken> {
    let mut tokens = Vec::new();
    for (li, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let indent = (line.len() - trimmed.len()) as u32;
        if trimmed.starts_with('#') {
            tokens.push(RawToken {
                line: li as u32,
                start_char: indent,
                length: trimmed.len() as u32,
                token_type: TokenType::Comment,
                modifiers: 0,
            });
            continue;
        }
        // Templates
        let mut s = 0;
        while let Some(start) = line[s..].find("{{") {
            let abs = s + start;
            if let Some(end) = line[abs..].find("}}") {
                tokens.push(RawToken {
                    line: li as u32,
                    start_char: abs as u32,
                    length: (end + 2) as u32,
                    token_type: TokenType::Variable,
                    modifiers: 0,
                });
                if let Some(p) = line[abs + 2..abs + end].find('|') {
                    let t = line[abs + 2 + p + 1..abs + end].trim();
                    if !t.is_empty() {
                        let ts = abs
                            + 2
                            + p
                            + 1
                            + (line[abs + 2 + p + 1..abs + end].len()
                                - line[abs + 2 + p + 1..abs + end].trim_start().len());
                        tokens.push(RawToken {
                            line: li as u32,
                            start_char: ts as u32,
                            length: t.len() as u32,
                            token_type: TokenType::Function,
                            modifiers: 0,
                        });
                    }
                }
                s = abs + end + 2;
            } else {
                break;
            }
        }
        if let Some(cp) = trimmed.find(':') {
            let key = trimmed[..cp].trim().trim_start_matches("- ");
            if key.is_empty() {
                continue;
            }
            let ks = line.find(key).unwrap_or(indent as usize);
            let tt = if VERBS.contains(&key) {
                TokenType::Verb
            } else if indent == 0 && TOP_KEYS.contains(&key) {
                TokenType::Keyword
            } else if TASK_KEYS.contains(&key) || VERB_SUBKEYS.contains(&key) {
                TokenType::Property
            } else if key == "type" {
                TokenType::Type
            } else {
                continue;
            };
            tokens.push(RawToken {
                line: li as u32,
                start_char: ks as u32,
                length: key.len() as u32,
                token_type: tt,
                modifiers: 0,
            });
        }
    }
    tokens
}

pub fn encode_tokens(tokens: &[RawToken]) -> Vec<u32> {
    let mut sorted: Vec<_> = tokens.to_vec();
    sorted.sort_by_key(|t| (t.line, t.start_char));
    let mut data = Vec::with_capacity(sorted.len() * 5);
    let (mut pl, mut ps) = (0u32, 0u32);
    for t in &sorted {
        let dl = t.line - pl;
        let ds = if dl == 0 {
            t.start_char - ps
        } else {
            t.start_char
        };
        data.extend_from_slice(&[dl, ds, t.length, t.token_type.index(), t.modifiers]);
        pl = t.line;
        ps = t.start_char;
    }
    data
}

pub fn token_legend() -> Vec<&'static str> {
    vec![
        "keyword", "macro", "property", "variable", "function", "type", "comment",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments() {
        assert!(semantic_tokens("# hi\n")
            .iter()
            .any(|t| t.token_type == TokenType::Comment));
    }
    #[test]
    fn verbs() {
        assert!(semantic_tokens("    infer: x")
            .iter()
            .any(|t| t.token_type == TokenType::Verb));
    }
    #[test]
    fn templates() {
        assert!(semantic_tokens("  x: \"{{with.d}}\"")
            .iter()
            .any(|t| t.token_type == TokenType::Variable));
    }
    #[test]
    fn encode() {
        let d = encode_tokens(&[RawToken {
            line: 0,
            start_char: 0,
            length: 5,
            token_type: TokenType::Keyword,
            modifiers: 0,
        }]);
        assert_eq!(d.len(), 5);
    }
    #[test]
    fn empty() {
        assert!(semantic_tokens("").is_empty());
    }
    #[test]
    fn all_verbs() {
        for v in VERBS {
            assert!(semantic_tokens(&format!("    {v}: x"))
                .iter()
                .any(|t| t.token_type == TokenType::Verb));
        }
    }
}
