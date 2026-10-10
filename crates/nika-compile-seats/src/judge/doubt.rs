// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The localization a run of the judged bytes would ask, asked over the bytes when no run of them
//! exists (R6). A judge that answered the whole request `unfaithful`, then found each of its
//! parts asked alone carried, superseded or asking nothing, and no task doing anything the
//! request does not ask, has located nothing. It is shown its own answer to each part and what
//! each task touches ([`Effects`]), new evidence beside the bytes, and asked once where its
//! doubt is: a part of the request, a task the facts leave open, or nothing.
//!
//! A part it names is asked its task next and becomes a defect only with that reason; a task is
//! a defect; nothing named leaves the doubt unresolved, never READY. No option carries the
//! request, so a second answer of the same judge never outvotes its first.
//!
//! A whole-request answer a seat weighs (a distribution over the offered options) decides only
//! as a confident rejection (amended 2026-10-10, [`Doubt::decides`]): otherwise its parts decide,
//! an answer they overrule is stated with the proposal ([`Doubt::stated`]) and never asked where
//! it is. A location it weighs below [`Doubt::LOCATING_PROBABILITY`] names nothing
//! ([`Doubt::locates`]). An answer reported with no distribution is taken at its word. The
//! thresholds read the probability the seat reported for the option it chose, per question, and
//! cite the labels that set them, so the next calibration can move them.

use serde_json::{Value, json};

use super::{Construction, Effects};
use crate::decide::ChoiceOption;

/// What the question asks.
const DOUBT: &str = "You answered that the WHOLE user request is not carried by the candidate's bytes (candidate_nika). Asked one by one, you then answered each part of the request as `parts` shows (your own answers), and no task was found doing something the request does not ask. No run of these exact bytes exists to observe. Locate your doubt. part-<k>: that part of the request is in fact missing from the program or done differently. task-<id>: that task does something the request does not ask. unlocated: your doubt names no part of the request and no task.";

/// What `unlocated` means as an option.
const UNLOCATED: &str = "the doubt names no part of the request and no task";

/// What the judge's answer locates.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Located {
    /// The part at this place in the request: missing or done differently after all.
    Part(usize),
    /// A task the facts leave open, doing something the request does not ask.
    Task(String),
    /// Nothing: the doubt names no part and no task.
    Nothing,
}

/// The localizing question over a request's parts and the tasks the facts leave open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Doubt {
    /// Each part of the request, in its order.
    parts: Vec<String>,
    /// The tasks the facts leave open, in the candidate's order ([`Effects::open`]).
    open: Vec<String>,
}

impl Doubt {
    /// The question over the request's `parts` and the `open` tasks.
    #[must_use]
    pub fn new(parts: Vec<String>, open: Vec<String>) -> Self {
        Self { parts, open }
    }

    /// The question's state, instructions and options. `state` is what the whole request was
    /// asked over, told what the bytes hold ([`Construction::shown`]); it gains the judge's own
    /// answer to each part, read from `records` (`parts`), and what each task touches
    /// (`effects`). The options: each part, each open task, then `unlocated`.
    #[must_use]
    pub fn question(
        &self,
        state: &Value,
        effects: &Effects,
        records: &[Value],
    ) -> (Value, String, Vec<ChoiceOption>) {
        let (mut shown, said) = Construction::shown(state, DOUBT);
        let said = effects.show(&mut shown, records, &said);
        let answered = |k: usize| {
            let id = format!("verify-part-{k}");
            (records.iter())
                .find(|record| record["question"] == id.as_str())
                .map_or(Value::Null, |record| record["choice"].clone())
        };
        let parts: Vec<Value> = (self.parts.iter().enumerate())
            .map(|(k, text)| json!({"part": k, "text": text, "answer": answered(k)}))
            .collect();
        shown["parts"] = Value::Array(parts);
        let mut options: Vec<ChoiceOption> = (self.parts.iter().enumerate())
            .map(|(k, text)| ChoiceOption::new(format!("part-{k}"), text.clone()))
            .collect();
        options.extend((self.open.iter()).map(|task| {
            let said = format!("the task `{task}` does something the request does not ask");
            ChoiceOption::new(format!("task-{task}"), said)
        }));
        options.push(ChoiceOption::new("unlocated", UNLOCATED));
        (shown, said, options)
    }

    /// What the answer `key` locates; `None` for a key that is none of its options.
    #[must_use]
    pub fn read(&self, key: &str) -> Option<Located> {
        if key == "unlocated" {
            return Some(Located::Nothing);
        }
        if let Some(task) = key.strip_prefix("task-") {
            return (self.open.iter())
                .any(|open| open == task)
                .then(|| Located::Task(task.to_owned()));
        }
        let k: usize = key.strip_prefix("part-")?.parse().ok()?;
        (k < self.parts.len()).then_some(Located::Part(k))
    }

    /// The probability at or above which a decision seat's whole-request « unfaithful » decides:
    /// with nothing located, it holds the bytes. Balanced labels (2026-10-10, correct references
    /// and their mutants, three options): as a hold, a reported confidence of 0.75 (this
    /// probability) was right 23 times of 23, held-out requests 3 of 3, where 0.7 held two correct
    /// candidates and 0.6 five. Below it the answer decides nothing and is stated. Calibrated with
    /// the options in the order the verifier offers them (`faithful`, `unfaithful`, NONE): a seat
    /// leans 0.04 to 0.10 toward the option listed first, so another order needs a new calibration.
    pub const HOLDING_PROBABILITY: f64 = 0.83;

    /// The probability at or above which this question's answer locates a defect: on correct
    /// references it located one that is not there 3 times in 6, each below it (balanced labels,
    /// 2026-10-10), the options in the order [`Self::question`] offers them (each part, each open
    /// task, `unlocated`, NONE); another order needs a new calibration.
    pub const LOCATING_PROBABILITY: f64 = 0.5;

    /// The probability the seat reported for its own latest answer to `question` among
    /// `records`: its distribution's mass on the option it chose. `None` when it weighed nothing
    /// (no distribution, or a mass for its choice alone).
    #[must_use]
    pub fn reported(records: &[Value], question: &str) -> Option<f64> {
        latest(records, question).and_then(probability)
    }

    /// Whether the whole-request answer among `records` decides, as a judge's verdict did before
    /// any calibration: a weighed answer only as « unfaithful » at [`Self::HOLDING_PROBABILITY`]
    /// or more, an unweighed one taken at its word. A weighed « faithful » admits nothing by
    /// itself: in the balanced labels it never reached 0.6 and was wrong 5 times in 13.
    #[must_use]
    pub fn decides(records: &[Value]) -> bool {
        latest(records, "verify-request").is_none_or(|whole| {
            probability(whole)
                .is_none_or(|p| whole["choice"] == "unfaithful" && p >= Self::HOLDING_PROBABILITY)
        })
    }

    /// Whether this question's latest answer among `records` locates: weighed at
    /// [`Self::LOCATING_PROBABILITY`] or more, or unweighed (taken at its word).
    #[must_use]
    pub fn locates(records: &[Value]) -> bool {
        Self::reported(records, "verify-doubt").is_none_or(|p| p >= Self::LOCATING_PROBABILITY)
    }

    /// The doubt a verdict whose whole-request answer decides nothing leaves when its parts carry
    /// the request, in words for the proposal: that answer but « faithful », with the probability
    /// reported for it, and each part in `restated` a repair attempt met and the seat judged
    /// missing again. `None` when there is nothing to state.
    #[must_use]
    pub fn stated(records: &[Value], restated: &[String]) -> Option<String> {
        let mut said = Vec::new();
        let whole = latest(records, "verify-request").filter(|whole| whole["choice"] != "faithful");
        if let Some(whole) = whole {
            let reported = probability(whole)
                .map(|p| format!(" (reported at {p:.2})"))
                .unwrap_or_default();
            said.push(if whole["choice"] == "unfaithful" {
                format!("doubted that this workflow carries the request as a whole{reported}, a question that decides nothing on its own")
            } else {
                format!("made no decision on the request as a whole{reported}, a question that decides nothing on its own")
            });
        }
        said.extend(restated.iter().map(|part| {
            format!("judged « {part} » missing again after a repair attempt, a part's answer that holds no proposal")
        }));
        let rest = if restated.is_empty() {
            ": no part of the request, judged alone, is missing and no task does anything the request does not ask"
        } else {
            ""
        };
        (!said.is_empty()).then(|| {
            format!(
                "The verifier {}{rest}, so the workflow is proposed with this doubt stated. Read it before you consent.",
                said.join(", and ")
            )
        })
    }
}

/// The latest record of `question` among `records`.
fn latest<'r>(records: &'r [Value], question: &str) -> Option<&'r Value> {
    (records.iter().rev()).find(|record| record["question"] == question)
}

/// The probability the seat reported for the option `record` chose, when it weighed its answer: a
/// distribution over the offered options (a mass for one option alone weighs nothing).
fn probability(record: &Value) -> Option<f64> {
    let reported = (record["probabilities"].as_object()).filter(|reported| reported.len() > 1)?;
    reported.get(record["choice"].as_str()?)?.as_f64()
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
