//! Document Symbol Handler
//!
//! Provides document outline for Nika workflows:
//! - Workflow name
//! - Tasks with their verbs
//! - MCP server configurations
//! - Context and include sections

#[cfg(feature = "lsp")]
use tower_lsp::lsp_types::*;

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

        // Flows section
        if trimmed == "flows:" {
            symbols.push(create_symbol(
                "flows".to_string(),
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
fn placeholder_url() -> Url {
    // Use `expect` since this is a known-valid URL literal that cannot fail
    Url::parse("file:///placeholder").expect("static valid URL")
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
                return format!("{}: {}...", verb, &value[..37]);
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
    "use",
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
schema: nika/workflow@0.10
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
schema: nika/workflow@0.10
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
schema: nika/workflow@0.10
context:
  files:
    brand: ./brand.md
    data: ./data.json
"#;
        let symbols = compute_document_symbols(text);

        let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.iter().any(|n| *n == "context"));
        assert!(names.iter().any(|n| *n == "context.files"));
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
    infer: "Process {{use.item}}"
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
}
