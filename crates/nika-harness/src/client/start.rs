// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's own `initialize` — the one handshake of a session.
//!
//! A wrapper-class adapter (codex-acp, claude-agent-acp …) has no version
//! flag: its identity is the `agentInfo` it reports on `initialize`. A
//! spawner used to learn it from a standalone probe, which launched the
//! session argv, asked `initialize` and ended the process, before launching
//! the same argv again for the session. The session asks the same question
//! as its first frame, so the spawner now lends its judgment
//! ([`StartJudge`]) and the answer is judged here, before `session/new`:
//! identity before dialect, on one process.
//!
//! A judged start keeps the standalone probe's failure classes, so a task's
//! retry policy reads the same facts as before. An adapter that does not
//! answer within [`START_TIMEOUT`], closes its pipe before answering,
//! answers as another program or sits outside its pin is
//! [`HarnessError::Unavailable`], which no retry heals. A transport error
//! stays a session death. Without a judge (a lent transport, a scripted
//! peer) nothing changes: the answer is awaited under the idle deadline.
//!
//! [`START_TIMEOUT`]: crate::client::start::START_TIMEOUT

use std::time::Duration;

use nika_kernel::ai::harness::{HarnessError, HarnessRequest};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};

use super::{Driver, ID_INITIALIZE, PIPE_CLOSED, parse_payload, prompt};
use crate::authoring::acp::Milestone;
use crate::wire::{self, InitializeParams};

/// How long a judged session waits for its `initialize` answer: the bound the
/// standalone probe gave the same question (a cold wrapper resolve fits in it;
/// a hung adapter is unavailable, never a hang).
pub(crate) const START_TIMEOUT: Duration = Duration::from_secs(30);

/// A judgment of an `initialize` result: admitted, or the refusal.
type Judgment = dyn Fn(&Value) -> Result<(), HarnessError> + Send;

/// The judgment a spawner lends the session it opens: the adapter it
/// launched, and what that adapter's `initialize` answer must say.
pub(crate) struct StartJudge {
    adapter: String,
    judge: Box<Judgment>,
}

impl StartJudge {
    /// Judge the `initialize` result of the adapter `adapter` with `judge`.
    pub(crate) fn new(
        adapter: impl Into<String>,
        judge: impl Fn(&Value) -> Result<(), HarnessError> + Send + 'static,
    ) -> Self {
        Self {
            adapter: adapter.into(),
            judge: Box::new(judge),
        }
    }

    fn unavailable(&self, what: &str) -> HarnessError {
        HarnessError::Unavailable {
            reason: format!("adapter `{}`: {what}", self.adapter),
        }
    }
}

impl<R, W> Driver<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Send `initialize`, await its answer and admit the agent: the
    /// spawner's judgment when it lent one, the completion profile's own
    /// identity, the image prompts the request needs and protocol v1. The
    /// adapter's name and release are kept for the measured rows that key
    /// on them.
    pub(super) async fn initialize(
        &mut self,
        request: &HarnessRequest,
    ) -> Result<(), HarnessError> {
        self.send_request(
            ID_INITIALIZE,
            wire::METHOD_INITIALIZE,
            &InitializeParams {
                protocol_version: wire::PROTOCOL_V1,
                client_capabilities: wire::client_capabilities(),
            },
        )
        .await?;
        let value = self.initialize_answer().await?;
        if let Some(completion) = self.completion {
            crate::authoring::acp::admit(&value, completion)?;
        }
        prompt::admit_images(&value, &request.images)?;
        self.record_agent(&value);
        let init: wire::InitializeResult = parse_payload(value, "initialize")?;
        if init.protocol_version != wire::PROTOCOL_V1 {
            return Err(HarnessError::Refused {
                reason: format!(
                    "agent settled on protocol v{} — this client speaks v1 only \
                     (the schema-diff gate's runtime twin)",
                    init.protocol_version
                ),
            });
        }
        Milestone::InitializeAccepted.reached(self.progress.as_ref());
        Ok(())
    }

    /// The `initialize` result, judged by the spawner when it lent a judge.
    async fn initialize_answer(&mut self) -> Result<Value, HarnessError> {
        let Some(start) = self.start.take() else {
            return self.await_response(ID_INITIALIZE, "initialize").await;
        };
        let bound = START_TIMEOUT.min(self.idle);
        let answer = self.await_response::<Value>(ID_INITIALIZE, "initialize");
        let value = match tokio::time::timeout(bound, answer).await {
            Err(_) => {
                return Err(start.unavailable(&format!(
                    "the initialize handshake did not answer in {}s",
                    bound.as_secs()
                )));
            }
            Ok(Err(HarnessError::Session { reason })) if reason == PIPE_CLOSED => {
                return Err(
                    start.unavailable("the adapter closed its pipe without answering initialize")
                );
            }
            Ok(answered) => answered?,
        };
        (start.judge)(&value)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
