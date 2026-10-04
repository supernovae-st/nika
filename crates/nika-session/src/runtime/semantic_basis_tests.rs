// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The source basis of a proposal the semantic route (a sketch and its fills) authored, through
//! the real Session → compiler → registry → reqwest path on loopback, with scripted seat answers:
//! a yes saves the exact bytes over an unchanged project without any run, and withdraws them when
//! a key the program reads or its source is gone; rows added never move it. Nothing leaves the
//! machine.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
use super::inference_tests::wire::{Peer, response};
use super::*;
use crate::DataLocus;
use crate::reasoner::{ProviderReasoner, test_transport};
use serde_json::json;

const MODEL: &str = "deepseek/deepseek-v4-pro";
const WORK: &str = "Read inventory.json, keep the items whose stock is under 8, sort them by sku and write that JSON array to ./reorder.json.";
const ROWS: &str =
    r#"[{"sku": "B2", "stock": 3}, {"sku": "A1", "stock": 9}, {"sku": "C3", "stock": 5}]"#;
const JUDGE_APPROVES: &str = r#"{"choice":"faithful"}"#;

/// The seat's graph: read the inventory, keep and sort it, write the array.
fn graph() -> String {
    json!({"name": "reorder", "tasks": [
        {"id": "read_inventory", "verb": "invoke", "tool": "nika:read",
         "reads": ["inventory.json"], "purpose": "the inventory"},
        {"id": "pick", "verb": "invoke", "tool": "nika:jq",
         "with": [{"name": "document", "from": "read_inventory"}],
         "purpose": "keep the low stock items, sorted by sku"},
        {"id": "save", "verb": "invoke", "tool": "nika:write", "writes": ["./reorder.json"],
         "with": [{"name": "items", "from": "pick"}], "purpose": "write the array"}],
        "questions": [], "gaps": [], "notes": "graph"})
    .to_string()
}

/// The seat's fills: the one program, reading `stock` and `sku`.
fn fills() -> String {
    json!({"fills": [{"task": "pick", "field": "expression",
        "value": "fromjson | map(select(.stock < 8)) | sort_by(.sku) | map({sku, stock})"}],
        "notes": "fills"})
    .to_string()
}

/// A Session on the loopback seat, its sketch strategy observing `root`.
fn open(root: &Path) -> SessionRuntime {
    let selected = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        model: Some(MODEL.into()),
        locus: DataLocus::Metered {
            provider: "deepseek".into(),
        },
        ready: true,
        why: None,
    };
    let reasoner = || ProviderReasoner {
        model: MODEL.into(),
        label: "DeepSeek".into(),
    };
    let mut s = SessionRuntime::open(root, selected, Box::new(reasoner()));
    s.factory = Some(Box::new(move |_| Box::new(reasoner())));
    s.set_authoring_context(crate::authoring::AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none().with_strategy("sketch"),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    ));
    s
}

/// A proposal authored over `rows`: the scripted sketch, its fills and the judge's approval,
/// each a real loopback request.
struct Proposed {
    peer: Peer,
    _transport: test_transport::Installed,
    dir: tempfile::TempDir,
    s: SessionRuntime,
    id: ProposalId,
}

fn proposed(rows: &str) -> Proposed {
    let peer = Peer::start(vec![
        (200, response(&graph())),
        (200, response(&fills())),
        (200, response(JUDGE_APPROVES)),
    ]);
    let transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    std::fs::write(dir.path().join("inventory.json"), rows).expect("inventory");
    let mut s = open(dir.path());
    let out = s.turn(&format!("{WORK} budget 2 USD."));
    let TurnOutcome::Proposal { id, preview } = out else {
        panic!("a semantic proposal: {out:?}");
    };
    assert!(
        preview.contains("creates `reorder.nika` (35 lines)"),
        "{preview}"
    );
    assert_eq!(
        peer.bodies().len(),
        3,
        "the sketch, its fills and the judge"
    );
    Proposed {
        peer,
        _transport: transport,
        dir,
        s,
        id,
    }
}

impl Proposed {
    fn yes(&mut self) -> TurnOutcome {
        let id = self.id.clone();
        self.s.consent_to(&id, "yes")
    }

    /// The workflow the yes wrote, when it wrote one.
    fn saved(&self) -> Option<String> {
        std::fs::read_to_string(self.dir.path().join("reorder.nika")).ok()
    }

    /// Nothing ran: no output, no run journal, no trace.
    fn nothing_ran(&self) {
        for path in ["reorder.json", ".nika/runs", ".nika/traces"] {
            assert!(!self.dir.path().join(path).exists(), "{path}");
        }
    }

    fn inventory(&self, text: Option<&str>) {
        let path = self.dir.path().join("inventory.json");
        match text {
            Some(text) => std::fs::write(path, text).expect("inventory"),
            None => std::fs::remove_file(path).expect("deleted"),
        }
    }
}

#[test]
fn an_unchanged_project_saves_the_exact_semantic_candidate_without_any_run() {
    let mut p = proposed(ROWS);
    let bound = p.s.basis.is_some();
    let out = p.yes();
    let TurnOutcome::Facts(text) = out else {
        panic!("saved: {out:?}");
    };
    assert!(bound, "a source basis is bound at the proposal");
    assert!(
        text.contains(
            "sources judged again before writing: 2 recorded fact(s) of `inventory.json` hold"
        ) && text.contains("nothing has run"),
        "save must report the rejudged source facts and that nothing ran"
    );
    let workflow = p.saved().expect("the workflow is saved");
    assert_eq!(workflow.lines().count(), 35, "{workflow}");
    assert!(
        workflow.contains("map(select(.stock < 8)) | sort_by(.sku)"),
        "{workflow}"
    );
    p.nothing_ran();
    let kept = std::fs::read_to_string(p.dir.path().join("inventory.json")).expect("kept");
    assert_eq!(kept, ROWS);
    assert_eq!(p.peer.bodies().len(), 3, "a yes calls no seat");
}

#[test]
fn a_removed_key_or_a_deleted_source_withdraws_and_rows_added_still_save() {
    // `stock` renamed in every record: the program's filter would read nothing.
    let mut p = proposed(ROWS);
    p.inventory(Some(&ROWS.replace("stock", "qty")));
    let out = p.yes();
    let TurnOutcome::Refusal(refusal) = out else {
        panic!("withdrawn: {out:?}");
    };
    assert!(
        refusal
            .text
            .contains("the sources this proposal was built on changed")
            && refusal.text.contains("`stock`")
            && refusal.text.contains("nothing was written"),
        "withdrawal must identify the changed source key and that nothing was written"
    );
    assert!(p.saved().is_none());
    p.nothing_ran();
    // The source deleted.
    let mut p = proposed(ROWS);
    p.inventory(None);
    let out = p.yes();
    assert!(
        matches!(&out, TurnOutcome::Refusal(r) if r.text.contains("nothing was written")),
        "{out:?}"
    );
    assert!(p.saved().is_none());
    p.nothing_ran();
    // Rows added and reordered: the recorded facts still hold.
    let mut p = proposed(ROWS);
    p.inventory(Some(
        r#"[{"sku": "D4", "stock": 1}, {"sku": "C3", "stock": 5}, {"sku": "A1", "stock": 9}, {"sku": "B2", "stock": 3}]"#,
    ));
    let out = p.yes();
    assert!(
        matches!(&out, TurnOutcome::Facts(t) if t.contains("2 recorded fact(s) of `inventory.json` hold")),
        "{out:?}"
    );
    assert!(p.saved().is_some());
    p.nothing_ran();
}

#[test]
fn meaning_shows_the_semantic_reading_and_claims_no_carrier() {
    let mut p = proposed(ROWS);
    let out = format!("{:?}", p.s.turn("/meaning"));
    assert!(
        out.contains("Meaning · your request as the compiler read it"),
        "{out}"
    );
    assert!(
        out.contains("the program was judged against the whole request, not clause by clause"),
        "{out}"
    );
    for never in ["unavailable", "represented", "needs your answer"] {
        assert!(!out.contains(never), "{never}: {out}");
    }
    assert!(p.saved().is_none(), "a view writes nothing");
}
