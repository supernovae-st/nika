// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The selected subscription seat as the agent that leads a Session conversation over ACP
//! (Claude Code; Codex rides the same door): one persistent agent session under the seat's
//! audited conversation profile, the registry row the person's choice names, the model and the
//! effort they selected. Its only tools are the Session's own, served over MCP by a tool server
//! this conversation opens on loopback with its own bearer and closes with it. The agent keeps
//! its history; each person's entry is one prompt; Stop asks the agent once (`session/cancel`).
//! Nothing falls back: a seat that cannot hold such a conversation refuses with its reason.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use nika_harness::{Conversation, ConversationSetup, ToolOffer, TurnEnd, TurnEvent, seat_from_id};
use nika_kernel::provider::ReasoningEffort;
use nika_mcp::conversation::{Closer, SERVER_NAME, ToolServer};
use nika_onboard::compile::AuthoringReasoning;
use nika_session_agent::{Beat, Conversant, Led, LedEnd};
use nika_session_change::tools::SessionTools;
use nika_types::access::HarnessTransport;
use serde_json::Value;

/// How often a turn asks whether to stop while its agent is silent.
const POLL: Duration = Duration::from_millis(10);

/// One conversation the selected seat's agent leads. Dropping it ends the agent session (its
/// process group with it) and closes the tool server.
pub struct SeatConversation {
    conversation: Option<Conversation>,
    closer: Option<Closer>,
    serving: Option<JoinHandle<()>>,
    runtime: Option<tokio::runtime::Runtime>,
    opening: Value,
    broken: bool,
}

/// Open the conversation the seat `seat` leads with the person's `model` (none: the agent's own)
/// and `effort`, serving `tools` to its agent alone.
///
/// # Errors
///
/// The seat has no audited conversation profile, the model or the effort cannot be asked of
/// it, the seat is unavailable or disabled, the tool server cannot start, or the agent's
/// handshake or selection fails; nothing falls back.
pub fn open(
    seat: &str,
    model: Option<&str>,
    effort: Option<AuthoringReasoning>,
    tools: Arc<dyn SessionTools>,
) -> Result<SeatConversation, String> {
    let wire = nika_harness::authoring::validate_selection(seat, model, HarnessTransport::Acp)?;
    let mut setup = ConversationSetup::new();
    if wire != "session" {
        setup = setup.with_requested_model(wire);
    }
    if let Some(level) = effort {
        let asked = ReasoningEffort::parse(level.word()).ok_or_else(|| {
            format!(
                "the reasoning effort `{}` has no level an agent can apply · nothing was sent",
                level.word()
            )
        })?;
        setup = setup.with_requested_effort(asked.word());
    }
    let harness = seat_from_id(seat)?
        .ok_or_else(|| format!("the seat `{seat}` is disabled on this machine"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("the conversation cannot start its runtime: {e}"))?;
    let server = ToolServer::start(tools).map_err(|e| e.to_string())?;
    let url = server.url().map_err(|e| e.to_string())?;
    let offer = ToolOffer::new(SERVER_NAME, server.tool_names()).with_http(url, server.bearer());
    let closer = server.closer().map_err(|e| e.to_string())?;
    let serving = std::thread::Builder::new()
        .name("nika-session-tools".to_owned())
        .spawn(move || server.serve())
        .map_err(|e| format!("the tool server cannot start its thread: {e}"))?;
    let mut opened = SeatConversation {
        conversation: None,
        closer: Some(closer),
        serving: Some(serving),
        runtime: None,
        opening: Value::Null,
        broken: false,
    };
    let conversation =
        (runtime.block_on(harness.converse(setup, offer))).map_err(|e| e.to_string());
    opened.runtime = Some(runtime);
    let conversation = conversation?;
    opened.opening = conversation.record().clone();
    opened.conversation = Some(conversation);
    Ok(opened)
}

impl SeatConversation {
    /// The opening record: the profile, the transport chosen, the server mounted, the selection
    /// applied. No endpoint and no secret are in it.
    #[must_use]
    pub const fn opening(&self) -> &Value {
        &self.opening
    }
}

impl Conversant for SeatConversation {
    fn prompt(
        &mut self,
        text: &str,
        beats: &mut dyn FnMut(Beat),
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Led, String> {
        let (Some(conversation), Some(runtime)) = (&self.conversation, &self.runtime) else {
            return Err("the conversation with the agent has ended".to_owned());
        };
        let mut turn = match conversation.prompt(text) {
            Ok(turn) => turn,
            Err(error) => {
                self.broken = true;
                return Err(error.to_string());
            }
        };
        let led = runtime.block_on(async {
            let mut asked = false;
            loop {
                if !asked && stop() {
                    conversation.stop();
                    asked = true;
                }
                tokio::select! {
                    beat = turn.next_beat() => match beat {
                        None => return Err("the agent's turn ended without its record".to_owned()),
                        Some(Err(error)) => return Err(error.to_string()),
                        Some(Ok(TurnEvent::Ended { end, record })) => {
                            return Ok(Led::new(led_end(end), record));
                        }
                        Some(Ok(TurnEvent::Answer { text })) => beats(Beat::Answer(text)),
                        Some(Ok(TurnEvent::Thought { text })) => beats(Beat::Thought(text)),
                        Some(Ok(_)) => {}
                    },
                    () = tokio::time::sleep(POLL) => {}
                }
            }
        });
        // A transport failure ends the conversation: it takes no further prompt.
        self.broken |= led.is_err();
        led
    }

    fn ended(&self) -> bool {
        self.broken || self.conversation.is_none()
    }
}

/// A turn's end in the loop's words.
fn led_end(end: TurnEnd) -> LedEnd {
    match end {
        TurnEnd::EndTurn => LedEnd::Answered,
        TurnEnd::Cancelled => LedEnd::Stopped,
        other => LedEnd::Other(other.as_str().to_owned()),
    }
}

impl Drop for SeatConversation {
    fn drop(&mut self) {
        // The agent session first (no call reaches the server after it), then the server.
        drop(self.conversation.take());
        if let Some(runtime) = self.runtime.take() {
            // The driver and the agent's process group end with the runtime's tasks; a host
            // may drop a Session from an async context, where a blocking drop would panic.
            runtime.shutdown_background();
        }
        if let Some(closer) = self.closer.take() {
            closer.close();
        }
        if let Some(serving) = self.serving.take() {
            let _ = serving.join();
        }
    }
}
