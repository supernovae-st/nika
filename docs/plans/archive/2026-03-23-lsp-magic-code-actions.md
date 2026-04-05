# LSP Magic: Code Actions + CodeLens + InlayHints

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Migrate diagnostic-linked code actions, CodeLens, and InlayHints from embedded nika-engine LSP to the standalone nika-lsp-core + nika-lsp, then extend with new features (model cost hints, template fix actions).

**Architecture:** The intelligence lives in `nika-lsp-core` (protocol-agnostic, pure functions, no async). The `nika-lsp` binary wires these to tower-lsp. The embedded `nika-engine/src/lsp/` already has working implementations for CodeLens, InlayHints, and diagnostic code actions — we port them to `nika-lsp-core` as pure handlers, then wire in `nika-lsp/src/backend.rs`.

**Tech Stack:** Rust, tower-lsp-server 0.23, ls-types, nika-lsp-core (pure handlers), nika-lsp (binary)

**Key Insight:** nika-engine already has these handlers gated behind `#[cfg(feature = "lsp")]` using tower-lsp types. We need protocol-agnostic versions in nika-lsp-core.

---

## Batch 1: Diagnostic-Linked Code Actions (nika-lsp-core)

### Task 1: Add diagnostic info to code_actions signature

The current `code_actions(text, start, end)` in nika-lsp-core has NO diagnostic info. The nika-engine version takes `diagnostics: &[Diagnostic]` and matches on error codes. We need a protocol-agnostic diagnostic representation.

**Files:**
- Modify: `tools/nika-lsp-core/src/handlers/code_action.rs`
- Modify: `tools/nika-lsp-core/src/handler.rs`
- Test: `tools/nika-lsp-core/tests/code_action_e2e.rs`

**Step 1: Write the failing test**

Add to `tools/nika-lsp-core/tests/code_action_e2e.rs`:

```rust
// ---------------------------------------------------------------------------
// Diagnostic-linked code actions
// ---------------------------------------------------------------------------
use nika_lsp_core::handlers::code_action::{
    code_actions, code_actions_with_diagnostics, CodeActionKind, DiagnosticInfo,
};

#[test]
fn unknown_task_diagnostic_suggests_fix() {
    let yaml = "\
schema: \"@0.12\"
model: test
workflow: test
tasks:
  - id: step1
    infer: \"Hello\"
  - id: step2
    with:
      data: $setp1
    infer: \"World\"
";
    let diags = vec![DiagnosticInfo {
        code: "NIKA-140".to_string(),
        message: "Unknown task 'setp1' referenced in with block".to_string(),
        start_offset: yaml.find("$setp1").unwrap() as u32 + 1, // after $
        end_offset: yaml.find("$setp1").unwrap() as u32 + 6,
    }];
    let actions = code_actions_with_diagnostics(yaml, 0, 0, &diags);
    let fix = actions.iter().find(|a| a.title.contains("step1"));
    assert!(fix.is_some(), "Should suggest 'step1' for typo 'setp1'");
    assert_eq!(fix.unwrap().kind, CodeActionKind::QuickFix);
    assert!(fix.unwrap().is_preferred);
}

#[test]
fn duplicate_task_diagnostic_suggests_rename() {
    let yaml = "\
schema: \"@0.12\"
model: test
workflow: test
tasks:
  - id: step1
    infer: \"Hello\"
  - id: step1
    infer: \"World\"
";
    let dup_offset = yaml.rfind("id: step1").unwrap() as u32;
    let diags = vec![DiagnosticInfo {
        code: "NIKA-141".to_string(),
        message: "Duplicate task ID 'step1'".to_string(),
        start_offset: dup_offset + 4,
        end_offset: dup_offset + 9,
    }];
    let actions = code_actions_with_diagnostics(yaml, dup_offset, dup_offset, &diags);
    let fix = actions.iter().find(|a| a.title.contains("Rename"));
    assert!(fix.is_some(), "Should offer rename for duplicate task");
}

#[test]
fn invalid_schema_diagnostic_suggests_fix() {
    let yaml = "schema: \"@0.11\"\nmodel: test\nworkflow: test\ntasks:\n  - id: s\n    infer: x\n";
    let diags = vec![DiagnosticInfo {
        code: "NIKA-142".to_string(),
        message: "Invalid schema version".to_string(),
        start_offset: 0,
        end_offset: 15,
    }];
    let actions = code_actions_with_diagnostics(yaml, 0, 0, &diags);
    let fix = actions.iter().find(|a| a.title.contains("@0.12"));
    assert!(fix.is_some(), "Should offer schema upgrade");
}

#[test]
fn missing_field_diagnostic_suggests_add() {
    let yaml = "schema: \"@0.12\"\nmodel: test\ntasks:\n  - infer: \"hello\"\n";
    let diags = vec![DiagnosticInfo {
        code: "NIKA-145".to_string(),
        message: "Missing required field 'id'".to_string(),
        start_offset: yaml.find("- infer").unwrap() as u32,
        end_offset: yaml.find("- infer").unwrap() as u32 + 7,
    }];
    let actions = code_actions_with_diagnostics(yaml, 0, 0, &diags);
    let fix = actions.iter().find(|a| a.title.contains("id"));
    assert!(fix.is_some(), "Should offer 'Add id field'");
}

#[test]
fn missing_model_diagnostic_suggests_add() {
    let yaml = "\
schema: \"@0.12\"
workflow: test
tasks:
  - id: step1
    infer: \"Hello\"
";
    let diags = vec![DiagnosticInfo {
        code: "NIKA-034".to_string(),
        message: "model: required for infer verb".to_string(),
        start_offset: yaml.find("infer:").unwrap() as u32,
        end_offset: yaml.find("infer:").unwrap() as u32 + 6,
    }];
    let actions = code_actions_with_diagnostics(yaml, 0, 0, &diags);
    let fix = actions.iter().find(|a| a.title.contains("model"));
    assert!(fix.is_some(), "Should offer 'Add model' for NIKA-034");
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p nika-lsp-core --test code_action_e2e unknown_task_diagnostic -- --nocapture
```
Expected: FAIL — `DiagnosticInfo` and `code_actions_with_diagnostics` don't exist yet.

**Step 3: Implement DiagnosticInfo + code_actions_with_diagnostics**

Add to `tools/nika-lsp-core/src/handlers/code_action.rs` (after existing code, before tests):

```rust
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
            if id.is_empty() { None } else { Some(id.to_string()) }
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
        ("Add missing id field".into(), format!("{}id: new_task\n{}", indent_str, indent_str))
    } else if msg.contains("'schema'") {
        ("Add missing schema".into(), "schema: \"@0.12\"\n".into())
    } else if msg.contains("'tasks'") {
        ("Add missing tasks block".into(), "tasks:\n  - id: step1\n    infer: \"TODO\"\n".into())
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
    if a == b { return 1.0; }
    if a.is_empty() || b.is_empty() { return 0.0; }

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
    let prefix = if a.starts_with(b) || b.starts_with(a) { 0.2 } else { 0.0 };

    (lcs * 0.5 + overlap * 0.3 + prefix).min(1.0)
}
```

**Step 4: Update handler trait**

In `tools/nika-lsp-core/src/handler.rs`, add to `LspHandler` trait and `DefaultHandler`:

```rust
// Add to trait:
fn code_actions_with_diagnostics(
    &self,
    text: &str,
    start: u32,
    end: u32,
    diagnostics: &[crate::handlers::code_action::DiagnosticInfo],
) -> Vec<CodeActionEntry>;

// Add to DefaultHandler impl:
fn code_actions_with_diagnostics(
    &self,
    text: &str,
    start: u32,
    end: u32,
    diagnostics: &[crate::handlers::code_action::DiagnosticInfo],
) -> Vec<CodeActionEntry> {
    crate::handlers::code_action::code_actions_with_diagnostics(text, start, end, diagnostics)
}
```

**Step 5: Run tests to verify they pass**

```bash
cargo test -p nika-lsp-core --test code_action_e2e -- --nocapture
cargo test -p nika-lsp-core --lib -- --nocapture
```
Expected: ALL PASS

**Step 6: Commit**

```bash
git add tools/nika-lsp-core/src/handlers/code_action.rs tools/nika-lsp-core/src/handler.rs tools/nika-lsp-core/tests/code_action_e2e.rs
git commit -m "feat(lsp): add diagnostic-linked code actions to nika-lsp-core

Adds code_actions_with_diagnostics() supporting NIKA-140 (unknown task
with fuzzy match), NIKA-141 (duplicate → rename), NIKA-142 (schema fix),
NIKA-145 (missing field), NIKA-034 (missing model with provider-aware default).

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2: Wire diagnostic code actions in nika-lsp backend

Currently `backend.rs` calls `self.handler.code_actions(&text, start, end)` which ignores diagnostics. We need to pass `params.context.diagnostics` through.

**Files:**
- Modify: `tools/nika-lsp/src/backend.rs` (code_action method, ~line 435)

**Step 1: Modify the code_action handler**

In `tools/nika-lsp/src/backend.rs`, replace the `code_action` method body:

```rust
async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    let uri = &params.text_document.uri;
    let text = match self.documents.get(uri) {
        Some(d) => d.content(),
        None => return Ok(None),
    };

    let start = crate::position::position_to_offset(
        &text,
        params.range.start.line,
        params.range.start.character,
    )
    .map(|o| o.0)
    .unwrap_or(0);
    let end = crate::position::position_to_offset(
        &text,
        params.range.end.line,
        params.range.end.character,
    )
    .map(|o| o.0)
    .unwrap_or(0);

    // Convert LSP diagnostics to protocol-agnostic DiagnosticInfo
    let diag_infos: Vec<nika_lsp_core::handlers::code_action::DiagnosticInfo> = params
        .context
        .diagnostics
        .iter()
        .filter_map(|d| {
            let code = match d.code.as_ref()? {
                tower_lsp_server::ls_types::NumberOrString::String(s) => s.clone(),
                tower_lsp_server::ls_types::NumberOrString::Number(n) => format!("NIKA-{:03}", n),
            };
            let d_start = crate::position::position_to_offset(
                &text, d.range.start.line, d.range.start.character,
            ).map(|o| o.0).unwrap_or(0);
            let d_end = crate::position::position_to_offset(
                &text, d.range.end.line, d.range.end.character,
            ).map(|o| o.0).unwrap_or(0);
            Some(nika_lsp_core::handlers::code_action::DiagnosticInfo {
                code,
                message: d.message.clone(),
                start_offset: d_start,
                end_offset: d_end,
            })
        })
        .collect();

    let entries = self.handler.code_actions_with_diagnostics(&text, start, end, &diag_infos);

    let actions: Vec<CodeActionOrCommand> = entries
        .into_iter()
        .filter_map(|e| {
            let edit = e.edit?;
            let range = offset_range(&text, edit.offset, edit.end_offset);
            let kind = match e.kind {
                nika_lsp_core::handlers::code_action::CodeActionKind::QuickFix => {
                    CodeActionKind::QUICKFIX
                }
                nika_lsp_core::handlers::code_action::CodeActionKind::Refactor => {
                    CodeActionKind::REFACTOR
                }
            };
            let mut changes = std::collections::HashMap::new();
            changes.insert(
                uri.clone(),
                vec![tower_lsp_server::ls_types::TextEdit {
                    range,
                    new_text: edit.new_text,
                }],
            );
            Some(CodeActionOrCommand::CodeAction(CodeAction {
                title: e.title,
                kind: Some(kind),
                is_preferred: Some(e.is_preferred),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                ..Default::default()
            }))
        })
        .collect();

    if actions.is_empty() {
        return Ok(None);
    }
    Ok(Some(actions))
}
```

**Step 2: Run full test suite**

```bash
cargo test -p nika-lsp-core --lib --test code_action_e2e
cargo check -p nika-lsp
```
Expected: PASS

**Step 3: Commit**

```bash
git add tools/nika-lsp/src/backend.rs
git commit -m "feat(lsp): wire diagnostic-linked code actions in nika-lsp backend

Converts LSP Diagnostics to DiagnosticInfo and passes them through to
code_actions_with_diagnostics for NIKA-140/141/142/145/034 quick fixes.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 2: CodeLens (migrate from nika-engine)

### Task 3: Port CodeLens handler to nika-lsp-core

The existing `nika-engine/src/lsp/handlers/code_lens.rs` uses `tower_lsp_server::ls_types::*` directly. We need a protocol-agnostic version.

**Files:**
- Create: `tools/nika-lsp-core/src/handlers/code_lens.rs`
- Modify: `tools/nika-lsp-core/src/handlers/mod.rs`
- Modify: `tools/nika-lsp-core/src/handler.rs`
- Test: `tools/nika-lsp-core/tests/code_lens_e2e.rs`

**Step 1: Write the failing test**

Create `tools/nika-lsp-core/tests/code_lens_e2e.rs`:

```rust
//! E2E tests for code_lens handler.
use nika_lsp_core::handlers::code_lens::{code_lenses, CodeLensEntry, LensCommand};

#[test]
fn validate_on_schema_line() {
    let yaml = "schema: \"@0.12\"\nworkflow: test\ntasks:\n  - id: s\n    infer: x\n";
    let lenses = code_lenses(yaml);
    let validate = lenses.iter().find(|l| l.command == LensCommand::Validate);
    assert!(validate.is_some());
    assert_eq!(validate.unwrap().line, 0);
}

#[test]
fn run_on_tasks_line() {
    let yaml = "schema: \"@0.12\"\ntasks:\n  - id: s\n    infer: x\n";
    let lenses = code_lenses(yaml);
    let run = lenses.iter().find(|l| l.command == LensCommand::Run);
    assert!(run.is_some());
}

#[test]
fn task_count_label() {
    let yaml = "tasks:\n  - id: a\n    exec: x\n  - id: b\n    exec: y\n  - id: c\n    exec: z\n";
    let lenses = code_lenses(yaml);
    let count = lenses.iter().find(|l| matches!(l.command, LensCommand::TaskCount(_)));
    assert!(count.is_some());
    if let LensCommand::TaskCount(n) = count.unwrap().command {
        assert_eq!(n, 3);
    }
}

#[test]
fn no_lenses_on_empty() {
    assert!(code_lenses("").is_empty());
}
```

**Step 2: Implement code_lens handler**

Create `tools/nika-lsp-core/src/handlers/code_lens.rs`:

```rust
//! CodeLens handler — inline actionable buttons.
//!
//! Protocol-agnostic: returns `Vec<CodeLensEntry>` with line + command.
//! The tower-lsp shim converts to `CodeLens`.

/// Protocol-agnostic code lens entry.
#[derive(Debug, Clone)]
pub struct CodeLensEntry {
    pub line: u32,
    pub command: LensCommand,
}

/// Commands that a code lens can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LensCommand {
    /// "▶ Run Workflow" button
    Run,
    /// "✓ Validate" button
    Validate,
    /// "N tasks" info badge
    TaskCount(usize),
}

impl LensCommand {
    pub fn title(&self) -> String {
        match self {
            Self::Run => "▶ Run Workflow".into(),
            Self::Validate => "✓ Validate".into(),
            Self::TaskCount(n) => format!("{} task{}", n, if *n == 1 { "" } else { "s" }),
        }
    }

    pub fn vscode_command(&self) -> &'static str {
        match self {
            Self::Run => "nika.runWorkflow",
            Self::Validate => "nika.checkWorkflow",
            Self::TaskCount(_) => "nika.showTasks",
        }
    }
}

/// Compute code lenses for the document.
pub fn code_lenses(text: &str) -> Vec<CodeLensEntry> {
    let mut lenses = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    let task_count = lines
        .iter()
        .filter(|l| l.trim().starts_with("- id:"))
        .count();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("schema:") || trimmed.starts_with("workflow:") {
            lenses.push(CodeLensEntry {
                line: i as u32,
                command: LensCommand::Validate,
            });
        }

        if trimmed == "tasks:" {
            lenses.push(CodeLensEntry {
                line: i as u32,
                command: LensCommand::Run,
            });
            if task_count > 0 {
                lenses.push(CodeLensEntry {
                    line: i as u32,
                    command: LensCommand::TaskCount(task_count),
                });
            }
        }
    }

    lenses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lens_command_titles() {
        assert_eq!(LensCommand::Run.title(), "▶ Run Workflow");
        assert_eq!(LensCommand::TaskCount(1).title(), "1 task");
        assert_eq!(LensCommand::TaskCount(5).title(), "5 tasks");
    }
}
```

**Step 3: Register in mod.rs and handler.rs**

Add to `tools/nika-lsp-core/src/handlers/mod.rs`:
```rust
pub mod code_lens;
```

Add to `LspHandler` trait and `DefaultHandler` in `handler.rs`:
```rust
// trait
fn code_lenses(&self, text: &str) -> Vec<crate::handlers::code_lens::CodeLensEntry>;

// impl
fn code_lenses(&self, text: &str) -> Vec<crate::handlers::code_lens::CodeLensEntry> {
    crate::handlers::code_lens::code_lenses(text)
}
```

**Step 4: Run tests**

```bash
cargo test -p nika-lsp-core --test code_lens_e2e -- --nocapture
cargo test -p nika-lsp-core --lib -- --nocapture
```

**Step 5: Commit**

```bash
git add tools/nika-lsp-core/src/handlers/code_lens.rs tools/nika-lsp-core/src/handlers/mod.rs tools/nika-lsp-core/src/handler.rs tools/nika-lsp-core/tests/code_lens_e2e.rs
git commit -m "feat(lsp): add CodeLens handler to nika-lsp-core

Ported from nika-engine embedded LSP. Shows Run, Validate, and task count
badges. Protocol-agnostic pure function.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 4: Wire CodeLens in nika-lsp backend + register capability

**Files:**
- Modify: `tools/nika-lsp/src/backend.rs`

**Step 1: Add capability + handler**

In `backend.rs`, add to `ServerCapabilities` in `initialize()`:
```rust
code_lens_provider: Some(CodeLensOptions {
    resolve_provider: Some(false),
}),
```

Add the `code_lens` handler method to `impl LanguageServer for NikaBackend`:
```rust
async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
    let uri = &params.text_document.uri;
    let text = match self.documents.get(uri) {
        Some(d) => d.content(),
        None => return Ok(None),
    };

    let entries = self.handler.code_lenses(&text);

    let lenses: Vec<CodeLens> = entries
        .into_iter()
        .map(|e| {
            let range = Range {
                start: Position { line: e.line, character: 0 },
                end: Position { line: e.line, character: 0 },
            };
            CodeLens {
                range,
                command: Some(Command {
                    title: e.command.title(),
                    command: e.command.vscode_command().to_string(),
                    arguments: None,
                }),
                data: None,
            }
        })
        .collect();

    if lenses.is_empty() {
        Ok(None)
    } else {
        Ok(Some(lenses))
    }
}
```

**Step 2: Run checks**

```bash
cargo check -p nika-lsp
cargo test -p nika-lsp-core --lib
```

**Step 3: Commit**

```bash
git add tools/nika-lsp/src/backend.rs
git commit -m "feat(lsp): wire CodeLens in nika-lsp backend

Registers code_lens_provider capability. Shows Run/Validate/TaskCount
lenses on schema:, workflow:, and tasks: lines.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 3: Inlay Hints (migrate + extend)

### Task 5: Port InlayHints handler to nika-lsp-core

Existing `nika-engine/src/lsp/handlers/inlay_hints.rs` has 5 hints. We port them + add model cost hint.

**Files:**
- Create: `tools/nika-lsp-core/src/handlers/inlay_hints.rs`
- Modify: `tools/nika-lsp-core/src/handlers/mod.rs`
- Modify: `tools/nika-lsp-core/src/handler.rs`
- Test: `tools/nika-lsp-core/tests/inlay_hints_e2e.rs`

**Step 1: Write the failing tests**

Create `tools/nika-lsp-core/tests/inlay_hints_e2e.rs`:

```rust
//! E2E tests for inlay_hints handler.
use nika_lsp_core::handlers::inlay_hints::{inlay_hints, InlayHintEntry, HintKind};

#[test]
fn timeout_seconds() {
    let yaml = "    timeout: 30\n";
    let hints = inlay_hints(yaml, 0, yaml.len() as u32);
    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.contains("seconds"));
}

#[test]
fn timeout_minutes() {
    let hints = inlay_hints("    timeout: 120\n", 0, 100);
    assert!(hints[0].label.contains("2min"));
}

#[test]
fn binding_source() {
    let hints = inlay_hints("      data: $step1\n", 0, 100);
    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.contains("step1"));
}

#[test]
fn depends_on_count() {
    let hints = inlay_hints("    depends_on: [a, b, c]\n", 0, 100);
    assert!(hints[0].label.contains("3 dep"));
}

#[test]
fn max_turns_hint() {
    let hints = inlay_hints("      max_turns: 10\n", 0, 100);
    assert!(hints[0].label.contains("iterations"));
}

#[test]
fn concurrency_hint() {
    let hints = inlay_hints("    concurrency: 5\n", 0, 100);
    assert!(hints[0].label.contains("parallel"));
}

#[test]
fn model_cost_hint() {
    let yaml = "    model: claude-sonnet-4-20250514\n";
    let hints = inlay_hints(yaml, 0, yaml.len() as u32);
    assert_eq!(hints.len(), 1);
    assert!(hints[0].label.contains("$"), "Should show cost: {:?}", hints[0].label);
}

#[test]
fn no_hints_for_regular_line() {
    let hints = inlay_hints("    infer: \"hello\"\n", 0, 100);
    assert!(hints.is_empty());
}

#[test]
fn multiple_hints() {
    let text = "\
tasks:
  - id: step1
    infer: \"Generate\"
    timeout: 30
    model: gpt-4o

  - id: step2
    with:
      data: $step1
    exec: \"echo\"
    depends_on: [step1]
";
    let hints = inlay_hints(text, 0, text.len() as u32);
    assert!(hints.len() >= 4, "Expected at least 4 hints, got {}", hints.len());
}
```

**Step 2: Implement inlay_hints handler**

Create `tools/nika-lsp-core/src/handlers/inlay_hints.rs`:

```rust
//! Inlay hints handler — inline annotations.
//!
//! Protocol-agnostic: returns `Vec<InlayHintEntry>`.
//! Covers: timeout, bindings, depends_on count, max_turns,
//! concurrency, and model cost annotations.

/// Protocol-agnostic inlay hint.
#[derive(Debug, Clone)]
pub struct InlayHintEntry {
    /// Line number (0-based).
    pub line: u32,
    /// Character offset (end of line for suffix hints).
    pub character: u32,
    /// Hint text to display.
    pub label: String,
    /// Tooltip on hover.
    pub tooltip: String,
    /// Hint kind.
    pub kind: HintKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintKind {
    Type,
    Parameter,
}

/// Compute inlay hints for the byte range [start_offset, end_offset).
pub fn inlay_hints(text: &str, start_offset: u32, end_offset: u32) -> Vec<InlayHintEntry> {
    let mut hints = Vec::new();

    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }

        let line_char_len = line.chars().map(|c| c.len_utf16()).sum::<usize>() as u32;

        // 1. timeout: N → "N seconds" / "(Nm Ns)"
        if let Some(rest) = trimmed.strip_prefix("timeout:") {
            if let Ok(secs) = rest.trim().parse::<u64>() {
                let label = if secs == 1 {
                    " second".into()
                } else if secs < 60 {
                    " seconds".into()
                } else {
                    let (m, s) = (secs / 60, secs % 60);
                    if s == 0 { format!(" ({}min)", m) } else { format!(" ({}m{}s)", m, s) }
                };
                hints.push(InlayHintEntry {
                    line: i as u32,
                    character: line_char_len,
                    label,
                    tooltip: "Nika timeout is always in seconds".into(),
                    kind: HintKind::Type,
                });
            }
        }

        // 2. alias: $task_ref → "← task_ref output"
        if trimmed.contains(": $") && !trimmed.starts_with('-') {
            if let Some(dp) = trimmed.find(": $") {
                let task_ref: String = trimmed[dp + 3..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                if !task_ref.is_empty() {
                    hints.push(InlayHintEntry {
                        line: i as u32,
                        character: line_char_len,
                        label: format!(" <- {} output", task_ref),
                        tooltip: format!("Binds the output of task '{}'", task_ref),
                        kind: HintKind::Type,
                    });
                }
            }
        }

        // 3. depends_on: [a, b, c] → "(3 deps)"
        if let Some(rest) = trimmed.strip_prefix("depends_on:") {
            let deps_str = rest.trim();
            if deps_str.starts_with('[') {
                let count = deps_str
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .count();
                if count > 0 {
                    hints.push(InlayHintEntry {
                        line: i as u32,
                        character: line_char_len,
                        label: format!(" ({} dep{})", count, if count == 1 { "" } else { "s" }),
                        tooltip: "Number of upstream dependencies".into(),
                        kind: HintKind::Type,
                    });
                }
            }
        }

        // 4. max_turns: N → "iterations"
        if let Some(rest) = trimmed.strip_prefix("max_turns:") {
            if rest.trim().parse::<u64>().is_ok() {
                hints.push(InlayHintEntry {
                    line: i as u32,
                    character: line_char_len,
                    label: " iterations".into(),
                    tooltip: "Maximum agent loop iterations".into(),
                    kind: HintKind::Type,
                });
            }
        }

        // 5. concurrency: N → "parallel"
        if let Some(rest) = trimmed.strip_prefix("concurrency:") {
            if rest.trim().parse::<u64>().is_ok() {
                hints.push(InlayHintEntry {
                    line: i as u32,
                    character: line_char_len,
                    label: " parallel".into(),
                    tooltip: "Max parallel for_each iterations".into(),
                    kind: HintKind::Type,
                });
            }
        }

        // 6. model: <name> → "(Provider · $X/$Y per 1M)"
        if let Some(rest) = trimmed.strip_prefix("model:") {
            let model = rest.trim().trim_matches('"').trim_matches('\'');
            if let Some(cost_label) = model_cost_label(model) {
                hints.push(InlayHintEntry {
                    line: i as u32,
                    character: line_char_len,
                    label: cost_label,
                    tooltip: "Cost per million tokens (input/output)".into(),
                    kind: HintKind::Type,
                });
            }
        }
    }

    hints
}

/// Get a cost label for a known model.
///
/// Returns None for unknown models. Format: " (Provider · $in/$out per 1M)"
fn model_cost_label(model: &str) -> Option<String> {
    // Static table — keep in sync with nika-engine/src/provider/cost.rs
    let (provider, input, output) = match model {
        // Anthropic
        m if m.contains("opus-4") => ("Anthropic", 15.0, 75.0),
        m if m.contains("sonnet-4") => ("Anthropic", 3.0, 15.0),
        m if m.contains("haiku-3.5") || m.contains("haiku-4") => ("Anthropic", 0.8, 4.0),
        // OpenAI
        "gpt-4o" | "gpt-4o-2024-08-06" => ("OpenAI", 2.5, 10.0),
        "gpt-4o-mini" => ("OpenAI", 0.15, 0.6),
        m if m.starts_with("gpt-4.1") && !m.contains("mini") && !m.contains("nano") => ("OpenAI", 2.0, 8.0),
        m if m.starts_with("gpt-4.1-mini") => ("OpenAI", 0.4, 1.6),
        m if m.starts_with("gpt-4.1-nano") => ("OpenAI", 0.1, 0.4),
        "o1" => ("OpenAI", 15.0, 60.0),
        "o3-mini" => ("OpenAI", 1.1, 4.4),
        // Mistral
        "mistral-large-latest" | "mistral-large" => ("Mistral", 2.0, 6.0),
        "mistral-small-latest" | "mistral-small" => ("Mistral", 0.2, 0.6),
        // Groq
        m if m.contains("llama") && m.contains("70b") => ("Groq", 0.59, 0.79),
        m if m.contains("llama") && m.contains("8b") => ("Groq", 0.05, 0.08),
        // DeepSeek
        "deepseek-chat" => ("DeepSeek", 0.14, 0.28),
        "deepseek-reasoner" => ("DeepSeek", 0.55, 2.19),
        // Gemini
        m if m.contains("gemini") && m.contains("flash") => ("Gemini", 0.1, 0.4),
        m if m.contains("gemini") && m.contains("pro") => ("Gemini", 1.25, 5.0),
        // xAI
        "grok-3" => ("xAI", 3.0, 15.0),
        "grok-3-mini" => ("xAI", 0.3, 0.5),
        _ => return None,
    };

    Some(format!(" ({} · ${}/{})", provider, format_price(input), format_price(output)))
}

fn format_price(price: f64) -> String {
    if price >= 1.0 {
        format!("{}", price)
    } else {
        format!("{:.2}", price)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_claude_sonnet() {
        let label = model_cost_label("claude-sonnet-4-20250514");
        assert!(label.is_some());
        assert!(label.unwrap().contains("Anthropic"));
    }

    #[test]
    fn cost_gpt4o() {
        let label = model_cost_label("gpt-4o");
        assert!(label.is_some());
        assert!(label.unwrap().contains("OpenAI"));
    }

    #[test]
    fn cost_unknown() {
        assert!(model_cost_label("some-random-model").is_none());
    }
}
```

**Step 3: Register and wire**

Add `pub mod inlay_hints;` to `handlers/mod.rs`.

Add to `LspHandler` trait and `DefaultHandler`:
```rust
// trait
fn inlay_hints(&self, text: &str, start: u32, end: u32) -> Vec<crate::handlers::inlay_hints::InlayHintEntry>;

// impl
fn inlay_hints(&self, text: &str, start: u32, end: u32) -> Vec<crate::handlers::inlay_hints::InlayHintEntry> {
    crate::handlers::inlay_hints::inlay_hints(text, start, end)
}
```

**Step 4: Run tests**

```bash
cargo test -p nika-lsp-core --test inlay_hints_e2e -- --nocapture
cargo test -p nika-lsp-core --lib -- --nocapture
```

**Step 5: Commit**

```bash
git add tools/nika-lsp-core/src/handlers/inlay_hints.rs tools/nika-lsp-core/src/handlers/mod.rs tools/nika-lsp-core/src/handler.rs tools/nika-lsp-core/tests/inlay_hints_e2e.rs
git commit -m "feat(lsp): add InlayHints handler to nika-lsp-core

Ports 5 hints from embedded LSP (timeout, bindings, depends_on, max_turns,
concurrency) and adds model cost annotation showing provider + $/M pricing.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 6: Wire InlayHints in nika-lsp backend

**Files:**
- Modify: `tools/nika-lsp/src/backend.rs`

**Step 1: Add capability**

In `initialize()`, add to `ServerCapabilities`:
```rust
inlay_hint_provider: Some(OneOf::Left(true)),
```

**Step 2: Add handler method**

```rust
async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
    let uri = &params.text_document.uri;
    let text = match self.documents.get(uri) {
        Some(d) => d.content(),
        None => return Ok(None),
    };

    let start = crate::position::position_to_offset(
        &text, params.range.start.line, params.range.start.character,
    ).map(|o| o.0).unwrap_or(0);
    let end = crate::position::position_to_offset(
        &text, params.range.end.line, params.range.end.character,
    ).map(|o| o.0).unwrap_or(text.len() as u32);

    let entries = self.handler.inlay_hints(&text, start, end);

    let hints: Vec<InlayHint> = entries
        .into_iter()
        .map(|e| InlayHint {
            position: Position { line: e.line, character: e.character },
            label: InlayHintLabel::String(e.label),
            kind: Some(match e.kind {
                nika_lsp_core::handlers::inlay_hints::HintKind::Type => InlayHintKind::TYPE,
                nika_lsp_core::handlers::inlay_hints::HintKind::Parameter => InlayHintKind::PARAMETER,
            }),
            tooltip: Some(InlayHintTooltip::String(e.tooltip)),
            text_edits: None,
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
        })
        .collect();

    if hints.is_empty() { Ok(None) } else { Ok(Some(hints)) }
}
```

**Step 3: Verify + Commit**

```bash
cargo check -p nika-lsp
git add tools/nika-lsp/src/backend.rs
git commit -m "feat(lsp): wire InlayHints in nika-lsp backend

Registers inlay_hint_provider. Shows timeout units, binding sources,
dep counts, iteration counts, concurrency labels, and model cost.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 4: VS Code extension updates

### Task 7: Register showTasks command

The CodeLens uses `nika.showTasks` which isn't registered in the extension.

**Files:**
- Modify: `editors/vscode/package.json`
- Modify: `editors/vscode/src/extension.ts`

**Step 1: Add command to package.json**

In `contributes.commands` array, add:
```json
{
  "command": "nika.showTasks",
  "title": "Nika: Show Tasks (Outline)",
  "icon": "$(list-tree)"
}
```

**Step 2: Register command in extension.ts**

Add to `activate()`:
```typescript
// Command: Show tasks (focus outline view)
context.subscriptions.push(
  commands.registerCommand('nika.showTasks', () => {
    commands.executeCommand('workbench.action.focusOutline');
  }),
);
```

**Step 3: Commit**

```bash
git add editors/vscode/package.json editors/vscode/src/extension.ts
git commit -m "feat(vscode): register nika.showTasks command for CodeLens

The task count CodeLens now focuses the Outline panel when clicked.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 5: Full integration test

### Task 8: Run full workspace test + clippy

**Step 1: Run workspace tests**

```bash
cargo test -p nika-lsp-core --lib --test code_action_e2e --test code_lens_e2e --test inlay_hints_e2e
```

**Step 2: Run clippy**

```bash
cargo clippy -p nika-lsp-core -p nika-lsp -- -D warnings
```

**Step 3: Fix any warnings**

Fix all clippy warnings that appear. Typical: unused imports, needless borrows.

**Step 4: Run full workspace test**

```bash
cargo test --workspace --lib
```

**Step 5: Commit any fixes**

```bash
git add -A
git commit -m "fix(lsp): clippy + test fixes for code actions batch

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Summary of all changes

| File | Change |
|------|--------|
| `nika-lsp-core/src/handlers/code_action.rs` | +DiagnosticInfo, +code_actions_with_diagnostics, +fuzzy match, +5 fixers |
| `nika-lsp-core/src/handlers/code_lens.rs` | **NEW** — CodeLensEntry, LensCommand, code_lenses() |
| `nika-lsp-core/src/handlers/inlay_hints.rs` | **NEW** — InlayHintEntry, inlay_hints() + model cost |
| `nika-lsp-core/src/handlers/mod.rs` | +code_lens, +inlay_hints |
| `nika-lsp-core/src/handler.rs` | +code_actions_with_diagnostics, +code_lenses, +inlay_hints |
| `nika-lsp-core/tests/code_action_e2e.rs` | +5 diagnostic-linked tests |
| `nika-lsp-core/tests/code_lens_e2e.rs` | **NEW** — 4 tests |
| `nika-lsp-core/tests/inlay_hints_e2e.rs` | **NEW** — 9 tests |
| `nika-lsp/src/backend.rs` | Wire code_action (diagnostics), +code_lens, +inlay_hint, +capabilities |
| `editors/vscode/package.json` | +nika.showTasks command |
| `editors/vscode/src/extension.ts` | +showTasks handler |

**New e2e test count:** ~18 new tests across 3 test files
**Estimated effort:** ~45 min autonomous execution
