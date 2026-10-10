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

use std::io;

use nika_kernel::CancelCtx;
use nika_kernel::provider::{ContentBlock, Message, StopReason, TokenUsage, ToolDef};
use nika_session_change::outcome::StopReach;
use nika_session_change::tools::{SessionTools, ToolCall, ToolReply};
use serde_json::Value;

use crate::compact::{self, Window};
use crate::event::{AgentEvent, End};
use crate::steer::{QueueMode, Queued, Steering};
use crate::tree::{EntryId, EntryKind, Tree, TreeError, calls};

mod lead;

pub use lead::{Beat, Conversant, LED_TURN, Led, LedEnd, Relay, transcript};

/// What the model reads after a call that repeats the previous one exactly, reply included.
const REPEATED: &str = "\n\n(Nika: this call and its reply repeat the previous call exactly.)";

/// Why a call after a steering line is not run.
const STEERED: &str = "Not run: the person wrote meanwhile; read their message first.";

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
        }
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
        }
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
    /// ([`Agent::answer`]) instead.
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
        match self.user(text, None, None) {
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
        let request = compact::request(self.tree, &first_kept, focus);
        // A summary is no answer: nothing it streams reaches the person.
        let reply =
            model
                .complete(&request, &mut |_: AgentEvent| {})
                .map_err(|error| match error {
                    ModelError::Stopped => AgentError::Stopped,
                    ModelError::Failed(why) => AgentError::Model(why),
                })?;
        let summary = answer_text(&reply.content);
        if summary.trim().is_empty() {
            return Err(AgentError::Model("the summary came back empty".into()));
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
        let mut turn: u32 = 0;
        let mut last: Option<(String, Value, String)> = None;
        loop {
            if self.cancel.is_cancelled() {
                return self.stop(StopReach::BetweenSteps);
            }
            match self.fit(model, events) {
                Ok(()) => {}
                Err(AgentError::Stopped) => return self.stop(StopReach::RequestDropped),
                Err(error) => return Outcome::Failed { error },
            }
            turn = turn.saturating_add(1);
            events(AgentEvent::TurnStart { turn });
            let reply = match model.complete(&self.request(), events) {
                Ok(reply) => reply,
                Err(ModelError::Stopped) => return self.stop(StopReach::RequestDropped),
                Err(ModelError::Failed(why)) => {
                    return Outcome::Failed {
                        error: AgentError::Model(why),
                    };
                }
            };
            let (text, calls) = match self.record_reply(reply, events) {
                Ok(recorded) => recorded,
                Err(error) => return Outcome::Failed { error },
            };
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
                    Ok(false) => return Outcome::Answered { text },
                    Err(error) => return Outcome::Failed { error },
                }
            }
            match self.run_calls(&calls, &mut last, events) {
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

    /// Compact before a request that would leave the reply less than the window's reserve.
    fn fit(
        &mut self,
        model: &mut dyn Model,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<(), AgentError> {
        let Some(window) = self.window else {
            return Ok(());
        };
        if !window.exceeded_by(compact::estimate(self.tree)) {
            return Ok(());
        }
        self.compact(None, model, events).map(|_| ())
    }

    /// The request the model reads now: the branch's context and the Session's tools.
    fn request(&self) -> Request {
        let context = self.tree.context();
        let tools = (self.tools.tools().into_iter())
            .map(|tool| ToolDef::new(tool.name, tool.description, tool.input_schema))
            .collect();
        Request::new(context.system, context.messages, tools)
    }

    /// Record the model's message; returns its answer text and its calls.
    fn record_reply(
        &mut self,
        reply: Reply,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<(String, Vec<Call>), AgentError> {
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
        last: &mut Option<(String, Value, String)>,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<Option<(String, String)>, AgentError> {
        let mut parked: Option<(String, String)> = None;
        for (call, name, input) in made {
            if parked.is_none() && self.cancel.is_cancelled() {
                break;
            }
            let skip = match &parked {
                Some((_, by)) => Some(format!(
                    "Not run: the turn ended on `{by}`, which waits for the person's answer; \
                     call it again afterwards if it is still needed."
                )),
                None if self.steering.pending(QueueMode::Steer) => Some(STEERED.to_owned()),
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
            let asked = ToolCall::new(name.clone(), input.clone()).with_meta(call.clone());
            let mut reply = self.tools.call(asked);
            events(AgentEvent::ToolEnd {
                call: call.clone(),
                name: name.clone(),
                is_error: reply.is_error,
                ends_turn: reply.ends_turn,
            });
            let key = (name.clone(), input.clone(), reply.text.clone());
            if last.as_ref() == Some(&key) {
                reply.text.push_str(REPEATED);
            }
            *last = Some(key);
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

    /// Record a person's line; returns its entry and its citation.
    fn user(
        &mut self,
        text: &str,
        answers: Option<String>,
        queued: Option<QueueMode>,
    ) -> Result<(EntryId, String), AgentError> {
        let at = (self.now)();
        let reading = answers.as_ref().and(self.reading.take());
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
