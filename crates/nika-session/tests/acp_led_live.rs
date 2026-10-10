// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(all(unix, feature = "access-harness"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]

//! LIVE qualification of a Session led by a REAL ACP agent on this machine — the Session-led
//! step of `nika-mcp`'s `live_conversation` probe: the agent reaches the Session's real tools
//! over the conversation's tool server, and the Session keeps the tree, the identities and the
//! effects. Ignored by default, never part of CI, and run only by an operator who accepts
//! spending their own subscription.
//!
//! ```text
//! NIKA_LIVE_CONVERSATION=claude-code [NIKA_LIVE_MODEL=<offered>] \
//!   cargo test -p nika-session --locked --test acp_led_live -- --ignored --nocapture
//! ```
//!
//! The project and the Session's history live in a temporary directory; the agent runs under the
//! operator's own sign-in. Each step prints ONE JSON line of closed facts (outcome kinds, what
//! waits, the tools the agent called, the turn records' stop and permissions, whether a file
//! appeared), never answer text and never a credential:
//!
//! 1. `request`: an ordinary request; the agent asks or proposes through Nika's tools;
//! 2. `answer`: when it asked, the person accepts its recommendation;
//! 3. `foreign_tool`: asked to run a shell command, any permission it asks is rejected and the
//!    marker file never appears;
//! 4. `stop`: a long request stopped after three seconds ends `cancelled`;
//! 5. `after_stop`: the next line still rides the same agent session.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nika_session::intelligence::{IntelligenceCensus, IntelligenceKind, SeatSeen};
use nika_session::reasoner::HarnessReasoner;
use nika_session::work::Waiting;
use nika_session::{
    ResolvedSessionIntelligence, SessionReasoner, SessionRuntime, TurnOutcome,
    UserIntelligencePreference,
};
use nika_types::access::HarnessTransport;
use serde_json::{Value, json};

const REQUEST: &str = "fais moi un workflow tres simple qui recupere les news tech recentes, les resume et ecrit le resultat en markdown dans un dossier du projet";
const ACCEPT: &str = "oui tout me va, je suis tes recos";

fn line(step: &str, row: &Value) {
    // The receipt IS this qualification's output (stdout, one JSON line per step).
    #[allow(clippy::disallowed_macros, clippy::print_stdout)]
    {
        println!("LIVE {}", json!({"step": step, "row": row}));
    }
}

fn kind(outcome: &TurnOutcome) -> &'static str {
    match outcome {
        TurnOutcome::Question { .. } => "question",
        TurnOutcome::Proposal { .. } => "proposal",
        TurnOutcome::Reply(_) => "reply",
        TurnOutcome::Cancelled(_) => "cancelled",
        TurnOutcome::Refusal(_) => "refusal",
        TurnOutcome::Facts(_) => "facts",
        _ => "other",
    }
}

/// The entries of the Session tree kept under `home`.
fn tree(home: &Path) -> Vec<Value> {
    let Ok(dirs) = std::fs::read_dir(home.join(".nika/sessions")) else {
        return Vec::new();
    };
    let found = (dirs.filter_map(Result::ok))
        .map(|entry| entry.path().join("tree.jsonl"))
        .find(|path| path.exists());
    let text = found.map(std::fs::read_to_string).and_then(Result::ok);
    (text.unwrap_or_default().lines())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|line| line["body"]["entry"].clone())
        .filter(|entry| !entry.is_null())
        .collect()
}

/// The closed facts of what the tree recorded after its first `from` entries: the tools the
/// agent called, and each turn's end, stop and permissions.
fn recorded(home: &Path, from: usize) -> (Value, usize) {
    let entries = tree(home);
    let fresh = entries.get(from..).unwrap_or_default();
    let called: Vec<&Value> = (fresh.iter())
        .filter(|e| e["kind"]["type"] == "assistant")
        .flat_map(|e| e["kind"]["content"].as_array().into_iter().flatten())
        .filter(|block| block["type"] == "tool_use")
        .map(|block| &block["name"])
        .collect();
    let turns: Vec<Value> = (fresh.iter())
        .filter(|e| e["kind"]["type"] == "fact" && e["kind"]["name"] == "led_turn")
        .map(|e| json!({"stop": e["kind"]["data"]["stop"], "permissions": e["kind"]["data"]["permissions"]}))
        .collect();
    (json!({"called": called, "turns": turns}), entries.len())
}

fn shown(session: &SessionRuntime) -> Value {
    let work = serde_json::to_value(session.work()).unwrap_or_default();
    let provenance: Vec<&Value> = (work["bindings"].as_array().into_iter().flatten())
        .map(|b| &b["provenance"]["kind"])
        .collect();
    json!({"waiting": serde_json::to_value(session.waiting()).unwrap_or_default()["kind"],
        "questions": work["questions"].as_array().map_or(0, Vec::len),
        "provenance": provenance, "proposal": session.pending_proposal().is_some()})
}

#[test]
#[ignore = "live: spends the operator's own subscription on a real agent"]
fn live_session_led_by_an_acp_agent() {
    let Ok(seat) = std::env::var("NIKA_LIVE_CONVERSATION") else {
        return;
    };
    let model = std::env::var("NIKA_LIVE_MODEL").ok();
    let dir = tempfile::tempdir().expect("a scratch");
    let (root, home) = (dir.path().join("project"), dir.path().join("history"));
    std::fs::create_dir_all(&root).expect("a project");
    std::fs::create_dir_all(&home).expect("a history home");
    let mut census = IntelligenceCensus::empty();
    census
        .seats
        .push(SeatSeen::new(seat.clone(), true, true, true));
    let chosen = IntelligenceKind::Harness {
        seat: seat.clone(),
        transport: HarnessTransport::Acp,
    };
    let pref = UserIntelligencePreference::new(chosen, model);
    let factory = Box::new(
        |resolved: &ResolvedSessionIntelligence| -> Box<dyn SessionReasoner> {
            match &resolved.kind {
                IntelligenceKind::Harness { seat, transport } => Box::new(
                    HarnessReasoner { seat: seat.clone() }
                        .with_transport(resolved.model.clone(), *transport),
                ),
                other => panic!("the operator chose a seat, not {other:?}"),
            }
        },
    );
    let mut session = SessionRuntime::open_with(&root, census, &pref, Some(&home), factory);
    session.enable_history(&home).expect("history opens");

    let _ = session.begin_preparation_turn();
    let asked = session.turn(REQUEST);
    let (facts, mut seen) = recorded(&home, 0);
    line(
        "request",
        &json!({"outcome": kind(&asked), "shown": shown(&session), "recorded": facts}),
    );

    if matches!(
        session.waiting(),
        Waiting::Question { .. } | Waiting::Questions { .. }
    ) {
        let _ = session.begin_preparation_turn();
        let now = session.waiting();
        let answered = session.submit(ACCEPT, &now);
        let (facts, count) = recorded(&home, seen);
        seen = count;
        line(
            "answer",
            &json!({"outcome": kind(&answered), "shown": shown(&session), "recorded": facts}),
        );
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a clock")
        .as_nanos();
    let marker = std::env::temp_dir().join(format!("nika-live-led-marker-{nanos}"));
    let _ = session.begin_preparation_turn();
    let ask = format!(
        "Run the shell command `touch {}` and tell me whether it worked.",
        marker.display()
    );
    let foreign = session.turn(&ask);
    let (facts, count) = recorded(&home, seen);
    seen = count;
    line(
        "foreign_tool",
        &json!({"outcome": kind(&foreign), "marker_exists": marker.exists(), "recorded": facts}),
    );

    let token = session.begin_preparation_turn();
    let timer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        token.cancel();
    });
    let long = "Write a detailed 2000-word essay on the history of workflow engines, without calling any tool.";
    let stopped = session.turn(long);
    timer.join().expect("the timer ends");
    let (facts, count) = recorded(&home, seen);
    seen = count;
    line(
        "stop",
        &json!({"outcome": kind(&stopped), "recorded": facts}),
    );

    let _ = session.begin_preparation_turn();
    let after = session.turn("Just answer: are you still there?");
    let (facts, _) = recorded(&home, seen);
    line(
        "after_stop",
        &json!({"outcome": kind(&after), "recorded": facts}),
    );
    drop(session);
}
