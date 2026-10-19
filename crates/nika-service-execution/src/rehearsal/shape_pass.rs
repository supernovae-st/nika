// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shape pass, tried after a replay trial's run (live gauntlet r04 B1, 2026-10-17: the trial
//! ran neither the model step `select_top` nor the steps after it, and the first run failed in
//! `render`, which indexed a date that is text). A trial runs no model step, so the steps after
//! one never ran either. When a model step declares the shape of its answer (`schema:`,
//! `returns:`), a second pass of the same bytes stands an example of that shape in for its answer
//! (the in-process mock's `stand-in`, [`nika_providers::STAND_IN_MODEL`]), over the room's files
//! as the first pass left them, writing nothing. A step that reads only such stand-ins, among the
//! steps the trial did not run, and fails on the answer's shape (a jq type error, a binding that
//! reads beneath a value) fails the trial with its facts: the program meets that shape on its
//! first real run. A failure of content (an assertion, an empty filter, text read as a number) is
//! the example's, never counted, and nothing this pass computes is shown as the trial's result.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use nika_kernel::fs::{FileMetadata, FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};
use nika_providers::{ExecutionAccessPlan, STAND_IN_MODEL};
use nika_runtime::{RunOutcome, TaskErrorRecord, TaskStatus, TerminalCause, ValueBound};
use nika_schema::raw::{RawAction, RawWorkflow};

use super::isolated_jq::IsolatedJq;
use super::replay::{self, Captures, ReplayScreen};
use super::room::{DeniedTally, Lent};
use crate::ServiceExecutionDriver;

#[cfg(test)]
mod tests;

/// The code a step that fails on the shape a model step declares fails its task with.
pub(super) const SHAPE: &str = "NIKA-REHEARSAL-SHAPE";

/// What a jq error says when the program met the wrong shape, not the wrong content.
const SHAPES: &[&str] = &[
    "cannot index",
    "Cannot index",
    "cannot iterate",
    "Cannot iterate",
    "cannot use",
];

/// What the pass is lent: the run's plan, its jq evaluator, the captures it replays and the
/// bound on the values the run builds.
pub(super) struct Pass<'a> {
    pub(super) plan: ExecutionAccessPlan,
    pub(super) jq: &'a IsolatedJq,
    pub(super) captures: &'a Arc<Captures>,
    pub(super) limit: Option<u64>,
}

/// After a replay trial that settled ok: the steps behind a model step that declares its
/// answer's shape run on an example of it, over the room as the first pass left it, writing
/// nothing; the first that fails on that shape fails its task, with [`SHAPE`], and the trial.
pub(super) async fn try_shapes<F>(
    driver: &ServiceExecutionDriver,
    fs: &Arc<F>,
    pass: Pass<'_>,
    outcome: &mut RunOutcome,
) where
    F: FsReadDyn + FsWriteDyn + FsListDyn + FsMetaDyn + Send + Sync + 'static,
{
    if !outcome.ok {
        return;
    }
    let workflow = &driver.workflow;
    let screen = replay::screen(workflow, pass.captures);
    let stood = stood_in(workflow, &screen);
    if stood.is_empty() {
        return;
    }
    let behind: Vec<&str> = (workflow.tasks.iter())
        .map(|task| task.value.id.value.as_str())
        .filter(|id| !screen.is_unexercised(id) && !stood_for(&screen, id, &stood).is_empty())
        .collect();
    if behind.is_empty() {
        return;
    }
    let lent = Lent {
        tally: Arc::new(DeniedTally::default()),
        jq: Some(Arc::new(pass.jq.fresh())),
        captures: Some(Arc::clone(pass.captures)),
        values: pass.limit.map(|limit| Arc::new(ValueBound::new(limit))),
    };
    let room = Arc::new(ReadOnly(Arc::clone(fs)));
    let Ok(shaped) = driver.run_in(room, pass.plan, lent, STAND_IN_MODEL).await else {
        return;
    };
    for id in behind {
        let failed = (shaped.records.get(id))
            .filter(|record| record.status == TaskStatus::Failure)
            .and_then(|record| record.error.as_ref())
            .filter(|error| structural(error));
        let (Some(error), Some(record)) = (failed, outcome.records.get_mut(id)) else {
            continue;
        };
        let message = said(id, &stood_for(&screen, id, &stood), &error.message);
        record.status = TaskStatus::Failure;
        record.cause = TerminalCause::VerbError;
        record.error = Some(TaskErrorRecord::new(SHAPE, message, false));
        outcome.ok = false;
        return;
    }
}

/// The model steps the trial did not run that declare their answer's shape, in the workflow's
/// order: a stand-in of that shape can answer for them.
fn stood_in(workflow: &RawWorkflow, screen: &ReplayScreen) -> Vec<String> {
    (workflow.tasks.iter())
        .filter(|task| {
            let RawAction::Infer(infer) = &task.value.action else {
                return false;
            };
            (infer.schema.is_some() || task.value.returns.is_some())
                && screen.is_unexercised(&task.value.id.value)
        })
        .map(|task| task.value.id.value.clone())
        .collect()
}

/// The stand-ins `task` reads, through the steps the trial ran; empty when it reads none, or when
/// it reads any other step the trial did not run (that one gives nothing to stand in for).
fn stood_for(screen: &ReplayScreen, task: &str, stood: &[String]) -> Vec<String> {
    let (mut found, mut seen, mut open) = (Vec::new(), Vec::new(), vec![task]);
    while let Some(at) = open.pop() {
        for read in screen.upstream.get(at).into_iter().flatten() {
            if !screen.is_unexercised(read) {
                if !seen.contains(&read.as_str()) {
                    seen.push(read.as_str());
                    open.push(read.as_str());
                }
            } else if stood.contains(read) {
                if !found.contains(read) {
                    found.push(read.clone());
                }
            } else {
                return Vec::new();
            }
        }
    }
    found
}

/// Whether a failure is the program's meeting the wrong shape: a jq type error, or a binding
/// that reads beneath a value the shape does not hold.
fn structural(error: &TaskErrorRecord) -> bool {
    let jq = error.message.contains("jq runtime error")
        && SHAPES.iter().any(|shape| error.message.contains(shape));
    jq || error.code == "NIKA-VAR-001" || error.code == "NIKA-VAR-006"
}

/// What the author reads of a step that fails on the shape a model step declares.
fn said(task: &str, stood: &[String], error: &str) -> String {
    let error: String = error.chars().take(300).collect();
    let stood = stood.join(", ");
    format!(
        "{task} fails on an answer of the shape {stood} declares: « {error} ». An example of that \
         shape stood in for the model's answer, which did not run; repair {task} for that shape"
    )
}

/// The room as the shape pass reads it: every read the room answers, every write refused, so
/// the pass leaves the room as the first pass left it.
struct ReadOnly<F>(Arc<F>);

/// The refusal every write of the shape pass meets.
fn refused(path: &Path) -> FsError {
    FsError::PermissionDenied {
        path: path.display().to_string(),
    }
}

impl<F: FsReadDyn + Send + Sync> FsReadDyn for ReadOnly<F> {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        self.0.read(path).await
    }

    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        self.0.read_to_string(path).await
    }

    async fn exists(&self, path: &Path) -> bool {
        self.0.exists(path).await
    }

    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        self.0.canonicalize(path).await
    }

    async fn read_pinned(&self, path: &Path) -> Result<Bytes, FsError> {
        self.0.read_pinned(path).await
    }
}

impl<F: Send + Sync> FsWriteDyn for ReadOnly<F> {
    async fn write(&self, path: &Path, _contents: &[u8]) -> Result<(), FsError> {
        Err(refused(path))
    }

    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        Err(refused(path))
    }

    async fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        Err(refused(path))
    }
}

impl<F: FsMetaDyn + Send + Sync> FsMetaDyn for ReadOnly<F> {
    async fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError> {
        self.0.metadata(path).await
    }
}

impl<F: FsListDyn + Send + Sync> FsListDyn for ReadOnly<F> {
    async fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        self.0.list_dir(path).await
    }

    async fn glob(&self, root: &Path, pattern: &str) -> Result<Vec<PathBuf>, FsError> {
        self.0.glob(root, pattern).await
    }
}
