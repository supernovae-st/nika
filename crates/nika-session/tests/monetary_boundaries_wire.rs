// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Loopback protocol mechanics, not live qualification. A clean child owns
//! its environment; the real provider factory routes confirm-gate language.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]

mod common;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::LoopbackSeat;
use nika_session::reasoner::{NoReasoner, ProviderReasoner};
use nika_session::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, SessionReasoner,
    SessionRuntime, TurnOutcome, UserIntelligencePreference,
};

fn provider(root: &Path) -> SessionRuntime {
    let mut census = IntelligenceCensus::empty();
    census.locals.push("vllm".to_owned());
    let preference = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "vllm".to_owned(),
        },
        Some("vllm/monetary-boundaries".to_owned()),
    );
    SessionRuntime::open_with(
        root,
        census,
        &preference,
        None,
        Box::new(
            |selected: &ResolvedSessionIntelligence| -> Box<dyn SessionReasoner> {
                Box::new(ProviderReasoner {
                    model: selected.model.clone().expect("selected model"),
                    label: "monetary boundary loopback".to_owned(),
                })
            },
        ),
    )
}

fn gate_fixture(root: &Path) {
    let intelligence = ResolvedSessionIntelligence::resolve(
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        &IntelligenceCensus::empty(),
    );
    let mut session = SessionRuntime::open(root, intelligence, Box::new(NoReasoner));
    assert!(matches!(
        session.turn("Read ./entree.txt and write it to ./sortie.txt"),
        TurnOutcome::Proposal { .. }
    ));
    assert!(matches!(session.consent("yes"), TurnOutcome::Facts(_)));
    assert!(matches!(
        session.turn("run it"),
        TurnOutcome::RunRequested { .. }
    ));
    let trace = root.join("paused.ndjson");
    std::fs::write(&trace, "{\"kind\":\"workflow_paused\",\"fields\":[{\"key\":\"task\",\"value\":\"approve\"},{\"key\":\"mode\",\"value\":\"confirm\"},{\"key\":\"message\",\"value\":\"Proceed?\"}]}\n").expect("synthetic pause");
    assert!(matches!(
        session.observe_run(4, Some(&trace)),
        TurnOutcome::GateAsk { .. }
    ));
}

fn scenario(name: &str, expected_calls: usize) {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().join("project");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&root).expect("project");
    std::fs::create_dir_all(&home).expect("home");
    let log = home.join("child.log");
    let seat = LoopbackSeat::start(vec!["QUESTION".to_owned()]);
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .env("MONETARY_ROOT", &root)
        .env("MONETARY_SCENARIO", name)
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(&log).expect("log")))
        .stderr(Stdio::from(
            std::fs::OpenOptions::new()
                .append(true)
                .open(&log)
                .expect("log"),
        ))
        .spawn()
        .expect("child");
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("bounded child timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    seat.shutdown();
    assert!(
        status.success(),
        "{}",
        std::fs::read_to_string(log).expect("child evidence")
    );
    assert_eq!(
        seat.bodies().len(),
        expected_calls,
        "actual HTTP attempts: {}",
        seat.bodies().len()
    );
    assert!(!root.join("sortie.txt").exists(), "no workflow executed");
}

#[test]
fn compact_money_and_both_confirm_doors_make_no_http_attempts() {
    scenario("money", 0);
}

#[test]
fn unbounded_confirm_question_reaches_the_actual_provider_factory() {
    scenario("control", 1);
}

#[test]
#[ignore = "bounded child invoked by loopback parents"]
fn child() {
    let root = std::env::var("MONETARY_ROOT").expect("root");
    let root = Path::new(&root);
    gate_fixture(root);
    if std::env::var("MONETARY_SCENARIO").expect("scenario") == "control" {
        let mut session = provider(root);
        assert!(session.restore_state().is_some());
        let gate = session.waiting_gate().expect("restored gate");
        assert!(matches!(
            session.answer_gate("what does this permit?"),
            TurnOutcome::Aside(_)
        ));
        assert_eq!(session.waiting_gate(), Some(gate));
        return;
    }
    for (clause, amount) in [
        ("budget=0", Some(0.0)),
        ("budget:0", Some(0.0)),
        ("0USD", Some(0.0)),
        ("budget=NaN", None),
        ("budget=0.5oopsUSD", None),
        ("budget:inf", None),
        ("-1USD", None),
        ("budget=2", Some(2.0)),
        ("budget:0,50", Some(0.5)),
        ("2USD", Some(2.0)),
    ] {
        for verb in [
            "What can you tell me about stars,",
            "Prépare la copie de entree.txt dans sortie.txt,",
        ] {
            let mut session = provider(root);
            let input = format!("{verb} {clause}?");
            assert!(matches!(session.turn(&input), TurnOutcome::Refusal(_)));
            let money = session.monetary_decision().expect("money");
            assert_eq!(money.input, input);
            assert_eq!(money.effective_usd, amount);
            assert!(session.pending_proposal().is_none());
            assert!(session.pending_question().is_none());
        }
        for (addressed, start_with_zero) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let mut session = provider(root);
            assert!(session.restore_state().is_some());
            let gate = session.waiting_gate().expect("restored gate");
            if start_with_zero {
                assert!(matches!(
                    session.answer_gate("budget 0 USD"),
                    TurnOutcome::Refusal(_)
                ));
            }
            let input = format!("yes but {clause}");
            let out = if addressed {
                session.answer_gate_for(&gate, &input)
            } else {
                session.answer_gate(&input)
            };
            assert!(matches!(out, TurnOutcome::Refusal(_)), "{out:?}");
            assert_eq!(session.waiting_gate().as_ref(), Some(&gate));
            let money = session.monetary_decision().expect("money").clone();
            assert_eq!(money.input, input);
            assert_eq!(money.effective_usd, amount);
            let followup = if addressed {
                session.answer_gate_for(&gate, "what does this permit?")
            } else {
                session.answer_gate("what does this permit?")
            };
            assert!(matches!(followup, TurnOutcome::Refusal(_)), "{followup:?}");
            assert_eq!(session.waiting_gate().as_ref(), Some(&gate));
            assert_eq!(session.monetary_decision(), Some(&money));
        }
    }
}
