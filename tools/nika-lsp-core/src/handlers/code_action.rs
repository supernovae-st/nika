// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
                        "{indent}invoke:\n{indent}  tool: {tool}\n{indent}  params:\n{indent}    {{}}"
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
// Diagnostic-linked code actions
// ═══════════════════════════════════════════════════════════════════════════

/// Protocol-agnostic diagnostic info passed from the LSP layer.
#[derive(Debug, Clone)]
pub struct DiagnosticInfo {
    /// NIKA error code (e.g. "NIKA-140").
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Start byte offset of the diagnostic range in the document.
    pub start_offset: u32,
    /// End byte offset of the diagnostic range.
    pub end_offset: u32,
}

/// Compute code actions including diagnostic-linked quick fixes.
///
/// Combines text-based actions (from `code_actions`) with
/// diagnostic-aware fixes (unknown task, duplicate, missing field, etc.).
pub fn code_actions_with_diagnostics(
    text: &str,
    start_offset: u32,
    end_offset: u32,
    diagnostics: &[DiagnosticInfo],
) -> Vec<CodeActionEntry> {
    let mut actions = code_actions(text, start_offset, end_offset);

    // Extract known task IDs for fuzzy matching
    let task_ids = extract_task_ids_simple(text);

    for diag in diagnostics {
        if let Some(action) = quickfix_for_diagnostic(text, diag, &task_ids) {
            actions.push(action);
        }
    }

    actions
}

fn quickfix_for_diagnostic(
    text: &str,
    diag: &DiagnosticInfo,
    task_ids: &[String],
) -> Option<CodeActionEntry> {
    match diag.code.as_str() {
        "NIKA-140" => fix_unknown_task(text, diag, task_ids),
        "NIKA-141" => fix_duplicate_task(diag),
        "NIKA-142" => fix_invalid_schema(diag),
        "NIKA-145" => fix_missing_field(text, diag),
        "NIKA-034" => fix_missing_model(text, diag),
        _ => None,
    }
}

/// Extract task IDs from text (simple scan, no AST needed).
fn extract_task_ids_simple(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix("- id:")?;
            let id = rest.trim().trim_matches('"').trim_matches('\'');
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        })
        .collect()
}

fn fix_unknown_task(
    _text: &str,
    diag: &DiagnosticInfo,
    task_ids: &[String],
) -> Option<CodeActionEntry> {
    let unknown = extract_quoted_name(&diag.message)?;

    if task_ids.is_empty() {
        return None;
    }

    let best = find_best_match(&unknown, task_ids)?;

    Some(CodeActionEntry {
        title: format!("Did you mean '{}'?", best),
        kind: CodeActionKind::QuickFix,
        is_preferred: true,
        edit: Some(TextEdit {
            offset: diag.start_offset,
            end_offset: diag.end_offset,
            new_text: best,
        }),
    })
}

fn fix_duplicate_task(diag: &DiagnosticInfo) -> Option<CodeActionEntry> {
    let dup_name = extract_quoted_name(&diag.message)?;
    let new_name = format!("{}-2", dup_name);

    Some(CodeActionEntry {
        title: format!("Rename to '{}'", new_name),
        kind: CodeActionKind::QuickFix,
        is_preferred: false,
        edit: Some(TextEdit {
            offset: diag.start_offset,
            end_offset: diag.end_offset,
            new_text: new_name,
        }),
    })
}

fn fix_invalid_schema(diag: &DiagnosticInfo) -> Option<CodeActionEntry> {
    Some(CodeActionEntry {
        title: "Update to schema @0.12".into(),
        kind: CodeActionKind::QuickFix,
        is_preferred: true,
        edit: Some(TextEdit {
            offset: diag.start_offset,
            end_offset: diag.end_offset,
            new_text: "schema: \"@0.12\"".into(),
        }),
    })
}

fn fix_missing_field(text: &str, diag: &DiagnosticInfo) -> Option<CodeActionEntry> {
    let msg = &diag.message;
    let (title, new_text) = if msg.contains("'id'") {
        // Insert id: before the current line
        let line_start = text[..diag.start_offset as usize]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line = &text[line_start..];
        let indent = line.len() - line.trim_start().len();
        let indent_str: String = " ".repeat(indent);
        (
            "Add missing id field".into(),
            format!("{}id: new_task\n{}", indent_str, indent_str),
        )
    } else if msg.contains("'schema'") {
        ("Add missing schema".into(), "schema: \"@0.12\"\n".into())
    } else if msg.contains("'tasks'") {
        (
            "Add missing tasks block".into(),
            "tasks:\n  - id: step1\n    infer: \"TODO\"\n".into(),
        )
    } else {
        return None;
    };

    Some(CodeActionEntry {
        title,
        kind: CodeActionKind::QuickFix,
        is_preferred: true,
        edit: Some(TextEdit {
            offset: diag.start_offset,
            end_offset: diag.start_offset, // insert, don't replace
            new_text,
        }),
    })
}

fn fix_missing_model(text: &str, diag: &DiagnosticInfo) -> Option<CodeActionEntry> {
    // Find the task line to insert model after the verb line
    let offset = diag.start_offset as usize;
    let line_start = text[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line = &text[line_start..];
    let indent = line.len() - line.trim_start().len();
    let indent_str: String = " ".repeat(indent);

    // Check if workflow already has a provider to pick a matching default model
    let default_model = if text.contains("provider: openai") {
        "gpt-4o"
    } else if text.contains("provider: mistral") {
        "mistral-large-latest"
    } else if text.contains("provider: groq") {
        "llama-3.3-70b-versatile"
    } else if text.contains("provider: deepseek") {
        "deepseek-chat"
    } else if text.contains("provider: gemini") {
        "gemini-2.0-flash"
    } else if text.contains("provider: xai") {
        "grok-3"
    } else {
        "claude-sonnet-4-20250514"
    };

    // Find end of the verb line to insert after it
    let line_end = text[line_start..]
        .find('\n')
        .map(|p| line_start + p + 1)
        .unwrap_or(text.len());

    Some(CodeActionEntry {
        title: format!("Add model: {}", default_model),
        kind: CodeActionKind::QuickFix,
        is_preferred: true,
        edit: Some(TextEdit {
            offset: line_end as u32,
            end_offset: line_end as u32,
            new_text: format!("{}model: {}\n", indent_str, default_model),
        }),
    })
}

/// Extract a 'quoted name' from a diagnostic message.
fn extract_quoted_name(message: &str) -> Option<String> {
    let start = message.find('\'')?;
    let end = message[start + 1..].find('\'')?;
    Some(message[start + 1..start + 1 + end].to_string())
}

/// Find the best fuzzy match for `target` among `candidates`.
fn find_best_match(target: &str, candidates: &[String]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }

    let target_lower = target.to_lowercase();
    let mut best_score = 0.0_f64;
    let mut best = &candidates[0];

    for candidate in candidates {
        let score = fuzzy_score(&target_lower, &candidate.to_lowercase());
        if score > best_score {
            best_score = score;
            best = candidate;
        }
    }

    if best_score >= 0.3 {
        Some(best.clone())
    } else {
        None
    }
}

/// Simple fuzzy similarity: LCS ratio + char overlap + prefix bonus.
fn fuzzy_score(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    // LCS
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (m, n) = (a_chars.len(), b_chars.len());
    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        for j in 1..=n {
            curr[j] = if a_chars[i - 1] == b_chars[j - 1] {
                prev[j - 1] + 1
            } else {
                prev[j].max(curr[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }
    let lcs = prev[n] as f64 / m.max(n) as f64;

    // Char overlap
    let a_set: std::collections::HashSet<char> = a.chars().collect();
    let b_set: std::collections::HashSet<char> = b.chars().collect();
    let overlap = a_set.intersection(&b_set).count() as f64 / a_set.len().max(b_set.len()) as f64;

    // Prefix bonus
    let prefix = if a.starts_with(b) || b.starts_with(a) {
        0.2
    } else {
        0.0
    };

    (lcs * 0.5 + overlap * 0.3 + prefix).min(1.0)
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
