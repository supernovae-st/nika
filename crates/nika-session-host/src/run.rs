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
    /// The run was admitted but its end could not be observed: its effects are unknown.
    Unobserved {
        /// What was admitted and why its end is unknown, in words.
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

/// A future a [`Jobs`] port answers with.
pub type JobFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// What an admission came to.
#[derive(Debug)]
#[non_exhaustive]
pub enum Admitted {
    /// The job's identity.
    Job(String),
    /// The resident holds a fresh cost review the run waits at: nothing is admitted before the
    /// human's decision on it.
    Review {
        /// The review's identity, held by this door.
        review: String,
        /// The review's question.
        question: String,
        /// The review's document, shown on demand.
        details: String,
    },
}

/// What a resident's own job admission lends a Session's run door: the by-name admission its
/// job route takes (or the fresh cost review it holds first), the decision on that review, and
/// the wait for the job's end. Nothing else of the resident.
pub trait Jobs: Send + Sync {
    /// Admit the workflow `name` (relative to the project) by name, with the run's `name=value`
    /// pairs and access pin: the job, the review it waits at, or the resident's refusal in its
    /// own words.
    fn admit<'a>(
        &'a self,
        name: &'a str,
        vars: &'a [String],
        access: Option<&'a str>,
    ) -> JobFuture<'a, Result<Admitted, String>>;

    /// The human's decision on `review`: approved, the job its approval admitted; declined,
    /// none, and nothing is sent.
    fn decide<'a>(
        &'a self,
        review: &'a str,
        approve: bool,
    ) -> JobFuture<'a, Result<Option<String>, String>>;

    /// The job `id` once it settled or paused: the exit `nika run` gives for that end, and the
    /// journal the resident wrote, when it wrote one.
    fn settled<'a>(&'a self, id: &'a str) -> JobFuture<'a, Result<(u8, Option<PathBuf>), String>>;
}

/// A Session's runs through a resident's job admission, waited for on the resident's runtime
/// from the Session's own worker thread. A cost review it holds is declined when a new run
/// replaces it or when the door is dropped: a pending review never outlives its Session.
pub struct JobDoor {
    handle: tokio::runtime::Handle,
    jobs: Arc<dyn Jobs>,
    held: Option<String>,
}

impl std::fmt::Debug for JobDoor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobDoor")
            .field("held", &self.held)
            .finish_non_exhaustive()
    }
}

impl JobDoor {
    /// Runs admitted through `jobs`, awaited on `handle`.
    #[must_use]
    pub fn new(handle: tokio::runtime::Handle, jobs: Arc<dyn Jobs>) -> Self {
        Self {
            handle,
            jobs,
            held: None,
        }
    }

    /// Decline the review this door holds, if any: nothing is sent.
    fn release(&mut self) {
        if let Some(review) = self.held.take() {
            let _declined = self.handle.block_on(self.jobs.decide(&review, false));
        }
    }

    /// The admitted job, waited for to its end.
    fn observe(&self, job: &str, sink: &dyn RunSink) -> RunStep {
        sink.said(format!("run admitted as job {job}"));
        match self.handle.block_on(self.jobs.settled(job)) {
            Ok((exit, trace)) => {
                sink.said(format!("job {job} ended with exit {exit}"));
                RunStep::Observed { exit, trace }
            }
            Err(why) => RunStep::Unobserved {
                why: format!("job {job} was admitted; its end was not observed: {why}"),
            },
        }
    }
}

impl Drop for JobDoor {
    fn drop(&mut self) {
        self.release();
    }
}

impl RunDoor for JobDoor {
    fn run(&mut self, _root: &Path, run: &RunRequest, sink: &dyn RunSink) -> RunStep {
        self.release();
        let name = run.workflow.to_string_lossy().into_owned();
        let access = run.access_pin.as_deref();
        match self
            .handle
            .block_on(self.jobs.admit(&name, &run.vars, access))
        {
            Ok(Admitted::Job(job)) => self.observe(&job, sink),
            Ok(Admitted::Review {
                review,
                question,
                details,
            }) => {
                self.held = Some(review);
                RunStep::Review { question, details }
            }
            Err(why) => RunStep::NotStarted {
                why: format!("{why} · nothing ran"),
            },
        }
    }

    fn resume(
        &mut self,
        _root: &Path,
        _workflow: &Path,
        _trace: &Path,
        _answer: &str,
        _sink: &dyn RunSink,
    ) -> RunStep {
        RunStep::NotStarted {
            why: "this door does not resume a paused run · nothing was answered".to_owned(),
        }
    }

    fn answer_review(&mut self, approve: bool, sink: &dyn RunSink) -> RunStep {
        let Some(review) = self.held.take() else {
            return RunStep::NotStarted {
                why: "no run waits at a cost review on this door · nothing was sent".to_owned(),
            };
        };
        match self.handle.block_on(self.jobs.decide(&review, approve)) {
            Ok(Some(job)) => self.observe(&job, sink),
            Ok(None) => RunStep::Declined,
            Err(why) => RunStep::NotStarted {
                why: format!("{why} · nothing ran"),
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
