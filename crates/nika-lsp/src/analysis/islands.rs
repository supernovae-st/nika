// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The bare `for_each:` / `when:` value positions — whole `${{ … }}`
//! islands composed from THIS document's declarations (typed array vars
//! float first for a fan-out; upstream tasks ride cycle-safe), so the
//! author picks a working expression instead of recalling the grammar.
//! Once an island is open the generic expression lanes take over — this
//! module only serves the EMPTY value position.

use lsp_types::{CompletionItem, CompletionItemKind};
use nika_schema::{FileId, ParseMode, parse};

use super::{graph, scope};

/// `for_each:` or `when:` with an EMPTY value under the cursor — the
/// whole-island position. A partial non-island value stays silent (the
/// author is typing something of their own).
pub(super) fn island_value_key(prefix: &str) -> Option<&'static str> {
    let trimmed = prefix.trim_start();
    for key in ["for_each", "when"] {
        if let Some(rest) = trimmed.strip_prefix(key)
            && let Some(after) = rest.strip_prefix(':')
            && after.starts_with(' ')
            && after.trim().is_empty()
        {
            return Some(key);
        }
    }
    None
}

struct DocView {
    /// (name, `is_typed_array`) — typed arrays float first for a fan-out.
    vars: Vec<(String, bool)>,
    /// Upstream candidates: every task except the editing one and its
    /// downstream closure (a reference downstream is a cycle).
    upstream: Vec<String>,
    /// Ids already wired into the editing task's `depends_on`.
    wired: Vec<String>,
}

fn doc_view(text: &str, offset: usize) -> DocView {
    let current = scope::current_task_id(text, offset);
    if let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient)
        && !wf.tasks.is_empty()
    {
        let vars = wf
            .vars
            .iter()
            .map(|(name, decl)| {
                let is_array = matches!(
                    decl,
                    nika_schema::VarDecl::Typed {
                        r#type: nika_schema::VarType::Array,
                        ..
                    }
                );
                (name.value.clone(), is_array)
            })
            .collect();
        let illegal: Vec<&str> = current
            .as_deref()
            .map_or_else(Vec::new, |id| graph::illegal_reference_targets(&wf, id));
        let upstream = wf
            .tasks
            .iter()
            .map(|t| t.value.id.value.as_str())
            .filter(|id| Some(*id) != current.as_deref() && !illegal.contains(id))
            .map(str::to_owned)
            .collect();
        let wired = current
            .as_deref()
            .and_then(|id| {
                wf.tasks
                    .iter()
                    .find(|t| t.value.id.value == id)
                    .map(|t| t.value.depends_on.iter().map(|d| d.value.clone()).collect())
            })
            .unwrap_or_default();
        return DocView {
            vars,
            upstream,
            wired,
        };
    }
    // Mid-keystroke fallback — the same honest degradation as task_ids:
    // line scans, self excluded, no closure knowledge.
    DocView {
        vars: super::members::scan_vars_keys(text)
            .into_iter()
            .map(|n| (n, false))
            .collect(),
        upstream: super::completion::scan_task_ids(text)
            .into_iter()
            .filter(|id| Some(id.as_str()) != current.as_deref())
            .collect(),
        wired: Vec::new(),
    }
}

fn island(label: String, detail: String) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail),
        ..CompletionItem::default()
    }
}

/// `for_each:` — the collection candidates: typed array inputs lead,
/// upstream outputs follow (the `depends_on` law named in the detail),
/// other vars offered honestly.
pub(super) fn for_each_items(text: &str, offset: usize) -> Vec<CompletionItem> {
    let view = doc_view(text, offset);
    let mut items = Vec::new();
    for (name, _) in view.vars.iter().filter(|(_, a)| *a) {
        items.push(island(
            format!("${{{{ vars.{name} }}}}"),
            "array input — one run per element".to_owned(),
        ));
    }
    for id in &view.upstream {
        let edge = if view.wired.iter().any(|w| w == id) {
            "edge already wired".to_owned()
        } else {
            format!("needs depends_on: [{id}]")
        };
        items.push(island(
            format!("${{{{ tasks.{id}.output }}}}"),
            format!("upstream array output — {edge}"),
        ));
    }
    for (name, _) in view.vars.iter().filter(|(_, a)| !*a) {
        items.push(island(
            format!("${{{{ vars.{name} }}}}"),
            "runs if it holds a list at launch".to_owned(),
        ));
    }
    items
}

/// `when:` — the CEL v0.1 gate shapes composed from the document: a
/// var as a switch, an upstream status gate, the `size()` empty-check
/// (the subset's one function).
pub(super) fn when_items(text: &str, offset: usize) -> Vec<CompletionItem> {
    let view = doc_view(text, offset);
    let mut items = Vec::new();
    for (name, _) in &view.vars {
        items.push(island(
            format!("${{{{ vars.{name} }}}}"),
            "the input as a boolean switch".to_owned(),
        ));
    }
    for id in &view.upstream {
        let edge = if view.wired.iter().any(|w| w == id) {
            String::new()
        } else {
            format!(" — needs depends_on: [{id}]")
        };
        items.push(island(
            format!("${{{{ tasks.{id}.status == 'success' }}}}"),
            format!("run only when `{id}` succeeded{edge}"),
        ));
        items.push(island(
            format!("${{{{ size(tasks.{id}.output) > 0 }}}}"),
            format!("run only when `{id}` produced content{edge}"),
        ));
    }
    items
}
