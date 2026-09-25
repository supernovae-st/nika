// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A stated rule stands for a seat's paraphrased detail (measured on the sealed-v3
//! treatment lane, 2026-09-22: five seeds with the right rule recorded still asked
//! `const.rule_expression`); a verbatim detail keeps the sentence-coverage law.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

/// sv3-13 as the seat read it: one correct typed rule whose detail is a paraphrase in two
/// sentences; the request states the rule in one.
const ROAST: &str = "Hello! Would you mind reading ./roastery/batches.csv (columns batch, bean, weight_kg, roast_level) and computing the total weight_kg for each roast_level? I'd like the result written to ./out/by-roast.csv with exactly two columns, roast_level and total_kg, one row per roast level. Thank you so much!";

fn roast_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./roastery/batches.csv","evidence":"reading ./roastery/batches.csv"},
        {"op":"compute","detail":"Group the rows by roast_level and sum weight_kg into total_kg. Output exactly the two columns roast_level and total_kg, one row per roast level.","evidence":"computing the total weight_kg for each roast_level",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[],"group_by":"roast_level",
                        "aggregations":[{"field":"weight_kg","op":"sum","as":"total_kg","round":""}],
                        "sort_by":"","order":"","columns":["roast_level","total_kg"],"derived":[],"limit":"","renames":[]}}],
        "effects":[{"verb":"write","target":"./out/by-roast.csv","policy":"automatic","evidence":"written to ./out/by-roast.csv"}],
        "obligations":[],"constraints":["exactly two columns, roast_level and total_kg","one row per roast level"],"unknowns":[],
        "regions":[{"text":"Hello! Would you mind","role":"context"},
                   {"text":"reading ./roastery/batches.csv (columns batch, bean, weight_kg, roast_level)","role":"operation"},
                   {"text":"computing the total weight_kg for each roast_level","role":"operation"},
                   {"text":"I'd like the result written to ./out/by-roast.csv with exactly two columns, roast_level and total_kg, one row per roast level.","role":"effect"},
                   {"text":"Thank you so much!","role":"context"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_stated_rule_stands_for_the_seats_paraphrased_detail() {
    let provider = Provider::new(roast_proposal());
    // No language step remains (the grouped total is the write): nothing asks a model.
    let req = CompileRequest::create(ROAST).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !keys(&out).contains(&"const.rule_expression"),
        "the rule the seat stated is the computation; nothing to ask: {out:#?}"
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let source = out.candidate.as_deref().unwrap();
    assert!(source.contains("group_by(.roast_level)"), "{source}");
    assert!(source.contains("total_kg"), "{source}");
}

/// A verbatim detail in two sentences still needs a rule that covers both (the ORDERS
/// false READY of c0c7f8cd): the coverage law stays for the reader's own details.
#[test]
fn a_verbatim_detail_of_two_sentences_still_asks_when_one_rule_covers_one() {
    let intent = "Read ./orders.csv (columns id, status, country). Keep only the rows whose status is shipped. Write the count of those orders per country to ./out/by-country.csv.";
    let out = compile(&CompileRequest::create(intent).answer("model", r#""mock/echo""#)).unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
}
