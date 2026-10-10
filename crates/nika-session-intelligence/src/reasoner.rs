// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reasoners — ONE inference over the selected intelligence, never a
//! temporary workflow: a harness seat (an AI app the human already has,
//! through the same infer-grade adapter `nika run` uses), an API or a
//! local engine (through the same provider registry and the same one-shot
//! infer verb), a scripted reasoner (the tests' stand-in, which records
//! exactly what it was given), or none. [`agent_model`] asks the same API or
//! local route with a whole conversation and the Session's tools, for the
//! conversation the selected intelligence leads.

pub mod agent_model;
#[cfg(test)]
mod label_tests;
#[cfg(any(test, feature = "test-support"))]
pub mod test_transport;
#[cfg(test)]
#[allow(clippy::expect_used, clippy::disallowed_methods)]
pub(crate) mod wire;
#[cfg(any(test, feature = "test-support"))]
pub(crate) type ProviderHttp = Wire<test_transport::Client>;
#[cfg(not(any(test, feature = "test-support")))]
pub(crate) type ProviderHttp = Wire<nika_http::ReqwestHttp>;

use std::collections::VecDeque;
use std::sync::Arc;

use nika_onboard::compile::AuthoringReasoning;
use nika_providers::authoring::{
    policy::{completion_bounds, label_ceiling},
    preparation::PreparationCosts,
    requests::{Envelope, Wire},
};

/// Why a reasoner could not answer.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReasonError {
    /// The operator stopped preparation; the conversation and costs remain available.
    #[error("preparation stopped; conversation kept; any sent request may still be billed")]
    Cancelled,
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

impl Reply {
    /// One reply, as a path outside this crate produces it (INV-019).
    #[must_use]
    pub fn new(text: String, usage_observed: bool) -> Self {
        Self {
            text,
            usage_observed,
        }
    }
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

    /// The model that leads the Session's conversation with its tools on this path, when it can:
    /// an API or a local route asked with the whole conversation ([`agent_model::AgentModel`]).
    /// A path that answers prompts only, a subscription seat and no path lead none; the Session
    /// then keeps its round driver.
    fn agent_model(&self) -> Option<agent_model::AgentModel> {
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
    /// Over ACP the named effort travels with the turn: the session applies it through its
    /// advertised reasoning option and reads it back. A direct seat cannot carry it, and a
    /// subscription holds no billed admission: both refuse before any call (R4 B16).
    fn reason_effort(
        &mut self,
        prompt: &str,
        _label: bool,
        account: Option<&nika_providers::InferenceAdmission>,
        effort: AuthoringReasoning,
    ) -> Result<Reply, ReasonError> {
        if account.is_some() {
            return Err(ReasonError::Provider(
                "selected classifier has no catalog admission seam".into(),
            ));
        }
        if self.transport == nika_types::access::HarnessTransport::Native {
            return Err(ReasonError::Provider(format!(
                "this intelligence cannot carry the explicit reasoning effort `{}` · nothing was sent",
                effort.word()
            )));
        }
        let level = nika_verb_infer::ReasoningEffort::parse(effort.word()).ok_or_else(|| {
            ReasonError::Provider(format!(
                "the reasoning effort `{}` has no provider level · nothing was sent",
                effort.word()
            ))
        })?;
        let (text, usage_observed) = block_on(nika_harness::authoring::reason_with_effort(
            &self.harness.seat,
            self.model.as_deref(),
            self.transport,
            prompt,
            Some(level),
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

/// A classifier consumes a complete label, never a user-facing status notice.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplyPurpose {
    Chat,
    Label,
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
        let ceiling = allowance_ceiling(8192);
        self.infer(prompt, ceiling, Some(account), None, ReplyPurpose::Chat)
    }
    fn reason_label_with_admission(
        &mut self,
        prompt: &str,
        account: &nika_providers::InferenceAdmission,
    ) -> Result<Reply, ReasonError> {
        self.infer(
            prompt,
            allowance_ceiling(label_ceiling(&self.model)),
            Some(account),
            None,
            ReplyPurpose::Label,
        )
    }

    fn name(&self) -> String {
        self.label.clone()
    }

    fn authoring_model(&self) -> Option<String> {
        Some(self.model.clone())
    }

    fn agent_model(&self) -> Option<agent_model::AgentModel> {
        Some(agent_model::AgentModel::new(self.model.clone()))
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.infer(prompt, None, None, None, ReplyPurpose::Chat)
    }

    fn reason_label(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        self.infer(
            prompt,
            allowance_ceiling(label_ceiling(&self.model)),
            None,
            None,
            ReplyPurpose::Label,
        )
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
            allowance_ceiling(label_ceiling(&self.model))
        } else {
            account.and_then(|_| allowance_ceiling(8192))
        };
        self.infer(
            prompt,
            ceiling,
            account,
            Some(effort),
            if label {
                ReplyPurpose::Label
            } else {
                ReplyPurpose::Chat
            },
        )
    }
}

/// The historical output ceiling of a call made under an explicit Session allowance; none in
/// continuous preparation, where every call asks its route's real capacity (a fixed label
/// ceiling cut a reasoning model's answer before it said one word). A ceiling a caller passes
/// explicitly is never this default and is always kept.
fn allowance_ceiling(historical: u32) -> Option<u32> {
    (!PreparationCosts::active()).then_some(historical)
}

impl ProviderReasoner {
    /// The one-shot infer verb over the provider registry; a label call
    /// carries its ceiling and a zero temperature, and any call the explicit effort it asks.
    fn infer(
        &self,
        prompt: &str,
        ceiling: Option<u32>,
        admission: Option<&nika_providers::InferenceAdmission>,
        effort: Option<AuthoringReasoning>,
        purpose: ReplyPurpose,
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
        input.max_tokens = ceiling.or_else(|| admission.and_then(|_| allowance_ceiling(8192)));
        if ceiling.is_some() {
            input.temperature = Some(0.0);
        }
        input.reasoning_effort = reasoning_effort;
        if PreparationCosts::active() {
            let limits = completion_bounds(&self.model, false, provider_config());
            // Unbounded chat uses the route's full capacity. A label or an explicit
            // allowance keeps the ceiling its caller passed, including in this scope.
            input.max_tokens = Some(input.max_tokens.unwrap_or(limits.max_tokens));
            input.timeout = Some(limits.timeout);
        }
        let out = block_on(async { verb.run(input).await })?
            .map_err(|e| ReasonError::Provider(e.to_string()))?;
        let mut text = infer_text(&out.output);
        if matches!(
            out.response.stop_reason,
            nika_kernel::ai::provider::StopReason::MaxTokens
        ) {
            if purpose == ReplyPurpose::Label {
                return Err(ReasonError::Provider(
                    "classification response was truncated at its output limit; no label was accepted".to_owned(),
                ));
            }
            text.push_str("\n\n[Incomplete response: the output limit for this response was reached. The conversation remains open; this is not a completed answer.]");
        }
        Ok(Reply {
            text,
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

/// The providers configuration every reasoner and the provider authoring seat read: the
/// engine's ONE environment boundary (`nika_runtime::compose::config_from_env`). Under a test,
/// or a door built with `test-support`, an installed `test_transport` substitution answers
/// first.
#[must_use]
pub fn provider_config() -> nika_providers::ProvidersConfig {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(config) = test_transport::config() {
        return config;
    }
    nika_runtime::compose::config_from_env()
}

pub(crate) fn provider_http_for(bounded: bool) -> Result<ProviderHttp, String> {
    let mut config = nika_http::HttpConfig::default();
    config.ssrf = nika_http::SsrfMode::Disabled;
    config.retry_protocol_nacks = !bounded && !PreparationCosts::active();
    config.timeout = PROVIDER_TRANSPORT_CEILING;
    let http = nika_http::ReqwestHttp::with_config(config).map_err(|e| e.to_string())?;
    #[cfg(any(test, feature = "test-support"))]
    let http = test_transport::Client::new(http);
    Ok(Wire::new(http, Arc::new(Envelope::uncapped(""))))
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
///
/// # Errors
///
/// The current-thread runtime would not start ([`ReasonError::Runtime`]), or the operator
/// stopped preparation before the future finished ([`ReasonError::Cancelled`]).
pub fn block_on<F: std::future::Future>(fut: F) -> Result<F::Output, ReasonError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| ReasonError::Runtime(e.to_string()))?;
    runtime
        .block_on(PreparationCosts::while_active(fut))
        .ok_or(ReasonError::Cancelled)
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

    /// A harness turn asked an explicit effort: a direct seat refuses it before the seat is met,
    /// while over ACP the effort reaches the harness door, so this unprofiled adapter meets the
    /// door's own refusal instead of the effort's (the backend half is pinned in nika-harness).
    #[cfg(feature = "access-harness")]
    #[test]
    fn a_harness_turn_carries_an_effort_over_acp_and_a_direct_seat_refuses_it() {
        use nika_types::access::HarnessTransport;
        let seat = |transport| {
            HarnessReasoner {
                seat: "no-such-harness".to_owned(),
            }
            .with_transport(None, transport)
        };
        let direct =
            seat(HarnessTransport::Native).reason_effort("p", false, None, AuthoringReasoning::Max);
        assert!(
            matches!(&direct, Err(ReasonError::Provider(why))
                if why.contains("cannot carry the explicit reasoning effort `max`")
                    && why.contains("nothing was sent")),
            "{direct:?}"
        );
        let acp =
            seat(HarnessTransport::Acp).reason_effort("p", false, None, AuthoringReasoning::Max);
        assert!(
            matches!(&acp, Err(ReasonError::Seat(why))
                if why.contains("audited completion profile")
                    && !why.contains("reasoning effort")),
            "{acp:?}"
        );
    }
}
