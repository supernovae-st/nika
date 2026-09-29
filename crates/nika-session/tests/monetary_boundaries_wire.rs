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

/// The paused journal of the C7b `S1` public run (C6 binary, minimized, every frame kept): the
/// engine's own run identity, which a reopened session judges before it offers the gate again.
const S1_PAUSED: (&str, &str) = (
    "2026-09-28T12-59-54Z-53b6.ndjson",
    r#"{"id":{"uuid":"01a0e819-b68b-7649-85b2-39277be74e66"},"timestamp":1790600394379000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"68bc0fa6f93982fd69bcd7dc3b4074d55f54a57579461599d47765293bdbf7cd"}]}
{"id":{"uuid":"01a0e819-b68c-726d-a8e3-3ef859c76d0f"},"timestamp":1790600394380000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"527926e042b24c4415b65b50cca37f0f1f609ec9f52478191a9faf23491600c3","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b68d-735a-9777-3c6706958b21"},"timestamp":1790600394381000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"eee513cc41db18434eb38cbf51b55d48946fbea527dc0f80777a31deaff40551","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-b68d-735a-9777-3c683f5bba50"},"timestamp":1790600394381000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"0c76a73643ecc528974ba46ceb6025423d93139955cbd8807ee4549ed3be67f9","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-b690-75e2-8832-255d6a8aeee0"},"timestamp":1790600394384000000,"kind":"task_started","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"1ee9c3dc4a185833b486d65cc324a5022fc69a96f4f39707ff60d4f19e4493a2","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b690-75e2-8832-255ea327f97a"},"timestamp":1790600394384000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"a7f5e04b1f6ddcd5ee13fa89aed3240f228b59b4ea4620fbb6c51bd442e8c5d8","fields":[{"key":"task","value":"before"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-b690-75e2-8832-255f93243257"},"timestamp":1790600394384000000,"kind":"task_completed","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"1e89b99a6737b7686166dd97a346879fb2d829c2ca73b550ba2a9b00967273b7","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b691-7011-a1fc-369f8aa8657f"},"timestamp":1790600394385000000,"kind":"workflow_paused","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"33180c9c50ec797c947a4969df6319a91f404bad00f875bcfd7d44e02bdccba0","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"message","value":"Ship it?"},{"key":"status","value":"paused"},{"key":"cause","value":"human_gate"}]}
"#,
);

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
    let traces = root.join(".nika/traces");
    std::fs::create_dir_all(&traces).expect("trace store");
    let trace = traces.join(S1_PAUSED.0);
    std::fs::write(&trace, S1_PAUSED.1).expect("the engine's paused journal");
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
            let out = session.turn(&input);
            // A6 permits the deterministic copy to reach review under a valid ceiling;
            // cognition, save and execution are still unauthorized by this turn.
            let deterministic = verb.starts_with("Prépare") && amount.is_some();
            if deterministic {
                assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
            } else {
                assert!(matches!(out, TurnOutcome::Refusal(_)), "{out:?}");
            }
            let money = session.monetary_decision().expect("money");
            assert_eq!(money.input, input);
            assert_eq!(money.effective_usd, amount);
            assert_eq!(session.pending_proposal().is_some(), deterministic);
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
