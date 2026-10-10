// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What one ACP authoring call received after writing its prompt, in closed categories only: how
//! many inbound frames of each kind its driver completed reading, when it completed the last of
//! them, and what ended the driver if anything did before the call concluded. A frame counts once
//! its line is complete: the bytes of an unfinished line count nothing, and nothing read before
//! the prompt was written counts (the handshake has its milestones). Frames the call can attribute
//! to itself (its own session's updates and requests, the answer or error to its own prompt) are
//! counted apart from every other frame, so another session's beats, a stray response or an
//! unknown notification never read as this call advancing. Milestones stay ordered completed
//! boundaries; this is a separate fact beside them.
//!
//! The facts are private to the call's [`Progress`] and reach only its concluded record, as
//! `activity` (null when the prompt was never written): `from_ms`, when the prompt was written;
//! `session` and `foreign`, each a fixed set of saturating `frames` counts and `last_ms`, when the
//! last of them was completed (null before any); and `ended_by`, null while the driver had not
//! ended, else `completed`, `transport` or the exact category of the frame it failed on (a refused
//! answer to the prompt as `turn_*`, in Nika's words). `session` also says when its first thought
//! and its first answer were completed (`first_thought_ms`, `first_answer_ms`) and its last update
//! by exact word with its time (`last_update`: `kind`, `ms`), each null before any, so a record
//! tells a call silent until its first answer from one that thought first, and what its agent
//! last wrote before a silence. Every time counts from the call's start, like its `elapsed_ms`. No
//! text is kept: a frame is read only to choose its category, and no prompt, answer, thought,
//! method, tag, stop spelling, identifier, path, tool argument, other metadata or error message is
//! retained or emitted.
//!
//! What a count proves is narrow. A thought, usage or status frame proves that the adapter wrote
//! that frame, not that a model kept working, which model served it or what it cost. No completed
//! frame after the prompt cannot tell an adapter or SDK that stalled from a model reasoning
//! without emitting anything: an adapter may suppress upstream events, and this side sees only
//! the frames written to it.
//!
//! The same frames keep the call alive. Each frame of this session that shows its agent working
//! (an answer, thought, tool, media or plan update) re-arms the call's deadline when its line is
//! complete. Usage and status updates are bookkeeping an adapter can write without its agent
//! advancing, and requests, the prompt's own answer and every frame the call cannot attribute to
//! itself never re-arm it.
use nika_kernel::ai::harness::HarnessError;
use serde_json::{Value, json};
use tokio::time::Instant;

use super::{ACCEPTED_STOP, Progress};
use crate::wire::{self, Incoming, WireError};

/// The categories a call attributes to itself, in the record's order.
const SESSION: [&str; 8] = [
    "answer",
    "thought",
    "usage",
    "status",
    "other_update",
    "client_request",
    "prompt_result",
    "prompt_error",
];

/// The categories it never attributes to itself, in the record's order.
const FOREIGN: [&str; 3] = ["other_session", "uncorrelated", "unreadable"];

/// How the answer to a prompt said its turn ended, in Nika's own closed words: the wire's stop is
/// read to choose one, and its spelling never reaches the record (an accepted completion alone
/// records the one stop it accepted, and a failed call records none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    /// The turn ended as an answer's turn ends.
    EndTurn,
    /// The agent reached its token limit.
    MaxTokens,
    /// The agent reached its limit of model requests in one turn.
    MaxTurnRequests,
    /// The agent declined to continue.
    Refusal,
    /// The agent called its turn off.
    Cancelled,
    /// Any other stop, or none the answer names.
    Other,
}

impl Stop {
    fn of(result: &Value) -> Self {
        match result.get("stopReason").and_then(Value::as_str) {
            Some(ACCEPTED_STOP) => Self::EndTurn,
            Some("max_tokens") => Self::MaxTokens,
            Some("max_turn_requests") => Self::MaxTurnRequests,
            Some("refusal") => Self::Refusal,
            Some("cancelled") => Self::Cancelled,
            _ => Self::Other,
        }
    }

    const fn word(self) -> &'static str {
        match self {
            Self::EndTurn => "turn_ended",
            Self::MaxTokens => "turn_token_limit",
            Self::MaxTurnRequests => "turn_request_limit",
            Self::Refusal => "turn_declined",
            Self::Cancelled => "turn_called_off",
            Self::Other => "turn_unknown_stop",
        }
    }
}

/// One completed inbound frame, by closed category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    /// This session's answer text.
    Answer,
    /// This session's thought.
    Thought,
    /// This session's usage update.
    Usage,
    /// This session's configuration, mode, session-information or commands update.
    Status,
    /// This session's tool call or tool-call update.
    Tool,
    /// This session's message chunk whose content is not text.
    Media,
    /// This session's plan update.
    Plan,
    /// This session's update of any other kind, or of none.
    OtherUpdate,
    /// An agent request for a permission (else for another client action), and whether it names
    /// this session (one that does not is counted as another session's).
    Request { permission: bool, ours: bool },
    /// The answer to this call's prompt.
    PromptResult(Stop),
    /// A JSON-RPC error: answering this call's prompt, or naming another request id.
    RpcError { prompt: bool },
    /// Another session's update.
    OtherSession,
    /// A response naming another request id, or a notification of another method.
    Uncorrelated,
    /// A line that is no JSON-RPC message, or an update naming no session.
    Unreadable,
}

impl Frame {
    /// The category of one parsed line, read on `session`'s connection after the prompt whose
    /// request id is `prompt`.
    fn of(parsed: Result<&Incoming, &WireError>, session: &str, prompt: u64) -> Self {
        let names = |params: &Value| {
            (params.get("sessionId").and_then(Value::as_str)).map(|named| named == session)
        };
        match parsed {
            Err(_) => Self::Unreadable,
            Ok(Incoming::Response { id, result }) if *id == prompt => {
                Self::PromptResult(Stop::of(result))
            }
            Ok(Incoming::ErrorResponse { id, .. }) => Self::RpcError {
                prompt: *id == prompt,
            },
            Ok(Incoming::Notification { method, params })
                if method == wire::METHOD_SESSION_UPDATE =>
            {
                match names(params) {
                    None => Self::Unreadable,
                    Some(false) => Self::OtherSession,
                    Some(true) => Self::update(&params["update"]),
                }
            }
            Ok(Incoming::Request { method, params, .. }) => Self::Request {
                permission: method == wire::METHOD_REQUEST_PERMISSION,
                ours: names(params) == Some(true),
            },
            Ok(_) => Self::Uncorrelated,
        }
    }

    /// The category of this session's `update`: each kind the completion profile accepts by its
    /// own name, and each kind it refuses by the class of that refusal.
    fn update(update: &Value) -> Self {
        let text = update.pointer("/content/type").and_then(Value::as_str) == Some("text")
            && update
                .pointer("/content/text")
                .is_some_and(Value::is_string);
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("agent_message_chunk") if text => Self::Answer,
            Some("agent_message_chunk") => Self::Media,
            Some("agent_thought_chunk") => Self::Thought,
            Some("usage_update") => Self::Usage,
            Some(
                "config_option_update"
                | "current_mode_update"
                | "session_info_update"
                | "available_commands_update",
            ) => Self::Status,
            Some("tool_call" | "tool_call_update") => Self::Tool,
            Some("plan") => Self::Plan,
            _ => Self::OtherUpdate,
        }
    }

    /// Whether the frame is one of this session's updates (an answer, thought, usage, status, tool,
    /// media, plan or other update), never a request, nor an answer or error to the prompt.
    const fn is_update(self) -> bool {
        matches!(
            self,
            Self::Answer
                | Self::Thought
                | Self::Usage
                | Self::Status
                | Self::Tool
                | Self::Media
                | Self::Plan
                | Self::OtherUpdate
        )
    }

    /// Whether the frame shows this session's agent working, which re-arms the call's deadline.
    const fn rearms(self) -> bool {
        matches!(
            self,
            Self::Answer | Self::Thought | Self::Tool | Self::Media | Self::Plan
        )
    }

    /// Where the frame is counted: whether this call attributes it to itself, and its category.
    const fn slot(self) -> (bool, &'static str) {
        match self {
            Self::Answer => (true, "answer"),
            Self::Thought => (true, "thought"),
            Self::Usage => (true, "usage"),
            Self::Status => (true, "status"),
            Self::Tool | Self::Media | Self::Plan | Self::OtherUpdate => (true, "other_update"),
            Self::Request { ours: true, .. } => (true, "client_request"),
            Self::PromptResult(_) => (true, "prompt_result"),
            Self::RpcError { prompt: true } => (true, "prompt_error"),
            Self::OtherSession | Self::Request { ours: false, .. } => (false, "other_session"),
            Self::Uncorrelated | Self::RpcError { prompt: false } => (false, "uncorrelated"),
            Self::Unreadable => (false, "unreadable"),
        }
    }

    /// The frame's exact word, as the one a driver failed on.
    const fn word(self) -> &'static str {
        match self {
            Self::Answer => "answer",
            Self::Thought => "thought",
            Self::Usage => "usage",
            Self::Status => "status",
            Self::Tool => "tool_update",
            Self::Media => "media_update",
            Self::Plan | Self::OtherUpdate => "other_update",
            Self::Request {
                permission: true, ..
            } => "permission_request",
            Self::Request {
                permission: false, ..
            } => "client_request",
            Self::PromptResult(stop) => stop.word(),
            Self::RpcError { prompt: true } => "prompt_error",
            Self::RpcError { prompt: false } => "uncorrelated_error",
            Self::OtherSession => "other_session",
            Self::Uncorrelated => "uncorrelated",
            Self::Unreadable => "unreadable",
        }
    }
}

/// What ended a session's driver after its prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    /// The turn closed: the driver delivered its completed outcome.
    Completed,
    /// The driver failed on this frame (an answer chunk fails only over the answer byte limit).
    Frame(Frame),
    /// The transport ended the driver, whatever frame was in hand: its end of file, a line over
    /// the line bound or not UTF-8, the per-frame bound, or a failed write.
    Transport,
}

impl Ending {
    const fn word(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Frame(frame) => frame.word(),
            Self::Transport => "transport",
        }
    }
}

/// What one call received after writing its prompt, held by its [`Progress`] until the record
/// snapshots it.
#[derive(Debug, Default)]
pub(super) struct Activity {
    /// When the prompt was written: the window opens, and no earlier frame counts.
    from: Option<Instant>,
    /// This session's counts, in [`SESSION`] order, each saturating.
    session: [u32; SESSION.len()],
    session_last: Option<Instant>,
    /// When this session's first thought and first answer were completed.
    first_thought: Option<Instant>,
    first_answer: Option<Instant>,
    /// This session's last update, and when it was completed.
    last_update: Option<(Frame, Instant)>,
    /// The other frames' counts, in [`FOREIGN`] order, each saturating.
    foreign: [u32; FOREIGN.len()],
    foreign_last: Option<Instant>,
    /// When the last frame showing this session's agent working arrived: the call's deadline
    /// counts its silence from there.
    rearmed: Option<Instant>,
    /// What a failure of the driver would end on now: the frame it last received, or the
    /// transport once a read or a write failed.
    current: Option<Ending>,
    /// What ended the driver, once it ended.
    ended: Option<Ending>,
}

impl Activity {
    /// The prompt was written at `at`: the window opens, once.
    pub(super) fn open(&mut self, at: Instant) {
        self.from.get_or_insert(at);
    }

    /// One completed frame, received at `at`; outside the window it counts nothing.
    fn received(&mut self, frame: Frame, at: Instant) {
        if self.from.is_none() {
            return;
        }
        let (ours, word) = frame.slot();
        let (words, counts, last): (&[&str], &mut [u32], _) = if ours {
            (&SESSION, &mut self.session, &mut self.session_last)
        } else {
            (&FOREIGN, &mut self.foreign, &mut self.foreign_last)
        };
        let slot = words.iter().position(|w| *w == word);
        if let Some(count) = slot.and_then(|index| counts.get_mut(index)) {
            *count = count.saturating_add(1);
        }
        *last = Some(at);
        if frame.rearms() {
            self.rearmed = Some(at);
        }
        match frame {
            Frame::Thought => self.first_thought = self.first_thought.or(Some(at)),
            Frame::Answer => self.first_answer = self.first_answer.or(Some(at)),
            _ => {}
        }
        if frame.is_update() {
            self.last_update = Some((frame, at));
        }
        self.current = Some(Ending::Frame(frame));
    }

    /// When the last frame showing this session's agent working arrived, if one did.
    pub(super) const fn rearmed(&self) -> Option<Instant> {
        self.rearmed
    }

    /// A read or a write failed inside the window: a failure now ends on the transport, and the
    /// frame last received stays counted.
    fn transport_failed(&mut self) {
        if self.from.is_some() {
            self.current = Some(Ending::Transport);
        }
    }

    /// The driver ended inside the window, completed or failed on what it held; the first
    /// ending stays.
    fn end(&mut self, completed: bool) {
        if self.from.is_none() || self.ended.is_some() {
            return;
        }
        let failed = self.current.unwrap_or(Ending::Transport);
        self.ended = Some(if completed { Ending::Completed } else { failed });
    }

    /// The record's `activity`, every time counted from the call's `start`; null when the prompt
    /// was never written.
    pub(super) fn record(&self, start: Instant) -> Value {
        let Some(from) = self.from else {
            return Value::Null;
        };
        let ms = |at: Instant| {
            u64::try_from(at.saturating_duration_since(start).as_millis()).unwrap_or(u64::MAX)
        };
        let frames = |words: &[&str], counts: &[u32]| {
            let pairs = words.iter().zip(counts);
            Value::Object(pairs.map(|(w, n)| ((*w).to_owned(), json!(n))).collect())
        };
        let update = |(frame, at): (Frame, Instant)| json!({"kind": frame.word(), "ms": ms(at)});
        json!({"from_ms": ms(from),
            "session": {"frames": frames(&SESSION, &self.session),
                "last_ms": self.session_last.map(ms),
                "first_thought_ms": self.first_thought.map(ms),
                "first_answer_ms": self.first_answer.map(ms),
                "last_update": self.last_update.map(update)},
            "foreign": {"frames": frames(&FOREIGN, &self.foreign),
                "last_ms": self.foreign_last.map(ms)},
            "ended_by": self.ended.map(Ending::word)})
    }
}

/// The driver of `progress`'s call read after its prompt, on `session`'s connection whose prompt
/// request id is `prompt`: a failed read, a line that is no message, or a parsed message. Nothing
/// is read when no call listens.
pub(crate) fn received(
    progress: Option<&Progress>,
    read: &Result<Result<Incoming, WireError>, HarnessError>,
    session: &str,
    prompt: u64,
) {
    let Some(progress) = progress else {
        return;
    };
    match read {
        Err(_) => progress.activity().transport_failed(),
        Ok(parsed) => {
            let frame = Frame::of(parsed.as_ref(), session, prompt);
            progress.activity().received(frame, Instant::now());
        }
    }
}

/// A write by the driver of `progress`'s call failed: after the prompt, a failure ends on the
/// transport, whatever frame the write answered.
pub(crate) fn write_failed(progress: Option<&Progress>) {
    if let Some(progress) = progress {
        progress.activity().transport_failed();
    }
}

/// The driver of `progress`'s call is delivering its completed turn.
pub(crate) fn completed(progress: Option<&Progress>) {
    if let Some(progress) = progress {
        progress.activity().end(true);
    }
}

/// The driver of `progress`'s call failed, on what it held.
pub(crate) fn failed(progress: Option<&Progress>) {
    if let Some(progress) = progress {
        progress.activity().end(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(session: &str, update: &Value) -> Value {
        json!({"jsonrpc":"2.0","method":"session/update",
            "params":{"sessionId":session,"update":update}})
    }

    fn of(line: &str) -> Frame {
        Frame::of(wire::parse_line(line).as_ref(), "s", 3)
    }

    /// One line's exact word and count slot.
    fn sorted(line: &Value) -> (&'static str, (bool, &'static str)) {
        let frame = of(&line.to_string());
        (frame.word(), frame.slot())
    }

    /// Every update kind of this session lands in one closed category; another session's never
    /// counts as this one's.
    #[test]
    fn every_update_kind_has_one_closed_category() {
        let kinds = [
            ("agent_thought_chunk", "thought", "thought"),
            ("usage_update", "usage", "usage"),
            ("config_option_update", "status", "status"),
            ("current_mode_update", "status", "status"),
            ("session_info_update", "status", "status"),
            ("available_commands_update", "status", "status"),
            ("tool_call", "tool_update", "other_update"),
            ("tool_call_update", "tool_update", "other_update"),
            ("plan", "other_update", "other_update"),
            ("user_message_chunk", "other_update", "other_update"),
        ];
        for (kind, word, slot) in kinds {
            let line = update("s", &json!({ "sessionUpdate": kind }));
            assert_eq!(sorted(&line), (word, (true, slot)), "{kind}");
        }
        let chunk =
            |content: Value| json!({"sessionUpdate":"agent_message_chunk","content":content});
        let text = chunk(json!({"type":"text","text":"x"}));
        let image = chunk(json!({"type":"image"}));
        let answer = ("answer", (true, "answer"));
        assert_eq!(sorted(&update("s", &text)), answer);
        let media = ("media_update", (true, "other_update"));
        assert_eq!(sorted(&update("s", &image)), media);
        let unnamed = ("other_update", (true, "other_update"));
        assert_eq!(sorted(&update("s", &json!({}))), unnamed);
        let other = ("other_session", (false, "other_session"));
        assert_eq!(sorted(&update("t", &text)), other);
    }

    /// Requests, responses, errors and other lines are attributed to this call only when they name
    /// its session or its prompt; everything else is kept apart.
    #[test]
    fn requests_answers_and_strays_are_attributed_or_kept_apart() {
        let request = |method: &str, session: &str| {
            json!({"jsonrpc":"2.0","id":7,"method":method,
                "params":{"sessionId":session}})
        };
        let permission = request("session/request_permission", "s");
        let asked = ("permission_request", (true, "client_request"));
        assert_eq!(sorted(&permission), asked);
        let foreign = ("client_request", (false, "other_session"));
        assert_eq!(sorted(&request("fs/read_text_file", "t")), foreign);
        let error = |id: u64| json!({"jsonrpc":"2.0","id":id,"error":{"code":1,"message":"m"}});
        let prompt_error = ("prompt_error", (true, "prompt_error"));
        assert_eq!(sorted(&error(3)), prompt_error);
        let stray_error = ("uncorrelated_error", (false, "uncorrelated"));
        assert_eq!(sorted(&error(4)), stray_error);
        let stray = ("uncorrelated", (false, "uncorrelated"));
        assert_eq!(sorted(&json!({"jsonrpc":"2.0","id":2,"result":{}})), stray);
        let vendor = json!({"jsonrpc":"2.0","method":"_x/ping","params":{"sessionId":"s"}});
        assert_eq!(sorted(&vendor), stray);
        let nameless = json!({"jsonrpc":"2.0","method":"session/update","params":{}});
        assert_eq!(sorted(&nameless), ("unreadable", (false, "unreadable")));
        assert_eq!(of("not json"), Frame::Unreadable);
        let text_id = r#"{"jsonrpc":"2.0","id":"3","result":{}}"#;
        assert_eq!(of(text_id), Frame::Unreadable);
    }

    /// The answer to the prompt names its stop in Nika's closed words only, never in the wire's
    /// spelling.
    #[test]
    fn a_prompt_result_names_its_stop_in_closed_words() {
        let stops = [
            ("end_turn", Stop::EndTurn, "turn_ended"),
            ("max_tokens", Stop::MaxTokens, "turn_token_limit"),
            (
                "max_turn_requests",
                Stop::MaxTurnRequests,
                "turn_request_limit",
            ),
            ("refusal", Stop::Refusal, "turn_declined"),
            ("cancelled", Stop::Cancelled, "turn_called_off"),
            ("end_turn_secret", Stop::Other, "turn_unknown_stop"),
        ];
        for (stop, closed, word) in stops {
            let line = json!({"jsonrpc":"2.0","id":3,"result":{"stopReason":stop}});
            assert_eq!(of(&line.to_string()), Frame::PromptResult(closed));
            assert_eq!(Frame::PromptResult(closed).word(), word);
            assert!(!word.contains(stop), "{word}");
        }
        for result in [json!({}), json!({"stopReason":7})] {
            let line = json!({"jsonrpc":"2.0","id":3,"result":result});
            assert_eq!(of(&line.to_string()), Frame::PromptResult(Stop::Other));
        }
    }

    /// Only a frame showing this session's agent working re-arms the call's deadline: an answer,
    /// thought, tool, media or plan update. Its usage and status bookkeeping, another kind of
    /// update, a request, an answer or error to the prompt, a stray response, an unreadable line
    /// and every other session's frame, the same kinds included, never do.
    #[test]
    fn only_this_session_agent_work_rearms_the_deadline() {
        let chunk =
            |content: Value| json!({"sessionUpdate":"agent_message_chunk","content":content});
        let working = [
            json!({"sessionUpdate":"agent_thought_chunk"}),
            chunk(json!({"type":"text","text":"x"})),
            chunk(json!({"type":"image"})),
            json!({"sessionUpdate":"tool_call"}),
            json!({"sessionUpdate":"tool_call_update"}),
            json!({"sessionUpdate":"plan"}),
        ];
        for kind in &working {
            assert!(of(&update("s", kind).to_string()).rearms(), "{kind}");
            assert!(!of(&update("t", kind).to_string()).rearms(), "{kind}");
        }
        let bookkeeping = [
            "usage_update",
            "config_option_update",
            "current_mode_update",
            "session_info_update",
            "available_commands_update",
            "user_message_chunk",
            "unknown_update",
        ];
        for kind in bookkeeping {
            let line = update("s", &json!({ "sessionUpdate": kind }));
            assert!(!of(&line.to_string()).rearms(), "{kind}");
        }
        let permission = json!({"jsonrpc":"2.0","id":7,"method":"session/request_permission",
            "params":{"sessionId":"s"}});
        let others = [
            json!({"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}),
            json!({"jsonrpc":"2.0","id":3,"error":{"code":1,"message":"m"}}),
            json!({"jsonrpc":"2.0","id":2,"result":{}}),
            permission,
        ];
        for line in &others {
            assert!(!of(&line.to_string()).rearms(), "{line}");
        }
        assert!(!of("not json").rearms());
    }

    /// The re-arming is the time of the last working frame inside the window: none before the
    /// prompt was written, and bookkeeping or foreign frames after it leave it where it was.
    #[tokio::test(start_paused = true)]
    async fn the_last_working_frame_in_the_window_is_the_rearming() {
        let start = Instant::now();
        let mut activity = Activity::default();
        activity.received(Frame::Thought, start);
        assert_eq!(activity.rearmed(), None, "nothing before the prompt");
        activity.open(start);
        tokio::time::sleep(std::time::Duration::from_millis(7)).await;
        let thought = Instant::now();
        activity.received(Frame::Thought, thought);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let later = [
            Frame::Usage,
            Frame::Status,
            Frame::OtherSession,
            Frame::Unreadable,
        ];
        for frame in later {
            activity.received(frame, Instant::now());
        }
        assert_eq!(activity.rearmed(), Some(thought));
    }

    /// Nothing counts before the window opens; counts saturate; a failed read or write ends on the
    /// transport over the frame in hand, and the first ending stays.
    #[tokio::test(start_paused = true)]
    async fn counts_saturate_inside_the_window_and_the_first_ending_stays() {
        let start = Instant::now();
        let mut activity = Activity::default();
        activity.received(Frame::Thought, start);
        activity.end(false);
        assert_eq!(activity.record(start), Value::Null);
        tokio::time::sleep(std::time::Duration::from_millis(7)).await;
        activity.open(Instant::now());
        activity.session[1] = u32::MAX - 1;
        for _ in 0..3 {
            activity.received(Frame::Thought, Instant::now());
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        activity.received(Frame::OtherSession, Instant::now());
        activity.transport_failed();
        activity.end(false);
        activity.end(true);
        let record = activity.record(start);
        assert_eq!(record["from_ms"], 7);
        assert_eq!(record["session"]["frames"]["thought"], json!(u32::MAX));
        assert_eq!(record["session"]["last_ms"], 7);
        assert_eq!(record["foreign"]["frames"]["other_session"], 1);
        assert_eq!(record["foreign"]["last_ms"], 12);
        assert_eq!(record["ended_by"], "transport");
    }

    /// The one interleaving that could make a record contradict itself: the call concludes on its
    /// own thread while the driver holds the activity mid-frame, completes that frame 20 ms later
    /// and lets go. The record must hold the frame, and no time in it may be later than the
    /// elapsed time it reports, whatever the threads' timing.
    #[test]
    fn a_frame_completed_while_the_call_concludes_is_never_later_than_its_elapsed_time() {
        use crate::authoring::acp::{Deadline, Milestone, conclude};
        let deadline = Deadline::start(std::time::Duration::from_secs(600));
        let progress = Progress::default();
        progress.mark(Milestone::PromptWritten);
        let record = std::thread::scope(|scope| {
            let mut driver = progress.activity();
            let call = scope.spawn(|| conclude(json!({"status":"timed_out"}), &progress, deadline));
            std::thread::sleep(std::time::Duration::from_millis(20));
            driver.received(Frame::Thought, Instant::now());
            drop(driver);
            call.join().expect("the call concludes")
        });
        assert_eq!(record["activity"]["session"]["frames"]["thought"], 1);
        let ms = |pointer: &str| record.pointer(pointer).and_then(Value::as_u64);
        let from = ms("/activity/from_ms");
        let last = ms("/activity/session/last_ms");
        let elapsed = ms("/elapsed_ms");
        assert!(
            matches!((from, last, elapsed), (Some(f), Some(l), Some(e)) if f <= l && l <= e),
            "{record}"
        );
        assert!(last >= Some(20), "{record}");
    }

    /// The race the final review named: the driver publishes the written prompt and is held right
    /// there, as a preempted thread would be, while the call concludes on its Stop or deadline on
    /// another thread. The record that names the written prompt must hold its open window, and
    /// one taken before the publication names neither. Channels order every step: no sleep.
    #[test]
    fn a_record_that_names_the_written_prompt_holds_its_window() {
        use crate::authoring::acp::call::pause;
        use crate::authoring::acp::{Deadline, Milestone, conclude};
        let deadline = Deadline::start(std::time::Duration::from_secs(600));
        let progress = Progress::default();
        progress.mark(Milestone::SelectionChecked);
        let before = conclude(json!({"status":"cancelled"}), &progress, deadline);
        assert_eq!(before["last_milestone"], "selection_checked");
        assert_eq!(before.get("activity"), Some(&Value::Null));
        let (at_pause, paused) = std::sync::mpsc::channel::<()>();
        let (resume, held) = std::sync::mpsc::channel::<()>();
        let marks = &progress;
        let record = std::thread::scope(|scope| {
            let driver = scope.spawn(move || {
                pause::when_published(move || {
                    at_pause.send(()).expect("the test waits for the pause");
                    held.recv().expect("the test lets the driver go");
                });
                marks.mark(Milestone::PromptWritten);
            });
            paused
                .recv()
                .expect("the driver published the written prompt");
            let record = conclude(json!({"status":"cancelled"}), marks, deadline);
            resume.send(()).expect("the driver is held");
            driver.join().expect("the driver ends");
            record
        });
        assert_eq!(record["last_milestone"], "prompt_written");
        let ms = |pointer: &str| record.pointer(pointer).and_then(Value::as_u64);
        let (from, elapsed) = (ms("/activity/from_ms"), ms("/elapsed_ms"));
        assert!(
            matches!((from, elapsed), (Some(f), Some(e)) if f <= e),
            "{record}"
        );
        let start = Instant::now();
        let mut window = Activity::default();
        window.open(start);
        let mut opened = record["activity"].clone();
        opened["from_ms"] = json!(0);
        assert_eq!(opened, window.record(start), "{record}");
    }
}
