// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a run tells a host while it happens: one stream for the terminal, the JSON door, Serve
//! and the SDK (`nika/session-events@0`). An event reports; it records nothing and authorizes
//! nothing: the tree is the record, and only the Session's doors act.

use serde::{Deserialize, Serialize};

use crate::steer::QueueMode;

/// The version a host checks before rendering these events.
pub const EVENTS: &str = "nika/session-events@0";

/// Why a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum End {
    /// The model answered without a call, and no line was queued.
    Answered,
    /// A call ended the turn: the run waits for the person's answer.
    Parked,
    /// The person stopped the run.
    Stopped,
    /// The model or the tree failed; what was recorded stays.
    Failed,
}

/// One event of a run, in the order it happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentEvent {
    /// A run began at the person's line `entry`.
    AgentStart {
        /// The tree entry the run began at.
        entry: String,
    },
    /// One request to the model began; `turn` counts the requests of this run from 1.
    TurnStart {
        /// The request's rank in this run.
        turn: u32,
    },
    /// Text the model streamed.
    TextDelta {
        /// The text, as streamed.
        text: String,
    },
    /// Thinking the model streamed.
    ThinkingDelta {
        /// The thinking, as streamed.
        text: String,
    },
    /// The model's message is recorded at `entry`.
    MessageEnd {
        /// The tree entry of the message.
        entry: String,
        /// Its answer text, every text block in order.
        text: String,
        /// How many calls it made.
        calls: usize,
    },
    /// The tokens one response used, as the provider reported them.
    Usage {
        /// Input tokens, cached ones included.
        input_tokens: u64,
        /// Output tokens, reasoning included.
        output_tokens: u64,
    },
    /// A call began.
    ToolStart {
        /// The model's identity of the call.
        call: String,
        /// The tool called.
        name: String,
    },
    /// A call answered.
    ToolEnd {
        /// The model's identity of the call.
        call: String,
        /// The tool called.
        name: String,
        /// The reply reports a failure.
        is_error: bool,
        /// The turn ends on the reply: the run waits for the person.
        ends_turn: bool,
    },
    /// A call was not run.
    ToolSkipped {
        /// The model's identity of the call.
        call: String,
        /// The tool called.
        name: String,
        /// Why, as the model reads it.
        reason: String,
    },
    /// A line the person sent while a run was under way entered the conversation at `entry`.
    Dequeued {
        /// How the line waited.
        mode: QueueMode,
        /// The tree entry of the line.
        entry: String,
    },
    /// Earlier entries were folded into the summary at `entry`.
    Compacted {
        /// The tree entry of the summary.
        entry: String,
        /// The estimated size of the conversation before, in tokens.
        tokens_before: u64,
    },
    /// The run ended.
    AgentEnd {
        /// Why it ended.
        end: End,
    },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A host reads each event by its `type`, with snake-case words and the fields named here.
    #[test]
    fn events_reach_a_host_under_their_wire_names() {
        let cases = [
            (
                AgentEvent::AgentStart { entry: "e2".into() },
                json!({"type": "agent_start", "entry": "e2"}),
            ),
            (
                AgentEvent::ToolEnd {
                    call: "c1".into(),
                    name: "ask".into(),
                    is_error: false,
                    ends_turn: true,
                },
                json!({"type": "tool_end", "call": "c1", "name": "ask", "is_error": false,
                    "ends_turn": true}),
            ),
            (
                AgentEvent::Dequeued {
                    mode: QueueMode::FollowUp,
                    entry: "e9".into(),
                },
                json!({"type": "dequeued", "mode": "follow_up", "entry": "e9"}),
            ),
            (
                AgentEvent::AgentEnd { end: End::Parked },
                json!({"type": "agent_end", "end": "parked"}),
            ),
        ];
        for (event, wire) in cases {
            assert_eq!(serde_json::to_value(&event).unwrap(), wire);
            assert_eq!(serde_json::from_value::<AgentEvent>(wire).unwrap(), event);
        }
        assert_eq!(EVENTS, "nika/session-events@0");
    }
}
