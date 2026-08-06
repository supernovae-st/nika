// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session driver — ONE delegated run over any byte transport.
//!
//! [`drive`] takes a reader/writer pair (the spawned adapter's stdio in
//! production · a duplex pipe in tests), performs the Client-role
//! narrow waist (`initialize` → `session/new` → `session/prompt`),
//! routes incoming beats into the kernel's [`HarnessEvent`] stream, and
//! bridges `session/request_permission` through the seam's
//! [`PermissionReply`] under the fail-closed law: a reply DROPPED
//! unanswered answers the agent `cancelled` (Deny) — never a hang,
//! never a silent allow (A-5).
//!
//! Cancel safety: dropping the returned stream ends the run — every
//! event send fails once the receiver is gone and the driver task
//! returns, dropping the transport (B3.2's confined spawn adds
//! kill-on-drop on the child underneath).

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use nika_kernel::ai::harness::{
    HarnessError, HarnessEvent, HarnessEventStream, HarnessOutcome, HarnessRequest,
    PermissionDecision, PermissionReply,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::wire::{
    self, Incoming, InitializeParams, NewSessionParams, PermissionRequestIn, PermissionResult,
    PromptParams, PromptResult, SessionUpdateParams, TextBlock,
};

/// One incoming line may not exceed this (bounded reads · spec §4): a
/// hostile or broken peer overflows into a refusal, never an OOM.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

const ID_INITIALIZE: u64 = 1;
const ID_SESSION_NEW: u64 = 2;
const ID_PROMPT: u64 = 3;

/// Drive ONE delegated run over `reader`/`writer` — the stream is live
/// immediately; the handshake and session run inside the driver task.
pub fn drive<R, W>(reader: R, writer: W, request: HarnessRequest) -> HarnessEventStream
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (event_tx, event_rx) = mpsc::channel::<Result<HarnessEvent, HarnessError>>(64);
    tokio::spawn(async move {
        let mut driver = Driver {
            reader: BufReader::new(reader),
            writer,
            event_tx,
            output: String::new(),
        };
        if let Err(e) = driver.run(request).await {
            // The stream may already be dropped — best-effort final word.
            let _ = driver.event_tx.send(Err(e)).await;
        }
    });
    Box::pin(EventStream(event_rx))
}

struct EventStream(mpsc::Receiver<Result<HarnessEvent, HarnessError>>);

impl Stream for EventStream {
    type Item = Result<HarnessEvent, HarnessError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}

struct Driver<R, W> {
    reader: BufReader<R>,
    writer: W,
    event_tx: mpsc::Sender<Result<HarnessEvent, HarnessError>>,
    output: String,
}

impl<R, W> Driver<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    async fn run(&mut self, request: HarnessRequest) -> Result<(), HarnessError> {
        self.send_request(
            ID_INITIALIZE,
            wire::METHOD_INITIALIZE,
            &InitializeParams {
                protocol_version: wire::PROTOCOL_V1,
                client_capabilities: serde_json::json!({}),
            },
        )
        .await?;
        let init: wire::InitializeResult = self.await_response(ID_INITIALIZE, "initialize").await?;
        if init.protocol_version != wire::PROTOCOL_V1 {
            return Err(HarnessError::Refused {
                reason: format!(
                    "agent settled on protocol v{} — this client speaks v1 only \
                     (the schema-diff gate's runtime twin)",
                    init.protocol_version
                ),
            });
        }

        self.send_request(
            ID_SESSION_NEW,
            wire::METHOD_SESSION_NEW,
            &NewSessionParams {
                cwd: request.cwd.clone(),
                mcp_servers: Vec::new(),
            },
        )
        .await?;
        let session: wire::NewSessionResult =
            self.await_response(ID_SESSION_NEW, "session/new").await?;

        // The system prompt has no v1 seat outside session modes — B3.1
        // folds it ahead of the user text (the wrapped-fidelity class:
        // this access class never claims `exact` · seam doc).
        let text = match &request.system {
            Some(system) => format!("{system}\n\n{}", request.prompt),
            None => request.prompt.clone(),
        };
        self.send_request(
            ID_PROMPT,
            wire::METHOD_SESSION_PROMPT,
            &PromptParams {
                session_id: session.session_id.clone(),
                prompt: vec![TextBlock { kind: "text", text }],
            },
        )
        .await?;

        self.pump(&session.session_id, &request).await
    }

    /// The main pump: route beats until the prompt response closes the
    /// turn.
    async fn pump(
        &mut self,
        session_id: &str,
        request: &HarnessRequest,
    ) -> Result<(), HarnessError> {
        let (ptx, prx) = mpsc::unbounded_channel();
        let mut reply_rx = prx;
        loop {
            tokio::select! {
                line = read_bounded_line(&mut self.reader) => {
                    let line = line?;
                    match wire::parse_line(&line).map_err(|e| HarnessError::Session {
                        reason: e.to_string(),
                    })? {
                        Incoming::Response { id: ID_PROMPT, result } => {
                            let done: PromptResult = parse_payload(result, "session/prompt")?;
                            let outcome = self.close_turn(&done, request);
                            let _ = self
                                .event_tx
                                .send(Ok(HarnessEvent::Completed { outcome: Box::new(outcome) }))
                                .await;
                            return Ok(());
                        }
                        Incoming::ErrorResponse { message, .. } => {
                            return Err(HarnessError::Session { reason: message });
                        }
                        Incoming::Notification { method, params }
                            if method == wire::METHOD_SESSION_UPDATE =>
                        {
                            self.on_update(session_id, params).await?;
                        }
                        // A late/unknown correlation or a foreign
                        // notification mid-pump: observed, never fatal
                        // (the narrow waist has no other request in
                        // flight · reader leniency).
                        Incoming::Response { .. } | Incoming::Notification { .. } => {}
                        Incoming::Request { id, method, params } if method == wire::METHOD_REQUEST_PERMISSION => {
                            self.on_permission_ask(id, params, &ptx).await?;
                        }
                        Incoming::Request { id, .. } => {
                            // An unknown agent request refuses politely —
                            // JSON-RPC method-not-found keeps the wire honest.
                            let line = wire::response_line(
                                &id,
                                &serde_json::json!({}),
                            )
                            .map_err(session_err)?;
                            self.write_line(&line).await?;
                        }
                    }
                }
                Some((id, decision, options)) = reply_rx.recv() => {
                    let result = permission_outcome(decision, &options);
                    let line = wire::response_line(&id, &result).map_err(session_err)?;
                    self.write_line(&line).await?;
                }
            }
        }
    }

    async fn on_update(&mut self, session_id: &str, params: Value) -> Result<(), HarnessError> {
        let update: SessionUpdateParams = parse_payload(params, "session/update")?;
        if update.session_id != session_id {
            return Ok(()); // another session's beat — observed, never ours
        }
        if let Some(text) = wire::agent_chunk_text(&update.update) {
            self.output.push_str(&text);
            let _ = self
                .event_tx
                .send(Ok(HarnessEvent::MessageChunk { text }))
                .await;
        }
        Ok(())
    }

    async fn on_permission_ask(
        &mut self,
        id: Value,
        params: Value,
        ptx: &mpsc::UnboundedSender<(Value, PermissionDecision, Vec<wire::PermissionOptionIn>)>,
    ) -> Result<(), HarnessError> {
        let ask: PermissionRequestIn = parse_payload(params, "session/request_permission")?;
        let question = ask
            .tool_call
            .get("title")
            .and_then(Value::as_str)
            .map_or_else(
                || "the harness asked to perform an action".to_owned(),
                str::to_owned,
            );
        // The drop-reads-as-Deny guard: if the engine never answers,
        // the closure is DROPPED, the guard fires, the agent hears
        // `cancelled` — fail-closed, never a hang (A-5 twin).
        let reply = {
            let mut guard = ReplyGuard {
                tx: Some(ptx.clone()),
                id,
                options: ask.options,
            };
            PermissionReply::new(Box::new(move |decision| guard.answer(decision)))
        };
        let _ = self
            .event_tx
            .send(Ok(HarnessEvent::PermissionAsked { question, reply }))
            .await;
        Ok(())
    }

    fn close_turn(&mut self, done: &PromptResult, request: &HarnessRequest) -> HarnessOutcome {
        let mut outcome = HarnessOutcome::new(std::mem::take(&mut self.output));
        // The requested model is recorded by the CALLER's receipt; the
        // observed identity is absent on wire v1 (no seat) — absent
        // stays absent (A-7), never copied from the request.
        let _ = request;
        let _ = &done.stop_reason;
        outcome.observed_model = None;
        outcome
    }

    async fn send_request(
        &mut self,
        id: u64,
        method: &str,
        params: &impl serde::Serialize,
    ) -> Result<(), HarnessError> {
        let line = wire::request_line(id, method, params).map_err(session_err)?;
        self.write_line(&line).await
    }

    async fn write_line(&mut self, line: &str) -> Result<(), HarnessError> {
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| io_err(&e))?;
        self.writer.write_all(b"\n").await.map_err(|e| io_err(&e))?;
        self.writer.flush().await.map_err(|e| io_err(&e))
    }

    /// Await the response to `id`, routing any interleaved beat that
    /// arrives first (updates may precede a handshake response).
    async fn await_response<T: serde::de::DeserializeOwned>(
        &mut self,
        id: u64,
        what: &str,
    ) -> Result<T, HarnessError> {
        loop {
            let line = read_bounded_line(&mut self.reader).await?;
            match wire::parse_line(&line).map_err(session_err)? {
                Incoming::Response { id: got, result } if got == id => {
                    return parse_payload(result, what);
                }
                Incoming::ErrorResponse { id: got, message } if got == id => {
                    return Err(HarnessError::Refused { reason: message });
                }
                _ => {} // interleaved beats before the handshake settles
            }
        }
    }
}

/// The engine's verdict → the wire outcome. `AllowOnce` selects the
/// agent's `allow_once` option; `allow_always` is NEVER selected even
/// when it is the only allow (A-5 — the ask then cancels, fail-closed).
/// `Deny` always cancels.
fn permission_outcome(
    decision: PermissionDecision,
    options: &[wire::PermissionOptionIn],
) -> PermissionResult {
    match decision {
        PermissionDecision::AllowOnce => options
            .iter()
            .find(|o| o.kind == "allow_once")
            .map_or_else(PermissionResult::cancelled, |o| {
                PermissionResult::selected(&o.option_id)
            }),
        // Deny cancels — and a FUTURE decision variant refuses
        // conservatively through the same word (fail-closed).
        _ => PermissionResult::cancelled(),
    }
}

struct ReplyGuard {
    tx: Option<mpsc::UnboundedSender<(Value, PermissionDecision, Vec<wire::PermissionOptionIn>)>>,
    id: Value,
    options: Vec<wire::PermissionOptionIn>,
}

impl ReplyGuard {
    fn answer(&mut self, decision: PermissionDecision) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send((self.id.clone(), decision, std::mem::take(&mut self.options)));
        }
    }
}

impl Drop for ReplyGuard {
    fn drop(&mut self) {
        // Unanswered = Deny (fail-closed) — the agent hears `cancelled`.
        self.answer(PermissionDecision::Deny);
    }
}

fn session_err(e: impl std::fmt::Display) -> HarnessError {
    HarnessError::Session {
        reason: e.to_string(),
    }
}

fn io_err(e: &std::io::Error) -> HarnessError {
    HarnessError::Session {
        reason: format!("transport: {e}"),
    }
}

fn parse_payload<T: serde::de::DeserializeOwned>(v: Value, what: &str) -> Result<T, HarnessError> {
    serde_json::from_value(v).map_err(|e| HarnessError::Session {
        reason: format!("{what}: malformed payload: {e}"),
    })
}

/// Read one newline-terminated line, bounded at [`MAX_LINE_BYTES`] —
/// overflow and EOF are both session deaths with their own words.
async fn read_bounded_line<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<String, HarnessError> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let budget = (MAX_LINE_BYTES + 1).saturating_sub(buf.len());
        let mut limited = reader.take(budget as u64);
        let n = limited
            .read_until(b'\n', &mut buf)
            .await
            .map_err(|e| io_err(&e))?;
        if buf.last() == Some(&b'\n') {
            buf.pop();
            let line = String::from_utf8(buf).map_err(|_| HarnessError::Session {
                reason: "transport: line is not UTF-8".to_owned(),
            })?;
            return Ok(line);
        }
        if n == 0 {
            return Err(HarnessError::Session {
                reason: "transport: the agent closed the pipe mid-run".to_owned(),
            });
        }
        if buf.len() > MAX_LINE_BYTES {
            return Err(HarnessError::Session {
                reason: format!(
                    "transport: line exceeds the {MAX_LINE_BYTES}-byte bound (update overflow)"
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader as TestReader;

    /// Read one line from the scripted agent's side of the pipe.
    async fn agent_read(
        reader: &mut TestReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    ) -> Value {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .expect("agent reads a line");
        serde_json::from_str(line.trim_end()).expect("client lines are valid JSON")
    }

    async fn agent_write(writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, v: &Value) {
        let mut line = serde_json::to_string(v).expect("test JSON");
        line.push('\n');
        writer
            .write_all(line.as_bytes())
            .await
            .expect("agent writes");
    }

    /// The scripted handshake every dialogue opens with: assert the
    /// client's initialize + session/new, answer both, and return the
    /// prompt request it then sends.
    async fn script_handshake(
        reader: &mut TestReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
        writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
    ) -> Value {
        let init = agent_read(reader).await;
        assert_eq!(init["method"], "initialize");
        assert_eq!(init["params"]["protocolVersion"], 1);
        agent_write(
            writer,
            &serde_json::json!({"jsonrpc":"2.0","id":init["id"],
                "result":{"protocolVersion":1,"agentCapabilities":{}}}),
        )
        .await;

        let new = agent_read(reader).await;
        assert_eq!(new["method"], "session/new");
        assert!(new["params"]["cwd"].is_string());
        agent_write(
            writer,
            &serde_json::json!({"jsonrpc":"2.0","id":new["id"],
                "result":{"sessionId":"s-test"}}),
        )
        .await;

        let prompt = agent_read(reader).await;
        assert_eq!(prompt["method"], "session/prompt");
        assert_eq!(prompt["params"]["sessionId"], "s-test");
        prompt
    }

    fn chunk(text: &str) -> Value {
        serde_json::json!({"jsonrpc":"2.0","method":"session/update","params":{
            "sessionId":"s-test",
            "update":{"sessionUpdate":"agent_message_chunk",
                      "content":{"type":"text","text":text}}}})
    }

    async fn collect(mut stream: HarnessEventStream) -> (Vec<String>, Option<HarnessOutcome>) {
        use futures_core::Stream as _;
        let mut chunks = Vec::new();
        let mut outcome = None;
        // Poll the stream to completion via a tiny hand pump (no
        // StreamExt dep): poll_next in a poll_fn loop.
        loop {
            let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
            match next {
                Some(Ok(HarnessEvent::MessageChunk { text })) => chunks.push(text),
                Some(Ok(HarnessEvent::Completed { outcome: o })) => {
                    outcome = Some(*o);
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => panic!("stream error: {e}"),
                None => break,
            }
        }
        (chunks, outcome)
    }

    #[tokio::test]
    async fn a_happy_turn_streams_chunks_and_completes() {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (agent_read_half, agent_write_half) = tokio::io::split(theirs);

        let agent = tokio::spawn(async move {
            let mut r = TestReader::new(agent_read_half);
            let mut w = agent_write_half;
            let prompt = script_handshake(&mut r, &mut w).await;
            assert_eq!(prompt["params"]["prompt"][0]["type"], "text");
            assert_eq!(prompt["params"]["prompt"][0]["text"], "hello harness");
            agent_write(&mut w, &chunk("the mock ")).await;
            agent_write(&mut w, &chunk("heard you")).await;
            agent_write(
                &mut w,
                &serde_json::json!({"jsonrpc":"2.0","id":prompt["id"],
                    "result":{"stopReason":"end_turn"}}),
            )
            .await;
        });

        let stream = drive(
            client_read,
            client_write,
            HarnessRequest::new("hello harness", "/tmp"),
        );
        let (chunks, outcome) = collect(stream).await;
        agent.await.expect("scripted agent completes");
        assert_eq!(chunks, vec!["the mock ", "heard you"]);
        let outcome = outcome.expect("a completed turn carries its outcome");
        assert_eq!(outcome.output, "the mock heard you");
        assert!(
            outcome.usage.is_none(),
            "no usage seat on wire v1 — absent stays absent"
        );
    }

    #[tokio::test]
    async fn the_system_prompt_folds_ahead_of_the_user_text() {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (agent_read_half, agent_write_half) = tokio::io::split(theirs);

        let agent = tokio::spawn(async move {
            let mut r = TestReader::new(agent_read_half);
            let mut w = agent_write_half;
            let prompt = script_handshake(&mut r, &mut w).await;
            let text = prompt["params"]["prompt"][0]["text"]
                .as_str()
                .expect("text");
            assert!(text.starts_with("be terse\n\n"), "{text}");
            assert!(text.ends_with("do it"), "{text}");
            agent_write(
                &mut w,
                &serde_json::json!({"jsonrpc":"2.0","id":prompt["id"],
                    "result":{"stopReason":"end_turn"}}),
            )
            .await;
        });

        let stream = drive(
            client_read,
            client_write,
            HarnessRequest::new("do it", "/tmp").with_system("be terse"),
        );
        let (_, outcome) = collect(stream).await;
        agent.await.expect("scripted agent completes");
        assert!(outcome.is_some());
    }

    #[tokio::test]
    async fn a_permission_ask_bridges_allow_once_and_never_allow_always() {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (agent_read_half, agent_write_half) = tokio::io::split(theirs);

        let agent = tokio::spawn(async move {
            let mut r = TestReader::new(agent_read_half);
            let mut w = agent_write_half;
            let prompt = script_handshake(&mut r, &mut w).await;
            agent_write(
                &mut w,
                &serde_json::json!({"jsonrpc":"2.0","id":"ask-1",
                "method":"session/request_permission","params":{
                    "sessionId":"s-test",
                    "toolCall":{"title":"run `ls`"},
                    "options":[
                        {"optionId":"opt-always","name":"Always","kind":"allow_always"},
                        {"optionId":"opt-once","name":"Once","kind":"allow_once"}
                    ]}}),
            )
            .await;
            // The bridge must select the allow_once option — NEVER the
            // allow_always sitting first in the list (A-5).
            let answer = agent_read(&mut r).await;
            assert_eq!(answer["id"], "ask-1");
            assert_eq!(answer["result"]["outcome"]["outcome"], "selected");
            assert_eq!(answer["result"]["outcome"]["optionId"], "opt-once");
            agent_write(
                &mut w,
                &serde_json::json!({"jsonrpc":"2.0","id":prompt["id"],
                    "result":{"stopReason":"end_turn"}}),
            )
            .await;
        });

        let mut stream = drive(
            client_read,
            client_write,
            HarnessRequest::new("touch things", "/tmp"),
        );
        let mut outcome = None;
        loop {
            let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
            match next {
                Some(Ok(HarnessEvent::PermissionAsked { question, reply })) => {
                    assert_eq!(question, "run `ls`");
                    reply.respond(PermissionDecision::AllowOnce);
                }
                Some(Ok(HarnessEvent::Completed { outcome: o })) => outcome = Some(*o),
                Some(Ok(_)) => {}
                Some(Err(e)) => panic!("stream error: {e}"),
                None => break,
            }
        }
        agent.await.expect("scripted agent completes");
        assert!(outcome.is_some());
    }

    #[tokio::test]
    async fn an_unanswered_permission_reads_cancelled_fail_closed() {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (agent_read_half, agent_write_half) = tokio::io::split(theirs);

        let agent = tokio::spawn(async move {
            let mut r = TestReader::new(agent_read_half);
            let mut w = agent_write_half;
            let prompt = script_handshake(&mut r, &mut w).await;
            agent_write(
                &mut w,
                &serde_json::json!({"jsonrpc":"2.0","id":"ask-2",
                    "method":"session/request_permission","params":{
                        "sessionId":"s-test",
                        "options":[{"optionId":"o","name":"Allow","kind":"allow_once"}]}}),
            )
            .await;
            let answer = agent_read(&mut r).await;
            assert_eq!(answer["id"], "ask-2");
            assert_eq!(answer["result"]["outcome"]["outcome"], "cancelled");
            agent_write(
                &mut w,
                &serde_json::json!({"jsonrpc":"2.0","id":prompt["id"],
                    "result":{"stopReason":"cancelled"}}),
            )
            .await;
        });

        let mut stream = drive(
            client_read,
            client_write,
            HarnessRequest::new("try things", "/tmp"),
        );
        loop {
            let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
            match next {
                Some(Ok(HarnessEvent::PermissionAsked { reply, .. })) => {
                    // The engine never answers — DROPPING the reply must
                    // surface `cancelled` to the agent (fail-closed).
                    drop(reply);
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => panic!("stream error: {e}"),
                None => break,
            }
        }
        agent.await.expect("scripted agent observed the cancel");
    }

    #[tokio::test]
    async fn a_v2_agent_refuses_with_the_version_witness() {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (agent_read_half, agent_write_half) = tokio::io::split(theirs);

        tokio::spawn(async move {
            let mut r = TestReader::new(agent_read_half);
            let mut w = agent_write_half;
            let init = agent_read(&mut r).await;
            agent_write(
                &mut w,
                &serde_json::json!({"jsonrpc":"2.0","id":init["id"],
                    "result":{"protocolVersion":2}}),
            )
            .await;
        });

        let mut stream = drive(client_read, client_write, HarnessRequest::new("hi", "/tmp"));
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        let Some(Err(HarnessError::Refused { reason })) = next else {
            panic!("a v2 settlement must refuse, got {next:?}");
        };
        assert!(reason.contains("v2"), "{reason}");
        assert!(reason.contains("v1"), "{reason}");
    }

    #[tokio::test]
    async fn a_closed_pipe_is_a_session_death() {
        let (ours, theirs) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(ours);
        drop(theirs);
        let mut stream = drive(client_read, client_write, HarnessRequest::new("hi", "/tmp"));
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        assert!(
            matches!(next, Some(Err(HarnessError::Session { .. }))),
            "EOF mid-handshake is a session death, got {next:?}"
        );
    }
}
