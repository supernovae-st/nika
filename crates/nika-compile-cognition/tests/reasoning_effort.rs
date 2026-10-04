// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An explicit reasoning effort through every seat the compiler asks (R4 B16). A capturing
//! double records each request's output cap and level and answers with the wire read-back it is
//! handed: hermetic test evidence of what cognition asks and records, never a wire capture.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Mutex;
use std::time::Duration;

use nika_compile::{
    AuthoringPolicy, AuthoringReasoning, CompileOutcome, CompileRequest, NativeMode,
};
use nika_compile_cognition::compile_with_provider;
use nika_compile_cognition::decide::{ChoiceOption, ChoiceQuestion, DecisionSeat, ProviderChoice};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ReasoningEffort,
    ReasoningWire, StopReason, TokenUsage,
};
use serde_json::{Value, json};

/// A clause the deterministic reader cannot consume ("harmonise le ton") forces COLD.
const INTENT: &str = "Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse. Demande un accord humain avant le remboursement.";

/// A plan for [`INTENT`] whose first evidence is `first` (not verbatim: a repair call follows).
fn plan(first: &str) -> String {
    json!({"steps":[{"op":"lookup","detail":"le client","evidence":first},{"op":"classify","detail":"le problème","evidence":"classe le problème"},{"op":"draft","detail":"la réponse","evidence":"harmonise le ton de la réponse"}],
           "effects":[{"verb":"refund","target":"le remboursement","policy":"human_first","evidence":"Demande un accord humain avant le remboursement"}],
           "obligations":[],"constraints":[],"unknowns":[]})
    .to_string()
}

/// Test double: records each request's cap and level, answers `text` with `wire` as the read-back.
struct Capture {
    text: String,
    wire: Option<ReasoningWire>,
    asked: Mutex<Vec<(Option<u32>, Option<ReasoningEffort>)>>,
}

impl Capture {
    fn new(text: impl Into<String>, wire: impl Into<Option<ReasoningWire>>) -> Self {
        Self {
            text: text.into(),
            wire: wire.into(),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<(Option<u32>, Option<ReasoningEffort>)> {
        self.asked.lock().unwrap().clone()
    }
}

impl ProviderInferDyn for Capture {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.asked
            .lock()
            .unwrap()
            .push((request.max_tokens, request.reasoning_effort));
        let mut usage = TokenUsage::new(120, 90);
        usage.reasoning_tokens = Some(60);
        let mut response = InferResponse::new(
            vec![ContentBlock::Text {
                text: self.text.clone(),
            }],
            usage,
            StopReason::EndTurn,
        );
        response.reasoning_wire.clone_from(&self.wire);
        response.gen_ai.response_model = Some("deepseek-v4-pro".to_owned());
        Ok(response)
    }
}

fn max_wire() -> ReasoningWire {
    ReasoningWire::new(Some("enabled".into()), Some("max".into()))
}

fn low_wire() -> ReasoningWire {
    ReasoningWire::new(None, Some("low".into()))
}

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("deepseek/deepseek-v4-pro", 4096, Duration::from_secs(5))
}

/// Every call the receipt holds: its role and its reasoning record.
fn records(out: &CompileOutcome) -> Vec<(String, Value)> {
    out.provenance
        .authoring
        .as_ref()
        .map(|receipt| {
            receipt
                .context
                .iter()
                .map(|c| {
                    (
                        c["call"].as_str().unwrap_or_default().to_owned(),
                        c["reasoning"].clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn an_authoring_level_is_its_exact_word_and_nothing_else() {
    for word in AuthoringReasoning::WORDS {
        let level = AuthoringReasoning::parse(word).expect(word);
        assert_eq!(level.word(), word);
    }
    for word in ["medium", "MAX", " max", "", "minimal"] {
        assert_eq!(AuthoringReasoning::parse(word), None, "{word:?}");
    }
}

/// A saved workflow a revision starts from: the native door's remaining route (a creation under
/// `only` is retired and sends nothing).
const BASE: &str = "nika: greeting\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"./out/result.txt\"]\ntasks:\n  save:\n    invoke:\n      tool: \"nika:write\"\n      args:\n        path: \"./out/result.txt\"\n        content: \"hello\"\n";

#[tokio::test]
async fn every_authoring_call_asks_the_configured_level_under_the_policy_cap() {
    let asked_max = (Some(4096), Some(ReasoningEffort::Max));
    let recorded = json!({"configured": "max", "transmitted": {"thinking": "enabled", "effort": "max"},
        "served": "unknown", "reasoning_tokens": 60, "response_model": "deepseek-v4-pro"});
    let create = || CompileRequest::create(INTENT);
    let revise = || {
        CompileRequest::edit(BASE, "Write bonjour instead of hello.")
            .with_original_intent("Write the text hello to ./out/result.txt.")
    };
    // Each seat a request reaches: the private plan and its repair, the plan escalating to the
    // sketch door, the sketch door itself, and the native door a revision keeps.
    let cases: [(NativeMode, CompileRequest, String, &[&str]); 4] = [
        (
            NativeMode::Off,
            create(),
            plan("consulte les clients"),
            &["plan", "repair"],
        ),
        (
            NativeMode::Escalate,
            create(),
            "no plan".to_owned(),
            &["plan", "sketch"],
        ),
        (
            NativeMode::Sketch,
            create(),
            "no sketch".to_owned(),
            &["sketch"],
        ),
        (
            NativeMode::Only,
            revise(),
            "no workflow".to_owned(),
            &["native"],
        ),
    ];
    for (native, request, text, roles) in cases {
        let provider = Capture::new(text, max_wire());
        let reasoning = policy()
            .with_native(native)
            .with_reasoning(AuthoringReasoning::Max);
        let request = request.with_authoring_policy(reasoning);
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert!(!provider.asked().is_empty(), "{native:?}: a seat was asked");
        let asked = provider.asked();
        assert!(
            asked.iter().all(|a| *a == asked_max),
            "{native:?}: {asked:?}"
        );
        let records = records(&out);
        assert_eq!(
            records.len(),
            asked.len(),
            "{native:?}: one record per request"
        );
        for role in roles {
            assert!(
                records.iter().any(|(call, _)| call == role),
                "{native:?}: {role} {records:?}"
            );
        }
        for (call, record) in &records {
            assert_eq!(record, &recorded, "{native:?} {call}");
        }
    }
}

#[tokio::test]
async fn without_a_level_every_call_keeps_its_route_default_and_the_receipt_says_so() {
    let provider = Capture::new(plan("consulte le client"), low_wire());
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy());
    let out = compile_with_provider(&request, &provider).await.unwrap();
    let asked = provider.asked();
    assert!(!asked.is_empty());
    assert!(asked.iter().all(|a| *a == (Some(4096), None)), "{asked:?}");
    for (call, record) in records(&out) {
        assert_eq!(record["configured"], Value::Null, "{call}");
        assert_eq!(
            record["transmitted"],
            json!({"thinking": null, "effort": "low"}),
            "{call}: the route's own default stays visible"
        );
    }
}

#[tokio::test]
async fn a_configured_level_the_wire_never_confirmed_is_recorded_unobserved() {
    let provider = Capture::new(plan("consulte le client"), None);
    let reasoning = policy().with_reasoning(AuthoringReasoning::Max);
    let request = CompileRequest::create(INTENT).with_authoring_policy(reasoning);
    let out = compile_with_provider(&request, &provider).await.unwrap();
    let records = records(&out);
    assert!(!records.is_empty());
    for (call, record) in records {
        assert_eq!(record["configured"], "max", "{call}");
        assert_eq!(
            record["transmitted"], "unobserved",
            "{call}: never the configured level"
        );
        assert_eq!(record["served"], "unknown", "{call}");
    }
}

#[tokio::test]
async fn the_decision_call_asks_the_level_under_the_declared_cap_or_keeps_its_own() {
    let question = ChoiceQuestion::new(
        "q1",
        "pick the option",
        json!({}),
        vec![ChoiceOption::new("a", "the a option")],
    );
    let provider = Capture::new(r#"{"choice":"a"}"#, max_wire());
    let seat = ProviderChoice::new(
        &provider,
        "deepseek/deepseek-v4-pro",
        Duration::from_secs(5),
    )
    .with_reasoning(AuthoringReasoning::Max, 4096);
    let answer = seat.choose(&question).await.unwrap();
    assert_eq!(provider.asked(), [(Some(4096), Some(ReasoningEffort::Max))]);
    let record = answer.reasoning.expect("recorded");
    assert_eq!(record["configured"], "max");
    assert_eq!(
        record["transmitted"],
        json!({"thinking": "enabled", "effort": "max"})
    );

    let legacy = Capture::new(r#"{"choice":"a"}"#, low_wire());
    let seat = ProviderChoice::new(&legacy, "deepseek/deepseek-v4-pro", Duration::from_secs(5));
    let answer = seat.choose(&question).await.unwrap();
    assert_eq!(legacy.asked(), [(Some(256), None)], "compatibility");
    let record = answer.reasoning.expect("recorded");
    assert_eq!(record["configured"], Value::Null);
    assert_eq!(
        record["transmitted"],
        json!({"thinking": null, "effort": "low"})
    );
}

#[tokio::test]
async fn stated_money_still_closes_every_seat_whatever_the_level() {
    for text in [
        format!("{INTENT} Budget: 0 USD."),
        format!("{INTENT} Budget: 2 USD."),
        format!("{INTENT} Budget: 2 USD. Budget: 3 USD."),
    ] {
        let provider = Capture::new(plan("consulte le client"), max_wire());
        let reasoning = policy().with_reasoning(AuthoringReasoning::Max);
        let request = CompileRequest::create(text.as_str())
            .with_authoring_policy(reasoning)
            .with_stated_money();
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert!(provider.asked().is_empty(), "{text}: {out:#?}");
    }
    // The control: the same words with no money stated reach the seat, at the level asked.
    let provider = Capture::new(plan("consulte le client"), max_wire());
    let reasoning = policy().with_reasoning(AuthoringReasoning::Max);
    let words = format!("{INTENT} Budget: 0 USD.");
    let request = CompileRequest::create(words.as_str()).with_authoring_policy(reasoning);
    compile_with_provider(&request, &provider).await.unwrap();
    let asked = provider.asked();
    assert!(!asked.is_empty());
    assert!(asked.iter().all(|a| a.1 == Some(ReasoningEffort::Max)));
}
