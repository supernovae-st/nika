// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reasoners — ONE inference over the selected intelligence, never a
//! temporary workflow: a harness seat (an AI app the human already has,
//! through the same infer-grade adapter `nika run` uses), an API or a
//! local engine (through the same provider registry and the same one-shot
//! infer verb), a scripted reasoner (the tests' stand-in, which records
//! exactly what it was given), or none.

#[cfg(test)]
mod label_tests;
#[cfg(test)]
pub(crate) mod test_transport;
#[cfg(test)]
pub(crate) type ProviderHttp = test_transport::Client;
#[cfg(not(test))]
pub(crate) type ProviderHttp = nika_http::ReqwestHttp;

use std::collections::VecDeque;
use std::sync::Arc;

use nika_onboard::compile::AuthoringReasoning;

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

/// A reasoner: a name and one turn. `Send`, so a host may run a turn on a
/// worker thread while its terminal stays live (the renderer's busy state).
pub trait SessionReasoner: Send {
    /// The path's name for the banner (`codex` · `mistral API` · `none`).
    fn name(&self) -> String;

    /// One turn over the broker's prompt.
    ///
    /// # Errors
    ///
    /// When the path cannot answer — the runtime refuses the turn with
    /// the reason, never silently switches paths.
    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError>;

    /// One bounded label (the semantic route): the same call under a
    /// small output ceiling and a zero temperature where the path can
    /// set them; by default the ordinary turn.
    ///
    /// # Errors
    ///
    /// The path could not answer.
    fn reason_label(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.reason(prompt)
    }

    /// The explicitly selected tool-free subscription authoring adapter.
    /// A wrapper must forward this capability; the default grants none.
    /// This is independent of billed-provider catalog admission.
    fn authoring_harness(&self) -> Option<String> {
        None
    }

    /// The connection granted by this reasoner; wrappers preserve an explicit ACP choice.
    fn harness_transport(&self) -> nika_types::access::HarnessTransport {
        nika_types::access::HarnessTransport::Native
    }

    /// Whether this implementation opts into the shared admission seam.
    /// Custom and subscription implementations remain default-refusing.
    fn supports_admission(&self) -> bool {
        false
    }

    /// Reason using a shared catalog allowance; never delegates unmetered.
    /// # Errors
    /// Unsupported implementations refuse without calling `reason`.
    fn reason_with_admission(
        &mut self,
        _prompt: &str,
        _account: &nika_providers::InferenceAdmission,
    ) -> Result<Reply, ReasonError> {
        Err(ReasonError::Provider(
            "selected reasoner has no catalog admission seam".into(),
        ))
    }

    /// Label using the same shared allowance, including fresh classifiers.
    /// # Errors
    /// Unsupported implementations refuse without calling `reason_label`.
    fn reason_label_with_admission(
        &mut self,
        _prompt: &str,
        _account: &nika_providers::InferenceAdmission,
    ) -> Result<Reply, ReasonError> {
        Err(ReasonError::Provider(
            "selected classifier has no catalog admission seam".into(),
        ))
    }

    /// One turn, or one bounded label, asking an explicit reasoning effort (R4 B16), under the
    /// shared allowance when one is given. A path that cannot carry an effort refuses it before
    /// any call: the level is never dropped.
    ///
    /// # Errors
    /// The path cannot carry the effort, or could not answer.
    fn reason_effort(
        &mut self,
        _prompt: &str,
        _label: bool,
        _account: Option<&nika_providers::InferenceAdmission>,
        effort: AuthoringReasoning,
    ) -> Result<Reply, ReasonError> {
        Err(ReasonError::Provider(format!(
            "this intelligence cannot carry the explicit reasoning effort `{}` · nothing was sent",
            effort.word()
        )))
    }

    /// The `<provider>/<model>` the compiler may author with under this
    /// path — the same model the human chose to reason with, when the
    /// path is a metered API or a local engine. A seat that reasons in
    /// words only, and no path at all, name none. Subscription capability
    /// is declared separately by `authoring_harness`.
    fn authoring_model(&self) -> Option<String> {
        None
    }
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
#[derive(Debug, Clone)]
pub struct HarnessReasoner {
    /// The seat id.
    pub seat: String,
}

#[cfg(feature = "access-harness")]
impl HarnessReasoner {
    /// Retain the host's explicit model for conversation, classification and
    /// clarification as well as authoring; the original struct stays compatible.
    #[must_use]
    pub fn with_model(self, model: Option<String>) -> impl SessionReasoner {
        self.with_transport(model, nika_types::access::HarnessTransport::Native)
    }
    /// Select the connection explicitly for every conversational and authoring call.
    #[must_use]
    pub fn with_transport(
        self,
        model: Option<String>,
        transport: nika_types::access::HarnessTransport,
    ) -> impl SessionReasoner {
        SelectedHarnessReasoner {
            harness: self,
            model,
            transport,
        }
    }
}

#[cfg(feature = "access-harness")]
struct SelectedHarnessReasoner {
    harness: HarnessReasoner,
    model: Option<String>,
    transport: nika_types::access::HarnessTransport,
}

#[cfg(feature = "access-harness")]
impl SessionReasoner for SelectedHarnessReasoner {
    fn name(&self) -> String {
        self.harness.name()
    }
    fn authoring_harness(&self) -> Option<String> {
        self.harness.authoring_harness()
    }
    fn harness_transport(&self) -> nika_types::access::HarnessTransport {
        self.transport
    }
    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        let (text, usage_observed) = block_on(nika_harness::authoring::reason(
            &self.harness.seat,
            self.model.as_deref(),
            self.transport,
            prompt,
        ))?
        .map_err(ReasonError::Seat)?;
        Ok(Reply {
            text,
            usage_observed,
        })
    }
}

#[cfg(feature = "access-harness")]
impl SessionReasoner for HarnessReasoner {
    fn authoring_harness(&self) -> Option<String> {
        Some(self.seat.clone())
    }
    fn name(&self) -> String {
        self.seat.clone()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.clone().with_model(None).reason(prompt)
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
    fn supports_admission(&self) -> bool {
        true
    }
    fn reason_with_admission(
        &mut self,
        prompt: &str,
        account: &nika_providers::InferenceAdmission,
    ) -> Result<Reply, ReasonError> {
        self.infer(prompt, Some(8192), Some(account), None)
    }
    fn reason_label_with_admission(
        &mut self,
        prompt: &str,
        account: &nika_providers::InferenceAdmission,
    ) -> Result<Reply, ReasonError> {
        self.infer(prompt, Some(self.label_ceiling()), Some(account), None)
    }

    fn name(&self) -> String {
        self.label.clone()
    }

    fn authoring_model(&self) -> Option<String> {
        Some(self.model.clone())
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.infer(prompt, None, None, None)
    }

    fn reason_label(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.infer(prompt, Some(self.label_ceiling()), None, None)
    }

    /// The same call, the same ceiling, asking the level (R4 B16): the effort never moves a cap.
    fn reason_effort(
        &mut self,
        prompt: &str,
        label: bool,
        account: Option<&nika_providers::InferenceAdmission>,
        effort: AuthoringReasoning,
    ) -> Result<Reply, ReasonError> {
        let ceiling = if label {
            Some(self.label_ceiling())
        } else {
            account.map(|_| 8192)
        };
        self.infer(prompt, ceiling, account, Some(effort))
    }
}

/// Ordinary labels retain their existing finite ceiling.
const LABEL_CEILING_TOKENS: u32 = 1024;
/// Catalog-known reasoning shares output tokens with the visible label.
/// This finite first-call ceiling matches the compiler's reasoning draft
/// floor; it does not guarantee an answer and never triggers a larger retry.
const REASONING_LABEL_CEILING_TOKENS: u32 = 4096;

impl ProviderReasoner {
    /// Select the label default only; caller-supplied infer limits stay intact.
    fn label_ceiling(&self) -> u32 {
        let reasoning = self.model.split_once('/').is_some_and(|(provider, model)| {
            // The mock catalog row claims every capability for fixtures.
            !provider.eq_ignore_ascii_case("mock")
                && nika_catalog::model_capabilities(provider, model).reasoning
        });
        if reasoning {
            REASONING_LABEL_CEILING_TOKENS
        } else {
            LABEL_CEILING_TOKENS
        }
    }

    /// The one-shot infer verb over the provider registry; a label call
    /// carries its ceiling and a zero temperature, and any call the explicit effort it asks.
    fn infer(
        &self,
        prompt: &str,
        ceiling: Option<u32>,
        admission: Option<&nika_providers::InferenceAdmission>,
        effort: Option<AuthoringReasoning>,
    ) -> Result<Reply, ReasonError> {
        let reasoning_effort = effort
            .map(|level| {
                nika_verb_infer::ReasoningEffort::parse(level.word()).ok_or_else(|| {
                    ReasonError::Provider(format!(
                        "the reasoning effort `{}` has no provider level · nothing was sent",
                        level.word()
                    ))
                })
            })
            .transpose()?;
        let http = provider_http_for(admission.is_some()).map_err(ReasonError::Provider)?;
        let mut registry = nika_providers::ProviderRegistry::new(Arc::new(http), provider_config());
        if let Some(a) = admission {
            registry = registry.with_inference_admission(a.clone());
        }
        let registry = Arc::new(registry);
        let verb = nika_verb_infer::InferVerb::new(registry, self.model.clone());
        let mut input = nika_verb_infer::InferInput::new(prompt);
        input.max_tokens = ceiling.or_else(|| admission.map(|_| 8192));
        if ceiling.is_some() {
            input.temperature = Some(0.0);
        }
        input.reasoning_effort = reasoning_effort;
        let out = block_on(async { verb.run(input).await })?
            .map_err(|e| ReasonError::Provider(e.to_string()))?;
        Ok(Reply {
            text: infer_text(&out.output),
            usage_observed: out.response.usage_reported,
        })
    }
}

/// The transport ceiling of the provider client — the engine's own
/// (`nika_runtime::compose`): the per-request deadline is the wire layer's.
const PROVIDER_TRANSPORT_CEILING: std::time::Duration = std::time::Duration::from_secs(600);

/// The HTTP client for the PROVIDER plane, the same law as the engine's
/// run path (`nika_runtime::compose`): SSRF disabled on purpose — the
/// endpoints come from the fixed provider profiles, never from workflow
/// data, and the local engines (`ollama` · `lmstudio` · …) bind
/// `127.0.0.1` by design — and the transport ceiling raised so a long
/// local answer is not cut at the fetch client's 30 s. The fetch guard
/// (`ReqwestHttp::new`) stays for workflow-controlled URLs only.
///
/// # Errors
///
/// The TLS backend would not initialize.
pub(crate) fn provider_http() -> Result<ProviderHttp, String> {
    provider_http_for(false)
}

pub(crate) fn provider_config() -> nika_providers::ProvidersConfig {
    #[cfg(test)]
    if let Some(config) = test_transport::config() {
        return config;
    }
    nika_runtime::compose::config_from_env()
}

pub(crate) fn provider_http_for(bounded: bool) -> Result<ProviderHttp, String> {
    let mut config = nika_http::HttpConfig::default();
    config.ssrf = nika_http::SsrfMode::Disabled;
    config.retry_protocol_nacks = !bounded;
    config.timeout = PROVIDER_TRANSPORT_CEILING;
    let http = nika_http::ReqwestHttp::with_config(config).map_err(|e| e.to_string())?;
    #[cfg(test)]
    let http = test_transport::Client::new(http);
    Ok(http)
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
