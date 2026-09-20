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
/// A Ready intent the compiler settles with no question and no model; its
/// run needs `notes/brief.md` and writes `out/copy.md`.
const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
const BRIEF: &str = "# Brief\n\nThe launch moves to October.\n";

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
    std::fs::write(root.join("foreign.nika"), ECHO).expect("fixture");
    let (code, trace) = run_once(root, &request("foreign.nika"), theme());
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
        root.path().join("own.nika"),
        ECHO.replace("exact-session-result", "only-this-session-result"),
    )
    .expect("workflow");
    let (code, trace) = run_once(root.path(), &request("own.nika"), theme());
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
    let (code, trace) = run_once(root.path(), &request("missing.nika"), theme());
    assert_eq!(code, exit::ENV);
    assert!(trace.is_none());
    assert_eq!(nika_trace::trace::manage::latest(), Some(foreign));
}

#[test]
fn paused_and_resumed_legs_return_their_own_traces() {
    let root = tempfile::tempdir().expect("project");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let foreign = foreign_latest(root.path());
    std::fs::write(root.path().join("gate.nika"), GATE).expect("workflow");
    let (code, trace) = run_once(root.path(), &request("gate.nika"), theme());
    assert_eq!(code, exit::PAUSED);
    let paused = trace.expect("exact paused trace");
    assert_ne!(paused, foreign);
    let (code, trace) = run_resume(
        root.path(),
        Path::new("gate.nika"),
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

/// The durable conversation over the ONE compiler: the intent is compiled
/// and proposed, the consent lands the bytes (never a run), an explicit
/// run line requests the checked run, the door executes it for real, the
/// session observes the trace — and a reopened session restores the goal
/// without replaying any effect.
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
    std::fs::create_dir_all(root.path().join("notes")).expect("notes");
    std::fs::write(root.path().join("notes/brief.md"), BRIEF).expect("brief");
    let workflow = root.path().join("compiled-workflow.nika");
    let copy = root.path().join("out/copy.md");
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".to_owned());
    let preference = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        None,
    );
    // The reasoner is never asked: the intent reaches the compiler.
    let open = || {
        SessionRuntime::open(
            root.path(),
            ResolvedSessionIntelligence::resolve(&preference, &census),
            Box::new(ScriptedReasoner::new(Vec::new())),
        )
    };
    let mut session = open();
    session
        .enable_history(home.path())
        .expect("private history");
    let proposed = session.turn(COPY);
    assert!(
        matches!(proposed, TurnOutcome::Proposal { .. }),
        "{proposed:?}"
    );
    assert!(!workflow.exists(), "nothing is written before consent");
    let TurnOutcome::Facts(report) = session.consent("yes") else {
        panic!("consent lands the bytes and is never a run");
    };
    assert!(report.contains("clean ✔"), "{report}");
    assert!(workflow.is_file(), "the consent landed the workflow");
    assert!(!copy.exists(), "consent is never a run");
    let TurnOutcome::RunRequested { run, .. } = session.turn("run it") else {
        panic!("an explicit run line requests the checked run");
    };
    assert_eq!(run.workflow, PathBuf::from("compiled-workflow.nika"));
    assert!(!copy.exists(), "only the host executes");
    let (code, trace) = run_once(root.path(), &run, theme());
    assert_eq!(code, exit::OK);
    assert_eq!(
        std::fs::read_to_string(&copy).expect("effect"),
        BRIEF,
        "the run copied the brief"
    );
    let trace = trace.expect("effect trace");
    let observed = session.observe_run(code, Some(&trace));
    assert!(matches!(observed, TurnOutcome::Facts(_)));
    drop(session);

    std::fs::write(&copy, "changed after execution").expect("external edit");
    let mut reopened = open();
    let notice = reopened
        .enable_history(home.path())
        .expect("reopen")
        .expect("restored");
    assert!(!notice.contains("result may be unknown"), "{notice}");
    assert_eq!(reopened.intent.goal.as_deref(), Some(COPY));
    assert!(matches!(reopened.consent("yes"), TurnOutcome::Refusal(_)));
    assert_eq!(
        std::fs::read_to_string(&copy).expect("preserved"),
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
