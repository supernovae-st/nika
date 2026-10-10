// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The selected API or local intelligence as the model of a Session conversation: the same
//! provider registry, the same preparation account, the same Stop and the same gates as
//! [`ProviderReasoner`], asked with the whole conversation and the Session's tools instead of
//! one prompt.
//!
//! [`ProviderReasoner`]: super::ProviderReasoner

use std::sync::Arc;

use nika_kernel::provider::{
    InferRequest, InferResponse, Message, ReasoningEffort, Role, StopReason,
};
use nika_onboard::compile::AuthoringReasoning;
use nika_providers::authoring::{policy::completion_bounds, preparation::PreparationCosts};
use nika_providers::probe::{LocalLiveness, local_run_gate};
use nika_session_agent::{AgentEvent, Model, ModelError, Reply, Request};
use nika_types::cost::UnpricedReason;

use super::{ReasonError, allowance_ceiling, block_on, provider_config, provider_http_for};

/// One conversation's model: a `provider/name` route of the provider registry.
#[derive(Debug, Clone)]
pub struct AgentModel {
    model: String,
    admission: Option<nika_providers::InferenceAdmission>,
    effort: Option<AuthoringReasoning>,
}

impl AgentModel {
    /// The route `model` (`provider/name`), as the person selected it.
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            admission: None,
            effort: None,
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
        let http = provider_http_for(self.admission.is_some()).map_err(ModelError::Failed)?;
        let mut registry = nika_providers::ProviderRegistry::new(Arc::new(http), provider_config());
        if let Some(admission) = &self.admission {
            registry = registry.with_inference_admission(admission.clone());
        }
        let provider = (registry.resolve(&self.model))
            .map_err(|error| ModelError::Failed(error.to_string()))?;
        // A silent or mute local server refuses before any wire call, as a run's does.
        if let Some(LocalLiveness::Mute(addr) | LocalLiveness::Silent(addr)) =
            local_run_gate(&registry, &self.model)
        {
            return Err(ModelError::Failed(format!(
                "the local endpoint {addr} does not answer · nothing was sent"
            )));
        }
        let infer =
            (self.infer_request(provider.wire_model(), request)).map_err(ModelError::Failed)?;
        let answered = block_on(async { provider.infer_reported(infer).await }).map_err(
            |error| match error {
                ReasonError::Cancelled => ModelError::Stopped,
                other => ModelError::Failed(other.to_string()),
            },
        )?;
        let (response, _transport) =
            answered.map_err(|(error, _report)| ModelError::Failed(error.to_string()))?;
        usable(&self.model, &response).map_err(ModelError::Failed)?;
        let reply = Reply::new(response.content, response.stop_reason).with_model(&self.model);
        Ok(if response.usage_reported {
            reply.with_usage(response.usage)
        } else {
            reply
        })
    }
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
