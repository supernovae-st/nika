// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The work a session holds, typed once for every host (ADR-133 · the portable session): what
//! the next line answers, with the identity an answer names, and one snapshot of the request,
//! the candidate under review, the saved workflow and the last observed run. The session builds
//! it from its own state; a terminal, the plain loop or a remote door renders it and decides
//! nothing from it. It serializes so a remote door can carry the same facts, and it grants
//! nothing: a consent, an answer or a run still goes through the session's own doors.
//!
//! The contract is versioned by [`CONTRACT`]; additive fields keep the version. What the
//! session does not hold yet (a revision lineage, requirement states, typed verdicts, a run's
//! effects) is absent rather than inferred.

use std::path::PathBuf;

use serde::Serialize;

use crate::change::{ProjectChange, ProjectChangeSet, Witness, WorkflowAudit};
use crate::outcome::{GateId, ProposalId, QuestionId};
use crate::world::World;

/// The version a host checks before reading a [`Work`].
pub const CONTRACT: &str = "nika/session-work@0";

/// A question's identity on the wire: its witness (the question, its request revision and the
/// intelligence that reads the answer), never the session that asked it.
fn question_witness<S: serde::Serializer>(id: &QuestionId, out: S) -> Result<S::Ok, S::Error> {
    out.serialize_str(id.as_str())
}

/// What the next line answers, by the session's one precedence: the one-time cost decision,
/// the choice of intelligence, a proposal's consent, a run's gate, then the values a question,
/// a run input or an activation asks. Hosts route a line through the session with it; none
/// keeps a routing bit of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Waiting {
    /// Nothing waits: the next line is a new turn.
    Free,
    /// The one-time decision on an inference whose cost is unknown; nothing is sent before it.
    CostChoice,
    /// The first screen: the intelligence the human prepares with.
    IntelligenceChoice,
    /// A proposal waits for its Save consent; nothing is written before it, and Save is never
    /// a Run.
    Consent {
        /// The identity a consent names.
        proposal: ProposalId,
    },
    /// A paused run waits for the human's answer at its gate.
    Gate {
        /// The identity an answer names.
        gate: GateId,
    },
    /// The compiler asks one authoring value it cannot invent.
    Question {
        /// The semantic hole the answer fills (`model` · `const.x` …).
        key: String,
        /// The identity an answer names: this question as this session asked it, at this
        /// revision of the request. It serializes as its witness; the session that asked it
        /// is held in memory only, so an answer carried across a restart never matches.
        #[serde(serialize_with = "question_witness")]
        id: QuestionId,
    },
    /// A requested run waits for the value of one declared input.
    Input {
        /// The input's name.
        name: String,
    },
    /// An activation waits for one project value.
    Activation {
        /// The value's key (`project.timezone` · `project.missed` · `project.ceiling`).
        key: String,
    },
}

/// One snapshot of the work, for every host.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Work {
    /// [`CONTRACT`].
    pub contract: &'static str,
    /// The proven project root.
    pub root: PathBuf,
    /// The request as the session keeps it.
    pub request: Request,
    /// What the next line answers.
    pub waiting: Waiting,
    /// The candidate under review, when one is.
    pub candidate: Option<Candidate>,
    /// The workflow saved by the last consent of this session, when one was.
    pub saved: Option<Saved>,
    /// The last observed run, from this session or kept from an earlier one ([`Run::current`]).
    pub run: Option<Run>,
    /// The automation rail, each field at its own stage.
    pub rail: Rail,
}

/// The request as the session keeps it: decisions, not chat.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Request {
    /// The goal as first stated, with the corrections the session joined to it.
    pub goal: Option<String>,
    /// Decisions the human made in words.
    pub decisions: Vec<String>,
    /// Questions still open.
    pub unresolved: Vec<String>,
}

impl Request {
    /// The request from the session's durable intent.
    #[must_use]
    pub fn new(goal: Option<String>, decisions: Vec<String>, unresolved: Vec<String>) -> Self {
        Self {
            goal,
            decisions,
            unresolved,
        }
    }
}

/// The candidate under review: the exact changes a yes lands and their audits.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Candidate {
    /// The identity a consent names.
    pub proposal: ProposalId,
    /// Set aside while a revision's question waits: not consentable meanwhile.
    pub aside: bool,
    /// A rehearsal proof is bound to this proposal.
    pub rehearsed: bool,
    /// The set asks for one run once it is saved and checked clean.
    pub run_after_save: bool,
    /// Every file the yes lands, in the set's order.
    pub files: Vec<CandidateFile>,
}

impl Candidate {
    /// The candidate a set describes, under the identity its consent names.
    #[must_use]
    pub fn of(proposal: ProposalId, set: &ProjectChangeSet, aside: bool, rehearsed: bool) -> Self {
        let files = set
            .changes
            .iter()
            .map(|change| CandidateFile::of(change, &set.audits))
            .collect();
        Self {
            proposal,
            aside,
            rehearsed,
            run_after_save: set.run.is_some(),
            files,
        }
    }

    /// Where each audited workflow of the candidate reaches, in the set's order.
    pub fn worlds(&self) -> impl Iterator<Item = &World> {
        self.files
            .iter()
            .filter_map(|f| f.audit.as_ref())
            .map(|a| &a.world)
    }
}

/// How a file of the candidate lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Landing {
    /// The file does not exist yet.
    Create,
    /// The file is replaced whole, over the witnessed bytes the preview was built on.
    Update,
}

/// One file the yes lands.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct CandidateFile {
    /// Relative to the root.
    pub path: PathBuf,
    /// How it lands.
    pub landing: Landing,
    /// Whether it is a workflow (checked after it lands).
    pub workflow: bool,
    /// The witness of the exact bytes the yes lands.
    pub bytes: Witness,
    /// The witness of the bytes an update replaces.
    pub replaces: Option<Witness>,
    /// The audit of a workflow's exact bytes, when the set carries one.
    pub audit: Option<Audit>,
}

impl CandidateFile {
    fn of(change: &ProjectChange, audits: &[WorkflowAudit]) -> Self {
        let path = change.path();
        let audit = audits.iter().find(|a| a.path == path).map(Audit::of);
        Self {
            landing: if change.witness().is_some() {
                Landing::Update
            } else {
                Landing::Create
            },
            workflow: change.is_workflow(),
            bytes: Witness::of(change.content().as_bytes()),
            replaces: change.witness().cloned(),
            audit,
            path,
        }
    }
}

/// The audit of a workflow's exact bytes, as the preview showed it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Audit {
    /// The verdict the check facade gave these bytes.
    pub clean: bool,
    /// `code · message`, the first eight.
    pub findings: Vec<String>,
    /// Where the bytes reach.
    pub world: World,
}

impl Audit {
    /// The audit a set carries for one workflow.
    #[must_use]
    pub fn of(audit: &WorkflowAudit) -> Self {
        Self {
            clean: audit.clean,
            findings: audit.findings.clone(),
            world: audit.world.clone(),
        }
    }
}

/// The workflow saved by the last consent of this session.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Saved {
    /// Relative to the root.
    pub workflow: PathBuf,
    /// The check at that consent: clean or findings; `None` when no check ran.
    pub check_clean: Option<bool>,
}

impl Saved {
    /// The saved workflow and the check its consent ran.
    #[must_use]
    pub fn new(workflow: PathBuf, check_clean: Option<bool>) -> Self {
        Self {
            workflow,
            check_clean,
        }
    }
}

/// How an observed run ended, from the exit the host observed (the run door's own codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "end", content = "exit", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RunEnd {
    /// Exit 0.
    Succeeded,
    /// Exit 1: the workflow failed.
    Failed,
    /// Exit 2: refused before running (findings).
    RefusedFindings,
    /// Exit 3: refused by the environment.
    RefusedEnvironment,
    /// Exit 4: paused for a human answer.
    Paused,
    /// Exit 130: interrupted before it finished.
    Interrupted,
    /// Any other exit.
    Unknown(u8),
}

impl RunEnd {
    /// The end an exit code names.
    #[must_use]
    pub fn of(exit: u8) -> Self {
        match exit {
            0 => Self::Succeeded,
            1 => Self::Failed,
            2 => Self::RefusedFindings,
            3 => Self::RefusedEnvironment,
            4 => Self::Paused,
            130 => Self::Interrupted,
            other => Self::Unknown(other),
        }
    }

    /// The meaning a host shows beside the exit.
    #[must_use]
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "the workflow failed",
            Self::RefusedFindings => "refused before running (findings)",
            Self::RefusedEnvironment => "refused by the environment",
            Self::Paused => {
                "paused for a human answer — `nika run <file> --resume <trace> --answer <task>=<value>` continues it"
            }
            Self::Interrupted => {
                "interrupted before it finished (Ctrl+C) — the trace shows what ran"
            }
            Self::Unknown(_) => "ended with an unknown code",
        }
    }
}

/// The last observed run, with the identities its observation carried and nothing re-derived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Run {
    /// The run of the workflow this session saved last, observed by this session. `false` for
    /// a run kept from an earlier session or observed before a later Save: evidence, never the
    /// result of the bytes saved now. `workflow_sha256` names the bytes it ran.
    pub current: bool,
    /// The workflow the run was asked of, relative to the root.
    pub workflow: Option<String>,
    /// How it ended, when the exit was observed.
    pub end: Option<RunEnd>,
    /// The trace its settlement named.
    pub trace: Option<String>,
    /// The execution its frames and settlement carried.
    pub execution: Option<String>,
    /// The source hash its start named: ties the run to the exact bytes it ran.
    pub workflow_sha256: Option<String>,
    /// The journal head and length its receipt named.
    pub chain_head: Option<String>,
    /// The journal length its receipt named.
    pub chain_len: Option<u64>,
}

impl Run {
    /// A run from its observed parts.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        current: bool,
        workflow: Option<String>,
        exit: Option<u8>,
        trace: Option<String>,
        execution: Option<String>,
        workflow_sha256: Option<String>,
        chain_head: Option<String>,
        chain_len: Option<u64>,
    ) -> Self {
        Self {
            current,
            workflow,
            end: exit.map(RunEnd::of),
            trace,
            execution,
            workflow_sha256,
            chain_head,
            chain_len,
        }
    }
}

/// One rail field's stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Stage {
    /// Not reached.
    Pending,
    /// Under way: a question or a value waits.
    Working,
    /// Done and observed.
    Done,
    /// Declared in the file, not proven by a firer.
    Declared,
    /// Paused: a gate waits, or the declaration is suspended.
    Paused,
    /// Failed.
    Failed,
    /// Needs attention: findings, a refusal.
    Attention,
    /// A stage this contract version does not name yet: shown as unknown, never guessed.
    Other,
}

impl From<nika_onboard::lifecycle::Stage> for Stage {
    fn from(stage: nika_onboard::lifecycle::Stage) -> Self {
        use nika_onboard::lifecycle::Stage as S;
        match stage {
            S::Pending => Self::Pending,
            S::Working => Self::Working,
            S::Done => Self::Done,
            S::Declared => Self::Declared,
            S::Paused => Self::Paused,
            S::Failed => Self::Failed,
            S::Attention => Self::Attention,
            _ => Self::Other,
        }
    }
}

/// The automation rail: DECLARED is never ACTIVE, SAVED is never RUN, findings are not a clean
/// check.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Rail {
    /// The draft: proposed or saved, or being composed.
    pub draft: Stage,
    /// Saved at consent.
    pub saved: Stage,
    /// The check at that consent.
    pub checked: Stage,
    /// The schedule: declared or suspended, never « active » from the session.
    pub active: Stage,
    /// The last run of this session.
    pub run: Stage,
}

impl From<&nika_onboard::lifecycle::Lifecycle> for Rail {
    fn from(l: &nika_onboard::lifecycle::Lifecycle) -> Self {
        Self {
            draft: l.draft.into(),
            saved: l.saved.into(),
            checked: l.checked.into(),
            active: l.active.into(),
            run: l.run.into(),
        }
    }
}

impl Work {
    /// A snapshot from its parts, under the current [`CONTRACT`].
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: PathBuf,
        request: Request,
        waiting: Waiting,
        candidate: Option<Candidate>,
        saved: Option<Saved>,
        run: Option<Run>,
        rail: Rail,
    ) -> Self {
        Self {
            contract: CONTRACT,
            root,
            request,
            waiting,
            candidate,
            saved,
            run,
            rail,
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
