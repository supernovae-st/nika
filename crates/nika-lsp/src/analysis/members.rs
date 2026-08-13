// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `${{ inputs. / const. / secrets. }}` member lanes — the workflow's
//! OWN declarations offered at the island, parse-first with a line-scan
//! fallback for mid-keystroke documents (the `scan_task_ids` spirit).

use lsp_types::{CompletionItem, CompletionItemKind};
use nika_schema::{FileId, ParseMode, parse};

/// An open island ending in `inputs.` / `const.` / `secrets.` — the member
/// position for the workflow's OWN declarations.
pub(super) fn template_member_root(prefix: &str) -> Option<&'static str> {
    let island = prefix.rfind("${{")?;
    let after = prefix.get(island..).unwrap_or("");
    if after.contains("}}") {
        return None;
    }
    let t = after.trim_end();
    for root in ["inputs", "const", "secrets", "with"] {
        if t.ends_with(&format!("{root}.")) {
            return Some(root);
        }
    }
    None
}

/// The file's own declared members for one island root — a lenient parse
/// of the document itself, so the workflow teaches its own names. A
/// mid-keystroke document that no longer parses falls back to a line
/// scan of the block (same spirit as `scan_task_ids`).
/// The `with.` lane — the ENCLOSING task's own aliases (spec 04: `with`
/// is task-local; another task's aliases are not in scope). Parse-first
/// with the same line-scan fallback discipline as the other roots.
pub(super) fn with_items(text: &str, offset: usize) -> Vec<CompletionItem> {
    let Some(editing) = super::scope::current_task_id(text, offset) else {
        return Vec::new();
    };
    if let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient)
        && let Some(task) = wf.tasks.iter().find(|t| t.value.id.value == editing)
    {
        return task
            .value
            .with
            .iter()
            .map(|(name, _)| member_item(&name.value, "with · this task's alias".to_owned()))
            .collect();
    }
    scan_task_with_keys(text, &editing)
        .into_iter()
        .map(|name| member_item(&name, "with · this task's alias".to_owned()))
        .collect()
}

/// Line-scan fallback: the `with:` child keys of the task block named
/// `editing` (mid-keystroke documents keep their aliases).
fn scan_task_with_keys(text: &str, editing: &str) -> Vec<String> {
    let mut in_task = false;
    let mut in_with = false;
    let mut with_indent = 0usize;
    let mut keys = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        let indent = line.len() - t.len();
        // W1 « the map »: the task boundary is the bare `name:` key at
        // indent 2 — never an `- id:` row.
        if indent == 2
            && let Some(name) = t
                .split('#')
                .next()
                .unwrap_or("")
                .trim_end()
                .strip_suffix(':')
            && !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            in_task = name == editing;
            in_with = false;
            continue;
        }
        if in_task && t.starts_with("with:") {
            in_with = true;
            with_indent = indent;
            continue;
        }
        if in_with {
            if !t.is_empty() && indent <= with_indent {
                in_with = false;
                continue;
            }
            if indent == with_indent + 2
                && let Some((name, _)) = t.split_once(':')
                && !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                keys.push(name.to_owned());
            }
        }
    }
    keys
}

pub(super) fn member_items(text: &str, root: &str) -> Vec<CompletionItem> {
    let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient) else {
        return scan_block_keys(text, root)
            .into_iter()
            .map(|name| {
                member_item(
                    &name,
                    match root {
                        "inputs" => "input".to_owned(),
                        "const" => "const".to_owned(),
                        _ => "secret · masked, never echoed".to_owned(),
                    },
                )
            })
            .collect();
    };
    let mut items = Vec::new();
    match root {
        "inputs" => {
            for (name, decl) in &wf.inputs {
                let detail = match decl {
                    nika_schema::VarDecl::Typed {
                        r#type,
                        required,
                        default,
                        description,
                        ..
                    } => {
                        use std::fmt::Write as _;
                        let mut d = format!(
                            "input · {}",
                            nika_schema::types::type_expr_display(&r#type.value)
                        );
                        if *required {
                            d.push_str(" · required");
                        }
                        if let Some(def) = default {
                            let _ = write!(d, " · default {def}");
                        }
                        if let Some(desc) = description {
                            d.push_str(" — ");
                            d.push_str(desc);
                        }
                        d
                    }
                    // The parser enforces typed-only inputs; an untyped
                    // entry (lenient scan) still completes.
                    nika_schema::VarDecl::Untyped(v) => format!("input · {v}"),
                };
                items.push(member_item(&name.value, detail));
            }
        }
        "const" => {
            for (name, decl) in &wf.consts {
                let detail = match decl {
                    nika_schema::VarDecl::Typed { r#type, .. } => {
                        format!(
                            "const · {}",
                            nika_schema::types::type_expr_display(&r#type.value)
                        )
                    }
                    nika_schema::VarDecl::Untyped(v) => format!("const · {v}"),
                };
                items.push(member_item(&name.value, detail));
            }
        }
        _ => {
            for (name, _) in &wf.secrets {
                items.push(member_item(
                    &name.value,
                    "secret · masked, never echoed".to_owned(),
                ));
            }
        }
    }
    items
}

fn member_item(name: &str, detail: String) -> CompletionItem {
    CompletionItem {
        label: name.to_owned(),
        kind: Some(CompletionItemKind::VARIABLE),
        detail: Some(detail),
        ..CompletionItem::default()
    }
}

/// The value-authority names by line shape (`inputs:` · `const:`)
/// — the mid-keystroke fallback the island lanes share (a
/// `for_each:`/`when:` position often sits in a document that no longer
/// parses).
pub(super) fn scan_value_authority_keys(text: &str) -> Vec<String> {
    // Dotted refs (`inputs.x` · `const.x`) — the mid-keystroke
    // fallback offers exactly what a valid island would spell.
    ["inputs", "const"]
        .into_iter()
        .flat_map(|root| {
            scan_block_keys(text, root)
                .into_iter()
                .map(move |k| format!("{root}.{k}"))
        })
        .collect()
}

/// The immediate child keys of a top-level `inputs:` / `const:` /
/// `secrets:` block, by line shape — the fallback when the
/// document mid-keystroke no longer parses.
fn scan_block_keys(text: &str, root: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent == 0 {
            in_block = trimmed.trim_end() == format!("{root}:");
            continue;
        }
        if !in_block || trimmed.is_empty() {
            continue;
        }
        // an immediate child: exactly one indent step, `name:` shape
        if indent == 2
            && let Some((name, _)) = trimmed.split_once(':')
            && !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            keys.push(name.to_owned());
        }
    }
    keys
}

/// An open island ending in `tasks.<id>.` — the TASK member position:
/// the per-task facts the spec names (`output` · `status` · `error`) plus
/// the task's own named `extract:` bindings (04-variables: bindings are
/// addressed as `tasks.<id>.<binding>`).
pub(super) fn template_task_member(prefix: &str) -> Option<String> {
    let island = prefix.rfind("${{")?;
    let after = prefix.get(island..).unwrap_or("");
    if after.contains("}}") {
        return None;
    }
    let t = after.trim_end();
    let rest = t.rfind("tasks.").map(|i| &t[i + "tasks.".len()..])?;
    let (id, tail) = rest.split_once('.')?;
    if tail.is_empty() && !id.is_empty() && id.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Some(id.to_owned());
    }
    None
}

/// The member items for one task: the three spec facts, verb-aware when
/// the task is found, plus the task's named `extract:` bindings. The open
/// `${{` island makes the document YAML-invalid at the very moment this
/// lane fires, so the task facts come from a LINE SCAN of its block
/// (parse-first would be a dead branch here); an unknown id
/// (mid-rename · scratch) still teaches the three facts — silence would
/// read as « nothing exists here ».
pub(super) fn task_member_items(text: &str, task_id: &str) -> Vec<CompletionItem> {
    let scanned = scan_task_block(text, task_id);
    let output_detail = scanned
        .as_ref()
        .and_then(|t| t.verb.as_deref())
        .map_or_else(
            || "the task's recorded output".to_owned(),
            |verb| format!("the task's recorded output ({verb} result)"),
        );
    let mut items = vec![
        member_item("output", output_detail),
        member_item(
            "status",
            "success · failure · skipped · cancelled — gate `when:` on it".to_owned(),
        ),
        member_item(
            "error",
            "the typed error — populated when `on_error.skip` kept it readable".to_owned(),
        ),
    ];
    if let Some(t) = scanned {
        for name in t.bindings {
            items.push(member_item(&name, "named `extract:` binding".to_owned()));
        }
    }
    items
}

struct ScannedTask {
    verb: Option<String>,
    bindings: Vec<String>,
}

/// Line-scan `task_id`'s block: its verb key and its `extract:` binding
/// names. W1 « the map »: the block runs from the task's map key
/// (`<task_id>:` at indent 2) to the next key at the same or shallower
/// indent.
fn scan_task_block(text: &str, task_id: &str) -> Option<ScannedTask> {
    const VERBS: [&str; 4] = ["infer:", "exec:", "invoke:", "agent:"];
    let mut found: Option<usize> = None; // the task key line's indent
    let mut verb = None;
    let mut bindings = Vec::new();
    let mut in_extract = false;
    let mut extract_indent = 0;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        match found {
            None => {
                if indent == 2
                    && let Some(name) = trimmed
                        .split('#')
                        .next()
                        .unwrap_or("")
                        .trim_end()
                        .strip_suffix(':')
                    && name == task_id
                {
                    found = Some(indent);
                }
            }
            Some(item_indent) => {
                if trimmed.is_empty() {
                    continue;
                }
                // the next sibling item or a shallower key ends the block
                if indent <= item_indent && (trimmed.starts_with("- ") || !trimmed.starts_with('-'))
                {
                    break;
                }
                if in_extract {
                    if indent <= extract_indent {
                        in_extract = false;
                    } else if let Some((name, _)) = trimmed.split_once(':')
                        && !name.is_empty()
                        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        bindings.push(name.to_owned());
                        continue;
                    }
                }
                if verb.is_none()
                    && let Some(v) = VERBS.iter().find(|v| trimmed.starts_with(**v))
                {
                    verb = Some(v.trim_end_matches(':').to_owned());
                }
                if trimmed.starts_with("extract:") {
                    in_extract = true;
                    extract_indent = indent;
                }
            }
        }
    }
    found.map(|_| ScannedTask { verb, bindings })
}
