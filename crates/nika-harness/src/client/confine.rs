// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session confinement an `agent:` task requires (NIK-266) — applied between `session/new`
//! (plus any model and effort selection) and the prompt, never best effort.
//!
//! The workflow's `permits:` arrive projected as a `HarnessConfinement`. For an adapter whose
//! modes were MEASURED (`nika_types::access::HarnessBoundary`), the client selects the mode whose every approval
//! reaches the engine and reads it back from the complete configuration the session answers:
//! an unoffered mode, a refused selection or a read-back that does not hold it refuses before
//! the prompt. A mode that approves on its own (an auto-reviewer, accept-edits, full access,
//! bypass) is never selected. For an adapter with no measurement the client selects nothing:
//! it observes the session's own mode, refuses one that visibly approves on its own, and says
//! so in the witness — an unmeasured mode is never presented as a confinement.
//!
//! Whatever was settled rides `HarnessEvent::SessionConfined` before the prompt, and the
//! mode is then held for the whole turn: a `config_option_update` or `current_mode_update`
//! that moves it ends the session (`judge_drift`).

use nika_kernel::ai::harness::{HarnessConfinement, HarnessError, HarnessEvent};
use nika_types::access::HarnessBoundary;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};

use super::{Driver, effort, seats};
use crate::wire;

const ID_SESSION_CONFINE: u64 = 7;

/// Words that mark a mode as approving actions on its own (`_meta.kind`, id or name): such a
/// mode is refused when no measured mode can replace it. Lowercase substrings, chosen to
/// match the self-approving modes the shipped adapters name (`auto` · `auto_review` ·
/// `autoEdit` · `autopilot` · `acceptEdits` · `bypassPermissions` · `yolo` · `full_access`)
/// and nothing an asking mode is called.
const SELF_APPROVING: [&str; 7] = [
    "full",
    "bypass",
    "yolo",
    "auto",
    "accept",
    "dangerous",
    "skip",
];

fn refusal(reason: String) -> HarnessError {
    HarnessError::Refused { reason }
}

/// The one `category: "mode"` option of a configuration.
fn mode_option(config: Option<&Value>) -> Option<&Value> {
    config?
        .as_array()?
        .iter()
        .find(|option| option.get("category").and_then(Value::as_str) == Some("mode"))
}

/// The ids the legacy `modes` block advertises, in order.
fn legacy_ids(modes: Option<&Value>) -> Vec<String> {
    modes
        .and_then(|m| m.get("availableModes"))
        .and_then(Value::as_array)
        .map(|all| {
            all.iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The legacy block's current mode id.
fn legacy_current(modes: Option<&Value>) -> Option<String> {
    modes?
        .get("currentModeId")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Whether the advertised entry for `mode` (its id, name or `_meta.kind`) reads as a mode that
/// approves actions on its own.
fn self_approving(entry: Option<&Value>, mode: &str) -> bool {
    let mut words = vec![mode.to_ascii_lowercase()];
    if let Some(entry) = entry {
        for key in ["name", "value", "id"] {
            if let Some(text) = entry.get(key).and_then(Value::as_str) {
                words.push(text.to_ascii_lowercase());
            }
        }
        if let Some(kind) = entry.pointer("/_meta/kind").and_then(Value::as_str) {
            words.push(kind.to_ascii_lowercase());
        }
    }
    words
        .iter()
        .any(|word| SELF_APPROVING.iter().any(|marker| word.contains(marker)))
}

/// The advertised entry (config choice or legacy mode) describing `mode`.
fn entry<'a>(option: Option<&'a Value>, modes: Option<&'a Value>, mode: &str) -> Option<&'a Value> {
    let by_option = option.and_then(|o| {
        seats::choices(o)
            .into_iter()
            .find(|c| c.get("value").and_then(Value::as_str) == Some(mode))
    });
    by_option.or_else(|| {
        modes?
            .get("availableModes")?
            .as_array()?
            .iter()
            .find(|m| m.get("id").and_then(Value::as_str) == Some(mode))
    })
}

/// A mode change the agent reported mid-turn that leaves `held`: the session no longer runs
/// under the confinement it was admitted with, so it ends here.
///
/// # Errors
/// The update names another current mode than `held`.
pub(super) fn judge_drift(update: &Value, held: &str) -> Result<(), HarnessError> {
    let now = match update.get("sessionUpdate").and_then(Value::as_str) {
        Some("config_option_update") => effort::current(mode_option(update.get("configOptions"))),
        Some("current_mode_update") => update
            .get("currentModeId")
            .or_else(|| update.get("modeId"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    };
    match now {
        Some(now) if now != held => Err(refusal(format!(
            "the harness left the confined mode `{held}` for `{now}` during the turn; the \
             session is abandoned (NIK-266: a session runs only under the mode it was \
             admitted with)"
        ))),
        _ => Ok(()),
    }
}

impl<R, W> Driver<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Settle `confinement` over `config` (the configuration the session last reported) and
    /// the legacy `modes`, then publish it before the prompt.
    pub(super) async fn confine(
        &mut self,
        sid: &str,
        session: &wire::NewSessionResult,
        config: Option<&Value>,
        confinement: HarnessConfinement,
    ) -> Result<(), HarnessError> {
        let boundary = self
            .agent_name
            .as_deref()
            .and_then(HarnessBoundary::for_agent);
        let (mode, selected, unasked) = if let Some(boundary) = boundary {
            let writes = confinement == HarnessConfinement::WorkspaceWrite;
            let wanted = boundary.mode_for_writes(writes);
            self.select_confined(sid, session, config, wanted, &boundary)
                .await?;
            (Some(wanted.to_owned()), true, boundary.unasked(writes))
        } else {
            let current = self.unmeasured_mode(session, config)?;
            let unasked = format!(
                "{} {}",
                nika_types::access::UNMEASURED_HARNESS_UNASKED,
                nika_types::access::HARNESS_SHARED_RESIDUAL
            );
            (current, false, unasked)
        };
        self.confined.clone_from(&mode);
        let _ = self
            .event_tx
            .send(Ok(HarnessEvent::SessionConfined {
                mode,
                selected,
                unasked,
            }))
            .await;
        Ok(())
    }

    /// Select `wanted` through the mode option (read back from its answer) or the legacy
    /// `session/set_mode` door, or refuse before the prompt.
    async fn select_confined(
        &mut self,
        sid: &str,
        session: &wire::NewSessionResult,
        config: Option<&Value>,
        wanted: &str,
        boundary: &HarnessBoundary,
    ) -> Result<(), HarnessError> {
        let measured = boundary.measured_on;
        let option = mode_option(config);
        let offered = match option {
            Some(option) => seats::choice_values(option),
            None => legacy_ids(session.modes.as_ref()),
        };
        if !offered.iter().any(|value| value == wanted) {
            return Err(refusal(format!(
                "the harness does not offer the mode `{wanted}` measured on {measured} to keep \
                 every approval with Nika (it offers {}); Nika never runs an agent task in a \
                 mode that approves on its own — no prompt was sent",
                if offered.is_empty() {
                    "no mode".to_owned()
                } else {
                    offered.join(" · ")
                }
            )));
        }
        if let Some(option) = option {
            let id = option
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| refusal("the session mode option names no id".to_owned()))?
                .to_owned();
            if effort::current(Some(option)).as_deref() == Some(wanted) {
                return Ok(()); // already held, as the complete configuration reports it
            }
            self.send_request(
                ID_SESSION_CONFINE,
                wire::METHOD_SET_CONFIG_OPTION,
                &wire::SetConfigOptionParams {
                    session_id: sid.to_owned(),
                    config_id: id,
                    value: Value::String(wanted.to_owned()),
                },
            )
            .await?;
            let answered: Value = self
                .await_response(ID_SESSION_CONFINE, "session/set_config_option")
                .await?;
            let read = effort::current(mode_option(answered.get("configOptions")));
            if read.as_deref() != Some(wanted) {
                return Err(refusal(format!(
                    "the harness did not confirm the confined mode `{wanted}` (it reports {}); \
                     no prompt was sent",
                    read.map_or_else(|| "none".to_owned(), |v| format!("`{v}`"))
                )));
            }
            // The answer is the complete state: the model and effort selected before hold.
            return self.confirm_holds(answered.get("configOptions"));
        }
        if legacy_current(session.modes.as_ref()).as_deref() == Some(wanted) {
            return Ok(());
        }
        self.send_request(
            ID_SESSION_CONFINE,
            wire::METHOD_SET_MODE,
            &wire::SetModeParams {
                session_id: sid.to_owned(),
                mode_id: wanted.to_owned(),
            },
        )
        .await?;
        // The legacy door answers no state: its success is the acknowledgement, and any later
        // `current_mode_update` away from `wanted` ends the session (`judge_drift`).
        let _: Value = self
            .await_response(ID_SESSION_CONFINE, "session/set_mode")
            .await?;
        Ok(())
    }

    /// Whether the held mode runs ordinary commands in the harness's own sandbox without asking,
    /// so a command it asks about asks to leave that sandbox (NIK-274: codex-acp `read-only`).
    /// Only a measured mode the client selected and read back says so.
    pub(super) fn commands_escalate(&self) -> bool {
        self.confined.is_some()
            && self
                .agent_name
                .as_deref()
                .and_then(HarnessBoundary::for_agent)
                .is_some_and(|row| row.asked_command_leaves_sandbox)
    }

    /// The session's own mode on an unmeasured adapter: observed, never selected, refused when
    /// it visibly approves actions on its own.
    fn unmeasured_mode(
        &self,
        session: &wire::NewSessionResult,
        config: Option<&Value>,
    ) -> Result<Option<String>, HarnessError> {
        let option = mode_option(config);
        let current = effort::current(option).or_else(|| legacy_current(session.modes.as_ref()));
        let Some(current) = current else {
            return Ok(None);
        };
        if self_approving(entry(option, session.modes.as_ref(), &current), &current) {
            let agent = self.agent_name.as_deref().unwrap_or("this harness");
            return Err(refusal(format!(
                "{agent} runs in mode `{current}`, which approves actions on its own, and Nika \
                 has no measured mode to select for it; set the harness's default mode to one \
                 that asks before acting, or choose a measured seat — no prompt was sent"
            )));
        }
        Ok(Some(current))
    }
}

#[cfg(test)]
mod tests;
