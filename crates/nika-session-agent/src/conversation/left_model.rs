// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A run model the person left to Nika (QA gauntlet q04: « carte blanche » on a model switch was
//! asked three times, then the person had to type a model id). The fidelity law binds a run model
//! only from an offer or from the person's own words, never from the author's own choice, so the
//! author offers the models with one recommended, and Nika takes that one for the person with no
//! second question to them.

use nika_session_change::work::{AskedQuestion, Provenance, ProvenanceKind, ValueRole};
use serde_json::{Value, json};

use super::answer::recommended;
use super::choices::NotAsked;
use super::{Citations, Conversation, Facts, Mint, Pick, question_options, role_word};

/// What the author reads when the person left the run model to Nika and no offer stands for it.
pub(super) fn offer_one_model(cite: &str) -> String {
    format!(
        "the person left the run model to Nika with {cite}; a run model is bound only from an offer, never from your own choice: ask again offering the models you recommend, one marked recommended, and Nika takes it for them without asking them again"
    )
}

impl Conversation {
    /// A run-model question asked while the person's delegation of the run model stands: its one
    /// recommended offer is bound for them as delegated, never asked; with none recommended the
    /// author is told to offer one. None for any other question, when no such delegation stands,
    /// once the person's own words chose a model, and when the question reopens the choice on
    /// their words written after the delegation.
    pub(super) fn take_left_model(
        &mut self,
        question: &Value,
        citations: &Citations,
        (call, ordinal): (Option<&str>, usize),
        mint: Mint<'_>,
        facts: Facts<'_>,
    ) -> Option<Value> {
        let key = question["key"].as_str().unwrap_or_default();
        let options = question_options(question, facts);
        let model = question["role"].as_str().and_then(role_word) == Some(ValueRole::RunModel)
            || (options.iter().flat_map(|offer| &offer.values))
                .any(|value| value.role == ValueRole::RunModel);
        let chosen = (self.bindings.iter()).any(|binding| {
            binding.role == ValueRole::RunModel
                && binding.provenance.kind != ProvenanceKind::Delegated
        });
        if !model || chosen {
            return None;
        }
        let (cite, excerpt) = self.model_left(key)?;
        let since = citations.get(&cite).map_or(0, |line| line.at);
        let reopens = question["reopens"]["message"].as_str();
        if reopens
            .and_then(|message| citations.get(message))
            .is_some_and(|line| line.at > since)
        {
            return None;
        }
        let text = question["question"].as_str().unwrap_or_default();
        let context = format!(
            "agent question\n{}\n{}\n{ordinal}\n{key}\n{text}\n{}",
            self.request,
            call.unwrap_or_default(),
            question["options"]
        );
        let asked = AskedQuestion::new(mint(&context), key, text)
            .with_options(options, false, false)
            .with_role(ValueRole::RunModel);
        let reason = NotAsked::Delegated.word();
        let Some(offer) = recommended(&asked).cloned() else {
            return Some(json!({"key": key, "reason": reason, "why": offer_one_model(&cite)}));
        };
        let provenance = Provenance::new(ProvenanceKind::Delegated, &cite)
            .with_excerpt(&excerpt)
            .with_offer(key, Some(offer.key.clone()));
        self.bind_offer(&asked, &offer, &provenance);
        self.remember_answer(
            &asked,
            (&cite, &excerpt),
            &[&offer],
            ProvenanceKind::Delegated,
        );
        let why = format!(
            "the person left the run model to Nika with {cite}; Nika took the recommended `{}` ({}) for them, unasked: say so, and that they can change it",
            offer.key, offer.label
        );
        Some(json!({"key": key, "reason": reason, "why": why, "taken": offer.key}))
    }

    /// The person's line and words that left the run model to Nika: those of this question's own
    /// delegated answer, else the latest delegation of the run model.
    fn model_left(&self, key: &str) -> Option<(String, String)> {
        let own = (self.answered.get(key))
            .filter(|answer| answer.pick == Pick::Delegated)
            .map(|answer| answer.cite.as_str());
        let delegation = (self.delegations.iter().rev()).find(|delegation| {
            own.map_or(delegation.scope == ValueRole::RunModel, |cite| {
                delegation.message == cite
            })
        })?;
        Some((delegation.message.clone(), delegation.excerpt.clone()))
    }
}

#[cfg(test)]
mod tests;
