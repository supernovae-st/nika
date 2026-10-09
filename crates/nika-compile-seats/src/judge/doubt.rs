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
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
