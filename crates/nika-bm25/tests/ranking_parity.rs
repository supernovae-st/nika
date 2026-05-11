// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Gate 5 MUTATION killer · ranking set-equivalence at threshold.
//!
//! Per ADR-038 + rust-architect 2026-05-12 mitigation · ranking ties
//! confound exact-float-equality mutants. Strategy · pin the **set** of
//! top-K `doc_id` values for canonical fixtures + assert pairwise score-ordering
//! relations (Doc A > Doc B by score). Any arithmetic mutation in the
//! BM25 formula that flips ordering OR removes a doc from the top-K set
//! gets killed.
//!
//! Reference · Manning IIR ch.11 example 11.22-11.23 (best/car/insurance).

#![allow(clippy::expect_used)]

use nika_bm25::{BmIndex, BmParams};
use std::collections::BTreeSet;

fn manning() -> BmIndex {
    let mut idx = BmIndex::new(BmParams::default());
    idx.add_document(1, "auto car insurance");
    idx.add_document(2, "best auto insurance");
    idx.add_document(3, "car insurance best auto");
    idx.add_document(4, "insurance best car");
    idx.finalize();
    idx
}

#[test]
fn manning_top_3_set_equivalence() {
    let idx = manning();
    let top = idx.top_k("best car insurance", 3);
    let ids: BTreeSet<u32> = top.iter().map(|(id, _)| *id).collect();
    // Top-3 MUST include docs 3 + 4 (best matches per Manning fixture).
    // Doc 1 OR doc 2 takes the third slot · neither doc 1 nor doc 2 alone
    // can be excluded from the top-3 (set equivalence).
    assert!(ids.contains(&3), "doc 3 must be in top-3 · got {ids:?}");
    assert!(ids.contains(&4), "doc 4 must be in top-3 · got {ids:?}");
    assert_eq!(top.len(), 3);
}

#[test]
fn manning_pairwise_ordering() {
    let idx = manning();
    let s1 = idx.score("best car insurance", 1).expect("doc 1");
    let s2 = idx.score("best car insurance", 2).expect("doc 2");
    let s3 = idx.score("best car insurance", 3).expect("doc 3");
    let s4 = idx.score("best car insurance", 4).expect("doc 4");

    // All 4 scores are strictly positive (every doc has ≥1 query term).
    assert!(s1 > 0.0, "doc 1 has car+insurance overlap · {s1}");
    assert!(s2 > 0.0, "doc 2 has best+insurance overlap · {s2}");
    assert!(s3 > 0.0, "doc 3 has all 3 terms · {s3}");
    assert!(s4 > 0.0, "doc 4 has all 3 terms · {s4}");

    // Length-norm preference · short doc with all matches beats longer doc.
    // Doc 4 (len=3, 3/3 match) MUST beat doc 3 (len=4, 3/3 match) — same
    // term coverage, doc 4 is denser → higher BM25 score via b=0.75 norm.
    assert!(
        s4 > s3,
        "doc 4 (shorter, all match) > doc 3 (longer, all match) · s4={s4} s3={s3}"
    );

    // Full-coverage docs (3 + 4) MUST beat partial-coverage docs (1 + 2).
    assert!(
        s3 > s1,
        "doc 3 (3/3 match) > doc 1 (2/3 match) · s3={s3} s1={s1}"
    );
    assert!(
        s3 > s2,
        "doc 3 (3/3 match) > doc 2 (2/3 match) · s3={s3} s2={s2}"
    );
    assert!(
        s4 > s1,
        "doc 4 (3/3 match) > doc 1 (2/3 match) · s4={s4} s1={s1}"
    );
    assert!(
        s4 > s2,
        "doc 4 (3/3 match) > doc 2 (2/3 match) · s4={s4} s2={s2}"
    );
}

#[test]
fn idf_rare_term_outranks_common() {
    // Build a corpus where one term appears in 1 doc · another in all 4.
    // Query containing both terms · the doc with ONLY the rare term should
    // outrank the doc with ONLY the common term (IDF dominates).
    let mut idx = BmIndex::new(BmParams::default());
    idx.add_document(1, "rare pad pad"); // has rare · no common
    idx.add_document(2, "common pad pad"); // has common · no rare
    idx.add_document(3, "common pad pad");
    idx.add_document(4, "common pad pad");
    idx.finalize();

    let s_rare = idx.score("rare common", 1).expect("doc 1");
    let s_common = idx.score("rare common", 2).expect("doc 2");
    assert!(
        s_rare > s_common,
        "rare-term doc must outrank common-term doc · s_rare={s_rare} s_common={s_common}"
    );
}

#[test]
fn length_norm_b_parameter_active() {
    // With b > 0 · longer docs are penalized · two docs with same TF but
    // different lengths must score differently.
    let mut idx = BmIndex::new(BmParams::default());
    idx.add_document(1, "target pad"); // len=2 · tf(target)=1
    idx.add_document(2, "target pad pad pad"); // len=4 · tf(target)=1
    idx.finalize();
    let s_short = idx.score("target", 1).expect("doc 1");
    let s_long = idx.score("target", 2).expect("doc 2");
    assert!(
        s_short > s_long,
        "shorter doc must score higher (b=0.75) · s_short={s_short} s_long={s_long}"
    );
}

#[test]
fn k1_saturation_parameter_active() {
    // With k1 > 0 · TF saturates · tf=10 must NOT score 10x tf=1.
    let mut idx = BmIndex::new(BmParams::default());
    idx.add_document(1, "target");
    idx.add_document(
        2,
        "target target target target target target target target target target",
    );
    idx.finalize();
    let s1 = idx.score("target", 1).expect("doc 1");
    let s10 = idx.score("target", 2).expect("doc 2");
    assert!(
        s10 > s1,
        "10x TF must score higher than 1x · s10={s10} s1={s1}"
    );
    let ratio = s10 / s1;
    assert!(
        ratio < 10.0,
        "BM25 saturation must apply · 10x TF should score <10x · ratio={ratio}"
    );
}
