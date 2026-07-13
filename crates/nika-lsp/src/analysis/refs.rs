// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `${{ tasks.` island lane — routed by the law that judges each
//! context (DAG-003 declared edges · the recover carve-out · workflow
//! `outputs:` freedom). The completion dispatch calls in; the shared
//! `task_ids` builder stays in `completion`.

use lsp_types::{CompletionItem, CompletionItemKind};
use nika_schema::{FileId, ParseMode, parse};

use super::completion::task_ids;
use super::scope;

/// A `${{ tasks.` island routes by the law that judges it (probed at
/// the binary, 2026-07-13) ·
/// - a task's NORMAL field: DAG-003 accepts only the task's DIRECT
///   `depends_on` — even a transitive ancestor is refused, so the
///   lane offers exactly the declared edges;
/// - a `recover:` value: exempt from DAG-003 (the spec 05 carve-out),
///   bounded by DAG-004 instead — everything except itself and its
///   downstream closure (the deadlock set);
/// - outside any task (workflow `outputs:`): every task is legal.
pub(super) fn island_task_refs(text: &str, offset: usize) -> Vec<CompletionItem> {
    let Some(editing) = scope::current_task_id(text, offset) else {
        return task_ids(text, None);
    };
    if scope::in_recover_value(text, offset) {
        return task_ids(text, Some(&editing));
    }
    direct_dep_items(text, &editing)
}

/// The editing task's declared `depends_on`, as completion items with
/// verb details — parse-first; a mid-keystroke document falls back to
/// an upward line scan of the task block. No deps declared → empty
/// (silence beats teaching a DAG-003).
fn direct_dep_items(text: &str, editing: &str) -> Vec<CompletionItem> {
    if let Ok(wf) = parse(text, FileId::new(0), ParseMode::Lenient) {
        let verbs: std::collections::BTreeMap<&str, &'static str> = wf
            .tasks
            .iter()
            .map(|t| (t.value.id.value.as_str(), t.value.action.verb()))
            .collect();
        if let Some(task) = wf.tasks.iter().find(|t| t.value.id.value == editing) {
            return task
                .value
                .depends_on
                .iter()
                .map(|d| CompletionItem {
                    label: d.value.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some(verbs.get(d.value.as_str()).map_or_else(
                        || "declared dependency".to_owned(),
                        |v| format!("task ({v}) · declared dependency"),
                    )),
                    ..CompletionItem::default()
                })
                .collect();
        }
    }
    scan_direct_deps(text, editing)
        .into_iter()
        .map(|id| CompletionItem {
            label: id,
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("declared dependency".to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

/// Line-scan fallback: the `depends_on: [...]` items of the task block
/// named `editing` (mid-keystroke documents keep their edges).
fn scan_direct_deps(text: &str, editing: &str) -> Vec<String> {
    let mut in_task = false;
    let mut deps = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("- id:") {
            in_task = rest.split('#').next().unwrap_or("").trim() == editing;
            continue;
        }
        if in_task && let Some(rest) = t.strip_prefix("depends_on:") {
            let inner = rest.trim().trim_start_matches('[').trim_end_matches(']');
            deps.extend(
                inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').trim_matches('\''))
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    deps
}
