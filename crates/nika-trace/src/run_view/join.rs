// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Whether one execution ran a component of a saved candidate, read from that execution's own
//! frames: its start must name the hash of the saved bytes, and each of the component's tasks
//! is settled by the frames of that execution alone.
//!
//! What never stands in for it: the run's exit or terminal word (a run that succeeds may have
//! skipped the component), the last start a journal holds ([`RunFacts::workflow_sha256`]: a
//! journal may hold several executions), and a task name read across executions (a child run's
//! task may share it). What the frames cannot attribute stays unknown. A fold, never a verdict:
//! the chain stays the verifier's to judge.

use std::path::Path;

use nika_types::id::ExecutionId;
use serde_json::Value;

use super::{RunFacts, TaskState, fields, text};

/// What one execution's own frames say of a saved candidate's component.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComponentRun {
    /// The execution started exactly the saved bytes: each component task as that execution's
    /// frames settle it, in the order asked.
    Joined(Vec<ComponentTask>),
    /// The execution started other bytes, whose hash its start names: it proves nothing of this
    /// candidate.
    OtherSource(String),
    /// The frames cannot tie the component to the execution (why), never a guess.
    Unknown(String),
}

impl ComponentRun {
    /// Whether the execution ran the component: it started the saved bytes and completed every
    /// component task (never one recovered, skipped or replayed from a cache).
    #[must_use]
    pub fn ran(&self) -> bool {
        matches!(self, Self::Joined(tasks)
            if !tasks.is_empty() && tasks.iter().all(ComponentTask::completed))
    }
}

/// One component task, as the frames of the joined execution settle it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComponentTask {
    id: String,
    /// `None`: no frame of the execution names it.
    state: Option<TaskState>,
}

impl ComponentTask {
    /// The task, as the component names it.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// How the execution's frames settle it: `completed` · `failed` · `recovered` · `skipped` ·
    /// `cancelled` · `cache_hit` · `running` · `scheduled`, or `absent` when no frame of the
    /// execution names it.
    #[must_use]
    pub fn settled(&self) -> &'static str {
        match self.state {
            None => "absent",
            Some(TaskState::Scheduled) => "scheduled",
            Some(TaskState::Running) => "running",
            Some(TaskState::Ok) => "completed",
            Some(TaskState::Failed) => "failed",
            Some(TaskState::Recovered) => "recovered",
            Some(TaskState::Skipped) => "skipped",
            Some(TaskState::Cancelled) => "cancelled",
            Some(TaskState::CacheHit) => "cache_hit",
        }
    }

    /// Whether the execution completed it.
    #[must_use]
    pub fn completed(&self) -> bool {
        self.state == Some(TaskState::Ok)
    }
}

/// The join behind [`super::KeptRun::component_run`]: the frames of `execution` in `journal`,
/// its one start against `source_sha256`, then each of `tasks` from those frames alone.
pub(super) fn component_run(
    journal: &str,
    execution: Option<&str>,
    source_sha256: &str,
    tasks: &[&str],
) -> ComponentRun {
    let Some(execution) = execution else {
        return ComponentRun::Unknown("the kept run names no execution".to_owned());
    };
    if tasks.is_empty() {
        return ComponentRun::Unknown("no component task was named".to_owned());
    }
    let mut own = String::new();
    for line in journal.lines() {
        let Ok(frame) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let id = (frame.get("execution").cloned()).map(serde_json::from_value::<ExecutionId>);
        match id {
            Some(Ok(id)) if id.uuid.to_string() == execution => {
                own.push_str(line);
                own.push('\n');
            }
            Some(Ok(_)) => {}
            // A frame no execution owns is no execution's evidence: naming a component task, it
            // leaves that task unattributed.
            _ => {
                if let Some(task) = text(&fields(&frame), "task")
                    && tasks.contains(&task.as_str())
                {
                    return ComponentRun::Unknown(format!(
                        "a frame with no execution names the task `{task}`"
                    ));
                }
            }
        }
    }
    let Some(facts) = RunFacts::of(Path::new(""), &own) else {
        return ComponentRun::Unknown(format!(
            "no frame of execution `{execution}` is in this journal"
        ));
    };
    match facts.starts() {
        [Some(hash)] if hash == source_sha256 => ComponentRun::Joined(
            tasks
                .iter()
                .map(|id| ComponentTask {
                    id: (*id).to_owned(),
                    state: (facts.tasks.iter())
                        .find(|task| task.id == *id)
                        .map(|task| task.state),
                })
                .collect(),
        ),
        [Some(hash)] => ComponentRun::OtherSource(hash.clone()),
        [None] => ComponentRun::Unknown(format!(
            "the start of execution `{execution}` names no source hash"
        )),
        [] => ComponentRun::Unknown(format!(
            "execution `{execution}` has no start in this journal"
        )),
        starts => ComponentRun::Unknown(format!(
            "execution `{execution}` started {} times in this journal",
            starts.len()
        )),
    }
}
