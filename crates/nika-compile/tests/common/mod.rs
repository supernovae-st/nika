// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Helpers the compile suites share: one hermetic generative provider that returns a fixed
//! text and counts its calls, the bounded authoring policy, the question keys of an outcome.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus};
use nika_compile_cognition::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

pub(crate) struct Provider {
    pub(crate) text: String,
    pub(crate) calls: AtomicU32,
}
impl Provider {
    pub(crate) fn new(plan: impl std::fmt::Display) -> Self {
        Self {
            text: plan.to_string(),
            calls: AtomicU32::new(0),
        }
    }
}
impl ProviderInferDyn for Provider {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.max_tokens, Some(1024));
        assert_eq!(request.timeout, Some(Duration::from_secs(2)));
        assert!(request.tools.is_empty());
        assert!(request.temperature.is_none());
        assert!(request.extra.params.is_empty());
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: self.text.clone(),
            }],
            TokenUsage::new(120, 90),
            StopReason::EndTurn,
        ))
    }
}
/// An explicit judge double (R4 A11): the verifier's closed choices (the whole request, a clause)
/// are answered with an approval and counted; every other call goes to the wrapped provider
/// unchanged. A suite that reads the emitted program, not the judgment, opts in by naming it;
/// without it the provider's own text answers the judge, and no model-shaped plan is READY.
pub(crate) struct Judged<'a, P> {
    pub(crate) inner: &'a P,
    pub(crate) judged: AtomicU32,
}
impl<'a, P> Judged<'a, P> {
    pub(crate) fn approving(inner: &'a P) -> Self {
        Self {
            inner,
            judged: AtomicU32::new(0),
        }
    }
}
impl<P: ProviderInferDyn> ProviderInferDyn for Judged<'_, P> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        if let Some(key) = approval(&request) {
            self.judged.fetch_add(1, Ordering::SeqCst);
            return Ok(InferResponse::new(
                vec![ContentBlock::Text {
                    text: json!({"choice": key}).to_string(),
                }],
                TokenUsage::new(1, 1),
                StopReason::EndTurn,
            ));
        }
        self.inner.infer(request).await
    }
}
/// The same explicit judge double over a decision seat (R4 A11): the verifier's questions are
/// approved and counted; every other closed choice goes to the wrapped seat unchanged.
pub(crate) struct JudgedSeat<'a> {
    pub(crate) inner: &'a dyn DecisionSeat,
    pub(crate) judged: AtomicU32,
}
impl<'a> JudgedSeat<'a> {
    pub(crate) fn approving(inner: &'a dyn DecisionSeat) -> Self {
        Self {
            inner,
            judged: AtomicU32::new(0),
        }
    }
}
impl DecisionSeat for JudgedSeat<'_> {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn choose<'b>(&'b self, question: &'b ChoiceQuestion) -> ChoiceFuture<'b> {
        let keys = question.keys();
        let Some(key) = ["faithful", "carried"]
            .into_iter()
            .find(|key| keys.iter().any(|k| k == key))
        else {
            return self.inner.choose(question);
        };
        self.judged.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(ChoiceAnswer::new(key, self.inner.name())) })
    }
}
/// A decision seat that settles no choice of its own: the inner seat of a judge double where a
/// test permits no other decision.
pub(crate) struct NoChoice;
impl DecisionSeat for NoChoice {
    fn name(&self) -> &'static str {
        "test/no-choice"
    }
    fn choose<'b>(&'b self, _: &'b ChoiceQuestion) -> ChoiceFuture<'b> {
        Box::pin(async {
            Err(nika_compile_cognition::decide::DecisionError(
                "this seat settles no choice".to_owned(),
            ))
        })
    }
}
/// The approval a verifier question offers: `faithful` for the whole request, `carried` for a
/// clause; `None` for any other call.
fn approval(request: &InferRequest) -> Option<&'static str> {
    let nika_kernel::ai::provider::ResponseFormat::JsonSchema(schema) = &request.response_format
    else {
        return None;
    };
    let keys = schema["properties"]["choice"]["enum"].as_array()?;
    ["faithful", "carried"]
        .into_iter()
        .find(|key| keys.iter().any(|k| k == key))
}
pub(crate) fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2))
}
pub(crate) fn keys(out: &nika_compile::CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

/// A clause the deterministic reader cannot consume ("harmonise le ton") forces COLD.
pub(crate) const INTENT: &str = "Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse. Demande un accord humain avant le remboursement.";
pub(crate) fn plan() -> Value {
    json!({"steps":[{"op":"lookup","detail":"le client","evidence":"consulte le client"},{"op":"classify","detail":"le problème","evidence":"classe le problème"},{"op":"draft","detail":"la réponse","evidence":"harmonise le ton de la réponse"}],
           "effects":[{"verb":"refund","target":"le remboursement","policy":"human_first","evidence":"Demande un accord humain avant le remboursement"}],
           "obligations":[],"constraints":[],"unknowns":[]})
}
pub(crate) fn request() -> CompileRequest {
    CompileRequest::create(INTENT).with_authoring_policy(policy())
}

/// A provider answering its plans in order, round-robin, counting its calls.
pub(crate) struct Rotating {
    pub(crate) plans: Vec<String>,
    pub(crate) calls: AtomicU32,
}
impl Rotating {
    pub(crate) fn new(plans: Vec<String>) -> Self {
        Self {
            plans,
            calls: AtomicU32::new(0),
        }
    }
}
impl ProviderInferDyn for Rotating {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst) as usize;
        let text = self.plans[index % self.plans.len()].clone();
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(100, 50),
            StopReason::EndTurn,
        ))
    }
}

pub(crate) struct ChoosePlan {
    pub(crate) choice: &'static str,
    pub(crate) asked: Mutex<Vec<ChoiceQuestion>>,
}
impl DecisionSeat for ChoosePlan {
    fn name(&self) -> &'static str {
        "double/plans"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.clone());
            Ok(ChoiceAnswer::new(self.choice, "double-1.0"))
        })
    }
}
pub(crate) fn disagreeing_provider() -> Rotating {
    let mut with_compute = plan();
    with_compute["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"op":"compute","detail":"le problème","evidence":"classe le problème","computation":{"present":true}}));
    Rotating::new(vec![
        plan().to_string(),
        with_compute.to_string(),
        plan().to_string(),
    ])
}
pub(crate) fn candidates(doc: &Value) -> Vec<Value> {
    doc["provenance"]["decision"]["candidates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}
pub(crate) fn route(doc: &Value) -> String {
    doc["provenance"]["decision"]["route"].to_string()
}

/// What the CLI host observes of the stated files (the shape `nika-cli-host`'s observation
/// emits, R4 S1): a CSV's header, a JSON file's keys in every sampled record; a bounded sample,
/// never a complete schema. A request whose source nobody observed grounds no key by its words.
pub(crate) fn observed(files: &[(&str, &[&str])]) -> Value {
    let rows: Vec<Value> = files
        .iter()
        .map(|(path, keys)| {
            let csv = std::path::Path::new(path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
            let mut row = json!({"path": path, "state": "observed", "complete": false,
                "kind": if csv { "csv" } else { "json" }, "columns": keys});
            if !csv {
                row["common_columns"] = json!(keys);
            }
            row
        })
        .collect();
    json!({ "observed": rows })
}

/// The E14 near-miss fixtures, as the CLI observed them.
pub(crate) fn e14_world() -> Value {
    observed(&[
        ("./tickets.json", &["id", "status", "amount", "score"]),
        ("./sales.csv", &["client", "amount", "montant", "status"]),
    ])
}

/// The compute task's expression of a candidate, its number law folded (`short`): empty when
/// the candidate has none.
pub(crate) fn compute(candidate: &str) -> String {
    let doc: Value = serde_yaml_bw::from_str(candidate).unwrap_or(Value::Null);
    short(
        doc["tasks"]["compute"]["invoke"]["args"]["expression"]
            .as_str()
            .unwrap_or_default(),
    )
}

/// The number law's reads as the reader writes them (R4 A5), folded to `(key | num)` and its tests
/// to `(key | isnum)`: a suite pins where a number is read, `nika-compile-reader` pins the law.
pub(crate) fn short(jq: &str) -> String {
    let law = format!(
        "(type == \"number\" and (isinfinite or isnan | not)) or (type == \"string\" and test({}) and (fromjson | isinfinite or isnan | not))",
        json!(nika_compile_reader::text::NUMBER_TEXT)
    );
    let read = format!(" | if {law} then tonumber else error(");
    let close = ", not a number\") end)";
    let mut out = jq.to_owned();
    while let Some(at) = out.find(&read) {
        let start = out[..at].rfind('(').expect("a read opens");
        let end = out[at..].find(close).expect("a read closes");
        let key = out[start + 1..at].to_owned();
        out = format!(
            "{}({key} | num){}",
            &out[..start],
            &out[at + end + close.len()..]
        );
    }
    let test = format!(" | {law})");
    while let Some(at) = out.find(&test) {
        let start = out[..at].rfind('(').expect("a test opens");
        let key = out[start + 1..at].to_owned();
        out = format!(
            "{}({key} | isnum){}",
            &out[..start],
            &out[at + test.len()..]
        );
    }
    out
}

/// An answer round's finish the laws admit, held for that round's judge (R4 A11, step 2): no law
/// reads a native seat's program, so a round that permits no judge is INCOMPLETE on the whole
/// request alone (`decision.pending`: the `intent` the core replayed, at its whole span), with a
/// clean check and the candidate as its preview.
pub(crate) fn held_for_its_judge(out: &CompileOutcome, intent: &str) -> bool {
    let whole = json!([{"clause": intent, "witness": null, "spans": [[0, intent.len()]]}]);
    out.status == CompileStatus::Incomplete
        && out.candidate.is_some()
        && out
            .check_preview
            .as_ref()
            .is_some_and(|preview| preview.report.is_clean())
        && out
            .provenance
            .decision
            .as_ref()
            .is_some_and(|decision| decision["pending"]["open"] == whole)
}
