// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Semantic Tokens Handler
//!
//! Provides syntax-aware token classification for Nika workflows:
//! - Keywords: schema, workflow, tasks, mcp, context, include, with, depends_on, for_each, edges
//! - Verbs: infer, exec, fetch, invoke, agent
//! - Task IDs: declarations and references
//! - Template expressions: {{with.alias}}
//! - Providers and MCP server names
//!
//! ## Token Types
//!
//! Uses LSP SemanticTokenType constants:
//! - KEYWORD: top-level fields (schema, workflow, tasks, mcp, context)
//! - FUNCTION: verb names (infer, exec, fetch, invoke, agent)
//! - VARIABLE: task IDs in references (depends_on values, $task_id)
//! - STRING: template expressions ({{with.alias}})
//! - PROPERTY: sub-fields (id, prompt, command, url, tool, params)
//! - NAMESPACE: MCP server names, provider names
//! - COMMENT: YAML comments
//!
//! ## Architecture
//!
//! Two code paths:
//! - `compute_semantic_tokens_with_ast`: AST-aware (preferred, uses AstIndex)
//! - `compute_semantic_tokens`: Text-based fallback (regex scanning)

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

#[cfg(feature = "lsp")]
pub use tower_lsp_server::ls_types::Uri;

#[cfg(feature = "lsp")]
use super::super::ast_index::AstIndex;

/// Token type indices matching the legend order
#[cfg(feature = "lsp")]
pub const TOKEN_TYPE_KEYWORD: u32 = 0;
#[cfg(feature = "lsp")]
pub const TOKEN_TYPE_FUNCTION: u32 = 1;
#[cfg(feature = "lsp")]
pub const TOKEN_TYPE_VARIABLE: u32 = 2;
#[cfg(feature = "lsp")]
pub const TOKEN_TYPE_STRING: u32 = 3;
#[cfg(feature = "lsp")]
pub const TOKEN_TYPE_PROPERTY: u32 = 4;
#[cfg(feature = "lsp")]
pub const TOKEN_TYPE_NAMESPACE: u32 = 5;
#[cfg(feature = "lsp")]
pub const TOKEN_TYPE_COMMENT: u32 = 6;

/// Token modifier bit flags
#[cfg(feature = "lsp")]
pub const TOKEN_MOD_DECLARATION: u32 = 0b0000_0001;
#[cfg(feature = "lsp")]
pub const TOKEN_MOD_DEFINITION: u32 = 0b0000_0010;

/// Build the semantic token legend for capability registration
#[cfg(feature = "lsp")]
pub fn semantic_token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::KEYWORD,   // 0
            SemanticTokenType::FUNCTION,  // 1
            SemanticTokenType::VARIABLE,  // 2
            SemanticTokenType::STRING,    // 3
            SemanticTokenType::PROPERTY,  // 4
            SemanticTokenType::NAMESPACE, // 5
            SemanticTokenType::COMMENT,   // 6
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION, // bit 0
            SemanticTokenModifier::DEFINITION,  // bit 1
        ],
    }
}

/// A raw token before delta-encoding
#[cfg(feature = "lsp")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawToken {
    pub line: u32,
    pub start: u32,
    pub length: u32,
    pub token_type: u32,
    pub token_modifiers: u32,
}

/// Top-level YAML keywords in Nika workflows
#[cfg(feature = "lsp")]
const KEYWORDS: &[&str] = &[
    "schema", "workflow", "tasks", "mcp", "context", "include", "edges", "skills", "provider",
];

/// Task-level field keywords
#[cfg(feature = "lsp")]
const TASK_FIELDS: &[&str] = &[
    "id",
    "with",
    "depends_on",
    "for_each",
    "as",
    "model",
    "provider",
    "structured",
    "output",
    "retries",
    "timeout",
    "condition",
];

/// Verb names (the 5 Nika verbs)
#[cfg(feature = "lsp")]
const VERBS: &[&str] = &["infer", "exec", "fetch", "invoke", "agent"];

/// Sub-fields within verb blocks
#[cfg(feature = "lsp")]
const VERB_FIELDS: &[&str] = &[
    // shared / infer
    "prompt",
    "model",
    "temperature",
    "max_tokens",
    "system",
    "response_format",
    "extended_thinking",
    "thinking_budget",
    // exec
    "command",
    "shell",
    "cwd",
    "env",
    // fetch v0.35
    "url",
    "method",
    "headers",
    "body",
    "extract",
    "selector",
    "response",
    "json",
    "follow_redirects",
    // invoke
    "tool",
    "params",
    "mcp",
    "resource",
    "timeout",
    // agent
    "goal",
    "max_turns",
    "tools",
    "tool_choice",
    "depth_limit",
    "token_budget",
    "stop_sequences",
    "scope",
    "skills",
    // mcp top-level
    "servers",
    // vision v0.34
    "content",
    "source",
    "detail",
    // task-level
    "retry",
    "description",
    "artifact",
    "log",
];

/// Compute semantic tokens using text-based scanning (fallback)
///
/// Scans each line for known patterns: keywords, verbs, task IDs, templates,
/// comments, MCP server names, and provider values.
#[cfg(feature = "lsp")]
pub fn compute_semantic_tokens(text: &str) -> Vec<RawToken> {
    let mut tokens = Vec::new();

    if text.is_empty() {
        return tokens;
    }

    // Track context: are we inside mcp.servers: block
    let mut in_mcp_servers = false;
    let mut mcp_servers_indent: usize = 0;

    for (line_num, line) in text.lines().enumerate() {
        let line_num = line_num as u32;
        let trimmed = line.trim();
        let indent = line.len() - trimmed.len();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // YAML comments (full line or inline)
        if let Some(comment_start) = find_yaml_comment(line) {
            let comment_len = line.len() - comment_start;
            tokens.push(RawToken {
                line: line_num,
                start: comment_start as u32,
                length: comment_len as u32,
                token_type: TOKEN_TYPE_COMMENT,
                token_modifiers: 0,
            });
        }

        // Extract the key before ':'
        let key = extract_yaml_key(trimmed);

        // Track section context
        if indent == 0 && !trimmed.starts_with("mcp:") {
            in_mcp_servers = false;
        }

        if trimmed == "servers:" {
            in_mcp_servers = true;
            mcp_servers_indent = indent + 2; // server names are 2 deeper
        }

        // Top-level keywords
        if let Some(k) = &key {
            if indent == 0 && KEYWORDS.contains(&k.as_str()) {
                tokens.push(RawToken {
                    line: line_num,
                    start: 0,
                    length: k.len() as u32,
                    token_type: TOKEN_TYPE_KEYWORD,
                    token_modifiers: 0,
                });

                // Provider value → namespace
                if k == "provider" {
                    if let Some(val) = extract_yaml_value(trimmed) {
                        let val_start = line.find(&val).unwrap_or(0);
                        tokens.push(RawToken {
                            line: line_num,
                            start: val_start as u32,
                            length: val.len() as u32,
                            token_type: TOKEN_TYPE_NAMESPACE,
                            token_modifiers: 0,
                        });
                    }
                }
            }
        }

        // Verb detection
        if let Some(k) = &key {
            if VERBS.contains(&k.as_str()) && indent > 0 {
                let key_start = indent;
                tokens.push(RawToken {
                    line: line_num,
                    start: key_start as u32,
                    length: k.len() as u32,
                    token_type: TOKEN_TYPE_FUNCTION,
                    token_modifiers: 0,
                });
            }
        }

        // Task fields (id, with, depends_on, for_each, as, etc.)
        if let Some(k) = &key {
            let k_str = k.as_str();
            // Handle "- id:" (list item with id field)
            if trimmed.starts_with("- id:") {
                let id_start = indent + 2; // after "- "
                tokens.push(RawToken {
                    line: line_num,
                    start: id_start as u32,
                    length: 2, // "id"
                    token_type: TOKEN_TYPE_PROPERTY,
                    token_modifiers: 0,
                });

                // Task ID value → variable with declaration
                if let Some(val) = extract_yaml_value(&trimmed[2..]) {
                    // Find value position in original line
                    let val_trimmed = val.trim_matches('"').trim_matches('\'');
                    if let Some(val_pos) = line.rfind(val_trimmed) {
                        tokens.push(RawToken {
                            line: line_num,
                            start: val_pos as u32,
                            length: val_trimmed.len() as u32,
                            token_type: TOKEN_TYPE_VARIABLE,
                            token_modifiers: TOKEN_MOD_DECLARATION,
                        });
                    }
                }
            } else if TASK_FIELDS.contains(&k_str) && indent > 0 && !VERBS.contains(&k_str) {
                tokens.push(RawToken {
                    line: line_num,
                    start: indent as u32,
                    length: k.len() as u32,
                    token_type: TOKEN_TYPE_PROPERTY,
                    token_modifiers: 0,
                });

                // depends_on values → variable references
                if k_str == "depends_on" {
                    emit_depends_on_refs(trimmed, line_num, indent, &mut tokens);
                }
            }
        }

        // Verb sub-fields (prompt, command, url, tool, params, etc.)
        if let Some(k) = &key {
            if VERB_FIELDS.contains(&k.as_str()) && indent >= 6 {
                tokens.push(RawToken {
                    line: line_num,
                    start: indent as u32,
                    length: k.len() as u32,
                    token_type: TOKEN_TYPE_PROPERTY,
                    token_modifiers: 0,
                });
            }
        }

        // MCP server names (indent at server level, key ends with :, no space in key)
        if in_mcp_servers && indent == mcp_servers_indent {
            if let Some(k) = &key {
                if !k.contains(' ') {
                    tokens.push(RawToken {
                        line: line_num,
                        start: indent as u32,
                        length: k.len() as u32,
                        token_type: TOKEN_TYPE_NAMESPACE,
                        token_modifiers: 0,
                    });
                }
            }
        }

        // Template expressions: {{...}}
        emit_template_tokens(line, line_num, &mut tokens);

        // Dollar references in with: values: $task_id
        if indent > 0 && trimmed.contains(": $") {
            if let Some(dollar_pos) = line.find('$') {
                // Extract the reference after $
                let after_dollar = &line[dollar_pos + 1..];
                let ref_len = after_dollar
                    .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                    .unwrap_or(after_dollar.len());
                if ref_len > 0 {
                    tokens.push(RawToken {
                        line: line_num,
                        start: dollar_pos as u32,
                        length: (ref_len + 1) as u32, // include $
                        token_type: TOKEN_TYPE_VARIABLE,
                        token_modifiers: 0,
                    });
                }
            }
        }
    }

    tokens
}

/// Compute semantic tokens using AstIndex for AST-aware classification
///
/// Falls back to text-based scanning when no cached AST is available.
#[cfg(feature = "lsp")]
pub fn compute_semantic_tokens_with_ast(
    ast_index: &AstIndex,
    uri: &Uri,
    text: &str,
) -> Vec<RawToken> {
    // Start with text-based tokens (accurate positions from line scanning)
    let mut tokens = compute_semantic_tokens(text);

    // If we have a cached analyzed AST, enrich tokens with semantic info
    let task_names: std::collections::HashSet<String> =
        ast_index.get_task_names(uri).into_iter().collect();

    if task_names.is_empty() {
        return tokens;
    }

    // Collect source lines for text extraction
    let lines: Vec<&str> = text.lines().collect();

    // Enrich VARIABLE tokens: cross-reference with AST-known task names
    // to confirm declaration modifiers on task ID definitions
    for token in &mut tokens {
        if token.token_type != TOKEN_TYPE_VARIABLE {
            continue;
        }

        // Extract the token text from source
        let line_idx = token.line as usize;
        if line_idx >= lines.len() {
            continue;
        }
        let line = lines[line_idx];
        let start = token.start as usize;
        let end = start + token.length as usize;
        if end > line.len() {
            continue;
        }
        let token_text = &line[start..end];

        // If this token text matches an AST-known task name,
        // ensure the declaration modifier is set for id: definitions
        if task_names.contains(token_text) {
            let trimmed = line.trim();
            if trimmed.starts_with("- id:") {
                token.token_modifiers |= TOKEN_MOD_DECLARATION;
            }
        }
    }

    tokens
}

/// Delta-encode raw tokens into LSP SemanticTokens format
///
/// Sorts tokens by position, then encodes each as delta from the previous.
#[cfg(feature = "lsp")]
pub fn encode_tokens(mut tokens: Vec<RawToken>) -> Vec<SemanticToken> {
    if tokens.is_empty() {
        return Vec::new();
    }

    // Sort by line, then by start position
    tokens.sort_by(|a, b| a.line.cmp(&b.line).then(a.start.cmp(&b.start)));

    let mut result = Vec::with_capacity(tokens.len());
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for token in &tokens {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.start - prev_start
        } else {
            token.start // new line resets start
        };

        result.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.token_modifiers,
        });

        prev_line = token.line;
        prev_start = token.start;
    }

    result
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Extract the YAML key from a trimmed line (before the first ':')
#[cfg(feature = "lsp")]
fn extract_yaml_key(trimmed: &str) -> Option<String> {
    let s = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    s.find(':').map(|i| s[..i].to_string())
}

/// Extract the YAML value from a line (after ':' and whitespace)
#[cfg(feature = "lsp")]
fn extract_yaml_value(line: &str) -> Option<String> {
    let colon_pos = line.find(':')?;
    let after = line[colon_pos + 1..].trim();
    if after.is_empty() {
        None
    } else {
        Some(after.trim_matches('"').trim_matches('\'').to_string())
    }
}

/// Find the start position of a YAML comment in a line
///
/// Returns None if no comment found. Handles strings to avoid false positives.
#[cfg(feature = "lsp")]
fn find_yaml_comment(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    // Full-line comment
    if trimmed.starts_with('#') {
        return Some(line.find('#').unwrap());
    }

    // Inline comment: find # that's not inside quotes
    let mut in_single = false;
    let mut in_double = false;
    for (i, ch) in line.char_indices() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double => {
                // Must be preceded by whitespace to be a comment
                if i > 0 && line.as_bytes()[i - 1] == b' ' {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Emit template expression tokens ({{...}}) from a line
#[cfg(feature = "lsp")]
fn emit_template_tokens(line: &str, line_num: u32, tokens: &mut Vec<RawToken>) {
    let mut search_from = 0;
    while let Some(start) = line[search_from..].find("{{") {
        let abs_start = search_from + start;
        if let Some(end) = line[abs_start..].find("}}") {
            let abs_end = abs_start + end + 2; // include }}
            tokens.push(RawToken {
                line: line_num,
                start: abs_start as u32,
                length: (abs_end - abs_start) as u32,
                token_type: TOKEN_TYPE_STRING,
                token_modifiers: 0,
            });
            search_from = abs_end;
        } else {
            break;
        }
    }
}

/// Emit variable reference tokens from a depends_on value
#[cfg(feature = "lsp")]
fn emit_depends_on_refs(
    trimmed: &str,
    line_num: u32,
    base_indent: usize,
    tokens: &mut Vec<RawToken>,
) {
    // depends_on: [step1, step2] or depends_on: step1
    if let Some(colon_pos) = trimmed.find(':') {
        let after = trimmed[colon_pos + 1..].trim();
        let inner = after.trim_start_matches('[').trim_end_matches(']');
        for part in inner.split(',') {
            let ref_name = part.trim().trim_matches('"').trim_matches('\'');
            if !ref_name.is_empty() {
                // Find position in original trimmed line
                if let Some(pos) = trimmed.find(ref_name) {
                    tokens.push(RawToken {
                        line: line_num,
                        start: (base_indent + pos) as u32,
                        length: ref_name.len() as u32,
                        token_type: TOKEN_TYPE_VARIABLE,
                        token_modifiers: 0,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Legend Tests
    // ========================================================================

    #[test]
    #[cfg(feature = "lsp")]
    fn test_legend_has_all_token_types() {
        let legend = semantic_token_legend();
        assert_eq!(legend.token_types.len(), 7);
        assert_eq!(legend.token_types[0], SemanticTokenType::KEYWORD);
        assert_eq!(legend.token_types[1], SemanticTokenType::FUNCTION);
        assert_eq!(legend.token_types[2], SemanticTokenType::VARIABLE);
        assert_eq!(legend.token_types[3], SemanticTokenType::STRING);
        assert_eq!(legend.token_types[4], SemanticTokenType::PROPERTY);
        assert_eq!(legend.token_types[5], SemanticTokenType::NAMESPACE);
        assert_eq!(legend.token_types[6], SemanticTokenType::COMMENT);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_legend_has_modifiers() {
        let legend = semantic_token_legend();
        assert_eq!(legend.token_modifiers.len(), 2);
        assert_eq!(
            legend.token_modifiers[0],
            SemanticTokenModifier::DECLARATION
        );
        assert_eq!(legend.token_modifiers[1], SemanticTokenModifier::DEFINITION);
    }

    // ========================================================================
    // Delta Encoding Tests
    // ========================================================================

    #[test]
    #[cfg(feature = "lsp")]
    fn test_encode_empty_tokens() {
        let tokens = vec![];
        let encoded = encode_tokens(tokens);
        assert!(encoded.is_empty());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_encode_single_token() {
        let tokens = vec![RawToken {
            line: 0,
            start: 0,
            length: 6,
            token_type: TOKEN_TYPE_KEYWORD,
            token_modifiers: 0,
        }];
        let encoded = encode_tokens(tokens);
        assert_eq!(encoded.len(), 1);
        assert_eq!(encoded[0].delta_line, 0);
        assert_eq!(encoded[0].delta_start, 0);
        assert_eq!(encoded[0].length, 6);
        assert_eq!(encoded[0].token_type, TOKEN_TYPE_KEYWORD);
        assert_eq!(encoded[0].token_modifiers_bitset, 0);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_encode_multiple_tokens_same_line() {
        let tokens = vec![
            RawToken {
                line: 0,
                start: 0,
                length: 6,
                token_type: TOKEN_TYPE_KEYWORD,
                token_modifiers: 0,
            },
            RawToken {
                line: 0,
                start: 8,
                length: 22,
                token_type: TOKEN_TYPE_NAMESPACE,
                token_modifiers: 0,
            },
        ];
        let encoded = encode_tokens(tokens);
        assert_eq!(encoded.len(), 2);
        // First token: absolute
        assert_eq!(encoded[0].delta_line, 0);
        assert_eq!(encoded[0].delta_start, 0);
        // Second token: delta from first
        assert_eq!(encoded[1].delta_line, 0); // same line
        assert_eq!(encoded[1].delta_start, 8); // delta from 0
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_encode_multiple_tokens_different_lines() {
        let tokens = vec![
            RawToken {
                line: 0,
                start: 0,
                length: 6,
                token_type: TOKEN_TYPE_KEYWORD,
                token_modifiers: 0,
            },
            RawToken {
                line: 2,
                start: 0,
                length: 5,
                token_type: TOKEN_TYPE_KEYWORD,
                token_modifiers: 0,
            },
        ];
        let encoded = encode_tokens(tokens);
        assert_eq!(encoded.len(), 2);
        assert_eq!(encoded[1].delta_line, 2); // 2 lines down
        assert_eq!(encoded[1].delta_start, 0); // new line resets start
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_encode_preserves_modifiers() {
        let tokens = vec![RawToken {
            line: 0,
            start: 0,
            length: 5,
            token_type: TOKEN_TYPE_VARIABLE,
            token_modifiers: TOKEN_MOD_DECLARATION,
        }];
        let encoded = encode_tokens(tokens);
        assert_eq!(encoded[0].token_modifiers_bitset, TOKEN_MOD_DECLARATION);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_encode_sorts_by_position() {
        // Tokens given out of order should be sorted
        let tokens = vec![
            RawToken {
                line: 2,
                start: 0,
                length: 5,
                token_type: TOKEN_TYPE_KEYWORD,
                token_modifiers: 0,
            },
            RawToken {
                line: 0,
                start: 0,
                length: 6,
                token_type: TOKEN_TYPE_KEYWORD,
                token_modifiers: 0,
            },
        ];
        let encoded = encode_tokens(tokens);
        assert_eq!(encoded.len(), 2);
        // First should be line 0 (sorted)
        assert_eq!(encoded[0].delta_line, 0);
        assert_eq!(encoded[0].length, 6);
        // Second should be line 2
        assert_eq!(encoded[1].delta_line, 2);
        assert_eq!(encoded[1].length, 5);
    }

    // ========================================================================
    // Text-Based Semantic Token Tests
    // ========================================================================

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_schema_keyword() {
        let text = "schema: nika/workflow@0.12\n";
        let tokens = compute_semantic_tokens(text);
        // "schema" should be a keyword at line 0, col 0, len 6
        let schema_token = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_KEYWORD && t.line == 0);
        assert!(schema_token.is_some(), "Should find schema keyword");
        let tok = schema_token.unwrap();
        assert_eq!(tok.start, 0);
        assert_eq!(tok.length, 6);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_workflow_keyword() {
        let text = "schema: nika/workflow@0.12\nworkflow: my-test\n";
        let tokens = compute_semantic_tokens(text);
        let wf_token = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_KEYWORD && t.line == 1 && t.start == 0);
        assert!(wf_token.is_some(), "Should find workflow keyword");
        assert_eq!(wf_token.unwrap().length, 8); // "workflow"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_tasks_keyword() {
        let text = "schema: nika/workflow@0.12\nworkflow: test\n\ntasks:\n  - id: step1\n";
        let tokens = compute_semantic_tokens(text);
        let tasks_tok = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_KEYWORD && t.line == 3);
        assert!(tasks_tok.is_some(), "Should find tasks keyword");
        assert_eq!(tasks_tok.unwrap().length, 5); // "tasks"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_verb_infer() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    infer: \"Generate\"\n";
        let tokens = compute_semantic_tokens(text);
        let verb_tok = tokens.iter().find(|t| t.token_type == TOKEN_TYPE_FUNCTION);
        assert!(verb_tok.is_some(), "Should find infer verb");
        assert_eq!(verb_tok.unwrap().length, 5); // "infer"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_all_five_verbs() {
        let text = r#"schema: nika/workflow@0.12
tasks:
  - id: t1
    infer: "Hello"
  - id: t2
    exec: "echo hello"
  - id: t3
    fetch:
      url: "https://example.com"
  - id: t4
    invoke:
      mcp: novanet
      tool: novanet_search
  - id: t5
    agent:
      goal: "Research"
"#;
        let tokens = compute_semantic_tokens(text);
        let verb_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.token_type == TOKEN_TYPE_FUNCTION)
            .collect();
        // Should find all 5 verbs
        assert_eq!(verb_tokens.len(), 5, "Should find all 5 verbs");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_task_id_declaration() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: my_task\n    infer: \"test\"\n";
        let tokens = compute_semantic_tokens(text);
        // Task ID "my_task" should be a variable with declaration modifier
        let id_tok = tokens.iter().find(|t| {
            t.token_type == TOKEN_TYPE_VARIABLE && t.token_modifiers & TOKEN_MOD_DECLARATION != 0
        });
        assert!(id_tok.is_some(), "Should find task ID declaration");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_task_id_field() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    infer: \"test\"\n";
        let tokens = compute_semantic_tokens(text);
        // "id" itself should be a property
        let id_field = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_PROPERTY && t.line == 2);
        assert!(id_field.is_some(), "Should find 'id' as property");
        assert_eq!(id_field.unwrap().length, 2); // "id"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_depends_on_reference() {
        let text = r#"schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    depends_on: [step1]
    infer: "World"
"#;
        let tokens = compute_semantic_tokens(text);
        // "step1" in depends_on should be a variable reference (no declaration modifier)
        let ref_tok = tokens.iter().find(|t| {
            t.token_type == TOKEN_TYPE_VARIABLE
                && t.token_modifiers & TOKEN_MOD_DECLARATION == 0
                && t.line == 5
        });
        assert!(
            ref_tok.is_some(),
            "Should find task reference in depends_on"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_template_expression() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: s2\n    with:\n      title: s1\n    infer: \"Write about {{with.title}}\"\n";
        let tokens = compute_semantic_tokens(text);
        // {{with.title}} should be a string token
        let tmpl_tok = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_STRING && t.line == 5);
        assert!(tmpl_tok.is_some(), "Should find template expression");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_with_keyword() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: s1\n    with:\n      data: prev\n    infer: \"test\"\n";
        let tokens = compute_semantic_tokens(text);
        let with_tok = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_PROPERTY && t.line == 3);
        assert!(with_tok.is_some(), "Should find 'with' as property");
        assert_eq!(with_tok.unwrap().length, 4); // "with"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_mcp_keyword() {
        let text =
            "schema: nika/workflow@0.12\nmcp:\n  servers:\n    novanet:\n      command: cargo\n";
        let tokens = compute_semantic_tokens(text);
        let mcp_tok = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_KEYWORD && t.line == 1);
        assert!(mcp_tok.is_some(), "Should find mcp keyword");
        assert_eq!(mcp_tok.unwrap().length, 3); // "mcp"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_mcp_server_name() {
        let text =
            "schema: nika/workflow@0.12\nmcp:\n  servers:\n    novanet:\n      command: cargo\n";
        let tokens = compute_semantic_tokens(text);
        let server_tok = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_NAMESPACE && t.line == 3);
        assert!(
            server_tok.is_some(),
            "Should find MCP server name as namespace"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_provider_namespace() {
        let text = "schema: nika/workflow@0.12\nprovider: anthropic\n";
        let tokens = compute_semantic_tokens(text);
        // "provider" should be keyword, "anthropic" should be namespace
        let ns_tok = tokens.iter().find(|t| t.token_type == TOKEN_TYPE_NAMESPACE);
        assert!(ns_tok.is_some(), "Should find provider value as namespace");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_yaml_comment() {
        let text = "# This is a comment\nschema: nika/workflow@0.12\n";
        let tokens = compute_semantic_tokens(text);
        let comment_tok = tokens.iter().find(|t| t.token_type == TOKEN_TYPE_COMMENT);
        assert!(comment_tok.is_some(), "Should find YAML comment");
        assert_eq!(comment_tok.unwrap().line, 0);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_for_each_keyword() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: t1\n    for_each: [a, b]\n    as: item\n    infer: \"Process {{with.item}}\"\n";
        let tokens = compute_semantic_tokens(text);
        let fe_tok = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_PROPERTY && t.line == 3);
        assert!(fe_tok.is_some(), "Should find for_each as property");
        assert_eq!(fe_tok.unwrap().length, 8); // "for_each"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_verb_sub_fields() {
        let text = r#"schema: nika/workflow@0.12
tasks:
  - id: t1
    invoke:
      mcp: novanet
      tool: novanet_search
      params:
        query: "test"
"#;
        let tokens = compute_semantic_tokens(text);
        // "mcp", "tool", "params" inside invoke should be properties
        let prop_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.token_type == TOKEN_TYPE_PROPERTY && t.line >= 4)
            .collect();
        assert!(
            prop_tokens.len() >= 3,
            "Should find verb sub-fields as properties"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_empty_document() {
        let tokens = compute_semantic_tokens("");
        assert!(tokens.is_empty());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_minimal_workflow() {
        let text = "schema: nika/workflow@0.12\n";
        let tokens = compute_semantic_tokens(text);
        // Should at least have the schema keyword
        assert!(!tokens.is_empty());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_inline_comment() {
        let text = "schema: nika/workflow@0.12 # schema version\ntasks:\n";
        let tokens = compute_semantic_tokens(text);
        let comment_tok = tokens.iter().find(|t| t.token_type == TOKEN_TYPE_COMMENT);
        assert!(comment_tok.is_some(), "Should find inline comment");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_multiple_template_expressions() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: s1\n    infer: \"{{with.a}} and {{with.b}}\"\n";
        let tokens = compute_semantic_tokens(text);
        let tmpl_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.token_type == TOKEN_TYPE_STRING && t.line == 3)
            .collect();
        assert_eq!(
            tmpl_tokens.len(),
            2,
            "Should find both template expressions"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_context_keyword() {
        let text = "schema: nika/workflow@0.12\ncontext:\n  files:\n    data: ./data.json\n";
        let tokens = compute_semantic_tokens(text);
        let ctx_tok = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_KEYWORD && t.line == 1);
        assert!(ctx_tok.is_some(), "Should find context keyword");
        assert_eq!(ctx_tok.unwrap().length, 7); // "context"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_dollar_reference() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: s1\n    infer: \"Hello\"\n  - id: s2\n    with:\n      prev: $s1\n    infer: \"Use {{with.prev}}\"\n";
        let tokens = compute_semantic_tokens(text);
        // $s1 should be variable reference
        let dollar_tok = tokens
            .iter()
            .find(|t| t.token_type == TOKEN_TYPE_VARIABLE && t.line == 6);
        assert!(dollar_tok.is_some(), "Should find $task_id reference");
    }

    // ========================================================================
    // AST-Aware Semantic Token Tests
    // ========================================================================

    #[test]
    #[cfg(feature = "lsp")]
    fn test_ast_tokens_basic_workflow() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: step1
    infer: "Generate content"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        ast_index.parse_document(&uri, text, 0);

        let tokens = compute_semantic_tokens_with_ast(&ast_index, &uri, text);
        assert!(!tokens.is_empty(), "AST-aware should produce tokens");

        // Should find keywords, verb, task ID
        let has_keyword = tokens.iter().any(|t| t.token_type == TOKEN_TYPE_KEYWORD);
        let has_verb = tokens.iter().any(|t| t.token_type == TOKEN_TYPE_FUNCTION);
        assert!(has_keyword, "Should find keywords");
        assert!(has_verb, "Should find verb");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_ast_tokens_fallback_when_no_cache() {
        let text = "schema: nika/workflow@0.12\nworkflow: test\n";
        let uri = "file:///uncached.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        // Deliberately NOT parsing into AST index

        let tokens = compute_semantic_tokens_with_ast(&ast_index, &uri, text);
        // Should fall back to text-based and still produce tokens
        assert!(!tokens.is_empty(), "Should fall back to text-based tokens");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_ast_tokens_with_mcp_servers() {
        let text = r#"schema: nika/workflow@0.12
mcp:
  servers:
    novanet:
      command: cargo
    perplexity:
      command: npx
tasks:
  - id: search
    invoke:
      mcp: novanet
      tool: novanet_search
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        ast_index.parse_document(&uri, text, 0);

        let tokens = compute_semantic_tokens_with_ast(&ast_index, &uri, text);
        let ns_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.token_type == TOKEN_TYPE_NAMESPACE)
            .collect();
        // Should find MCP server names
        assert!(
            !ns_tokens.is_empty(),
            "Should find namespace tokens for MCP servers"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_ast_tokens_task_id_with_declaration() {
        let text = r#"schema: nika/workflow@0.12
tasks:
  - id: generate
    infer: "Hello"
  - id: process
    depends_on: [generate]
    exec: "echo done"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        ast_index.parse_document(&uri, text, 0);

        let tokens = compute_semantic_tokens_with_ast(&ast_index, &uri, text);

        // Task ID declarations should have declaration modifier
        let decl_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| {
                t.token_type == TOKEN_TYPE_VARIABLE
                    && t.token_modifiers & TOKEN_MOD_DECLARATION != 0
            })
            .collect();
        assert!(
            decl_tokens.len() >= 2,
            "Should find at least 2 task ID declarations"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_v035_fetch_sub_fields() {
        let text = r#"schema: nika/workflow@0.12
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: markdown
      selector: ".main"
      response: full
      follow_redirects: true
"#;
        let tokens = compute_semantic_tokens(text);
        let lines: Vec<&str> = text.lines().collect();
        let prop_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.token_type == TOKEN_TYPE_PROPERTY && t.line >= 4)
            .collect();
        // Extract token text for verification
        let prop_names: Vec<&str> = prop_tokens
            .iter()
            .map(|t| {
                let line = lines[t.line as usize];
                &line[t.start as usize..(t.start + t.length) as usize]
            })
            .collect();
        for expected in &["url", "extract", "selector", "response", "follow_redirects"] {
            assert!(
                prop_names.contains(expected),
                "Should find '{}' as property, found: {:?}",
                expected,
                prop_names
            );
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_tokens_vision_and_agent_sub_fields() {
        let text = r#"schema: nika/workflow@0.12
tasks:
  - id: see
    infer:
      content:
        - type: image
          source: "abc123"
          detail: high
      extended_thinking: true
      thinking_budget: 8192
  - id: act
    agent:
      goal: "Research"
      tools: [nika:read]
      tool_choice: auto
      depth_limit: 3
      scope: project
"#;
        let tokens = compute_semantic_tokens(text);
        let lines: Vec<&str> = text.lines().collect();
        let prop_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.token_type == TOKEN_TYPE_PROPERTY)
            .collect();
        let prop_names: Vec<&str> = prop_tokens
            .iter()
            .map(|t| {
                let line = lines[t.line as usize];
                &line[t.start as usize..(t.start + t.length) as usize]
            })
            .collect();
        for expected in &[
            "content",
            "source",
            "detail",
            "extended_thinking",
            "thinking_budget",
            "tools",
            "tool_choice",
            "depth_limit",
            "scope",
        ] {
            assert!(
                prop_names.contains(expected),
                "Should find '{}' as property, found: {:?}",
                expected,
                prop_names
            );
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_ast_enrichment_declaration_modifiers_match_task_names() {
        // Workflow with 3 tasks: the AST enrichment should confirm declaration
        // modifiers only for tokens whose text matches AST-known task names
        let text = r#"schema: nika/workflow@0.12
workflow: pipeline
tasks:
  - id: fetch_data
    exec: "curl http://example.com"
  - id: transform
    depends_on: [fetch_data]
    exec: "jq '.data'"
  - id: publish
    depends_on: [transform]
    model: gpt-4o-mini
    infer: "Summarize the results"
"#;
        let uri = "file:///enrichment-test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        ast_index.parse_document(&uri, text, 0);

        // Verify AST knows all 3 task names
        let task_names = ast_index.get_task_names(&uri);
        assert_eq!(task_names.len(), 3, "AST should know 3 task names");
        assert!(task_names.contains(&"fetch_data".to_string()));
        assert!(task_names.contains(&"transform".to_string()));
        assert!(task_names.contains(&"publish".to_string()));

        let tokens = compute_semantic_tokens_with_ast(&ast_index, &uri, text);
        let lines: Vec<&str> = text.lines().collect();

        // Collect all VARIABLE tokens and extract their text
        let var_tokens: Vec<_> = tokens
            .iter()
            .filter(|t| t.token_type == TOKEN_TYPE_VARIABLE)
            .collect();

        // Declaration tokens: VARIABLE + TOKEN_MOD_DECLARATION set
        let decl_tokens: Vec<_> = var_tokens
            .iter()
            .filter(|t| t.token_modifiers & TOKEN_MOD_DECLARATION != 0)
            .collect();

        // Should have exactly 3 task ID declarations (fetch_data, transform, publish)
        assert_eq!(
            decl_tokens.len(),
            3,
            "Should find exactly 3 task ID declarations, got: {:?}",
            decl_tokens
        );

        // Each declaration token text should match an AST-known task name
        for tok in &decl_tokens {
            let line_text = lines[tok.line as usize];
            let tok_text = &line_text[tok.start as usize..(tok.start + tok.length) as usize];
            assert!(
                task_names.contains(&tok_text.to_string()),
                "Declaration token '{}' should be an AST-known task name",
                tok_text
            );
        }

        // Reference tokens: VARIABLE with NO declaration modifier
        let ref_tokens: Vec<_> = var_tokens
            .iter()
            .filter(|t| t.token_modifiers & TOKEN_MOD_DECLARATION == 0)
            .collect();

        // Should have exactly 2 references (fetch_data in depends_on, transform in depends_on)
        assert_eq!(
            ref_tokens.len(),
            2,
            "Should find exactly 2 task ID references, got: {:?}",
            ref_tokens
        );

        // Each reference token text should also match an AST-known task name
        for tok in &ref_tokens {
            let line_text = lines[tok.line as usize];
            let tok_text = &line_text[tok.start as usize..(tok.start + tok.length) as usize];
            assert!(
                task_names.contains(&tok_text.to_string()),
                "Reference token '{}' should be an AST-known task name",
                tok_text
            );
        }
    }
}
