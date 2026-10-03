// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the Live host itself observed of each run leg, from the typed frames
//! it relayed (never a path or an identity the renderer supplies): the
//! execution its first frame bound, the source hash its one start named, the
//! files its `nika:write` tasks reported, the trace and receipt its
//! settlement named. The file and Proof faces reach only what a leg here
//! names; a frame of another execution, or after the settlement, adds
//! nothing. Bounded: [`LEGS_KEPT`] legs, [`FILES_KEPT`] files each.
//!
//! The child run a task called is kept as its settle frame named it, by
//! the rule the fold follows (`RunView::child`): only that frame names one,
//! a new attempt leaves none. The journal of a child is opened only by the
//! relation kept here, [`CHILDREN_KEPT`] at most: a relation past the bound
//! is not kept and opens nothing, never by evicting another.
//!
//! A leg kept from an earlier session (`KeptRun`) carries its execution,
//! source hash and receipt as HOME history recorded them, and no file: a
//! record is evidence to re-verify, never a list of paths to read.

use std::collections::BTreeSet;

use nika_display::run_story::{ChildRun, Event, EventKind, ExecutionId, RunFrame, Settled};
use nika_session::KeptRun;

use super::acquire::Expect;

/// The most legs the host remembers.
pub(crate) const LEGS_KEPT: usize = 4;
/// The most written files one leg remembers; more are counted.
pub(crate) const FILES_KEPT: usize = 64;
/// The most child relations one leg keeps (one per task).
pub(crate) const CHILDREN_KEPT: usize = 64;

/// One leg, as the host relayed it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Leg {
    pub execution: ExecutionId,
    starts: Vec<Option<String>>,
    writing: BTreeSet<String>,
    pub written: Vec<String>,
    pub unlisted: usize,
    pub trace: Option<String>,
    chain_head: Option<String>,
    chain_len: Option<u64>,
    children: Vec<(String, ChildRun)>,
    pub settled: bool,
    pub kept: bool,
}

impl Leg {
    fn new(execution: ExecutionId) -> Self {
        Self {
            execution,
            starts: Vec::new(),
            writing: BTreeSet::new(),
            written: Vec::new(),
            unlisted: 0,
            trace: None,
            chain_head: None,
            chain_len: None,
            children: Vec::new(),
            settled: false,
            kept: false,
        }
    }

    /// The child relation task `task` holds: its settle frame's row, set,
    /// replaced or dropped by the fold's rule.
    fn relate(&mut self, event: &Event) {
        if !matches!(
            event.kind,
            EventKind::TaskStarted | EventKind::TaskCompleted | EventKind::TaskCacheHit
        ) {
            return;
        }
        let Some(task) = event.str_field("task") else {
            return;
        };
        let at = self.children.iter().position(|(kept, _)| kept == task);
        match (ChildRun::of(event), at) {
            (Some(child), Some(at)) => self.children[at].1 = child,
            (Some(child), None) if self.children.len() < CHILDREN_KEPT => {
                self.children.push((task.to_owned(), child));
            }
            (None, Some(at)) => {
                self.children.remove(at);
            }
            _ => {}
        }
    }

    /// The child run task `task` called, as this leg's frames named it.
    pub(crate) fn child(&self, task: &str) -> Option<&ChildRun> {
        (self.children.iter())
            .find(|(kept, _)| kept == task)
            .map(|(_, child)| child)
    }

    fn event(&mut self, event: &Event) {
        self.relate(event);
        let task = event.str_field("task").map(str::to_owned);
        match (event.kind, task) {
            (EventKind::WorkflowStarted, _) => {
                self.starts
                    .push(event.str_field("workflow_sha256").map(str::to_owned));
            }
            (EventKind::TaskStarted, Some(task))
                if event
                    .str_field("note")
                    .is_some_and(|n| n.contains("nika:write")) =>
            {
                self.writing.insert(task);
            }
            (EventKind::TaskCompleted, Some(task)) if self.writing.remove(&task) => {
                // The write's output is the path it wrote, JSON-encoded.
                let path = event
                    .str_field("output")
                    .and_then(|o| serde_json::from_str::<String>(o).ok());
                match path {
                    Some(path) if self.written.contains(&path) => {}
                    Some(path) if self.written.len() < FILES_KEPT => self.written.push(path),
                    _ => self.unlisted += 1,
                }
            }
            _ => {}
        }
    }

    fn settle(&mut self, settled: &Settled) {
        self.trace = settled.trace.as_ref().map(|t| t.display().to_string());
        self.chain_head.clone_from(&settled.chain_head);
        self.chain_len = settled.chain_len;
        self.settled = true;
    }

    /// The source hash the leg's start named, when it saw exactly one start.
    pub(crate) fn workflow_sha256(&self) -> Option<&str> {
        match self.starts.as_slice() {
            [Some(hash)] => Some(hash),
            _ => None,
        }
    }

    /// What a captured journal must name to be this leg's.
    pub(crate) fn proof_expectation(&self) -> Expect {
        Expect {
            execution: self.execution,
            workflow_sha256: self.workflow_sha256().map(str::to_owned),
            chain_head: self.chain_head.clone(),
            chain_len: self.chain_len,
        }
    }

    /// The leg's identity as HOME history keeps it (the session adds its own
    /// workflow, exit and trace).
    pub(crate) fn kept_run(&self) -> KeptRun {
        let mut run = KeptRun::new();
        run.execution = Some(self.execution.uuid.to_string());
        run.workflow_sha256 = self.workflow_sha256().map(str::to_owned);
        run.chain_head.clone_from(&self.chain_head);
        run.chain_len = self.chain_len;
        run
    }
}

/// The legs the host relayed, the newest first.
#[derive(Debug, Default)]
pub(crate) struct Legs {
    relayed: Vec<Leg>,
    fresh: bool,
    current: bool,
}

impl Legs {
    /// A run was asked: its first frame binds a new leg.
    pub(crate) fn asked(&mut self) {
        self.fresh = true;
        self.current = false;
    }

    /// One typed frame the host relayed.
    pub(crate) fn frame(&mut self, frame: &RunFrame) {
        let Some(execution) = frame.execution() else {
            return;
        };
        if std::mem::take(&mut self.fresh) {
            self.relayed.retain(|leg| leg.execution != execution);
            self.relayed.insert(0, Leg::new(execution));
            self.relayed.truncate(LEGS_KEPT);
            self.current = true;
        }
        let Some(leg) = self
            .relayed
            .first_mut()
            .filter(|l| l.execution == execution && !l.settled)
        else {
            return;
        };
        match frame {
            RunFrame::Event(event) => leg.event(event),
            RunFrame::Settled(settled) => leg.settle(settled),
            _ => {}
        }
    }

    /// The leg of `execution`, when the host relayed or kept it.
    pub(crate) fn find(&self, execution: &ExecutionId) -> Option<&Leg> {
        self.relayed.iter().find(|leg| leg.execution == *execution)
    }

    /// The leg the host relayed since the last request, when a frame came.
    pub(crate) fn newest(&self) -> Option<&Leg> {
        self.relayed.first().filter(|leg| self.current && !leg.kept)
    }

    /// A leg HOME history kept from an earlier session: its identity only.
    pub(crate) fn kept(&mut self, run: &KeptRun) -> Option<ExecutionId> {
        let execution = execution_of(run)?;
        let mut leg = Leg::new(execution);
        leg.starts = vec![run.workflow_sha256.clone()];
        leg.trace.clone_from(&run.trace);
        leg.chain_head.clone_from(&run.chain_head);
        leg.chain_len = run.chain_len;
        leg.settled = true;
        leg.kept = true;
        self.relayed.retain(|known| known.execution != execution);
        self.relayed.push(leg);
        self.relayed.truncate(LEGS_KEPT);
        Some(execution)
    }
}

/// The execution a kept run names, read back as the frames carry it
/// (`{"uuid": …}`): a value that is not one names none.
pub(crate) fn execution_of(run: &KeptRun) -> Option<ExecutionId> {
    let uuid = run.execution.as_deref()?;
    serde_json::from_value(serde_json::json!({ "uuid": uuid })).ok()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod legs_tests;
