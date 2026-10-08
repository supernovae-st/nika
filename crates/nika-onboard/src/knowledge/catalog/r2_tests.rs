// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A profile r2 release as a catalogue and a pack: the base payload of the shared vectors, which
//! holds every kind, admitted in memory. Every row but a source is a role-labelled choice
//! presented whole with its provenance, contract and body; a counterexample is a boundary that
//! never resolves as a component; a block resolves with its whole contract.

use std::collections::BTreeMap;
use std::path::Path;

use nika_compile_seats::foundry::entry::role;
use nika_compile_seats::foundry::reach::descriptor;
use nika_compile_seats::foundry::release::canonical::jcs_json;
use nika_compile_seats::foundry::{ComponentCatalog, ComponentRef, Unresolved, qualified_with};
use serde_json::Value;

use crate::compile::CompileRequest;
use crate::compile::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat};
use crate::knowledge::{Snapshot, TrustedIdentity};

/// The base payload of the shared r2 vectors, admitted against its identity.
fn base() -> Snapshot {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/knowledge-r2/p01-base.json");
    let vector: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let bytes = |entry: &Value| {
        if let Some(text) = entry["text"].as_str() {
            return text.as_bytes().to_vec();
        }
        let hex = entry["hex"].as_str().unwrap();
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
            .collect()
    };
    let files: BTreeMap<String, Vec<u8>> = (vector["files"].as_object().unwrap().iter())
        .map(|(path, entry)| (path.clone(), bytes(entry)))
        .collect();
    let identity = TrustedIdentity::from_json(&vector["expected"]).unwrap();
    Snapshot::from_files("r2-base", files, Some(&identity)).unwrap()
}

#[test]
fn every_row_but_a_source_is_a_choice_presented_whole_in_its_role() {
    let snapshot = base();
    let entries = snapshot.entries();
    let ids: Vec<&str> = entries
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect();
    let kinds: Vec<&str> = entries
        .iter()
        .filter_map(|row| row["kind"].as_str())
        .collect();
    assert_eq!(entries.len(), 17, "{ids:?}");
    assert!(!kinds.contains(&"source_artifact"));
    for kind in [
        "block",
        "callable",
        "capability_interface",
        "construct",
        "counterexample",
    ] {
        assert!(kinds.contains(&kind), "{kind}");
    }
    for kind in [
        "diagnostic",
        "example",
        "family",
        "intent_facet",
        "pattern",
        "pattern_pack",
    ] {
        assert!(kinds.contains(&kind), "{kind}");
    }
    for kind in ["repair_principle", "skeleton", "skill"] {
        assert!(kinds.contains(&kind), "{kind}");
    }
    for row in &entries {
        let id = row["id"].as_str().unwrap();
        let reference = snapshot.reference(id).unwrap();
        assert_eq!(reference.kind, row["kind"].as_str().unwrap());
        let text = &reference.text;
        assert!(
            text.contains(&format!("role: {}", role(row).unwrap())),
            "{text}"
        );
        assert!(text.contains(&jcs_json(row)), "the whole contract of {id}");
        if let Some(file) = row["file"].as_str() {
            let body = std::str::from_utf8(&snapshot.files[file]).unwrap();
            assert!(text.contains(body.trim_end()), "the whole body of {id}");
        }
        for source in row["provenance"]["sources"].as_array().unwrap() {
            let source = snapshot.row(source.as_str().unwrap()).unwrap();
            assert!(
                text.contains(source["title"].as_str().unwrap()),
                "{id}: {text}"
            );
        }
        let described = descriptor(row);
        assert!(described.contains(&format!("role: {}", role(row).unwrap())));
    }
    assert!(
        snapshot.reference("src:probes").is_none(),
        "a source is no choice"
    );
}

#[test]
fn a_counterexample_is_a_boundary_and_a_block_keeps_its_whole_contract() {
    let snapshot = base();
    let boundary = "counterexample:total:R0:19c2e1a5";
    let refused = snapshot.resolve(&ComponentRef::new(boundary));
    assert!(
        matches!(refused, Err(Unresolved::NotExecutable { .. })),
        "{refused:?}"
    );
    let text = snapshot.reference(boundary).unwrap().text;
    assert!(
        text.contains(
            "role: boundary — what fails and why, to be avoided; never a component, never reused"
        ),
        "{text}"
    );
    let block = snapshot.resolve(&ComponentRef::new("block:total")).unwrap();
    let row = snapshot.row("block:total").unwrap();
    assert_eq!(&block.contract, row);
    assert!(!block.holes.is_empty());
    for (hole, stated) in block.holes.iter().zip(row["holes"].as_array().unwrap()) {
        assert_eq!(&hole.contract, stated);
        assert!(hole.contract["hole_type"].is_string());
    }
    assert_eq!(block.release.profile, "nika-knowledge-release-profile/r2");
}

#[test]
fn an_r2_pack_presents_its_boundaries_contracts_skills_and_diagnostics() {
    let snapshot = base();
    let pack = snapshot
        .pack("total the paid rows of a csv file", None)
        .unwrap();
    let shown: Vec<(&str, &str)> = (pack.references.iter())
        .map(|r| (r.kind.as_str(), r.id.as_str()))
        .collect();
    for wanted in [
        ("block", "block:total"),
        ("example", "example:total"),
        ("counterexample", "counterexample:total:R0:19c2e1a5"),
        ("callable", "callable:nika:jq"),
        ("construct", "construct:with"),
        ("skill", "skill:aggregate"),
    ] {
        assert!(shown.contains(&wanted), "{wanted:?} in {shown:?}");
    }
    let boundary = &pack.selection["counterexamples"][0];
    assert_eq!(boundary["why"], "contrasts with example:total");
    assert_eq!(boundary["presented"], true);
    assert_eq!(pack.selection["receipt"]["contracts"]["presented"], 2);
    let repairs = &pack.repairs["NIKA-VAR-021"];
    assert!(
        repairs
            .iter()
            .any(|line| line.starts_with("diagnostic « NIKA-VAR-021 »")),
        "{repairs:?}"
    );
    assert!(
        repairs
            .iter()
            .any(|line| line.starts_with("repair principle")),
        "{repairs:?}"
    );
    // The case's own corpus is never recalled: neither its example nor that example's boundary.
    let honest = snapshot
        .pack("total the paid rows of a csv file", Some("refeng-cases-v0"))
        .unwrap();
    assert!(
        honest
            .references
            .iter()
            .all(|r| r.id != "example:total" && !r.id.starts_with("counterexample:"))
    );
}

/// A seat that keeps every question it was asked and cannot tell any.
struct Recorder(std::sync::Mutex<Vec<ChoiceQuestion>>);

impl DecisionSeat for Recorder {
    fn name(&self) -> &'static str {
        "test/recorder"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.0.lock().unwrap().push(question.clone());
        Box::pin(async move { Ok(ChoiceAnswer::new("none", "test/recorder")) })
    }
}

#[tokio::test]
async fn a_held_out_corpus_stays_out_of_the_catalogue_reach_judged_or_not() {
    let snapshot = base();
    let holdout = Some("refeng-cases-v0");
    let absent = ["example:total", "counterexample:total:R0:19c2e1a5"];
    let intent = "total the paid rows of a csv file";
    let pack = snapshot.pack(intent, holdout).unwrap();
    let request = CompileRequest::create(intent).with_authoring_knowledge(pack);
    let catalogue = snapshot.catalogue(holdout);
    for id in absent {
        assert!(catalogue.reference(id).is_none(), "{id}");
        assert!(
            !catalogue.entries().iter().any(|row| row["id"] == id),
            "{id}"
        );
        // The whole snapshot still lends it: the holdout is the request's, not the release's.
        assert!(snapshot.reference(id).is_some(), "{id}");
    }
    let seat = Recorder(std::sync::Mutex::new(Vec::new()));
    let (judged, _) = qualified_with(intent, &request, Some(&seat), Some(&catalogue))
        .await
        .unwrap();
    let asked = seat.0.lock().unwrap().clone();
    assert!(!asked.is_empty());
    let (unjudged, _) = qualified_with(intent, &request, None, Some(&catalogue))
        .await
        .unwrap();
    for id in absent {
        let named =
            |q: &ChoiceQuestion| q.instructions.contains(id) || q.state.to_string().contains(id);
        assert!(!asked.iter().any(named), "{id} asked");
        for shown in [&judged, &unjudged] {
            let pack = shown.authoring_knowledge.as_ref().unwrap();
            assert!(
                pack.references
                    .iter()
                    .all(|r| r.id != id && !r.text.contains(id)),
                "{id} shown"
            );
        }
    }
}
