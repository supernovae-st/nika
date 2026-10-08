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

/// A request for `workflow` under `root`, bound to the bytes and the world the Session's own
/// check judges as they stand now (none when absent).
fn request(root: &Path, workflow: &str) -> RunRequest {
    let audit = nika_session::change::check_on_disk(root, Path::new(workflow));
    RunRequest {
        workflow: PathBuf::from(workflow),
        vars: Vec::new(),
        max_cost_usd: 0.1,
        access_pin: None,
        bytes: audit.bytes.map(Box::new),
        closure: audit.closure.map(Box::new),
    }
}

fn theme() -> Theme {
    Theme::new(false, false, false)
}

fn foreign_latest(root: &Path) -> PathBuf {
    std::fs::write(root.join("foreign.nika"), ECHO).expect("fixture");
    let (code, trace) = run_once(root, &request(root, "foreign.nika"), theme());
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
    let (code, trace) = run_once(root.path(), &request(root.path(), "own.nika"), theme());
    assert_eq!(code, exit::OK);
    let trace = trace.expect("this execution's trace, independent of latest");
    assert_ne!(trace, foreign);
    assert_eq!(nika_trace::trace::manage::latest(), Some(foreign));
    let evidence = std::fs::read_to_string(trace).expect("own trace");
    assert!(evidence.contains("only-this-session-result"));
}

#[test]
fn a_workflow_replaced_after_its_check_runs_nothing() {
    let root = tempfile::tempdir().expect("project");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let foreign = foreign_latest(root.path());
    let own = root.path().join("own.nika");
    std::fs::write(&own, ECHO).expect("the checked bytes");
    let run = request(root.path(), "own.nika");
    // Another valid workflow lands at the same path after the check: refused before any task.
    let replaced = ECHO.replace("exact-session-result", "replaced-after-check");
    std::fs::write(&own, replaced).expect("replaced");
    let (code, trace) = run_once(root.path(), &run, theme());
    assert_eq!(code, exit::ENV);
    assert!(trace.is_none(), "nothing ran, so no trace");
    assert_eq!(nika_trace::trace::manage::latest(), Some(foreign));
    // A request that names no checked bytes runs nothing either.
    let unbound = RunRequest {
        bytes: None,
        ..run.clone()
    };
    assert_eq!(run_once(root.path(), &unbound, theme()).0, exit::ENV);
    // The checked bytes back in place: they run.
    std::fs::write(&own, ECHO).expect("the checked bytes again");
    let (code, trace) = run_once(root.path(), &run, theme());
    assert_eq!(code, exit::OK);
    let evidence = std::fs::read_to_string(trace.expect("its own trace")).expect("trace");
    assert!(evidence.contains("exact-session-result"));
}

#[test]
fn refused_run_never_borrows_an_existing_trace() {
    let root = tempfile::tempdir().expect("project");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let foreign = foreign_latest(root.path());
    let (code, trace) = run_once(root.path(), &request(root.path(), "missing.nika"), theme());
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
    let (code, trace) = run_once(root.path(), &request(root.path(), "gate.nika"), theme());
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

#[test]
fn the_plain_session_runner_refuses_an_explicit_bad_pin_without_a_fallback() {
    let root = tempfile::tempdir().expect("project");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let foreign = foreign_latest(root.path());
    std::fs::write(root.path().join("pinned.nika"), ECHO).expect("workflow");
    let mut run = request(root.path(), "pinned.nika");
    run.access_pin = Some("not-a-real-access-pin".into());
    let (code, trace) = run_once(root.path(), &run, theme());
    assert_ne!(
        code,
        exit::OK,
        "a pin never disappears into the available mock backend"
    );
    assert_eq!(nika_trace::trace::manage::latest(), Some(foreign.clone()));
    // Admission emits no runtime event; local signing custody may still seal an empty journal.
    let Some(trace) = trace else {
        return;
    };
    assert_ne!(trace, foreign, "never borrow an earlier execution's trace");
    let raw = std::fs::read_to_string(&trace).expect("refusal trace");
    let recovered = nika_dap::recover::recover_events(&raw, "access refusal")
        .expect("read the actual refusal events");
    assert!(recovered.truncated_note.is_none());
    assert_eq!(recovered.events.len(), 1, "no task may start: {raw}");
    assert_eq!(recovered.events[0].kind, nika_event::EventKind::RunSealed);
}

/// A parent that calls `./child.nika`, granting the one write its child makes.
const PARENT: &str = "nika: drift-parent\npermits:\n  fs: { write: [\"./out/child.txt\"] }\n  tools: [\"nika:write\"]\ntasks:\n  call:\n    invoke: { workflow: \"./child.nika\" }\n";
/// The child: one write, its only effect.
const CHILD: &str = "nika: drift-child\npermits:\n  fs: { write: [\"./out/child.txt\"] }\n  tools: [\"nika:write\"]\ntasks:\n  write:\n    invoke: { tool: \"nika:write\", args: { path: \"./out/child.txt\", content: \"checked child\" } }\n";
/// A workflow whose agent reads one skill.
const SKILLED: &str = "nika: drift-skill\nmodel: mock/echo\npermits:\n  fs:\n    read: [\"skills/review/SKILL.md\"]\ntasks:\n  review:\n    agent: { prompt: checked-skill-ran, skills: [\"skills/review/SKILL.md\"] }\noutputs:\n  said: ${{ tasks.review.output }}\n";
const SKILL: &str = "---\nname: review\ndescription: Review a draft.\n---\nBe careful.\n";

/// The traces a run left under `root`.
fn traces(root: &Path) -> usize {
    std::fs::read_dir(root.join(".nika/traces")).map_or(0, Iterator::count)
}

/// [`request`] for `workflow`, once the Session's check judged its whole world.
fn checked_world(root: &Path, workflow: &str) -> RunRequest {
    let run = request(root, workflow);
    assert!(
        run.closure.is_some(),
        "the check judged the world of {workflow}"
    );
    run
}

/// The plain door runs only the world the Session checked: a child rewritten after the check
/// while its parent is unchanged runs nothing and writes nothing; the checked child back in place
/// runs exactly once.
#[test]
fn a_child_changed_after_the_check_runs_nothing() {
    let root = tempfile::tempdir().expect("project");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let child = root.path().join("child.nika");
    std::fs::write(root.path().join("parent.nika"), PARENT).expect("parent");
    std::fs::write(&child, CHILD).expect("child");
    let run = checked_world(root.path(), "parent.nika");
    std::fs::write(&child, CHILD.replace("checked child", "changed child")).expect("rewritten");
    let output = root.path().join("out/child.txt");
    let (code, trace) = run_once(root.path(), &run, theme());
    assert_eq!(code, exit::ENV, "another world is refused before any task");
    assert!(trace.is_none(), "nothing ran, so no trace");
    assert!(!output.exists(), "nothing was written");
    assert_eq!(traces(root.path()), 0);
    std::fs::write(&child, CHILD).expect("the checked child again");
    // A request that recorded no world runs nothing, whatever stands on disk.
    let unbound = RunRequest {
        closure: None,
        ..run.clone()
    };
    assert_eq!(run_once(root.path(), &unbound, theme()).0, exit::ENV);
    assert!(!output.exists(), "nothing was written");
    let (code, trace) = run_once(root.path(), &run, theme());
    assert_eq!(code, exit::OK);
    assert!(trace.is_some(), "its own trace");
    assert_eq!(
        std::fs::read_to_string(&output).expect("its one write"),
        "checked child"
    );
    assert_eq!(
        traces(root.path()),
        2,
        "executed exactly once: the parent's trace and its child's"
    );
}

/// The same for a skill the workflow's agent reads: rewritten after the check while the workflow
/// is unchanged, nothing runs; the checked skill back in place runs exactly once.
#[test]
fn a_skill_changed_after_the_check_runs_nothing() {
    let root = tempfile::tempdir().expect("project");
    let _cwd = crate::cwd::enter(root.path()).expect("isolated cwd");
    let skill = root.path().join("skills/review/SKILL.md");
    std::fs::create_dir_all(root.path().join("skills/review")).expect("skills");
    std::fs::write(&skill, SKILL).expect("skill");
    std::fs::write(root.path().join("skilled.nika"), SKILLED).expect("workflow");
    let run = checked_world(root.path(), "skilled.nika");
    std::fs::write(&skill, SKILL.replace("careful", "reckless")).expect("rewritten");
    let (code, trace) = run_once(root.path(), &run, theme());
    assert_eq!(code, exit::ENV, "another world is refused before any task");
    assert!(trace.is_none(), "nothing ran, so no trace");
    assert_eq!(traces(root.path()), 0);
    std::fs::write(&skill, SKILL).expect("the checked skill again");
    let (code, trace) = run_once(root.path(), &run, theme());
    assert_eq!(code, exit::OK);
    let evidence = std::fs::read_to_string(trace.expect("its own trace")).expect("trace");
    assert!(evidence.contains("checked-skill-ran"), "{evidence}");
    assert_eq!(traces(root.path()), 1, "executed exactly once");
}
