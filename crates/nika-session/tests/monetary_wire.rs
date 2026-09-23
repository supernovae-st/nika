// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Protocol mechanics ONLY: the real Session/compiler/provider adapter over
//! an observed loopback double. These canned replies do not qualify intent
//! understanding, live billing, a subscription, or the original population.
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

use common::{LoopbackSeat, message, native_answer};
use nika_session::reasoner::ProviderReasoner;
use nika_session::turn::{
    RoutingMethod, SessionPhase, TurnAct, TurnClassifier, TurnContext, TurnDecision,
};
use nika_session::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, SessionReasoner,
    SessionRuntime, TurnOutcome, UserIntelligencePreference,
};

const INPUT: &str = "Prépare la copie de entree.txt dans sortie.txt.";

struct Routing;
impl TurnClassifier for Routing {
    fn classify(&mut self, context: &TurnContext, _: &str) -> TurnDecision {
        TurnDecision::new(
            if context.phase == SessionPhase::ProposalPending {
                TurnAct::Modify
            } else {
                TurnAct::NewWork
            },
            RoutingMethod::Model,
        )
    }
}

fn open(root: &Path) -> SessionRuntime {
    let mut census = IntelligenceCensus::empty();
    census.locals.push("vllm".to_owned());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "vllm".to_owned(),
        },
        Some("vllm/s17-protocol".to_owned()),
    );
    let mut runtime = SessionRuntime::open_with(
        root,
        census,
        &pref,
        None,
        Box::new(
            |selected: &ResolvedSessionIntelligence| -> Box<dyn SessionReasoner> {
                Box::new(ProviderReasoner {
                    model: selected.model.clone().expect("selected model"),
                    label: "S17 loopback protocol double".to_owned(),
                })
            },
        ),
    );
    runtime.with_classifier(Box::new(Routing));
    runtime
}

fn child_run(root: &Path, home: &Path, seat: &LoopbackSeat, scenario: &str) {
    let log = home.join("child.log");
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
        .env("HOME", home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .env("S17_ROOT", root)
        .env("S17_SCENARIO", scenario)
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
    assert!(
        status.success(),
        "{}",
        std::fs::read_to_string(log).expect("child evidence")
    );
}

#[test]
fn invalid_zero_and_positive_bounded_prepare_never_reach_the_wire() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().join("project");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&root).expect("root");
    std::fs::create_dir_all(&home).expect("home");
    let seat = LoopbackSeat::start(vec!["unexpected call".to_owned()]);
    child_run(&root, &home, &seat, "invalid");
    seat.shutdown();
    assert!(
        seat.bodies().is_empty(),
        "no classifier, reasoner or authoring HTTP call"
    );
    assert_eq!(
        std::fs::read_dir(root).expect("root").count(),
        0,
        "no proposal/Run/file effect"
    );
}

#[test]
fn monetary_amendment_after_unbounded_authoring_crosses_save_and_run_protocol() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().join("project");
    let home = dir.path().join("home");
    std::fs::create_dir_all(&root).expect("root");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::write(root.join("entree.txt"), "A\n").expect("input");
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/compile/copy-fr.json"))
            .expect("candidate fixture");
    let candidate = fixture["candidate"]
        .as_str()
        .expect("candidate")
        .replace("./notes/brief.md", "./entree.txt")
        .replace("./out/copie.md", "./sortie.txt");
    let seat = LoopbackSeat::start(vec![native_answer(&candidate)]);
    child_run(&root, &home, &seat, "binding");
    seat.shutdown();
    let bodies = seat.bodies();
    assert!(!bodies.is_empty());
    assert!(
        bodies
            .iter()
            .any(|body| message(body, "user").contains(INPUT)),
        "exact original Prepare reaches authoring"
    );
    assert!(!root.join("sortie.txt").exists(), "Save is never execution");
    assert_eq!(
        std::fs::read_to_string(root.join("entree.txt")).expect("input"),
        "A\n"
    );
}

#[test]
#[ignore = "bounded child, invoked by the loopback mechanics parents"]
fn child() {
    let root = std::env::var("S17_ROOT").expect("root");
    let mut runtime = open(Path::new(&root));
    if std::env::var("S17_SCENARIO").expect("scenario") == "invalid" {
        for value in ["NaN", "inf", "-1", "0", "0,50", "2"] {
            let input =
                format!("Prépare la copie de entree.txt dans sortie.txt, budget {value} dollars.");
            let out = runtime.turn(&input);
            assert!(matches!(out, TurnOutcome::Refusal(_)), "{out:?}");
            assert!(runtime.pending_proposal().is_none());
            assert!(runtime.pending_question().is_none());
        }
        return;
    }
    let out = runtime.turn(INPUT);
    let TurnOutcome::Proposal { id: old, .. } = out else {
        panic!("protocol candidate: {out:?}")
    };
    let out = runtime.consent("budget 0,50 dollar");
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("monetary revision: {out:?}")
    };
    assert_ne!(id, old);
    let money = runtime.monetary_decision().expect("money");
    assert_eq!(money.effective_usd, Some(0.5));
    assert_eq!(money.original_intent, INPUT);
    assert_eq!(money.input, "budget 0,50 dollar");
    assert_eq!(money.proposal.as_ref(), Some(&id));
    assert!(matches!(
        runtime.consent_to(&id, "yes"),
        TurnOutcome::Facts(_)
    ));
    let out = runtime.turn("run it");
    assert!(
        matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 0.5_f64.to_bits()),
        "{out:?}"
    );
    let out = runtime.turn("run it budget 2 USD");
    assert!(
        matches!(out, TurnOutcome::RunRequested { ref run, .. } if run.max_cost_usd.to_bits() == 2.0_f64.to_bits()),
        "{out:?}"
    );
}
