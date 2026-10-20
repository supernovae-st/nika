// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Configuration lends no authority to a suspended decision and resumes no older request.

use super::*;
use crate::intelligence::IntelligenceKind;
use crate::reasoner::NoReasoner;
use crate::work::Waiting;

fn session(root: &Path) -> SessionRuntime {
    let census = IntelligenceCensus::new(vec![], vec!["deepseek".into()], vec![]);
    let preference = UserIntelligencePreference::new(
        IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        None,
    );
    let mut session = SessionRuntime::open_with(
        root,
        census,
        &preference,
        None,
        Box::new(|_| Box::new(NoReasoner)),
    );
    session.enable_continuous_preparation();
    session
}

fn configure_over(s: &mut SessionRuntime, held: &Waiting) {
    const KEPT: &str = "an earlier request still waiting for its own decision";
    s.interrupted = Some(KEPT.into());
    let next_model = if s.intelligence.model.as_deref() == Some("deepseek/deepseek-chat") {
        "deepseek/deepseek-flash"
    } else {
        "deepseek/deepseek-chat"
    };
    let choose = format!("2 {next_model}");
    for answer in ["cancel", choose.as_str()] {
        let original_model = s.intelligence.model.clone();
        let opened = s.submit("/intelligence", held);
        assert!(matches!(opened, TurnOutcome::Ask(_)), "{opened:?}");
        assert_eq!(s.waiting(), Waiting::IntelligenceChoice);
        assert_eq!(s.interrupted.as_deref(), Some(KEPT));
        let configured = s.submit(answer, &Waiting::IntelligenceChoice);
        assert!(
            matches!(configured, TurnOutcome::Facts(_)),
            "{configured:?}"
        );
        assert_eq!(&s.waiting(), held);
        assert_eq!(s.interrupted.as_deref(), Some(KEPT));
        assert!(s.work().requested.is_none());
        assert_eq!(s.work().run, None);
        assert_eq!(
            s.intelligence.model.as_deref(),
            if answer == "cancel" {
                original_model.as_deref()
            } else {
                Some(next_model)
            }
        );
    }
}

#[test]
fn intelligence_configuration_keeps_the_exact_unsaved_proposal() {
    let root = tempfile::tempdir().expect("root");
    let mut s = session(root.path());
    let _ = s.turn("aggregate-by-key");
    let _ = s.submit("EUR", &s.waiting());
    let proposal = s.pending_proposal().expect("proposal");
    let held = Waiting::Consent {
        proposal: proposal.clone(),
    };
    let changes = s.pending.clone();
    configure_over(&mut s, &held);
    assert_eq!(s.pending_proposal(), Some(proposal));
    assert_eq!(s.pending, changes);
    assert!(!root.path().join("aggregate-by-key.nika").exists());
}

#[test]
fn intelligence_configuration_keeps_gate_and_run_review_identities() {
    let root = tempfile::tempdir().expect("root");
    let mut s = session(root.path());
    s.pending_gate = Some(crate::change::PendingGate {
        workflow: PathBuf::from("gate.nika"),
        trace: PathBuf::from(".nika/traces/gate.ndjson"),
        task: "approve".into(),
        message: "Write the report?".into(),
        mode: "confirm".into(),
        values: None,
        withheld: None,
    });
    let gate = s.waiting_gate().expect("gate");
    configure_over(&mut s, &Waiting::Gate { gate: gate.clone() });
    let review = s.run_review_asked("Run this workflow once?", "exact route and bounds");
    configure_over(
        &mut s,
        &Waiting::RunReview {
            review: review.clone(),
        },
    );
    assert_eq!(s.waiting_gate(), Some(gate));
    assert_eq!(s.waiting_review(), Some(review));
    assert!(!root.path().join(".nika/traces/gate.ndjson").exists());
}
