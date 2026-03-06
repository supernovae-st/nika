//! Hover Handler
//!
//! Provides hover information for Nika workflow elements:
//! - Verb documentation (infer, exec, fetch, invoke, agent)
//! - Field descriptions
//! - Task reference information
//! - Template variable documentation

#[cfg(feature = "lsp")]
use tower_lsp::lsp_types::*;

/// Compute hover information at a position
#[cfg(feature = "lsp")]
pub fn compute_hover(text: &str, position: Position) -> Option<Hover> {
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return None;
    }

    let line = lines[line_idx];
    let col = position.character as usize;

    // Check what's at this position
    if let Some(hover) = check_verb_hover(line, col) {
        return Some(hover);
    }

    if let Some(hover) = check_field_hover(line, col) {
        return Some(hover);
    }

    if let Some(hover) = check_template_hover(line, col) {
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
    // Find template expressions like {{use.alias}} or {{context.files.name}}
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

    if let Some(alias) = template.strip_prefix("use.") {
        format!(
            "## Binding Reference\n\n\
            **`{{{{use.{}}}}}`**\n\n\
            References the output of a task bound to `{}` in the `use:` block.\n\n\
            ```yaml\n\
            use:\n  {}: task_id\n\
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
    } else if template == "item" || template == "use.item" {
        "## Loop Item Reference\n\n\
        **`{{item}}`** or **`{{use.item}}`**\n\n\
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
        ```\n\n\
        **Icon:** 🛰️",
    ),
    (
        "invoke",
        "## `invoke:` - MCP Tool Call\n\n\
        Calls a tool on an MCP server.\n\n\
        ```yaml\n\
        invoke:\n\
          mcp: novanet\n\
          tool: novanet_generate\n\
          params:\n\
            entity: \"qr-code\"\n\
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
        schema: nika/workflow@0.10\n\
        ```\n\n\
        **Current version:** `@0.10` (Two-Phase AST)",
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
            use:\n\
              input: step1\n\
            infer: \"Process: {{use.input}}\"\n\
        ```",
    ),
    (
        "flows",
        "## `flows:` - Task Dependencies\n\n\
        Defines execution order between tasks.\n\n\
        ```yaml\n\
        flows:\n\
          - source: step1\n\
            target: step2\n\
        ```",
    ),
    (
        "id",
        "## `id:` - Task Identifier\n\n\
        Unique identifier for the task. Used in bindings and flows.\n\n\
        ```yaml\n\
        - id: my_task\n\
          infer: \"...\"\n\
        ```",
    ),
    (
        "use",
        "## `use:` - Data Bindings\n\n\
        Binds outputs from other tasks to local aliases.\n\n\
        ```yaml\n\
        use:\n\
          result: step1        # Bind step1's output\n\
          lazy_val:            # Lazy binding\n\
            path: future_task\n\
            lazy: true\n\
            default: \"fallback\"\n\
        ```\n\n\
        Access via `{{use.alias}}` in prompts.",
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
        infer: \"Generate for {{use.locale}}\"\n\
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
        provider: claude  # claude, openai, mistral, ollama, groq, deepseek, gemini\n\
        ```",
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
        let hover = check_field_hover("schema: nika/workflow@0.10", 0);
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
        let hover = check_template_hover("  prompt: \"Process {{use.result}}\"", 22);
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
}
