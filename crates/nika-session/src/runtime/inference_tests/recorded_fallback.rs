// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A workflow this Session authored, its semantic record kept, asked for a change its fixed graph
//! cannot carry (another write): the seat's one typed links answer adds the clause and names the
//! write it copies (`like`), so the source laws prove the added destination on the saved bytes
//! from that same answer (no second revision call, no fill), then it is judged, proposed and
//! saved; the earlier bytes stay. Public turns, real loopback inference.
use super::*;
use crate::turn::RoutingMethod;

const CHANGE: &str =
    "Je veux aussi que copie-b.txt contienne exactement les octets présents dans entree.txt.";

struct Acts;
impl TurnClassifier for Acts {
    fn classify(&mut self, _: &TurnContext, line: &str) -> TurnDecision {
        let act = if line == CHANGE {
            TurnAct::Modify
        } else {
            TurnAct::NewWork
        };
        TurnDecision::new(act, RoutingMethod::Model)
    }
    fn classify_with_admission(
        &mut self,
        c: &TurnContext,
        line: &str,
        _: &nika_providers::InferenceAdmission,
    ) -> TurnDecision {
        self.classify(c, line)
    }
}

#[test]
fn a_recorded_workflow_takes_a_change_its_graph_refuses_over_its_whole_document() {
    // The added copy, typed by the seat as a destination copying the base's write (`like`):
    // no fill carries another write; the source laws prove it on the saved bytes.
    let links = json!({"supersedes": [], "adds": [CHANGE.trim_end_matches('.')],
        "like": "sortie.txt", "notes": ""});
    // The saved workflow's semantic record is the sketch door's: this test names that door.
    let mut replies: Vec<_> = (sketched_copy("sortie.txt").iter())
        .map(|t| (200, response(t)))
        .collect();
    replies.push((200, response(JUDGE_APPROVES)));
    // The room copies the source in and runs the bytes whole: « faithful » stands once the
    // request's one part is carried over that run.
    replies.push((200, response(r#"{"choice":"carried"}"#)));
    replies.push((200, response(&links.to_string())));
    replies.extend((0..6).map(|_| (200, response(JUDGE_APPROVES))));
    let peer = Peer::start(replies);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let mut s = open(dir.path());
    s.set_authoring_context(crate::authoring::AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none().with_strategy("sketch"),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    ));
    s.with_classifier(Box::new(Acts));
    s.admit_money("budget 2 USD", false, false)
        .expect("allowance");

    let out = s.turn(WORK);
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("the authored proposal: {out:?}");
    };
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    let saved = s.last_workflow.clone().expect("the saved workflow");
    let before = std::fs::read_to_string(dir.path().join(&saved)).expect("saved bytes");
    assert!(
        (s.programs.as_ref()).is_some_and(|p| p.to_string().contains("semantic_record")),
        "the saved workflow keeps its semantic record"
    );
    let authored_calls = peer.bodies().len();

    // The Session's revision door over the saved workflow (the routing of a typed line is not
    // what this test reads).
    let out = s.revise_saved(&saved, CHANGE);
    let TurnOutcome::Proposal { id, preview } = out else {
        panic!(
            "the change over the whole document: {out:?}\n{:#?}",
            s.last_outcome
        );
    };
    assert!(preview.contains("copie-b.txt"), "{preview}");
    let plan = (s.last_outcome.as_ref())
        .and_then(|o| o.provenance.plan.clone())
        .unwrap_or_default();
    assert!(
        !plan["source_revision"].is_null() && plan["semantic_base_sha256"].is_string(),
        "the source laws proved it on the bound bytes: {plan:#}"
    );
    let receipt = (s.last_outcome.as_ref())
        .and_then(|o| o.provenance.authoring.clone())
        .expect("the receipt");
    let calls: Vec<&str> = (receipt.context.iter())
        .filter_map(|call| call["call"].as_str())
        .collect();
    assert_eq!(
        calls.iter().filter(|c| c.starts_with("revision")).count(),
        1,
        "one typed answer, no second revision call: {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("fill")),
        "no fill: {calls:?}"
    );
    assert!(peer.bodies().len() > authored_calls + 1);

    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    let after = std::fs::read_to_string(dir.path().join(&saved)).expect("revised bytes");
    assert!(after.contains("copie-b.txt"), "{after}");
    let kept: Vec<&str> = (before.lines())
        .filter(|line| {
            !["fs:", "write:", "permits:"]
                .iter()
                .any(|p| line.trim_start().starts_with(p))
        })
        .collect();
    for line in &kept {
        assert!(
            after.contains(line),
            "the earlier line « {line} » is kept: {after}"
        );
    }
    assert!(!dir.path().join("copie-b.txt").exists(), "Save is not Run");
}
