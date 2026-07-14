// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `${{ tasks.` island lane — routed by the reference boundary
//! (spec 04 §the reference boundary · W2): the legal set FOLLOWS the
//! position. The completion dispatch calls in; the shared `task_ids`
//! builder stays in `completion`.

use lsp_types::{CompletionItem, CompletionItemKind};

use super::completion::task_ids;
use super::scope;

/// A `${{ tasks.` island routes by the boundary rule that judges it ·
/// - a `with:` value — the binding IS the edge (DAG-001 bounds it):
///   everything except itself and its downstream closure;
/// - a `recover:` value — the same legal set under DAG-004 (the
///   deadlock closure);
/// - an `on_finally:` surface — the PARENT is the only readable task;
/// - `when:` / `for_each:` / a verb body — `tasks.*` is OUTSIDE the
///   boundary (NIKA-VAR-021): the lane offers NOTHING (silence beats
///   teaching an error · never offer what check refuses);
/// - outside any task (workflow `outputs:`): every task is legal.
pub(super) fn island_task_refs(text: &str, offset: usize) -> Vec<CompletionItem> {
    let Some(editing) = scope::current_task_id(text, offset) else {
        return task_ids(text, None);
    };
    if scope::in_recover_value(text, offset) {
        return task_ids(text, Some(&editing));
    }
    if scope::in_task_block(text, offset, "with") {
        return task_ids(text, Some(&editing));
    }
    if scope::in_task_block(text, offset, "on_finally") {
        return vec![CompletionItem {
            label: editing.clone(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("the parent — the only readable task in a cleanup".to_owned()),
            ..CompletionItem::default()
        }];
    }
    // when: · for_each: · verb fields — outside the boundary.
    Vec::new()
}
