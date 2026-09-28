// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The money a host admitted is read before every strategy of the seated ladder (R4 B15 · F3):
//! support, HOT, WARM, COLD and native all read the request with its admitted directives
//! blanked, and an admitted zero opens no seat. Hermetic provider doubles count every call they
//! are sent; a positive control proves the count sees one.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus};
use nika_compile_cognition::compile_with_provider;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

mod common;
use common::{INTENT, Provider, plan, policy};

/// The columns of the frozen B15 fixture (`./data/input.csv`).
const COLUMNS: [&str; 7] = [
    "id",
    "cost",
    "budget",
    "price",
    "plafond",
    "montant",
    "amount_usd",
];

/// Frozen case S08-EN: deterministic work under an explicit zero ceiling.
const ZERO: &str = "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json, budget 0 USD";

/// The request under an authoring policy with every directive the money reader finds admitted,
/// as a money gate that meters its seats admits them.
fn admitted(request: &str) -> CompileRequest {
    let spans = nika_compile::money::directives(request)
        .unwrap()
        .found
        .into_iter()
        .map(|d| d.span)
        .collect();
    CompileRequest::create(request)
        .with_knowledge(common::observed(&[("./data/input.csv", &COLUMNS)]))
        .with_authoring_policy(policy())
        .with_admitted_money(spans)
}

/// A provider that answers the support plan and records what each call was asked.
#[derive(Default)]
struct Recording {
    calls: AtomicU32,
    asked: Mutex<Vec<String>>,
}

impl ProviderInferDyn for Recording {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.asked
            .lock()
            .unwrap()
            .push(format!("{:?}", request.messages));
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: plan().to_string(),
            }],
            TokenUsage::new(120, 90),
            StopReason::EndTurn,
        ))
    }
}

#[tokio::test]
async fn the_count_sees_the_call_a_seat_is_sent() {
    let provider = Provider::new(plan());
    let out = compile_with_provider(&common::request(), &provider)
        .await
        .unwrap();
    assert!(provider.calls.load(Ordering::SeqCst) > 0, "{out:#?}");
}

#[tokio::test]
async fn an_admitted_zero_leaves_deterministic_work_on_hot_with_no_call() {
    let provider = Provider::new(plan());
    let out = compile_with_provider(&admitted(ZERO), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0, "{out:#?}");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["money"]["directives"][0]["text"], "budget 0 USD",
        "{decision:#}"
    );
    assert!(
        decision["route"].to_string().contains("hot"),
        "{decision:#}"
    );
}

#[tokio::test]
async fn an_admitted_zero_opens_no_seat_for_work_hot_cannot_settle() {
    let provider = Provider::new(plan());
    let request = format!("{INTENT} Budget: 0 USD.");
    let out = compile_with_provider(&admitted(&request), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("no request was sent")),
        "{out:#?}"
    );
}

#[tokio::test]
async fn a_metered_positive_ceiling_is_never_read_to_the_seat_as_work() {
    let provider = Recording::default();
    let request = format!("{INTENT} Budget: 2 USD.");
    let out = compile_with_provider(&admitted(&request), &provider)
        .await
        .unwrap();
    let asked = provider.asked.lock().unwrap().clone();
    assert!(!asked.is_empty(), "a metered seat is still asked: {out:#?}");
    assert!(
        asked.iter().all(|a| !a.contains("Budget: 2 USD")),
        "{asked:#?}"
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["money"]["directives"][0]["text"], "Budget: 2 USD",
        "{decision:#}"
    );
}

/// The request as a door that meters no seat sends it: its operator's money stated in words.
fn stated(request: &str) -> CompileRequest {
    CompileRequest::create(request)
        .with_knowledge(common::observed(&[("./data/input.csv", &COLUMNS)]))
        .with_authoring_policy(policy())
        .with_stated_money()
}

#[tokio::test]
async fn a_stated_positive_ceiling_opens_no_unmetered_seat() {
    let provider = Provider::new(plan());
    let out = compile_with_provider(&stated(&format!("{INTENT} Budget: 2 USD.")), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics.iter().any(|d| d.target == "authoring_money"
            && d.message.contains("cannot bind an unpriced authoring seat")),
        "{out:#?}"
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["money"]["directives"][0]["text"], "Budget: 2 USD",
        "{decision:#}"
    );
}

#[tokio::test]
async fn a_stated_zero_keeps_deterministic_work_on_hot_with_no_call() {
    let provider = Provider::new(plan());
    let out = compile_with_provider(&stated(ZERO), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0, "{out:#?}");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "authoring_money"),
        "the named seat stayed closed and the outcome says why: {out:#?}"
    );
}

/// A replacement request is read afresh: its own zero binds (nothing is sent, the named seat
/// stays closed and says why), and its money never changes its business reading — the same
/// outcome as the replacement without it, the money record aside.
#[tokio::test]
async fn a_stated_replacement_is_read_afresh_and_its_own_zero_binds() {
    let replaced = |replacement: &str| {
        stated(INTENT).answer(
            "intent.clarification",
            serde_json::to_string(replacement).unwrap(),
        )
    };
    let provider = Provider::new(plan());
    let out = compile_with_provider(&replaced(ZERO), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0, "{out:#?}");
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["money"]["directives"][0]["text"], "budget 0 USD",
        "{decision:#}"
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "authoring_money"),
        "{out:#?}"
    );
    let unstated = ZERO.trim_end_matches(", budget 0 USD");
    let plain = compile_with_provider(&replaced(unstated), &Provider::new(plan()))
        .await
        .unwrap();
    assert_eq!(out.status, plain.status, "{out:#?}\n{plain:#?}");
    assert_eq!(common::keys(&out), common::keys(&plain), "{out:#?}");
    assert_eq!(out.candidate, plain.candidate);
}
