//! Code action handler — quick fixes and refactorings.
//!
//! Protocol-agnostic: returns `Vec<CodeActionEntry>` with text edits.
//! The tower-lsp shim converts to `CodeActionResponse`.
//!
//! ## Actions provided (9 total)
//! - Add missing `schema:` version (QuickFix)
//! - Upgrade `@0.11`/`@0.10` schema to `@0.12` (QuickFix)
//! - Expand shorthand `infer: "prompt"` to block form (Refactor)
//! - Expand shorthand `exec: "cmd"` to block form (Refactor)
//! - Expand shorthand `fetch: "url"` to block form (Refactor)
//! - Expand shorthand `invoke: "tool"` to block form (Refactor)
//! - Expand shorthand `agent: "goal"` to block form (Refactor)
//! - Add missing `id:` field to task (QuickFix)
//! - Add default `provider:` to workflow (QuickFix)

/// Protocol-agnostic code action entry.
#[derive(Debug, Clone)]
pub struct CodeActionEntry {
    /// Human-readable title shown in the action menu.
    pub title: String,
    /// Action kind (QuickFix or Refactor).
    pub kind: CodeActionKind,
    /// Text edit to apply (if any).
    pub edit: Option<TextEdit>,
    /// Whether this is the preferred/default action.
    pub is_preferred: bool,
}

/// Code action kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeActionKind {
    QuickFix,
    Refactor,
}

/// Protocol-agnostic text edit.
#[derive(Debug, Clone)]
pub struct TextEdit {
    /// Start byte offset in the document.
    pub offset: u32,
    /// End byte offset (for replacement range).
    pub end_offset: u32,
    /// New text to insert/replace.
    pub new_text: String,
}

/// Compute code actions for the given range in the document.
///
/// Returns all applicable quick fixes and refactorings. The caller (tower-lsp shim)
/// filters based on `CodeActionKind` if requested.
pub fn code_actions(text: &str, start_offset: u32, _end_offset: u32) -> Vec<CodeActionEntry> {
    let mut actions = Vec::new();

    // 1. Add schema version if missing (skip comments)
    let has_schema = text.lines().any(|l| {
        let t = l.trim();
        !t.starts_with('#') && t.starts_with("schema:")
    });
    if !has_schema {
        actions.push(CodeActionEntry {
            title: "Add schema version (@0.12)".into(),
            kind: CodeActionKind::QuickFix,
            is_preferred: true,
            edit: Some(TextEdit {
                offset: 0,
                end_offset: 0,
                new_text: "schema: \"@0.12\"\n".into(),
            }),
        });
    }

    // 2. Upgrade old schema version
    for old_schema in ["@0.11", "@0.10", "@0.9"] {
        let needle = format!("schema: \"{}\"", old_schema);
        if let Some(pos) = text.find(&needle) {
            actions.push(CodeActionEntry {
                title: format!("Upgrade schema {} → @0.12", old_schema),
                kind: CodeActionKind::QuickFix,
                is_preferred: true,
                edit: Some(TextEdit {
                    offset: pos as u32,
                    end_offset: (pos + needle.len()) as u32,
                    new_text: "schema: \"@0.12\"".into(),
                }),
            });
        }
    }

    // Get the line at the cursor (with char boundary safety)
    let start = (start_offset as usize).min(text.len());
    if !text.is_char_boundary(start) {
        return actions;
    }
    let line_start = text[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = text[start..]
        .find('\n')
        .map(|p| start + p)
        .unwrap_or(text.len());
    let line = &text[line_start..line_end];
    let trimmed = line.trim();
    let indent: String = " ".repeat(line.len() - trimmed.len());

    // 3. Expand shorthand infer: "prompt" → block form
    if let Some(rest) = trimmed.strip_prefix("infer:") {
        let prompt = rest.trim().trim_matches('"').trim_matches('\'');
        if !prompt.is_empty() && !prompt.contains('\n') {
            actions.push(CodeActionEntry {
                title: "Expand infer to block form".into(),
                kind: CodeActionKind::Refactor,
                is_preferred: false,
                edit: Some(TextEdit {
                    offset: line_start as u32,
                    end_offset: line_end as u32,
                    new_text: format!("{indent}infer:\n{indent}  prompt: |\n{indent}    {prompt}"),
                }),
            });
        }
    }

    // 4. Expand shorthand exec: "cmd" → block form
    if let Some(rest) = trimmed.strip_prefix("exec:") {
        let cmd = rest.trim().trim_matches('"').trim_matches('\'');
        if !cmd.is_empty() && !cmd.contains('\n') {
            actions.push(CodeActionEntry {
                title: "Expand exec to block form".into(),
                kind: CodeActionKind::Refactor,
                is_preferred: false,
                edit: Some(TextEdit {
                    offset: line_start as u32,
                    end_offset: line_end as u32,
                    new_text: format!(
                        "{indent}exec:\n{indent}  command: \"{cmd}\"\n{indent}  timeout: 30"
                    ),
                }),
            });
        }
    }

    // 5. Expand shorthand fetch: "url" → block form
    if let Some(rest) = trimmed.strip_prefix("fetch:") {
        let url = rest.trim().trim_matches('"').trim_matches('\'');
        if !url.is_empty() && url.starts_with("http") {
            actions.push(CodeActionEntry {
                title: "Expand fetch to block form".into(),
                kind: CodeActionKind::Refactor,
                is_preferred: false,
                edit: Some(TextEdit {
                    offset: line_start as u32,
                    end_offset: line_end as u32,
                    new_text: format!(
                        "{indent}fetch:\n{indent}  url: \"{url}\"\n{indent}  method: GET"
                    ),
                }),
            });
        }
    }

    // 6. Expand shorthand invoke: "tool" → block form
    if let Some(rest) = trimmed.strip_prefix("invoke:") {
        let tool = rest.trim().trim_matches('"').trim_matches('\'');
        if !tool.is_empty() && !tool.contains('\n') && !tool.contains(':') {
            actions.push(CodeActionEntry {
                title: "Expand invoke to block form".into(),
                kind: CodeActionKind::Refactor,
                is_preferred: false,
                edit: Some(TextEdit {
                    offset: line_start as u32,
                    end_offset: line_end as u32,
                    new_text: format!(
                        "{indent}invoke:\n{indent}  mcp: novanet\n{indent}  tool: {tool}\n{indent}  params:\n{indent}    {{}}"
                    ),
                }),
            });
        }
    }

    // 7. Expand shorthand agent: "goal" → block form
    if let Some(rest) = trimmed.strip_prefix("agent:") {
        let goal = rest.trim().trim_matches('"').trim_matches('\'');
        if !goal.is_empty() && !goal.contains('\n') {
            actions.push(CodeActionEntry {
                title: "Expand agent to block form".into(),
                kind: CodeActionKind::Refactor,
                is_preferred: false,
                edit: Some(TextEdit {
                    offset: line_start as u32,
                    end_offset: line_end as u32,
                    new_text: format!(
                        "{indent}agent:\n{indent}  prompt: |\n{indent}    {goal}\n{indent}  max_turns: 10"
                    ),
                }),
            });
        }
    }

    // 8. Add missing id: to task (cursor on - without id:)
    if trimmed == "-" || (trimmed.starts_with("- ") && !trimmed.contains("id:")) {
        // Check if this looks like inside a tasks: block (has a verb or field)
        let next_lines: Vec<&str> = text[line_end..].lines().take(5).collect();
        let has_verb = next_lines.iter().any(|l| {
            let t = l.trim();
            t.starts_with("infer:")
                || t.starts_with("exec:")
                || t.starts_with("fetch:")
                || t.starts_with("invoke:")
                || t.starts_with("agent:")
        });
        if has_verb {
            actions.push(CodeActionEntry {
                title: "Add task id".into(),
                kind: CodeActionKind::QuickFix,
                is_preferred: true,
                edit: Some(TextEdit {
                    offset: line_start as u32,
                    end_offset: line_end as u32,
                    new_text: format!("{indent}- id: task_name\n{indent}  "),
                }),
            });
        }
    }

    // 9. Add provider if missing (only at workflow level)
    if text.contains("tasks:") && !text.contains("provider:") {
        // Find where to insert (after schema: or workflow:, or at top)
        let insert_offset = text
            .find("tasks:")
            .map(|p| {
                // Insert just before tasks:
                text[..p].rfind('\n').map(|n| n + 1).unwrap_or(p)
            })
            .unwrap_or(0);
        actions.push(CodeActionEntry {
            title: "Add default provider".into(),
            kind: CodeActionKind::QuickFix,
            is_preferred: false,
            edit: Some(TextEdit {
                offset: insert_offset as u32,
                end_offset: insert_offset as u32,
                new_text: "provider: anthropic\n".into(),
            }),
        });
    }

    actions
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_schema_offers_add() {
        let actions = code_actions("workflow: test\ntasks:\n", 0, 0);
        assert!(actions.iter().any(|a| a.title.contains("schema")));
    }

    #[test]
    fn has_schema_no_add() {
        let actions = code_actions("schema: \"@0.12\"\nworkflow: test\n", 0, 0);
        assert!(!actions.iter().any(|a| a.title.contains("Add schema")));
    }

    #[test]
    fn upgrade_old_schema() {
        let text = "schema: \"@0.11\"\n";
        let actions = code_actions(text, 0, 0);
        let upgrade = actions.iter().find(|a| a.title.contains("Upgrade"));
        assert!(upgrade.is_some());
        assert!(upgrade
            .unwrap()
            .edit
            .as_ref()
            .unwrap()
            .new_text
            .contains("@0.12"));
    }

    #[test]
    fn expand_infer_shorthand() {
        let text = "    infer: \"Generate a headline\"\n";
        let offset = text.find("infer").unwrap() as u32;
        let actions = code_actions(text, offset, offset);
        let expand = actions.iter().find(|a| a.title.contains("Expand infer"));
        assert!(expand.is_some());
        let edit = expand.unwrap().edit.as_ref().unwrap();
        assert!(edit.new_text.contains("prompt:"));
    }

    #[test]
    fn expand_exec_shorthand() {
        let text = "    exec: \"npm run build\"\n";
        let offset = text.find("exec").unwrap() as u32;
        let actions = code_actions(text, offset, offset);
        assert!(actions.iter().any(|a| a.title.contains("Expand exec")));
    }

    #[test]
    fn expand_fetch_shorthand() {
        let text = "    fetch: \"https://api.example.com\"\n";
        let offset = text.find("fetch").unwrap() as u32;
        let actions = code_actions(text, offset, offset);
        assert!(actions.iter().any(|a| a.title.contains("Expand fetch")));
    }

    #[test]
    fn no_expand_non_url_fetch() {
        let text = "    fetch:\n      url: x\n";
        let offset = text.find("fetch").unwrap() as u32;
        let actions = code_actions(text, offset, offset);
        assert!(!actions.iter().any(|a| a.title.contains("Expand fetch")));
    }

    #[test]
    fn add_provider_when_missing() {
        let text = "schema: \"@0.12\"\ntasks:\n  - id: x\n";
        let actions = code_actions(text, 0, 0);
        assert!(actions.iter().any(|a| a.title.contains("provider")));
    }

    #[test]
    fn no_add_provider_when_present() {
        let text = "schema: \"@0.12\"\nprovider: openai\ntasks:\n";
        let actions = code_actions(text, 0, 0);
        assert!(!actions
            .iter()
            .any(|a| a.title.contains("Add default provider")));
    }

    #[test]
    fn quickfix_is_preferred() {
        let actions = code_actions("workflow: test\n", 0, 0);
        let schema_action = actions.iter().find(|a| a.title.contains("schema")).unwrap();
        assert!(schema_action.is_preferred);
        assert_eq!(schema_action.kind, CodeActionKind::QuickFix);
    }

    #[test]
    fn expand_invoke_shorthand() {
        let text = "    invoke: \"novanet_search\"\n";
        let offset = text.find("invoke").unwrap() as u32;
        let actions = code_actions(text, offset, offset);
        let expand = actions.iter().find(|a| a.title.contains("Expand invoke"));
        assert!(expand.is_some());
        let edit = expand.unwrap().edit.as_ref().unwrap();
        assert!(edit.new_text.contains("tool: novanet_search"));
    }

    #[test]
    fn expand_agent_shorthand() {
        let text = "    agent: \"Research AI papers\"\n";
        let offset = text.find("agent").unwrap() as u32;
        let actions = code_actions(text, offset, offset);
        let expand = actions.iter().find(|a| a.title.contains("Expand agent"));
        assert!(expand.is_some());
        let edit = expand.unwrap().edit.as_ref().unwrap();
        assert!(edit.new_text.contains("prompt:"));
        assert!(edit.new_text.contains("max_turns:"));
    }

    #[test]
    fn no_expand_invoke_block() {
        let text = "    invoke:\n      tool: x\n";
        let offset = text.find("invoke").unwrap() as u32;
        let actions = code_actions(text, offset, offset);
        assert!(!actions.iter().any(|a| a.title.contains("Expand invoke")));
    }

    #[test]
    fn add_task_id_when_missing() {
        let text = "tasks:\n  -\n    infer: \"hello\"\n";
        let offset = text.find('-').unwrap() as u32;
        let actions = code_actions(text, offset, offset);
        assert!(
            actions.iter().any(|a| a.title.contains("task id")),
            "Expected 'Add task id' action, got: {:?}",
            actions.iter().map(|a| &a.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn schema_check_ignores_comments() {
        let text = "# schema: old version\nworkflow: test\ntasks:\n";
        let actions = code_actions(text, 0, 0);
        assert!(
            actions.iter().any(|a| a.title.contains("Add schema")),
            "Should offer Add schema when only comment has schema:"
        );
    }

    #[test]
    fn refactor_not_preferred() {
        let text = "    infer: \"test\"\n";
        let offset = text.find("infer").unwrap() as u32;
        let actions = code_actions(text, offset, offset);
        let expand = actions.iter().find(|a| a.title.contains("Expand")).unwrap();
        assert!(!expand.is_preferred);
        assert_eq!(expand.kind, CodeActionKind::Refactor);
    }
}
