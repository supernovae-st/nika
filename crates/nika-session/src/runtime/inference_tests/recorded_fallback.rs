// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A workflow this Session authored, its semantic record kept, asked for a change its fixed graph
//! cannot carry (another write): the links route refuses the structural addition, and the same
//! change is stated over the complete document of the saved bytes, then judged, proposed and
//! saved; the earlier bytes stay, and every call stays paid. Public turns, real loopback inference.
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

/// The added write stated over the document: a new task and the grant it needs.
fn operations() -> String {
    let text = "with: { content: \"${{ tasks.read_source.output }}\" }\ninvoke: { tool: \"nika:write\", args: { path: \"./copie-b.txt\", content: \"${{ with.content }}\" } }";
    let empty = |op: Value| {
        let mut op = op;
        for field in [
            "path",
            "key",
            "text",
            "to",
            "value_json",
            "component",
            "version",
            "bindings_json",
        ] {
            if op.get(field).is_none() {
                op[field] = json!("");
            }
        }
        op
    };
    json!({"notes": "", "replace": "", "operations": [
        empty(json!({"op": "insert_text", "path": "/tasks", "key": "write_copy_b", "text": text})),
        empty(json!({"op": "push", "path": "/permits/fs/write", "value_json": "\"./copie-b.txt\""})),
    ]})
    .to_string()
}

#[test]
fn a_recorded_workflow_takes_a_change_its_graph_refuses_over_its_whole_document() {
    // Links the fixed graph refuses (a clause the original request never states, told back
    // once and stated again: nothing new): the route a refused structural addition takes too.
    let links = json!({"supersedes": [{"replaces": "écris aussi dans une autre copie",
        "by": CHANGE.trim_end_matches('.')}], "adds": [], "notes": ""});
    let mut replies = authored(response);
    replies.push((200, response(&links.to_string())));
    replies.push((200, response(&links.to_string())));
    replies.push((200, response(&operations())));
    replies.extend((0..6).map(|_| (200, response(JUDGE_APPROVES))));
    let peer = Peer::start(replies);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let mut s = open(dir.path());
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
    let revision = s.pending_revision().expect("a document revision");
    assert_eq!(revision.mode, "operations", "{preview}");
    let decision = (s.last_outcome.as_ref())
        .and_then(|o| o.provenance.decision.clone())
        .unwrap_or_default();
    assert!(
        decision["recorded_attempt"]["refused"].is_array(),
        "the refused links round stays journaled: {decision:#}"
    );
    let receipt = (s.last_outcome.as_ref())
        .and_then(|o| o.provenance.authoring.clone())
        .expect("the receipt");
    let calls: Vec<&str> = (receipt.context.iter())
        .filter_map(|call| call["call"].as_str())
        .collect();
    assert!(
        calls.iter().filter(|c| c.starts_with("revision")).count() >= 2,
        "the refused links round and the document revision are both paid: {calls:?}"
    );
    assert!(peer.bodies().len() > authored_calls + 1);

    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    let after = std::fs::read_to_string(dir.path().join(&saved)).expect("revised bytes");
    assert!(after.contains("./copie-b.txt"), "{after}");
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
