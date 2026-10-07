// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A computation the reader states in pieces: the contract types each piece whole (a filter,
//! an order, a projection, a column computed from the row) or leaves the computation
//! unsupported, keeps every untyped word as an explicit obligation, and judges a run against
//! it before any candidate exists. The expected rows below are written by hand from the
//! request, never by evaluating the contract under test.

use std::collections::BTreeMap;

use nika_compile_reader::rules::Comparator;

use super::{
    Contract, Outcome, Presence, ReadBack, Requirement, Run, RunEnd, contract_of_request, judge,
};
use crate::behavior::judge_tests::{budget, run, usage};
use crate::behavior::{Consumed, Coverage};

const REORDER: &str = "À partir de inventory.json, prépare un rapport de réapprovisionnement : \
    conserve uniquement les articles dont stock est strictement inférieur à 8, trie-les par sku \
    croissant, et garde exactement les champs sku, stock et reorder_qty, avec reorder_qty égal à \
    12 moins stock. Écris ce tableau JSON dans ./reorder.json en remplaçant son contenu à chaque \
    exécution et expose le même tableau dans la sortie du workflow nommée reorder_report. Ne \
    modifie pas inventory.json.";

const INVENTORY: &str = r#"[{"sku":"PEN-BLACK","stock":7},{"sku":"PAPER-A4","stock":8},{"sku":"LABEL-ROLL","stock":0},{"sku":"TAPE-CLEAR","stock":5},{"sku":"ENVELOPE-C5","stock":4},{"sku":"FOLDER-A4","stock":12}]"#;

/// The rows the request asks for, written by hand.
const EXPECTED: &str = r#"[{"sku":"ENVELOPE-C5","stock":4,"reorder_qty":8},{"sku":"LABEL-ROLL","stock":0,"reorder_qty":12},{"sku":"PEN-BLACK","stock":7,"reorder_qty":5},{"sku":"TAPE-CLEAR","stock":5,"reorder_qty":7}]"#;

fn contract(intent: &str) -> Contract {
    contract_of_request(intent, &BTreeMap::new())
}

fn obligation<'a>(contract: &'a Contract, id: &str) -> &'a super::Obligation {
    contract
        .obligations
        .iter()
        .find(|obligation| obligation.id == id)
        .unwrap_or_else(|| panic!("no obligation {id}: {contract:#?}"))
}

fn computed(contract: &Contract, id: &str) -> String {
    match &obligation(contract, id).requirement {
        Requirement::Computed { pipeline, .. } => pipeline.describe(),
        other => panic!("{id} is not computed: {other:?} in {contract:#?}"),
    }
}

/// Each obligation's outcome on one run of the request's world.
fn outcomes(contract: &Contract, run: Run) -> BTreeMap<String, Outcome> {
    let report = judge(contract, &[run], &mut budget());
    report
        .judged
        .iter()
        .map(|judged| {
            let worst = judged
                .findings
                .iter()
                .map(|finding| finding.outcome)
                .find(|outcome| *outcome == Outcome::Failed)
                .or_else(|| judged.findings.first().map(|finding| finding.outcome));
            (judged.obligation.clone(), worst.unwrap_or(Outcome::NotRun))
        })
        .collect()
}

/// A completed run over the inventory that wrote `output` and left the source unwritten.
fn reorder_run(output: &str) -> Run {
    run("inventory.json", INVENTORY, "./reorder.json", output)
        .with_read_back(ReadBack::unwritten("inventory.json"))
}

#[test]
fn the_reorder_request_is_computed_with_every_untyped_word_kept() {
    let contract = contract(REORDER);
    assert_eq!(
        computed(&contract, "write ./reorder.json"),
        "keep stock < 8, reorder_qty = 12 - stock, sort by sku ascending, columns sku, stock, \
         reorder_qty"
    );
    let heading = obligation(&contract, "operation draft");
    assert!(matches!(heading.requirement, Requirement::Unsupported(_)));
    assert_eq!(
        heading.evidence,
        "prépare un rapport de réapprovisionnement"
    );
    // The named workflow output is never absorbed into the file: it stays unsupported, whole.
    let beyond = obligation(&contract, "beyond write ./reorder.json");
    assert!(matches!(beyond.requirement, Requirement::Unsupported(_)));
    assert!(beyond.target.is_none());
    assert!(
        beyond
            .evidence
            .contains("sortie du workflow nommée reorder_report")
    );
    let kept = obligation(&contract, "keep inventory.json");
    assert_eq!(kept.presence, Presence::Forbidden);
    assert_eq!(kept.requirement, Requirement::PresenceOnly);
    assert_eq!(contract.sources, vec!["inventory.json".to_owned()]);
}

#[test]
fn the_right_rows_fail_nothing_and_certify_nothing() {
    let outcomes = outcomes(&contract(REORDER), reorder_run(EXPECTED));
    assert!(
        !outcomes.values().any(|o| *o == Outcome::Failed),
        "{outcomes:?}"
    );
    // No proof states the write unconditional, and the output stays unverified.
    assert_eq!(
        outcomes["write ./reorder.json"],
        Outcome::Incomplete,
        "{outcomes:?}"
    );
    assert_eq!(
        outcomes["keep inventory.json"],
        Outcome::Passed,
        "{outcomes:?}"
    );
    assert_eq!(
        outcomes["beyond write ./reorder.json"],
        Outcome::Incomplete,
        "{outcomes:?}"
    );
}

#[test]
fn a_decoy_that_states_the_filter_but_does_not_apply_it_fails() {
    let contract = contract(REORDER);
    // `stock <= 8`: the threshold's words are present, its strictness is not consumed.
    let not_strict = r#"[{"sku":"ENVELOPE-C5","stock":4,"reorder_qty":8},{"sku":"LABEL-ROLL","stock":0,"reorder_qty":12},{"sku":"PAPER-A4","stock":8,"reorder_qty":4},{"sku":"PEN-BLACK","stock":7,"reorder_qty":5},{"sku":"TAPE-CLEAR","stock":5,"reorder_qty":7}]"#;
    // Every row, computed and sorted: the filter is never applied.
    let unfiltered = r#"[{"sku":"ENVELOPE-C5","stock":4,"reorder_qty":8},{"sku":"FOLDER-A4","stock":12,"reorder_qty":0},{"sku":"LABEL-ROLL","stock":0,"reorder_qty":12},{"sku":"PAPER-A4","stock":8,"reorder_qty":4},{"sku":"PEN-BLACK","stock":7,"reorder_qty":5},{"sku":"TAPE-CLEAR","stock":5,"reorder_qty":7}]"#;
    // The right rows in file order: the sort is never applied.
    let unsorted = r#"[{"sku":"PEN-BLACK","stock":7,"reorder_qty":5},{"sku":"LABEL-ROLL","stock":0,"reorder_qty":12},{"sku":"TAPE-CLEAR","stock":5,"reorder_qty":7},{"sku":"ENVELOPE-C5","stock":4,"reorder_qty":8}]"#;
    for output in [not_strict, unfiltered, unsorted] {
        let outcomes = outcomes(&contract, reorder_run(output));
        assert_eq!(
            outcomes["write ./reorder.json"],
            Outcome::Failed,
            "{output}: {outcomes:?}"
        );
    }
}

#[test]
fn a_wrong_computed_column_or_projection_fails() {
    let contract = contract(REORDER);
    let added = r#"[{"sku":"ENVELOPE-C5","stock":4,"reorder_qty":16},{"sku":"LABEL-ROLL","stock":0,"reorder_qty":12},{"sku":"PEN-BLACK","stock":7,"reorder_qty":19},{"sku":"TAPE-CLEAR","stock":5,"reorder_qty":17}]"#;
    let missing = r#"[{"sku":"ENVELOPE-C5","stock":4},{"sku":"LABEL-ROLL","stock":0},{"sku":"PEN-BLACK","stock":7},{"sku":"TAPE-CLEAR","stock":5}]"#;
    let extra = r#"[{"sku":"ENVELOPE-C5","stock":4,"reorder_qty":8,"ok":true},{"sku":"LABEL-ROLL","stock":0,"reorder_qty":12,"ok":true},{"sku":"PEN-BLACK","stock":7,"reorder_qty":5,"ok":true},{"sku":"TAPE-CLEAR","stock":5,"reorder_qty":7,"ok":true}]"#;
    for output in [added, missing, extra] {
        let outcomes = outcomes(&contract, reorder_run(output));
        assert_eq!(
            outcomes["write ./reorder.json"],
            Outcome::Failed,
            "{output}: {outcomes:?}"
        );
    }
}

#[test]
fn a_run_that_writes_its_source_fails_the_prohibition() {
    let contract = contract(REORDER);
    let rewritten = Run::new("observed", RunEnd::Completed, usage())
        .with_consumed(Consumed::new(
            "inventory.json",
            INVENTORY,
            Coverage::Complete,
        ))
        .with_read_back(ReadBack::new("./reorder.json", EXPECTED))
        .with_read_back(ReadBack::new("inventory.json", INVENTORY));
    let outcomes = outcomes(&contract, rewritten);
    assert_eq!(
        outcomes["keep inventory.json"],
        Outcome::Failed,
        "{outcomes:?}"
    );
}

#[test]
fn variants_are_read_by_their_own_fields_constants_and_comparisons() {
    let english = "Read ./in/stock.json, keep the items whose qty is less than 10, sort them by \
                   name descending, and keep only the fields name, qty and to_order, with \
                   to_order equal to 10 minus qty. Write them to ./out/low.json.";
    assert_eq!(
        computed(&contract(english), "write ./out/low.json"),
        "keep qty < 10, to_order = 10 - qty, sort by name descending, columns name, qty, \
         to_order"
    );
    let not_strict = "À partir de stock.json, conserve les articles dont quantite est inférieur \
                      ou égal à 3, trie-les par nom, et garde les champs nom et quantite. Écris \
                      ce tableau JSON dans ./bas.json.";
    assert_eq!(
        computed(&contract(not_strict), "write ./bas.json"),
        "keep quantite <= 3, sort by nom ascending, columns nom, quantite"
    );
    let added = "Read ./in/items.json, keep the items whose price is greater than 5, and keep \
                 only the fields id and line_total, with line_total equal to price plus fee. \
                 Write them to ./out/totals.json.";
    assert_eq!(
        computed(&contract(added), "write ./out/totals.json"),
        "keep price > 5, line_total = price + fee, columns id, line_total"
    );
}

#[test]
fn a_negated_comparison_is_never_read_as_its_positive() {
    let english = "Read ./in/stock.json, keep the items whose qty is not less than 10, sort \
                   them by name. Write them to ./out/high.json.";
    let french = "À partir de stock.json, conserve les articles dont quantite n'est pas \
                  inférieur à 3, et garde les champs nom et quantite. Écris ce tableau JSON \
                  dans ./bas.json.";
    for (intent, id) in [
        (english, "write ./out/high.json"),
        (french, "write ./bas.json"),
    ] {
        let read = contract(intent);
        if let Requirement::Computed { pipeline, .. } = &obligation(&read, id).requirement {
            let mut tests = pipeline.steps.iter().flat_map(|step| &step.filter.tests);
            assert!(
                tests.all(|test| test.comparator != Comparator::Lt),
                "{intent}: {read:#?}"
            );
        }
    }
}

#[test]
fn a_piece_the_grammar_does_not_type_leaves_the_computation_unsupported() {
    // A model's work beside typed pieces: the computed rows would omit it.
    let translated = "Read ./in/stock.json, keep the items whose qty is less than 10, \
                      translate their names into French, sort them by name. Write them to \
                      ./out/fr.json.";
    // A product is not a column this component computes.
    let product = "Read ./in/items.json, keep only the fields id and total, with total equal \
                   to price times qty. Write them to ./out/totals.json.";
    // A plain word as the computed column's name: the rule grammar reads no equality on it, so
    // the clause stays unread and the rows stay unsupported, never a partial relation.
    let plain_name = "Read ./in/stock.json, keep the items whose qty is less than 10, sort them \
                      by name descending, and keep only the fields name, qty and shortfall, with \
                      shortfall equal to 10 minus qty. Write them to ./out/low.json.";
    for (intent, id) in [
        (plain_name, "write ./out/low.json"),
        (translated, "write ./out/fr.json"),
        (product, "write ./out/totals.json"),
    ] {
        let contract = contract(intent);
        let write = obligation(&contract, id);
        assert!(
            matches!(write.requirement, Requirement::Unsupported(_)),
            "{intent}: {contract:#?}"
        );
    }
}
