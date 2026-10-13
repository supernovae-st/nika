// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exact answers may continue a preparation request without asking for its mandate again.
//! These receipts keep the selected meaning, not a live question or execution authority.

use super::{
    Acts, Citations, Cited, Conversation, OfferValue, Pick, QuestionId, Value, Witness, answer,
    bare, in_line, json, same_literal,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Continuation {
    request: u64,
    witness: String,
    meaning: Value,
}

type Continued<'a> = (&'a str, &'a Cited, &'a Continuation);

impl Conversation {
    /// Only an exact protocol pick for a validated question can continue a mandate. A model
    /// reading of a free sentence, including a delegation with another clause, cannot do so.
    pub(super) fn continuation(&self, id: &QuestionId, line: &str) -> Option<Continuation> {
        let question = self
            .asked
            .iter()
            .chain(&self.open)
            .find(|question| question.id == *id)?;
        let pick = Pick::of_line(question, line)?;
        let offers = answer::selected(question, &pick)?;
        Some(Continuation {
            request: self.request,
            witness: Witness::of(line.as_bytes()).0,
            meaning: json!({
                "question": question.question,
                "key": question.key,
                "selected_offers": offers,
            }),
        })
    }

    fn continuations_after<'a>(
        &'a self,
        citations: &'a Citations,
        at: u64,
    ) -> Option<Vec<Continued<'a>>> {
        let mut later = citations
            .lines
            .iter()
            .filter(|(_, line)| line.at > at)
            .map(|(cite, line)| {
                let receipt = self.continuations.get(cite)?;
                (receipt.request == self.request
                    && receipt.witness == Witness::of(line.text.as_bytes()).0)
                    .then_some((cite.as_str(), line, receipt))
            })
            .collect::<Option<Vec<_>>>()?;
        later.sort_by_key(|(_, line, _)| line.at);
        Some(later)
    }

    pub(super) fn preparation_continues(&self, citations: &Citations, at: u64) -> bool {
        self.continuations_after(citations, at)
            .is_some_and(|later| later.iter().all(|(_, line, _)| line.at <= self.written_at))
    }

    /// Whether the person's words at `at` still mandate the proposal of revision `number`,
    /// asking `wanted` with `scope`: the one predicate for both of its gates. Every later line of
    /// theirs is an exact bound answer, and the candidate was written after those answers (after
    /// the proposal shown, when one was). Without such a rewrite, only the very bytes that
    /// proposal showed are proposed again, with its scope, on the words it cited and within the
    /// acts it asked, once the person answered each choice it showed, and nothing else, with the
    /// default those bytes carry.
    pub(super) fn mandate_continues(
        &self,
        citations: &Citations,
        at: u64,
        (number, scope, wanted): (u64, &str, Acts),
    ) -> bool {
        let Some(shown) = &self.shown else {
            return at <= self.written_at && self.preparation_continues(citations, at);
        };
        if at > shown.at {
            return false;
        }
        if self.written_at > shown.at {
            return self.preparation_continues(citations, at);
        }
        let Some(later) = self.continuations_after(citations, at) else {
            return false;
        };
        let picks: Vec<&str> = (later.iter())
            .filter(|(_, line, _)| line.at > shown.at)
            .map(|(cite, _, _)| *cite)
            .collect();
        let bytes = (self.candidate.as_ref()).map(|c| Witness::of(c.source.as_bytes()).0);
        let within = |asked: Acts| (asked.save || !wanted.save) && (asked.run || !wanted.run);
        let defaulted = |id: &QuestionId, cite: &str| self.picked_default(id, cite);
        shown.number == number
            && bytes.as_deref() == Some(shown.witness.as_str())
            && shown.scope == scope
            && (shown.mandate).is_some_and(|(line, asked)| line == at && within(asked))
            && !shown.choices.is_empty()
            && self.picked_other.is_empty()
            && (later.iter()).all(|(_, line, _)| line.at > shown.at || line.at <= self.written_at)
            && (picks.iter()).all(|&cite| shown.choices.iter().any(|id| defaulted(id, cite)))
            && (shown.choices.iter()).all(|id| picks.iter().any(|&cite| defaulted(id, cite)))
    }

    /// Whether the person's line `cite` answered the shown choice `id` with the default it
    /// carries: an exact pick of its one recommended offer, each of whose values that pick bound
    /// and the candidate holds.
    fn picked_default(&self, id: &QuestionId, cite: &str) -> bool {
        let source = (self.candidate.as_ref()).map_or("", |c| c.source.as_str());
        (self.answered.values()).any(|one| {
            let key = Some(one.question.key.as_str());
            let bound = |value: &OfferValue| {
                (self.bindings.iter()).any(|b| {
                    b.key.as_deref() == key
                        && b.role == value.role
                        && same_literal(&b.value, &value.value)
                })
            };
            one.question.id == *id
                && one.cite == cite
                && answer::recommended(&one.question).is_some_and(|default| {
                    matches!(&one.pick, Pick::Offer(picked) if *picked == default.key)
                        && (default.values.iter())
                            .all(|value| source.contains(bare(&value.value)) && bound(value))
                })
        })
    }

    /// A bound answer continues the most recent human instruction, not a new act. Every
    /// later human line must have exact continuation evidence before that origin can be used
    /// by the consent reader.
    fn authorization_origin<'a>(
        &self,
        citations: &'a Citations,
        message: &str,
    ) -> Option<(&'a str, &'a Cited)> {
        let (cite, line) = citations.lines.get_key_value(message)?;
        if !self.continuations.contains_key(cite) {
            return Some((cite.as_str(), line));
        }
        let (origin, instruction) = citations
            .lines
            .iter()
            .filter(|(cite, entry)| {
                entry.at >= self.since
                    && entry.at < line.at
                    && !self.continuations.contains_key(*cite)
            })
            .max_by_key(|(_, entry)| entry.at)?;
        let later = self.continuations_after(citations, instruction.at)?;
        later
            .iter()
            .any(|(choice, _, _)| *choice == message)
            .then_some((origin.as_str(), instruction))
    }

    /// Validate the author's exact citation before resolving any bound-choice continuation.
    pub(super) fn authorization_line<'a>(
        &self,
        citations: &'a Citations,
        said: &Value,
    ) -> Option<(&'a str, &'a Cited)> {
        let message = said["message"].as_str()?;
        let excerpt = said["excerpt"].as_str()?;
        if !in_line(citations.get(message)?, excerpt, None) {
            return None;
        }
        self.authorization_origin(citations, message)
    }

    /// The cited human instruction, plus the exact later choices that continued it. The
    /// Session independently reads this whole record for consent: a later selected offer may
    /// narrow or withdraw the earlier request. Questions and unselected options grant nothing.
    /// A citation of a bound choice includes the original current request as well. This
    /// presentation grants nothing and does not bypass proposal or consumption checks.
    /// `None` means some later human line was not such a bound protocol answer.
    #[must_use]
    pub fn authorization_text(&self, citations: &Citations, message: &str) -> Option<String> {
        let (message, original) = self.authorization_origin(citations, message)?;
        let later = self.continuations_after(citations, original.at)?;
        if later.is_empty() {
            return Some(original.text.clone());
        }
        let choices = later
            .into_iter()
            .map(|(cite, line, receipt)| {
                json!({"human_citation": cite, "human_reply": line.text,
                    "selected_meaning": receipt.meaning})
            })
            .collect::<Vec<_>>();
        Some(format!(
            "Human instruction ({message}): {}\n\
             Later human choices, in order (these can narrow or withdraw that instruction; \
             only the selected offers were chosen, and the questions authorize nothing): {}",
            json!(original.text),
            json!(choices),
        ))
    }
}

#[cfg(test)]
mod tests;
