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
