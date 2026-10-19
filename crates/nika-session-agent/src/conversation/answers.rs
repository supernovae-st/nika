// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A value such as `true` does not identify the checkbox that bound it. The judge receives
//! the selected offer's description and provenance, with exact bindings when it has values.
//! The person's own words for a question Nika asked, on the line read as their answer, keep that
//! question's description with no option selected (a free entry). Each description carries its
//! line as typed (`human_reply`): the laws read that line as the answer's, never as the request's.
//! These descriptions are evidence, not live questions, human words or execution authority; a
//! picked offer's label they keep still tells a schedule what a line picked after a reopen.

use super::{
    AskedQuestion, BTreeMap, Binding, Citations, Conversation, Offer, Value, json, same_literal,
};
use nika_session_change::work::{Provenance, ProvenanceKind, ValueRole};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct AnswerMeaning {
    question: String,
    citation: String,
    selected: Vec<DescribedOffer>,
    #[serde(default)]
    selection_kind: Option<ProvenanceKind>,
    /// The role the question asked, for a free entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    role: Option<ValueRole>,
    /// The person's line the answer cites, as typed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply: Option<String>,
    /// The question that recorded the line, when the answer is bound under another key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recorded_for: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DescribedOffer {
    key: String,
    label: String,
    values: Vec<DescribedValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DescribedValue {
    role: ValueRole,
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

impl Conversation {
    /// The labels the kept record of each answer the person picked offers by (one offer, several,
    /// or a default their consent kept), with the line that picked them, for the questions this
    /// conversation holds no live answer of: what a pick before a reopen said. A choice left to
    /// Nika or a free entry says none.
    pub(super) fn kept_picks(&self) -> impl Iterator<Item = (&str, &str)> {
        (self.answer_meanings.iter())
            .filter(|(key, meaning)| {
                !self.answered.contains_key(*key)
                    && meaning.selection_kind == Some(ProvenanceKind::Offered)
            })
            .flat_map(|(_, meaning)| {
                let cite = meaning.citation.as_str();
                (meaning.selected.iter()).map(move |offer| (cite, offer.label.as_str()))
            })
    }

    /// Called only after `take` validated and bound this selection.
    pub(super) fn remember_answer(
        &mut self,
        question: &AskedQuestion,
        (citation, reply): (&str, &str),
        selected: &[&Offer],
        kind: ProvenanceKind,
    ) {
        self.answer_meanings.insert(
            question.key.clone(),
            AnswerMeaning {
                question: question.question.clone(),
                citation: citation.to_owned(),
                selection_kind: Some(kind),
                role: None,
                reply: Some(reply.to_owned()),
                recorded_for: None,
                selected: selected
                    .iter()
                    .map(|offer| DescribedOffer {
                        key: offer.key.clone(),
                        label: offer.label.clone(),
                        values: offer
                            .values
                            .iter()
                            .map(|value| DescribedValue {
                                role: value.role,
                                value: value.value.clone(),
                                name: value.name.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            },
        );
    }

    /// Called only after `bind_answer` bound, under `key`, the person's own words for `question`,
    /// the question Nika asked that recorded their line `citation` (its text, `reply`): a free
    /// entry, no option picked, that keeps that question even under another key.
    pub(super) fn remember_typed(
        &mut self,
        key: &str,
        question: &AskedQuestion,
        (citation, reply): (&str, &str),
    ) {
        self.answer_meanings.insert(
            key.to_owned(),
            AnswerMeaning {
                question: question.question.clone(),
                citation: citation.to_owned(),
                selected: Vec::new(),
                selection_kind: Some(ProvenanceKind::Answered),
                role: question.role,
                reply: Some(reply.to_owned()),
                recorded_for: (question.key != key).then(|| question.key.clone()),
            },
        );
    }

    /// The person's answers by question, as JSON literals for the document's judges. A
    /// choice keeps its exact bound values and the descriptions of the selected options.
    /// The descriptions came from the author; the selection receipt and citation attest the
    /// scoped pick, and bindings attest any business values it introduced. A free entry keeps
    /// its question's key, words, citation and role, with no option selected. Every description
    /// carries its line as typed. Any other typed value retains its literal shape. Nothing here
    /// grants Save or Run.
    #[must_use]
    pub fn answers(&self) -> BTreeMap<String, String> {
        let mut by_key: BTreeMap<String, Vec<&Binding>> = BTreeMap::new();
        for binding in &self.bindings {
            if let Some(key) = &binding.key {
                by_key.entry(key.clone()).or_default().push(binding);
            }
        }
        // A valid choice need not introduce a business literal. Its receipt still tells the
        // judge which described alternative the person picked; no value is synthesized.
        for key in self.answer_meanings.keys() {
            by_key.entry(key.clone()).or_default();
        }
        by_key
            .into_iter()
            .filter_map(|(key, bindings)| {
                let values = bindings
                    .iter()
                    .map(|b| b.value.as_str())
                    .collect::<Vec<_>>();
                let literal = match values.as_slice() {
                    [one] => json!(one),
                    _ => json!(values),
                };
                let described = self
                    .answer_meanings
                    .get(&key)
                    .and_then(|meaning| meaning.described(&key, &bindings, &literal));
                if bindings.is_empty() && described.is_none() {
                    return None;
                }
                Some((key, described.unwrap_or(literal).to_string()))
            })
            .collect()
    }
}

impl Conversation {
    /// [`Self::answers`], each description also locating the line it cites in the person's words
    /// from the request's start ([`Citations::stated`]): `choice_context.human_lines`, its first
    /// line and the one after its last, found by that line's citation, never by its text.
    #[must_use]
    pub fn answers_in(&self, citations: &Citations) -> BTreeMap<String, String> {
        let mut answers = self.answers();
        // A free yes that bound nothing (« Oui » to « Veux-tu valider chaque envoi avant qu'il
        // parte ? ») still states the approval its question asks for: the judges read it with the
        // question, its line and no option or value, so the gate it approved is the person's.
        // Any other unbound reply stays its located line (`replies_in`). It binds, settles and
        // authorizes nothing.
        for (key, answered) in &self.answered {
            if answers.contains_key(key) {
                continue;
            }
            let Some(line) = citations.get(&answered.cite) else {
                continue;
            };
            if !nika_compile_fidelity::fidelity::approved_by_reply(
                &answered.question.question,
                &line.text,
            ) {
                continue;
            }
            let context = json!({
                "question_key": key,
                "question": answered.question.question,
                "human_citation": answered.cite,
                "selected_options": [],
                "human_reply": line.text,
            });
            answers.insert(
                key.clone(),
                json!({ "choice_context": context }).to_string(),
            );
        }
        for raw in answers.values_mut() {
            let Ok(mut answer) = serde_json::from_str::<Value>(raw) else {
                continue;
            };
            let cite = answer["choice_context"]["human_citation"].as_str();
            let Some((start, end)) = cite.and_then(|cite| citations.located(self.since, cite))
            else {
                continue;
            };
            answer["choice_context"]["human_lines"] = json!([start, end]);
            *raw = answer.to_string();
        }
        answers
    }
}

impl AnswerMeaning {
    fn described(&self, key: &str, bindings: &[&Binding], values: &Value) -> Option<Value> {
        if self.selection_kind == Some(ProvenanceKind::Answered) {
            return self.typed(key, bindings, values);
        }
        let selected = self
            .selected
            .iter()
            .filter(|offer| {
                if offer.values.is_empty() {
                    matches!(
                        self.selection_kind,
                        Some(ProvenanceKind::Offered | ProvenanceKind::Delegated)
                    )
                } else {
                    offer.values.iter().all(|value| {
                        bindings.iter().any(|binding| {
                            binding.role == value.role
                                && same_literal(&binding.value, &value.value)
                                && binding.provenance.message == self.citation
                                && binding.provenance.question.as_deref() == Some(key)
                                && binding.provenance.option.as_deref() == Some(offer.key.as_str())
                        })
                    })
                }
            })
            .map(|offer| {
                let mut selected =
                    json!({"key": offer.key, "label": offer.label, "values": offer.values});
                if let Some(kind @ (ProvenanceKind::Offered | ProvenanceKind::Delegated)) =
                    self.selection_kind
                {
                    selected["provenance"] = json!(
                        Provenance::new(kind, &self.citation)
                            .with_offer(key, Some(offer.key.clone()))
                    );
                } else if let Some(binding) = bindings.iter().find(|binding| {
                    binding.provenance.message == self.citation
                        && binding.provenance.question.as_deref() == Some(key)
                        && binding.provenance.option.as_deref() == Some(offer.key.as_str())
                }) {
                    // Older value-bearing receipts still have the exact host binding.
                    selected["provenance"] = json!(binding.provenance);
                }
                selected
            })
            .collect::<Vec<_>>();
        (!selected.is_empty()).then(|| {
            let mut context = json!({
                "question_key": key,
                "question": self.question,
                "human_citation": self.citation,
                "selected_options": selected,
            });
            self.reply_into(&mut context);
            json!({"bound_values": values, "bindings": bindings, "choice_context": context})
        })
    }

    /// A free entry: the person's own words for the question, no option picked, described while
    /// a binding of it still rests on the line that replied, as typed.
    fn typed(&self, key: &str, bindings: &[&Binding], values: &Value) -> Option<Value> {
        let held = bindings.iter().any(|binding| {
            binding.provenance.kind == ProvenanceKind::Answered
                && binding.provenance.message == self.citation
                && binding.provenance.option.is_none()
        });
        held.then(|| {
            let mut context = json!({
                "question_key": self.recorded_for.as_deref().unwrap_or(key),
                "question": self.question,
                "human_citation": self.citation,
                "selected_options": [],
            });
            if let Some(role) = self.role {
                context["role"] = json!(role);
            }
            self.reply_into(&mut context);
            json!({"bound_values": values, "bindings": bindings, "choice_context": context})
        })
    }

    /// The person's line beside its citation, as typed, when the meaning kept it.
    fn reply_into(&self, context: &mut Value) {
        if let Some(reply) = &self.reply {
            context["human_reply"] = json!(reply);
        }
    }
}

#[cfg(test)]
mod tests;
