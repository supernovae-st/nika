// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A value such as `true` does not identify the checkbox that bound it. The judge receives
//! the selected offer's description and provenance, with exact bindings when it has values.
//! These descriptions are evidence, not live questions, human words or execution authority.

use super::{AskedQuestion, BTreeMap, Binding, Conversation, Offer, Value, json, same_literal};
use nika_session_change::work::{Provenance, ProvenanceKind, ValueRole};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct AnswerMeaning {
    question: String,
    citation: String,
    selected: Vec<DescribedOffer>,
    #[serde(default)]
    selection_kind: Option<ProvenanceKind>,
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
    /// Called only after `take` validated and bound this selection.
    pub(super) fn remember_answer(
        &mut self,
        question: &AskedQuestion,
        citation: &str,
        selected: &[&Offer],
        kind: ProvenanceKind,
    ) {
        self.answer_meanings.insert(
            question.key.clone(),
            AnswerMeaning {
                question: question.question.clone(),
                citation: citation.to_owned(),
                selection_kind: Some(kind),
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

    /// The person's answers by question, as JSON literals for the document's judges. A
    /// choice keeps its exact bound values and the descriptions of the selected options.
    /// The descriptions came from the author; the selection receipt and citation attest the
    /// scoped pick, and bindings attest any business values it introduced.
    /// Simple typed answers retain their literal shape. Nothing here grants Save or Run.
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

impl AnswerMeaning {
    fn described(&self, key: &str, bindings: &[&Binding], values: &Value) -> Option<Value> {
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
            json!({
                "bound_values": values,
                "bindings": bindings,
                "choice_context": {
                    "question_key": key,
                    "question": self.question,
                    "human_citation": self.citation,
                    "selected_options": selected,
                },
            })
        })
    }
}

#[cfg(test)]
mod tests;
