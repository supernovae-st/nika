// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The composer: the finite candidate set, its feasibility verdicts, the seat's closed
//! choice, and the anchoring laws over a proposal's evidence. All seats are injected
//! hermetic doubles. Split from `compile_cognition.rs` at the file-LOC cap.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    Cognition, CompileRequest, CompileStatus, Strategy, compile_with_cognition,
    compile_with_provider, decide::NONE_OPTION, outcome_document,
};
use serde_json::{Value, json};

mod common;
use common::{
    ChoosePlan, INTENT, Provider, Rotating, candidates, disagreeing_provider, keys, plan, policy,
    request, route,
};
use std::sync::{Mutex, atomic::Ordering};

// ── The composer: a finite candidate set, deterministic feasibility, one seat call at most ──
//
// The coordinated drafting sentence is not explicit under the strict contract and forces
// COLD; "Fetch <url>" is an explicit literal step of the deterministic reading, so a
// proposal that drops it drops a recognized operation AND the only URL literal of the
// request.
const FETCH_INTENT: &str =
    "Fetch https://example.com/pricing. Harmonize the tone and prepare a summary.";

fn fetch_plan() -> Value {
    json!({"steps":[
        {"op":"fetch","detail":"https://example.com/pricing","evidence":"Fetch https://example.com/pricing"},
        {"op":"draft","detail":"the tone","evidence":"Harmonize the tone"},
        {"op":"draft","detail":"a summary","evidence":"prepare a summary"}],
        "effects":[],"obligations":[],"constraints":[],"unknowns":[]})
}
/// Drops the fetch of the literal URL: infeasible, never offered.
fn dropped_fetch_plan() -> Value {
    let mut p = fetch_plan();
    p["steps"].as_array_mut().unwrap().remove(0);
    p
}
/// Reads "prepare a summary" as a classification instead of a draft: a distinct admissible
/// reading (one clause is one step: a classify added beside the draft would be folded).
fn classified_fetch_plan() -> Value {
    let mut p = fetch_plan();
    p["steps"][2] = json!({"op":"classify","detail":"the page","evidence":"prepare a summary"});
    p
}
fn rotating(plans: &[Value]) -> Rotating {
    Rotating::new(plans.iter().map(Value::to_string).collect())
}

#[tokio::test]
async fn compose_records_every_distinct_candidate_and_the_seat_picks_among_feasible_ones() {
    // (a) two distinct admissible plans, a seat choosing the second: both recorded feasible,
    // the chosen one assembled.
    let provider = disagreeing_provider();
    let seat = ChoosePlan {
        choice: "plan-1",
        asked: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(INTENT).with_authoring_policy(policy().with_samples(3));
    let out = compile_with_cognition(
        &req,
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    let doc = outcome_document(&out);
    let listed = candidates(&doc);
    assert_eq!(listed.len(), 2, "{doc:#}");
    for (k, candidate) in listed.iter().enumerate() {
        assert_eq!(candidate["index"], k, "{candidate}");
        assert_eq!(candidate["feasible"], true, "{candidate}");
        assert_eq!(candidate["reasons"], json!([]), "{candidate}");
        assert_eq!(candidate["source"]["kind"], "cold_sample", "{candidate}");
        assert!(candidate["signature"].is_array(), "{candidate}");
        assert!(candidate["plan"]["operations"].is_array(), "{candidate}");
    }
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 2);
    assert_eq!(doc["provenance"]["decision"]["selected_candidate"], 1);
    assert!(
        listed[1]["plan"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["op"] == "compute"),
        "the chosen candidate is the one with the code rule: {doc:#}"
    );
    assert!(route(&doc).contains("compose: seat"), "{}", route(&doc));
    assert!(keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    // The seat saw exactly the feasible candidates plus NONE, each described by its
    // signature and what differs.
    let asked = seat.asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].keys(), ["plan-0", "plan-1", NONE_OPTION]);
    let with_compute = asked[0].options.iter().find(|o| o.key == "plan-1").unwrap();
    assert!(
        with_compute.description.contains("op:compute"),
        "{}",
        with_compute.description
    );
}

#[tokio::test]
async fn compose_never_offers_an_infeasible_candidate_to_the_seat() {
    // (b) the sample that drops the fetch of the literal URL is recorded infeasible with
    // its reasons and the seat receives only the two feasible readings.
    let provider = rotating(&[fetch_plan(), dropped_fetch_plan(), classified_fetch_plan()]);
    let seat = ChoosePlan {
        choice: "plan-0",
        asked: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(FETCH_INTENT).with_authoring_policy(policy().with_samples(3));
    let out = compile_with_cognition(
        &req,
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
    let doc = outcome_document(&out);
    let listed = candidates(&doc);
    assert_eq!(listed.len(), 3, "{doc:#}");
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 2);
    let infeasible = &listed[1];
    assert_eq!(infeasible["feasible"], false, "{infeasible}");
    assert_eq!(infeasible["source"]["sample"], 1);
    let reasons = infeasible["reasons"].to_string();
    assert!(
        reasons.contains("dropped the recognized operation `fetch`"),
        "{reasons}"
    );
    assert!(reasons.contains("https://example.com/pricing"), "{reasons}");
    let asked = seat.asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].keys(), ["plan-0", "plan-1", NONE_OPTION]);
    for option in &asked[0].options {
        assert!(
            option.key == NONE_OPTION || option.description.contains("op:fetch"),
            "an option without the fetch was offered: {option:?}"
        );
    }
    assert_eq!(doc["provenance"]["decision"]["selected_candidate"], 0);
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    // The URL is bound from the request: only the runtime model is asked.
    assert_eq!(keys(&out), ["model"], "{out:#?}");
}

#[tokio::test]
async fn compose_with_a_single_feasible_candidate_never_calls_the_seat() {
    // (c) one feasible reading beside an infeasible one: no seat call, route "single".
    let provider = rotating(&[fetch_plan(), dropped_fetch_plan()]);
    let seat = ChoosePlan {
        choice: NONE_OPTION,
        asked: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(FETCH_INTENT).with_authoring_policy(policy().with_samples(2));
    let out = compile_with_cognition(
        &req,
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert!(seat.asked.lock().unwrap().is_empty(), "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    let doc = outcome_document(&out);
    assert_eq!(candidates(&doc).len(), 2, "{doc:#}");
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 1);
    assert_eq!(doc["provenance"]["decision"]["selected_candidate"], 0);
    assert!(route(&doc).contains("compose: single"), "{}", route(&doc));
    assert!(
        doc["provenance"]["decision"]
            .get("warm_after_cold")
            .is_none(),
        "{doc:#}"
    );
    assert_eq!(keys(&out), ["model"], "{out:#?}");
}

#[tokio::test]
async fn compose_seat_none_is_a_clarification_with_the_candidates_on_record() {
    // (d) the seat finds none faithful: nothing assembled, the candidates stay recorded.
    let provider = disagreeing_provider();
    let seat = ChoosePlan {
        choice: NONE_OPTION,
        asked: Mutex::new(Vec::new()),
    };
    let req = CompileRequest::create(INTENT).with_authoring_policy(policy().with_samples(3));
    let out = compile_with_cognition(
        &req,
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
    )
    .await
    .unwrap();
    assert!(out.candidate.is_none());
    assert!(out.provenance.strategy.is_none());
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let doc = outcome_document(&out);
    assert_eq!(candidates(&doc).len(), 2, "{doc:#}");
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 2);
    assert!(doc["provenance"]["decision"]["selected_candidate"].is_null());
    assert_eq!(
        doc["provenance"]["decision"]["warm_after_cold"]["choice"],
        NONE_OPTION
    );
    assert!(
        route(&doc).contains("compose: seat none"),
        "{}",
        route(&doc)
    );
}

#[tokio::test]
async fn compose_ranks_deterministically_without_a_seat_and_asks_when_nothing_is_feasible() {
    // Several feasible candidates and no seat: the documented scorer keeps the candidate
    // closest to every accepted sample (support-weighted medoid); a tie keeps the first.
    let provider = rotating(&[fetch_plan(), classified_fetch_plan(), fetch_plan()]);
    let req = CompileRequest::create(FETCH_INTENT).with_authoring_policy(policy().with_samples(3));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold), "{out:#?}");
    let doc = outcome_document(&out);
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 2);
    assert_eq!(doc["provenance"]["decision"]["selected_candidate"], 0);
    assert!(
        route(&doc).contains("compose: deterministic rank"),
        "{}",
        route(&doc)
    );
    // Zero feasible candidates: the reasons are the findings and a human settles it.
    let provider = rotating(&[dropped_fetch_plan()]);
    let req = CompileRequest::create(FETCH_INTENT).with_authoring_policy(policy().with_samples(2));
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(out.candidate.is_none());
    assert!(out.provenance.strategy.is_none(), "{out:#?}");
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        out.diagnostics.iter().any(|d| d
            .message
            .contains("dropped the recognized operation `fetch`")),
        "{out:#?}"
    );
    let doc = outcome_document(&out);
    assert_eq!(candidates(&doc).len(), 1, "{doc:#}");
    assert_eq!(doc["provenance"]["decision"]["feasible_count"], 0);
    assert!(doc["provenance"]["decision"]["selected_candidate"].is_null());
    assert!(
        route(&doc).contains("compose: none feasible"),
        "{}",
        route(&doc)
    );
}

#[tokio::test]
async fn compose_records_pattern_dimensions_without_composing_an_inexpressible_variant() {
    // The recalled candidates may suggest fan-out or fan-in; the assembler expresses one
    // linear chain today, so the dimension is recorded and no variant is composed.
    let provider = Provider::new(plan());
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    let doc = outcome_document(&out);
    let compose = &doc["provenance"]["decision"]["compose"];
    assert_eq!(compose["cap"], 8, "{doc:#}");
    let dimensions = compose["pattern_dimensions"].as_array().unwrap();
    for dimension in dimensions {
        assert!(
            matches!(dimension["dimension"].as_str(), Some("fanout" | "fanin")),
            "{dimension}"
        );
        assert!(
            !dimension["hits"].as_array().unwrap().is_empty(),
            "{dimension}"
        );
        assert_eq!(dimension["expressible"], false, "{dimension}");
        assert!(dimension["fixed_by_request"].is_boolean(), "{dimension}");
    }
    assert!(
        candidates(&doc)
            .iter()
            .all(|c| c["source"]["kind"] == "cold_sample"),
        "{doc:#}"
    );
}

/// A proposal may wrap a line or drop a double space in its evidence; the compiler anchors
/// it to the exact request excerpt instead of rejecting the whole plan. A changed word is
/// still no excerpt at all.
#[tokio::test]
async fn whitespace_folded_evidence_is_anchored_to_the_exact_excerpt() {
    let mut folded = plan();
    folded["steps"][0]["evidence"] = json!("consulte   le\nclient");
    let provider = Provider::new(folded);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    let doc = outcome_document(&out);
    let evidence: Vec<&str> = doc["provenance"]["plan"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["evidence"].as_str())
        .collect();
    assert!(evidence.contains(&"consulte le client"), "{evidence:?}");
    let mut altered = plan();
    altered["steps"][0]["evidence"] = json!("consulte la cliente");
    let provider = Provider::new(altered);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.provenance
            .plan
            .as_ref()
            .is_none_or(|p| p["operations"].as_array().is_none_or(Vec::is_empty)),
        "{out:#?}"
    );
}

/// A provider's strict structured-output mode answers `null` for optional properties
/// (categories, the bypass evidence); the plan is still decoded and assembled.
#[tokio::test]
async fn explicit_nulls_from_a_strict_provider_are_absent_fields() {
    let mut with_nulls = plan();
    with_nulls["steps"][0]["categories"] = Value::Null;
    with_nulls["steps"][1]["categories"] = Value::Null;
    with_nulls["approval_bypass"] = json!({"present": false, "evidence": null});
    with_nulls["obligations"] = json!([]);
    let provider = Provider::new(with_nulls);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    assert!(
        out.diagnostics
            .iter()
            .all(|d| !d.message.contains("not a valid closed semantic plan")),
        "{out:#?}"
    );
    assert!(out.provenance.plan.is_some(), "{out:#?}");
}

/// The second clean-shell gate: a proposal that kept "write a brief of at most 5 bullet
/// points … to ./out/brief.md" as a write and produced no draft was assembled as a copy of
/// the fetched page. Such a candidate is not feasible; the compiler asks instead.
#[tokio::test]
async fn a_proposal_that_keeps_the_write_and_drops_the_draft_is_not_feasible() {
    let intent = "Fetch https://www.rfc-editor.org/rfc/rfc2324.txt and write a brief of at most 5 bullet points to ./out/brief.md explaining what the document specifies.";
    let proposal = json!({
        "steps": [{"op": "fetch", "detail": "https://www.rfc-editor.org/rfc/rfc2324.txt", "evidence": "Fetch https://www.rfc-editor.org/rfc/rfc2324.txt"}],
        "effects": [{"verb": "write", "target": "./out/brief.md", "policy": "automatic", "evidence": "write a brief of at most 5 bullet points to ./out/brief.md explaining what the document specifies"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [{"text": "Fetch https://www.rfc-editor.org/rfc/rfc2324.txt", "role": "operation"}, {"text": "write a brief of at most 5 bullet points to ./out/brief.md explaining what the document specifies.", "role": "effect"}],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(proposal);
    let req = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .answer("model", r#""mock/echo""#);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("names content no step produces")),
        "{out:#?}"
    );
}

/// The reader's refund guard is a word-level backstop; the proposal's accounting settles it.
#[tokio::test]
async fn the_refund_backstop_yields_to_the_proposals_accounting() {
    // A status value in a filter, read as an operation region: no refund effect is missing.
    let intent = "Read ./data/orders.csv, keep only the rows whose status is refunded and whose amount_eur is greater than 100, and write those rows to ./out/kept.csv with the same header.";
    let proposal = json!({
        "steps": [
            {"op": "read", "detail": "./data/orders.csv", "evidence": "Read ./data/orders.csv"},
            {"op": "compute", "detail": "rows whose status is refunded and whose amount_eur is greater than 100", "evidence": "keep only the rows whose status is refunded and whose amount_eur is greater than 100"}
        ],
        "effects": [{"verb": "write", "target": "./out/kept.csv", "policy": "automatic", "evidence": "write those rows to ./out/kept.csv with the same header"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Read ./data/orders.csv,", "role": "operation"},
            {"text": "keep only the rows whose status is refunded and whose amount_eur is greater than 100,", "role": "operation"},
            {"text": "and write those rows to ./out/kept.csv with the same header.", "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(proposal);
    let req = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .answer("model", r#""mock/echo""#);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        out.diagnostics
            .iter()
            .all(|d| !d.message.contains("mentions a refund")),
        "{out:#?}"
    );
    assert!(
        out.provenance
            .plan
            .as_ref()
            .is_some_and(|p| p["unknowns"].as_array().is_some_and(Vec::is_empty)),
        "{out:#?}"
    );
    // A refund effect the proposal carries satisfies the guard outright.
    let intent = "Look up the customer in ./crm/customers.json, draft a polite reply, and ask a human to approve before the refund is posted.";
    let proposal = json!({
        "steps": [
            {"op": "lookup", "detail": "the customer", "evidence": "Look up the customer in ./crm/customers.json"},
            {"op": "draft", "detail": "a polite reply", "evidence": "draft a polite reply"}
        ],
        "effects": [{"verb": "refund", "target": "the refund", "policy": "human_first", "evidence": "ask a human to approve before the refund is posted"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Look up the customer in ./crm/customers.json,", "role": "operation"},
            {"text": "draft a polite reply,", "role": "operation"},
            {"text": "and ask a human to approve before the refund is posted.", "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(proposal);
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &provider,
    )
    .await
    .unwrap();
    assert!(
        out.diagnostics
            .iter()
            .all(|d| !d.message.contains("mentions a refund")),
        "{out:#?}"
    );
}

/// A proposal may abbreviate a long clause with an ellipsis ("Sépare-les en trois fichiers :
/// ./out/a.json ... ./out/c.json"); the excerpt is the contiguous request span from the first
/// fragment to the last. Fragments out of order or absent are still no excerpt.
#[tokio::test]
async fn an_ellipsis_in_the_evidence_names_the_span_it_abbreviates() {
    let mut abbreviated = plan();
    abbreviated["steps"][0]["evidence"] = json!("consulte ... client");
    let provider = Provider::new(abbreviated);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    let doc = outcome_document(&out);
    let evidence: Vec<&str> = doc["provenance"]["plan"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["evidence"].as_str())
        .collect();
    assert!(evidence.contains(&"consulte le client"), "{evidence:?}");
    let mut reversed = plan();
    reversed["steps"][0]["evidence"] = json!("client ... consulte");
    let provider = Provider::new(reversed);
    let out = compile_with_provider(&request(), &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("lacks an exact source excerpt")),
        "{out:#?}"
    );
}

/// The fourth gate: a Spanish inventory rule ("una línea por cada producto cuyo stock sea menor
/// que su minimo … Total urgentes: N") was read as a constraint region and produced nothing;
/// the workflow copied the CSV and reported ready. A producing region without an element is
/// a dropped clause: the compiler asks instead.
#[tokio::test]
async fn a_region_the_proposal_read_but_produced_nothing_for_is_not_understood() {
    let intent = "Lee ./data/inventario.csv y escribe ./out/alerta.txt con una línea por cada producto cuyo stock sea menor que su minimo.";
    let dropped = json!({
        "steps": [{"op": "read", "detail": "./data/inventario.csv", "evidence": "Lee ./data/inventario.csv"}],
        "effects": [{"verb": "write", "target": "./out/alerta.txt", "policy": "automatic", "evidence": "escribe ./out/alerta.txt"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Lee ./data/inventario.csv", "role": "operation"},
            {"text": "y escribe ./out/alerta.txt", "role": "effect"},
            {"text": "con una línea por cada producto cuyo stock sea menor que su minimo.", "role": "constraint"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(dropped);
    let req = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .answer("model", r#""mock/echo""#);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("produced nothing for it")),
        "{out:#?}"
    );
}

/// A prohibition proposed as a computation ("Do not copy more than 10 consecutive words")
/// becomes a constraint that shapes the prompt; no rule question is asked for it.
#[tokio::test]
async fn a_prohibition_proposed_as_a_computation_is_a_constraint() {
    let intent = "Fetch https://example.com/spec and draft ./out/brief.md with 4 bullets in your own words. Do not copy more than 10 consecutive words from the page.";
    let proposal = json!({
        "steps": [
            {"op": "fetch", "detail": "https://example.com/spec", "evidence": "Fetch https://example.com/spec"},
            {"op": "draft", "detail": "./out/brief.md with 4 bullets in your own words", "evidence": "draft ./out/brief.md with 4 bullets in your own words"},
            {"op": "compute", "detail": "more than 10 consecutive words", "evidence": "Do not copy more than 10 consecutive words from the page"}
        ],
        "effects": [{"verb": "write", "target": "./out/brief.md", "policy": "automatic", "evidence": "draft ./out/brief.md with 4 bullets in your own words"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "Fetch https://example.com/spec", "role": "operation"},
            {"text": "and draft ./out/brief.md with 4 bullets in your own words.", "role": "operation"},
            {"text": "Do not copy more than 10 consecutive words from the page.", "role": "constraint"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    let provider = Provider::new(proposal);
    let out = compile_with_provider(
        &CompileRequest::create(intent).with_authoring_policy(policy()),
        &provider,
    )
    .await
    .unwrap();
    assert!(!keys(&out).contains(&"const.rule_expression"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    assert!(
        plan["operations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["op"] != "compute"),
        "{plan:#}"
    );
    assert!(
        plan["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap_or("").starts_with("Do not copy")),
        "{plan:#}"
    );
}
