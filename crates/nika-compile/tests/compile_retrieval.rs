// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Measured recall of the compiler's candidate retrieval layer.
//!
//! Two hermetic lists. SEEN holds example intents copied verbatim from the
//! spec's family inventory with their family id — the corpus the index was
//! built from, so this measures the floor, not generalization. UNSEEN holds
//! hand-written paraphrases (English and French) that appear nowhere in the
//! corpus. A twin family (an inventory row that differs only by domain noun or
//! by one adjacent step) counts as accepted and is listed explicitly, so the
//! number never hides behind a lenient match.
//!
//! The floors asserted here are the measured values at the time of writing;
//! the test prints the current numbers so a change is read, not inferred.

// The measured numbers are the deliverable: the test reports them on stderr
// (the arm_emit / differential-proof precedent), then floors them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_macros,
    clippy::print_stderr
)]

use nika_compile::{Hit, retrieve};

/// (intent verbatim from `example_intent`, accepted family ids).
const SEEN: &[(&str, &[&str])] = &[
    ("Summarize https://example.com", &["A01"]),
    ("Compare these three product pages", &["A04"]),
    ("Extract invoice fields from this document", &["A09"]),
    (
        "Answer from this corpus; say unknown if unsupported",
        &["A11"],
    ),
    ("Summarize this meeting transcript", &["A19"]),
    ("Extract action items from this transcript", &["A20"]),
    ("Look up the customer for this ticket", &["B03"]),
    ("Classify this ticket and route it", &["B05"]),
    ("Lookup customer, classify, draft reply", &["B06"]),
    ("Find duplicate tickets", &["B08"]),
    ("Refund this order if eligible", &["B16"]),
    (
        "Handle this enterprise ticket with approval before send",
        &["B18"],
    ),
    ("Enrich this lead", &["C03"]),
    ("Route this lead by territory", &["C06"]),
    ("Send outreach after approval", &["C11"]),
    ("Propose duplicate contact merges", &["C21"]),
    ("Turn this article into social posts", &["D03"]),
    ("SEO metadata for this page", &["D07"]),
    ("Publish this content after approval", &["D16"]),
    ("Changelog from these commits", &["E08"]),
    ("Diff these two dependency snapshots", &["E14"]),
    ("Triage this CI run failure", &["E25"]),
    ("Sum/count these records by key", &["F04"]),
    ("ETL with resume from last state", &["F18"]),
    ("Recover if this optional file is missing", &["F19"]),
    ("Upload this artifact then create the record", &["F20"]),
    ("Categorize these NPS comments", &["G13"]),
    ("Validate exact invoice totals in code", &["H02"]),
    ("Group this task list by owner", &["I13"]),
    ("N independent reviews of this artifact", &["J13"]),
];

/// (paraphrase never present in the corpus, accepted family ids).
const UNSEEN: &[(&str, &[&str])] = &[
    // French
    ("Résume-moi cette page web : https://acme.example", &["A01"]),
    (
        "Classe ces tickets support et envoie-les à la bonne équipe",
        &["B05", "B01"],
    ),
    ("Trouve les doublons dans cette liste de factures", &["H03"]),
    ("Résume la transcription de cette réunion", &["A19"]),
    (
        "Vérifie que les totaux de cette facture sont exacts, en code, pas par un modèle",
        &["H02"],
    ),
    // English
    (
        "Go through these customer reviews and group them by recurring theme",
        &["G03", "D22"],
    ),
    (
        "Before anything gets sent to this enterprise account, I want to sign off on the reply",
        &["B18"],
    ),
    (
        "Pick up the ETL job where it left off last time using the saved state",
        &["F18"],
    ),
    (
        "Upload the build artifact and then register it through the API",
        &["F20"],
    ),
    (
        "Run the same review three times independently and only pass if all agree",
        &["J13", "J14"],
    ),
];

struct Recall {
    at1: usize,
    at5: usize,
    total: usize,
    misses: Vec<String>,
}

fn measure(name: &str, cases: &[(&str, &[&str])]) -> Recall {
    let mut recall = Recall {
        at1: 0,
        at5: 0,
        total: cases.len(),
        misses: Vec::new(),
    };
    for (intent, accepted) in cases {
        let hits: Vec<Hit> = retrieve(intent, 5);
        let rank = hits
            .iter()
            .position(|hit| accepted.contains(&hit.id.as_str()));
        match rank {
            Some(0) => {
                recall.at1 += 1;
                recall.at5 += 1;
            }
            Some(_) => recall.at5 += 1,
            None => {}
        }
        if rank != Some(0) {
            let ids: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
            recall
                .misses
                .push(format!("  {accepted:?} rank={rank:?} «{intent}» → {ids:?}"));
        }
    }
    eprintln!(
        "retrieval recall · {name} · n={} · recall@1={}/{} ({:.2}) · recall@5={}/{} ({:.2})",
        recall.total,
        recall.at1,
        recall.total,
        ratio(recall.at1, recall.total),
        recall.at5,
        recall.total,
        ratio(recall.at5, recall.total),
    );
    for miss in &recall.misses {
        eprintln!("{miss}");
    }
    recall
}

fn ratio(hits: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        // Both counts are tiny; the cast is exact.
        #[allow(clippy::cast_precision_loss)]
        let r = hits as f64 / total as f64;
        r
    }
}

#[test]
fn seen_example_intents_recall_their_family() {
    let recall = measure("seen", SEEN);
    assert_eq!(recall.total, 30);
    assert!(
        recall.at5 >= 30,
        "recall@5 fell below the measured floor: {}/{}",
        recall.at5,
        recall.total
    );
    assert!(
        recall.at1 >= 30,
        "recall@1 fell below the measured floor: {}/{}",
        recall.at1,
        recall.total
    );
}

/// Measured 2026-09-20: recall@1 9/10 (« Trouve les doublons dans cette
/// liste de factures » ranks the task-list dedupe family first, the invoice
/// one second), recall@5 10/10.
#[test]
fn unseen_paraphrases_recall_their_family() {
    let recall = measure("unseen", UNSEEN);
    assert_eq!(recall.total, 10);
    assert!(
        recall.at5 >= 10,
        "recall@5 fell below the measured floor: {}/{}",
        recall.at5,
        recall.total
    );
    assert!(
        recall.at1 >= 9,
        "recall@1 fell below the measured floor: {}/{}",
        recall.at1,
        recall.total
    );
}

#[test]
fn every_seen_intent_names_a_distinct_family() {
    let mut seen = std::collections::BTreeSet::new();
    for (_, accepted) in SEEN {
        assert!(
            seen.insert(accepted[0]),
            "duplicate family in SEEN: {accepted:?}"
        );
    }
}
