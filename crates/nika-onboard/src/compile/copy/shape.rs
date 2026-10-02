// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The closed copy: the only shape of candidate this door sends to a room, the assembler's two
//! exact copies and nothing wider.
//!
//! - **Tasks.** Exactly two: a whole-value read of the source path (marked binary in the byte
//!   lowering), then, after its success, a whole-value write of that one binding to the output
//!   path.
//! - **Envelope.** The two path constants, the write's status as the only output, and no model,
//!   input, secret or run declaration.
//! - **No other field.** No condition, loop, retry, recovery, timeout, extraction, return type,
//!   lift or group, and no binding, island or collection beside the one value each task passes
//!   on.
//!
//! An argument is expanded before a room's byte budget sees the write, so a wider shape is never
//! rehearsed here. The paths themselves are the request's own: any source path passes.

use nika_compile::parse;
use nika_compile::surface::assemble::CopyLowering;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawTask, RawWorkflow};
use nika_vocab::{AfterPredicate, OutputDecl};
use serde_json::{Value, json};

/// The read task, as the assembler names it.
const READ: &str = "read_source";
/// The write task, as the assembler names it.
const WRITE: &str = "write_output";

/// Whether `candidate` is exactly the closed copy under `lowering`.
pub(super) fn closed(candidate: &str, lowering: CopyLowering) -> bool {
    let binary = match lowering {
        CopyLowering::Text => false,
        CopyLowering::Bytes => true,
        _ => return false,
    };
    let Ok(workflow) = parse(candidate) else {
        return false;
    };
    workflow.tasks.len() == 2
        && envelope(&workflow)
        && task(&workflow, READ).is_some_and(|read| read_of(read, binary))
        && task(&workflow, WRITE).is_some_and(write_of)
}

/// The workflow beside its tasks: the two path constants, the write's status as its only output,
/// and no model, input, secret or run declaration.
fn envelope(workflow: &RawWorkflow) -> bool {
    let mut consts: Vec<&str> = workflow
        .consts
        .iter()
        .map(|(name, _)| name.value.as_str())
        .collect();
    consts.sort_unstable();
    let output = matches!(
        workflow.outputs.as_slice(),
        [(name, OutputDecl::Untyped(value))]
            if name.value == "write_status" && value.value == "${{ tasks.write_output.status }}"
    );
    workflow.model.is_none()
        && workflow.inputs.is_empty()
        && workflow.secrets.is_empty()
        && workflow.run.is_none()
        && consts == ["output_path", "source_path"]
        && output
}

/// The task named `id`.
fn task<'a>(workflow: &'a RawWorkflow, id: &str) -> Option<&'a RawTask> {
    workflow
        .tasks
        .iter()
        .map(|task| &task.value)
        .find(|task| task.id.value == id)
}

/// The read: a whole-value read of the source path, binary in the byte lowering, after nothing
/// and bound to nothing.
fn read_of(task: &RawTask, binary: bool) -> bool {
    let args = if binary {
        json!({"path": "${{ const.source_path }}", "binary": true})
    } else {
        json!({"path": "${{ const.source_path }}"})
    };
    bare(task) && task.after.is_empty() && task.with.is_empty() && invokes(task, "nika:read", &args)
}

/// The write: after the read's success, its one binding the read's whole value, its content that
/// binding whole, written to the output path.
fn write_of(task: &RawTask) -> bool {
    let args = json!({
        "path": "${{ const.output_path }}",
        "content": "${{ with.content }}",
        "create_dirs": true,
        "overwrite": true,
    });
    let after = matches!(
        task.after.as_slice(),
        [(producer, predicate)]
            if producer.value == READ && predicate.value == AfterPredicate::Success
    );
    let with = matches!(
        task.with.as_slice(),
        [(name, value)]
            if name.value == "content" && value.value == json!("${{ tasks.read_source.output }}")
    );
    bare(task) && after && with && invokes(task, "nika:write", &args)
}

/// Whether `task` evaluates nothing beside its one call: no condition, loop, retry, recovery,
/// timeout, extraction, return type, lift or group.
fn bare(task: &RawTask) -> bool {
    task.when.is_none()
        && task.for_each.is_none()
        && task.max_parallel.is_none()
        && task.max_items.is_none()
        && task.fail_fast.is_none()
        && task.retry.is_none()
        && task.on_error.is_none()
        && task.timeout.is_none()
        && task.extract.is_empty()
        && task.returns.is_none()
        && task.lift.is_empty()
        && task.group.is_none()
}

/// Whether `task` calls the builtin `tool` with exactly `args`.
fn invokes(task: &RawTask, tool: &str, args: &Value) -> bool {
    let RawAction::Invoke(invoke) = &task.action else {
        return false;
    };
    matches!(&invoke.target, RawInvokeTarget::Tool(name) if name.value == tool)
        && invoke
            .args
            .as_ref()
            .is_some_and(|given| given.value == *args)
}
