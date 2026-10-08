// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rehearsal port: a host that can run a candidate in a safe room built from the observed
//! world says what the run did before the native door declares the candidate READY. The
//! compile core does no I/O, so the run is a host act behind this port, the way a decision
//! seat is ([`crate::decide::DecisionSeat`]). A rehearsal is never a provider call, never
//! records consent and never writes outside its room; a host that cannot rehearse a candidate
//! safely answers [`Rehearsal::NotRun`] with its reason. Every report names the exact bytes it
//! rehearsed and the world it admitted, whether a run began and how it ended, what the host did
//! around the run, and the effects its denied seams saw attempted. The host's [`Observation`] of
//! the copied world rides each report, and [`judged_run`] maps it to the behavioural judge's run.

use std::{future::Future, pin::Pin, time::Duration};

use nika_compile::surface::{UNJUDGED_DEPENDENCY, parse, ready_by_law, sha256};
use nika_compile::{CompileOutcome, CompileStatus, DiagnosticKind};
use serde_json::json;

pub mod arguments;
#[cfg(test)]
mod composed_tests;
mod judged;
#[cfg(test)]
mod judged_tests;
mod observed;
mod shown;

pub use judged::{judged_run, targets_of};
pub use observed::{
    Bounds, CopyReceipt, Digest, FailureRecord, FinalReceipt, FinalState, Held, LedgerFacts,
    Observation, RecordedCause, Refusal, Spent,
};
pub use shown::{trial_receipts, trial_shown, trial_whole};

/// One declared output a rehearsal read back from its room.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RehearsedOutput {
    /// The declared path, relative to the project root, as the candidate writes it.
    pub path: String,
    /// A bounded preview of what the room holds at that path, cut at a character boundary.
    pub text: String,
    /// Whether the run itself published this path. A copied file, a failed publish or a file
    /// that already existed is no write.
    pub written: bool,
    /// Whether `text` is only a prefix of the content: a cut preview never proves a
    /// whole-output contract.
    pub truncated: bool,
    /// The byte length of the whole content.
    pub full_bytes: u64,
    /// The sha256 of the whole content (lowercase hex), streamed; empty when the host did not
    /// compute it.
    pub full_sha256: String,
}

impl RehearsedOutput {
    /// One output the run wrote, read back whole.
    #[must_use]
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            path: path.into(),
            full_bytes: text.len() as u64,
            text,
            written: true,
            truncated: false,
            full_sha256: String::new(),
        }
    }

    /// The same output, marked as published by the run or not.
    #[must_use]
    pub fn with_written(mut self, written: bool) -> Self {
        self.written = written;
        self
    }

    /// The same output, its preview cut from content of `full_bytes` bytes hashing to
    /// `full_sha256`.
    #[must_use]
    pub fn with_full(mut self, full_bytes: u64, full_sha256: impl Into<String>) -> Self {
        self.truncated = full_bytes > self.text.len() as u64;
        self.full_bytes = full_bytes;
        self.full_sha256 = full_sha256.into();
        self
    }
}

/// What one rehearsal of a candidate proved.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rehearsal {
    /// The run completed and every declared output was read back.
    Passed {
        /// The declared outputs, in the order the candidate writes them.
        outputs: Vec<RehearsedOutput>,
    },
    /// The run failed: the failing task, its code and its message.
    Failed {
        /// The error code the runtime reported (`NIKA-…`).
        code: String,
        /// The task id that failed.
        task: String,
        /// The runtime's message, cut at the host's byte bound.
        message: String,
    },
    /// The run completed, but a declared output the contract requires was never written.
    Missing {
        /// The declared outputs the run never wrote.
        outputs: Vec<String>,
    },
    /// The host did not complete a run it could vouch for: a provider, the network, exec, a
    /// gate, a path outside the room, an input nobody observed, an effect its seams denied, an
    /// admission refusal, or the time bound. The reason is stated; [`Attempt`] says whether a
    /// run began, and [`RoomEvidence`] what the host did around it.
    NotRun {
        /// Why, in words.
        reason: String,
    },
}

/// Whether a run began, and how it ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Attempt {
    /// No run began. A room may still have been prepared and cleaned: see [`RoomEvidence`].
    NeverAttempted,
    /// The run ended by itself.
    Completed {
        /// Milliseconds from the start of the run to the end of its drain.
        elapsed_ms: u64,
    },
    /// The run was stopped at the bound or cancelled; every accepted operation was joined.
    Stopped {
        /// Milliseconds from the start of the run to the end of the drain, the join included.
        elapsed_ms: u64,
    },
}

/// What the host did around the run, stated whatever the attempt was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RoomEvidence {
    /// A room was created and the observed inputs were copied into it.
    pub prepared: bool,
    /// Cleanup verified that no room is left behind.
    pub cleaned: bool,
    /// Operations refused because they arrived after their phase was sealed. Nothing they
    /// asked for ran; any count above zero means a producer outlived its phase, so the
    /// outcome is never a pass.
    pub late_refused: u32,
}

impl RoomEvidence {
    /// The evidence of a host that stated both facts, with no late operation.
    #[must_use]
    pub fn new(prepared: bool, cleaned: bool) -> Self {
        Self {
            prepared,
            cleaned,
            late_refused: 0,
        }
    }

    /// The same evidence with the late refusals the host counted.
    #[must_use]
    pub fn with_late_refused(mut self, late_refused: u32) -> Self {
        self.late_refused = late_refused;
        self
    }

    /// No room was ever prepared, so none is left behind.
    #[must_use]
    pub fn untouched() -> Self {
        Self::new(false, true)
    }
}

/// The effects the host's denied seams saw attempted. A safe rehearsal reports none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct EffectCounts {
    /// Network requests, the fetch and provider transports included.
    pub network: u32,
    /// Provider calls.
    pub provider: u32,
    /// Process spawns.
    pub spawn: u32,
    /// Prompts to a human (a gate); none is ever answered.
    pub prompt: u32,
    /// Secret-store reads.
    pub secret: u32,
    /// Nested workflow runs.
    pub child: u32,
}

impl EffectCounts {
    /// No effect attempted.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Whether no effect was attempted.
    #[must_use]
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }
}

/// What one rehearsal proved, bound to the exact bytes it rehearsed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RehearsalReport {
    /// The outcome.
    pub outcome: Rehearsal,
    /// Whether a run began, and how it ended.
    pub attempt: Attempt,
    /// What the host did around the run.
    pub room: RoomEvidence,
    /// The effects the host's denied seams saw attempted.
    pub effects: EffectCounts,
    /// The sha256 of the candidate bytes the host rehearsed (lowercase hex).
    pub candidate_sha256: String,
    /// The digest the admission door gave the world it admitted (the candidate alone, admitted
    /// from its bytes); empty when nothing was admitted.
    pub admitted_digest: String,
    /// What the host observed of the copied world: the internal record [`judged_run`] reads.
    pub observation: Observation,
}

impl RehearsalReport {
    /// One report of a host that prepared no room and admitted nothing.
    #[must_use]
    pub fn new(
        outcome: Rehearsal,
        attempt: Attempt,
        effects: EffectCounts,
        candidate_sha256: impl Into<String>,
    ) -> Self {
        Self {
            outcome,
            attempt,
            room: RoomEvidence::untouched(),
            effects,
            candidate_sha256: candidate_sha256.into(),
            admitted_digest: String::new(),
            observation: Observation::none(),
        }
    }

    /// The same report with the host's room evidence.
    #[must_use]
    pub fn with_room(mut self, room: RoomEvidence) -> Self {
        self.room = room;
        self
    }

    /// The same report with the digest of the world the admission door admitted.
    #[must_use]
    pub fn with_admitted_digest(mut self, admitted_digest: impl Into<String>) -> Self {
        self.admitted_digest = admitted_digest.into();
        self
    }

    /// The same report with what the host observed of the copied world.
    #[must_use]
    pub fn with_observation(mut self, observation: Observation) -> Self {
        self.observation = observation;
        self
    }
}

/// The object-safe future a rehearsal answers with.
pub type RehearsalFuture<'a> = Pin<Box<dyn Future<Output = RehearsalReport> + Send + 'a>>;

/// What a host's read-only composition check found of one candidate: the candidate checked with
/// every child workflow it invokes, at the place its bytes will be saved, nothing run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Composed {
    /// The host offers no composition check: the source-only hold on a child stands.
    Unoffered,
    /// The host cannot say where these bytes will be saved, so their relative children are not
    /// resolved; nothing was read.
    Unresolved {
        /// Why the location is not known.
        reason: String,
    },
    /// The closure could not be captured, or one of its workflows failed its composed Check.
    Refused {
        /// The capture or Check reason, its codes included.
        reason: String,
    },
    /// The closure was captured at its location and every captured workflow checked clean.
    Clean(Closure),
}

/// The closure a clean composition check captured: the bytes checked, where, and every workflow
/// read with them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Closure {
    /// The SHA-256 of the candidate bytes checked.
    pub candidate_sha256: String,
    /// The project-relative path the candidate was checked at: where its bytes will be saved.
    pub logical_root: String,
    /// The snapshot identity of the captured closure: SHA-256 over the snapshot's own framing of
    /// its format, root, unit roles, paths and bytes (no file's digest).
    pub snapshot_identity: String,
    /// The snapshot format that framing follows.
    pub snapshot_format: u32,
    /// Every captured unit: its project-relative path and the SHA-256 of its exact bytes.
    pub units: Vec<(String, String)>,
}

impl Closure {
    /// The closure of `candidate_sha256` checked at `logical_root`, captured as `snapshot`
    /// (its identity and format) with these `units`.
    #[must_use]
    pub fn new(
        candidate_sha256: impl Into<String>,
        logical_root: impl Into<String>,
        snapshot: (impl Into<String>, u32),
        units: Vec<(String, String)>,
    ) -> Self {
        Self {
            candidate_sha256: candidate_sha256.into(),
            logical_root: logical_root.into(),
            snapshot_identity: snapshot.0.into(),
            snapshot_format: snapshot.1,
            units,
        }
    }
}

/// The child units (every captured unit but the root) on whose digest `closure` and the
/// `decision.composition` `record` of an earlier check disagree, a unit gone or new included,
/// each path quoted: what changed between that check and this capture.
#[must_use]
pub fn changed_children(record: &serde_json::Value, closure: &Closure) -> Vec<String> {
    use std::collections::{BTreeMap, BTreeSet};
    let units = (record["units"].as_array().into_iter().flatten())
        .filter_map(|unit| Some((unit[0].as_str()?.to_owned(), unit[1].as_str()?.to_owned())));
    let recorded: BTreeMap<String, String> = units
        .filter(|(path, _)| record["logical_root"] != path.as_str())
        .collect();
    let fresh: BTreeMap<String, String> = (closure.units.iter())
        .filter(|(path, _)| *path != closure.logical_root)
        .cloned()
        .collect();
    let paths: BTreeSet<&String> = recorded.keys().chain(fresh.keys()).collect();
    (paths.into_iter())
        .filter(|path| recorded.get(*path) != fresh.get(*path))
        .map(|path| format!("`{path}`"))
        .collect()
}

/// The tasks of `out`'s candidate that invoke a child workflow and still carry the source-only
/// hold the core states on them ([`UNJUDGED_DEPENDENCY`]).
#[must_use]
pub fn held_children(out: &CompileOutcome) -> Vec<String> {
    use nika_schema::raw::{RawAction, RawInvokeTarget};
    let Some(wf) = out.candidate.as_deref().and_then(|c| parse(c).ok()) else {
        return Vec::new();
    };
    let held = |id: &str| {
        (out.diagnostics.iter()).any(|d| {
            d.kind == DiagnosticKind::Unknown && d.target == id && d.message == UNJUDGED_DEPENDENCY
        })
    };
    (wf.tasks.iter())
        .filter(|t| match &t.value.action {
            RawAction::Invoke(invoke) => matches!(invoke.target, RawInvokeTarget::Workflow(_)),
            _ => false,
        })
        .map(|t| t.value.id.value.clone())
        .filter(|id| held(id))
        .collect()
}

/// Lift the source-only hold on the children `closure` checked composed with `out`'s candidate:
/// only on the tasks [`held_children`] names, only when the closure names the outcome's own
/// bytes, its Check is clean and nothing is refused; READY then follows the core's own law. An
/// MCP tool, a skill, a question or any other finding keeps its hold. Returns the tasks lifted.
pub fn discharge_children(out: &mut CompileOutcome, closure: &Closure) -> Vec<String> {
    let clean = (out.check_preview.as_ref()).is_some_and(|preview| preview.report.is_clean());
    let same = (out.candidate.as_deref()).is_some_and(|c| sha256(c) == closure.candidate_sha256);
    if !(clean && same) || out.status == CompileStatus::Refused {
        return Vec::new();
    }
    let lifted = held_children(out);
    (out.diagnostics).retain(|d| d.message != UNJUDGED_DEPENDENCY || !lifted.contains(&d.target));
    if !lifted.is_empty() && ready_by_law(out, clean) {
        out.status = CompileStatus::Ready;
    }
    lifted
}

/// The composition check `host` makes of `out`'s candidate when it holds a child workflow
/// source-only (R5): a fresh capture on every call (a child may change between rounds), recorded
/// on `decision.composition`. A clean closure of these exact bytes lifts the hold; any other
/// answer keeps it and says why.
pub fn composed(host: &dyn Rehearse, out: &mut CompileOutcome) {
    let held = held_children(out);
    let Some(candidate) = out.candidate.clone().filter(|_| !held.is_empty()) else {
        return;
    };
    let mut record = json!({"candidate_sha256": sha256(&candidate), "held": held,
        "discharged": [], "reason": null, "logical_root": null});
    match host.compose(&candidate) {
        Composed::Clean(closure) => {
            record["verdict"] = json!("clean");
            record["logical_root"] = json!(closure.logical_root);
            record["snapshot_identity"] = json!(closure.snapshot_identity);
            record["snapshot_format"] = json!(closure.snapshot_format);
            record["units"] = json!(closure.units);
            record["discharged"] = json!(discharge_children(out, &closure));
        }
        Composed::Refused { reason } => {
            record["verdict"] = json!("refused");
            record["reason"] = json!(reason);
        }
        Composed::Unresolved { reason } => {
            record["verdict"] = json!("unresolved");
            record["reason"] = json!(reason);
        }
        Composed::Unoffered => record["verdict"] = json!("unoffered"),
    }
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["composition"] = record;
    out.provenance.decision = Some(decision);
}

/// A host that can rehearse a candidate in a safe room built from the observed world.
pub trait Rehearse: Send + Sync {
    /// Rehearse `candidate` (the exact bytes every law passed) over `inputs` (the observed
    /// paths, relative to the project root), within [`Rehearse::bound`].
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a>;
    /// The time bound the host applies to one rehearsal, recorded with it.
    fn bound(&self) -> Duration;
    /// Rehearse as [`Rehearse::rehearse`] does, and read back every path of `targets` (the
    /// results the request's contract names, declared by the candidate or not) beside the
    /// declared outputs, whatever the run's end. A host that keeps this default ignores
    /// `targets`: its report then holds no final state of an undeclared target, which the
    /// judge reads as no observation.
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        _targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        self.rehearse(candidate, inputs)
    }
    /// Check `candidate` read-only with every child workflow it invokes, at the place its bytes
    /// will be saved: the closure captured from the project and each captured workflow checked
    /// composed, nothing run. A host that keeps this default offers none.
    fn compose(&self, candidate: &str) -> Composed {
        let _ = candidate;
        Composed::Unoffered
    }
}
