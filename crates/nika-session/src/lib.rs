// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native session — bare `nika` on a terminal (ADR-125 · One Door ·
//! wave 4).
//!
//! A grounded conversation over the INSTALLED engine, never a temporary
//! workflow: the runtime observes the project ([`snapshot`]), selects the
//! intelligence the human chose ([`intelligence`] · a seat, an API, a
//! local engine, or none), hands the reasoner a minimal typed bundle
//! through the [`broker`] (the proven root · only what the human named ·
//! secrets redacted · the environment never injected · provenance kept),
//! answers Nika facts from the engine's own authorities ([`facts`]), and
//! reads every reply through the [`guard`] before a human sees it — a
//! named builtin, model, code, MCP server, verb or field this engine does
//! not carry is corrected, never presented as real.
//!
//! Work to build reaches the ONE compiler ([`authoring`] · the canonical
//! typed Compile CREATE): its typed question is the next line's meaning,
//! its Ready candidate is reviewed from the engine's own facts
//! ([`review`]) and becomes a typed [`change`] set — previewed from the
//! exact bytes the apply consumes, witnessed against stale targets, landed
//! only on the human's consent, checked by the real checker after it lands
//! (ADR-126 · wave 5). A reasoner's reply is words: it never becomes a file.
//!
//! What the session must NOT own is what it queries: the grammar, the
//! catalogs, the codes, the checker, the compiler, the runtime, and what a
//! run's trace proves (`nika_trace::run_view`, read for the result, gate
//! and `/proof` views). Its identity core ([`identity`]) says so to the
//! model in six laws.

/// The session's typed activity and the model's identity core (its laws and the language
/// digest) are owned beside the other engine-knowledge words since 2026-09-30:
/// `nika_onboard::{activity, identity}`. These paths are kept for source compatibility and
/// name the very same items (types, functions, constant).
#[doc(inline)]
pub use nika_onboard::{activity, identity};
pub mod authoring;
pub mod broker;
/// The project change a proposal writes ([`change`]), its factual review ([`review`]), the
/// typed outcome a host renders ([`outcome`]) and the consent record ([`consent`]) are owned
/// by the size-cap member below the session since 2026-10-06 (ADR-144):
/// `nika_session_change::{change, consent, outcome, review}`. These paths are kept and name
/// the very same items (types, functions, constants).
#[doc(inline)]
pub use nika_session_change::{change, consent, outcome, review};
pub mod facts;
/// The hallucination guard (a reply's named builtins, models, codes, MCP servers, verbs
/// and fields checked against what this engine carries) is owned beside the other
/// engine-knowledge words since 2026-09-29: `nika_onboard::guard`. This path is kept for
/// source compatibility and names the very same items (types, functions).
#[doc(inline)]
pub use nika_onboard::guard;
pub mod intelligence;
/// The Meaning view — what survived of a request, clause by clause, read from
/// the compiler's obligation ledger — is owned beside that ledger since
/// 2026-09-28: `nika_onboard::compile::meaning`. This path is kept for source
/// compatibility and names the very same items (types, functions, constant).
#[doc(inline)]
pub use nika_onboard::compile::meaning;
/// The automation rail (DRAFT · SAVED · CHECKED · ACTIVE · RUN, each at its own stage) is
/// owned beside the other engine-knowledge words since 2026-09-29: `nika_onboard::lifecycle`.
/// This path is kept for source compatibility and names the very same items (types,
/// functions).
#[doc(inline)]
pub use nika_onboard::lifecycle;
pub mod money;
pub mod reasoner;
pub mod runtime;
pub mod snapshot;
pub mod state;
pub mod turn;

// What a run's trace proves (the result, gate and `/proof` views) is read
// here, never owned: the flight-recorder reader holds it, and
// `crate::run_view` stays the session's one path to it.
use nika_trace::run_view;
pub use nika_trace::run_view::KeptRun;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod episode_tests;

pub use authoring::{AuthoringRound, AuthoringSeat, Reading};
pub use broker::{ContextBroker, SessionContextBundle, Snippet};
pub use change::{
    Applied, ChangeError, PendingGate, ProjectChange, ProjectChangeSet, RunRequest, Witness,
    WorkflowAudit,
};
pub use consent::{ConsentDecision, ConsentRecord, ConsentWitness};
pub use guard::{Finding, KnownWorld};
pub use intelligence::{
    DataLocus, IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence,
    UserIntelligencePreference,
};
pub use lifecycle::{Lifecycle, LifecycleFacts, RunFact, Stage};
/// The program a host names to run the observed room's `nika:jq` steps.
pub use nika_onboard::compile::room::JqHelper;
pub use nika_runtime::cost_choice::{CapEvidence, CostHostEvidence};
pub use outcome::{GateId, ProposalId, QuestionId, Refusal, RefusalClass};
pub use reasoner::{ReasonError, Reply, ScriptedReasoner, SessionReasoner};
pub use runtime::{SessionRuntime, TurnOutcome};
pub use snapshot::ProjectSnapshot;
pub use state::{Pending, SessionState};

pub use nika_providers::{AdmissionState, AttemptReceipt, InferenceReceipt};
