// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation driver: one ACP session kept open across turns over any byte transport
//! (the spawned adapter's stdio, a duplex pipe in tests). It reuses the one-shot driver's
//! handshake, admission, selection and bounded frame reads and writes, then serves its handle:
//! one `session/prompt` per entry, `session/cancel` once per Stop, every permission answered by
//! the conversation's own law (`crate::conversation`), each turn's beats and record sent to the
//! turn's stream.
//!
//! Between turns the agent may stay silent for as long as the person takes. During a turn, the
//! turn's silence allowance applies while no tool call is open, and again once Stop was sent;
//! the allowance re-arms on each frame showing the agent working. A transport failure, a line
//! that is no message or a turn that outlives its allowance ends the conversation: the turn in
//! flight receives the error and every later prompt is refused.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nika_kernel::ai::harness::{HarnessError, HarnessRequest, HarnessSelection};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio::sync::{mpsc, oneshot};

use super::{
    Driver, ID_INITIALIZE, ID_SESSION_NEW, answered_error, parse_payload, read_bounded_line,
    session_err,
};
use crate::authoring::acp::{
    self, Completion, Deadline, Milestone, OneShot, Phase, Profile, Progress, activity, conclude,
};
use crate::conversation::{
    self as law, Command, Conversation, ConversationSetup, ToolOffer, Transport, TurnEnd, TurnEvent,
};
use crate::wire::{self, Incoming};

/// The first turn's request id: clear of the handshake's.
const ID_TURNS: u64 = 100;

/// How long a read waits between turns: the person's time, never a transport bound.
const BETWEEN_TURNS: Duration = Duration::from_secs(365 * 24 * 60 * 60);

/// What the conversation is opened with.
pub(crate) struct Opening {
    pub(crate) setup: ConversationSetup,
    pub(crate) offer: ToolOffer,
    pub(crate) profile: Profile,
    /// The session's root: the conversation's own scratch directory.
    pub(crate) cwd: PathBuf,
}

/// Open the conversation over `reader`/`writer` and hand back its handle once `session/new`
/// and the selection are done. `keep` lives as long as the driver (the adapter process).
///
/// # Errors
/// The admission, the transport choice, the handshake or the selection failed.
pub(crate) async fn open<R, W, K>(
    reader: R,
    writer: W,
    opening: Opening,
    keep: K,
) -> Result<Conversation, HarnessError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
    K: Send + 'static,
{
    let (opened, handed) = oneshot::channel();
    let (commands, inbox) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let Opening {
            setup,
            offer,
            profile,
            cwd,
        } = opening;
        // The handshake sends no beat: its event lane goes nowhere.
        let (event_tx, _) = mpsc::channel(1);
        let one_shot = OneShot {
            role: Completion::Conversation,
            profile,
        };
        let mut driver = Driver {
            reader: BufReader::new(reader),
            writer,
            event_tx,
            output: String::new(),
            idle: setup.allowance,
            pending: Vec::new(),
            observed_model: None,
            observed_source: None,
            selection: HarnessSelection::default(),
            media: crate::media::MediaState::default(),
            completion: Some(one_shot),
            progress: None,
            meta: None,
        };
        match driver.handshake(&setup, &offer, one_shot, &cwd).await {
            Err(error) => {
                let _ = opened.send(Err(error));
            }
            Ok((session, handle)) => {
                if opened.send(Ok(handle)).is_ok() {
                    driver.completion = None;
                    driver.converse(&session, &offer, inbox).await;
                }
            }
        }
        // What the driver kept alive (the adapter process, its scratch) ends with it.
        drop(keep);
    });
    let (transport, record) = handed.await.map_err(|_| law::ended())??;
    Ok(Conversation::new(commands, transport, record))
}

/// The turn in flight.
struct Turn {
    id: u64,
    events: mpsc::Sender<Result<TurnEvent, HarnessError>>,
    progress: Progress,
    deadline: Deadline,
    /// The Stops received, and whether `session/cancel` went out.
    stops: u32,
    cancel_sent: bool,
    /// The agent's tool calls begun and not yet completed or failed.
    open: BTreeSet<String>,
    allowed: u32,
    denied: Vec<String>,
}

impl Turn {
    fn new(id: u64, events: mpsc::Sender<Result<TurnEvent, HarnessError>>, idle: Duration) -> Self {
        Self {
            id,
            events,
            progress: Progress::default(),
            deadline: Deadline::start(idle),
            stops: 0,
            cancel_sent: false,
            open: BTreeSet::new(),
            allowed: 0,
            denied: Vec::new(),
        }
    }

    /// The silence the turn is held to now: none while a tool call is open, unless Stop was sent.
    fn silence(&self) -> Option<(Deadline, Progress)> {
        (self.open.is_empty() || self.cancel_sent).then(|| (self.deadline, self.progress.clone()))
    }

    /// A tool call began, changed or ended, as the agent reported its status.
    fn track(&mut self, update: &Value) {
        let Some(id) = update.get("toolCallId").and_then(Value::as_str) else {
            return;
        };
        let status = update.get("status").and_then(Value::as_str);
        let first = update.get("sessionUpdate").and_then(Value::as_str) == Some("tool_call");
        // A call reported without a status is pending (the protocol's default); an update
        // without one changes nothing.
        match status {
            Some("completed" | "failed") => {
                self.open.remove(id);
            }
            Some(_) => {
                self.open.insert(id.to_owned());
            }
            None => {
                if first {
                    self.open.insert(id.to_owned());
                }
            }
        }
    }

    async fn send(&self, event: TurnEvent) {
        let _ = self.events.send(Ok(event)).await;
    }

    async fn end(self, result: &Value) {
        let end = TurnEnd::of(result.get("stopReason").and_then(Value::as_str));
        self.progress.reach(Phase::Completion);
        activity::completed(Some(&self.progress));
        let stop = json!({"requested": self.stops, "cancel_sent": self.cancel_sent});
        let permissions = json!({"allowed": self.allowed, "denied": self.denied});
        let record = json!({"profile": "conversation", "turn": self.id - ID_TURNS,
            "end": end.as_str(), "stop": stop, "permissions": permissions});
        let record = conclude(record, &self.progress, self.deadline);
        self.send(TurnEvent::Ended { end, record }).await;
    }

    async fn fail(self, error: HarnessError) {
        activity::failed(Some(&self.progress));
        let _ = self.events.send(Err(error)).await;
    }
}

/// Resolves once the turn kept its silence for a whole allowance; never without one.
async fn quiet(silence: Option<(Deadline, Progress)>) {
    match silence {
        Some((deadline, progress)) => deadline.passed(&progress).await,
        None => std::future::pending().await,
    }
}

impl<R, W> Driver<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// `initialize`, the admission and the transport, `session/new` with exactly Nika's server
    /// and the conversation profile, then the selection `setup` asks.
    async fn handshake(
        &mut self,
        setup: &ConversationSetup,
        offer: &ToolOffer,
        one_shot: OneShot,
        cwd: &Path,
    ) -> Result<(String, (Transport, Value)), HarnessError> {
        let hello = wire::InitializeParams {
            protocol_version: wire::PROTOCOL_V1,
            client_capabilities: json!({}),
        };
        (self.send_request(ID_INITIALIZE, wire::METHOD_INITIALIZE, &hello)).await?;
        let init: Value = self.await_response(ID_INITIALIZE, "initialize").await?;
        acp::admit(&init, one_shot)?;
        let profile = one_shot.profile;
        let http = profile == Profile::Codex || law::advertises_http(&init);
        let transport = law::choose(http, offer)?;
        let new = wire::NewSessionParams {
            cwd: cwd.to_path_buf(),
            mcp_servers: law::mounts(offer, transport, profile),
        };
        let mut params = serde_json::to_value(new).map_err(session_err)?;
        if profile == Profile::ClaudeCode {
            params["_meta"] = acp::conversation_profile();
        }
        (self.send_request(ID_SESSION_NEW, wire::METHOD_SESSION_NEW, &params)).await?;
        let session: wire::NewSessionResult =
            self.await_response(ID_SESSION_NEW, "session/new").await?;
        let mut request = HarnessRequest::new(String::new(), cwd)
            .with_requested_effort(setup.requested_effort.clone());
        if let Some(model) = &setup.requested_model {
            request = request.with_requested_model(model.clone());
        }
        self.seat_session(&session, &request).await?;
        let seat = (&self.selection, self.observed_model.as_deref());
        let record = law::opening(&init, offer, transport, profile, seat);
        Ok((session.session_id, (transport, record)))
    }

    /// Serve the handle until it is dropped or the conversation fails.
    async fn converse(
        &mut self,
        session: &str,
        offer: &ToolOffer,
        mut inbox: mpsc::UnboundedReceiver<Command>,
    ) {
        let mut turn: Option<Turn> = None;
        let mut turns = 0;
        loop {
            let silence = turn.as_ref().and_then(Turn::silence);
            // The handle first: a Stop or a prompt is never kept waiting behind a frame.
            let step = tokio::select! {
                biased;
                command = inbox.recv() => match command {
                    None => return,
                    Some(Command::Prompt { text, events }) if turn.is_none() => {
                        turns += 1;
                        let next = turn.insert(Turn::new(ID_TURNS + turns, events, self.idle));
                        self.prompt(session, next, text).await
                    }
                    Some(Command::Prompt { events, .. }) => {
                        let _ = events.try_send(Err(busy()));
                        Ok(())
                    }
                    Some(Command::Stop) => match turn.as_mut() {
                        Some(stopped) => self.cancel(session, stopped).await,
                        None => Ok(()),
                    },
                },
                line = read_bounded_line(&mut self.reader, &mut self.pending, BETWEEN_TURNS) => {
                    self.frame(session, offer, &mut turn, line).await
                }
                () = quiet(silence) => Err(silent(self.idle)),
            };
            if let Err(error) = step {
                if let Some(failed) = turn.take() {
                    failed.fail(error).await;
                }
                return;
            }
        }
    }

    /// Write the turn's prompt; its activity window opens once the line is flushed.
    async fn prompt(
        &mut self,
        session: &str,
        turn: &mut Turn,
        text: String,
    ) -> Result<(), HarnessError> {
        turn.progress.opened(self.idle);
        self.progress = Some(turn.progress.clone());
        let params = wire::PromptParams {
            session_id: session.to_owned(),
            prompt: vec![wire::TextBlock { kind: "text", text }],
        };
        (self.send_request(turn.id, wire::METHOD_SESSION_PROMPT, &params)).await?;
        turn.progress.mark(Milestone::PromptWritten);
        Ok(())
    }

    /// Stop the turn: `session/cancel` once, every Stop counted.
    async fn cancel(&mut self, session: &str, turn: &mut Turn) -> Result<(), HarnessError> {
        turn.stops = turn.stops.saturating_add(1);
        if turn.cancel_sent {
            return Ok(());
        }
        let params = wire::CancelParams {
            session_id: session.to_owned(),
        };
        let line =
            wire::notification_line(wire::METHOD_SESSION_CANCEL, &params).map_err(session_err)?;
        self.write_line(&line).await?;
        turn.cancel_sent = true;
        Ok(())
    }

    /// Route one frame: the turn's answer or error, an update, a permission, any other request.
    async fn frame(
        &mut self,
        session: &str,
        offer: &ToolOffer,
        turn: &mut Option<Turn>,
        line: Result<String, HarnessError>,
    ) -> Result<(), HarnessError> {
        let read = line.map(|line| wire::parse_line(&line));
        if let Some(current) = turn.as_ref() {
            activity::received(Some(&current.progress), &read, session, current.id);
        }
        let current = turn.as_ref().map(|current| current.id);
        match read?.map_err(session_err)? {
            Incoming::Response { id, result } if current == Some(id) => {
                if let Some(done) = turn.take() {
                    done.end(&result).await;
                }
            }
            Incoming::ErrorResponse { id, message, kind } if current == Some(id) => {
                if let Some(failed) = turn.take() {
                    failed.fail(answered_error(message, kind.as_deref())).await;
                }
            }
            Incoming::Notification { method, params } if method == wire::METHOD_SESSION_UPDATE => {
                self.update(session, offer, turn.as_mut(), params).await?;
            }
            Incoming::Request { id, method, params }
                if method == wire::METHOD_REQUEST_PERMISSION =>
            {
                self.permission(session, offer, turn.as_mut(), &id, params)
                    .await?;
            }
            Incoming::Request { id, method, .. } => {
                // No client capability was advertised: any other request is refused.
                let error =
                    json!({"code": -32601, "message": format!("method not found: {method}")});
                let line = json!({"jsonrpc": "2.0", "id": id, "error": error}).to_string();
                self.write_line(&line).await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// One update of this session: answer and thought text, tool frames; the rest is activity.
    async fn update(
        &mut self,
        session: &str,
        offer: &ToolOffer,
        turn: Option<&mut Turn>,
        params: Value,
    ) -> Result<(), HarnessError> {
        let update: wire::SessionUpdateParams = parse_payload(params, "session/update")?;
        if update.session_id != session {
            return Ok(());
        }
        let update = update.update;
        let kind = update.get("sessionUpdate").and_then(Value::as_str);
        if kind == Some("config_option_update") {
            self.observe_config_update(&update);
        }
        let Some(turn) = turn else {
            return Ok(());
        };
        let read_text = || update.pointer("/content/text").and_then(Value::as_str);
        let text = (update.pointer("/content/type").and_then(Value::as_str) == Some("text"))
            .then(read_text)
            .flatten()
            .map(str::to_owned);
        let event = match (kind, text) {
            (Some("agent_message_chunk"), Some(text)) => {
                turn.progress.reach(Phase::Answer);
                Some(TurnEvent::Answer { text })
            }
            (Some("agent_thought_chunk"), Some(text)) => Some(TurnEvent::Thought { text }),
            (Some("tool_call" | "tool_call_update"), _) => {
                turn.track(&update);
                (update.get("toolCallId").and_then(Value::as_str)).map(|id| TurnEvent::Tool {
                    tool_call_id: id.to_owned(),
                    status: update
                        .get("status")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    nika_tool: law::nika_tool(&update, offer),
                })
            }
            _ => None,
        };
        if let Some(event) = event {
            turn.send(event).await;
        }
        Ok(())
    }

    /// Answer a permission by the conversation's law: one of Nika's offered tools is allowed
    /// once, anything else (another session's ask, an ask outside a turn) is rejected; both
    /// are told to the turn and recorded.
    async fn permission(
        &mut self,
        session: &str,
        offer: &ToolOffer,
        turn: Option<&mut Turn>,
        id: &Value,
        params: Value,
    ) -> Result<(), HarnessError> {
        let ask: wire::PermissionRequestIn = parse_payload(params, "session/request_permission")?;
        let nika_tool = law::nika_tool(&ask.tool_call, offer);
        let allowed = nika_tool.is_some() && ask.session_id == session && turn.is_some();
        let result = law::answer(allowed, &ask.options);
        let line = wire::response_line(id, &result).map_err(session_err)?;
        self.write_line(&line).await?;
        let Some(turn) = turn else {
            return Ok(());
        };
        let call = ask.tool_call.get("toolCallId").and_then(Value::as_str);
        let tool = law::asked_tool(&ask.tool_call);
        if allowed {
            turn.allowed = turn.allowed.saturating_add(1);
            turn.open.extend(call.map(str::to_owned));
        } else {
            turn.denied
                .push(tool.clone().unwrap_or_else(|| "(unnamed)".to_owned()));
            if let Some(call) = call {
                turn.open.remove(call);
            }
        }
        let decided = TurnEvent::Permission {
            tool,
            nika_tool,
            allowed,
        };
        turn.send(decided).await;
        Ok(())
    }
}

fn busy() -> HarnessError {
    HarnessError::Session {
        reason: "a turn is already in flight in this ACP conversation; send the next entry \
                 after it ends (or Stop it)"
            .to_owned(),
    }
}

fn silent(allowance: Duration) -> HarnessError {
    HarnessError::Session {
        reason: format!(
            "the agent sent nothing for {}s during a turn with no tool call open (the turn's \
             silence allowance); the ACP conversation is abandoned",
            allowance.as_secs_f32()
        ),
    }
}

#[cfg(test)]
mod tests;
