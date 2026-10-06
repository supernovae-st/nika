// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A removal of duplicates over the rows asks for no cross-run state (F2-Q2, E39 PILOT14
//! V6-DEV4-P2). « Déduplique par customer et `invoice_id`, première occurrence conservée » was
//! read as « no second effect for the same incoming identifier », and the live cold round asked
//! which JSON file keeps the identifiers already processed. The removal is now read on as an
//! operation: a cold round's merged plan holds no obligation and the outcome tells the reading,
//! and at the deterministic door (which stops on this row's other questions first, before and
//! after) the plan holds no obligation and records what it read. A cross-run request still asks
//! for its state file. Its meaning survives: the removal reaches the judge as a pending duty, and
//! a program that keeps every duplicate is never READY behind a judge that refuses it (amendment
//! 1).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use nika_compile_cognition::compile_with_provider;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU32, Ordering};

mod common;
use common::{Judged, Rotating, keys, policy};

const READ_FR: &str = "Lis ./data/input.csv.";
const DEDUP_FR: &str = "Déduplique par customer et invoice_id, première occurrence conservée";
const COUNT_FR: &str =
    "ensuite exclue les lignes dont status vaut cancelled et compte par customer";
const WRITE_FR: &str = "Écris un tableau JSON trié par customer, avec exactement customer textuel et count entier par groupe dans ./out/result.json.";

/// V6-DEV4-P2-FR and its EN twin, verbatim (DEV public rows).
fn dev4_fr() -> String {
    format!("{READ_FR} {DEDUP_FR} ; {COUNT_FR}. {WRITE_FR}")
}
const DEV4_EN: &str = "Read ./data/input.csv. Deduplicate by customer and invoice_id, keeping the first occurrence; then exclude cancelled rows and count by customer. Write a JSON array sorted by customer with exactly textual customer and integer count per group to ./out/result.json.";

/// Whether the recorded plan of `out` holds the dedup obligation.
fn dedup_obligation(out: &CompileOutcome) -> bool {
    let plan = out.provenance.plan.clone().unwrap_or_default();
    let obligations = plan["obligations"].as_array().cloned().unwrap_or_default();
    obligations.iter().any(|o| o["kind"] == "dedup")
}

/// The clauses the recorded plan of `out` holds as read over the rows (amendment 2).
fn in_data(out: &CompileOutcome) -> Vec<String> {
    let plan = out.provenance.plan.clone().unwrap_or_default();
    let bindings = plan["bindings"].as_array().cloned().unwrap_or_default();
    let read = bindings.iter().filter(|b| b["role"] == "in_data_dedup");
    read.filter_map(|b| b["literal"].as_str().map(str::to_owned))
        .collect()
}

/// Whether the outcome tells whoever meant across runs that no state across runs is asked.
fn tells_in_data(out: &CompileOutcome) -> bool {
    (out.diagnostics.iter())
        .any(|d| d.target == "dedup" && d.message.contains("no state across runs is asked"))
}

/// The deterministic door: keyless, these rows stop on their other questions before any state-file
/// question, on the base as after (measured on 21 deduplication requests); what
/// changes is the plan, which holds no obligation and records the reading. The door emits nothing
/// and names the removal unresolved, so no choice is made silently; a round that binds a workflow
/// tells the in-data reading in its finding (the cold round below).
#[test]
fn a_keyed_removal_keeping_an_occurrence_is_no_obligation_at_the_door() {
    for intent in [dev4_fr(), DEV4_EN.to_owned()] {
        let out = compile(&CompileRequest::create(intent.as_str())).unwrap();
        assert!(
            !keys(&out).contains(&"const.state_file"),
            "{intent}: {:#?}",
            out.questions
        );
        assert!(
            !dedup_obligation(&out),
            "{intent}: {:#?}",
            out.provenance.plan
        );
        // The reading is visible: recorded in the plan, the removal named unresolved (amendment 2).
        assert_eq!(
            in_data(&out).len(),
            1,
            "{intent}: {:#?}",
            out.provenance.plan
        );
        let named = out.diagnostics.iter().any(|d| {
            d.message.starts_with("Unresolved clause:")
                && (d.message.contains("Déduplique") || d.message.contains("Deduplicate"))
        });
        assert!(named, "{intent}: {:#?}", out.diagnostics);
        assert!(out.candidate.is_none(), "{intent}: {:#?}", out.candidate);
    }
}

#[test]
fn a_cross_run_removal_still_asks_its_state_file() {
    let intents = [
        "Lis ./data/factures.csv. Déduplique par invoice_id, première occurrence conservée, les factures déjà traitées. Écris-les dans ./out/result.json.",
        "Read ./data/invoices.csv. Deduplicate by invoice_id, keeping the first occurrence, and never process the same invoice twice across runs. Write them to ./out/result.json.",
        "Lis ./data/input.csv. Déduplique par invoice_id ; écris le résultat dans ./out/result.json.",
    ];
    for intent in intents {
        let out = compile(&CompileRequest::create(intent)).unwrap();
        assert!(
            keys(&out).contains(&"const.state_file"),
            "{intent}: {:#?}",
            out.questions
        );
        assert!(in_data(&out).is_empty() && !tells_in_data(&out), "{intent}");
    }
}

/// A judge double over a seat (amendment 1): it approves the whole request, and refuses every
/// clause question whose words hold the removal clause, as a judge reading bytes that keep every
/// duplicate would: missing, and no task performs it (`omitted`, the removal being an operation
/// of its own). It counts the clause questions it refused.
struct RefusesRemoval<'a> {
    inner: &'a Rotating,
    refused: AtomicU32,
}

impl ProviderInferDyn for RefusesRemoval<'_> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            return self.inner.infer(request).await;
        };
        let choices: Vec<String> = (schema["properties"]["choice"]["enum"].as_array())
            .map(|keys| {
                keys.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let about_removal = format!("{request:?}").contains(DEDUP_FR);
        let answer = if choices.iter().any(|k| k == "faithful") {
            "faithful".to_owned()
        } else if about_removal && choices.iter().any(|k| k == "omitted") {
            "omitted".to_owned()
        } else if choices.iter().any(|k| k == "carried") {
            match choices.iter().find(|k| *k != "carried") {
                Some(other) if about_removal => {
                    self.refused.fetch_add(1, Ordering::SeqCst);
                    other.clone()
                }
                _ => "carried".to_owned(),
            }
        } else {
            return self.inner.infer(request).await;
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: json!({"choice": answer}).to_string(),
            }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// The seat's plan: the read, one compute step over the removal and the count, the write.
fn plan_fr() -> Value {
    let computed = format!("{DEDUP_FR} ; {COUNT_FR}");
    json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": READ_FR},
            {"op": "compute", "detail": computed, "evidence": computed,
             "computation": {"present": false}}
        ],
        "effects": [{"verb": "write", "target": WRITE_FR, "policy": "automatic", "evidence": WRITE_FR}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": READ_FR, "role": "operation"},
            {"text": format!("{computed}."), "role": "operation"},
            {"text": WRITE_FR, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    })
}

/// A program that keeps every duplicate: it never removes a repeated customer and `invoice_id`,
/// consistent with its own example.
fn keeps_duplicates() -> Value {
    let jq = ".records | map(select(.status != \"cancelled\")) | group_by(.customer) | map({customer: .[0].customer, count: length}) | sort_by(.customer)";
    json!({"jq": jq, "columns_read": ["customer", "status"],
        "example_input": [
            {"customer": "acme", "invoice_id": "i1", "status": "paid"},
            {"customer": "acme", "invoice_id": "i1", "status": "paid"},
            {"customer": "beta", "invoice_id": "i2", "status": "cancelled"}],
        "expected_output": [{"customer": "acme", "count": 2}]})
}

/// A program that removes the duplicates by customer and `invoice_id`, the first occurrence kept
/// (`unique_by` keeps the first of each stable group), then excludes cancelled rows and counts.
fn removes_duplicates() -> Value {
    let jq = ".records | unique_by([.customer, .invoice_id]) | map(select(.status != \"cancelled\")) | group_by(.customer) | map({customer: .[0].customer, count: length}) | sort_by(.customer)";
    json!({"jq": jq, "columns_read": ["customer", "invoice_id", "status"],
        "example_input": [
            {"customer": "acme", "invoice_id": "i1", "status": "paid"},
            {"customer": "acme", "invoice_id": "i1", "status": "paid"},
            {"customer": "beta", "invoice_id": "i2", "status": "cancelled"},
            {"customer": "acme", "invoice_id": "i3", "status": "paid"}],
        "expected_output": [{"customer": "acme", "count": 2}]})
}

/// The cold compile request of DEV4-P2-FR over the observed input.
fn cold_request() -> CompileRequest {
    let world = json!({"observed": [
        {"path": "./data/input.csv", "state": "observed", "kind": "csv", "complete": false,
         "columns": ["id", "customer", "invoice_id", "status", "amount_cents"]},
        {"path": "./out/result.json", "state": "absent", "complete": false}
    ]});
    CompileRequest::create(dev4_fr())
        .with_authoring_policy(policy())
        .with_knowledge(world)
}

/// The cold compile of DEV4-P2-FR whose seat writes [`keeps_duplicates`], behind the judge that
/// refuses the removal clause: the outcome and how many removal questions the judge refused.
async fn keeping_every_duplicate() -> (CompileOutcome, u32) {
    let seat = Rotating::new(vec![plan_fr().to_string(), keeps_duplicates().to_string()]);
    let judge = RefusesRemoval {
        inner: &seat,
        refused: AtomicU32::new(0),
    };
    let out = compile_with_provider(&cold_request(), &judge)
        .await
        .unwrap();
    (out, judge.refused.load(Ordering::SeqCst))
}

#[tokio::test]
async fn a_program_keeping_every_duplicate_is_never_ready() {
    let (out, _) = keeping_every_duplicate().await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
}

/// A cold round with a seat whose program removes the duplicates and an approving judge: the merged
/// plan holds no obligation and the outcome tells the reading. (This seat's round asks no state
/// file on the base either: the live one asked it for a plan of another shape.)
#[tokio::test]
async fn the_cold_round_holds_no_obligation_and_tells_the_reading() {
    let seat = Rotating::new(vec![
        plan_fr().to_string(),
        removes_duplicates().to_string(),
    ]);
    let out = compile_with_provider(&cold_request(), &Judged::approving(&seat))
        .await
        .unwrap();
    assert!(
        !keys(&out).contains(&"const.state_file"),
        "{:#?}",
        out.questions
    );
    assert!(!dedup_obligation(&out), "{:#?}", out.provenance.plan);
    assert!(tells_in_data(&out), "{:#?}", out.diagnostics);
}

/// The removal reaches the judge as a pending duty on the final bytes (R4 A11, amendment 1): the
/// judge finds it missing, with no task performing it, a defect the COLD door repairs from; the
/// repair writes the same bytes and names the same defect, which is no progress, so the repairs
/// end and the duty stays pending. The in-data reading is told beside it (a role any step
/// consumes hides no clause, D2 check); the judge declined these bytes, so no record replays
/// them.
#[tokio::test]
async fn the_removal_reaches_the_judge_as_a_pending_duty() {
    let (out, refused) = keeping_every_duplicate().await;
    assert!(refused > 0, "{out:#?}");
    let told = format!(
        "`{DEDUP_FR}` is read as a removal of duplicates within the data, by the keys it names, keeping the occurrence it states: no state across runs is asked. If items processed in earlier runs must be skipped, say so."
    );
    let dedup: Vec<&str> = (out.diagnostics.iter())
        .filter(|d| d.target == "dedup")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(dedup, [told.as_str()], "{out:#?}");
    assert!(out.provenance.plan.is_none(), "{:#?}", out.provenance.plan);
    let decision = out.provenance.decision.clone().unwrap_or_default();
    let attempts = decision["semantic_verification"].as_array().cloned();
    let attempts = attempts.unwrap_or_default();
    assert_eq!(attempts.len(), 2, "{decision:#}");
    let removal = format!("{DEDUP_FR} ; {COUNT_FR}");
    let noted = json!([{"defect": removal, "note": "the judge finds no task performing it"}]);
    for attempt in &attempts {
        assert_eq!(attempt["notes"], noted, "{decision:#}");
    }
    // The repair wrote the very bytes the judge declined.
    let digests = (
        &attempts[0]["candidate_sha256"],
        &attempts[1]["candidate_sha256"],
    );
    assert_eq!(digests.0, digests.1, "{decision:#}");
    let route = decision["route"].to_string();
    for step in ["verify: no progress", "verify: doubted, not replayable"] {
        assert!(route.contains(step), "{step}: {route}");
    }
    let ledger = decision["ledger"].as_array().cloned().unwrap_or_default();
    let removal = ledger
        .iter()
        .find(|d| d["evidence"].as_str().is_some_and(|e| e.contains(DEDUP_FR)));
    assert_eq!(
        removal.map(|d| d["state"].clone()),
        Some(json!("pending")),
        "{ledger:#?}"
    );
}
