// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The bounded canonical-spelling expansion of a text equality (R4 A5 · C2). « …dont le statut
//! est livré… » over a CSV whose statut spells « livré » with e + U+0301 was READY, ran clean and
//! wrote a header only: byte-exact equality selected no row. Where the observed categorical
//! values of the compared column hold a spelling canonically equivalent (NFC) to the stated
//! literal, the equality now also matches exactly that spelling, and the decision records the
//! law. Case, compatibility forms, accents and spellings the bounded sample did not show stay
//! byte-exact: those cases are pinned here as not solved, never claimed.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use serde_json::{Value, json};

mod common;

const LIVRE: &str = "Lis ./commandes.csv, garde seulement les lignes dont le statut est livré et écris-les dans ./livrees.csv";
const NFC: &str = "livr\u{e9}";
const NFD: &str = "livre\u{301}";

/// The world a host observes for one CSV head: its row and its kinds, as the host builds them.
fn world(head: &str) -> Value {
    let sample = nika_compile::observation::csv(head, false);
    let mut row = json!({
        "path": "./commandes.csv", "state": "observed", "complete": false, "kind": "csv",
        "columns": sample.columns, "bytes": head.len(), "peek_sha256": format!("len-{}", head.len()),
        "delimiter": ",",
    });
    if !sample.values.is_empty() {
        row["values"] = Value::Object(sample.values.into_iter().collect());
    }
    json!({"observed": [row], "kinds": {"./commandes.csv": sample.kinds}})
}

fn head(statuts: &[&str]) -> String {
    let rows: Vec<String> = statuts
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{},{s},{}", i + 1, (i + 1) * 10))
        .collect();
    format!("id,statut,montant\n{}\n", rows.join("\n"))
}

fn ready(intent: &str, world: &Value) -> CompileOutcome {
    let out = compile(&CompileRequest::create(intent).with_knowledge(world.clone())).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    out
}

fn expression(out: &CompileOutcome) -> String {
    common::compute(out.candidate.as_deref().unwrap())
}

#[test]
fn an_observed_decomposed_spelling_is_matched_exactly_beside_the_stated_one() {
    let out = ready(LIVRE, &world(&head(&[NFD, "en cours", NFD, NFD])));
    assert_eq!(
        expression(&out),
        format!("[.records[] | select((.statut == \"{NFC}\" or .statut == \"{NFD}\"))]")
    );
    let record = &out.provenance.decision.as_ref().unwrap()["spellings"][0];
    assert_eq!(record["field"], "statut");
    assert_eq!(record["literal"], NFC);
    assert_eq!(record["spellings"], json!([NFD]));
    assert!(
        record["law"]
            .as_str()
            .unwrap()
            .starts_with("bounded canonical-spelling expansion"),
        "{record}"
    );
    assert_eq!(record["source"], "./commandes.csv");
}

#[test]
fn a_mixed_file_matches_both_spellings_and_a_precomposed_file_stays_byte_exact() {
    let mixed = ready(LIVRE, &world(&head(&[NFD, NFC, "en cours", NFD])));
    assert_eq!(
        expression(&mixed),
        format!("[.records[] | select((.statut == \"{NFC}\" or .statut == \"{NFD}\"))]")
    );
    // The sample holds the stated spelling only: a decomposed one beyond it is unseen, and stays
    // unmatched (never claimed solved).
    let exact = ready(LIVRE, &world(&head(&[NFC, "en cours", NFC])));
    assert_eq!(
        expression(&exact),
        format!("[.records[] | select(.statut == \"{NFC}\")]")
    );
    assert!(
        exact
            .provenance
            .decision
            .as_ref()
            .unwrap()
            .get("spellings")
            .is_none()
    );
}

#[test]
fn a_request_typed_decomposed_matches_the_precomposed_spelling_the_file_holds() {
    let decomposed = LIVRE.replace(NFC, NFD);
    assert!(decomposed.contains(NFD));
    let out = ready(&decomposed, &world(&head(&[NFC, "en cours", NFC])));
    assert_eq!(
        expression(&out),
        format!("[.records[] | select((.statut == \"{NFD}\" or .statut == \"{NFC}\"))]")
    );
}

#[test]
fn a_recorded_plan_keeps_its_bytes_and_is_expanded_again_from_its_fresh_observation() {
    let decomposed = world(&head(&[NFD, "en cours", NFD, NFD]));
    let first = ready(LIVRE, &decomposed);
    let plan = first.provenance.plan.clone().expect("a recorded plan");
    // The record keeps the reader's own reading: the expansion lives in the binding.
    assert!(
        plan["rules"][0]["clauses"][0].get("spellings").is_none(),
        "{plan}"
    );
    assert_eq!(
        plan["rules"][0]["jq"],
        format!("[.records[] | select(.statut == \"{NFC}\")]")
    );
    let replay = |world: Value| {
        compile(
            &CompileRequest::create(LIVRE)
                .with_plan(plan.clone())
                .with_knowledge(world),
        )
        .unwrap()
    };
    let again = replay(decomposed);
    assert_eq!(again.status, CompileStatus::Ready, "{again:#?}");
    assert_eq!(expression(&again), expression(&first));
    // The file now spells it precomposed: the replay is byte-exact again.
    let now = replay(world(&head(&[NFC, "en cours", NFC])));
    assert_eq!(now.status, CompileStatus::Ready, "{now:#?}");
    assert_eq!(
        expression(&now),
        format!("[.records[] | select(.statut == \"{NFC}\")]")
    );
    // A record forged to carry spellings is refused, never replayed without them.
    let mut forged = plan.clone();
    forged["rules"][0]["clauses"][0]["spellings"] = json!([NFD]);
    let out = compile(&CompileRequest::create(LIVRE).with_plan(forged)).unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "recorded_plan" && d.message.contains("cannot be replayed")),
        "{out:#?}"
    );
}

#[test]
fn case_compatibility_forms_and_unseen_spellings_are_not_claimed() {
    // A different case is another text: no case folding.
    let upper = ready(
        LIVRE,
        &world(&head(&["Livr\u{e9}", "en cours", "Livr\u{e9}"])),
    );
    assert_eq!(
        expression(&upper),
        format!("[.records[] | select(.statut == \"{NFC}\")]")
    );
    // A spelling the observer did not find categorical (every value distinct) stays byte-exact.
    let scattered = ready(LIVRE, &world(&head(&[NFD, "en cours", "annulé"])));
    assert_eq!(
        expression(&scattered),
        format!("[.records[] | select(.statut == \"{NFC}\")]")
    );
    // A compatibility form (the « ﬁ » ligature) is no canonical equivalent: no NFKC folding.
    let defini = "Lis ./commandes.csv, garde seulement les lignes dont le statut est défini et écris-les dans ./livrees.csv";
    let ligature = "d\u{e9}\u{fb01}ni";
    let out = ready(defini, &world(&head(&[ligature, "en cours", ligature])));
    assert_eq!(
        expression(&out),
        "[.records[] | select(.statut == \"d\u{e9}fini\")]"
    );
}
