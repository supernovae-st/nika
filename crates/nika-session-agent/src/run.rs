// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The loop: a person's line, the model's message, the calls it makes and their replies,
//! again, until the model answers without a call. No step, turn, token or time quota ends a
//! run: Stop, a call that waits for the person ([`Outcome::Parked`]) or a failure does, and the
//! route's real context window is kept by compaction.
//!
//! A steering line enters after the current calls, and the calls not yet run are skipped so
//! the model reads it first; a follow-up line enters when the model would end. A call that
//! repeats the previous one exactly, reply included, is said so to the model, never stopped.
//! Every entry reaches the Session's file before the tree takes it.

use std::collections::VecDeque;
use std::io;

use nika_kernel::CancelCtx;
use nika_kernel::provider::{ContentBlock, Message, StopReason, TokenUsage, ToolDef};
use nika_session_change::outcome::StopReach;
use nika_session_change::tools::{SessionTools, ToolCall, ToolReply};
use serde_json::Value;

use crate::compact::{self, Window};
use crate::context_edit::Stubs;
use crate::event::{AgentEvent, End};
use crate::steer::{QueueMode, Queued, Steering};
use crate::tree::{EntryId, EntryKind, Tree, TreeError, calls};

mod lead;

pub use lead::{Beat, Conversant, LED_TURN, Led, LedEnd, Relay, transcript};

/// What the model reads after a call that repeats the previous one exactly, reply included.
const REPEATED: &str = "\n\n(Nika: this call and its reply repeat the previous call exactly.)";

/// How many identical calls in a row make the repetition a stall the model is told to break.
const STALLED_AFTER: u32 = 3;

/// What the model reads once the same call, reply included, comes back [`STALLED_AFTER`] times
/// in a row: the call cannot succeed as it is (live gauntlet q03, 2026-10-16: 80 identical
/// `candidate_write` calls in 15 minutes, each refused the same way).
fn stalled(times: u32) -> String {
    format!(
        "\n\n(Nika: this exact call has now returned this exact reply {times} times in a row. \
         Repeating it cannot change the reply: change the call, use another tool, or answer \
         the person with what blocks you.)"
    )
}

/// Why a call after a steering line is not run.
const STEERED: &str = "Not run: the person wrote meanwhile; read their message first.";

/// The one tool whose consecutive calls run side by side: it reads a public page, nothing else.
const OBSERVE: &str = "observe";

/// A reply as the repetition check compares it: a candidate's `revision` counts the calls, never
/// what they answered, so it is left out (live gauntlet, 2026-10-16: Grok 4.7 sent the same
/// `candidate_write` some sixty times, each refused with the same findings under a new revision
/// number, and was never told it repeated itself).
fn steady(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(mut reply)) => {
            reply.remove("revision");
            Value::Object(reply).to_string()
        }
        _ => text.to_owned(),
    }
}

/// A call's arguments without carriage returns: no Session tool reads one (a document's line ends
/// are its newlines), and a model that slipped them between its words copied its own earlier
/// calls, more each time (live gauntlet r05, 2026-10-16: GLM 5.3 Flash wrote `schedule\r_zone`
/// and `Indian\r/Mauritius`, 115 calls refused for a key or a zone that did not match, with
/// up to 3,904 of them in one call).
fn without_carriage_returns(value: &mut Value) {
    match value {
        Value::String(text) if text.contains('\r') => text.retain(|c| c != '\r'),
        Value::Array(items) => items.iter_mut().for_each(without_carriage_returns),
        Value::Object(fields) => {
            let keys: Vec<String> = fields
                .keys()
                .filter(|k| k.contains('\r'))
                .cloned()
                .collect();
            for key in keys {
                if let Some(field) = fields.remove(&key) {
                    fields.insert(key.replace('\r', ""), field);
                }
            }
            fields.values_mut().for_each(without_carriage_returns);
        }
        _ => {}
    }
}

/// Whether a call hands the turn to the person or replaces their request: a steering line the
/// author has not read holds it back, while candidate and read-only calls still run.
fn hands_over(name: &str) -> bool {
    matches!(name, "ask" | "propose" | "new_request")
}

/// What each call of a message cut at the output limit reads: none of them runs.
const CUT: &str = "Not run: your message was cut at the output limit; re-issue the call with \
complete arguments.";

/// What the person reads under an answer cut at the output limit.
const CUT_ANSWER: &str = "\n… (this answer was cut at the output limit)";

/// Why a turn fails when its provider interrupted the answer: nothing of it is kept or run. What
/// the person reads when the provider ended an answer with its own error: what it was, and
/// the two ways on (live gauntlet r05, 2026-10-16: three identical failures after 2 min 05 s, and
/// nothing said whether to retry, rephrase or change the model).
const INTERRUPTED: &str = "the provider ended the answer with an error before it was complete: nothing of it was kept or run · say the same line again to retry, or choose another model with `/intelligence`";

/// One request: the instructions, the conversation and the tools the model may call.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Request {
    /// The instructions, when the branch has any.
    pub system: Option<String>,
    /// The conversation, in order.
    pub messages: Vec<Message>,
    /// The tools the model may call.
    pub tools: Vec<ToolDef>,
}

impl Request {
    /// A request (INV-019).
    #[must_use]
    pub fn new(system: Option<String>, messages: Vec<Message>, tools: Vec<ToolDef>) -> Self {
        Self {
            system,
            messages,
            tools,
        }
    }
}

/// The model's whole message for one request.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Reply {
    /// Its blocks: text, thinking and calls, in order.
    pub content: Vec<ContentBlock>,
    /// Why it stopped.
    pub stop: StopReason,
    /// The tokens it used, as reported.
    pub usage: Option<TokenUsage>,
    /// The model that answered, as reported.
    pub model: Option<String>,
    /// The request filled the model's context window (the provider's
    /// window-full marker): a cut at the window, or more input than the
    /// window holds. Asking again needs a shorter conversation, not complete
    /// arguments.
    pub window_full: bool,
}

impl Reply {
    /// A message (INV-019).
    #[must_use]
    pub fn new(content: Vec<ContentBlock>, stop: StopReason) -> Self {
        Self {
            content,
            stop,
            usage: None,
            model: None,
            window_full: false,
        }
    }

    /// Whether the request filled the model's context window.
    #[must_use]
    pub fn with_window_full(mut self, window_full: bool) -> Self {
        self.window_full = window_full;
        self
    }

    /// The tokens it used, as reported.
    #[must_use]
    pub fn with_usage(mut self, usage: TokenUsage) -> Self {
        self.usage = Some(usage);
        self
    }

    /// The model that answered, as reported.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

/// Why a request returned no message.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ModelError {
    /// Stop ended the request.
    #[error("the person stopped the request")]
    Stopped,
    /// Any other failure, in words the person reads.
    #[error("{0}")]
    Failed(String),
    /// The request is over the model's context window: the same request overflows again, so the
    /// conversation must shorten first.
    #[error("the request is over the model's context window")]
    Overflow {
        /// The window in tokens, when the route knows it.
        window: Option<u64>,
    },
}

/// The selected intelligence, asked one request at a time.
pub trait Model {
    /// Answer `request` with the model's whole message; the text and thinking it streams go to
    /// `events` as they arrive.
    ///
    /// # Errors
    ///
    /// [`ModelError::Stopped`] when Stop ended the request; [`ModelError::Failed`] otherwise.
    fn complete(
        &mut self,
        request: &Request,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<Reply, ModelError>;
}

/// Where a tree's lines go: the Session's own file, each line appended and synced before the
/// tree takes its entry.
pub trait Store {
    /// Make `line` durable.
    ///
    /// # Errors
    ///
    /// The line could not be written; its entry is then not in the tree.
    fn append(&mut self, line: &str) -> io::Result<()>;
}

/// Lines kept in memory, for a conversation nothing persists and for tests.
impl Store for Vec<String> {
    fn append(&mut self, line: &str) -> io::Result<()> {
        self.push(line.to_owned());
        Ok(())
    }
}

/// Why a run or a compaction cannot go on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentError {
    /// The model failed, in its words.
    #[error("the model failed: {0}")]
    Model(String),
    /// Stop ended a compaction's request.
    #[error("the person stopped the request")]
    Stopped,
    /// The tree could not record an entry.
    #[error(transparent)]
    Tree(#[from] TreeError),
    /// The run waits for the person's answer to `call`: a line answers it.
    #[error("the run waits for the person's answer to call `{call}`")]
    Parked {
        /// The call waiting.
        call: String,
    },
    /// No call waits for an answer.
    #[error("no call waits for the person's answer")]
    NotParked,
}

/// How a run ended.
#[derive(Debug)]
#[non_exhaustive]
pub enum Outcome {
    /// The model answered without a call, and no line was queued.
    Answered {
        /// Its answer text.
        text: String,
    },
    /// A call ended the turn: the run waits for the person's answer to it ([`Agent::answer`]).
    Parked {
        /// The model's identity of the call.
        call: String,
        /// The tool called.
        name: String,
        /// The lines queued during the run, never sent: they return to the person.
        queued: Vec<(QueueMode, String)>,
    },
    /// The person stopped the run.
    Stopped {
        /// The lines queued during the run, never sent: they return to the person.
        queued: Vec<(QueueMode, String)>,
    },
    /// The model or the tree failed; what was recorded stays.
    Failed {
        /// Why.
        error: AgentError,
    },
}

impl Outcome {
    /// Why the run ended, as its last event says it.
    #[must_use]
    pub fn end(&self) -> End {
        match self {
            Self::Answered { .. } => End::Answered,
            Self::Parked { .. } => End::Parked,
            Self::Stopped { .. } => End::Stopped,
            Self::Failed { .. } => End::Failed,
        }
    }
}

/// A call as the model made it: its identity, the tool, the arguments.
type Call = (String, String, Value);

/// One conversation's loop over its tree, the Session's tools and the person's queued lines.
pub struct Agent<'a> {
    tree: &'a mut Tree,
    store: &'a mut dyn Store,
    tools: &'a dyn SessionTools,
    now: &'a dyn Fn() -> u64,
    steering: Steering,
    cancel: CancelCtx,
    window: Option<Window>,
    stopped: Option<StopReach>,
    reading: Option<String>,
    note: Option<String>,
    pending: Option<&'a dyn Fn() -> bool>,
    late: Option<String>,
}

impl<'a> Agent<'a> {
    /// The loop over `tree`, whose lines go to `store`, calling `tools`; `now` stamps each
    /// entry in Unix milliseconds. Without [`Agent::with_window`] nothing is compacted unasked.
    #[must_use]
    pub fn new(
        tree: &'a mut Tree,
        store: &'a mut dyn Store,
        tools: &'a dyn SessionTools,
        now: &'a dyn Fn() -> u64,
    ) -> Self {
        Self {
            tree,
            store,
            tools,
            now,
            steering: Steering::new(),
            cancel: CancelCtx::new(),
            window: None,
            stopped: None,
            reading: None,
            note: None,
            pending: None,
            late: None,
        }
    }

    /// The `ask` call the person's next line replies to although nothing waits: the line answers
    /// another question of that call's group, still asked, after the call stopped waiting. The
    /// tree records the line under that call, which still reads as any person's line; it answers
    /// no call and grants nothing.
    #[must_use]
    pub fn with_late_reply(mut self, call: Option<String>) -> Self {
        self.late = call;
        self
    }

    /// How the person's Stop reached the last run, when it stopped one.
    #[must_use]
    pub const fn stop_reach(&self) -> Option<StopReach> {
        self.stopped
    }

    /// The person's queued lines this loop takes.
    #[must_use]
    pub fn with_steering(mut self, steering: &Steering) -> Self {
        self.steering = steering.clone();
        self
    }

    /// What the Session read the person's next answer to pick (`notes`, one per question it
    /// answers): shown to the author after the answer, never as the person's words.
    #[must_use]
    pub fn with_reading(mut self, notes: &[String]) -> Self {
        self.reading = (!notes.is_empty()).then(|| {
            let lines: Vec<String> = notes.iter().map(|note| format!("- {note}")).collect();
            format!("Its reading of this answer:\n{}", lines.join("\n"))
        });
        self
    }

    /// What Nika says to the author with the person's next line (the facts of the last run):
    /// shown after their words, never as theirs.
    #[must_use]
    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }

    /// What says the candidate the author wrote waits unproposed: a turn that would end in words
    /// while it does is asked once more to end with a proposal, a question or a refusal.
    #[must_use]
    pub fn with_pending(mut self, pending: &'a dyn Fn() -> bool) -> Self {
        self.pending = Some(pending);
        self
    }

    /// Nika's own `note` (a failed run's facts, what to repair) starts a run with no person's
    /// line. While a question waits for the person, nothing starts; a shown proposal the person's
    /// words acted on (saved, run) waits for nothing more.
    pub fn note(
        &mut self,
        note: &str,
        model: &mut dyn Model,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Outcome {
        if let Some((call, name)) = self.tree.parked()
            && name != "propose"
        {
            let call = call.to_owned();
            return Outcome::Failed {
                error: AgentError::Parked { call },
            };
        }
        let text = note.to_owned();
        match self.record(EntryKind::Note { text }) {
            Ok(entry) => self.drive(&entry, model, events),
            Err(error) => Outcome::Failed { error },
        }
    }

    /// The Stop this loop obeys.
    #[must_use]
    pub fn with_cancel(mut self, cancel: &CancelCtx) -> Self {
        self.cancel = cancel.clone();
        self
    }

    /// The route's window: the conversation is compacted before a request that would leave the
    /// reply less than its reserve.
    #[must_use]
    pub fn with_window(mut self, window: Window) -> Self {
        self.window = Some(window);
        self
    }

    /// A person's line starts a run. While a call waits for the person, a line answers it
    /// ([`Agent::answer`]) instead; a late reply ([`Agent::with_late_reply`]) names its call.
    pub fn prompt(
        &mut self,
        text: &str,
        model: &mut dyn Model,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Outcome {
        if let Some((call, _)) = self.tree.parked() {
            let call = call.to_owned();
            return Outcome::Failed {
                error: AgentError::Parked { call },
            };
        }
        let late = self.late.take();
        match self.user(text, late, None) {
            Ok((entry, _)) => self.drive(&entry, model, events),
            Err(error) => Outcome::Failed { error },
        }
    }

    /// The person's line answers the call the run waits on, and the run goes on.
    pub fn answer(
        &mut self,
        text: &str,
        model: &mut dyn Model,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Outcome {
        let Some(call) = self.tree.parked().map(|(call, _)| call.to_owned()) else {
            return Outcome::Failed {
                error: AgentError::NotParked,
            };
        };
        match self.user(text, Some(call), None) {
            Ok((entry, _)) => self.drive(&entry, model, events),
            Err(error) => Outcome::Failed { error },
        }
    }

    /// Fold the branch before its recent part into a summary the model writes (`focus`: what
    /// the person asked it to keep in view). Returns the summary's entry, or none when there is
    /// nothing to fold.
    ///
    /// # Errors
    ///
    /// [`AgentError::Stopped`] or [`AgentError::Model`] when the summary was not written;
    /// [`AgentError::Tree`] when it could not be recorded.
    pub fn compact(
        &mut self,
        focus: Option<&str>,
        model: &mut dyn Model,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<Option<EntryId>, AgentError> {
        let keep = self.window.map_or(0, |w| w.keep);
        let Some(first_kept) = compact::cut(self.tree, keep) else {
            return Ok(None);
        };
        let tokens_before = compact::estimate(self.tree);
        // Budget each complete summary payload by its estimate, including the carried summary.
        // A larger fold is summarized in whole-object parts; an object that cannot fit is refused.
        let room = (self.window).map(|window| window.tokens.saturating_sub(window.reserve));
        let (mut summary, lines) = compact::folded(self.tree, &first_kept);
        let mut at = 0;
        loop {
            let rest = lines.get(at..).unwrap_or_default();
            let take = compact::fitting(summary.as_deref(), rest, focus, room)
                .map_err(|needed| self.summary_budget_error(needed, room.unwrap_or_default()))?;
            let part = rest.get(..take).unwrap_or_default();
            let request = compact::summary_request(summary.as_deref(), part, focus);
            let needed = compact::request_tokens(&request);
            if let Some(budget) = room
                && needed > budget
            {
                return Err(self.summary_budget_error(needed, budget));
            }
            summary = Some(Self::summarize(&request, model)?);
            at += take;
            if at >= lines.len() {
                break;
            }
        }
        let summary = summary.unwrap_or_default();
        // A summary no lighter than what it folds folds nothing.
        if compact::tokens(&summary) >= compact::folded_weight(self.tree, &first_kept) {
            return Ok(None);
        }
        let at = (self.now)();
        let store = &mut *self.store;
        let entry =
            self.tree
                .append_compaction(summary, first_kept, tokens_before, at, |line| {
                    store.append(line)
                })?;
        events(AgentEvent::Compacted {
            entry: entry.to_string(),
            tokens_before,
        });
        Ok(Some(entry))
    }

    /// A summary payload that cannot fit leaves every original entry untouched.
    fn summary_budget_error(&self, needed: u64, budget: u64) -> AgentError {
        let window = self.window.map_or(budget, |window| window.tokens);
        AgentError::Model(format!(
            "the summary request needs an estimated {needed} tokens, \
             over the {budget}-token budget of the {window}-token window: nothing was folded"
        ))
    }

    /// The summary the model writes for `request`, never streamed to the person. One that came
    /// back empty, cut at the output limit or interrupted by its provider would silently replace
    /// what it folds: nothing is folded, the originals stay the context, and the turn says why.
    fn summarize(request: &Request, model: &mut dyn Model) -> Result<String, AgentError> {
        let reply =
            (model.complete(request, &mut |_: AgentEvent| {})).map_err(|error| match error {
                ModelError::Stopped => AgentError::Stopped,
                ModelError::Failed(why) => AgentError::Model(why),
                other @ ModelError::Overflow { .. } => {
                    AgentError::Model(format!("{other}: nothing was folded"))
                }
            })?;
        let summary = answer_text(&reply.content);
        if summary.trim().is_empty() {
            return Err(AgentError::Model("the summary came back empty".into()));
        }
        if matches!(reply.stop, StopReason::MaxTokens) {
            return Err(AgentError::Model(
                "the summary was cut at the output limit: nothing was folded".into(),
            ));
        }
        if matches!(reply.stop, StopReason::Interrupted) {
            return Err(AgentError::Model(
                "the summary was interrupted by its provider: nothing was folded".into(),
            ));
        }
        Ok(summary)
    }

    fn drive(
        &mut self,
        start: &EntryId,
        model: &mut dyn Model,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Outcome {
        events(AgentEvent::AgentStart {
            entry: start.to_string(),
        });
        // The run reads the person's queue while it lasts; what it did not read comes back.
        self.steering.open();
        let outcome = self.turns(model, events);
        self.steering.close();
        events(AgentEvent::AgentEnd { end: outcome.end() });
        outcome
    }

    fn turns(&mut self, model: &mut dyn Model, events: &mut dyn FnMut(AgentEvent)) -> Outcome {
        // What this run reads of earlier work as stubs, fixed now: never edited mid-run.
        let stubs = Stubs::of(&self.tree.context().messages);
        let mut turn: u32 = 0;
        let mut last: Option<((String, Value, String), u32)> = None;
        let mut asked_again = false;
        let (mut nudged, mut proposed) = (false, false);
        // At most one fold per step: a request over the window, or a reply that filled it, folds
        // the conversation once and asks the same step again.
        let mut refolded = false;
        loop {
            if self.cancel.is_cancelled() {
                return self.stop(StopReach::BetweenSteps);
            }
            let compacted = if refolded {
                false
            } else {
                match self.fit(model, events) {
                    Ok(compacted) => compacted,
                    Err(AgentError::Stopped) => return self.stop(StopReach::RequestDropped),
                    Err(error) => return Outcome::Failed { error },
                }
            };
            // A line steered while the context was compacted enters the request it prepared, once.
            if compacted && let Err(error) = self.dequeue(QueueMode::Steer, events) {
                return Outcome::Failed { error };
            }
            turn = turn.saturating_add(1);
            events(AgentEvent::TurnStart { turn });
            let reply = match self.ask(&stubs, compacted || refolded, model, events) {
                Asked::Reply(reply) => *reply,
                Asked::Folded => {
                    refolded = true;
                    continue;
                }
                Asked::Ended(outcome) => return outcome,
            };
            refolded = false;
            // A message cut at the output limit runs none of its calls; its answer says so.
            let cut = matches!(reply.stop, StopReason::MaxTokens);
            let (text, calls) = match self.record_reply(reply, events) {
                Ok(recorded) => recorded,
                Err(error) => return Outcome::Failed { error },
            };
            proposed = proposed || calls.iter().any(|(_, name, _)| name == "propose");
            if calls.is_empty() {
                if self.cancel.is_cancelled() {
                    return self.stop(StopReach::BetweenSteps);
                }
                let entered = match self.dequeue(QueueMode::Steer, events) {
                    Ok(false) => self.dequeue(QueueMode::FollowUp, events),
                    other => other,
                };
                match entered {
                    Ok(true) => continue,
                    // No word and no call (a model that only reasoned): asked once more.
                    Ok(false) if text.trim().is_empty() && !asked_again => {
                        asked_again = true;
                        if let Err(error) = self.note_to_author(SILENT) {
                            return Outcome::Failed { error };
                        }
                        continue;
                    }
                    Ok(false) if cut => {
                        let text = format!("{text}{CUT_ANSWER}");
                        return Outcome::Answered { text };
                    }
                    // Words that would end a turn that never proposed the candidate it wrote:
                    // asked once more to end it with a proposal, a question or a refusal.
                    Ok(false) if self.asks_to_propose(nudged, proposed) => {
                        nudged = true;
                        if let Err(error) = self.note_to_author(UNPROPOSED) {
                            return Outcome::Failed { error };
                        }
                        continue;
                    }
                    Ok(false) => return Outcome::Answered { text },
                    Err(error) => return Outcome::Failed { error },
                }
            }
            match self.run_calls(&calls, cut, &mut last, events) {
                Ok(Some((call, name))) => {
                    let queued = pairs(&self.steering.drain());
                    return Outcome::Parked { call, name, queued };
                }
                Ok(None) => {}
                Err(error) => return Outcome::Failed { error },
            }
            if self.cancel.is_cancelled() {
                return self.stop(StopReach::BetweenSteps);
            }
            if let Err(error) = self.dequeue(QueueMode::Steer, events) {
                return Outcome::Failed { error };
            }
        }
    }

    /// Nika's own note to the author, read with its next request (never the person's words).
    fn note_to_author(&mut self, text: &str) -> Result<(), AgentError> {
        let note = EntryKind::Note {
            text: text.to_owned(),
        };
        self.record(note).map(|_| ())
    }

    /// Whether words that would end this turn are asked once more to propose the candidate the
    /// author wrote: never twice, never after a `propose`, only while one waits unproposed.
    fn asks_to_propose(&self, nudged: bool, proposed: bool) -> bool {
        !nudged && !proposed && self.pending.is_some_and(|pending| pending())
    }

    /// A request over the window, or a reply that filled it (`window`: the provider's count, when
    /// it gave one): the conversation folded once, so the same step is asked again. A step that
    /// already `folded`, or a conversation with nothing earlier to fold, fails naming the window.
    fn refold(
        &mut self,
        folded: bool,
        window: Option<u64>,
        model: &mut dyn Model,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<(), AgentError> {
        let window = window.or(self.window.map(|window| window.tokens));
        let over = |why: &str| {
            let size = window.map_or_else(String::new, |tokens| format!(" of {tokens} tokens"));
            AgentError::Model(format!(
                "the request is over the model's context window{size}: {why}; nothing more was sent"
            ))
        };
        if folded {
            return Err(over(
                "the conversation was already folded once for this step",
            ));
        }
        match self.compact(None, model, events)? {
            Some(_) => Ok(()),
            None => Err(over("nothing earlier can be folded")),
        }
    }

    /// The model's message for the next step. A reply that filled the window, or a request over
    /// it, folds the conversation once so the same step is asked again (`refold`: a step that
    /// already `folded` fails naming the window). An answer its provider interrupted is no
    /// answer: nothing of it is kept or run, and the turn fails as the transient failure it is.
    fn ask(
        &mut self,
        stubs: &Stubs,
        folded: bool,
        model: &mut dyn Model,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Asked {
        let window = match model.complete(&self.request(stubs), events) {
            // A reply that filled the window is no answer: nothing of it is kept or run.
            Ok(reply) if reply.window_full => None,
            Ok(reply) if matches!(reply.stop, StopReason::Interrupted) => {
                return Asked::Ended(Outcome::Failed {
                    error: AgentError::Model(INTERRUPTED.to_owned()),
                });
            }
            Ok(reply) => return Asked::Reply(Box::new(reply)),
            Err(ModelError::Overflow { window }) => window,
            Err(ModelError::Stopped) => return Asked::Ended(self.stop(StopReach::RequestDropped)),
            Err(ModelError::Failed(why)) => {
                return Asked::Ended(Outcome::Failed {
                    error: AgentError::Model(why),
                });
            }
        };
        match self.refold(folded, window, model, events) {
            Ok(()) => Asked::Folded,
            Err(AgentError::Stopped) => Asked::Ended(self.stop(StopReach::RequestDropped)),
            Err(error) => Asked::Ended(Outcome::Failed { error }),
        }
    }

    /// Compact before a request that would leave the reply less than the window's reserve;
    /// returns whether it compacted.
    fn fit(
        &mut self,
        model: &mut dyn Model,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<bool, AgentError> {
        let Some(window) = self.window else {
            return Ok(false);
        };
        if !window.exceeded_by(compact::estimate(self.tree)) {
            return Ok(false);
        }
        self.compact(None, model, events)
            .map(|folded| folded.is_some())
    }

    /// The request the model reads now: the branch's context and the Session's tools.
    fn request(&self, stubs: &Stubs) -> Request {
        let mut context = self.tree.context();
        stubs.apply(&mut context.messages);
        let tools = (self.tools.tools().into_iter())
            .map(|tool| ToolDef::new(tool.name, tool.description, tool.input_schema))
            .collect();
        Request::new(context.system, context.messages, tools)
    }

    /// Record the model's message; returns its answer text and its calls.
    fn record_reply(
        &mut self,
        mut reply: Reply,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<(String, Vec<Call>), AgentError> {
        for block in &mut reply.content {
            if let ContentBlock::ToolUse { input, .. } = block {
                without_carriage_returns(input);
            }
        }
        let text = answer_text(&reply.content);
        let made: Vec<Call> = calls(&reply.content)
            .map(|(id, name, input)| (id.to_owned(), name.to_owned(), input.clone()))
            .collect();
        if let Some(usage) = &reply.usage {
            events(AgentEvent::Usage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
            });
        }
        let entry = self.record(EntryKind::Assistant {
            content: reply.content,
            stop: reply.stop,
            usage: reply.usage.map(Box::new),
            model: reply.model,
        })?;
        events(AgentEvent::MessageEnd {
            entry: entry.to_string(),
            text: text.clone(),
            calls: made.len(),
        });
        Ok((text, made))
    }

    /// Run the calls of one message in order. Returns the call that ended the turn, if one
    /// did; the calls after it, or after a steering line, are recorded as not run.
    fn run_calls(
        &mut self,
        made: &[Call],
        cut: bool,
        last: &mut Option<((String, Value, String), u32)>,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<Option<(String, String)>, AgentError> {
        let mut parked: Option<(String, String)> = None;
        let mut ahead: VecDeque<ToolReply> = VecDeque::new();
        for (k, (call, name, input)) in made.iter().enumerate() {
            // A Stop ends the message before its next call; the calls of a batch already run
            // (`ahead`) keep their replies.
            if parked.is_none() && ahead.is_empty() && self.cancel.is_cancelled() {
                break;
            }
            let skip = match &parked {
                Some((_, by)) => Some(format!(
                    "Not run: the turn ended on `{by}`, which waits for the person's answer; \
                     call it again afterwards if it is still needed."
                )),
                None if cut => Some(CUT.to_owned()),
                None if self.steering.pending(QueueMode::Steer) && hands_over(name) => {
                    Some(STEERED.to_owned())
                }
                None => None,
            };
            if let Some(reason) = skip {
                events(AgentEvent::ToolSkipped {
                    call: call.clone(),
                    name: name.clone(),
                    reason: reason.clone(),
                });
                let reply = ToolReply::error(reason);
                let (call, name) = (call.clone(), name.clone());
                self.record(EntryKind::ToolResult { call, name, reply })?;
                continue;
            }
            events(AgentEvent::ToolStart {
                call: call.clone(),
                name: name.clone(),
            });
            let mut reply = match ahead.pop_front() {
                Some(reply) => reply,
                None if name == OBSERVE => {
                    let run = made.get(k..).unwrap_or_default();
                    let len = run.iter().take_while(|(_, n, _)| n == OBSERVE).count();
                    ahead = self.observe_all(run.get(..len).unwrap_or_default());
                    (ahead.pop_front()).unwrap_or_else(|| ToolReply::error("nothing observed"))
                }
                None => {
                    let asked = ToolCall::new(name.clone(), input.clone()).with_meta(call.clone());
                    self.tools.call(asked)
                }
            };
            events(AgentEvent::ToolEnd {
                call: call.clone(),
                name: name.clone(),
                is_error: reply.is_error,
                ends_turn: reply.ends_turn,
            });
            let key = (name.clone(), input.clone(), steady(&reply.text));
            let times = match last.as_ref() {
                Some((previous, times)) if *previous == key => times + 1,
                _ => 1,
            };
            if times >= STALLED_AFTER {
                reply.text.push_str(&stalled(times));
            } else if times > 1 {
                reply.text.push_str(REPEATED);
            }
            *last = Some((key, times));
            let (call, name) = (call.clone(), name.clone());
            if reply.ends_turn {
                parked = Some((call.clone(), name.clone()));
                self.record(EntryKind::Parked { call, name, reply })?;
            } else {
                self.record(EntryKind::ToolResult { call, name, reply })?;
            }
        }
        Ok(parked)
    }

    /// Consecutive observations of one message, side by side: each reads one public page and no
    /// conversation state. Their replies come back in the message's order.
    fn observe_all(&self, run: &[Call]) -> VecDeque<ToolReply> {
        let tools = self.tools;
        std::thread::scope(|scope| {
            let handles: Vec<_> = (run.iter())
                .map(|(call, name, input)| {
                    let asked = ToolCall::new(name.clone(), input.clone()).with_meta(call.clone());
                    std::thread::Builder::new()
                        .name("nika-observe".to_owned())
                        .spawn_scoped(scope, move || tools.call(asked))
                })
                .collect();
            (handles.into_iter())
                .map(|handle| match handle {
                    Ok(running) => (running.join())
                        .unwrap_or_else(|_| ToolReply::error("the observation stopped")),
                    Err(why) => ToolReply::error(format!("the observation did not start: {why}")),
                })
                .collect()
        })
    }

    /// Record the queued lines of `mode` as the person's lines; returns whether any entered.
    fn dequeue(
        &mut self,
        mode: QueueMode,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<bool, AgentError> {
        self.queued_lines(mode, events)
            .map(|lines| !lines.is_empty())
    }

    /// Record the queued lines of `mode` as the person's lines; returns each with its citation.
    /// A line that could not be recorded returns to the queue with the ones after it.
    fn queued_lines(
        &mut self,
        mode: QueueMode,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<Vec<(String, String)>, AgentError> {
        let lines = self.steering.take(mode);
        let mut entered = Vec::with_capacity(lines.len());
        for (k, queued) in lines.iter().enumerate() {
            match self.user(&queued.line, None, Some(mode)) {
                Ok((entry, cite)) => {
                    self.steering.entered(&queued.id, &cite);
                    events(AgentEvent::Dequeued {
                        mode,
                        entry: entry.to_string(),
                    });
                    entered.push((queued.line.clone(), cite));
                }
                Err(error) => {
                    self.steering.requeue(&ids(&lines[k..]));
                    return Err(error);
                }
            }
        }
        Ok(entered)
    }

    /// Stop, as it reached the run: the queued lines return unsent, and the tree records that
    /// the person stopped.
    fn stop(&mut self, reach: StopReach) -> Outcome {
        self.stopped = Some(reach);
        let returned = self.steering.drain();
        let lines = returned.iter().map(|queued| queued.line.clone()).collect();
        match self.record(EntryKind::Stopped { queued: lines }) {
            Ok(_) => Outcome::Stopped {
                queued: pairs(&returned),
            },
            Err(error) => {
                self.steering.requeue(&ids(&returned));
                Outcome::Failed { error }
            }
        }
    }

    fn record(&mut self, kind: EntryKind) -> Result<EntryId, AgentError> {
        let at = (self.now)();
        let store = &mut *self.store;
        Ok(self.tree.append(kind, at, |line| store.append(line))?)
    }

    /// Record a person's line; returns its entry and its citation. The Session's reading goes with
    /// the line it read, the run's own line, whether it answers the waiting call, replies late to
    /// an earlier call or answers a question by its identity while nothing waits; a line queued
    /// during the run is another line, never the one read. What the line answers is unchanged.
    fn user(
        &mut self,
        text: &str,
        answers: Option<String>,
        queued: Option<QueueMode>,
    ) -> Result<(EntryId, String), AgentError> {
        let at = (self.now)();
        let reading = if queued.is_none() {
            self.reading.take()
        } else {
            None
        };
        let nika = match (self.note.take(), reading) {
            (Some(note), Some(reading)) => Some(format!("{note}\n\n{reading}")),
            (note, reading) => note.or(reading),
        };
        let store = &mut *self.store;
        let line = (answers, queued);
        Ok(self
            .tree
            .append_user_read(text, line, nika, at, |line| store.append(line))?)
    }
}

/// What asking the model for a step gave.
enum Asked {
    /// Its message, to record and run.
    Reply(Box<Reply>),
    /// The conversation was folded after an overflow: the same step is asked again.
    Folded,
    /// The turn ends with this outcome.
    Ended(Outcome),
}

/// Queued lines as an outcome returns them: each with how it waited.
fn pairs(queued: &[Queued]) -> Vec<(QueueMode, String)> {
    (queued.iter())
        .map(|queued| (queued.mode, queued.line.clone()))
        .collect()
}

/// The identities of queued lines.
fn ids(queued: &[Queued]) -> Vec<String> {
    queued.iter().map(|queued| queued.id.clone()).collect()
}

/// What the loop tells the model after a message with no word and no call: the person saw
/// nothing of it.
const SILENT: &str = "Your last message had no words and no call, so the person saw nothing. \
                      Answer them, ask them, or call a tool.";

/// What the author reads when its turn would end in words while the candidate it wrote waits
/// unproposed: asked once to end it with a proposal, a question or a refusal.
const UNPROPOSED: &str = "Your turn ended in words, but the candidate you wrote is not proposed, \
                          so the person cannot accept it. End the turn with `propose`, ask with \
                          `ask` what only they can say, or say why it cannot be done.";

/// The answer text of a message: its text blocks, in order.
fn answer_text(content: &[ContentBlock]) -> String {
    let texts: Vec<&str> = content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    texts.join("\n")
}

#[cfg(test)]
mod tests;
