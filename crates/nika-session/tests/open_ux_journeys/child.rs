// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The child: the Session a host door opens on the scenario's project (`open_with`, the
//! environment read once at open, the local engine route the parent's peer answers), driven
//! through the public doors a host uses (`turn` · `submit` against what it shows ·
//! `answer_question_for` with an identity it kept · a line queued or Stop from a host's thread
//! while the author's request is held), with a snapshot of what the Session shows after every
//! act: what waits, the work snapshot hosts render, and the identities pending. The tool steps
//! the Session reports are recorded as a host receives them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nika_session::activity::{Activity, ToolState};
use nika_session::reasoner::{NoReasoner, ProviderReasoner};
use nika_session::steer::{QueueRefused, Queued, Steering};
use nika_session::turn::{
    RoutingMethod, SessionPhase, TurnAct, TurnClassifier, TurnContext, TurnDecision,
};
use nika_session::work::Waiting;
use nika_session::{
    IntelligenceCensus, IntelligenceKind, QuestionId, ResolvedSessionIntelligence, SessionReasoner,
    SessionRuntime, TurnOutcome, UserIntelligencePreference,
};
use serde_json::{Value, json};

use super::scenarios::{Act, During, Ran, SEAT, scenario};

/// A host's router: a line at a question answers it, a line at a proposal changes it, and a
/// line while nothing waits is work. (An author that reads the conversation itself needs none.)
struct Routes;

impl TurnClassifier for Routes {
    fn classify(&mut self, context: &TurnContext, _raw: &str) -> TurnDecision {
        let act = match context.phase {
            SessionPhase::QuestionPending => TurnAct::Answer,
            SessionPhase::ProposalPending => TurnAct::Modify,
            _ => TurnAct::NewWork,
        };
        TurnDecision::new(act, RoutingMethod::Model)
    }
}

/// The Session a host door opens: the person's local engine route, its own reasoner, history
/// kept under the home when the scenario keeps it.
fn open(root: &Path, home: &Path, history: bool) -> SessionRuntime {
    let mut census = IntelligenceCensus::empty();
    census.locals.push("vllm".to_owned());
    let local = IntelligenceKind::Local {
        provider: "vllm".to_owned(),
    };
    let pref = UserIntelligencePreference::new(local, Some(SEAT.to_owned()));
    let factory = Box::new(
        |resolved: &ResolvedSessionIntelligence| -> Box<dyn SessionReasoner> {
            match (&resolved.kind, &resolved.model) {
                (IntelligenceKind::Local { provider }, Some(model)) => Box::new(ProviderReasoner {
                    model: model.clone(),
                    label: format!("{provider} · local"),
                }),
                _ => Box::new(NoReasoner),
            }
        },
    );
    let mut session = SessionRuntime::open_with(root, census, &pref, Some(home), factory);
    session.with_classifier(Box::new(Routes));
    if history {
        session.enable_history(home).expect("history opens");
        session.restore_state();
    }
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
        TurnOutcome::Facts(text) => json!({"kind": "facts", "text": text}),
        TurnOutcome::Reply(text) => json!({"kind": "reply", "text": text}),
        TurnOutcome::Aside(text) => json!({"kind": "aside", "text": text}),
        TurnOutcome::Held { id, preview } => {
            json!({"kind": "held", "id": id.to_string(), "text": preview})
        }
        TurnOutcome::RunRequested { report, run } => {
            json!({"kind": "run_requested", "text": report, "run": format!("{run:?}")})
        }
        TurnOutcome::Stopped(stopped) => json!({"kind": "stopped",
            "reach": stopped.reach.as_str(), "unsent": stopped.unsent,
            "candidate": stopped.candidate, "text": stopped.text()}),
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

/// The project's files, relative, sorted (the `.nika` store aside).
fn files(root: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                walk(base, &path, out);
            } else if let Ok(relative) = path.strip_prefix(base) {
                out.push(relative.display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// The tool steps a host received: `[call, tool, state, whether it was timed]`.
type Tools = Arc<Mutex<Vec<Value>>>;

/// Record into `tools` each tool step the Session reports, as a host receives it.
fn watch_tools(session: &mut SessionRuntime, tools: &Tools) {
    let seen = Arc::clone(tools);
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
}

/// The trace a run leaves, shaped as a real one (frames only): the digest's tasks in order,
/// each completed until the one that failed; the exit code the door saw, and the trace's path
/// as the project names it.
fn run_trace(root: &Path, ran: Ran, k: usize) -> (u8, PathBuf) {
    let frame = |kind: &str, fields: &[(&str, &str)]| {
        let fields: Vec<Value> = (fields.iter())
            .map(|(key, value)| json!({"key": key, "value": value}))
            .collect();
        json!({"kind": kind, "fields": fields}).to_string()
    };
    let tasks = ["hacker_news", "techcrunch", "summarize", "write_digest"];
    let mut lines = vec![frame("workflow_started", &[("workflow", "news-digest")])];
    let mut exit = 0;
    for task in tasks {
        lines.push(frame("task_started", &[("task", task)]));
        match ran {
            Ran::Failed(failed, detail) if failed == task => {
                lines.push(frame("task_failed", &[("task", task), ("detail", detail)]));
                exit = 1;
                break;
            }
            _ => lines.push(frame(
                "task_completed",
                &[("task", task), ("duration_ms", "2")],
            )),
        }
    }
    let end = if exit == 0 {
        frame("workflow_completed", &[("status", "succeeded")])
    } else {
        frame("workflow_failed", &[("status", "failed")])
    };
    lines.push(end);
    let at = PathBuf::from(format!(".nika/traces/run-{k}.ndjson"));
    let file = root.join(&at);
    std::fs::create_dir_all(file.parent().expect("traces")).expect("the traces directory");
    std::fs::write(&file, lines.join("\n") + "\n").expect("the trace");
    (exit, at)
}

/// A queue receipt, as a host shows it.
fn receipt(queued: Result<Queued, QueueRefused>) -> Value {
    match queued {
        Ok(queued) => json!({"queued": queued}),
        Err(refused) => json!({"refused": refused.as_str()}),
    }
}

/// `line` in a turn the host can stop; once the author's request is held under `marker`, the
/// person's act `during` from the host's thread. The cue lets the held request answer: right
/// after a queued line, after the turn returned when Stop dropped the request.
fn act_while(
    session: &mut SessionRuntime,
    markers: &Path,
    (line, marker, during): (&str, &str, During),
) -> (Value, Value) {
    let queue: Steering = session
        .steering()
        .expect("the author leads this conversation");
    let token = session.begin_preparation_turn();
    let (seen, cue) = (markers.join(marker), markers.join(format!("{marker}.go")));
    let go = cue.clone();
    let host = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !seen.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        match during {
            During::Steer(text) => {
                let queued = receipt(queue.steer(text));
                std::fs::write(&go, b"").expect("the cue is written");
                queued
            }
            During::FollowUp(text) => {
                let queued = receipt(queue.follow_up(text));
                std::fs::write(&go, b"").expect("the cue is written");
                queued
            }
            During::FollowUpThenStop(text) => {
                let queued = receipt(queue.follow_up(text));
                token.cancel();
                queued
            }
        }
    });
    let result = outcome(&session.turn(line));
    std::fs::write(&cue, b"").expect("the cue is written");
    (result, host.join().expect("the host's thread"))
}

/// Drive the scenario `name` and return its report.
pub(crate) fn drive(name: &str, root: &Path, home: &Path, markers: &Path) -> Value {
    let scenario = scenario(name);
    let mut session = open(root, home, scenario.history);
    if scenario.preparing {
        session.enable_continuous_preparation();
    }
    let tools = Tools::default();
    watch_tools(&mut session, &tools);
    let mut waited: Vec<Waiting> = Vec::new();
    let mut asked: Vec<Option<QuestionId>> = Vec::new();
    let mut steps = Vec::new();
    for act in scenario.acts {
        let (line, result) = match act {
            Act::Turn(line) => (line, outcome(&session.turn(line))),
            Act::Stoppable(line) => {
                let _ = session.begin_preparation_turn();
                (line, outcome(&session.turn(line)))
            }
            Act::RunEnds(ran) => {
                let (exit, trace) = run_trace(root, ran, steps.len());
                ("(run)", outcome(&session.observe_run(exit, Some(&trace))))
            }
            Act::While(line, marker, during) => {
                let (result, queued) = act_while(&mut session, markers, (line, marker, during));
                steps.push(
                    json!({"act": format!("{act:?}"), "line": line, "outcome": result,
                    "receipt": queued, "shown": shown(&session)}),
                );
                waited.push(session.waiting());
                asked.push(session.pending_question_id());
                continue;
            }
            Act::Submit(line) => {
                let now = session.waiting();
                (line, outcome(&session.submit(line, &now)))
            }
            Act::SubmitAsShownAfter(at, line) => {
                let then = waited[at].clone();
                (line, outcome(&session.submit(line, &then)))
            }
            Act::AnswerQuestionOf(at, line) => {
                let result = match &asked[at] {
                    Some(id) => outcome(&session.answer_question_for(id, line)),
                    None => json!({"kind": "no_question_was_waiting"}),
                };
                (line, result)
            }
            Act::AnswerElsewhere(at, line) => {
                let mut elsewhere = open(root, home, false);
                let result = match &asked[at] {
                    Some(id) => outcome(&elsewhere.answer_question_for(id, line)),
                    None => json!({"kind": "no_question_was_waiting"}),
                };
                let after = shown(&elsewhere);
                drop(elsewhere);
                steps.push(json!({"act": "elsewhere", "line": line, "outcome": result,
                    "shown": after}));
                waited.push(session.waiting());
                asked.push(session.pending_question_id());
                continue;
            }
            Act::Edit(path, text) => {
                std::fs::write(root.join(path), text).expect("the project file is edited");
                (path, json!({"kind": "edited"}))
            }
            Act::Reopen => {
                drop(session);
                session = open(root, home, scenario.history);
                watch_tools(&mut session, &tools);
                ("(reopen)", json!({"kind": "reopened"}))
            }
        };
        steps.push(
            json!({"act": format!("{act:?}"), "line": line, "outcome": result,
            "shown": shown(&session)}),
        );
        waited.push(session.waiting());
        asked.push(session.pending_question_id());
    }
    let late = (session.steering()).map(|queue| receipt(queue.steer("trop tard")));
    let tools = tools.lock().expect("the tool steps").clone();
    json!({"steps": steps, "files": files(root), "after_turn": late, "tools": tools})
}
