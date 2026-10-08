// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's NATIVE reasoning effort — discovered, applied and read
//! back between `session/new` (plus any model selection) and the prompt.
//!
//! An ACP agent advertises its reasoning selector as a session config
//! option of category `thought_level` (codex-acp: `reasoning_effort`,
//! claude-agent-acp: `effort`); its values depend on the selected model,
//! so the option is read from the COMPLETE configuration the agent
//! returned after the model selection, never from `session/new`'s stale
//! copy. The requested value must equal one advertised value exactly (no
//! case folding, no alias between `max`/`ultra`/`xhigh`, no fabricated
//! model suffix), it is sent through the option's own id, and both the
//! effort and the model are read back from the answer before the prompt.
//! Any absence, ambiguity or mismatch refuses before the prompt.

use nika_kernel::ai::harness::{HarnessError, ModelProvenance};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};

use super::{Driver, seats};
use crate::wire;

const ID_SESSION_EFFORT: u64 = 5;

/// The one `category: "thought_level"` option, when advertised. Two of
/// them are ambiguous: the client cannot know which one is the effort.
pub(super) fn thought_option(config: Option<&Value>) -> Result<Option<&Value>, String> {
    let Some(options) = config.and_then(Value::as_array) else {
        return Ok(None);
    };
    let found: Vec<&Value> = options
        .iter()
        .filter(|o| o.get("category").and_then(Value::as_str) == Some("thought_level"))
        .collect();
    match found.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one)),
        many => Err(format!(
            "the harness advertises {} reasoning options ({}) — it is ambiguous which one is \
             the effort",
            many.len(),
            many.iter()
                .filter_map(|o| o.get("id").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" · ")
        )),
    }
}

/// The current value of an option.
pub(super) fn current(option: Option<&Value>) -> Option<String> {
    option?
        .get("currentValue")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Every advertised option id (with its category when it has one), for a
/// refusal that has no reasoning option to name.
fn advertised_ids(config: Option<&Value>) -> String {
    let ids: Vec<String> = config
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .filter_map(|o| {
                    let id = o.get("id").and_then(Value::as_str)?;
                    Some(match o.get("category").and_then(Value::as_str) {
                        Some(category) => format!("{id} ({category})"),
                        None => id.to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        "no configuration option at all".to_owned()
    } else {
        ids.join(" · ")
    }
}

fn refusal(reason: String) -> HarnessError {
    HarnessError::Selection { reason }
}

impl<R, W> Driver<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Apply `wanted` (when asked) over `config` — the complete state the
    /// session last reported — and read it back; without a request, record
    /// the session's own current effort as stated (never applied).
    pub(super) async fn select_effort(
        &mut self,
        sid: &str,
        config: Option<&Value>,
        wanted: Option<&str>,
    ) -> Result<(), HarnessError> {
        let Some(wanted) = wanted else {
            // Nothing asked: record the session's own value when it states one unambiguously;
            // an ambiguous pair is simply not recorded (no request depends on it).
            self.selection.configured_effort = thought_option(config).ok().and_then(current);
            self.selection.configured_effort_source = self
                .selection
                .configured_effort
                .as_ref()
                .map(|_| ModelProvenance::SessionConfig);
            return Ok(());
        };
        let option = thought_option(config).map_err(refusal)?;
        let model = self
            .observed_model
            .clone()
            .unwrap_or_else(|| "the session default".into());
        let Some(option) = option else {
            return Err(refusal(format!(
                "the harness offers no reasoning-effort option for model `{model}`, so effort \
                 `{wanted}` cannot be applied — it advertises {} (no prompt was sent)",
                advertised_ids(config)
            )));
        };
        let id = option
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| refusal("the harness reasoning option names no id".to_owned()))?
            .to_owned();
        let offered = seats::choice_values(option);
        if option.get("type").and_then(Value::as_str) != Some("select")
            || !offered.iter().any(|value| value == wanted)
        {
            return Err(refusal(format!(
                "the harness offers no reasoning effort `{wanted}` for model `{model}` — it \
                 offers: {} (name one of these exact values on `run.reasoning.effort`; no \
                 prompt was sent)",
                if offered.is_empty() {
                    "(no value)".to_owned()
                } else {
                    offered.join(" · ")
                }
            )));
        }
        self.send_request(
            ID_SESSION_EFFORT,
            wire::METHOD_SET_CONFIG_OPTION,
            &wire::SetConfigOptionParams {
                session_id: sid.to_owned(),
                config_id: id.clone(),
                value: Value::String(wanted.to_owned()),
            },
        )
        .await?;
        self.selection.effort_option = Some(id);
        self.selection.transmitted_effort = Some(wanted.to_owned());
        let answered: Value = self
            .await_response(ID_SESSION_EFFORT, "session/set_config_option")
            .await?;
        self.confirm_holds(answered.get("configOptions"))
    }

    /// Read back the complete configuration an answer carried: the effort
    /// that was sent and the model that was selected must both still hold.
    pub(super) fn confirm_holds(&mut self, config: Option<&Value>) -> Result<(), HarnessError> {
        if let Some(wanted) = self.selection.transmitted_effort.clone() {
            let read = current(thought_option(config).map_err(refusal)?);
            if read.as_deref() != Some(wanted.as_str()) {
                return Err(refusal(format!(
                    "the harness did not confirm reasoning effort `{wanted}` after selection (it \
                     reports {}); no prompt was sent",
                    read.map_or_else(|| "none".to_owned(), |v| format!("`{v}`"))
                )));
            }
            self.selection.configured_effort = read;
            self.selection.configured_effort_source = Some(ModelProvenance::ConfirmedSelection);
        }
        if self.observed_source == Some(ModelProvenance::ConfirmedSelection)
            && let Some(model) = self.selection.transmitted_model.clone()
        {
            let read = seats::current_model(seats::model_option(config), None);
            if read.as_deref() != Some(model.as_str()) {
                return Err(refusal(format!(
                    "the harness moved the model from `{model}` to {} while its effort was set; \
                     no prompt was sent",
                    read.map_or_else(|| "none".to_owned(), |v| format!("`{v}`"))
                )));
            }
        }
        Ok(())
    }

    /// A `config_option_update` during the turn: the agent itself moved
    /// the model or the effort away from what was configured — recorded,
    /// never hidden (an ACP agent may fall back on a rate limit).
    pub(super) fn observe_config_update(&mut self, update: &Value) {
        let config = update.get("configOptions");
        let model = seats::current_model(seats::model_option(config), None);
        if let (Some(now), Some(was)) = (model, self.observed_model.as_deref())
            && now != was
        {
            self.selection.changed_mid_turn.push(format!("model={now}"));
        }
        if let (Ok(option), Some(was)) = (
            thought_option(config),
            self.selection.configured_effort.clone(),
        ) && let Some(now) = current(option)
            && now != was
        {
            self.selection
                .changed_mid_turn
                .push(format!("effort={now}"));
        }
    }
}

#[cfg(test)]
mod tests;
