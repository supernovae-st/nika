// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Real engine executions: the session's trace belongs to its invocation,
//! even when another trace sorts first. Run with process-per-test isolation.
#![allow(clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{RunRequest, Theme, exit, run_once, run_resume};

const ECHO: &str = "nika: session-receipt\nmodel: mock/echo\ntasks:\n  echo:\n    infer: { prompt: exact-session-result, max_tokens: 20 }\noutputs:\n  result: ${{ tasks.echo.output }}\n";
const GATE: &str = "nika: session-gate\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke: { tool: \"nika:prompt\", args: { mode: confirm, message: \"Confirm this session?\" } }\noutputs:\n  answer: ${{ tasks.ask.output }}\n";

fn request(workflow: &str) -> RunRequest {
    RunRequest {
        workflow: PathBuf::from(workflow),
        vars: Vec::new(),
        max_cost_usd: 0.1,
    }
}

fn theme() -> Theme {
    Theme::new(false, false, false)
}

fn foreign_latest(root: &Path) -> PathBuf {
    std::fs::write(root.join("foreign.nika.yaml"), ECHO).expect("fixture");
    let (code, trace) = run_once(root, &request("foreign.nika.yaml"), theme());
    assert_eq!(code, exit::OK);
    let trace = trace.expect("foreign trace");
    let future = SystemTime::now() + Duration::from_secs(3600);
    std::fs::File::open(&trace)
        .expect("foreign file")
        .set_times(std::fs::FileTimes::new().set_modified(future))
        .expect("future modification time");
    assert_eq!(nika_trace::trace::manage::latest(), Some(trace.clone()));
    trace
}

#[test]
fn run_observes_its_exact_trace_even_when_another_sorts_first() {
    let root = tempfile::tempdir().expect("project");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let foreign = foreign_latest(root.path());
    std::fs::write(
        root.path().join("own.nika.yaml"),
        ECHO.replace("exact-session-result", "only-this-session-result"),
    )
    .expect("workflow");
    let (code, trace) = run_once(root.path(), &request("own.nika.yaml"), theme());
    assert_eq!(code, exit::OK);
    let trace = trace.expect("this execution's trace, independent of latest");
    assert_ne!(trace, foreign);
    assert_eq!(nika_trace::trace::manage::latest(), Some(foreign));
    let evidence = std::fs::read_to_string(trace).expect("own trace");
    assert!(evidence.contains("only-this-session-result"));
}

#[test]
fn refused_run_never_borrows_an_existing_trace() {
    let root = tempfile::tempdir().expect("project");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let foreign = foreign_latest(root.path());
    let (code, trace) = run_once(root.path(), &request("missing.nika.yaml"), theme());
    assert_eq!(code, exit::ENV);
    assert!(trace.is_none());
    assert_eq!(nika_trace::trace::manage::latest(), Some(foreign));
}

#[test]
fn paused_and_resumed_legs_return_their_own_traces() {
    let root = tempfile::tempdir().expect("project");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let foreign = foreign_latest(root.path());
    std::fs::write(root.path().join("gate.nika.yaml"), GATE).expect("workflow");
    let (code, trace) = run_once(root.path(), &request("gate.nika.yaml"), theme());
    assert_eq!(code, exit::PAUSED);
    let paused = trace.expect("exact paused trace");
    assert_ne!(paused, foreign);
    let (code, trace) = run_resume(
        root.path(),
        Path::new("gate.nika.yaml"),
        &paused,
        "ask=true",
        theme(),
    );
    assert_eq!(code, exit::OK);
    let resumed = trace.expect("exact resumed trace");
    assert_ne!(resumed, foreign);
    assert_ne!(resumed, paused);
    let evidence = std::fs::read_to_string(resumed).expect("resumed evidence");
    assert!(evidence.contains("workflow_completed"));
    assert_eq!(nika_trace::trace::manage::latest(), Some(foreign));
}

#[test]
fn durable_conversation_runs_a_real_effect_and_reopens_without_replaying_it() {
    use nika_session::intelligence::{
        IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence,
        UserIntelligencePreference,
    };
    use nika_session::{ScriptedReasoner, SessionRuntime, TurnOutcome};

    let root = tempfile::tempdir().expect("project");
    let home = tempfile::tempdir().expect("history home");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let reply = "```yaml path=write.nika.yaml\nnika: remembered-write\npermits:\n  tools: [\"nika:write\"]\n  fs: { write: [\"./result.txt\"] }\ntasks:\n  save:\n    invoke: { tool: \"nika:write\", args: { path: \"./result.txt\", content: \"first execution\" } }\n```\n";
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".to_owned());
    let preference = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        None,
    );
    let open = || {
        SessionRuntime::open(
            root.path(),
            ResolvedSessionIntelligence::resolve(&preference, &census),
            Box::new(ScriptedReasoner::new(vec![reply.to_owned()])),
        )
    };
    let mut session = open();
    session
        .enable_history(home.path())
        .expect("private history");
    let goal = "Create a workflow that writes result.txt and run it once.";
    let proposed = session.turn(goal);
    assert!(
        matches!(proposed, TurnOutcome::Proposal { .. }),
        "{proposed:?}"
    );
    assert!(!root.path().join("result.txt").exists());
    let TurnOutcome::RunRequested { run, .. } = session.consent("yes") else {
        panic!("fresh consent must produce a checked run request");
    };
    assert!(
        !root.path().join("result.txt").exists(),
        "only the host executes"
    );
    let (code, trace) = run_once(root.path(), &run, theme());
    assert_eq!(code, exit::OK);
    assert_eq!(
        std::fs::read_to_string(root.path().join("result.txt")).expect("effect"),
        "first execution"
    );
    let trace = trace.expect("effect trace");
    let observed = session.observe_run(code, Some(&trace));
    assert!(matches!(observed, TurnOutcome::Facts(_)));
    drop(session);

    std::fs::write(root.path().join("result.txt"), "changed after execution")
        .expect("external edit");
    let mut reopened = open();
    let notice = reopened
        .enable_history(home.path())
        .expect("reopen")
        .expect("restored");
    assert!(!notice.contains("result may be unknown"), "{notice}");
    assert_eq!(reopened.intent.goal.as_deref(), Some(goal));
    assert!(matches!(reopened.consent("yes"), TurnOutcome::Refusal(_)));
    assert_eq!(
        std::fs::read_to_string(root.path().join("result.txt")).expect("preserved"),
        "changed after execution"
    );
    let histories = home.path().join(".nika/sessions");
    let folder = std::fs::read_dir(histories)
        .expect("histories")
        .next()
        .expect("one")
        .expect("entry")
        .path();
    let journal = std::fs::read_to_string(folder.join("events.ndjson")).expect("journal");
    assert!(
        journal.contains(
            trace
                .file_name()
                .expect("trace name")
                .to_str()
                .expect("UTF-8")
        )
    );
}
