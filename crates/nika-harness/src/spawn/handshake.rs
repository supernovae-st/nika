// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `initialize` handshake of a wrapper-class adapter (codex-acp,
//! claude-agent-acp …): these programs have no version flag, so their
//! identity is the `agentInfo` they report on `initialize`.
//!
//! Two doors read that answer. The standalone probe (`nika doctor`, the
//! access rows, a conversation) spawns the session argv, asks once and ends
//! the process ([`roundtrip`]). A run's session asks the same question as its
//! own first frame, so it lends the judgment to its driver instead of
//! spawning twice (`SpawnedHarness::start_judge`). Both meet
//! [`HarnessAdapter::judge_handshake`].
//!
//! [`roundtrip`]: crate::spawn::handshake::roundtrip

use nika_kernel::ai::harness::HarnessError;

use super::{HarnessAdapter, stop_probe};

impl HarnessAdapter {
    /// Judge an `initialize` RESULT (a v1 `InitializeResult`) against this row: the agent's
    /// self-report must name this adapter, and its version must sit inside the pin when the
    /// row declares one. The one judgment the standalone probe and a session's own first
    /// answer both meet.
    ///
    /// # Errors
    /// [`HarnessError::Unavailable`], naming the adapter: another program answered, the
    /// version is outside the pin, or a pinned row's answer carries none.
    pub(crate) fn judge_handshake(
        &self,
        initialize: &serde_json::Value,
    ) -> Result<Option<(u32, u32)>, HarnessError> {
        let version = initialize
            .pointer("/agentInfo/version")
            .and_then(serde_json::Value::as_str)
            .and_then(crate::probe::parse_version);
        let name = initialize
            .pointer("/agentInfo/name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if !self.answers_as(name) {
            let accepted = std::iter::once(self.id.as_str())
                .chain(self.identities.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("` · `");
            return Err(HarnessError::Unavailable {
                reason: format!(
                    "adapter `{}`: the handshake answered as `{name}` — this binary is not \
                     the adapter it claims to be (it answers as `{accepted}`)",
                    self.id
                ),
            });
        }
        match (&self.version_pin, version) {
            (Some(pin), Some((major, minor))) if pin.accepts(major, minor) => {
                Ok(Some((major, minor)))
            }
            (Some(pin), Some((major, minor))) => Err(HarnessError::Unavailable {
                reason: format!(
                    "adapter `{}`: handshake version {major}.{minor} is outside the pin \
                     (>= {}.{} · major <= {})",
                    self.id, pin.min.0, pin.min.1, pin.max_major
                ),
            }),
            (Some(_), None) => Err(HarnessError::Unavailable {
                reason: format!(
                    "adapter `{}`: the initialize answer carried no `agentInfo.version`",
                    self.id
                ),
            }),
            (None, seen) => Ok(seen),
        }
    }
}

/// One initialize roundtrip over the child's pipes (the probe's
/// transport half — write the request, read one bounded line).
pub(super) async fn roundtrip(
    mut cmd: tokio::process::Command,
) -> Result<serde_json::Value, HarnessError> {
    let mut child = cmd.spawn().map_err(|e| HarnessError::Unavailable {
        reason: format!("cannot spawn the adapter: {e}"),
    })?;
    let answer = ask(&mut child).await;
    // One answer is all a probe needs: the child — and the grandchild an
    // npm wrapper forked — end here, whatever the answer was.
    stop_probe(&mut child).await;
    answer
}

/// The initialize request written and one bounded line read.
async fn ask(child: &mut tokio::process::Child) -> Result<serde_json::Value, HarnessError> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let mut stdin = child.stdin.take().ok_or_else(|| HarnessError::Session {
        reason: "the child's stdin was not piped".to_owned(),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| HarnessError::Session {
        reason: "the child's stdout was not piped".to_owned(),
    })?;
    let line = crate::wire::request_line(
        1,
        crate::wire::METHOD_INITIALIZE,
        &crate::wire::InitializeParams {
            protocol_version: crate::wire::PROTOCOL_V1,
            client_capabilities: serde_json::json!({}),
        },
    )
    .map_err(|e| HarnessError::Session {
        reason: e.to_string(),
    })?;
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| HarnessError::Session {
            reason: format!("handshake write: {e}"),
        })?;
    stdin
        .write_all(b"\n")
        .await
        .map_err(|e| HarnessError::Session {
            reason: format!("handshake write: {e}"),
        })?;
    let mut line = String::new();
    let n = tokio::io::BufReader::new(stdout)
        .read_line(&mut line)
        .await
        .map_err(|e| HarnessError::Session {
            reason: format!("handshake read: {e}"),
        })?;
    if n == 0 {
        return Err(HarnessError::Unavailable {
            reason: "the adapter closed its pipe without answering initialize".to_owned(),
        });
    }
    serde_json::from_str(line.trim_end()).map_err(|e| HarnessError::Session {
        reason: format!("the initialize answer is not valid JSON: {e}"),
    })
}
