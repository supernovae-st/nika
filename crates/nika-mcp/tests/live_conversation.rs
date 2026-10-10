// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! LIVE qualification of the persistent ACP conversation over a REAL adapter on this machine,
//! with this crate's real tool server — ignored by default, never part of CI, and run only by
//! an operator who accepts spending their own subscription.
//!
//! ```text
//! NIKA_LIVE_CONVERSATION=claude-code [NIKA_LIVE_MODEL=<offered>] [NIKA_LIVE_EFFORT=<offered>] \
//!   cargo test -p nika-mcp --locked --test live_conversation -- --ignored --nocapture
//! ```
//!
//! Each step prints ONE JSON line of closed facts (the transport, beats by kind, permission
//! decisions, the turn's end and stop, whether the checked token was answered), never answer
//! text and never a credential:
//!
//! 1. `open`: the opening record (transport chosen, advertised, offered);
//! 2. `nika_tool`: the agent calls `candidate_read`, which returns a fresh token; the session
//!    saw that call with the agent's tool-use id, and the answer carries the token;
//! 3. `history`: the agent repeats the token it answered before, with no tool;
//! 4. `foreign_tool`: asked to run a shell command, the agent has no built-in tool; any
//!    permission it asks is rejected and recorded, and the marker file never appears;
//! 5. `stop`: a long answer stopped after its first beat ends `cancelled`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::disallowed_methods)] // a live, operator-run qualification reads its env

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nika_harness::{ConversationSetup, ToolOffer, TurnEvent, TurnStream, seat_from_id};
use nika_mcp::conversation::{SERVER_NAME, ToolServer};
use nika_session_change::tools::{SessionTools, ToolCall, ToolDef, ToolReply};
use serde_json::{Value, json};

/// A session serving one read-only tool that answers a fresh token.
struct Probe {
    token: String,
    calls: Mutex<Vec<ToolCall>>,
}

impl SessionTools for Probe {
    fn tools(&self) -> Vec<ToolDef> {
        let schema = json!({"type": "object", "properties": {}, "additionalProperties": false});
        let what = "Read the Session's current workflow candidate; returns its revision token.";
        vec![ToolDef::new("candidate_read", what, schema, true)]
    }

    fn call(&self, call: ToolCall) -> ToolReply {
        self.calls.lock().expect("calls").push(call);
        ToolReply::ok(format!("candidate revision token: {}", self.token))
    }
}

fn line(step: &str, row: &Value) {
    // The receipt IS this qualification's output (stdout, one JSON line per step).
    #[allow(clippy::disallowed_macros, clippy::print_stdout)]
    {
        println!("LIVE {}", json!({"step": step, "row": row}));
    }
}

/// Every beat of a turn, an error kept as its words.
async fn beats(mut turn: TurnStream) -> (Vec<TurnEvent>, Option<String>) {
    let mut seen = Vec::new();
    while let Some(beat) = turn.next_beat().await {
        match beat {
            Ok(beat) => seen.push(beat),
            Err(error) => return (seen, Some(error.to_string())),
        }
    }
    (seen, None)
}

/// The closed facts of a turn: beats by kind, the decisions, the end and its record's stop,
/// permissions and activity frames.
fn summary(beats: &[TurnEvent], error: Option<&str>) -> Value {
    let count = |kind: fn(&TurnEvent) -> bool| beats.iter().filter(|beat| kind(beat)).count();
    let decisions: Vec<Value> = (beats.iter())
        .filter_map(|beat| match beat {
            TurnEvent::Permission {
                tool,
                nika_tool,
                allowed,
            } => Some(json!({"tool": tool, "nika_tool": nika_tool, "allowed": allowed})),
            _ => None,
        })
        .collect();
    let ended = beats.iter().find_map(|beat| match beat {
        TurnEvent::Ended { end, record } => Some(json!({"end": end.as_str(),
            "stop": record["stop"], "permissions": record["permissions"],
            "frames": record["activity"]["session"]["frames"],
            "first_thought_ms": record["activity"]["session"]["first_thought_ms"],
            "first_answer_ms": record["activity"]["session"]["first_answer_ms"],
            "elapsed_ms": record["elapsed_ms"]})),
        _ => None,
    });
    json!({"answers": count(|beat| matches!(beat, TurnEvent::Answer { .. })),
        "thoughts": count(|beat| matches!(beat, TurnEvent::Thought { .. })),
        "tool_beats": count(|beat| matches!(beat, TurnEvent::Tool { .. })),
        "decisions": decisions, "ended": ended, "error": error})
}

fn answer(beats: &[TurnEvent]) -> String {
    (beats.iter())
        .filter_map(|beat| match beat {
            TurnEvent::Answer { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "live: spends the operator's own subscription on a real adapter"]
async fn live_conversation() {
    let Ok(seat) = std::env::var("NIKA_LIVE_CONVERSATION") else {
        return;
    };
    let harness = seat_from_id(&seat)
        .expect("a registry seat")
        .expect("the seat is enabled");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a clock")
        .as_nanos();
    let token = format!("nika-{nanos}");
    let probe = Arc::new(Probe {
        token: token.clone(),
        calls: Mutex::new(Vec::new()),
    });
    let server = ToolServer::start(Arc::clone(&probe) as Arc<dyn SessionTools>).expect("binds");
    let offer = ToolOffer::new(SERVER_NAME, server.tool_names())
        .with_http(server.url().expect("url"), server.bearer());
    let closer = server.closer().expect("a closer");
    let serving = tokio::task::spawn_blocking(move || server.serve());

    let mut setup = ConversationSetup::new().with_allowance(Duration::from_secs(600));
    if let Ok(model) = std::env::var("NIKA_LIVE_MODEL") {
        setup = setup.with_requested_model(model);
    }
    if let Ok(effort) = std::env::var("NIKA_LIVE_EFFORT") {
        setup = setup.with_requested_effort(effort);
    }
    let conversation = match harness.converse(setup, offer).await {
        Ok(conversation) => conversation,
        Err(error) => {
            line(
                "open",
                &json!({"outcome": "refused", "error": error.to_string()}),
            );
            return;
        }
    };
    line("open", conversation.record());

    let ask = "Call the candidate_read tool, then reply with only the revision token it returns.";
    let (seen, error) = beats(conversation.prompt(ask).expect("a turn")).await;
    let calls = probe.calls.lock().expect("calls").clone();
    let named = calls.iter().filter(|call| call.meta.is_some()).count();
    let row = json!({"summary": summary(&seen, error.as_deref()),
        "token_answered": answer(&seen).contains(&token), "calls": calls.len(),
        "calls_naming_their_tool_use": named});
    line("nika_tool", &row);

    let ask = "Without calling any tool, repeat the revision token you replied with before. \
               Reply with the token only.";
    let (seen, error) = beats(conversation.prompt(ask).expect("a turn")).await;
    let row = json!({"summary": summary(&seen, error.as_deref()),
        "token_answered": answer(&seen).contains(&token)});
    line("history", &row);

    let marker = std::env::temp_dir().join(format!("nika-live-marker-{nanos}"));
    let ask = format!(
        "Run the shell command `touch {}` and tell me whether it worked.",
        marker.display()
    );
    let (seen, error) = beats(conversation.prompt(ask).expect("a turn")).await;
    let row = json!({"summary": summary(&seen, error.as_deref()),
        "marker_exists": marker.exists()});
    line("foreign_tool", &row);

    let ask = "Write a detailed 2000-word essay on the history of workflow engines.";
    let mut turn = conversation.prompt(ask).expect("a turn");
    let first = turn.next_beat().await;
    conversation.stop();
    let (rest, error) = beats(turn).await;
    let first = match first {
        Some(Ok(TurnEvent::Answer { .. })) => "answer",
        Some(Ok(TurnEvent::Thought { .. })) => "thought",
        Some(Ok(_)) => "other",
        Some(Err(_)) => "error",
        None => "none",
    };
    let row = json!({"first_beat": first, "summary": summary(&rest, error.as_deref())});
    line("stop", &row);

    drop(conversation);
    closer.close();
    serving.await.expect("the server stops once closed");
}
