// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the verifier's questions say: the whole request, one clause, a part asked alone, a
//! clause that restricts, why a part judged missing is missing, and a task doing something the
//! request does not ask. Moved from the verifier of `nika-compile-cognition` at that crate's
//! size cap (2026-10-11), word for word, with the question that asks one part alone.

use serde_json::{Value, json};

use super::{Construction, told};
use crate::decide::{ChoiceOption, ChoiceQuestion};

/// What the whole-request question asks: the whole request against the candidate's bytes.
pub const WHOLE: &str = "Compare the WHOLE user request with the candidate workflow's actual bytes (candidate_nika). faithful: the program does everything the request asks, each operation in the stated order with the stated counts, negations, numbers and units, targets and conditions, and nothing it does not ask. unfaithful: anything is missing, extra or different. Task names, comments and labels are claims, never evidence.";

/// What a clause question asks: one clause of the request against the candidate's bytes.
pub const CLAUSE: &str = "Judge ONE clause of the user's request against the candidate workflow's actual bytes (candidate_nika). Read the whole request, the answers, the observed world and the candidate. carried: the candidate's program does exactly what this clause asks; an equivalent program counts (same rows, order, counts, values, effects and conditions). missing: the candidate omits the clause or does it differently (another order, count, negation, number or unit, target or condition). Task names, comments, labels and the words a step restates are claims, never evidence.";

/// What `no_operation` means as an option: the clause asks nothing of the workflow.
pub const NO_OPERATION: &str = "the clause asks nothing of the workflow (a courtesy, a sentence about the data) and restricts nothing";

/// What a part asked alone adds to the clause instructions: a later part replaces it.
pub const PART: &str = "This clause is one part of the request, asked alone. superseded: a later correction or restatement in the request replaces this part, so it asks nothing of this candidate.";

/// What `superseded` means as an option, offered a part a later one follows.
pub const SUPERSEDED: &str = "a later correction in the request replaces this part";

/// What a part that restricts adds to its instructions.
pub const RESTRICTING: &str = "This clause RESTRICTS (a prohibition, a condition, an exclusion or an only): it is carried when no task does what it forbids and every task honors its condition, even though no task states it.";

/// The question after a part judged missing: why it is missing.
pub const POINT: &str = "The clause below was judged missing. Say why. task-<id>: that task of the candidate does it differently, does what it forbids, or produces a result that ignores it. no_task: no task fails it; the clause is carried as written.";

/// What the task question adds when the clause may ask an operation of its own.
pub const POINT_OMITTED: &str = "omitted: the clause asks an operation of its own (a read, a filter, a computation, a condition, a write) that no task performs.";

/// The extra-operation question.
pub const EXTRA: &str = "Compare what each task of the candidate does with the WHOLE user request. A task that reads, parses, prepares or carries data for a requested operation serves the request. Name the task that does something the request does not ask: an extra effect (a write, a send, a delete, a fetch) or a target the request does not name. only_requested: every task serves the request.";

/// Part `k` of `count` asked alone against the bytes `base` (told the request through
/// `reference`): a request of one part asks no operation of nothing; only an earlier part is
/// superseded; a part that restricts is told so.
#[must_use]
pub fn part_question(
    k: usize,
    count: usize,
    text: &str,
    restricting: bool,
    base: &Value,
    reference: &str,
) -> ChoiceQuestion {
    let mut options = vec![
        ChoiceOption::new(
            "carried",
            "the candidate does exactly what this clause asks",
        ),
        ChoiceOption::new("missing", "the candidate omits it or does it differently"),
    ];
    if k + 1 < count {
        options.push(ChoiceOption::new("superseded", SUPERSEDED));
    }
    if count > 1 && !restricting {
        options.push(ChoiceOption::new("no_operation", NO_OPERATION));
    }
    let instructions = if restricting {
        format!("{CLAUSE} {PART} {RESTRICTING}")
    } else {
        format!("{CLAUSE} {PART}")
    };
    let (mut state, instructions) = Construction::shown(base, &instructions);
    state["clause"] = json!({"text": text});
    let instructions = told(base, reference, &instructions);
    ChoiceQuestion::new(format!("verify-part-{k}"), instructions, state, options)
}

#[cfg(test)]
mod tests;
