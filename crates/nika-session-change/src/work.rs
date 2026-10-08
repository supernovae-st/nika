// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The work a session holds, typed once for every host (ADR-133 · the portable session): what
//! the next line answers, with the identity an answer names, and one snapshot of the request,
//! the compiler's last word on it, the candidate under review, the saved workflow, the run
//! requested last and the last observed run, each workflow with the reach its exact bytes
//! declare. The session builds
//! it from its own state; a terminal, the plain loop or a remote door renders it and decides
//! nothing from it. It serializes so a remote door can carry the same facts, and it grants
//! nothing: a consent, an answer or a run still goes through the session's own doors.
//!
//! The contract is versioned by [`CONTRACT`]; additive fields keep the version. What the
//! session does not hold yet (a revision lineage, requirement states, typed verdicts, a run's
//! effects) is absent rather than inferred.

use std::path::PathBuf;

use serde::Serialize;

use nika_onboard::compile::{CompileDiagnostic, CompileOutcome, CompileStatus, DiagnosticKind};

use crate::change::{ProjectChange, ProjectChangeSet, Witness, WorkflowAudit};
use crate::outcome::{GateId, ProposalId, QuestionId, ReviewId};
use crate::world::World;

/// The version a host checks before reading a [`Work`].
pub const CONTRACT: &str = "nika/session-work@0";

/// A question's identity on the wire: its witness (the question, its request revision and the
/// intelligence that reads the answer), never the session that asked it.
fn question_witness<S: serde::Serializer>(id: &QuestionId, out: S) -> Result<S::Ok, S::Error> {
    out.serialize_str(id.as_str())
}

/// What the next line answers, by the session's one precedence: a requested run's cost review,
/// the one-time cost decision, the choice of intelligence, a proposal's consent, a run's gate,
/// then the values a question, a run input or an activation asks. Hosts route a line through the
/// session with it; none keeps a routing bit of its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Waiting {
    /// Nothing waits: the next line is a new turn.
    Free,
    /// A requested run's child waits at its fresh cost review: one yes runs it once, a decline
    /// sends nothing. The host holds the child; the review's screen and evidence stay with it.
    RunReview {
        /// The identity an answer names.
        review: ReviewId,
    },
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

impl Waiting {
    /// A spending decision: only a line typed after it was shown may answer it, so a host never
    /// hands it a line typed ahead (a run's cost review, the one-time cost decision).
    #[must_use]
    pub fn requires_fresh_input(&self) -> bool {
        matches!(self, Self::RunReview { .. } | Self::CostChoice)
    }
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
    /// The compiler's last word on the request, when the session holds one ([`Work::with_authoring`]).
    pub authoring: Option<Authoring>,
    /// What the next line answers.
    pub waiting: Waiting,
    /// The candidate under review, when one is.
    pub candidate: Option<Candidate>,
    /// The workflow saved by the last consent of this session, when one was.
    pub saved: Option<Saved>,
    /// The run this session requested last, when one was: its workflow, the inputs it binds and
    /// where its bytes reach. A requested run is not an observed one ([`Work::run`]).
    pub requested: Option<RequestedRun>,
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

/// The compiler's last word on the request: whether a candidate is ready and, when none is,
/// what it applied, missed or could not tell. The session reads it from its last authoring
/// round and re-derives nothing, so a host can show why nothing is ready rather than a generic
/// failure. Ready is a compiler status, never a consent and never a run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct Authoring {
    /// The compiler's status.
    pub status: AuthoringStatus,
    /// The semantic holes the compiler asks, by key, in its order.
    pub questions: Vec<String>,
    /// What happened to each part of the request, in the compiler's order and words.
    pub diagnostics: Vec<AuthoringNote>,
    /// The witness of the candidate bytes the compiler built, proposed or not.
    pub candidate: Option<Witness>,
}

impl Authoring {
    /// What a host shows of a compile outcome; the candidate's bytes stay with the session.
    #[must_use]
    pub fn of(outcome: &CompileOutcome) -> Self {
        Self {
            status: outcome.status.into(),
            questions: outcome.questions.iter().map(|q| q.key.clone()).collect(),
            diagnostics: outcome.diagnostics.iter().map(AuthoringNote::of).collect(),
            candidate: (outcome.candidate.as_deref()).map(|source| Witness::of(source.as_bytes())),
        }
    }
}

/// The compiler's status, as this contract names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AuthoringStatus {
    /// A candidate exists, every question is answered and its preview is clean.
    Ready,
    /// A value, a clarification or an unsupported part remains.
    Incomplete,
    /// A compiler policy refused the request.
    Refused,
    /// A status this contract version does not name yet: shown as unknown, never guessed.
    Other,
}

impl From<CompileStatus> for AuthoringStatus {
    fn from(status: CompileStatus) -> Self {
        match status {
            CompileStatus::Ready => Self::Ready,
            CompileStatus::Incomplete => Self::Incomplete,
            CompileStatus::Refused => Self::Refused,
            _ => Self::Other,
        }
    }
}

/// What happened to one part of the request, in the compiler's words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct AuthoringNote {
    /// What happened to it.
    pub kind: NoteKind,
    /// The request fragment or hole it concerns.
    pub target: String,
    /// The compiler's explanation, carried as written and never parsed.
    pub message: String,
}

impl AuthoringNote {
    fn of(diagnostic: &CompileDiagnostic) -> Self {
        Self {
            kind: diagnostic.kind.into(),
            target: diagnostic.target.clone(),
            message: diagnostic.message.clone(),
        }
    }
}

/// What happened to a part of the request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NoteKind {
    /// Applied as asked.
    Applied,
    /// Not applied.
    Missed,
    /// The compiler does not know the requested semantics.
    Unknown,
    /// A value or a clarification must come from the human.
    RequiresHuman,
    /// A compiler policy refused it.
    Refused,
    /// A kind this contract version does not name yet: shown as unknown, never guessed.
    Other,
}

impl From<DiagnosticKind> for NoteKind {
    fn from(kind: DiagnosticKind) -> Self {
        match kind {
            DiagnosticKind::Applied => Self::Applied,
            DiagnosticKind::Missed => Self::Missed,
            DiagnosticKind::Unknown => Self::Unknown,
            DiagnosticKind::RequiresHuman => Self::RequiresHuman,
            DiagnosticKind::Refused => Self::Refused,
            _ => Self::Other,
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
    /// How the workflow it lands was revised over its complete document, when it was: bound to
    /// these exact bytes, never to an earlier candidate's.
    pub revision: Option<DocumentRevision>,
}

/// A revision applied over a complete workflow document: what was changed and which admitted
/// components the bytes hold, each witnessed on these very bytes. It states what the compiler
/// did, never that the result is what the request meant: that is the judge's and the run's.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct DocumentRevision {
    /// `operations` (literal edits and component merges) or `replaced` (the whole source).
    pub mode: String,
    /// The digest (sha256) of the bytes revised.
    pub base_sha256: Option<String>,
    /// The digest (sha256) of the candidate's bytes, the ones the record binds.
    pub candidate_sha256: String,
    /// The node paths and components the operations changed, in order.
    pub changed: Vec<String>,
    /// The preservation the compiler claims, in words (none for a replacement).
    pub preservation: String,
    /// The admitted components the bytes hold.
    pub components: Vec<ComponentUse>,
}

impl DocumentRevision {
    /// The revision a compile record states (`document_revision`), with the witness of each of
    /// its components on the candidate's bytes, in order. `None` when the record states none.
    #[must_use]
    pub fn of(record: &serde_json::Value, witnesses: &[String]) -> Option<Self> {
        let text = |value: &serde_json::Value| value.as_str().map(str::to_owned);
        let components = (record["components"].as_array().into_iter().flatten())
            .enumerate()
            .map(|(at, receipt)| ComponentUse {
                id: text(&receipt["component"]["id"]).unwrap_or_default(),
                version: text(&receipt["component"]["release"]["version"]),
                release: text(&receipt["component"]["release"]["snapshot_sha256"]),
                file_sha256: text(&receipt["component"]["file_sha256"]),
                bindings: (receipt["bindings"].as_array().into_iter().flatten())
                    .map(|row| Bound {
                        path: text(&row["path"]).unwrap_or_default(),
                        value: row["bound"].clone(),
                    })
                    .collect(),
                witness: witnesses
                    .get(at)
                    .cloned()
                    .unwrap_or_else(|| "unwitnessed".to_owned()),
            })
            .collect();
        Some(Self {
            mode: text(&record["mode"])?,
            base_sha256: text(&record["base_sha256"]),
            candidate_sha256: text(&record["candidate_sha256"])?,
            changed: (record["changed"].as_array().into_iter().flatten())
                .filter_map(text)
                .collect(),
            preservation: text(&record["preservation"]).unwrap_or_default(),
            components,
        })
    }
}

/// One admitted component a candidate's bytes hold.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct ComponentUse {
    /// The component (`block:<name>`).
    pub id: String,
    /// The release version it resolved in.
    pub version: Option<String>,
    /// The release snapshot digest it resolved in.
    pub release: Option<String>,
    /// The digest of the admitted bytes it was expanded from.
    pub file_sha256: Option<String>,
    /// Each hole bound, with its value.
    pub bindings: Vec<Bound>,
    /// What the bytes show of it now: `expanded` (its nodes, as bound), `revised` (some
    /// changed since), `absent`, or `unwitnessed`.
    pub witness: String,
}

/// One hole bound and its value.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Bound {
    /// The hole's path in the component (`const.max_age_hours`).
    pub path: String,
    /// The bound literal.
    pub value: serde_json::Value,
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
            revision: None,
        }
    }

    /// The same candidate, with how its workflow was revised over its complete document.
    #[must_use]
    pub fn with_revision(mut self, revision: Option<DocumentRevision>) -> Self {
        self.revision = revision;
        self
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
    /// Where the bytes that consent saved reach, from that same check. `None` when the workflow
    /// named here is not the one the consent saved (a workflow only run) or no check ran.
    pub world: Option<World>,
}

impl Saved {
    /// The saved workflow, the check its consent ran and the reach that check declared.
    #[must_use]
    pub fn new(workflow: PathBuf, check_clean: Option<bool>, world: Option<World>) -> Self {
        Self {
            workflow,
            check_clean,
            world,
        }
    }
}

/// The run this session requested last: a request handed to the host, never an observation of
/// what ran. It keeps the reach of the bytes the check cleared for it, so a host can say before
/// and after the run whether those bytes contact a local service or a connected one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct RequestedRun {
    /// The workflow, relative to the root.
    pub workflow: PathBuf,
    /// The names of the inputs the request binds. Their values stay with the run request: an
    /// address, an account or a token is never copied into the snapshot.
    pub inputs: Vec<String>,
    /// Where the requested bytes reach, from the check that cleared the request.
    pub world: World,
}

impl RequestedRun {
    /// A request over `workflow` binding `vars` (`name=value`, as the run door takes them); only
    /// the names are kept.
    #[must_use]
    pub fn new(workflow: PathBuf, vars: &[String], world: World) -> Self {
        let inputs = vars
            .iter()
            .map(|var| var.split_once('=').map_or(var.as_str(), |(name, _)| name))
            .map(str::to_owned)
            .collect();
        Self {
            workflow,
            inputs,
            world,
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
        requested: Option<RequestedRun>,
        run: Option<Run>,
        rail: Rail,
    ) -> Self {
        Self {
            contract: CONTRACT,
            root,
            request,
            authoring: None,
            waiting,
            candidate,
            saved,
            requested,
            run,
            rail,
        }
    }

    /// The same snapshot with the compiler's last word on the request.
    #[must_use]
    pub fn with_authoring(mut self, authoring: Option<Authoring>) -> Self {
        self.authoring = authoring;
        self
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
