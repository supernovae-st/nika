// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exact answers may continue a preparation request without asking for its mandate again.
//! These receipts keep the selected meaning, not a live question or execution authority.

use super::{
    Citations, Cited, Conversation, Pick, QuestionId, Value, Witness, answer, in_line, json,
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
