// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Terminal-2 (`nika:done`) classification and the loop's per-run
//! ledger — split from `lib.rs` so `classify_turn` and `run_loop` stay
//! under the 100-line fn-length ratchet without growing the loop file
//! past 1,500.

use nika_kernel::ai::provider::Message;
use nika_kernel::runtime::agent::AgentStopReason;

use crate::shape;
use crate::{AgentOutput, AgentValue, VerbAgentError};

/// The ONE schema-repair allowance of a run, shared by BOTH repair
/// paths: a non-conforming `nika:done` `result:` (fed back as a tool
/// result so the model calls the sentinel again) and a free-text final
/// answer (`finalize_schema`'s tools-OFF re-ask). One budget, so a
/// mixed run can never spend two.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RepairBudget {
    /// Repairs already spent this run, across both paths.
    pub(crate) spent: u8,
    /// The run's allowance (`AgentVerb::schema_retry_budget`).
    pub(crate) total: u8,
}

impl RepairBudget {
    /// Repairs still available.
    pub(crate) fn left(self) -> u8 {
        self.total.saturating_sub(self.spent)
    }

    /// Decorate the NIKA-464 detail with what the loop actually tried —
    /// a verdict that spent repairs must SAY so (a bare validation
    /// message reads as « never attempted »). Zero spent ⇒ unchanged:
    /// a single-shot run has no attempt count to report.
    pub(crate) fn exhausted_detail(self, detail: &str) -> String {
        match self.spent {
            0 => detail.to_owned(),
            1 => format!("{detail} (after 1 repair attempt)"),
            n => format!("{detail} (after {n} repair attempts)"),
        }
    }
}

/// The loop's own per-run ledger — what every turn reads and the
/// feed-back arms write: the live transcript, the turn and token
/// counters, the model's last words, the last observations digest and
/// the ONE schema-repair counter. One value threaded through the loop's
/// helpers, so `run_loop` stays the exit-conditions site rather than a
/// variable ledger, and so the two repair paths cannot each keep a
/// counter of their own.
pub(crate) struct LoopState {
    /// The transcript the next request is built from.
    pub(crate) messages: Vec<Message>,
    /// Turns spent so far.
    pub(crate) turns: u32,
    /// Tokens spent so far, across every provider call of the run.
    pub(crate) total_tokens: u64,
    /// The model's most recent non-empty words.
    pub(crate) last_text: String,
    /// The last dispatched batch's observations digest (a routing signal).
    pub(crate) last_observations: String,
    /// Schema repairs spent this run: `nika:done` result repairs AND
    /// free-text re-asks draw on this one counter, so a mixed run can
    /// never spend two budgets.
    pub(crate) repairs: u8,
}

impl LoopState {
    /// A fresh ledger over the opening transcript.
    pub(crate) fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            turns: 0,
            total_tokens: 0,
            last_text: String::new(),
            last_observations: String::new(),
            repairs: 0,
        }
    }

    /// The repair allowance as of now, against the run's `total`.
    pub(crate) fn repair_budget(&self, total: u8) -> RepairBudget {
        RepairBudget {
            spent: self.repairs,
            total,
        }
    }
}

/// A `nika:done` turn: a finalized output, free text the loop must
/// schema-finalize (the C07 re-ask), or a non-conforming `result:` the
/// loop repairs in-place.
pub(crate) enum ExplicitDone {
    /// Validated `result:` — return as-is.
    Output(Box<AgentOutput>),
    /// Result-less done, or a string/null result vs an object schema.
    FinalText {
        /// Text the loop schema-finalizes (or returns on a no-schema task).
        text: String,
        /// Always `ExplicitCompletion` — the sentinel fired.
        stop_reason: AgentStopReason,
    },
    /// A `result:` that missed the schema while repair budget remains:
    /// the validation errors go BACK as this call's tool result and the
    /// model calls the sentinel again (the agentic convention).
    Repair {
        /// The validator's human-readable failure, verbatim.
        detail: String,
    },
}

/// Classify a `nika:done` call: a `result:` is a DELIBERATE structured
/// value, so it is validated directly — but a miss is REPAIRABLE, not
/// instantly fatal: the errors ride back as the call's tool result and
/// the model gets another go, bounded by the run's one [`RepairBudget`]
/// (the C07 string/null-vs-object case still routes to the free-text
/// re-ask, which is the shape that path already fixes). A result-LESS
/// done finishes on the last free text.
pub(crate) fn classify_explicit_done(
    result: Option<&serde_json::Value>,
    last_text: &str,
    schema: Option<&serde_json::Value>,
    turns: (u32, u64),
    repairs: RepairBudget,
) -> Result<ExplicitDone, VerbAgentError> {
    let (turns, total_tokens) = turns;
    let Some(result) = result else {
        return Ok(ExplicitDone::FinalText {
            text: last_text.to_owned(),
            stop_reason: AgentStopReason::ExplicitCompletion,
        });
    };
    let coerced = shape::coerce_done_result(result);
    match shape::shape_output(AgentValue::Structured(coerced), schema) {
        Ok(value) => Ok(ExplicitDone::Output(Box::new(AgentOutput::new(
            value,
            AgentStopReason::ExplicitCompletion,
            turns,
            total_tokens,
        )))),
        Err(_) if shape::string_or_null_may_reask(result, schema) => Ok(ExplicitDone::FinalText {
            text: result.as_str().unwrap_or("").to_owned(),
            stop_reason: AgentStopReason::ExplicitCompletion,
        }),
        Err(VerbAgentError::SchemaValidation { detail, .. }) if repairs.left() > 0 => {
            Ok(ExplicitDone::Repair { detail })
        }
        Err(VerbAgentError::SchemaValidation { detail, spend }) => {
            Err(VerbAgentError::SchemaValidation {
                detail: repairs.exhausted_detail(&detail),
                spend,
            })
        }
        Err(err) => Err(err),
    }
}
