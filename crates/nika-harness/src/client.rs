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
///
/// **This is a TRANSPORT bound and it is not the forensics decode
/// grain**, even though both are 1 MiB today. It guards one line off a
/// live wire from a peer this process is talking to; `nika_dap`'s
/// `bounded::MAX_ARTIFACT_BYTES` guards a stored artifact a verifier
/// reads whole. Two readers, two threat models, two bounds that happen
/// to agree on a number.
///
/// Do not "de-duplicate" them onto one constant. A grep finds three
/// `1024 * 1024` in this tree and reads like a single value restated
/// three times; it is not (measured 2026-08-13 · nika-spec
/// `conformance/FINDINGS.md` F-4). Aliasing this to a forensics
/// constant would couple a wire protocol to a decoder, so raising one
/// bound would move the other for no stated reason — the coupling
/// costs more than the repetition. The one real duplicate is
/// `nika-registry-client`'s own artifact bound, which shares this
/// crate's number *and* `nika-dap`'s meaning.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// How long the driver waits for the NEXT byte before calling the
/// session dead. A size bound alone leaves the wedge the refuter
/// found (2026-08-06): a peer that writes half a line and goes silent
/// without closing its pipe hangs the driver forever — no EOF, no
/// overflow, no newline. Agent turns are long (a harness may think for
/// minutes), so the bound is on SILENCE between bytes, never on the
/// turn: any progress resets it.
pub const IDLE_TIMEOUT_SECS: u64 = 300;

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
    drive_with_idle(
        reader,
        writer,
        request,
        std::time::Duration::from_secs(IDLE_TIMEOUT_SECS),
    )
}

/// [`drive`] with an explicit idle deadline — the seam the wedge tests
/// drive at millisecond scale (a five-minute constant cannot be proven
/// by a test that must finish; virtual time does not compose with the
/// spawned driver task).
pub fn drive_with_idle<R, W>(
    reader: R,
    writer: W,
    request: HarnessRequest,
    idle: std::time::Duration,
) -> HarnessEventStream
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
            idle,
            pending: Vec::new(),
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
    /// How long a silence may last before the session is abandoned.
    idle: std::time::Duration,
    /// The partial-line accumulator (see [`read_bounded_line`]'s
    /// cancel-safety note): it MUST outlive any single read future,
    /// because a `select!` may drop that future mid-line.
    pending: Vec<u8>,
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
                line = read_bounded_line(&mut self.reader, &mut self.pending, self.idle) => {
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
        // B5: the judgeable facts ride beside the question (kind ·
        // locations · command · url — the wire's own words, absent when
        // the agent did not speak them).
        let (kind, locations, command, url) = wire::ask_facts(&ask.tool_call);
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
            .send(Ok(HarnessEvent::PermissionAsked {
                question,
                reply,
                kind,
                locations,
                command,
                url,
            }))
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

    /// Write one line — deadlined like the read half: a peer that
    /// stops READING its stdin fills the pipe and blocks us here, and
    /// a driver blocked in `write_all` is not reading either (the
    /// mutual wedge · review 2026-08-06).
    async fn write_line(&mut self, line: &str) -> Result<(), HarnessError> {
        let idle = self.idle;
        let stalled = || HarnessError::Session {
            reason: format!(
                "transport: the agent stopped reading its input for {}s \
                 (write deadline) — the session is abandoned",
                idle.as_secs_f32()
            ),
        };
        tokio::time::timeout(idle, self.writer.write_all(line.as_bytes()))
            .await
            .map_err(|_| stalled())?
            .map_err(|e| io_err(&e))?;
        tokio::time::timeout(idle, self.writer.write_all(b"\n"))
            .await
            .map_err(|_| stalled())?
            .map_err(|e| io_err(&e))?;
        tokio::time::timeout(idle, self.writer.flush())
            .await
            .map_err(|_| stalled())?
            .map_err(|e| io_err(&e))
    }

    /// Await the response to `id`, routing any interleaved beat that
    /// arrives first (updates may precede a handshake response).
    async fn await_response<T: serde::de::DeserializeOwned>(
        &mut self,
        id: u64,
        what: &str,
    ) -> Result<T, HarnessError> {
        loop {
            let line = read_bounded_line(&mut self.reader, &mut self.pending, self.idle).await?;
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
///
/// CANCEL SAFETY, the load-bearing detail: `read_until` is only
/// conditionally cancel-safe — tokio's own contract says partial bytes
/// are appended to `buf` and the call may be resumed, which holds ONLY
/// if the CALLER owns `buf` across the cancellation. This future is a
/// `select!` branch (the reply lane can win the race), so the
/// accumulator lives on the DRIVER, not here: a future-local buffer
/// dropped mid-line silently ate bytes already consumed out of the
/// `BufReader`, and the next read started mid-frame — truncation
/// reported as the peer's invalid JSON (found by review 2026-08-06,
/// verified against `tokio::io::AsyncBufReadExt::read_until` docs).
async fn read_bounded_line<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    buf: &mut Vec<u8>,
    idle: std::time::Duration,
) -> Result<String, HarnessError> {
    loop {
        let budget = (MAX_LINE_BYTES + 1).saturating_sub(buf.len());
        let mut limited = reader.take(budget as u64);
        // The idle deadline (the anti-wedge · refuted 2026-08-06): the
        // size bound alone let a peer write half a line and go silent
        // WITHOUT closing its pipe — no EOF, no overflow, no newline,
        // a driver hung forever. Each read waits at most `idle` for
        // progress; any byte resets it, so a long thinking turn is
        // never killed, only a dead one.
        let n = tokio::time::timeout(idle, limited.read_until(b'\n', buf))
            .await
            .map_err(|_| HarnessError::Session {
                reason: format!(
                    "transport: the agent sent nothing for {}s (idle deadline) \
                     — the session is abandoned",
                    idle.as_secs_f32()
                ),
            })?
            .map_err(|e| io_err(&e))?;
        if buf.last() == Some(&b'\n') {
            buf.pop();
            let line =
                String::from_utf8(std::mem::take(buf)).map_err(|_| HarnessError::Session {
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
                    "toolCall":{"title":"run `ls`","kind":"execute",
                                "rawInput":{"command":["ls"]}},
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
                Some(Ok(HarnessEvent::PermissionAsked {
                    question,
                    reply,
                    kind,
                    locations,
                    command,
                    ..
                })) => {
                    assert_eq!(question, "run `ls`");
                    assert_eq!(kind.as_deref(), Some("execute"));
                    assert_eq!(command, vec!["ls"]);
                    assert!(locations.is_empty());
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

#[cfg(test)]
mod wedge_tests {
    use super::*;
    use nika_kernel::ai::harness::HarnessRequest;
    const FAST_IDLE: std::time::Duration = std::time::Duration::from_millis(80);

    /// The refuter's counterexample (2026-08-06), pinned: a peer that
    /// writes half a line and goes silent WITHOUT closing its pipe used
    /// to hang the driver forever (a size bound, no time bound).
    #[tokio::test]
    async fn a_silent_partial_line_ends_at_the_idle_deadline() {
        let (ours, theirs) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(ours);
        let (_agent_read, mut agent_write) = tokio::io::split(theirs);
        agent_write
            .write_all(b"{\"jsonrpc\":\"2.0\"")
            .await
            .expect("partial write");

        let mut stream = drive_with_idle(
            client_read,
            client_write,
            HarnessRequest::new("hi", "/tmp"),
            FAST_IDLE,
        );
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        let Some(Err(HarnessError::Session { reason })) = next else {
            panic!("the idle deadline must end a silent peer, got {next:?}");
        };
        assert!(reason.contains("idle deadline"), "{reason}");
        drop(agent_write); // the pipe stayed OPEN for the whole wait
    }

    /// Progress resets the deadline: an agent that answers slowly (each
    /// beat inside the window) is never killed mid-turn.
    #[tokio::test]
    async fn progress_resets_the_idle_deadline() {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (agent_read, mut agent_write) = tokio::io::split(theirs);

        let agent = tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt as _;
            let mut r = tokio::io::BufReader::new(agent_read);
            let mut line = String::new();
            let answer = async |w: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, v: Value| {
                let mut s = serde_json::to_string(&v).expect("json");
                s.push('\n');
                w.write_all(s.as_bytes()).await.expect("write");
            };
            for step in 0..3 {
                // Each beat lands at 60% of the window: three gaps in a
                // row, none fatal — the reset is what proves it.
                tokio::time::sleep(FAST_IDLE.mul_f32(0.6)).await;
                line.clear();
                r.read_line(&mut line).await.expect("read");
                let req: Value = serde_json::from_str(line.trim_end()).expect("json");
                let result = match step {
                    0 => serde_json::json!({"protocolVersion": 1}),
                    1 => serde_json::json!({"sessionId": "s-slow"}),
                    _ => serde_json::json!({"stopReason": "end_turn"}),
                };
                answer(
                    &mut agent_write,
                    serde_json::json!({"jsonrpc":"2.0","id":req["id"],"result":result}),
                )
                .await;
            }
        });

        let mut stream = drive_with_idle(
            client_read,
            client_write,
            HarnessRequest::new("slow", "/tmp"),
            FAST_IDLE,
        );
        let mut completed = false;
        loop {
            let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
            match next {
                Some(Ok(HarnessEvent::Completed { .. })) => completed = true,
                Some(Ok(_)) => {}
                Some(Err(e)) => panic!("a slow BUT ALIVE agent must survive: {e}"),
                None => break,
            }
        }
        agent.await.expect("scripted slow agent");
        assert!(
            completed,
            "the turn completes across three near-deadline gaps"
        );
    }
}
