// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The choices a candidate makes where the request leaves one open: a language, a format, a
//! default, an output field the request does not name. The provider judge lists them beside its
//! whole answer (`assumed`, no extra call). Each one is recorded on the attempt and, when the
//! verdict carries the request, shown as an assumption the person can change by restating: it
//! never holds the candidate. A seat lists nothing: its answers are choices, never text. What
//! the judge listed is its own words, kept on one line and bounded, never parsed.

use serde_json::{Value, json};

use super::{Judge, Verdict};
use crate::decide::{AlsoProperty, ChoiceQuestion};
use crate::{CompileOutcome, DiagnosticKind};
use nika_kernel::ai::provider::ProviderInferDyn;

/// The whole-request question's id.
const REQUEST: &str = "verify-request";
/// The property's name beside the whole answer.
const NAME: &str = "assumed";
/// What the provider judge is asked to list.
const STATEMENT: &str = "Also list in `assumed` each choice the program makes that the request leaves open (a language, a format, a default, an output field the request does not name), in a few words each, at most three; an empty list when it makes none.";
/// The most choices kept from one answer.
const MOST: usize = 3;
/// The most characters kept of one choice.
const LONGEST: usize = 160;

/// `question` as `judge` is asked it: a provider judge also lists the choices the program makes
/// where the request leaves one open; a seat answers the choice alone, its bytes unchanged.
pub(super) fn asked<P: ProviderInferDyn>(
    judge: &Judge<'_, P>,
    question: ChoiceQuestion,
) -> ChoiceQuestion {
    match judge {
        Judge::Provider(..) => {
            let schema = json!({"type": "array", "items": {"type": "string"}});
            question.with_also(AlsoProperty::new(NAME, schema, STATEMENT))
        }
        Judge::Seat(_) => question,
    }
}

/// What the whole answer listed, or the earlier answer it reads back: at most [`MOST`], each
/// on one line and cut at [`LONGEST`] characters; nothing when it listed none.
pub(super) fn read(verdict: &Verdict) -> Vec<String> {
    let whole = (verdict.records.iter()).rfind(|record| record["question"] == REQUEST);
    let answer = whole.map(|record| {
        let earlier = (verdict.earlier.iter()).rfind(|earlier| earlier["question"] == REQUEST);
        match earlier {
            Some(earlier) if record["read_back"] == Value::Bool(true) => earlier,
            _ => record,
        }
    });
    let listed = answer.and_then(|record| record["also"][NAME].as_array());
    (listed.into_iter().flatten())
        .filter_map(Value::as_str)
        .map(one_line)
        .filter(|said| !said.is_empty())
        .take(MOST)
        .collect()
}

/// `said` on one line: each control character a space, each run of whitespace one space, cut
/// at [`LONGEST`] characters.
fn one_line(said: &str) -> String {
    let spaced: String = (said.chars())
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let line = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(LONGEST) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line,
    }
}

/// The attempt's record keeps what the judge listed, when it listed anything.
pub(super) fn recorded(verdict: &Verdict, entry: &mut Value) {
    if !verdict.assumed.is_empty() {
        entry[NAME] = json!(verdict.assumed);
    }
}

/// Each listed choice, shown as an assumption the request leaves open: it holds nothing.
pub(super) fn shown(verdict: &Verdict, out: &mut CompileOutcome) {
    for said in &verdict.assumed {
        let message = format!(
            "The verifier notes a choice the request leaves open: « {said} ». It holds nothing; restate the request to choose otherwise."
        );
        crate::finding(out, DiagnosticKind::Applied, "verify_assumption", message);
    }
}

#[cfg(test)]
mod tests;
