// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Explicit Run access survives questions, but never becomes restored authority.
#![allow(clippy::expect_used, clippy::panic)]

use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use nika_session::reasoner::NoReasoner;
use nika_session::{SessionRuntime, TurnOutcome};
use std::path::Path;

const WORKFLOW: &str = "nika: pin-test\ninputs:\n  name: { type: string, required: true }\ntasks:\n  say:\n    invoke: { tool: \"nika:log\", args: { message: \"${{ inputs.name }}\" } }\n";

fn open(root: &Path) -> SessionRuntime {
    let none = ResolvedSessionIntelligence::resolve(
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        &IntelligenceCensus::empty(),
    );
    SessionRuntime::open(root, none, Box::new(NoReasoner))
}

#[test]
fn explicit_access_survives_input_questions_and_is_announced_without_changing_intelligence() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("one.nika"), WORKFLOW).expect("workflow");
    for flag in ["--access mock", "--access=mock"] {
        let mut session = open(root.path());
        let out = session.turn(&format!("run one.nika {flag} --max-cost-usd 0.1"));
        assert!(
            matches!(out, TurnOutcome::Question { ref key, .. } if key == "input.name"),
            "{out:?}"
        );
        let out = session.turn("Nika");
        let TurnOutcome::RunRequested { run, report } = out else {
            panic!("{out:?}")
        };
        assert_eq!(run.access_pin.as_deref(), Some("mock"));
        assert_eq!(run.vars, ["name=Nika"]);
        assert!(report.contains("access mock (explicit)"), "{report}");
        assert!(matches!(session.intelligence.kind, IntelligenceKind::None));
        let out = session.turn("run one.nika name=Other --max-cost-usd 0.1");
        assert!(
            matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.access_pin.is_none()),
            "{out:?}"
        );
    }
}

#[test]
fn invalid_options_and_unknown_or_incompatible_pins_never_request_a_run() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(root.path().join("one.nika"), WORKFLOW).expect("workflow");
    std::fs::write(root.path().join("other.nika"), WORKFLOW).expect("not an access");
    for flags in [
        "--access",
        "--access=",
        "--access mock --access mock",
        "--access=mock --access=api",
        "--unknown=codex",
        "--access other.nika",
        "--access=not-a-real-access-pin",
        "--max-cost-usd=0.1 --max-cost-usd=0.1",
    ] {
        let mut session = open(root.path());
        let out = session.turn(&format!("run one.nika name=Nika {flags}"));
        assert!(
            matches!(out, TurnOutcome::Refusal(_) | TurnOutcome::Facts(_)),
            "{flags}: {out:?}"
        );
        assert!(
            session.pending_input().is_none(),
            "invalid options must fail before asking input"
        );
    }
    std::fs::write(
        root.path().join("mock.nika"),
        "nika: mocked\nmodel: mock/echo\ntasks:\n  a: { infer: { prompt: test } }\n",
    )
    .expect("mock workflow");
    let out = open(root.path()).turn("run mock.nika --access codex --max-cost-usd 0.1");
    assert!(
        matches!(out, TurnOutcome::Facts(ref text) if text.contains("access") && text.contains("refused")),
        "{out:?}"
    );
}

#[test]
fn a_pending_access_choice_expires_when_the_session_reopens() {
    let root = tempfile::tempdir().expect("root");
    let home = tempfile::tempdir().expect("private home");
    std::fs::write(root.path().join("one.nika"), WORKFLOW).expect("workflow");
    {
        let mut session = open(root.path());
        session.enable_history(home.path()).expect("history");
        let out = session.turn("run one.nika --access mock --max-cost-usd 0.1");
        assert!(matches!(out, TurnOutcome::Question { .. }), "{out:?}");
    }
    let mut session = open(root.path());
    session.enable_history(home.path()).expect("reopen");
    assert!(session.pending_input().is_none());
    let out = session.turn("run one.nika name=Nika --max-cost-usd 0.1");
    assert!(
        matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.access_pin.is_none()),
        "{out:?}"
    );
}
