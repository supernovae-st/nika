// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's service tier — the language's word (`standard` · `fast`)
//! bound to the adapter's own speed option, judged, applied and read back
//! after the model and the effort, before the mode and the prompt.
//!
//! The binding is a measured row keyed by the adapter's own name on
//! `initialize` ([`ServiceTierBinding`]) and held for its exact release
//! only: its option id, the category the option is advertised under and
//! the native value of each word. What a native value becomes on a turn
//! belongs to the release and to the agent it bundles, so another release
//! of a measured adapter refuses like an unmeasured one. The category is
//! shared with unrelated options, so the option is matched by its id AND
//! its category, never by the category alone.
//!
//! An adapter may list the option only for a model that supports it, and
//! may store a setter's value without checking that support. So the offer
//! is read on the newest complete configuration (the answer to the last
//! selection, never `session/new`'s stale copy), the setter is sent only
//! for an offered value, and the answer must still hold the option with
//! that value beside the model and the effort. Every later complete
//! answer re-asserts it (`confirm_service_tier`, called by `confirm_holds`).
//! Any absence, other native form, ambiguity or mismatch refuses before the
//! prompt.
//!
//! Nothing here states the tier a turn carried or the tier that served it:
//! an ACP prompt result names neither.
//!
//! [`ServiceTierBinding`]: nika_types::access::ServiceTierBinding

use nika_kernel::ai::harness::{HarnessError, HarnessRequest, ModelProvenance};
use nika_types::access::{ServiceTier, ServiceTierBinding};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};

use super::{Driver, effort, seats};
use crate::wire;

const ID_SESSION_SPEED: u64 = 8;

fn refusal(reason: String) -> HarnessError {
    HarnessError::Selection { reason }
}

/// The one option `binding` names, when the configuration advertises it under
/// the measured category. Two options with that id, or the id under another
/// category, are not the measured control.
pub(super) fn tier_option<'c>(
    config: Option<&'c Value>,
    binding: &ServiceTierBinding,
) -> Result<Option<&'c Value>, String> {
    let Some(options) = config.and_then(Value::as_array) else {
        return Ok(None);
    };
    let found: Vec<&Value> = options
        .iter()
        .filter(|o| o.get("id").and_then(Value::as_str) == Some(binding.option))
        .collect();
    match found.as_slice() {
        [] => Ok(None),
        [one] => match one.get("category").and_then(Value::as_str) {
            Some(category) if category == binding.category => Ok(Some(one)),
            other => Err(format!(
                "the harness advertises `{}` under {}, not under `{}` as measured on {} — it \
                 is not the speed control this client knows",
                binding.option,
                other.map_or_else(|| "no category".to_owned(), |c| format!("`{c}`")),
                binding.category,
                binding.measured_on
            )),
        },
        many => Err(format!(
            "the harness advertises {} options with the id `{}` — it is ambiguous which one \
             is the speed control",
            many.len(),
            binding.option
        )),
    }
}

fn listed(values: &[String]) -> String {
    if values.is_empty() {
        "(no value)".to_owned()
    } else {
        values.join(" · ")
    }
}

impl<R, W> Driver<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Keep the adapter's own identity from its `initialize` answer: the name its measured
    /// rows are keyed by and the release a measured speed control holds for.
    pub(super) fn record_agent(&mut self, initialize: &Value) {
        let field = |pointer: &str| {
            initialize
                .pointer(pointer)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        self.agent_name = field("/agentInfo/name");
        self.agent_version = field("/agentInfo/version");
    }

    /// The selections that depend on the model, in order: the effort, then the
    /// service tier, each judged on the newest complete configuration the
    /// session reported, then the mode, whose answer re-asserts them all.
    pub(super) async fn seat_after_model(
        &mut self,
        sid: String,
        session: &wire::NewSessionResult,
        config: Option<Value>,
        request: &HarnessRequest,
    ) -> Result<(), HarnessError> {
        let effort = request.requested_effort.as_deref();
        let config = self.select_effort(&sid, config, effort).await?;
        let tier = request.requested_service_tier.as_deref();
        let config = self.select_service_tier(&sid, config, tier).await?;
        self.settle_mode(sid, session, config.as_ref(), request)
            .await
    }

    /// Apply the service tier `word` (when asked) over `config` — the complete
    /// state the session last reported — and read it back. Returns the newest
    /// complete configuration: the setter's answer, or `config` itself when no
    /// tier was asked (nothing is sent and nothing is recorded).
    async fn select_service_tier(
        &mut self,
        sid: &str,
        config: Option<Value>,
        word: Option<&str>,
    ) -> Result<Option<Value>, HarnessError> {
        let Some(word) = word else {
            return Ok(config);
        };
        let (binding, native, offered) = self.judge_service_tier(config.as_ref(), word)?;
        self.selection.service_tier_option = Some(binding.option.to_owned());
        self.selection.offered_service_tier = Some(offered);
        self.send_request(
            ID_SESSION_SPEED,
            wire::METHOD_SET_CONFIG_OPTION,
            &wire::SetConfigOptionParams {
                session_id: sid.to_owned(),
                config_id: binding.option.to_owned(),
                value: Value::String(native.to_owned()),
            },
        )
        .await?;
        self.selection.transmitted_service_tier = Some(native.to_owned());
        let answered: Value = self
            .await_response(ID_SESSION_SPEED, "session/set_config_option")
            .await?;
        // The answer is the complete state: the tier, and the model and effort selected
        // before it, must all hold on it.
        let newest = answered.get("configOptions").cloned();
        self.confirm_holds(newest.as_ref())?;
        Ok(newest)
    }

    /// Judge `word` before anything is sent: the word itself, the measured
    /// binding of the adapter that answered `initialize`, its exact release, the
    /// way the model was selected, and the option the session lists for that
    /// model. Returns the binding, the native value to send and the values the
    /// session offers.
    fn judge_service_tier(
        &self,
        config: Option<&Value>,
        word: &str,
    ) -> Result<(ServiceTierBinding, &'static str, Vec<String>), HarnessError> {
        let field = format!("`run.access.service_tier: {word}`");
        let Some(tier) = ServiceTier::parse(word) else {
            return Err(refusal(format!(
                "{field} is not a service tier (standard · fast); no prompt was sent"
            )));
        };
        let agent = self.agent_name.clone().unwrap_or_default();
        let Some(binding) = ServiceTierBinding::for_agent(&agent) else {
            return Err(refusal(format!(
                "{field} cannot be applied: no speed control is measured for the harness that \
                 answers as `{agent}` — remove the key to keep its own tier (no prompt was sent)"
            )));
        };
        let release = self.agent_version.clone();
        if !release.as_deref().is_some_and(|v| binding.measures(v)) {
            return Err(refusal(format!(
                "{field} cannot be applied: the speed control of `{agent}` is measured on {} \
                 and this session answers as {} — what its `{}` option does on a turn is not \
                 measured for that release (install the measured release, or remove the key to \
                 keep its own tier; no prompt was sent)",
                binding.measured_on,
                release.map_or_else(|| "no release".to_owned(), |v| format!("release `{v}`")),
                binding.option
            )));
        }
        let model = self
            .observed_model
            .clone()
            .unwrap_or_else(|| "the session default".into());
        if self.observed_source == Some(ModelProvenance::AcceptedRequest) {
            // A legacy model selection answers no configuration: the option the session
            // listed before it says nothing about the model now selected.
            return Err(refusal(format!(
                "the harness selects `{model}` only through the legacy model list, which \
                 reports no refreshed options — {field} cannot be judged for it (no prompt was \
                 sent)"
            )));
        }
        let judged = tier_option(config, &binding).map_err(|why| {
            refusal(format!(
                "{why}, so {field} cannot be applied (no prompt was sent)"
            ))
        })?;
        let Some(option) = judged else {
            return Err(refusal(format!(
                "the harness offers no `{}` option for model `{model}`, so {field} cannot be \
                 applied — it advertises {} (remove the key to keep the route's own tier; no \
                 prompt was sent)",
                binding.option,
                effort::advertised_ids(config)
            )));
        };
        let kind = option.get("type").and_then(Value::as_str);
        if kind != Some("select") {
            return Err(refusal(format!(
                "the harness advertises `{}` as {} for model `{model}`, a form this client did \
                 not negotiate — {field} cannot be applied (no prompt was sent)",
                binding.option,
                kind.map_or_else(|| "an untyped option".to_owned(), |k| format!("a `{k}`")),
            )));
        }
        let native = binding.native(tier);
        let offered = seats::choice_values(option);
        if !offered.iter().any(|value| value == native) {
            return Err(refusal(format!(
                "the harness offers no `{native}` on `{}` for model `{model}` — it offers: {} — \
                 so {field} cannot be applied (no prompt was sent)",
                binding.option,
                listed(&offered)
            )));
        }
        Ok((binding, native, offered))
    }

    /// The tier this session currently reports through its measured option,
    /// read as a native value. `None`: no measured binding, the option is not
    /// advertised as measured, or its current value is not a string. Only a
    /// session whose release was judged at selection ever asks.
    fn current_service_tier(&self, config: Option<&Value>) -> Option<String> {
        let binding = ServiceTierBinding::for_agent(self.agent_name.as_deref()?)?;
        effort::current(tier_option(config, &binding).ok()?)
    }

    /// Read back a complete configuration: the tier that was sent must still
    /// hold. A later selection that resets it, or drops the option, refuses.
    pub(super) fn confirm_service_tier(
        &mut self,
        config: Option<&Value>,
    ) -> Result<(), HarnessError> {
        let Some(sent) = self.selection.transmitted_service_tier.clone() else {
            return Ok(());
        };
        let read = self.current_service_tier(config);
        if read.as_deref() != Some(sent.as_str()) {
            let option = self.selection.service_tier_option.as_deref().unwrap_or("?");
            return Err(refusal(format!(
                "the harness did not confirm `{option}` = `{sent}` after selection (it reports \
                 {}), so the service tier `run.access.service_tier` asks is not held; no prompt \
                 was sent",
                read.map_or_else(|| "none".to_owned(), |v| format!("`{v}`"))
            )));
        }
        self.selection.configured_service_tier = read;
        self.selection.configured_service_tier_source = Some(ModelProvenance::ConfirmedSelection);
        Ok(())
    }

    /// A `config_option_update` during the turn: the agent itself moved the
    /// tier away from what was configured, or stopped offering it — recorded,
    /// never hidden.
    pub(super) fn observe_service_tier_update(&mut self, config: Option<&Value>) {
        let Some(was) = self.selection.configured_service_tier.clone() else {
            return;
        };
        match self.current_service_tier(config) {
            Some(now) if now == was => {}
            Some(now) => self
                .selection
                .changed_mid_turn
                .push(format!("service_tier={now}")),
            None => self
                .selection
                .changed_mid_turn
                .push("service_tier=unoffered".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests;
