// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session mode a completion profile requires (Codex: `read-only`), applied through the
//! session's own mode option and read back from the complete configuration it answers —
//! never best effort: an absent option, an unoffered mode or a read-back that does not hold
//! it refuses before the prompt.

use nika_kernel::ai::harness::HarnessError;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};

use super::{Driver, effort, seats};
use crate::authoring::acp::{OneShot, refusal};
use crate::wire;

const ID_SESSION_MODE: u64 = 6;

/// The one `category: "mode"` option of a configuration.
fn mode_option(config: Option<&Value>) -> Option<&Value> {
    config?
        .as_array()?
        .iter()
        .find(|option| option.get("category").and_then(Value::as_str) == Some("mode"))
}

impl<R, W> Driver<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Apply `one_shot`'s required mode over `config` (the configuration the session last
    /// reported) and read it back, with the model and effort still holding.
    pub(super) async fn require_mode(
        &mut self,
        sid: &str,
        config: Option<&Value>,
        one_shot: OneShot,
    ) -> Result<(), HarnessError> {
        let Some(mode) = one_shot.required_mode() else {
            return Ok(());
        };
        let label = one_shot.label();
        let Some(option) = mode_option(config) else {
            return Err(refusal(&format!(
                "{label} needs the session mode `{mode}`, and the session advertises no mode \
                 option; no prompt was sent"
            )));
        };
        let id = option
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| refusal(&format!("{label}: the session mode option names no id")))?
            .to_owned();
        let offered = seats::choice_values(option);
        if !offered.iter().any(|value| value == mode) {
            return Err(refusal(&format!(
                "{label} needs the session mode `{mode}`, and the session offers {}; no prompt \
                 was sent",
                offered.join(" · ")
            )));
        }
        self.send_request(
            ID_SESSION_MODE,
            wire::METHOD_SET_CONFIG_OPTION,
            &wire::SetConfigOptionParams {
                session_id: sid.to_owned(),
                config_id: id,
                value: Value::String(mode.to_owned()),
            },
        )
        .await?;
        let answered: Value = self
            .await_response(ID_SESSION_MODE, "session/set_config_option")
            .await?;
        let read = effort::current(mode_option(answered.get("configOptions")));
        if read.as_deref() != Some(mode) {
            return Err(refusal(&format!(
                "{label}: the session did not confirm mode `{mode}` (it reports {}); no prompt \
                 was sent",
                read.map_or_else(|| "none".to_owned(), |value| format!("`{value}`"))
            )));
        }
        // The answer is the complete state: what was selected before must still hold.
        self.confirm_holds(answered.get("configOptions"))
    }
}
