// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Document Symbol Handler
//!
//! Provides document outline for Nika workflows:
//! - Workflow name
//! - Tasks with their verbs
//! - MCP server configurations
//! - Context and include sections
//!
//! ## Phase 2 Architecture
//!
//! The symbol handler can use AstIndex for accurate span-based locations:
//! - Use `compute_document_symbols_with_ast` when AstIndex is available
//! - Returns hierarchical DocumentSymbol structure (tasks contain verbs)
//! - TaskTable provides O(1) task lookup with precise spans

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

#[cfg(feature = "lsp")]
pub use tower_lsp_server::ls_types::Uri;

#[cfg(feature = "lsp")]
use super::super::ast_index::AstIndex;
#[cfg(feature = "lsp")]
use crate::lsp::conversion::span_to_range;

/// Compute hierarchical document symbols using AstIndex for accurate positions
///
/// Returns a nested `DocumentSymbol` structure where:
/// - Top-level: workflow, mcp, context, tasks section
/// - Tasks contain their verbs as children
/// - All spans come from the AST for precise locations
#[cfg(feature = "lsp")]
pub fn compute_document_symbols_with_ast(
    ast_index: &AstIndex,
    uri: &Uri,
    text: &str,
) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    // Try to get cached AST
    if let Some(cached) = ast_index.get(uri) {
        if let Some(ref analyzed) = cached.analyzed {
            // Workflow name
            if let Some(ref name) = analyzed.name {
                symbols.push(create_doc_symbol(
                    format!("workflow: {}", name),
                    SymbolKind::MODULE,
                    range_for_line(text, 1), // Usually line 1 or 2
                    None,
                ));
            }

            // Schema version
            let schema_line = find_line_with_prefix(text, "schema:");
            if let Some(line_num) = schema_line {
                let line = text.lines().nth(line_num).unwrap_or("");
                symbols.push(create_doc_symbol(
                    line.trim().to_string(),
                    SymbolKind::NAMESPACE,
                    range_for_line(text, line_num),
                    None,
                ));
            }

            // Provider
            if let Some(ref provider) = analyzed.provider {
                symbols.push(create_doc_symbol(
                    format!("provider: {}", provider),
                    SymbolKind::CONSTANT,
                    find_range_with_prefix(text, "provider:"),
                    None,
                ));
            }

            // MCP servers
            if !analyzed.mcp_servers.is_empty() {
                let server_children: Vec<DocumentSymbol> = analyzed
                    .mcp_servers
                    .iter()
                    .map(|(name, server)| {
                        create_doc_symbol(
                            format!("🔌 {}", name),
                            SymbolKind::INTERFACE,
                            span_to_range(&server.span, text),
                            None,
                        )
                    })
                    .collect();

                symbols.push(create_doc_symbol(
                    "mcp.servers".to_string(),
                    SymbolKind::NAMESPACE,
                    find_range_with_prefix(text, "servers:"),
                    Some(server_children),
                ));
            }

            // Context files
            if !analyzed.context_files.is_empty() {
                let file_children: Vec<DocumentSymbol> = analyzed
                    .context_files
                    .iter()
                    .filter_map(|cf| {
                        cf.alias.as_ref().map(|alias| {
                            create_doc_symbol(
                                format!("📄 {}", alias),
                                SymbolKind::FILE,
                                span_to_range(&cf.span, text),
                                None,
                            )
                        })
                    })
                    .collect();

                if !file_children.is_empty() {
                    symbols.push(create_doc_symbol(
                        "context.files".to_string(),
                        SymbolKind::NAMESPACE,
                        find_range_with_prefix(text, "context:"),
                        Some(file_children),
                    ));
                }
            }

            // Tasks with their verbs as children
            if !analyzed.tasks.is_empty() {
                let task_symbols: Vec<DocumentSymbol> = analyzed
                    .tasks
                    .iter()
                    .map(|task| {
                        let task_range = span_to_range(&task.span, text);
                        let verb_child = task_verb_symbol(&task.action, task_range);
                        create_doc_symbol(
                            format!("task: {}", task.name),
                            SymbolKind::FUNCTION,
                            task_range,
                            verb_child.map(|v| vec![v]),
                        )
                    })
                    .collect();

                symbols.push(create_doc_symbol(
                    "tasks".to_string(),
                    SymbolKind::NAMESPACE,
                    find_range_with_prefix(text, "tasks:"),
                    Some(task_symbols),
                ));
            }

            return symbols;
        }
    }

    // Fallback to text-based symbols converted to DocumentSymbol
    compute_document_symbols(text)
        .into_iter()
        .map(symbol_info_to_doc_symbol)
        .collect()
}

/// Create a symbol for a task's verb action
#[cfg(feature = "lsp")]
fn task_verb_symbol(
    action: &crate::ast::analyzed::AnalyzedTaskAction,
    task_range: Range,
) -> Option<DocumentSymbol> {
    use crate::ast::analyzed::AnalyzedTaskAction;

    let (icon, name, kind) = match action {
        AnalyzedTaskAction::Infer(params) => {
            let detail = truncate_prompt(&params.prompt, 30);
            ("⚡", format!("infer: {}", detail), SymbolKind::METHOD)
        }
        AnalyzedTaskAction::Exec(params) => {
            let cmd = truncate_prompt(&params.command, 30);
            ("📟", format!("exec: {}", cmd), SymbolKind::METHOD)
        }
        AnalyzedTaskAction::Fetch(params) => {
            let url = truncate_prompt(&params.url, 30);
            ("🛰️", format!("fetch: {}", url), SymbolKind::METHOD)
        }
        AnalyzedTaskAction::Invoke(params) => {
            let tool = truncate_prompt(&params.tool, 30);
            ("🔌", format!("invoke: {}", tool), SymbolKind::METHOD)
        }
        AnalyzedTaskAction::Agent(params) => {
            let detail = truncate_prompt(&params.prompt, 30);
            ("🐔", format!("agent: {}", detail), SymbolKind::METHOD)
        }
    };

    Some(create_doc_symbol(
        format!("{} {}", icon, name),
        kind,
        task_range, // Use task range for the verb symbol
        None,
    ))
}

/// Truncate a prompt for display
#[cfg(feature = "lsp")]
fn truncate_prompt(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        let trunc_at = max_len.saturating_sub(3);
        let mut end = trunc_at.min(s.len());
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    } else {
        s.to_string()
    }
}

/// Create a DocumentSymbol
#[cfg(feature = "lsp")]
#[allow(deprecated)] // deprecated field still required
fn create_doc_symbol(
    name: String,
    kind: SymbolKind,
    range: Range,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        kind,
        range,
        selection_range: range,
        detail: None,
        tags: None,
        deprecated: None,
        children,
    }
}

/// Convert a SymbolInformation to DocumentSymbol (for fallback)
#[cfg(feature = "lsp")]
#[allow(deprecated)]
fn symbol_info_to_doc_symbol(info: SymbolInformation) -> DocumentSymbol {
    DocumentSymbol {
        name: info.name,
        kind: info.kind,
        range: info.location.range,
        selection_range: info.location.range,
        detail: None,
        tags: None,
        deprecated: None,
        children: None,
    }
}

/// Find the line number containing a prefix
#[cfg(feature = "lsp")]
fn find_line_with_prefix(text: &str, prefix: &str) -> Option<usize> {
    text.lines()
        .enumerate()
        .find(|(_, line)| line.trim().starts_with(prefix))
        .map(|(i, _)| i)
}

/// Get a range for a specific line
#[cfg(feature = "lsp")]
fn range_for_line(text: &str, line_num: usize) -> Range {
    let line_len = text.lines().nth(line_num).map(|l| l.len()).unwrap_or(0) as u32;
    Range {
        start: Position {
            line: line_num as u32,
            character: 0,
        },
        end: Position {
            line: line_num as u32,
            character: line_len,
        },
    }
}

/// Find range for a line with given prefix
#[cfg(feature = "lsp")]
fn find_range_with_prefix(text: &str, prefix: &str) -> Range {
    find_line_with_prefix(text, prefix)
        .map(|line_num| range_for_line(text, line_num))
        .unwrap_or(Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 0,
            },
        })
}

/// Compute document symbols for the workflow
#[cfg(feature = "lsp")]
pub fn compute_document_symbols(text: &str) -> Vec<SymbolInformation> {
    let mut symbols = Vec::new();

    // Track current section for context
    let mut current_task_id: Option<String> = None;
    let mut task_start_line: u32 = 0;

    for (line_num, line) in text.lines().enumerate() {
        let line_num = line_num as u32;
        let trimmed = line.trim();
        let indent = line.len() - trimmed.len();

        // Schema declaration
        if let Some(value) = trimmed.strip_prefix("schema:") {
            symbols.push(create_symbol(
                format!("schema: {}", value.trim()),
                SymbolKind::NAMESPACE,
                line_num,
                0,
                line.len() as u32,
            ));
        }

        // Workflow name
        if let Some(value) = trimmed.strip_prefix("workflow:") {
            let name = value.trim().trim_matches('"').trim_matches('\'');
            symbols.push(create_symbol(
                format!("workflow: {}", name),
                SymbolKind::MODULE,
                line_num,
                0,
                line.len() as u32,
            ));
        }

        // Provider
        if let Some(value) = trimmed.strip_prefix("provider:") {
            symbols.push(create_symbol(
                format!("provider: {}", value.trim()),
                SymbolKind::CONSTANT,
                line_num,
                0,
                line.len() as u32,
            ));
        }

        // Task ID - handle both "id:" and "- id:" (list item syntax)
        let task_id_value = if let Some(stripped) = trimmed.strip_prefix("- id:") {
            Some(stripped.trim())
        } else if let Some(stripped) = trimmed.strip_prefix("id:") {
            if indent > 0 {
                Some(stripped.trim())
            } else {
                None
            }
        } else {
            None
        };

        if let Some(id_str) = task_id_value {
            // Save previous task if exists
            if let Some(ref task_id) = current_task_id {
                // We'd update the end range here if we tracked it
                let _ = task_id; // suppress warning
            }

            let task_id = id_str.trim_matches('"').trim_matches('\'');
            current_task_id = Some(task_id.to_string());
            task_start_line = line_num;

            symbols.push(create_symbol(
                format!("task: {}", task_id),
                SymbolKind::FUNCTION,
                line_num,
                indent as u32,
                line.len() as u32,
            ));
        }

        // Verb detection (infer, exec, fetch, invoke, agent)
        for (verb, kind, icon) in VERB_SYMBOLS {
            if trimmed.starts_with(&format!("{}:", verb)) {
                let detail = extract_verb_detail(trimmed, verb);
                symbols.push(create_symbol(
                    format!("{} {}", icon, detail),
                    *kind,
                    line_num,
                    indent as u32,
                    line.len() as u32,
                ));
            }
        }

        // MCP servers section
        if trimmed == "servers:" {
            symbols.push(create_symbol(
                "mcp.servers".to_string(),
                SymbolKind::NAMESPACE,
                line_num,
                indent as u32,
                line.len() as u32,
            ));
        }

        // Individual MCP server
        if indent == 6 && trimmed.ends_with(':') && !trimmed.contains(' ') {
            let server_name = trimmed.trim_end_matches(':');
            symbols.push(create_symbol(
                format!("🔌 {}", server_name),
                SymbolKind::INTERFACE,
                line_num,
                indent as u32,
                line.len() as u32,
            ));
        }

        // Context section
        if trimmed == "context:" {
            symbols.push(create_symbol(
                "context".to_string(),
                SymbolKind::NAMESPACE,
                line_num,
                0,
                line.len() as u32,
            ));
        }

        // Context files
        if trimmed == "files:" && indent == 2 {
            symbols.push(create_symbol(
                "context.files".to_string(),
                SymbolKind::NAMESPACE,
                line_num,
                indent as u32,
                line.len() as u32,
            ));
        }

        // Individual context file
        if indent == 4 && trimmed.contains(':') && !trimmed.starts_with('-') {
            let file_name = trimmed.split(':').next().unwrap_or("");
            if !file_name.is_empty() && !RESERVED_FIELDS.contains(&file_name) {
                symbols.push(create_symbol(
                    format!("📄 {}", file_name),
                    SymbolKind::FILE,
                    line_num,
                    indent as u32,
                    line.len() as u32,
                ));
            }
        }

        // Include section
        if trimmed == "include:" {
            symbols.push(create_symbol(
                "include".to_string(),
                SymbolKind::NAMESPACE,
                line_num,
                0,
                line.len() as u32,
            ));
        }

        // Include path
        if trimmed.starts_with("path:") && indent > 0 {
            let path = trimmed[5..].trim().trim_matches('"').trim_matches('\'');
            symbols.push(create_symbol(
                format!("📦 {}", path),
                SymbolKind::PACKAGE,
                line_num,
                indent as u32,
                line.len() as u32,
            ));
        }

        // Skills section
        if trimmed == "skills:" {
            symbols.push(create_symbol(
                "skills".to_string(),
                SymbolKind::NAMESPACE,
                line_num,
                0,
                line.len() as u32,
            ));
        }

        // For each parallel iteration
        if trimmed.starts_with("for_each:") {
            symbols.push(create_symbol(
                "🔄 for_each".to_string(),
                SymbolKind::EVENT,
                line_num,
                indent as u32,
                line.len() as u32,
            ));
        }
    }

    // Clear last task
    current_task_id = None;
    let _ = (current_task_id, task_start_line); // suppress warnings

    symbols
}

/// Placeholder URL used for symbols (valid file URI)
///
/// Note: Symbols have no native way to exclude URI in tower-lsp.
/// The actual document URI should ideally be passed, but compute_document_symbols
/// doesn't receive it. This is a limitation of the current API.
#[cfg(feature = "lsp")]
fn placeholder_url() -> Uri {
    // Use `expect` since this is a known-valid URL literal that cannot fail
    "file:///placeholder"
        .parse::<Uri>()
        .expect("static valid URL")
}

/// Create a SymbolInformation
#[cfg(feature = "lsp")]
fn create_symbol(
    name: String,
    kind: SymbolKind,
    line: u32,
    start_char: u32,
    end_char: u32,
) -> SymbolInformation {
    #[allow(deprecated)] // container_name is deprecated but still required
    SymbolInformation {
        name,
        kind,
        tags: None,
        deprecated: None,
        location: Location {
            uri: placeholder_url(),
            range: Range {
                start: Position {
                    line,
                    character: start_char,
                },
                end: Position {
                    line,
                    character: end_char,
                },
            },
        },
        container_name: None,
    }
}

/// Extract a detail string from a verb line
#[cfg(feature = "lsp")]
fn extract_verb_detail(line: &str, verb: &str) -> String {
    let after_colon = &line[verb.len() + 1..];
    let trimmed = after_colon.trim();

    // Shorthand form: verb: "value"
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        let quote = trimmed.chars().next().unwrap();
        if let Some(end) = trimmed[1..].find(quote) {
            let value = &trimmed[1..end + 1];
            // Truncate long prompts
            if value.len() > 40 {
                return format!("{}: {}...", verb, crate::util::truncate_str(value, 37));
            }
            return format!("{}: {}", verb, value);
        }
    }

    // Full form or empty
    verb.to_string()
}

/// Verb symbols with their kinds and icons
#[cfg(feature = "lsp")]
const VERB_SYMBOLS: &[(&str, SymbolKind, &str)] = &[
    ("infer", SymbolKind::METHOD, "⚡"),
    ("exec", SymbolKind::METHOD, "📟"),
    ("fetch", SymbolKind::METHOD, "🛰️"),
    ("invoke", SymbolKind::METHOD, "🔌"),
    ("agent", SymbolKind::METHOD, "🐔"),
];

/// Reserved field names (not context files)
#[cfg(feature = "lsp")]
const RESERVED_FIELDS: &[&str] = &[
    "schema",
    "workflow",
    "tasks",
    "flows",
    "mcp",
    "context",
    "include",
    "skills",
    "provider",
    "id",
    "with",
    "for_each",
    "as",
    "concurrency",
    "fail_fast",
    "infer",
    "exec",
    "fetch",
    "invoke",
    "agent",
    "servers",
    "files",
    "session",
    "path",
    "prefix",
    "alias",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "lsp")]
    fn test_document_symbols_basic() {
        let text = r#"
schema: nika/workflow@0.12
workflow: my-workflow

tasks:
  - id: step1
    infer: "Generate content"
  - id: step2
    exec: "npm run build"
"#;
        let symbols = compute_document_symbols(text);

        // Check we found schema, workflow, tasks, and verbs
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("schema")));
        assert!(names.iter().any(|n| n.contains("workflow: my-workflow")));
        assert!(names.iter().any(|n| n.contains("task: step1")));
        assert!(names.iter().any(|n| n.contains("task: step2")));
        assert!(names.iter().any(|n| n.contains("⚡"))); // infer icon
        assert!(names.iter().any(|n| n.contains("📟"))); // exec icon
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_document_symbols_mcp() {
        let text = r#"
schema: nika/workflow@0.12
mcp:
  servers:
    novanet:
      command: node
    perplexity:
      command: npx
"#;
        let symbols = compute_document_symbols(text);

        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("mcp.servers")));
        assert!(names.iter().any(|n| n.contains("novanet")));
        assert!(names.iter().any(|n| n.contains("perplexity")));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_document_symbols_context() {
        let text = r#"
schema: nika/workflow@0.12
context:
  files:
    brand: ./brand.md
    data: ./data.json
"#;
        let symbols = compute_document_symbols(text);

        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"context"));
        assert!(names.contains(&"context.files"));
        assert!(names.iter().any(|n| n.contains("brand")));
        assert!(names.iter().any(|n| n.contains("data")));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_document_symbols_for_each() {
        let text = r#"
tasks:
  - id: parallel_task
    for_each: ["a", "b", "c"]
    as: item
    infer: "Process {{with.item}}"
"#;
        let symbols = compute_document_symbols(text);

        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("for_each")));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_verb_detail() {
        assert_eq!(
            extract_verb_detail("infer: \"Generate a headline\"", "infer"),
            "infer: Generate a headline"
        );

        // Test truncation of long prompts
        let long_prompt = "infer: \"This is a very long prompt that should be truncated because it exceeds forty characters\"";
        let detail = extract_verb_detail(long_prompt, "infer");
        assert!(detail.ends_with("..."));
        assert!(detail.len() < 50);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_verb_icon_mapping() {
        let text = r#"
tasks:
  - id: t1
    infer: "test"
  - id: t2
    agent:
      prompt: "test"
"#;
        let symbols = compute_document_symbols(text);

        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.iter().any(|n| n.starts_with("⚡"))); // infer
        assert!(names.iter().any(|n| n.starts_with("🐔"))); // agent
    }

    // ============================================================================
    // AST-Aware Symbol Tests (Phase 2)
    // ============================================================================

    #[test]
    #[cfg(feature = "lsp")]
    fn test_document_symbols_with_ast_hierarchical() {
        let text = r#"schema: nika/workflow@0.12
workflow: test-workflow

tasks:
  - id: step1
    model: gpt-4o-mini
    infer: "Generate content"
  - id: step2
    exec: "npm run build"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();

        // Parse document into AST index
        ast_index.parse_document(&uri, text, 0);

        let symbols = compute_document_symbols_with_ast(&ast_index, &uri, text);

        // Should have hierarchical structure
        assert!(!symbols.is_empty());

        // Check for top-level symbols
        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("workflow: test-workflow")));
        assert!(names.iter().any(|n| n.contains("schema:")));
        assert!(names.contains(&"tasks"));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_document_symbols_with_ast_tasks_have_verb_children() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: generate
    model: gpt-4o-mini
    infer: "Generate something"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();

        // Parse document into AST index
        ast_index.parse_document(&uri, text, 0);

        let symbols = compute_document_symbols_with_ast(&ast_index, &uri, text);

        // Find the tasks section
        let tasks_symbol = symbols.iter().find(|s| s.name == "tasks");
        assert!(tasks_symbol.is_some());

        // Tasks should have children (task symbols)
        let tasks = tasks_symbol.unwrap();
        assert!(tasks.children.is_some());
        let task_children = tasks.children.as_ref().unwrap();
        assert!(!task_children.is_empty());

        // Each task should have a verb child
        let task = task_children.iter().find(|s| s.name.contains("generate"));
        assert!(task.is_some());
        let task = task.unwrap();
        assert!(task.children.is_some());

        // Verb child should have the infer icon
        let verb = &task.children.as_ref().unwrap()[0];
        assert!(verb.name.contains("⚡"));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_document_symbols_with_ast_fallback() {
        // When AST has no cached document, falls back to text-based
        let text = r#"
schema: nika/workflow@0.12
workflow: fallback-test
tasks:
  - id: step1
    infer: "Hello"
"#;
        let uri = "file:///uncached.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        // Deliberately NOT parsing into AST index

        let symbols = compute_document_symbols_with_ast(&ast_index, &uri, text);

        // Should still return symbols via fallback
        assert!(!symbols.is_empty());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_truncate_prompt() {
        assert_eq!(truncate_prompt("short", 30), "short");
        assert_eq!(
            truncate_prompt("this is a very long prompt that exceeds the limit", 30),
            "this is a very long prompt ..."
        );
    }
}
