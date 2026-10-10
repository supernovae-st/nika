// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The child: the Session a host door opens on the scenario's project with the person's choice
//! — Claude Code over ACP, its own model — and the real reasoner factory's seat reasoner,
//! history kept under the home, driven through the public doors a host uses (`turn`, `submit`
//! against what it shows, `consent`, and Stop through the preparation turn's token).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use nika_session::intelligence::{IntelligenceCensus, IntelligenceKind, SeatSeen};
use nika_session::reasoner::HarnessReasoner;
use nika_session::{
    ResolvedSessionIntelligence, SessionReasoner, SessionRuntime, TurnOutcome,
    UserIntelligencePreference,
};
use nika_types::access::HarnessTransport;
use serde_json::{Value, json};

use super::{ACCEPT, REQUEST};

/// The Session a host door opens: the seat the person chose, over ACP, the history kept.
fn open(root: &Path, home: &Path) -> SessionRuntime {
    let mut census = IntelligenceCensus::empty();
    census
        .seats
        .push(SeatSeen::new("claude-code".to_owned(), true, true, true));
    let seat = IntelligenceKind::Harness {
        seat: "claude-code".to_owned(),
        transport: HarnessTransport::Acp,
    };
    let pref = UserIntelligencePreference::new(seat, None);
    let factory = Box::new(
        |resolved: &ResolvedSessionIntelligence| -> Box<dyn SessionReasoner> {
            match &resolved.kind {
                IntelligenceKind::Harness { seat, transport } => Box::new(
                    HarnessReasoner { seat: seat.clone() }
                        .with_transport(resolved.model.clone(), *transport),
                ),
                other => panic!("the person chose a seat, not {other:?}"),
            }
        },
    );
    let mut session = SessionRuntime::open_with(root, census, &pref, Some(home), factory);
    session.enable_history(home).expect("history opens");
    session.restore_state();
    session
}

/// What one outcome is, for the report.
fn outcome(outcome: &TurnOutcome) -> Value {
    match outcome {
        TurnOutcome::Question { key, question } => {
            json!({"kind": "question", "key": key, "text": question})
        }
        TurnOutcome::Proposal { id, preview } => {
            json!({"kind": "proposal", "id": id.to_string(), "text": preview})
        }
        TurnOutcome::Refusal(refusal) => {
            json!({"kind": "refusal", "class": refusal.class.as_str(), "text": refusal.text})
        }
        TurnOutcome::Reply(text) => json!({"kind": "reply", "text": text}),
        TurnOutcome::Cancelled(text) => json!({"kind": "cancelled", "text": text}),
        TurnOutcome::Facts(text) => json!({"kind": "facts", "text": text}),
        other => json!({"kind": "other", "text": format!("{other:?}")}),
    }
}

/// What the Session shows now.
fn shown(session: &SessionRuntime) -> Value {
    json!({
        "waiting": serde_json::to_value(session.waiting()).expect("waiting serializes"),
        "work": serde_json::to_value(session.work()).expect("work serializes"),
        "question_id": session.pending_question_id().map(|id| id.to_string()),
        "proposal": session.pending_proposal().map(|id| id.to_string()),
    })
}

fn step(session: &SessionRuntime, line: &str, result: &TurnOutcome) -> Value {
    json!({"line": line, "outcome": outcome(result), "shown": shown(session)})
}

/// Stop the turn under way once the agent waits for its cancel (bounded).
fn stop_when_waiting(observed: PathBuf, token: nika_types::cancel::CancelCtx) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !observed.join("waiting").exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        token.cancel();
    });
}

/// Whether an agent process the Session spawned still runs (a zombie its parent has not
/// reaped yet runs nothing), waiting up to ten seconds for it to end.
fn agents_alive(observed: &Path) -> bool {
    let noted = std::fs::read_to_string(observed.join("spawned")).unwrap_or_default();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let alive = noted.lines().any(|pid| {
            let shown = Command::new("/bin/ps")
                .args(["-o", "stat=", "-p", pid.trim()])
                .output();
            shown.is_ok_and(|out| {
                let stat = String::from_utf8_lossy(&out.stdout);
                !stat.trim().is_empty() && !stat.trim().starts_with('Z')
            })
        });
        if !alive || Instant::now() > deadline {
            return alive;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Drive the scenario `name` and return its report.
pub(crate) fn drive(name: &str, root: &Path, home: &Path, observed: &Path) -> Value {
    let mut session = open(root, home);
    let mut steps = Vec::new();
    match name {
        "lead" => {
            let first = session.turn(REQUEST);
            steps.push(step(&session, REQUEST, &first));
            let now = session.waiting();
            let second = session.submit(ACCEPT, &now);
            steps.push(step(&session, ACCEPT, &second));
            let third = session.consent("yes");
            steps.push(step(&session, "yes", &third));
        }
        "stop" => {
            let token = session.begin_preparation_turn();
            stop_when_waiting(observed.to_path_buf(), token);
            let first = session.turn("write me a long essay");
            steps.push(step(&session, "write me a long essay", &first));
            let _ = session.begin_preparation_turn();
            let second = session.turn("are you still there?");
            steps.push(step(&session, "are you still there?", &second));
        }
        "unavailable" => {
            let first = session.turn(REQUEST);
            steps.push(step(&session, REQUEST, &first));
        }
        other => panic!("unknown scenario {other}"),
    }
    drop(session);
    json!({"steps": steps, "agent_alive_after_close": agents_alive(observed)})
}
