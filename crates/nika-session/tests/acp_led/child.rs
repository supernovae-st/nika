// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The child: the Session a host door opens on the scenario's project with the person's choice
//! — Claude Code over ACP, its own model — and the real reasoner factory's seat reasoner,
//! history kept under the home, driven through the public doors a host uses (`turn`, `submit`
//! against what it shows, `consent`, Stop through the preparation turn's token, and the
//! conversation's queue for a line typed while the agent works). The tool steps the Session
//! reports are recorded as a host receives them.

use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nika_session::activity::{Activity, ToolState};
use nika_session::intelligence::{IntelligenceCensus, IntelligenceKind, SeatSeen};
use nika_session::reasoner::HarnessReasoner;
use nika_session::steer::{QueueRefused, Queued, Steering};
use nika_session::{
    ResolvedSessionIntelligence, SessionReasoner, SessionRuntime, TurnOutcome,
    UserIntelligencePreference,
};
use nika_types::access::HarnessTransport;
use serde_json::{Value, json};

use super::{ACCEPT, CHOOSE, REQUEST, deepseek_probe};

/// The tool steps a host received: `[call, tool, state, whether it was timed]`.
type Tools = Arc<Mutex<Vec<Value>>>;

/// The Session a host door opens: the seat the person chose, over ACP, the history kept; when
/// `metered`, this machine's census also holds one metered route, whose models have typed facts.
fn open(root: &Path, home: &Path, metered: bool) -> (SessionRuntime, Tools) {
    let mut census = IntelligenceCensus::empty();
    census
        .seats
        .push(SeatSeen::new("claude-code".to_owned(), true, true, true));
    if metered {
        census.provider_context.push(deepseek_probe());
    }
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
    let tools = Tools::default();
    let seen = Arc::clone(&tools);
    session.on_activity(Arc::new(move |activity: &Activity| {
        if let Some(tool) = &activity.tool {
            let state = match tool.state {
                ToolState::Started => "started",
                ToolState::Finished => "finished",
                ToolState::Failed => "failed",
                _ => "other",
            };
            let step = json!([tool.call, tool.name, state, tool.elapsed_ms.is_some()]);
            seen.lock().expect("the tool steps").push(step);
        }
    }));
    (session, tools)
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
        TurnOutcome::Stopped(stopped) => json!({"kind": "stopped",
            "reach": stopped.reach.as_str(), "unsent": stopped.unsent,
            "candidate": stopped.candidate, "text": stopped.text()}),
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

/// Wait (bounded) until the agent left `marker` under `observed`.
fn marked(observed: &Path, marker: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !observed.join(marker).exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// On another thread, once the agent left `marker`: `act`, then the marker `after` if one is
/// named (the agent's cue to go on). The thread hands back what `act` answered.
fn when_marked(
    observed: &Path,
    marker: &'static str,
    act: impl FnOnce() -> Value + Send + 'static,
    after: Option<&'static str>,
) -> std::thread::JoinHandle<Value> {
    let observed = observed.to_path_buf();
    std::thread::spawn(move || {
        marked(&observed, marker);
        let answered = act();
        if let Some(after) = after {
            std::fs::write(observed.join(after), b"").expect("the cue is written");
        }
        answered
    })
}

/// A queue receipt, as a host shows it.
fn receipt(queued: Result<Queued, QueueRefused>) -> Value {
    match queued {
        Ok(queued) => json!({"queued": queued}),
        Err(refused) => json!({"refused": refused.as_str()}),
    }
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

/// The Session's queue for the conversation, taken before the turn moves on.
fn steering(session: &SessionRuntime) -> Steering {
    session
        .steering()
        .expect("the agent leads this conversation")
}

/// Drive the scenario `name` and return its report.
pub(crate) fn drive(name: &str, root: &Path, home: &Path, observed: &Path) -> Value {
    let (mut session, tools) = open(root, home, name == "choices");
    let mut steps = Vec::new();
    let mut receipts = Vec::new();
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
            let queue = steering(&session);
            let token = session.begin_preparation_turn();
            let stopper = when_marked(
                observed,
                "waiting",
                move || {
                    let queued = receipt(queue.follow_up("then c"));
                    token.cancel();
                    queued
                },
                None,
            );
            let first = session.turn("write me a long essay");
            steps.push(step(&session, "write me a long essay", &first));
            receipts.push(stopper.join().expect("the stop's thread"));
            let _ = session.begin_preparation_turn();
            let second = session.turn("are you still there?");
            steps.push(step(&session, "are you still there?", &second));
        }
        "steer" => {
            let queue = steering(&session);
            let steerer = when_marked(
                observed,
                "waiting",
                move || receipt(queue.steer("use b instead")),
                None,
            );
            let first = session.turn("use a");
            steps.push(step(&session, "use a", &first));
            receipts.push(steerer.join().expect("the steering thread"));
        }
        "follow" => {
            let queue = steering(&session);
            let follower = when_marked(
                observed,
                "turn-open",
                move || receipt(queue.follow_up("and c")),
                Some("queued"),
            );
            let first = session.turn("do a");
            steps.push(step(&session, "do a", &first));
            receipts.push(follower.join().expect("the follow-up thread"));
        }
        "choices" => {
            let first = session.turn(CHOOSE);
            steps.push(step(&session, CHOOSE, &first));
        }
        "unavailable" => {
            let first = session.turn(REQUEST);
            steps.push(step(&session, REQUEST, &first));
        }
        other => panic!("unknown scenario {other}"),
    }
    let late = (session.steering()).map(|queue| receipt(queue.steer("too late")));
    drop(session);
    let tools = tools.lock().expect("the tool steps").clone();
    json!({"steps": steps, "receipts": receipts, "after_turn": late, "tools": tools,
        "agent_alive_after_close": agents_alive(observed)})
}
