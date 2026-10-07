// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Child workflow closure digests from the admitted in-memory snapshot.

use std::collections::BTreeMap;

use nika_event::source_id::sha256_hex;
use nika_execution::ExecutionSnapshot;
use nika_runtime::child::MAX_RUN_DEPTH;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_schema::{FileId, ParseMode};

use super::resolve_logical;

pub(super) fn admitted_closure_digests(
    workflow: &RawWorkflow,
    snapshot: &ExecutionSnapshot,
    parent_logical: &str,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for target in workflow_targets_of(workflow) {
        let Ok(resolved) = resolve_logical(parent_logical, &target) else {
            continue;
        };
        let mut stack = Vec::new();
        if let Some(digest) = closure_digest(snapshot, &resolved, &mut stack, 1) {
            out.insert(target, digest);
        }
    }
    out
}

fn workflow_targets_of(workflow: &RawWorkflow) -> Vec<String> {
    let target_of = |action: &RawAction| match action {
        RawAction::Invoke(action) => match &action.target {
            RawInvokeTarget::Workflow(workflow) => Some(workflow.value.clone()),
            RawInvokeTarget::Tool(_) => None,
        },
        _ => None,
    };
    workflow
        .tasks
        .iter()
        .filter_map(|task| target_of(&task.value.action))
        .collect()
}

fn closure_digest(
    snapshot: &ExecutionSnapshot,
    logical: &str,
    stack: &mut Vec<String>,
    depth: u32,
) -> Option<String> {
    if depth > MAX_RUN_DEPTH || stack.iter().any(|identity| identity == logical) {
        return None;
    }
    let source = snapshot.text(logical)?;
    let workflow = nika_schema::parse(source, FileId::new(0), ParseMode::Strict).ok()?;
    stack.push(logical.to_owned());
    let mut children = BTreeMap::new();
    for target in workflow_targets_of(&workflow) {
        if target.starts_with("registry:") {
            stack.pop();
            return None;
        }
        let Ok(resolved) = resolve_logical(logical, &target) else {
            stack.pop();
            return None;
        };
        let Some(digest) = closure_digest(snapshot, &resolved, stack, depth + 1) else {
            stack.pop();
            return None;
        };
        children.insert(target, digest);
    }
    stack.pop();
    let mut fold = String::from("nika-child-closure:v1\0");
    fold.push_str(&sha256_hex(source.as_bytes()));
    for (target, digest) in &children {
        fold.push('\0');
        fold.push_str(target);
        fold.push('\0');
        fold.push_str(digest);
    }
    Some(sha256_hex(fold.as_bytes()))
}
