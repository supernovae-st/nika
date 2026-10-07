// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Public turns and real loopback inference; a fixed classifier isolates accounting from labels.
use super::unknown_cost::{asked, open_unknown, unpriced_response};
use super::*;
use crate::turn::RoutingMethod;
const CREATE: &str = "Read inventory.json, keep the items whose stock is under 8, sort them by sku and write that JSON array to ./reorder.json.";
const CHANGE: &str = "Keep the items whose stock is under 5 instead.";
const ROWS: &str = r#"[{"sku":"B2","stock":3},{"sku":"A1","stock":9},{"sku":"C3","stock":5}]"#;
struct Acts;
impl TurnClassifier for Acts {
    fn classify(&mut self, _: &TurnContext, line: &str) -> TurnDecision {
        TurnDecision::new(
            if line == CHANGE {
                TurnAct::Modify
            } else {
                TurnAct::NewWork
            },
            RoutingMethod::Model,
        )
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
fn session(root: &Path, home: &Path) -> SessionRuntime {
    let mut s = open_unknown(root);
    s.set_authoring_context(crate::authoring::AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none().with_strategy("sketch"),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    ));
    s.with_classifier(Box::new(Acts));
    s.enable_history(home).unwrap();
    s.restore_state();
    s
}
fn fills(cutoff: u8) -> String {
    json!({"fills":[{"task":"pick","field":"expression","value":format!(
        "fromjson | map(select(.stock < {cutoff})) | sort_by(.sku) | map({{sku, stock}})")}],"notes":"filter"}).to_string()
}
fn replies() -> Vec<(u16, Value)> {
    let graph = json!({"name":"reorder","tasks":[
        {"id":"read_inventory","verb":"invoke","tool":"nika:read","reads":["inventory.json"],"purpose":"read"},
        {"id":"pick","verb":"invoke","tool":"nika:jq","with":[{"name":"document","from":"read_inventory"}],"purpose":"filter and sort"},
        {"id":"save","verb":"invoke","tool":"nika:write","writes":["./reorder.json"],"with":[{"name":"items","from":"pick"}],"purpose":"write"}],
        "questions":[],"gaps":[],"notes":"inventory"});
    let links = json!({"supersedes":[{"replaces":"keep the items whose stock is under 8",
        "by":"Keep the items whose stock is under 5 instead"}],"adds":[],"notes":"replace filter only"});
    [
        graph.to_string(),
        fills(8),
        JUDGE_APPROVES.into(),
        links.to_string(),
        fills(5),
        JUDGE_APPROVES.into(),
    ]
    .iter()
    .map(|text| (200, unpriced_response(text)))
    .collect()
}
fn approve_and_save(s: &mut SessionRuntime) -> crate::outcome::ProposalId {
    let out = s.turn("yes");
    let TurnOutcome::Proposal { id, .. } = out else {
        panic!("proposal: {out:?}")
    };
    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    id
}
#[test]
fn completed_unknown_create_save_reopen_edit_needs_fresh_consent_and_keeps_every_scope() {
    let peer = Peer::start(replies());
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("inventory.json"), ROWS).unwrap();
    let mut s = session(root.path(), home.path());
    asked(&s.turn(CREATE));
    assert!(peer.bodies().is_empty());
    approve_and_save(&mut s);
    assert_eq!(peer.bodies().len(), 3);
    let prior = s.cost_observations();
    let before = std::fs::read(root.path().join("reorder.nika")).unwrap();
    assert_eq!(prior.len(), 1);
    assert_eq!(prior[0]["state"], "Closed");
    drop(s);
    let mut s = session(root.path(), home.path());
    assert!(s.inference_receipt().unwrap().is_none());
    assert!(s.money.reconfirm);
    assert_eq!(s.cost_observations(), prior);
    asked(&s.turn(CHANGE));
    assert_eq!(peer.bodies().len(), 3, "no request before fresh review");
    assert_eq!(
        std::fs::read(root.path().join("reorder.nika")).unwrap(),
        before
    );
    approve_and_save(&mut s);
    assert_eq!(peer.bodies().len(), 6);
    let after = s.cost_observations();
    assert_eq!(after.len(), 2);
    assert_eq!(&after[..1], &prior);
    assert_ne!(
        after[0]["unknown_cost"]["invocation"],
        after[1]["unknown_cost"]["invocation"]
    );
    let source = std::fs::read_to_string(root.path().join("reorder.nika")).unwrap();
    assert!(source.contains("stock < 5") && source.contains("sort_by(.sku)"));
    assert!(!source.contains("stock < 8"));
    assert_eq!(
        std::fs::read_to_string(root.path().join("inventory.json")).unwrap(),
        ROWS
    );
    assert!(
        !root.path().join("reorder.json").exists(),
        "Save is not Run"
    );
    drop(s);
    let mut s = session(root.path(), home.path());
    asked(&s.turn(CHANGE));
    assert_eq!(peer.bodies().len(), 6);
    assert_eq!(
        s.cost_observations(),
        after,
        "reopen does not duplicate or discard scopes"
    );
    assert!(
        s.status()
            .contains("completed invocation exposure retained")
    );
}
#[test]
fn changed_closed_report_after_review_refuses_without_a_request() {
    let peer = Peer::start(vec![(200, unpriced_response("Hello"))]);
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut s = open_unknown(root.path());
    s.enable_history(home.path()).unwrap();
    asked(&s.turn("hello"));
    let _ = s.turn("yes");
    drop(s);
    let mut s = open_unknown(root.path());
    s.enable_history(home.path()).unwrap();
    s.restore_state();
    asked(&s.turn("hello"));
    let mut state = crate::SessionState::load(root.path()).unwrap().unwrap();
    state.updated_at = "2026-10-05T19:00:00Z".into();
    state.save(root.path()).unwrap();
    assert!(matches!(s.turn("yes"), TurnOutcome::Refusal(_)));
    assert_eq!(peer.bodies().len(), 1);
}
#[test]
fn zero_and_malformed_or_nonclosed_history_never_resume_cognition() {
    for bad in 0..6 {
        let peer = Peer::start(vec![(200, unpriced_response("Hello"))]);
        let _transport = test_transport::install(&peer.url);
        let root = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut s = open_unknown(root.path());
        s.enable_history(home.path()).unwrap();
        asked(&s.turn("hello"));
        let _ = s.turn("yes");
        drop(s);
        let mut state = crate::SessionState::load(root.path()).unwrap().unwrap();
        match bad {
            0 => state.inference_observations[0]["state"] = json!("Open"),
            1 => state.inference_observations[0]["state"] = json!("Uncertain"),
            2 => state.inference_checkpoint = Some(json!("other failure")),
            3 => state
                .decisions
                .push(format!("{}old", super::super::inference::DISPATCH_PREFIX)),
            5 => state.inference_observations.push(json!({
                "schema":"nika/session-decision-seat@1", "kind":"decision_seat",
                "seat":"typesafe/jev-1.13.0", "unbudgeted":true, "state":"Open",
                "calls_sent":0, "unknown_calls":0,
                "attempts":[{"question":"routing", "sent":false, "outcome":"refused",
                    "error":"not consulted: unknown-cost scope"}]
            })),
            _ => {}
        }
        state.save(root.path()).unwrap();
        let mut s = open_unknown(root.path());
        s.enable_history(home.path()).unwrap();
        s.restore_state();
        let line = if bad == 4 {
            "hello budget 0 USD"
        } else {
            "hello"
        };
        let _ = s.turn(line);
        assert!(!s.waiting_cost_choice());
        assert_eq!(peer.bodies().len(), 1);
        assert!(s.inference_receipt().unwrap().is_none());
        assert_eq!(s.cost_observations(), state.inference_observations);
    }
}
