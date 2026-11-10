// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Reading a session's offer: the handshake, `session/new` and the model
//! selection a run performs, then the three offers read from the complete
//! configuration the session answered, and nothing after.
//!
//! The model is selected the way a run selects it (the exact offered id or
//! display name, through the session's own option, read back), because an
//! adapter recomputes the reasoning values and the speed option when the
//! model changes: the offers of the session's default model say nothing
//! about another one. A model that is absent, that only the legacy list
//! names or that the session does not confirm is the answer, never an
//! error, and what depends on it is left unobserved rather than read from
//! another model's configuration.
//!
//! This path sends `initialize`, `session/new` and at most one
//! `session/set_config_option` (the model). It holds no setter for the
//! effort, the tier or the mode and no `session/prompt`.

use nika_kernel::ai::harness::{HarnessError, HarnessRequest, HarnessSelection};
use nika_types::access::{ServiceTier, ServiceTierBinding};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::sync::mpsc;

use super::{Driver, ID_SESSION_SEAT, StartJudge, effort, seats, speed};
use crate::offer::{EffortOffer, ModelOffer, ServiceTierOffer, SessionOffer, bounded};
use crate::wire;

/// Read the offer of the session `reader`/`writer` reach, for the model
/// `request` names, under the same `completion` profile, `session/new`
/// options and `initialize` judgment a run on this transport would carry.
/// Each frame read and each write may take at most `idle`. The request's
/// prompt is never sent.
pub(crate) async fn read_offer<R, W>(
    reader: R,
    writer: W,
    request: HarnessRequest,
    idle: std::time::Duration,
    completion: Option<crate::authoring::acp::OneShot>,
    meta: Option<Value>,
    start: Option<StartJudge>,
) -> Result<SessionOffer, HarnessError>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    // No consumer: a refused client call before the offer is answered on the wire and its
    // witness has nowhere to go, so the sender fails at once instead of filling a buffer.
    let (event_tx, _) = mpsc::channel(1);
    let mut driver = Driver {
        reader: BufReader::new(reader),
        writer,
        event_tx,
        output: String::new(),
        idle,
        pending: Vec::new(),
        observed_model: None,
        observed_source: None,
        selection: HarnessSelection::default(),
        media: crate::media::MediaState::default(),
        completion,
        progress: None,
        meta,
        start,
        agent_name: None,
        agent_version: None,
        confined: None,
    };
    driver.offer(&request).await
}

fn unobserved(reason: &str) -> (EffortOffer, ServiceTierOffer) {
    (
        EffortOffer::Unobserved {
            reason: reason.to_owned(),
        },
        ServiceTierOffer::Unobserved {
            reason: reason.to_owned(),
        },
    )
}

/// The efforts `config` lists: the values of its one reasoning option.
fn effort_offer(config: Option<&Value>) -> EffortOffer {
    let option = match effort::thought_option(config) {
        Err(reason) => return EffortOffer::Unobserved { reason },
        Ok(None) => return EffortOffer::NotOffered,
        Ok(Some(option)) => option,
    };
    let Some(id) = option.get("id").and_then(Value::as_str) else {
        return EffortOffer::Unobserved {
            reason: "the harness reasoning option names no id".to_owned(),
        };
    };
    if option.get("type").and_then(Value::as_str) != Some("select") {
        return EffortOffer::Unobserved {
            reason: format!(
                "the harness advertises `{id}` in a form this client does not negotiate"
            ),
        };
    }
    let (values, omitted) = bounded(seats::choice_values(option));
    EffortOffer::Offered {
        option: id.to_owned(),
        values,
        omitted,
        current: effort::current(Some(option)),
    }
}

impl<R, W> Driver<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Open the session, select the model `request` names and read what the
    /// session lists for it. Returns before any prompt exists.
    async fn offer(&mut self, request: &HarnessRequest) -> Result<SessionOffer, HarnessError> {
        let session = self.open_session(request).await?;
        let model_option = seats::model_option(session.config_options.as_ref());
        let listed = seats::offered_values(model_option, session.models.as_ref());
        let (model, config) = self.offered_model(&session, request).await?;
        let (effort, service_tier) = match &model {
            ModelOffer::SessionDefault { .. } | ModelOffer::Confirmed { .. } => (
                effort_offer(config.as_ref()),
                self.service_tier_offer(config.as_ref()),
            ),
            ModelOffer::LegacyList { .. } => unobserved(
                "the harness selects this model only through its legacy model list, which \
                 reports no refreshed options",
            ),
            ModelOffer::NotOffered => unobserved("the harness does not list the model"),
            ModelOffer::NotConfirmed { .. } => {
                unobserved("the harness did not confirm the model after selection")
            }
        };
        Ok(SessionOffer::new(model, effort, service_tier)
            .with_agent(self.agent_name.clone(), self.agent_version.clone())
            .with_models(listed))
    }

    /// The answer about the requested model and the complete configuration
    /// that holds for it: the answer to its selection, or `session/new`'s
    /// when no model was named. The selection is the one setter this path
    /// sends, with the exact payload a run sends.
    async fn offered_model(
        &mut self,
        session: &wire::NewSessionResult,
        request: &HarnessRequest,
    ) -> Result<(ModelOffer, Option<Value>), HarnessError> {
        let model_option = seats::model_option(session.config_options.as_ref());
        let requested = request.requested_model.as_deref();
        let Some(wanted) = seats::wanted(requested) else {
            let current = seats::current_model(model_option, session.models.as_ref());
            return Ok((
                ModelOffer::SessionDefault { current },
                session.config_options.clone(),
            ));
        };
        let option = |name: &str| seats::offered_option(model_option, name);
        let Some((config_id, value)) = seats::exact_first(requested, &wanted, option) else {
            let legacy = seats::exact_first(requested, &wanted, |name| {
                seats::offered_model(session.models.as_ref(), name)
            });
            let answer = legacy.map_or(ModelOffer::NotOffered, |native| ModelOffer::LegacyList {
                native,
            });
            return Ok((answer, None));
        };
        self.send_request(
            ID_SESSION_SEAT,
            wire::METHOD_SET_CONFIG_OPTION,
            &wire::SetConfigOptionParams {
                session_id: session.session_id.clone(),
                config_id,
                value: value.clone(),
            },
        )
        .await?;
        let answered: Value = self
            .await_response(ID_SESSION_SEAT, "session/set_config_option")
            .await?;
        let config = answered.get("configOptions").cloned();
        let reported = seats::current_model(seats::model_option(config.as_ref()), None);
        match reported {
            Some(native) if Some(native.as_str()) == value.as_str() => {
                Ok((ModelOffer::Confirmed { native }, config))
            }
            reported => Ok((ModelOffer::NotConfirmed { reported }, None)),
        }
    }

    /// The tiers `config` lists through the measured speed option of the
    /// adapter that answered `initialize`, at its measured release only.
    fn service_tier_offer(&self, config: Option<&Value>) -> ServiceTierOffer {
        let agent = self.agent_name.clone().unwrap_or_default();
        let Some(binding) = ServiceTierBinding::for_agent(&agent) else {
            return ServiceTierOffer::Unobserved {
                reason: format!(
                    "no speed control is measured for the harness that answers as `{agent}`"
                ),
            };
        };
        let release = self.agent_version.as_deref();
        if !release.is_some_and(|version| binding.measures(version)) {
            return ServiceTierOffer::Unobserved {
                reason: format!(
                    "the speed control of `{agent}` is measured on {} and this session answers \
                     as {}",
                    binding.measured_on,
                    release.map_or_else(|| "no release".to_owned(), |v| format!("release `{v}`")),
                ),
            };
        }
        let option = match speed::tier_option(config, &binding) {
            Err(reason) => return ServiceTierOffer::Unobserved { reason },
            Ok(None) => return ServiceTierOffer::NotOffered,
            Ok(Some(option)) => option,
        };
        if option.get("type").and_then(Value::as_str) != Some("select") {
            return ServiceTierOffer::Unobserved {
                reason: format!(
                    "the harness advertises `{}` in a form this client does not negotiate",
                    binding.option
                ),
            };
        }
        let natives = seats::choice_values(option);
        let tiers = ServiceTier::ALL
            .into_iter()
            .filter(|tier| natives.iter().any(|value| value == binding.native(*tier)))
            .collect();
        ServiceTierOffer::Offered {
            option: binding.option.to_owned(),
            tiers,
            current: effort::current(Some(option)).and_then(|value| binding.tier_of(&value)),
        }
    }
}

#[cfg(test)]
mod tests;
