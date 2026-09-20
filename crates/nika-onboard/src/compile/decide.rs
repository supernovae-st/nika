// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bounded decision seats (the WARM strategy): pick one admissible option or NONE.
//!
//! A seat answers exactly one question: among options Nika already found
//! admissible, which one fits this clause? It cannot add an option, invent a
//! primitive, grant authority, remove a gate or bypass Check. Its answer is
//! revalidated (the key must be one of the offered options) before assembly.
//! The compiler asks for the CAPABILITY (a closed choice with NONE); the host
//! decides which vendor seats it: a `TypeSafe` System One request, a generative
//! provider constrained to a closed enum, or a hermetic double in tests.

use std::{collections::BTreeMap, future::Future, pin::Pin};

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, Message, ProviderInferDyn, ResponseFormat, Role, StopReason,
};
use serde_json::{Value, json};

/// The reject-all option every closed choice carries.
pub const NONE_OPTION: &str = "none";

/// One admissible option, described for the seat.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChoiceOption {
    /// Stable key returned by the seat.
    pub key: String,
    /// What choosing it means; never source, permits or credentials.
    pub description: String,
}

/// One closed choice. `options` always includes [`NONE_OPTION`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChoiceQuestion {
    /// Stable question id (one per ambiguous clause).
    pub id: String,
    /// What the seat must decide.
    pub instructions: String,
    /// The bounded state the seat may read: intent, clause, definitions.
    pub state: Value,
    /// Admissible options plus NONE.
    pub options: Vec<ChoiceOption>,
}

impl ChoiceQuestion {
    /// Build a closed choice; NONE is appended when absent.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        instructions: impl Into<String>,
        state: Value,
        mut options: Vec<ChoiceOption>,
    ) -> Self {
        if !options.iter().any(|o| o.key == NONE_OPTION) {
            options.push(ChoiceOption {
                key: NONE_OPTION.to_owned(),
                description: "none of the options fits this clause".to_owned(),
            });
        }
        Self {
            id: id.into(),
            instructions: instructions.into(),
            state,
            options,
        }
    }
    /// The option keys, in offered order.
    #[must_use]
    pub fn keys(&self) -> Vec<String> {
        self.options.iter().map(|o| o.key.clone()).collect()
    }
}

/// A seat's answer. Probabilities are the seat's own report, never authority.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ChoiceAnswer {
    /// The selected option key; must be one of the offered keys.
    pub choice: String,
    /// Reported distribution over the offered keys, when the seat returns one.
    pub probabilities: BTreeMap<String, f64>,
    /// Seat-specific concentration statistic, when returned. Not a probability of correctness.
    pub confidence: Option<f64>,
    /// The model identity the seat reported.
    pub model: String,
    /// Reported input tokens or billing units.
    pub input_tokens: Option<u64>,
    /// Reported output tokens.
    pub output_tokens: Option<u64>,
}

impl ChoiceOption {
    /// One admissible option.
    #[must_use]
    pub fn new(key: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            description: description.into(),
        }
    }
}

impl ChoiceAnswer {
    /// A bare answer; the seat fills the reported fields it actually received.
    #[must_use]
    pub fn new(choice: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            choice: choice.into(),
            probabilities: BTreeMap::new(),
            confidence: None,
            model: model.into(),
            input_tokens: None,
            output_tokens: None,
        }
    }
}

/// A typed seat failure. The compiler records it and falls back or asks; it never retries.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("decision seat failed: {0}")]
pub struct DecisionError(pub String);

/// The object-safe future a seat returns.
pub type ChoiceFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ChoiceAnswer, DecisionError>> + Send + 'a>>;

/// A bounded decision capability. Vendor-neutral by construction.
pub trait DecisionSeat: Send + Sync {
    /// The requested seat identity (`provider/model`), for provenance.
    fn name(&self) -> &str;
    /// Exactly one physical request per question; no hidden retry.
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a>;
}

/// A generative provider seated as a closed-choice decider through a JSON-schema enum.
pub struct ProviderChoice<'p, P: ProviderInferDyn> {
    provider: &'p P,
    model: String,
    timeout: std::time::Duration,
}

impl<'p, P: ProviderInferDyn> ProviderChoice<'p, P> {
    /// Seat `model` (`provider/name`) behind an injected kernel provider.
    #[must_use]
    pub fn new(provider: &'p P, model: impl Into<String>, timeout: std::time::Duration) -> Self {
        Self {
            provider,
            model: model.into(),
            timeout,
        }
    }
}

impl<P: ProviderInferDyn> DecisionSeat for ProviderChoice<'_, P> {
    fn name(&self) -> &str {
        &self.model
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            let keys = question.keys();
            let system = format!(
                "You settle ONE closed choice for a workflow compiler. Read the state and pick exactly one option key. {} Choose \"{NONE_OPTION}\" when no option fits. Return only a JSON object {{\"choice\": <key>}}.",
                question.instructions
            );
            let options: Vec<String> = question
                .options
                .iter()
                .map(|o| format!("- {}: {}", o.key, o.description))
                .collect();
            let user = format!(
                "STATE:\n{}\n\nOPTIONS:\n{}",
                serde_json::to_string_pretty(&question.state).unwrap_or_default(),
                options.join("\n")
            );
            let mut infer = InferRequest::new(
                &self.model,
                vec![
                    Message::text(Role::System, system),
                    Message::text(Role::User, user),
                ],
            );
            infer.max_tokens = Some(256);
            infer.timeout = Some(self.timeout);
            infer.response_format = ResponseFormat::JsonSchema(json!({
                "type": "object", "additionalProperties": false, "required": ["choice"],
                "properties": {"choice": {"type": "string", "enum": keys}}
            }));
            let response = tokio::time::timeout(self.timeout, self.provider.infer(infer))
                .await
                .map_err(|_| {
                    DecisionError("the single decision call timed out; no retry".to_owned())
                })?
                .map_err(|e| DecisionError(e.to_string()))?;
            let text = match response.content.as_slice() {
                [ContentBlock::Text { text }] if response.stop_reason == StopReason::EndTurn => {
                    text.clone()
                }
                _ => {
                    return Err(DecisionError(
                        "the seat did not return one complete JSON text".to_owned(),
                    ));
                }
            };
            let value: Value = serde_json::from_str(&text)
                .map_err(|e| DecisionError(format!("the seat answer is not JSON: {e}")))?;
            let choice = value
                .get("choice")
                .and_then(Value::as_str)
                .ok_or_else(|| DecisionError("the seat answer has no choice".to_owned()))?
                .to_owned();
            if !question.options.iter().any(|o| o.key == choice) {
                return Err(DecisionError(format!(
                    "the seat chose `{choice}`, which was not offered"
                )));
            }
            let mut probabilities = BTreeMap::new();
            probabilities.insert(choice.clone(), 1.0);
            Ok(ChoiceAnswer {
                choice,
                probabilities,
                confidence: None,
                model: self.model.clone(),
                input_tokens: response
                    .usage_reported
                    .then_some(response.usage.input_tokens),
                output_tokens: response
                    .usage_reported
                    .then_some(response.usage.output_tokens),
            })
        })
    }
}

/// Revalidate an answer against the question it claims to answer.
pub(super) fn admit(question: &ChoiceQuestion, answer: &ChoiceAnswer) -> Result<(), DecisionError> {
    if !question.options.iter().any(|o| o.key == answer.choice) {
        return Err(DecisionError(format!(
            "seat chose `{}`, outside the offered options",
            answer.choice
        )));
    }
    if answer
        .probabilities
        .keys()
        .any(|k| !question.options.iter().any(|o| &o.key == k))
    {
        return Err(DecisionError(
            "seat reported a distribution over unknown options".to_owned(),
        ));
    }
    Ok(())
}

/// The provenance projection of one settled question.
pub(super) fn record(
    question: &ChoiceQuestion,
    answer: Result<&ChoiceAnswer, &DecisionError>,
) -> Value {
    match answer {
        Ok(answer) => json!({
            "question": question.id, "options": question.keys(), "choice": answer.choice,
            "probabilities": answer.probabilities, "confidence": answer.confidence,
            "model": answer.model, "input_tokens": answer.input_tokens, "output_tokens": answer.output_tokens,
        }),
        Err(error) => {
            json!({"question": question.id, "options": question.keys(), "error": error.0})
        }
    }
}
