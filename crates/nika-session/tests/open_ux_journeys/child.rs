// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The child: the Session a host door opens on the scenario's project (`open_with`, the
//! environment read once at open, the local engine route the parent's peer answers), driven
//! through the public doors a host uses (`turn` · `submit` against what it shows ·
//! `answer_question_for` with an identity it kept), with a snapshot of what the Session shows
//! after every act: what waits, the work snapshot hosts render, and the identities pending.

use std::path::Path;

use nika_session::reasoner::{NoReasoner, ProviderReasoner};
use nika_session::turn::{
    RoutingMethod, SessionPhase, TurnAct, TurnClassifier, TurnContext, TurnDecision,
};
use nika_session::work::Waiting;
use nika_session::{
    IntelligenceCensus, IntelligenceKind, QuestionId, ResolvedSessionIntelligence, SessionReasoner,
    SessionRuntime, TurnOutcome, UserIntelligencePreference,
};
use serde_json::{Value, json};

use super::scenarios::{Act, SEAT, scenario};

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

/// Drive the scenario `name` and return its report.
pub(crate) fn drive(name: &str, root: &Path, home: &Path) -> Value {
    let scenario = scenario(name);
    let mut session = open(root, home, scenario.history);
    let mut waited: Vec<Waiting> = Vec::new();
    let mut asked: Vec<Option<QuestionId>> = Vec::new();
    let mut steps = Vec::new();
    for act in scenario.acts {
        let (line, result) = match act {
            Act::Turn(line) => (line, outcome(&session.turn(line))),
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
            Act::Reopen => {
                drop(session);
                session = open(root, home, scenario.history);
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
    json!({"steps": steps, "files": files(root)})
}
