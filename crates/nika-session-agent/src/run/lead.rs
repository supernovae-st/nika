// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A conversation an agent leads with its own loop (an ACP agent such as Claude Code or Codex):
//! the person's entry reaches it as one prompt, it calls the Session's tools itself through the
//! [`Relay`] a tool server serves, and the tree records what Nika's own loop records — the
//! person's cited lines, each call with its reply, the call that waits for the person, the
//! answer — under the same steering, Stop and outcomes. The agent keeps its own history; the
//! tree is the Session's record, never what that agent reads.
//!
//! A steering line stops the agent's turn (the agent is asked once) and enters as the next
//! prompt; a follow-up line enters when the agent ends its turn. A call that ends the turn
//! (`ask` asked the person) parks the run: the relay refuses the agent's later calls of that
//! turn, and the person's next line answers it. Between turns no call reaches a tool.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use nika_kernel::provider::{ContentBlock, StopReason};
use nika_session_change::outcome::StopReach;
use nika_session_change::tools::{SessionTools, ToolCall, ToolDef, ToolReply};
use serde_json::{Value, json};

use super::{Agent, AgentError, Outcome, pairs};
use crate::event::AgentEvent;
use crate::steer::QueueMode;
use crate::tree::{EntryKind, Tree};

/// What a call reaching the relay between turns reads.
const NO_TURN: &str = "No turn is under way: nothing runs until the person writes.";

/// What a call after one that waits for the person reads.
const WAITING: &str =
    "Not run: the person was asked; end your turn now, their answer comes as their next line.";

/// What the agent reads after the reply of a call that waits for the person.
const ASKED: &str =
    "\n\n(Nika: the person was asked. End your turn now; their answer comes as their next line.)";

/// The name of the fact a led turn's transport record is kept under.
pub const LED_TURN: &str = "led_turn";

/// One beat of a led turn, as the agent's loop streams it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Beat {
    /// Answer text.
    Answer(String),
    /// Thinking.
    Thought(String),
}

/// How a led turn ended.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LedEnd {
    /// The agent ended its turn.
    Answered,
    /// The agent stopped because it was asked to.
    Stopped,
    /// Any other end, in the transport's word (`max_tokens`, `refusal`, …).
    Other(String),
}

/// A led turn's end, with what its transport recorded of it (the stop, the permissions it
/// answered, the activity): evidence the tree keeps, never an authority.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Led {
    /// How the turn ended.
    pub end: LedEnd,
    /// The transport's closed facts about the turn.
    pub record: Value,
}

impl Led {
    /// Construct (INV-019).
    #[must_use]
    pub const fn new(end: LedEnd, record: Value) -> Self {
        Self { end, record }
    }
}

/// An agent that runs its own loop over the Session's tools and keeps its own history: one
/// prompt per entry.
pub trait Conversant {
    /// Send `text` as the next entry and wait until its turn ends, each beat to `beats`. `stop`
    /// is asked between beats; the first time it says so, the agent is asked to stop, once.
    ///
    /// # Errors
    ///
    /// The conversation cannot go on (its transport failed, its agent left), in its words.
    fn prompt(
        &mut self,
        text: &str,
        beats: &mut dyn FnMut(Beat),
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<Led, String>;

    /// Whether the conversation takes no further prompt (its transport failed, its agent left):
    /// the next entry needs a new one.
    fn ended(&self) -> bool {
        false
    }
}

/// One call a relay served: the agent's identity of it (when it gave one), the call, the reply.
type Served = (Option<String>, ToolCall, ToolReply);

/// The Session's tools as an agent with its own loop reaches them: a call runs while a turn is
/// under way and no earlier call of that turn waits for the person, and each call is kept for
/// the tree. Calls are served one at a time.
pub struct Relay {
    tools: Arc<dyn SessionTools>,
    turn: Mutex<Turn>,
}

#[derive(Default)]
struct Turn {
    open: bool,
    parked: bool,
    served: Vec<Served>,
}

impl Relay {
    /// The relay of `tools`, closed until a turn begins.
    #[must_use]
    pub fn new(tools: Arc<dyn SessionTools>) -> Self {
        Self {
            tools,
            turn: Mutex::new(Turn::default()),
        }
    }

    fn turn(&self) -> MutexGuard<'_, Turn> {
        self.turn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A turn begins: calls reach the tools.
    fn open(&self) {
        *self.turn() = Turn {
            open: true,
            ..Turn::default()
        };
    }

    /// The turn ended: no call reaches the tools any more. Returns the calls it served, in
    /// order, once a call under way answered.
    fn close(&self) -> Vec<Served> {
        let mut turn = self.turn();
        turn.open = false;
        std::mem::take(&mut turn.served)
    }
}

impl SessionTools for Relay {
    fn tools(&self) -> Vec<ToolDef> {
        self.tools.tools()
    }

    fn call(&self, call: ToolCall) -> ToolReply {
        let mut turn = self.turn();
        if !turn.open {
            return ToolReply::error(NO_TURN);
        }
        let reply = if turn.parked {
            ToolReply::error(WAITING)
        } else {
            self.tools.call(call.clone())
        };
        turn.parked |= reply.ends_turn;
        turn.served.push((call.meta.clone(), call, reply.clone()));
        drop(turn);
        if reply.ends_turn {
            return ToolReply::ok(format!("{}{ASKED}", reply.text));
        }
        reply
    }
}

/// A person's entry as the agent reads it: their words with their citation, or their answer to
/// what the run waited on.
fn entry_text(text: &str, cite: &str, answering: bool) -> String {
    if answering {
        format!("The person answered, cited as {cite}:\n{text}")
    } else {
        format!("{text}\n\n(cited as {cite})")
    }
}

/// The conversation recorded so far, for an agent whose own session did not see it: what was
/// folded, the person's cited lines, what the author said, called and asked. Evidence: only the
/// person's lines carry their words. None when nothing was said yet.
#[must_use]
pub fn transcript(tree: &Tree) -> Option<String> {
    let branch = tree.branch();
    let (summary, kept) = Tree::kept(&branch);
    let mut lines: Vec<String> = Vec::new();
    if let Some(summary) = summary {
        lines.push(format!(
            "A summary of the earlier conversation (evidence, not the person's words):\n{summary}"
        ));
    }
    for entry in kept {
        match &entry.kind {
            EntryKind::User {
                cite,
                text,
                answers,
                ..
            } => lines.push(if answers.is_some() {
                format!("The person answered (cited as {cite}): {text}")
            } else {
                format!("The person (cited as {cite}): {text}")
            }),
            EntryKind::Assistant { content, .. } => lines.extend(content.iter().filter_map(said)),
            EntryKind::Stopped { .. } => lines.push("The person stopped the run here.".to_owned()),
            _ => {}
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// One block of the author's message, as a transcript line.
fn said(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text { text } if !text.trim().is_empty() => {
            Some(format!("The author said: {}", text.trim()))
        }
        ContentBlock::ToolUse { name, input, .. } if name == "ask" => {
            let asked: Vec<&str> = (input["questions"].as_array().into_iter().flatten())
                .filter_map(|q| q["question"].as_str())
                .collect();
            Some(format!("The author asked: {}", asked.join(" · ")))
        }
        ContentBlock::ToolUse { name, .. } => Some(format!("The author called `{name}`.")),
        _ => None,
    }
}

impl Agent<'_> {
    /// A person's line goes to an agent that leads with its own loop (`conversant`) and reaches
    /// the Session's tools through `relay`; `opening` precedes the line in the first prompt of
    /// the agent's session (the instructions, the conversation so far). While a call waits for
    /// the person, the line answers it.
    pub fn lead(
        &mut self,
        text: &str,
        opening: Option<&str>,
        conversant: &mut dyn Conversant,
        relay: &Relay,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Outcome {
        let answers = self.tree.parked().map(|(call, _)| call.to_owned());
        let answering = answers.is_some();
        let (entry, cite) = match self.user(text, answers, None) {
            Ok(user) => user,
            Err(error) => return Outcome::Failed { error },
        };
        events(AgentEvent::AgentStart {
            entry: entry.to_string(),
        });
        let line = entry_text(text, &cite, answering);
        let prompt = opening.map_or_else(|| line.clone(), |opening| format!("{opening}\n\n{line}"));
        self.steering.open();
        let outcome = self.led(prompt, conversant, relay, events);
        self.steering.close();
        events(AgentEvent::AgentEnd { end: outcome.end() });
        outcome
    }

    fn led(
        &mut self,
        mut prompt: String,
        conversant: &mut dyn Conversant,
        relay: &Relay,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Outcome {
        let mut turn: u32 = 0;
        loop {
            if self.cancel.is_cancelled() {
                return self.stop(StopReach::BetweenSteps);
            }
            turn = turn.saturating_add(1);
            events(AgentEvent::TurnStart { turn });
            let (cancel, steering) = (self.cancel.clone(), self.steering.clone());
            let (mut text, mut steered) = (String::new(), false);
            relay.open();
            let led = conversant.prompt(
                &prompt,
                &mut |beat| match beat {
                    Beat::Answer(chunk) => {
                        text.push_str(&chunk);
                        events(AgentEvent::TextDelta { text: chunk });
                    }
                    Beat::Thought(chunk) => events(AgentEvent::ThinkingDelta { text: chunk }),
                },
                &mut || {
                    steered = steered || steering.pending(QueueMode::Steer);
                    steered || cancel.is_cancelled()
                },
            );
            let led = match self.settle_turn(relay.close(), led, &text, events) {
                Ok(Settled::Parked(call, name)) => {
                    let queued = pairs(&self.steering.drain());
                    return Outcome::Parked { call, name, queued };
                }
                Ok(Settled::Ended(led)) => led,
                Err(error) => return Outcome::Failed { error },
            };
            if self.cancel.is_cancelled() {
                // The agent ended its turn on the cancel it was sent, or before reading it.
                let reach = if led.end == LedEnd::Stopped {
                    StopReach::AgentCancelled
                } else {
                    StopReach::BetweenSteps
                };
                return self.stop(reach);
            }
            let next = match &led.end {
                LedEnd::Answered => match self.queued_lines(QueueMode::Steer, events) {
                    Ok(lines) if lines.is_empty() => self.queued_lines(QueueMode::FollowUp, events),
                    other => other,
                },
                LedEnd::Stopped if steered => self.queued_lines(QueueMode::Steer, events),
                LedEnd::Stopped => Err(AgentError::Model("the agent stopped its turn".into())),
                LedEnd::Other(word) => Err(AgentError::Model(format!(
                    "the agent's turn ended `{word}`"
                ))),
            };
            match next {
                Ok(lines) if lines.is_empty() && led.end == LedEnd::Answered => {
                    return Outcome::Answered { text };
                }
                Ok(lines) if lines.is_empty() => return self.stop(StopReach::BetweenSteps),
                Ok(lines) => {
                    let entered: Vec<String> = (lines.iter())
                        .map(|(line, cite)| entry_text(line, cite, false))
                        .collect();
                    prompt = entered.join("\n\n");
                }
                Err(error) => return Outcome::Failed { error },
            }
        }
    }

    /// Record a turn the agent led: the calls the relay served, the transport's record, and
    /// what the agent said when it ended its turn or asked the person.
    fn settle_turn(
        &mut self,
        served: Vec<Served>,
        led: Result<Led, String>,
        text: &str,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<Settled, AgentError> {
        let parked = self.record_served(served, events)?;
        let (led, parked) = match (led, parked) {
            (Ok(led), parked) => (led, parked),
            // The question was asked and is durable: it waits for the person although the
            // agent's turn did not end cleanly, and the failure stays as evidence.
            (Err(why), Some((call, name))) => {
                self.record(EntryKind::Fact {
                    name: LED_TURN.to_owned(),
                    data: json!({"error": why}),
                })?;
                return Ok(Settled::Parked(call, name));
            }
            (Err(why), None) => return Err(AgentError::Model(why)),
        };
        let data = led.record.clone();
        self.record(EntryKind::Fact {
            name: LED_TURN.to_owned(),
            data,
        })?;
        let ended = parked.is_some() || led.end == LedEnd::Answered;
        if ended && !text.trim().is_empty() {
            let content = vec![ContentBlock::Text {
                text: text.to_owned(),
            }];
            let entry = self.record(EntryKind::Assistant {
                content,
                stop: StopReason::EndTurn,
                usage: None,
                model: None,
            })?;
            events(AgentEvent::MessageEnd {
                entry: entry.to_string(),
                text: text.to_owned(),
                calls: 0,
            });
        }
        Ok(match parked {
            Some((call, name)) => Settled::Parked(call, name),
            None => Settled::Ended(led),
        })
    }

    /// Record the calls a relay served, in order: each as the agent's call and its reply.
    /// Returns the call that ended the turn, if one did.
    fn record_served(
        &mut self,
        served: Vec<Served>,
        events: &mut dyn FnMut(AgentEvent),
    ) -> Result<Option<(String, String)>, AgentError> {
        let mut parked = None;
        for (meta, call, reply) in served {
            let id = meta.unwrap_or_else(|| format!("led-{}", self.tree.entries().len() + 1));
            let ToolCall {
                name, arguments, ..
            } = call;
            let content = vec![ContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: arguments,
            }];
            self.record(EntryKind::Assistant {
                content,
                stop: StopReason::ToolUse,
                usage: None,
                model: None,
            })?;
            events(AgentEvent::ToolEnd {
                call: id.clone(),
                name: name.clone(),
                is_error: reply.is_error,
                ends_turn: reply.ends_turn,
            });
            if reply.ends_turn && parked.is_none() {
                parked = Some((id.clone(), name.clone()));
                self.record(EntryKind::Parked {
                    call: id,
                    name,
                    reply,
                })?;
            } else {
                self.record(EntryKind::ToolResult {
                    call: id,
                    name,
                    reply,
                })?;
            }
        }
        Ok(parked)
    }
}

/// What a recorded turn leaves the run with.
enum Settled {
    /// A call waits for the person: its identity and tool.
    Parked(String, String),
    /// The turn ended this way.
    Ended(Led),
}

#[cfg(test)]
mod tests;
