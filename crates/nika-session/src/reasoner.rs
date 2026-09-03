// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reasoners — ONE inference over the selected intelligence, never a
//! temporary workflow: a harness seat (an AI app the human already has,
//! through the same infer-grade adapter `nika run` uses), an API or a
//! local engine (through the same provider registry and the same one-shot
//! infer verb), a scripted reasoner (the tests' stand-in, which records
//! exactly what it was given), or none.

use std::collections::VecDeque;
use std::sync::Arc;

/// Why a reasoner could not answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReasonError {
    /// No conversational intelligence was chosen.
    #[error("no conversational intelligence — the facts stay (`/intelligence` chooses a path)")]
    NoIntelligence,
    /// The seat could not serve the turn.
    #[error("the seat could not answer: {0}")]
    Seat(String),
    /// The provider could not serve the turn.
    #[error("the provider could not answer: {0}")]
    Provider(String),
    /// The async runtime could not start.
    #[error("the session's runtime could not start: {0}")]
    Runtime(String),
}

/// One reply.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Reply {
    /// The text as the reasoner produced it (the guard reads it next).
    pub text: String,
    /// The path reported its usage (a seat may not).
    pub usage_observed: bool,
}

/// A reasoner: a name and one turn.
pub trait SessionReasoner {
    /// The path's name for the banner (`codex` · `mistral API` · `none`).
    fn name(&self) -> String;

    /// One turn over the broker's prompt.
    ///
    /// # Errors
    ///
    /// When the path cannot answer — the runtime refuses the turn with
    /// the reason, never silently switches paths.
    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError>;
}

/// The tests' reasoner: canned replies, and a record of every prompt it
/// received — the proof that only the bundle reaches a model.
#[derive(Debug)]
pub struct ScriptedReasoner {
    replies: VecDeque<String>,
    /// Every prompt this reasoner was handed, in order.
    pub seen: Vec<String>,
}

impl ScriptedReasoner {
    /// A reasoner that answers with `replies` in order, then repeats the last.
    #[must_use]
    pub fn new(replies: Vec<String>) -> Self {
        Self {
            replies: replies.into(),
            seen: Vec::new(),
        }
    }
}

impl SessionReasoner for ScriptedReasoner {
    fn name(&self) -> String {
        "scripted".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.seen.push(prompt.to_owned());
        let text = if self.replies.len() > 1 {
            self.replies.pop_front().unwrap_or_default()
        } else {
            self.replies.front().cloned().unwrap_or_default()
        };
        Ok(Reply {
            text,
            usage_observed: false,
        })
    }
}

/// No conversational intelligence: every free-text turn is refused with
/// the fact that the facts stay.
#[derive(Debug)]
pub struct NoReasoner;

impl SessionReasoner for NoReasoner {
    fn name(&self) -> String {
        "none".to_owned()
    }

    fn reason(&mut self, _prompt: &str) -> Result<Reply, ReasonError> {
        Err(ReasonError::NoIntelligence)
    }
}

/// A harness seat (an AI app the human already has) — the SAME
/// infer-grade adapter `nika run` dispatches an `infer:` through.
#[cfg(feature = "access-harness")]
#[derive(Debug)]
pub struct HarnessReasoner {
    /// The seat id.
    pub seat: String,
}

#[cfg(feature = "access-harness")]
impl SessionReasoner for HarnessReasoner {
    fn name(&self) -> String {
        self.seat.clone()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        let seat =
            nika_harness::meet_infer_grade(&self.seat, nika_harness::StructuredOutputGrade::Text)
                .map_err(|e| ReasonError::Seat(e.to_string()))?;
        let request = nika_harness::HarnessInferRequest::new(prompt, "session");
        let outcome = block_on(async { seat.run(request).await })?
            .map_err(|e| ReasonError::Seat(e.to_string()))?;
        Ok(Reply {
            text: outcome.output,
            usage_observed: outcome.usage_observed,
        })
    }
}

/// An API or a local engine — the SAME provider registry and the SAME
/// one-shot infer verb a workflow's `infer:` rides, over the ONE env
/// boundary (`config_from_env`).
#[derive(Debug)]
pub struct ProviderReasoner {
    /// The `<provider>/<model>` the turn asks for.
    pub model: String,
    /// The banner's word for the path (`mistral API` · `ollama · local`).
    pub label: String,
}

impl SessionReasoner for ProviderReasoner {
    fn name(&self) -> String {
        self.label.clone()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        let http =
            nika_http::ReqwestHttp::new().map_err(|e| ReasonError::Provider(e.to_string()))?;
        let registry = Arc::new(nika_providers::ProviderRegistry::new(
            Arc::new(http),
            nika_runtime::compose::config_from_env(),
        ));
        let verb = nika_verb_infer::InferVerb::new(registry, self.model.clone());
        let input = nika_verb_infer::InferInput::new(prompt);
        let out = block_on(async { verb.run(input).await })?
            .map_err(|e| ReasonError::Provider(e.to_string()))?;
        Ok(Reply {
            text: infer_text(&out.output),
            usage_observed: true,
        })
    }
}

/// The text of an infer output — the text as is, a structured answer as JSON.
fn infer_text(value: &nika_verb_infer::InferValue) -> String {
    match value {
        nika_verb_infer::InferValue::Text(s) => s.clone(),
        nika_verb_infer::InferValue::Structured(v) => v.to_string(),
        _ => String::new(),
    }
}

/// Block on one future from the session's synchronous loop.
fn block_on<F: std::future::Future>(fut: F) -> Result<F::Output, ReasonError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| ReasonError::Runtime(e.to_string()))?;
    Ok(runtime.block_on(fut))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// The scripted reasoner records what it saw and repeats its last
    /// reply; the empty reasoner refuses with the facts-stay reason.
    #[test]
    fn the_scripted_reasoner_records_and_the_empty_one_refuses() {
        let mut s = ScriptedReasoner::new(vec!["one".to_owned(), "two".to_owned()]);
        assert_eq!(s.reason("p1").expect("ok").text, "one");
        assert_eq!(s.reason("p2").expect("ok").text, "two");
        assert_eq!(
            s.reason("p3").expect("ok").text,
            "two",
            "the last reply repeats"
        );
        assert_eq!(s.seen, vec!["p1", "p2", "p3"]);
        let mut none = NoReasoner;
        assert!(matches!(none.reason("x"), Err(ReasonError::NoIntelligence)));
        assert_eq!(none.name(), "none");
    }
}
