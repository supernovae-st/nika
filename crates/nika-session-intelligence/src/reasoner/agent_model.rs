// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The selected API or local intelligence as the model of a Session conversation: the same
//! provider registry, the same preparation account, the same Stop and the same gates as
//! [`ProviderReasoner`], asked with the whole conversation and the Session's tools instead of
//! one prompt.
//!
//! [`ProviderReasoner`]: super::ProviderReasoner

use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use nika_kernel::provider::{
    InferRequest, InferResponse, Message, ProviderError, ReasoningEffort, Role, StopReason,
};
use nika_onboard::compile::AuthoringReasoning;
use nika_providers::authoring::{policy::completion_bounds, preparation::PreparationCosts};
use nika_providers::probe::{LocalLiveness, local_run_gate};
use nika_providers::{ResolvedProvider, transient_rejection};
use nika_session_agent::{AgentEvent, Model, ModelError, Reply, Request};
use nika_types::cost::UnpricedReason;

use super::{ProviderHttp, allowance_ceiling, provider_config, provider_http};

/// How many times a call a provider rejected transiently is asked again.
const ASKED_AGAIN: usize = 2;

/// The longest delay the policy waits before asking again: a provider naming a longer one ends
/// the call, said, for the person to decide.
const LONGEST_WAIT: Duration = Duration::from_secs(30);

/// One conversation's model: a `provider/name` route of the provider registry.
#[derive(Debug, Clone)]
pub struct AgentModel {
    model: String,
    admission: Option<nika_providers::InferenceAdmission>,
    effort: Option<AuthoringReasoning>,
    transport: Option<Arc<Transport>>,
}

/// The turn's one transport: the route resolved over one client, and the runtime its
/// connections live on, built at the turn's first call and kept for its others. The admission
/// and Stop still wrap each request.
struct Transport {
    runtime: tokio::runtime::Runtime,
    provider: ResolvedProvider<ProviderHttp>,
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let model = self.provider.wire_model();
        f.debug_struct("Transport")
            .field("model", &model)
            .finish_non_exhaustive()
    }
}

impl AgentModel {
    /// The route `model` (`provider/name`), as the person selected it.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            admission: None,
            effort: None,
            transport: None,
        }
    }

    /// The Session's admission of inference spending, as its reasoner holds it.
    #[must_use]
    pub fn with_admission(mut self, admission: nika_providers::InferenceAdmission) -> Self {
        self.admission = Some(admission);
        self
    }

    /// The reasoning effort the person chose for the conversation.
    #[must_use]
    pub fn with_effort(mut self, effort: AuthoringReasoning) -> Self {
        self.effort = Some(effort);
        self
    }

    /// The route, as selected.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The request the route receives: the instructions first, the conversation, the tools,
    /// the effort asked and the route's real capacity in continuous preparation.
    fn infer_request(&self, wire_model: &str, request: &Request) -> Result<InferRequest, String> {
        let mut messages = Vec::with_capacity(request.messages.len() + 1);
        if let Some(system) = &request.system {
            messages.push(Message::text(Role::System, system.clone()));
        }
        messages.extend(request.messages.iter().cloned());
        let mut infer = InferRequest::new(wire_model, messages);
        infer.tools.clone_from(&request.tools);
        if let Some(level) = self.effort {
            let effort = ReasoningEffort::parse(level.word()).ok_or_else(|| {
                format!(
                    "the reasoning effort `{}` has no provider level · nothing was sent",
                    level.word()
                )
            })?;
            infer.reasoning_effort = Some(effort);
        }
        infer.max_tokens = self
            .admission
            .as_ref()
            .and_then(|_| allowance_ceiling(8192));
        if PreparationCosts::active() {
            // A conversation asks its route's real capacity, never a fixed ceiling.
            let limits = completion_bounds(&self.model, false, provider_config());
            infer.max_tokens = Some(limits.max_tokens);
            infer.timeout = Some(limits.timeout);
        }
        Ok(infer)
    }
}

impl Model for AgentModel {
    fn complete(
        &mut self,
        request: &Request,
        _events: &mut dyn FnMut(AgentEvent),
    ) -> Result<Reply, ModelError> {
        let transport = self.transport()?;
        let wire_model = transport.provider.wire_model();
        let infer = (self.infer_request(wire_model, request)).map_err(ModelError::Failed)?;
        let response = transport.asked(&infer, self.admission.as_ref())?;
        usable(&self.model, &response).map_err(ModelError::Failed)?;
        let reply = Reply::new(response.content, response.stop_reason).with_model(&self.model);
        Ok(if response.usage_reported {
            reply.with_usage(response.usage)
        } else {
            reply
        })
    }
}

impl AgentModel {
    /// The turn's transport, built at its first call. A silent or mute local server refuses
    /// before any wire call, as a run's does.
    fn transport(&mut self) -> Result<Arc<Transport>, ModelError> {
        if let Some(transport) = &self.transport {
            return Ok(Arc::clone(transport));
        }
        let http = provider_http().map_err(ModelError::Failed)?;
        let mut registry = nika_providers::ProviderRegistry::new(Arc::new(http), provider_config());
        if let Some(admission) = &self.admission {
            registry = registry.with_inference_admission(admission.clone());
        }
        let provider = (registry.resolve(&self.model))
            .map_err(|error| ModelError::Failed(error.to_string()))?;
        if let Some(LocalLiveness::Mute(addr) | LocalLiveness::Silent(addr)) =
            local_run_gate(&registry, &self.model)
        {
            return Err(ModelError::Failed(format!(
                "the local endpoint {addr} does not answer · nothing was sent"
            )));
        }
        let runtime = (tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build())
        .map_err(|error| ModelError::Failed(error.to_string()))?;
        let transport = Arc::new(Transport { runtime, provider });
        self.transport = Some(Arc::clone(&transport));
        Ok(transport)
    }
}

impl Transport {
    /// The call, asked again by policy after a transient rejection (429 · 503 · 529): after
    /// the delay the provider named, else 1 s then 2 s, `ASKED_AGAIN` times at most and never
    /// past `LONGEST_WAIT`. Each ask is a new physical call the admission counts, never one
    /// past an `account` that keeps the rejected call's charge as unknown; Stop ends a call or
    /// a wait.
    fn asked(
        &self,
        infer: &InferRequest,
        account: Option<&nika_providers::InferenceAdmission>,
    ) -> Result<InferResponse, ModelError> {
        let mut waited: Vec<Duration> = Vec::new();
        loop {
            let call = PreparationCosts::while_active(self.provider.infer_reported(infer.clone()));
            let error = match self.runtime.block_on(call).ok_or(ModelError::Stopped)? {
                Ok((response, _report)) => return Ok(response),
                Err((error, _report)) => error,
            };
            let Some(rejection) = transient_rejection(&error) else {
                return Err(ModelError::Failed(failed(&error, &waited, None)));
            };
            let wait =
                (rejection.retry_after).unwrap_or(Duration::from_secs(1_u64 << waited.len()));
            // An account that keeps the rejected call's charge as unknown sends nothing more.
            if account.is_some_and(|account| !admits(account)) {
                let said = failed(&error, &waited, None);
                return Err(ModelError::Failed(format!(
                    "{said} · the money account keeps that call's charge as unknown, so Nika did not ask again"
                )));
            }
            if waited.len() == ASKED_AGAIN || wait > LONGEST_WAIT {
                return Err(ModelError::Failed(failed(&error, &waited, Some(wait))));
            }
            // The timer is made inside the runtime that drives it, never before.
            let slept = async { PreparationCosts::while_active(tokio::time::sleep(wait)).await };
            self.runtime.block_on(slept).ok_or(ModelError::Stopped)?;
            waited.push(wait);
        }
    }
}

/// Whether `account` still admits a call: one it keeps as uncertain or closed sends nothing.
fn admits(account: &nika_providers::InferenceAdmission) -> bool {
    (account.snapshot()).is_ok_and(|receipt| receipt.state == nika_providers::AdmissionState::Open)
}

/// What the person reads of a call the policy gave up on: the provider's own error, the delay it
/// asked that no turn waits, and the asks made again with the time they waited.
fn failed(error: &ProviderError, waited: &[Duration], asked: Option<Duration>) -> String {
    let mut said = error.to_string();
    if let Some(wait) = asked.filter(|wait| *wait > LONGEST_WAIT) {
        let secs = wait.as_secs();
        let _ = write!(
            said,
            " · the provider asked to wait {secs} s: say it again later"
        );
    }
    if !waited.is_empty() {
        let total = waited.iter().sum::<Duration>().as_secs_f64();
        let _ = write!(
            said,
            " · asked again {}× ({total:.1} s waited)",
            waited.len()
        );
    }
    said
}

/// What this machine's inventory says of `model` as a run's model: the route that serves it
/// (a ready one first), how that route bills and the catalogue's output list price on a metered
/// route. None when no route here offers it: a choice is never invented.
#[must_use]
pub fn model_facts(
    probes: &[nika_providers::probe::ProviderProbe],
    model: &str,
) -> Option<nika_session_change::work::ModelFacts> {
    use nika_providers::model_choice::{ModelInventory, ModelRole};
    let inventory = ModelInventory::from_probes(probes);
    let offers = inventory.offers(ModelRole::Run);
    let named =
        |ready: bool| (offers.iter()).find(|o| o.model == model && (!ready || o.route.configured));
    let offer = named(true).or_else(|| named(false))?;
    let route = &offer.route;
    let words = (
        route.access.clone(),
        route.class.as_str().to_owned(),
        route.billing.as_str().to_owned(),
    );
    Some(nika_session_change::work::ModelFacts::new(
        "run",
        &offer.model,
        words,
        route.configured,
        offer.output_usd_per_million,
    ))
}

/// The sink a turn's tool steps go to: each real step as the Session's typed activity, its
/// call, its tool, where it is and the time it took (never its arguments or reply).
#[must_use]
pub fn tool_steps(hook: nika_onboard::activity::ActivityHook) -> nika_session_agent::StepSink {
    use nika_onboard::activity::{ToolState, tool_activity};
    use nika_session_agent::{StepState, ToolStep};
    Arc::new(move |step: &ToolStep| {
        let state = match step.state {
            StepState::Started => ToolState::Started,
            StepState::Finished => ToolState::Finished,
            _ => ToolState::Failed,
        };
        hook(&tool_activity(
            &step.call,
            &step.name,
            state,
            step.elapsed_ms,
        ));
    })
}

/// The infer verb's gates on an answer: a priced route that reported no usage spent real money
/// nobody can count, and a provider's explicit refusal is terminal.
fn usable(model: &str, response: &InferResponse) -> Result<(), String> {
    let (_, unpriced) = nika_providers::spend::spend_for_model(model, &response.usage);
    if !response.usage_reported && unpriced == Some(UnpricedReason::ProviderDidNotReportUsage) {
        return Err(format!(
            "`{model}` answered without reporting its usage: the call cost real money that \
             cannot be counted, so its answer is not used"
        ));
    }
    let refused = matches!(response.stop_reason, StopReason::ContentFilter)
        || response.finish_reason_raw.as_deref() == Some("refusal");
    if refused {
        return Err("the provider explicitly refused the response".to_owned());
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
