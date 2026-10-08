// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run door a host lends its Session (ADR-133: the Session requests a run as data and the
//! door executes it). The native door runs this binary's machine lane as a child, pipes only,
//! and keeps the live child across its fresh cost review, so the review's answer reaches that
//! very child once. A door that cannot start runs says so and observes nothing: no exit, no
//! trace and no assurance are invented for a run that did not happen.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nika_cli_host::lane::{self, ChildSlot, PendingRun, RunProgress};

/// What a door's run port receives and tells: the Session's run request and the run's story.
pub use nika_cli_host::lane::RunSink;
pub use nika_session::RunRequest;

/// What one step of a requested run came to.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunStep {
    /// The run ended and was observed: its exit and the trace it left.
    Observed {
        /// The run door's exit code.
        exit: u8,
        /// The trace the settlement named, when it named one.
        trace: Option<PathBuf>,
    },
    /// The run's child waits at its fresh cost review; this door holds it.
    Review {
        /// The review's first screen.
        question: String,
        /// The review's evidence, shown on demand.
        details: String,
    },
    /// The decline was honoured: the held child is gone and nothing was sent.
    Declined,
    /// This door did not start the run; nothing ran.
    NotStarted {
        /// Why, in words.
        why: String,
    },
}

/// How a host executes what its Session requested.
pub trait RunDoor: Send {
    /// Run the workflow once, as the Session requested it.
    fn run(&mut self, root: &Path, run: &RunRequest, sink: &dyn RunSink) -> RunStep;

    /// Resume a paused run with the human's answer (`task=value`).
    fn resume(
        &mut self,
        root: &Path,
        workflow: &Path,
        trace: &Path,
        answer: &str,
        sink: &dyn RunSink,
    ) -> RunStep;

    /// The human's answer to the cost review this door holds: one approval continues that very
    /// child once; a decline drops it and sends nothing.
    fn answer_review(&mut self, approve: bool, sink: &dyn RunSink) -> RunStep;
}

/// A door that starts no run, and says why.
#[derive(Debug)]
pub struct NoRunDoor {
    why: String,
}

impl NoRunDoor {
    /// A door that answers every run with `why`.
    #[must_use]
    pub fn new(why: impl Into<String>) -> Self {
        Self { why: why.into() }
    }

    fn not_started(&self) -> RunStep {
        RunStep::NotStarted {
            why: self.why.clone(),
        }
    }
}

impl RunDoor for NoRunDoor {
    fn run(&mut self, _root: &Path, _run: &RunRequest, _sink: &dyn RunSink) -> RunStep {
        self.not_started()
    }

    fn resume(
        &mut self,
        _root: &Path,
        _workflow: &Path,
        _trace: &Path,
        _answer: &str,
        _sink: &dyn RunSink,
    ) -> RunStep {
        self.not_started()
    }

    fn answer_review(&mut self, _approve: bool, _sink: &dyn RunSink) -> RunStep {
        self.not_started()
    }
}

/// This binary's machine lane as a child (`nika run --json`), pipes only: the native door's
/// runs, the same path the terminal renderer takes.
pub struct LaneRunDoor {
    exe: PathBuf,
    slot: ChildSlot,
    held: Option<Box<PendingRun>>,
}

impl std::fmt::Debug for LaneRunDoor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaneRunDoor")
            .field("exe", &self.exe)
            .field("held", &self.held.is_some())
            .finish_non_exhaustive()
    }
}

impl LaneRunDoor {
    /// Runs through `exe` (this binary).
    #[must_use]
    pub fn new(exe: PathBuf) -> Self {
        Self {
            exe,
            slot: Arc::new(Mutex::new(None)),
            held: None,
        }
    }

    /// The running child's pid, while one runs: the door that leaves can end it.
    #[must_use]
    pub fn slot(&self) -> ChildSlot {
        Arc::clone(&self.slot)
    }
}

impl RunDoor for LaneRunDoor {
    fn run(&mut self, root: &Path, run: &RunRequest, sink: &dyn RunSink) -> RunStep {
        // A new run replaces a review still held: dropping it ends its child, nothing is sent.
        self.held = None;
        let args = lane::run_args_with_access(
            root,
            &run.workflow,
            run.max_cost_usd,
            &run.vars,
            run.access_pin.as_deref(),
        );
        match lane::drive_reviewed_child_observed(&self.exe, &args, root, sink, &self.slot) {
            RunProgress::Complete((exit, trace, _)) => RunStep::Observed { exit, trace },
            RunProgress::Review(pending) => {
                let step = RunStep::Review {
                    question: pending.question(),
                    details: pending.details(),
                };
                self.held = Some(pending);
                step
            }
            _ => RunStep::NotStarted {
                why: "the run lane answered a progress this host does not know".to_owned(),
            },
        }
    }

    fn resume(
        &mut self,
        root: &Path,
        workflow: &Path,
        trace: &Path,
        answer: &str,
        sink: &dyn RunSink,
    ) -> RunStep {
        let args = lane::resume_args(root, workflow, trace, answer);
        let (exit, trace, _) = lane::drive_child_observed(&self.exe, &args, root, sink, &self.slot);
        RunStep::Observed { exit, trace }
    }

    fn answer_review(&mut self, approve: bool, sink: &dyn RunSink) -> RunStep {
        let Some(pending) = self.held.take() else {
            return RunStep::NotStarted {
                why: "no run waits at a cost review on this door".to_owned(),
            };
        };
        if !approve {
            drop(pending);
            return RunStep::Declined;
        }
        let (exit, trace, _) = (*pending).answer_observed(true, sink);
        RunStep::Observed { exit, trace }
    }
}
