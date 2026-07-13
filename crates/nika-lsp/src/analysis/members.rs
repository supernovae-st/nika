// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `${{ vars. / secrets. / env. }}` member lanes — the workflow's
//! OWN declarations offered at the island, parse-first with a line-scan
//! fallback for mid-keystroke documents (the `scan_task_ids` spirit).

use lsp_types::{CompletionItem, CompletionItemKind};
use nika_schema::{FileId, ParseMode, parse};

/// An open island ending in `vars.` / `secrets.` / `env.` — the member
/// position for the workflow's OWN declarations.
pub(super) fn template_member_root(prefix: &str) -> Option<&'static str> {
    let island = prefix.rfind("${{")?;
    let after = prefix.get(island..).unwrap_or("");
    if after.contains("}}") {
        return None;
    }
    let t = after.trim_end();
    for root in ["vars", "secrets", "env"] {
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
pub(super) fn member_items(text: &str, root: &str) -> Vec<CompletionItem> {
    let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient) else {
        return scan_block_keys(text, root)
            .into_iter()
            .map(|name| {
                member_item(
                    &name,
                    match root {
                        "vars" => "var".to_owned(),
                        "secrets" => "secret · masked, never echoed".to_owned(),
                        _ => "env · non-sensitive runtime config".to_owned(),
                    },
                )
            })
            .collect();
    };
    let mut items = Vec::new();
    match root {
        "vars" => {
            for (name, decl) in &wf.vars {
                let detail = match decl {
                    nika_schema::VarDecl::Typed {
                        r#type,
                        required,
                        description,
                        ..
                    } => {
                        let mut d = format!("var · {type}");
                        if *required {
                            d.push_str(" · required");
                        }
                        if let Some(desc) = description {
                            d.push_str(" — ");
                            d.push_str(desc);
                        }
                        d
                    }
                    nika_schema::VarDecl::Untyped(v) => format!("var · default {v}"),
                    // #[non_exhaustive] upstream — an unknown future form
                    // still names itself a var.
                    _ => "var".to_owned(),
                };
                items.push(member_item(&name.value, detail));
            }
        }
        "secrets" => {
            for (name, _) in &wf.secrets {
                items.push(member_item(
                    &name.value,
                    "secret · masked, never echoed".to_owned(),
                ));
            }
        }
        _ => {
            for (name, _) in &wf.env {
                items.push(member_item(
                    &name.value,
                    "env · non-sensitive runtime config".to_owned(),
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

/// The immediate child keys of a top-level `vars:` / `secrets:` / `env:`
/// block, by line shape — the fallback when the document mid-keystroke
/// no longer parses.
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
/// the task's own named `output:` bindings (04-variables: bindings are
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
/// the task is found, plus the task's named `output:` bindings. The open
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
            items.push(member_item(&name, "named `output:` binding".to_owned()));
        }
    }
    items
}

struct ScannedTask {
    verb: Option<String>,
    bindings: Vec<String>,
}

/// Line-scan `task_id`'s block: its verb key and its `output:` binding
/// names. The block runs from `- id: <task_id>` to the next `- ` item at
/// the same indent (or a top-level key).
fn scan_task_block(text: &str, task_id: &str) -> Option<ScannedTask> {
    const VERBS: [&str; 4] = ["infer:", "exec:", "invoke:", "agent:"];
    let mut found: Option<usize> = None; // the `- id:` line's indent
    let mut verb = None;
    let mut bindings = Vec::new();
    let mut in_output = false;
    let mut output_indent = 0;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        match found {
            None => {
                if let Some(rest) = trimmed.strip_prefix("- id:")
                    && rest.split('#').next().unwrap_or("").trim() == task_id
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
                if in_output {
                    if indent <= output_indent {
                        in_output = false;
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
                if trimmed.starts_with("output:") {
                    in_output = true;
                    output_indent = indent;
                }
            }
        }
    }
    found.map(|_| ScannedTask { verb, bindings })
}
