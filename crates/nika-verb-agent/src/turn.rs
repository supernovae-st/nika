// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Terminal-2 (`nika:done`) classification — split from `lib.rs` so
//! `classify_turn` stays under the 100-line fn-length ratchet without
//! growing the loop file past 1,500.

use nika_kernel::runtime::agent::AgentStopReason;

use crate::shape;
use crate::{AgentOutput, AgentValue, VerbAgentError};

/// A `nika:done` turn: either a finalized output, or free text the
/// loop must schema-finalize (the C07 re-ask).
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
}

/// Classify a `nika:done` call: a `result:` is a DELIBERATE structured
/// value (validate directly · a miss is a verdict, never a re-ask ·
/// BUG#11) except the C07 string/null-vs-object case, which re-asks.
/// A result-LESS done finishes on the last free text.
pub(crate) fn classify_explicit_done(
    result: Option<&serde_json::Value>,
    last_text: &str,
    schema: Option<&serde_json::Value>,
    turns: u32,
    total_tokens: u64,
) -> Result<ExplicitDone, VerbAgentError> {
    match result {
        Some(result) => {
            let coerced = shape::coerce_done_result(result);
            match shape::shape_output(AgentValue::Structured(coerced), schema) {
                Ok(value) => Ok(ExplicitDone::Output(Box::new(AgentOutput::new(
                    value,
                    AgentStopReason::ExplicitCompletion,
                    turns,
                    total_tokens,
                )))),
                Err(_) if shape::string_or_null_may_reask(result, schema) => {
                    Ok(ExplicitDone::FinalText {
                        text: result.as_str().unwrap_or("").to_owned(),
                        stop_reason: AgentStopReason::ExplicitCompletion,
                    })
                }
                Err(err) => Err(err),
            }
        }
        None => Ok(ExplicitDone::FinalText {
            text: last_text.to_owned(),
            stop_reason: AgentStopReason::ExplicitCompletion,
        }),
    }
}
