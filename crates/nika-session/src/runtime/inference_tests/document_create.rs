// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A fresh request created over its complete document through the Session door: the document
//! door composes an admitted block of the bundled release into the author's own document (real
//! loopback inference: the document, then the round's judge), and the proposal carries the
//! creation's record bound to the proposed bytes, its component witnessed on them.
use super::*;
use crate::turn::RoutingMethod;

const REQUEST: &str = "Compute the sum of one and two, deterministically.";

/// The author's own document: the name and the boundary, no task yet.
const ENVELOPE: &str = "nika: sum-two\npermits: {}\n";

/// Every line is new work: the routing is fixed, so the calls are the document's and the judge's.
struct Fresh;
impl TurnClassifier for Fresh {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        TurnDecision::new(TurnAct::NewWork, RoutingMethod::Model)
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

/// The document call's answer: the envelope, and the release's hole-free block composed into it.
fn composed(version: &str) -> String {
    json!({"candidate": ENVELOPE, "questions": [], "gaps": [], "notes": "the admitted block",
        "operations": [{"op": "compose", "component": "block:typed-inputs-outputs",
            "version": version, "bindings_json": "{}"}]})
    .to_string()
}

#[test]
fn a_created_document_composes_an_admitted_block_witnessed_on_the_proposed_bytes() {
    use nika_compile_seats::foundry::ComponentCatalog as _;
    let root = tempfile::tempdir().expect("root");
    let mut s = open(root.path());
    s.with_classifier(Box::new(Fresh));
    let pin = s
        .authoring_context
        .knowledge()
        .expect("the bundled release");
    let version = pin.reopen().expect("it reopens").release().version;
    let peer = Peer::start(vec![
        (200, response(&composed(&version))),
        (200, response(JUDGE_APPROVES)),
    ]);
    let _transport = test_transport::install(&peer.url);
    s.admit_money("budget 2 USD", false, false)
        .expect("allowance");

    let out = s.turn(REQUEST);
    let TurnOutcome::Proposal { preview, .. } = &out else {
        panic!("a proposal of the created document: {out:?}")
    };
    assert!(peer.bodies()[0].to_string().contains("operations"));
    let bytes = (s.candidate())
        .and_then(|c| {
            (c.set.changes.iter())
                .find(|c| c.is_workflow())
                .map(|c| c.content().to_owned())
        })
        .expect("the proposed workflow");
    let created = (s.work().candidate)
        .and_then(|c| c.revision)
        .expect("the creation record binds the proposed bytes");
    assert_eq!(
        (created.mode.as_str(), created.base_sha256.as_deref()),
        ("composed", None),
        "a creation revised no earlier bytes: the author's draft is no base"
    );
    assert_eq!(
        created.candidate_sha256,
        nika_compile::surface::sha256(&bytes)
    );
    let component = created.components.first().expect("the composed block");
    assert_eq!(
        (component.id.as_str(), component.version.as_deref()),
        ("block:typed-inputs-outputs", Some(version.as_str()))
    );
    assert_eq!(
        component.witness, "expanded",
        "its nodes, on these very bytes: {bytes}"
    );
    assert!(preview.contains("created · composed"), "{preview}");
    assert!(
        preview.contains("component · block:typed-inputs-outputs"),
        "{preview}"
    );
}
