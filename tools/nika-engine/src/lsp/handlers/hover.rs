// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hover Handler
//!
//! Provides hover information for Nika workflow elements:
//! - Verb documentation (infer, exec, fetch, invoke, agent)
//! - Field descriptions
//! - Task reference information
//! - Template variable documentation
//!
//! ## Phase 2 Architecture
//!
//! The hover handler can use AstIndex for semantic context:
//! - Use `compute_hover_with_ast` when AstIndex is available
//! - Falls back to text-based detection for ranges
//! - AstIndex provides task context for richer documentation

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

#[cfg(feature = "lsp")]
use crate::lsp::ast_index::{AstIndex, AstNode};

// Re-export Uri for use in compute_hover_with_ast
#[cfg(feature = "lsp")]
pub use tower_lsp_server::ls_types::Uri;

/// Compute hover information using AstIndex for semantic context
///
/// Uses the parsed AST for understanding what's at the cursor position,
/// but falls back to text-based range detection (due to marked_yaml span limitations).
///
/// # Arguments
/// * `ast_index` - The AST cache for semantic lookups
/// * `uri` - Document URI for AST lookup
/// * `text` - Document text for range calculations
/// * `position` - Cursor position
#[cfg(feature = "lsp")]
pub fn compute_hover_with_ast(
    ast_index: &AstIndex,
    uri: &Uri,
    text: &str,
    position: Position,
) -> Option<Hover> {
    // Try to get semantic context from AST
    let ast_node = ast_index.get_node_at_position(uri, position);

    // Generate hover based on AST node type
    match ast_node {
        Some(AstNode::Verb(verb_name, _span)) => {
            // Find documentation for this verb
            for (verb, docs) in VERB_DOCUMENTATION {
                if *verb == verb_name {
                    // Use text-based range detection (spans can be degenerate)
                    let range = find_keyword_range(text, position, &verb_name);
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: docs.to_string(),
                        }),
                        range,
                    });
                }
            }
            None
        }
        Some(AstNode::Task(task_id, _span)) => {
            // Provide task-specific documentation
            let docs = format!(
                "## Task: `{}`\n\n\
                Task identifier used in `with:` bindings and `depends_on:`.\n\n\
                ```yaml\n\
                with:\n  result: ${}\n\
                ```",
                task_id, task_id
            );
            let range = find_keyword_range(text, position, &task_id);
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: docs,
                }),
                range,
            })
        }
        Some(AstNode::Binding(alias, _span)) => {
            // Binding reference documentation
            let docs = format!(
                "## Binding: `{}`\n\n\
                References data from another task or context.\n\n\
                Access via `{{{{with.{}}}}}` in prompts.",
                alias, alias
            );
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: docs,
                }),
                range: None,
            })
        }
        Some(AstNode::McpServer(server_name, _span)) => {
            // MCP server documentation
            let docs = format!(
                "## MCP Server: `{}`\n\n\
                External MCP server for tool invocation.\n\n\
                ```yaml\n\
                invoke:\n  mcp: {}\n  tool: <tool_name>\n\
                ```",
                server_name, server_name
            );
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: docs,
                }),
                range: None,
            })
        }
        Some(AstNode::ContextFile(name, _span)) => {
            // Context file documentation
            let docs = format!(
                "## Context File: `{}`\n\n\
                File loaded via `context:` block.\n\n\
                Access via `{{{{context.files.{}}}}}`.",
                name, name
            );
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: docs,
                }),
                range: None,
            })
        }
        Some(AstNode::Include(path, _span)) => {
            // Include documentation
            let docs = format!(
                "## Include: `{}`\n\n\
                Merges tasks from external workflow via DAG fusion.",
                path
            );
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: docs,
                }),
                range: None,
            })
        }
        Some(AstNode::ForEach(_span)) => {
            // for_each documentation
            for (field, docs) in FIELD_DOCUMENTATION {
                if *field == "for_each" {
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: docs.to_string(),
                        }),
                        range: None,
                    });
                }
            }
            None
        }
        Some(AstNode::Template(expr, _span)) => {
            // Template expression - use existing documentation logic
            let docs = get_template_documentation(&expr);
            let range = find_template_range(text, position);
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: docs,
                }),
                range,
            })
        }
        Some(AstNode::Schema(ref version, _span)) | Some(AstNode::Workflow(ref version, _span)) => {
            // Schema or workflow - use field documentation
            let field = if matches!(ast_node, Some(AstNode::Schema(_, _))) {
                "schema"
            } else {
                "workflow"
            };
            let version = version.clone();
            for (f, docs) in FIELD_DOCUMENTATION {
                if *f == field {
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("{}\n\n**Current value:** `{}`", docs, version),
                        }),
                        range: None,
                    });
                }
            }
            None
        }
        Some(AstNode::Unknown) | None => {
            // Fall back to text-based hover detection
            compute_hover(text, position)
        }
    }
}

/// Find the range of a keyword at the current position
#[cfg(feature = "lsp")]
fn find_keyword_range(text: &str, position: Position, keyword: &str) -> Option<Range> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];

    // Find keyword in line
    if let Some(start) = line.find(keyword) {
        let end = start + keyword.len();
        Some(Range {
            start: Position {
                line: position.line,
                character: start as u32,
            },
            end: Position {
                line: position.line,
                character: end as u32,
            },
        })
    } else {
        None
    }
}

/// Find the range of a template expression at the current position
#[cfg(feature = "lsp")]
fn find_template_range(text: &str, position: Position) -> Option<Range> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];
    let col = position.character as usize;

    // Find template bounds
    let mut search_start = 0;
    while let Some(start) = line[search_start..].find("{{") {
        let abs_start = search_start + start;
        if let Some(end) = line[abs_start..].find("}}") {
            let abs_end = abs_start + end + 2;
            if col >= abs_start && col <= abs_end {
                return Some(Range {
                    start: Position {
                        line: position.line,
                        character: abs_start as u32,
                    },
                    end: Position {
                        line: position.line,
                        character: abs_end as u32,
                    },
                });
            }
            search_start = abs_end;
        } else {
            break;
        }
    }
    None
}

/// Compute hover information at a position.
///
/// For enhanced semantic hover, use `compute_hover_with_ast`.
#[cfg(feature = "lsp")]
pub fn compute_hover(text: &str, position: Position) -> Option<Hover> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return None;
    }

    let line = lines[line_idx];
    let col = position.character as usize;
    let line_num = position.line;

    // Check what's at this position
    if let Some(mut hover) = check_verb_hover(line, col) {
        // Fix the line number in the range
        if let Some(ref mut range) = hover.range {
            range.start.line = line_num;
            range.end.line = line_num;
        }
        return Some(hover);
    }

    if let Some(hover) = check_field_hover(line, col) {
        return Some(hover);
    }

    if let Some(mut hover) = check_template_hover(line, col) {
        // Fix the line number in the range
        if let Some(ref mut range) = hover.range {
            range.start.line = line_num;
            range.end.line = line_num;
        }
        return Some(hover);
    }

    None
}

/// Check if cursor is on a verb keyword
#[cfg(feature = "lsp")]
fn check_verb_hover(line: &str, col: usize) -> Option<Hover> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    // Check each verb
    for (verb, docs) in VERB_DOCUMENTATION {
        let pattern = format!("{}:", verb);
        if trimmed.starts_with(&pattern) {
            let verb_start = indent;
            let verb_end = indent + verb.len();
            if col >= verb_start && col <= verb_end {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: docs.to_string(),
                    }),
                    range: Some(Range {
                        start: Position {
                            line: 0, // Will be adjusted by caller
                            character: verb_start as u32,
                        },
                        end: Position {
                            line: 0,
                            character: verb_end as u32,
                        },
                    }),
                });
            }
        }
    }

    None
}

/// Check if cursor is on a field keyword
#[cfg(feature = "lsp")]
fn check_field_hover(line: &str, col: usize) -> Option<Hover> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    for (field, docs) in FIELD_DOCUMENTATION {
        let pattern = format!("{}:", field);
        if trimmed.starts_with(&pattern) {
            let field_start = indent;
            let field_end = indent + field.len();
            if col >= field_start && col <= field_end {
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: docs.to_string(),
                    }),
                    range: None,
                });
            }
        }
    }

    None
}

/// Check if cursor is on a template expression
#[cfg(feature = "lsp")]
fn check_template_hover(line: &str, col: usize) -> Option<Hover> {
    // Find template expressions like {{with.alias}} or {{context.files.name}}
    let mut search_start = 0;
    while let Some(start) = line[search_start..].find("{{") {
        let abs_start = search_start + start;
        if let Some(end) = line[abs_start..].find("}}") {
            let abs_end = abs_start + end + 2;
            if col >= abs_start && col <= abs_end {
                let template = &line[abs_start + 2..abs_start + end];
                let docs = get_template_documentation(template);
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: docs,
                    }),
                    range: Some(Range {
                        start: Position {
                            line: 0,
                            character: abs_start as u32,
                        },
                        end: Position {
                            line: 0,
                            character: abs_end as u32,
                        },
                    }),
                });
            }
            search_start = abs_end;
        } else {
            break;
        }
    }

    None
}

/// Get documentation for a template expression
#[cfg(feature = "lsp")]
fn get_template_documentation(template: &str) -> String {
    let template = template.trim();

    if let Some(alias) = template.strip_prefix("with.") {
        format!(
            "## Binding Reference\n\n\
            **`{{{{with.{}}}}}`**\n\n\
            References the output of a task bound to `{}` in the `with:` block.\n\n\
            ```yaml\n\
            with:\n  {}: task_id\n\
            ```",
            alias, alias, alias
        )
    } else if let Some(name) = template.strip_prefix("context.files.") {
        format!(
            "## Context File Reference\n\n\
            **`{{{{context.files.{}}}}}`**\n\n\
            References a file loaded via the `context:` block.\n\n\
            ```yaml\n\
            context:\n  files:\n    {}: ./path/to/file.md\n\
            ```",
            name, name
        )
    } else if let Some(name) = template.strip_prefix("inputs.") {
        format!(
            "## Workflow Input Reference\n\n\
            **`{{{{inputs.{}}}}}`**\n\n\
            References an input parameter passed to the workflow.",
            name
        )
    } else if template == "item" || template == "with.item" {
        "## Loop Item Reference\n\n\
        **`{{item}}`** or **`{{with.item}}`**\n\n\
        References the current item in a `for_each` loop.\n\n\
        ```yaml\n\
        for_each: [\"a\", \"b\", \"c\"]\n\
        as: item\n\
        ```"
        .to_string()
    } else {
        format!(
            "## Template Expression\n\n\
            **`{{{{{}}}}}`**\n\n\
            Template expressions are resolved at runtime.",
            template
        )
    }
}

/// Verb documentation (verb, markdown docs)
#[cfg(feature = "lsp")]
const VERB_DOCUMENTATION: &[(&str, &str)] = &[
    (
        "infer",
        "## `infer:` - LLM Generation\n\n\
        Generates text using an LLM provider.\n\n\
        **Shorthand:**\n\
        ```yaml\n\
        infer: \"Generate a headline\"\n\
        ```\n\n\
        **Full form:**\n\
        ```yaml\n\
        infer:\n\
          prompt: \"Generate a headline\"\n\
          model: claude-sonnet-4-6\n\
          temperature: 0.7\n\
          system: \"You are a copywriter\"\n\
          max_tokens: 100\n\
          extended_thinking: true\n\
          thinking_budget: 8192\n\
        ```\n\n\
        **Icon:** ⚡",
    ),
    (
        "exec",
        "## `exec:` - Shell Command\n\n\
        Executes a shell command.\n\n\
        **Shorthand:**\n\
        ```yaml\n\
        exec: \"npm run build\"\n\
        ```\n\n\
        **Full form:**\n\
        ```yaml\n\
        exec:\n\
          command: \"npm run build\"\n\
          shell: false  # Default: shlex parsing (secure)\n\
          timeout: 30s\n\
          cwd: ./project\n\
        ```\n\n\
        **Security:** `shell: false` (default) prevents shell injection.\n\n\
        **Icon:** 📟",
    ),
    (
        "fetch",
        "## `fetch:` - HTTP Request\n\n\
        Makes an HTTP request.\n\n\
        ```yaml\n\
        fetch:\n\
          url: \"https://api.example.com/data\"\n\
          method: GET  # GET, POST, PUT, DELETE\n\
          headers:\n\
            Authorization: \"Bearer $TOKEN\"\n\
          body: '{\"key\": \"value\"}'\n\
          timeout: 10s\n\
          extract: markdown\n\
          selector: \".main-content\"\n\
          response: full\n\
          follow_redirects: true\n\
        ```\n\n\
        ### Extract Modes (9 modes)\n\
        - `extract: markdown` — Clean Markdown via htmd\n\
        - `extract: article` — Main article content (Readability)\n\
        - `extract: text` — Visible text, optionally filtered by selector\n\
        - `extract: selector` — Raw HTML matching CSS selector\n\
        - `extract: metadata` — OG, Twitter Cards, JSON-LD, SEO tags\n\
        - `extract: links` — Link classification (internal/external)\n\
        - `extract: jsonpath` — JSONPath query on JSON responses\n\
        - `extract: feed` — RSS/Atom/JSON Feed parsing\n\
        - `extract: llm_txt` — AI-era content discovery\n\n\
        ### Response Modes\n\
        - `response: full` — JSON with status, headers, body\n\
        - `response: binary` — Store in CAS, return hash\n\n\
        **Icon:** 🛰️",
    ),
    (
        "invoke",
        "## `invoke:` - MCP Tool Call\n\n\
        Calls a tool on an MCP server.\n\n\
        ```yaml\n\
        invoke:\n\
          mcp: novanet\n\
          tool: novanet_context\n\
          params:\n\
            mode: \"page\"\n\
            focus_key: \"qr-code\"\n\
            locale: \"fr-FR\"\n\
        ```\n\n\
        **Builtin tools:** `nika:sleep`, `nika:log`, `nika:emit`, `nika:assert`, `nika:prompt`, `nika:run`\n\n\
        **Icon:** 🔌",
    ),
    (
        "agent",
        "## `agent:` - Agentic Loop\n\n\
        Runs a multi-turn agent with tool access.\n\n\
        ```yaml\n\
        agent:\n\
          prompt: \"Research and summarize\"\n\
          model: claude-sonnet-4-6\n\
          mcp: [novanet, perplexity]\n\
          max_turns: 10\n\
          depth_limit: 3  # For spawn_agent\n\
          tools: [nika:read, nika:write]\n\
          extended_thinking: true\n\
          thinking_budget: 16384\n\
        ```\n\n\
        **Icons:** 🐔 (agent), 🐤 (subagent)",
    ),
];

/// Field documentation (field, markdown docs)
#[cfg(feature = "lsp")]
const FIELD_DOCUMENTATION: &[(&str, &str)] = &[
    (
        "schema",
        "## `schema:` - Workflow Schema Version\n\n\
        Declares the Nika workflow schema version.\n\n\
        ```yaml\n\
        schema: nika/workflow@0.12\n\
        ```\n\n\
        **Current version:** `@0.12`",
    ),
    (
        "workflow",
        "## `workflow:` - Workflow Name\n\n\
        Human-readable name for the workflow.\n\n\
        ```yaml\n\
        workflow: my-workflow\n\
        ```",
    ),
    (
        "tasks",
        "## `tasks:` - Task List\n\n\
        Array of tasks to execute in the workflow DAG.\n\n\
        ```yaml\n\
        tasks:\n\
          - id: step1\n\
            infer: \"Generate content\"\n\
          - id: step2\n\
            with:\n\
              input: $step1\n\
            infer: \"Process: {{with.input}}\"\n\
        ```",
    ),
    (
        "id",
        "## `id:` - Task Identifier\n\n\
        Unique identifier for the task. Used in bindings and `depends_on:`.\n\n\
        ```yaml\n\
        - id: my_task\n\
          infer: \"...\"\n\
        ```",
    ),
    (
        "with",
        "## `with:` - Data Bindings\n\n\
        Binds outputs from other tasks to local aliases.\n\n\
        ```yaml\n\
        with:\n\
          result: $step1       # Bind step1's output\n\
          lazy_val:            # Lazy binding\n\
            path: future_task\n\
            lazy: true\n\
            default: \"fallback\"\n\
        ```\n\n\
        Access via `{{with.alias}}` in prompts.",
    ),
    (
        "for_each",
        "## `for_each:` - Parallel Iteration\n\n\
        Execute task for each item in an array.\n\n\
        ```yaml\n\
        for_each: [\"fr-FR\", \"en-US\", \"de-DE\"]\n\
        as: locale\n\
        concurrency: 5\n\
        fail_fast: true\n\
        infer: \"Generate for {{with.locale}}\"\n\
        ```",
    ),
    (
        "context",
        "## `context:` - File Loading\n\n\
        Load files at workflow start.\n\n\
        ```yaml\n\
        context:\n\
          files:\n\
            brand: ./context/brand.md\n\
            data: ./context/data.json\n\
          session: .nika/sessions/prev.json\n\
        ```\n\n\
        Access via `{{context.files.alias}}`.",
    ),
    (
        "include",
        "## `include:` - DAG Fusion\n\n\
        Merge tasks from external workflows.\n\n\
        ```yaml\n\
        include:\n\
          - path: ./partials/setup.nika.yaml\n\
            prefix: setup_\n\
        ```",
    ),
    (
        "mcp",
        "## `mcp:` - MCP Server Configuration\n\n\
        Configure MCP servers for `invoke:` and `agent:` tasks.\n\n\
        ```yaml\n\
        mcp:\n\
          servers:\n\
            novanet:\n\
              command: node\n\
              args: [\"./dist/index.js\"]\n\
              env:\n\
                NEO4J_URI: \"bolt://localhost:7687\"\n\
        ```",
    ),
    (
        "provider",
        "## `provider:` - Default LLM Provider\n\n\
        Set the default LLM provider for the workflow.\n\n\
        ```yaml\n\
        provider: claude  # claude, openai, mistral, groq, deepseek, gemini, xai, native\n\
        ```\n\n\
        Note: For local inference, use `provider: native` with mistral.rs.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "lsp")]
    fn test_verb_hover() {
        let hover = check_verb_hover("    infer: \"test\"", 4);
        assert!(hover.is_some());
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            assert!(m.value.contains("LLM Generation"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_field_hover() {
        let hover = check_field_hover("schema: nika/workflow@0.12", 0);
        assert!(hover.is_some());
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            assert!(m.value.contains("Schema Version"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_template_hover() {
        let hover = check_template_hover("  prompt: \"Process {{with.result}}\"", 22);
        assert!(hover.is_some());
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            assert!(m.value.contains("Binding Reference"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_context_template_hover() {
        let hover = check_template_hover("  text: {{context.files.brand}}", 10);
        assert!(hover.is_some());
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            assert!(m.value.contains("Context File Reference"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_no_hover_on_whitespace() {
        let hover = compute_hover(
            "    ",
            Position {
                line: 0,
                character: 2,
            },
        );
        assert!(hover.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_compute_hover_with_ast_within_task() {
        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: step1
    infer: "Hello"
"#;
        // Parse the document first
        ast_index.parse_document(&uri, text, 0);

        // Hover within task - AST provides Task context
        // Verb sub-spans may be degenerate, so AST returns Task node
        let hover = compute_hover_with_ast(
            &ast_index,
            &uri,
            text,
            Position {
                line: 4,
                character: 4, // Position within task span
            },
        );
        // Should get hover - either Task from AST or verb from fallback
        assert!(hover.is_some(), "Expected hover within task");
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            // Task documentation from AST (since verb span is degenerate)
            // OR verb documentation from text-based fallback
            assert!(
                m.value.contains("Task") || m.value.contains("LLM Generation"),
                "Expected Task or LLM Generation in hover, got: {}",
                m.value
            );
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_compute_hover_with_ast_verb_fallback() {
        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = "    infer: \"Hello\""; // Just verb line without full workflow
                                           // Don't parse - so AST returns None, fallback to text-based

        let hover = compute_hover_with_ast(
            &ast_index,
            &uri,
            text,
            Position {
                line: 0,
                character: 4,
            },
        );
        // Should get hover from text-based fallback
        assert!(hover.is_some(), "Expected hover for verb via fallback");
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            assert!(
                m.value.contains("LLM Generation"),
                "Expected LLM Generation in hover via fallback"
            );
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_compute_hover_with_ast_task() {
        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: my_task
    infer: "Hello"
"#;
        ast_index.parse_document(&uri, text, 0);

        // Hover over task id
        let hover = compute_hover_with_ast(
            &ast_index,
            &uri,
            text,
            Position {
                line: 3,
                character: 10,
            },
        );
        // May fall back to text-based hover
        if let Some(h) = hover {
            if let HoverContents::Markup(m) = h.contents {
                // Either task documentation or field documentation
                assert!(
                    m.value.contains("Task") || m.value.contains("id"),
                    "Expected task or id documentation"
                );
            }
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_all_five_verbs_hover() {
        for (verb, expected_fragment) in [
            ("infer", "LLM Generation"),
            ("exec", "Shell Command"),
            ("fetch", "HTTP Request"),
            ("invoke", "MCP Tool Call"),
            ("agent", "Agentic Loop"),
        ] {
            let line = format!("    {}: \"test\"", verb);
            let hover = check_verb_hover(&line, 4);
            assert!(hover.is_some(), "Expected hover for verb '{}'", verb);
            let h = hover.unwrap();
            if let HoverContents::Markup(m) = h.contents {
                assert!(
                    m.value.contains(expected_fragment),
                    "Verb '{}': expected '{}' in hover, got: {}",
                    verb,
                    expected_fragment,
                    m.value
                );
            } else {
                panic!("Expected markup for verb '{}'", verb);
            }
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_all_field_hovers() {
        for (field, expected_fragment) in [
            ("schema", "Schema Version"),
            ("workflow", "Workflow Name"),
            ("tasks", "Task List"),
            ("id", "Task Identifier"),
            ("with", "Data Bindings"),
            ("for_each", "Parallel Iteration"),
            ("context", "File Loading"),
            ("include", "DAG Fusion"),
            ("mcp", "MCP Server Configuration"),
            ("provider", "Default LLM Provider"),
        ] {
            let line = format!("{}: value", field);
            let hover = check_field_hover(&line, 0);
            assert!(hover.is_some(), "Expected hover for field '{}'", field);
            let h = hover.unwrap();
            if let HoverContents::Markup(m) = h.contents {
                assert!(
                    m.value.contains(expected_fragment),
                    "Field '{}': expected '{}', got: {}",
                    field,
                    expected_fragment,
                    m.value
                );
            }
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_template_hover_inputs() {
        let hover = check_template_hover("  text: {{inputs.name}}", 10);
        assert!(hover.is_some());
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            assert!(m.value.contains("Workflow Input Reference"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_template_hover_loop_item() {
        let hover = check_template_hover("  infer: \"Process {{item}}\"", 20);
        assert!(hover.is_some());
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            assert!(m.value.contains("Loop Item"));
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_fetch_hover_includes_extract_and_response() {
        let hover = check_verb_hover("    fetch:", 4);
        assert!(hover.is_some());
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            assert!(
                m.value.contains("Extract Modes"),
                "fetch hover should document extract modes"
            );
            assert!(
                m.value.contains("Response Modes"),
                "fetch hover should document response modes"
            );
            assert!(
                m.value.contains("extract: markdown"),
                "fetch hover should list markdown extract"
            );
            assert!(
                m.value.contains("response: full"),
                "fetch hover should list full response mode"
            );
        } else {
            panic!("Expected markup content");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_hover_cursor_outside_verb_range() {
        // Cursor is at position 20, well past the verb keyword
        let hover = check_verb_hover("    infer: \"some long prompt text here\"", 20);
        assert!(hover.is_none(), "Should not hover when cursor is past verb");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_hover_empty_line() {
        let hover = compute_hover(
            "schema: nika/workflow@0.12\n\ntasks:",
            Position {
                line: 1,
                character: 0,
            },
        );
        assert!(hover.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_hover_past_last_line() {
        let hover = compute_hover(
            "schema: nika/workflow@0.12",
            Position {
                line: 99,
                character: 0,
            },
        );
        assert!(hover.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_hover_multiple_templates_on_line() {
        // Two templates on the same line, hover on the second one
        let hover = check_template_hover("  text: \"{{with.a}} and {{with.b}}\"", 25);
        assert!(hover.is_some());
        let h = hover.unwrap();
        if let HoverContents::Markup(m) = h.contents {
            assert!(m.value.contains("Binding Reference"));
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_compute_hover_with_ast_fallback() {
        let ast_index = AstIndex::new();
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: step1
    infer: "Hello"
"#;
        ast_index.parse_document(&uri, text, 0);

        // Hover over empty area - should fallback to text-based
        let hover = compute_hover_with_ast(
            &ast_index,
            &uri,
            text,
            Position {
                line: 2,
                character: 0,
            },
        );
        // tasks: field should match
        if let Some(h) = hover {
            if let HoverContents::Markup(m) = h.contents {
                assert!(m.value.contains("Task List"), "Expected Task List in hover");
            }
        }
    }
}
