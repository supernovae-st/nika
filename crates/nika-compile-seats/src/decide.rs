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

use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Mutex};

use nika_compile::AuthoringReasoning;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderInferDyn, ResponseFormat, Role,
    StopReason,
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
    /// The call's reasoning, each fact apart (configured, transmitted, served, reasoning tokens,
    /// response model), when the seat observed one (R4 B16).
    pub reasoning: Option<Value>,
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
            reasoning: None,
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

mod batch;
pub use batch::{
    BatchFuture, BatchItem, Carried, ChoiceBatch, closed_choices, decoded_each, each_alone,
};

/// A bounded decision capability. Vendor-neutral by construction.
pub trait DecisionSeat: Send + Sync {
    /// The requested seat identity (`provider/model`), for provenance.
    fn name(&self) -> &str;
    /// Exactly one physical request per question; no hidden retry.
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a>;
    /// Several independent questions (A1): by default each asked alone, all at once, one
    /// physical request per question and no retry; a seat that settles a batch in one request
    /// answers it so. One answer per item, in item order, each bound to its item by id.
    fn choose_each<'a>(&'a self, batch: &'a ChoiceBatch) -> BatchFuture<'a> {
        each_alone(batch, |question| self.choose(question))
    }
}

/// A generative provider seated as a closed-choice decider through a JSON-schema enum.
pub struct ProviderChoice<'p, P: ProviderInferDyn> {
    provider: &'p P,
    model: String,
    timeout: std::time::Duration,
    reasoning: Option<AuthoringReasoning>,
    max_tokens: u32,
    /// One receipt per physical request, written before it can leave ([`Self::requests`]).
    sent: Mutex<Vec<Value>>,
}

impl<'p, P: ProviderInferDyn> ProviderChoice<'p, P> {
    /// Seat `model` (`provider/name`) behind an injected kernel provider, asking its route's
    /// output capacity: a closed choice from a reasoning model may think before it answers.
    #[must_use]
    pub fn new(
        provider: &'p P,
        model: impl Into<String>,
        timeout: std::time::Duration,
        max_tokens: u32,
    ) -> Self {
        Self {
            provider,
            model: model.into(),
            timeout,
            reasoning: None,
            max_tokens,
            sent: Mutex::new(Vec::new()),
        }
    }

    /// Ask the decision call for this reasoning effort under the operator's declared authoring
    /// output cap (R4 B16): the level is sent only where its route qualifies it, and the cap is
    /// the authoring calls' own. Without it the choice keeps its route's default level.
    #[must_use]
    pub fn with_reasoning(mut self, reasoning: AuthoringReasoning, max_tokens: u32) -> Self {
        self.reasoning = Some(reasoning);
        self.max_tokens = max_tokens;
        self
    }

    /// The physical requests this seat sent, one receipt each, apart from the questions they
    /// answered: the question ids a request carried, how it ended (`in_flight` when it never
    /// settled · `answered` · `error` · `timed_out`) and the usage its response reported, once per
    /// request whatever its questions came to (`null` when unreported or no response came).
    #[must_use]
    pub fn requests(&self) -> Vec<Value> {
        match self.sent.lock() {
            Ok(sent) => sent.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

impl<P: ProviderInferDyn> ProviderChoice<'_, P> {
    /// The request this seat sends, its messages and schema given: its model, output bound,
    /// deadline and reasoning level.
    fn request(
        &self,
        (messages, schema): (Vec<Message>, Value),
    ) -> Result<InferRequest, DecisionError> {
        let mut infer = InferRequest::new(&self.model, messages);
        infer.max_tokens = Some(self.max_tokens);
        infer.timeout = Some(self.timeout);
        if let Some(reasoning) = self.reasoning {
            infer.reasoning_effort =
                Some(crate::reasoning::effort(reasoning).ok_or_else(|| {
                    DecisionError("the reasoning effort has no provider level".to_owned())
                })?);
        }
        infer.response_format = ResponseFormat::JsonSchema(schema);
        Ok(infer)
    }

    /// The one call of a request carrying `questions`, under the seat's own deadline; no retry.
    /// Its receipt is written before it can leave and settled with the usage its response
    /// reported, so a reply that decides nothing still accounts for what it used.
    async fn call(
        &self,
        infer: InferRequest,
        questions: Vec<String>,
    ) -> Result<InferResponse, DecisionError> {
        let slot = self.sent.lock().ok().map(|mut sent| {
            sent.push(json!({"questions": questions, "outcome": "in_flight", "usage": null}));
            sent.len() - 1
        });
        let result = tokio::time::timeout(self.timeout, self.provider.infer(infer)).await;
        let (outcome, usage, error) = match &result {
            Ok(Ok(response)) => {
                let usage = response.usage_reported.then(|| {
                    json!({"input_tokens": response.usage.input_tokens,
                        "output_tokens": response.usage.output_tokens})
                });
                ("answered", usage, None)
            }
            Ok(Err(error)) => ("error", None, Some(error.to_string())),
            Err(_) => ("timed_out", None, None),
        };
        if let (Some(slot), Ok(mut sent)) = (slot, self.sent.lock())
            && let Some(receipt) = sent.get_mut(slot)
        {
            receipt["outcome"] = json!(outcome);
            receipt["usage"] = json!(usage);
            if let Some(error) = error {
                receipt["error"] = json!(error);
            }
        }
        result
            .map_err(|_| DecisionError("the single decision call timed out; no retry".to_owned()))?
            .map_err(|e| DecisionError(e.to_string()))
    }

    /// The answer a response makes of one choice: the response's usage and reasoning ride the
    /// answer only when `usage` (the first answered item of a batch carries the request's).
    fn answer(&self, choice: String, response: &InferResponse, usage: bool) -> ChoiceAnswer {
        let mut probabilities = BTreeMap::new();
        probabilities.insert(choice.clone(), 1.0);
        let reported = |tokens: u64| (usage && response.usage_reported).then_some(tokens);
        ChoiceAnswer {
            choice,
            probabilities,
            confidence: None,
            model: self.model.clone(),
            input_tokens: reported(response.usage.input_tokens),
            output_tokens: reported(response.usage.output_tokens),
            reasoning: usage
                .then(|| crate::reasoning::reasoning_record(self.reasoning, Some(response))),
        }
    }
}

impl<P: ProviderInferDyn> DecisionSeat for ProviderChoice<'_, P> {
    fn name(&self) -> &str {
        &self.model
    }

    /// A batch settled in ONE request: the shared instructions and state once, each item's own
    /// words, state and options, an answer mapping each item id to one of its keys. An item the
    /// answer leaves undecided fails alone; a request that fails fails every item it carried.
    /// Nothing is sent for an empty batch, nor for an id the batch asks twice (an answer keyed
    /// by id could not tell them apart); the request's usage rides its first answered item.
    fn choose_each<'a>(&'a self, batch: &'a ChoiceBatch) -> BatchFuture<'a> {
        Box::pin(async move {
            let repeated = batch.repeated();
            let mut answers: Vec<Result<ChoiceAnswer, DecisionError>> = (batch.items.iter())
                .map(|item| Err(batch::unsent(&item.question.id)))
                .collect();
            let sent: Vec<usize> = (0..batch.items.len())
                .filter(|at| !repeated.contains(batch.items[*at].question.id.as_str()))
                .collect();
            if sent.is_empty() {
                return answers;
            }
            let asked = batch::only(batch, &sent);
            let ids = (asked.items.iter()).map(|item| item.question.id.clone());
            let response = match self.request(closed_choices(&asked)) {
                Ok(infer) => self.call(infer, ids.collect()).await,
                Err(error) => Err(error),
            };
            let decoded = response.and_then(|response| {
                batch::decoded_items(&asked, &response).map(|keys| (response, keys))
            });
            match decoded {
                Ok((response, keys)) => {
                    let mut usage = true;
                    for (at, key) in sent.into_iter().zip(keys) {
                        answers[at] = key.map(|key| {
                            let answer = self.answer(key, &response, usage);
                            usage = false;
                            answer
                        });
                    }
                }
                Err(error) => (sent.into_iter()).for_each(|at| answers[at] = Err(error.clone())),
            }
            answers
        })
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            let infer = self.request(closed_choice(question))?;
            let response = self.call(infer, vec![question.id.clone()]).await?;
            let choice = decoded(question, &response)?;
            Ok(self.answer(choice, &response, true))
        })
    }
}

/// The two messages and the answer schema of one closed choice, as a provider seat asks it
/// (R4 A11: the verifier journals the same bytes through the authoring call).
#[must_use]
pub fn closed_choice(question: &ChoiceQuestion) -> (Vec<Message>, Value) {
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
    let schema = json!({
        "type": "object", "additionalProperties": false, "required": ["choice"],
        "properties": {"choice": {"type": "string", "enum": question.keys()}}
    });
    (
        vec![
            Message::text(Role::System, system),
            Message::text(Role::User, user),
        ],
        schema,
    )
}

/// The sole final Text of a completed answer; separate Thinking is never answer material.
/// All other block kinds and multiple Text blocks refuse the projection without changing the
/// response, its observation or its usage. An empty Text still has to pass the caller's decoder.
#[must_use]
pub fn answer_text(response: &nika_kernel::ai::provider::InferResponse) -> Option<&str> {
    if response.stop_reason != StopReason::EndTurn {
        return None;
    }
    let mut blocks = response
        .content
        .iter()
        .filter(|block| !matches!(block, ContentBlock::Thinking { .. }));
    match (blocks.next(), blocks.next()) {
        (Some(ContentBlock::Text { text }), None) => Some(text),
        _ => None,
    }
}

#[cfg(test)]
mod answer_tests;

/// The option a provider's answer to `question` chooses: one complete JSON text naming an
/// offered key, or why it does not.
///
/// # Errors
/// A [`DecisionError`] when the answer is not one complete JSON text, names no choice, or names
/// a key the question did not offer.
pub fn decoded(
    question: &ChoiceQuestion,
    response: &nika_kernel::ai::provider::InferResponse,
) -> Result<String, DecisionError> {
    let text = answer_text(response).ok_or_else(|| {
        DecisionError("the seat did not return one complete JSON text".to_owned())
    })?;
    let value: Value = serde_json::from_str(text)
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
    Ok(choice)
}

/// Revalidate an answer against the question it claims to answer.
///
/// # Errors
/// A [`DecisionError`] when the seat chose a key outside the offered options, or reported a
/// distribution over keys the question did not offer.
pub fn admit(question: &ChoiceQuestion, answer: &ChoiceAnswer) -> Result<(), DecisionError> {
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
#[must_use]
pub fn record(question: &ChoiceQuestion, answer: Result<&ChoiceAnswer, &DecisionError>) -> Value {
    match answer {
        Ok(answer) => json!({
            "question": question.id, "options": question.keys(), "choice": answer.choice,
            "probabilities": answer.probabilities, "confidence": answer.confidence,
            "model": answer.model, "input_tokens": answer.input_tokens, "output_tokens": answer.output_tokens,
            "reasoning": answer.reasoning,
        }),
        Err(error) => {
            json!({"question": question.id, "options": question.keys(), "error": error.0})
        }
    }
}
