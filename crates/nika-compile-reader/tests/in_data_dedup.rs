// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A removal of duplicates scoped by the fields it names, keeping a stated occurrence, is a
//! computation over the rows, never the cross-run dedup obligation, « no second effect for the
//! same incoming identifier » (F2-Q2, E39 PILOT14 V6-DEV4-P2). « Déduplique par customer et
//! `invoice_id`, première occurrence conservée » asked which JSON file keeps the identifiers
//! already processed. A removal that states no kept occurrence or no scope, or that holds a
//! cross-run cue or an event-shaped object, keeps the obligation and its state-file question.
use nika_compile_reader::{lexicon, plan::ObligationKind};

/// V6-DEV4-P2-FR and its EN twin, verbatim (DEV public rows).
const DEV4_P2_FR: &str = "Lis ./data/input.csv. Déduplique par customer et invoice_id, première occurrence conservée ; ensuite exclue les lignes dont status vaut cancelled et compte par customer. Écris un tableau JSON trié par customer, avec exactement customer textuel et count entier par groupe dans ./out/result.json.";
const DEV4_P2_EN: &str = "Read ./data/input.csv. Deduplicate by customer and invoice_id, keeping the first occurrence; then exclude cancelled rows and count by customer. Write a JSON array sorted by customer with exactly textual customer and integer count per group to ./out/result.json.";

/// Whether the reading of `intent` states the cross-run dedup obligation.
fn dedup(intent: &str) -> bool {
    let reading = lexicon::read(intent);
    (reading.plan.obligations.iter()).any(|o| matches!(o.kind, ObligationKind::Dedup))
}

#[test]
fn a_keyed_removal_keeping_an_occurrence_is_no_dedup_obligation() {
    let intents = [
        DEV4_P2_FR,
        DEV4_P2_EN,
        "Read ./data/input.csv. Remove duplicates by invoice_id, keeping the last occurrence. Write the rows to ./out/result.json.",
        "Lis ./data/input.csv. Dédoublonne les lignes par client, en gardant la dernière ligne. Écris-les dans ./out/result.json.",
    ];
    for intent in intents {
        let reading = lexicon::read(intent);
        assert!(!dedup(intent), "{intent}: {:#?}", reading.plan.obligations);
        // The reading is recorded where the review reads it: the clause, under its own role.
        let recorded = reading
            .plan
            .bindings
            .iter()
            .filter(|b| b.role == "in_data_dedup");
        assert_eq!(
            recorded.count(),
            1,
            "{intent}: {:#?}",
            reading.plan.bindings
        );
    }
}

/// The dedup words moved into `assets/dedup_words.txt` read as they did (D1): every former marker
/// and head, in a clause that keeps no stated occurrence, is still the obligation.
#[test]
fn every_former_dedup_marker_and_head_still_reads_the_obligation() {
    let cues = [
        "no second action for the same",
        "pas de seconde action",
        "évite les doublons",
        "évitez les doublons",
        "avoid duplicates",
        "déduplique",
        "dédoublonne",
        "deduplicate",
        "de-duplicate",
        "dedupe",
        "remove duplicates",
        "prevent duplicates",
        "deduplica",
        "elimina i duplicati",
        "rimuovi i duplicati",
        "evita i duplicati",
        "elimina los duplicados",
        "quita los duplicados",
        "evita los duplicados",
        "dédoublonnez",
        "dédupliquez",
    ];
    for cue in cues {
        let intent = format!("Read ./data/input.csv. {cue} the rows by invoice_id.");
        let reading = lexicon::read(&intent);
        assert!(dedup(&intent), "{intent}: {:#?}", reading.plan.obligations);
    }
}

#[test]
fn a_cross_run_unkept_or_unscoped_removal_keeps_the_dedup_obligation() {
    let intents = [
        // A kept occurrence and a scope, beside a cross-run cue.
        "Read ./data/invoices.csv. Deduplicate by invoice_id, keeping the first occurrence, and never process the same invoice twice across runs. Write them to ./out/result.json.",
        "Lis ./data/factures.csv. Déduplique par invoice_id, première occurrence conservée, les factures déjà traitées. Écris-les dans ./out/result.json.",
        // Event-shaped objects (pinned today by the event-fold suites).
        "Quand le bouton Slack de validation est utilisé, retrouve le dossier dans MongoDB, dédoublonne le callback par identifiant et vérifie de nouveau la version courante du dossier avant l'action finale.",
        "Read ./data/events.json. Deduplicate the events by id, keeping the first occurrence. Write them to ./out/result.json.",
        // No kept occurrence: ambiguous, the obligation stays.
        "Lis ./data/input.csv. Déduplique par invoice_id ; écris le résultat dans ./out/result.json.",
        // No scope: nothing names the fields; « par défaut » and its kin name none either.
        "Read ./data/input.csv. Deduplicate, keeping the first occurrence. Write the rows to ./out/result.json.",
        "Lis ./data/input.csv. Déduplique par défaut en gardant la première. Écris le résultat dans ./out/result.json.",
        "Read ./data/input.csv. Deduplicate by default, keeping the first occurrence. Write the rows to ./out/result.json.",
    ];
    for intent in intents {
        let reading = lexicon::read(intent);
        assert!(dedup(intent), "{intent}: {:#?}", reading.plan.obligations);
    }
}
