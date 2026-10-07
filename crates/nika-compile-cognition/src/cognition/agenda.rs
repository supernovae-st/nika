// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The next useful action for an open intent (R1 · R5 · A1). HOT, WARM and COLD are no longer
//! routes to exhaust in order: their mechanisms are actions, and the one taken next follows from
//! what the request still lacks and which intelligences the caller selected. A whole reading the
//! reader composed is checked by a judge before it can be READY (a lexical reading never proves
//! the request resolved); finite readings of known clauses are settled by the decision seat, and
//! the settled plan is checked; the author composes through the private plan, and a limit the
//! plan itself shows (a computation its typed stages cannot state) sends it to the sketch door
//! next, with no separate program round to exhaust first. Exact commands (skeletons, `hello`, the support
//! grammar, structured edits) and recorded replays keep their direct paths before this. The
//! historical words stay in the route and the strategy, so old receipts read as before.

use serde_json::{Value, json};

use crate::{CompileOutcome, CompileRequest, NativeMode};

/// The route step of a reader's whole plan checked before READY.
pub(super) const READER_CHECKED: &str = "check: the reader's own plan";
/// The route step of a seat-settled plan checked under the author's repair.
pub(super) const SETTLED_CHECKED: &str = "check: the settled plan, the author repairing";
/// The route step of a plan whose computation its typed stages cannot state, composed next by
/// the sketch door (its program a typed fill) rather than by a separate program round.
pub(super) const PLAN_LIMIT: &str = "compose: the plan's computation goes to the sketch door";

/// One action of the loop, each an existing owner's mechanism.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    /// A whole reading and no judge selected: the deterministic assembly, as without cognition.
    Assemble,
    /// A whole candidate exists: a judge checks it (the author, when selected, repairs it).
    Check,
    /// Only finite readings of known clauses are open: the decision seat settles them.
    Settle,
    /// The author composes in the reader's typed vocabulary (the private plan).
    Plan,
    /// The author composes structure and typed fills (the sketch door).
    Sketch,
    /// No selected intelligence can take the next step: what is missing is named.
    Ask,
}

impl Action {
    const fn word(self) -> &'static str {
        match self {
            Self::Assemble => "assemble",
            Self::Check => "check",
            Self::Settle => "settle",
            Self::Plan => "compose: plan",
            Self::Sketch => "compose: sketch",
            Self::Ask => "ask",
        }
    }
}

/// What the request still lacks after the reader, and what was already tried.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Missing {
    /// The reader composed a whole plan under the request's admission contract.
    pub(super) composed: bool,
    /// Only finite readings of otherwise explicit clauses are open.
    pub(super) choices: bool,
    /// Clauses the reader cannot type (unresolved or unparsed prose).
    pub(super) untyped: usize,
    /// The decision seat already answered the open readings.
    pub(super) settled: bool,
}

/// The intelligences the caller selected, and the policy of the author's doors.
#[derive(Clone, Copy, Debug)]
pub(super) struct Selected {
    pub(super) author: bool,
    pub(super) seat: bool,
    pub(super) native: NativeMode,
}

impl Selected {
    pub(super) fn of(request: &CompileRequest, author: bool, seat: bool) -> Self {
        let native = request
            .authoring
            .as_ref()
            .map_or(NativeMode::Off, |p| p.native);
        Self {
            author: author && request.authoring.is_some(),
            seat,
            native,
        }
    }
}

/// The next useful action. The sketch policy is the caller's explicit choice of composer; a
/// whole reading is checked whenever a judge is selected; open readings go to the seat once;
/// then the author composes through the plan, whose own observed limits (a computation its typed
/// stages cannot state, branches it cannot keep apart, a dead end) hand it to the sketch door.
pub(super) fn next(missing: Missing, selected: Selected) -> Action {
    if selected.author && selected.native == NativeMode::Sketch {
        return Action::Sketch;
    }
    if missing.composed {
        return if selected.author || selected.seat {
            Action::Check
        } else {
            Action::Assemble
        };
    }
    if missing.choices && selected.seat && !missing.settled {
        return Action::Settle;
    }
    if selected.author {
        Action::Plan
    } else {
        Action::Ask
    }
}

/// Record one decision of the loop in `decision.agenda`: the action and what motivated it.
pub(super) fn record(out: &mut CompileOutcome, missing: Missing, action: Action) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    let entry = json!({
        "action": action.word(),
        "missing": {
            "whole_reading": !missing.composed,
            "open_readings": missing.choices && !missing.settled,
            "untyped_clauses": missing.untyped,
        },
    });
    match decision.get_mut("agenda").and_then(Value::as_array_mut) {
        Some(agenda) => agenda.push(entry),
        None => decision["agenda"] = json!([entry]),
    }
    out.provenance.decision = Some(decision);
}

#[cfg(test)]
mod tests;
