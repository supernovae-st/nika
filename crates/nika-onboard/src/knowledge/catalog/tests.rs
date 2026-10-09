// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The embedded admitted releases as catalogues: a checked block resolved by id and release and
//! re-verified, expanded into a person's document and checked, witnessed on the candidate's bytes
//! and revised; a request that shares no word with that block reaching it through the whole
//! catalogue, never through the lexical pack. A retained R3 pin never resolves a later release's
//! component.

use nika_compile_seats::foundry::reach::DESCRIPTOR;
use nika_compile_seats::foundry::witness::witness;
use nika_compile_seats::foundry::{
    Binding, BindingError, ComponentCatalog, ComponentRef, Unresolved, expand, instantiate,
    qualified_with, reused, revise,
};
use serde_json::{Value, json};

use nika_compile::surface::{initial, literal_projection, sha256};

use crate::compile::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat};
use crate::compile::{AuthoringKnowledge, CompileRequest};
use crate::knowledge::{Snapshot, TrustedIdentity, bundled, fixture};

/// The admitted filter-then-report block of the current release.
const BLOCK: &str = "block:validate-quarantine-total";
/// Its admitted bytes, as the release ships them.
const BYTES: &str =
    include_str!("../../../assets/knowledge-release-r2/blocks/validate-quarantine-total.nika");

fn current() -> Snapshot {
    bundled::admit(Some(&bundled::identity().unwrap())).unwrap()
}

fn r3() -> Snapshot {
    let policy = "d0471eeb904416dd411fde12244918771a5578ae1526f1e0a775b5d421087f36";
    let identity = TrustedIdentity::new(bundled::R3_SNAPSHOT_SHA256, bundled::POLICY_ID, policy);
    bundled::admit(Some(&identity.unwrap())).unwrap()
}

/// The person's own document: their name, their boundary over their files.
const PARENT: &str = r#"nika: orders-quarantine
permits:
  fs: { read: ["./in/orders.json", "./in/order.schema.json"], write: ["./out/rejected.json", "./out/total.txt"] }
  tools: ["nika:read", "nika:jq", "nika:validate", "nika:write"]
tasks: {}
"#;

fn bindings() -> Vec<Binding> {
    vec![
        Binding::new("const.batch_path", json!("./in/orders.json")),
        Binding::new("const.schema_path", json!("./in/order.schema.json")),
        Binding::new("const.amount_field", json!("amount")),
        Binding::new("const.quarantine_path", json!("./out/rejected.json")),
        Binding::new("const.total_path", json!("./out/total.txt")),
    ]
}

#[test]
fn a_checked_block_resolves_by_id_and_release_with_its_admitted_bytes() {
    let snapshot = current();
    let release = snapshot.release();
    assert_eq!(
        release.version,
        "knowledge-0.123.0-r2-json-filter-records-20261009"
    );
    assert_eq!(release.snapshot_sha256, snapshot.manifest_sha256());
    let pinned = ComponentRef::new(BLOCK)
        .at_version(release.version.clone())
        .in_release(release.snapshot_sha256.clone());
    let component = snapshot.resolve(&pinned).unwrap();
    assert_eq!(component.source, BYTES);
    assert_eq!(component.file_sha256, sha256(BYTES));
    assert_eq!(
        component.file_sha256, "197b59e821e6b8c185c81153e18e2919d60c4ce45e63fd33f8ed5f85ab0eca9c",
        "the manifest's pin for blocks/validate-quarantine-total.nika"
    );
    let holes: Vec<&str> = component.holes.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(
        holes,
        [
            "const.batch_path",
            "const.schema_path",
            "const.amount_field",
            "const.quarantine_path",
            "const.total_path"
        ]
    );
    assert_eq!(
        (component.status.as_str(), component.proof_level.as_str()),
        ("EXPERIMENTAL", "CHECKED")
    );
    // Every refusal is typed; no other component and no text stands in.
    let resolve = |reference: ComponentRef| snapshot.resolve(&reference);
    assert_eq!(
        resolve(ComponentRef::new("block:nope")),
        Err(Unresolved::Unknown("block:nope".to_owned()))
    );
    assert!(matches!(
        resolve(ComponentRef::new("pattern:validate-records")),
        Err(Unresolved::NotExecutable { .. })
    ));
    assert!(matches!(
        resolve(ComponentRef::new(BLOCK).at_version("knowledge-0.122.0")),
        Err(Unresolved::VersionMismatch { .. })
    ));
    assert!(matches!(
        resolve(ComponentRef::new(BLOCK).in_release("0".repeat(64))),
        Err(Unresolved::ReleaseMismatch { .. })
    ));
    // A revision pinned to R3 keeps R3: that release never held this block.
    let earlier = r3();
    assert_eq!(
        earlier.resolve(&ComponentRef::new(BLOCK)),
        Err(Unresolved::Unknown(BLOCK.to_owned()))
    );
    let r3_block =
        ComponentRef::new("block:run-deterministic").in_release(bundled::R3_SNAPSHOT_SHA256);
    assert_eq!(
        earlier.resolve(&r3_block).unwrap().release.snapshot_sha256,
        bundled::R3_SNAPSHOT_SHA256
    );
    assert!(matches!(
        snapshot.resolve(&r3_block),
        Err(Unresolved::ReleaseMismatch { .. })
    ));
}

#[test]
fn the_admitted_block_expands_into_the_persons_document_checks_and_revises() {
    let snapshot = current();
    let component = snapshot.resolve(&ComponentRef::new(BLOCK)).unwrap();
    // Its probe's paths are never the request's: every human hole must be bound.
    let partial = instantiate(&component, &bindings()[..4]).unwrap();
    assert_eq!(
        expand(PARENT, &partial),
        Err(nika_compile_seats::foundry::ExpandError::Binding(
            BindingError::Unbound(vec!["const.total_path".to_owned()])
        ))
    );
    let expansion = expand(PARENT, &instantiate(&component, &bindings()).unwrap()).unwrap();
    assert!(expansion.ready, "{:#}", expansion.receipt["check"]);
    let candidate = &expansion.candidate;
    for probe in [
        "./data/batch.json",
        "./out/quarantine.json",
        "mock/echo",
        "p52-validate-quarantine-total",
    ] {
        assert!(
            !candidate.contains(probe),
            "{probe} inherited:\n{candidate}"
        );
    }
    let receipt = &expansion.receipt;
    assert_eq!(receipt["component"]["id"], BLOCK);
    assert_eq!(receipt["component"]["file_sha256"], sha256(BYTES));
    assert_eq!(
        receipt["component"]["release"]["snapshot_sha256"],
        snapshot.manifest_sha256()
    );
    assert_eq!(receipt["authority"]["inherited"], false);
    assert_eq!(receipt["not_inherited"]["model"], "mock/echo");
    let tasks: Vec<&String> = receipt["nodes"]["tasks"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(tasks.len(), 11, "{tasks:?}");
    assert_eq!(witness(receipt, candidate)["verdict"], "expanded");
    // A revision of one bound literal keeps every other byte and carries the receipt.
    let change = [Binding::new("const.amount_field", json!("montant"))];
    let (revised, carried) = revise(candidate, receipt, &component, &change).unwrap();
    let same = |a: &str, b: &str| a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    let head = same(candidate, &revised);
    let tail = same(
        &candidate[head..].chars().rev().collect::<String>(),
        &revised[head..].chars().rev().collect::<String>(),
    );
    assert_eq!(
        (
            &candidate[head..candidate.len() - tail],
            &revised[head..revised.len() - tail]
        ),
        ("amount", "\"montant\""),
        "one literal span changed, every other byte kept"
    );
    assert_eq!(
        literal_projection(&revised).unwrap()["const"]["amount_field"],
        "montant"
    );
    assert_eq!(carried["check"]["ready"], true, "{:#}", carried["check"]);
    assert_eq!(witness(&carried, &revised)["verdict"], "expanded");
    assert_eq!(witness(receipt, &revised)["verdict"], "revised");
    // The kind the block holds is the kind a binding must give.
    let wrong = [Binding::new("const.amount_field", json!(7))];
    assert!(matches!(
        revise(&revised, &carried, &component, &wrong),
        Err(BindingError::Incompatible { .. })
    ));
}

/// A seat that finds only the filter-then-report block applicable, and every other entry
/// unrelated or undecided.
struct OnlyTheBlock;

impl DecisionSeat for OnlyTheBlock {
    fn name(&self) -> &'static str {
        "test/only-the-block"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        let id = question.state["reference"]["id"]
            .as_str()
            .unwrap_or_default();
        let choice = match id {
            BLOCK => "applies",
            id if id.starts_with("block:") => "unrelated",
            _ => "none",
        };
        Box::pin(async move { Ok(ChoiceAnswer::new(choice, "test/only-the-block")) })
    }
}

#[tokio::test]
async fn a_request_sharing_no_word_with_the_block_reaches_it_through_the_whole_catalogue() {
    let snapshot = current();
    let intent = "Écarte chaque commande non conforme au contrat puis additionne le reste";
    let pack: AuthoringKnowledge = snapshot.pack(intent, None).unwrap();
    assert!(
        !pack.references.iter().any(|r| r.id == BLOCK),
        "the lexical pack never held it: {:?}",
        pack.references.iter().map(|r| &r.id).collect::<Vec<_>>()
    );
    let request = CompileRequest::create(intent).with_authoring_knowledge(pack);
    let (shown, record) = qualified_with(intent, &request, Some(&OnlyTheBlock), Some(&snapshot))
        .await
        .unwrap();
    let coverage = &record["coverage"];
    assert_eq!(coverage["complete"], true, "{coverage:#}");
    assert_eq!(
        coverage["admitted_entries"], 358,
        "every r2 row but its 9 source artifacts"
    );
    assert_eq!(coverage["resolved_in_full"], 1);
    assert_eq!(
        coverage["release"]["snapshot_sha256"],
        snapshot.manifest_sha256()
    );
    let references = &shown.authoring_knowledge.as_ref().unwrap().references;
    let block = references.iter().find(|r| r.id == BLOCK).unwrap();
    assert!(
        block.text.contains("total_valid:"),
        "resolved in full:\n{}",
        block.text
    );
    assert!(
        !references
            .iter()
            .any(|r| r.id == "block:lookup-enrich-by-key")
    );
    let undecided = references
        .iter()
        .find(|r| r.id == "skeleton:compile/validate-records")
        .unwrap();
    assert!(undecided.text.starts_with(DESCRIPTOR));
    let row: &Value = (record["references"].as_array().unwrap().iter())
        .find(|r| r["id"] == BLOCK)
        .unwrap();
    assert_eq!(
        (row["asked"].as_str(), row["shown"].as_str()),
        (Some("descriptor"), Some("full"))
    );
    // Selected, then bound and expanded: the reuse the candidate's bytes hold.
    let component = snapshot.resolve(&ComponentRef::new(BLOCK)).unwrap();
    let expansion = expand(PARENT, &instantiate(&component, &bindings()).unwrap()).unwrap();
    let mut out = initial();
    out.candidate = Some(expansion.candidate.clone());
    reused(
        &shown,
        record,
        std::slice::from_ref(&expansion.receipt),
        &mut out,
    );
    let reuse = &out.provenance.decision.as_ref().unwrap()["knowledge_qualification"]["reuse"];
    assert_eq!(reuse["expanded"], 1, "{reuse:#}");
}

/// A synthetic filter-then-report block: the records older than a threshold, counted and
/// written. A test fixture, never part of an issued release.
const STALE: &str = include_str!("../../../tests/fixtures/foundry/stale-filter-report.nika");

/// A synthetic release holding that block, admitted through the same strict door as an issued
/// one; its check receipt is the fixture's synthetic receipt.
fn stale_release() -> Snapshot {
    let file = "blocks/stale-filter-report.nika";
    let mut block = fixture::row(
        "block",
        "block:stale-filter-report",
        "Records older than a threshold, counted and written",
        "EXPERIMENTAL",
        "CHECKED",
    );
    block["purpose"] = json!(
        "Keep the records whose age_hours exceeds the threshold, count them, write their ids."
    );
    block["file"] = json!(file);
    block["file_sha256"] = json!(sha256(STALE));
    block["holes"] = json!([
        {"name": "const.records_path", "owner": "human"},
        {"name": "const.report_path", "owner": "human"},
        {"name": "const.max_age_hours", "owner": "human", "note": "the age the request states, in hours"},
    ]);
    block["effects"] = json!(["fs.read", "fs.write"]);
    block["authority"] = json!(["permits.fs", "permits.tools"]);
    block["interfaces"] = json!(["RecordTransformer"]);
    block["callables"] = json!(["nika:jq", "nika:read", "nika:write"]);
    block["known_failure_modes"] = json!(["a record without age_hours is never stale"]);
    block["check_receipt"] = json!({
        "verifier_sha256": fixture::VERIFIER, "spec_sha": fixture::SPEC,
        "sha256": sha256(STALE), "verdict": "CURRENT_CHECKED",
    });
    let mut payload = fixture::Payload::minimal();
    payload.kind("block").push(block);
    payload
        .files
        .insert(file.to_owned(), STALE.as_bytes().to_vec());
    let files = payload.files();
    let identity = fixture::identity_of(&files).unwrap();
    Snapshot::from_files("fixture:stale-filter-report", files, Some(&identity)).unwrap()
}

/// The person's document for the stale-tickets request.
const STALE_PARENT: &str = r#"nika: stale-tickets-report
permits:
  fs: { read: ["./in/tickets.json"], write: ["./out/report.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks: {}
"#;

fn stale_bindings(hours: i64) -> Vec<Binding> {
    vec![
        Binding::new("const.records_path", json!("./in/tickets.json")),
        Binding::new("const.report_path", json!("./out/report.json")),
        Binding::new("const.max_age_hours", json!(hours)),
    ]
}

/// The expansions a run is judged on, pinned byte for byte: what the engine runs is exactly what
/// expansion and revision produce here.
#[test]
fn the_expansions_and_the_48_to_72_revision_are_the_pinned_candidates() {
    let snapshot = current();
    let component = snapshot.resolve(&ComponentRef::new(BLOCK)).unwrap();
    let quarantine = expand(PARENT, &instantiate(&component, &bindings()).unwrap()).unwrap();
    let pinned = include_str!("../../../tests/fixtures/foundry/orders-quarantine.nika");
    assert_eq!(quarantine.candidate, pinned);
    assert_eq!(quarantine.receipt["candidate_sha256"], sha256(pinned));
    let stale = stale_release();
    assert_eq!(stale.release().version, fixture::VERSION);
    let component = stale
        .resolve(&ComponentRef::new("block:stale-filter-report"))
        .unwrap();
    assert_eq!(component.source, STALE);
    let at48 = expand(
        STALE_PARENT,
        &instantiate(&component, &stale_bindings(48)).unwrap(),
    );
    let at48 = at48.unwrap();
    assert!(at48.ready, "{:#}", at48.receipt["check"]);
    let pinned48 = include_str!("../../../tests/fixtures/foundry/stale-tickets-48.nika");
    assert_eq!(at48.candidate, pinned48);
    let change = [Binding::new("const.max_age_hours", json!(72))];
    let (at72, receipt72) = revise(&at48.candidate, &at48.receipt, &component, &change).unwrap();
    let pinned72 = include_str!("../../../tests/fixtures/foundry/stale-tickets-72.nika");
    assert_eq!(at72, pinned72);
    assert_eq!(
        (
            receipt72["revises"].clone(),
            receipt72["candidate_sha256"].clone()
        ),
        (json!(sha256(pinned48)), json!(sha256(pinned72)))
    );
    // The revision is the one bound literal; the rest of the program is the same bytes.
    assert_eq!(pinned48.replacen("value: 48 }", "value: 72 }", 1), pinned72);
    assert_eq!(witness(&receipt72, pinned72)["verdict"], "expanded");
    assert_eq!(witness(&at48.receipt, pinned72)["verdict"], "revised");
}
