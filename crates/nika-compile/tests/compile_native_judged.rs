// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A native candidate is judged against the whole request before READY (E39 C3, C11 live). The
//! seat writes the workflow itself (the sketch door's seat its tasks and program holes), so no
//! law of the core reads its programs: the static laws (parser, Check, fidelity) admitted a
//! technically valid program that keeps the wrong rows, and it was READY with no semantic
//! judgment. The same whole-request judge COLD uses now reads the candidate's actual final bytes
//! (the request as compiled and as first stated, its answers, the grounded reference) through
//! the journaled authoring call, under the same caps: a candidate the judge finds unfaithful, or
//! cannot settle, is never READY, and its record is withdrawn so no answer round replays it.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode};
use nika_compile_cognition::compile_with_provider;
use nika_kernel::ai::provider::{InferRequest, InferResponse, ProviderError, ProviderInferDyn};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;

mod common;
use common::{Judged, Rotating};

const INTENT: &str = "read ./data/sales.csv, order the rows by amount from highest to lowest and keep the first 2, write them to ./out/top.json";

fn policy(native: NativeMode) -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(native)
        .with_repairs(1)
}

/// A native candidate for [`INTENT`] whose ranking runs `order` over the amounts.
fn candidate(order: &str) -> String {
    format!(
        r#"nika: top-two
model: mock/echo
const:
  source_path: ./data/sales.csv
  output_path: ./out/top.json
permits:
  tools: ["nika:read", "nika:convert", "nika:jq", "nika:write"]
  fs:
    read: ["./data/sales.csv"]
    write: ["./out/top.json"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: {{ path: "${{{{ const.source_path }}}}" }}
  parse_source:
    with: {{ document: "${{{{ tasks.read_source.output }}}}" }}
    invoke:
      tool: "nika:convert"
      args: {{ input: "${{{{ with.document }}}}", from: csv, to: json }}
  compute:
    with: {{ records: "${{{{ tasks.parse_source.output }}}}" }}
    invoke:
      tool: "nika:jq"
      args:
        input: {{ records: "${{{{ with.records }}}}" }}
        expression: '.records | {order} | .[:2]'
  write_output:
    with: {{ content: "${{{{ tasks.compute.output }}}}" }}
    invoke:
      tool: "nika:write"
      args: {{ path: "${{{{ const.output_path }}}}", content: "${{{{ with.content }}}}", overwrite: true, create_dirs: true }}
"#
    )
}

fn answer(candidate: &str) -> String {
    json!({"candidate": candidate, "questions": [], "gaps": [], "notes": "read, parse, rank, write"})
        .to_string()
}

const HIGHEST_FIRST: &str = "sort_by(.amount | tonumber) | reverse";
const LOWEST_FIRST: &str = "sort_by(.amount | tonumber)";

/// The verifier's calls journaled in the authoring receipt, in call order.
fn judge_calls(out: &CompileOutcome) -> Vec<String> {
    out.provenance
        .authoring
        .as_ref()
        .map(|receipt| receipt.context.as_slice())
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| entry["call"].as_str())
        .filter(|call| call.starts_with("judge_"))
        .map(str::to_owned)
        .collect()
}

fn route(out: &CompileOutcome) -> String {
    out.provenance.decision.as_ref().unwrap()["route"].to_string()
}

/// A judged READY on the authoring seat: one approval, recorded with the authoring provider as
/// judge, its call journaled beside the authoring call under the same caps.
fn assert_judged_ready(out: &CompileOutcome, approvals: u32) {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(approvals, 1, "{out:#?}");
    assert!(
        route(out).contains("verify: judged (authoring_provider)"),
        "{out:#?}"
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let attempt = &decision["semantic_verification"][0];
    assert_eq!(
        attempt["judge"],
        json!({"seat": receipt.model, "kind": "authoring_provider"}),
        "{decision:#}"
    );
    assert_eq!(attempt["usage"]["calls"], json!(1), "{decision:#}");
    assert_eq!(judge_calls(out), ["judge_request"]);
    let judge = receipt
        .context
        .iter()
        .find(|entry| entry["call"] == "judge_request")
        .unwrap();
    let authored = &receipt.context[0];
    assert_eq!(judge["max_output_tokens"], authored["max_output_tokens"]);
    assert_eq!(judge["timeout_ms"], authored["timeout_ms"]);
    assert_eq!(judge["max_output_tokens"], json!(4096), "{receipt:#?}");
}

#[tokio::test]
async fn a_native_candidate_that_keeps_the_wrong_rows_is_never_ready() {
    // The static laws admit it (a valid program over the stated paths and count); the judge
    // finds the order unfaithful and places it.
    let provider = Rotating::new(vec![
        answer(&candidate(LOWEST_FIRST)),
        json!({"choice": "unfaithful"}).to_string(),
        json!({"choice": "part-1"}).to_string(),
    ]);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Only));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    // Withdrawn with its record: no answer round replays the refused candidate.
    assert!(out.provenance.plan.is_none(), "{out:#?}");
    // The native call, then the whole-request judge and its locate question: journaled calls.
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3, "{out:#?}");
    assert_eq!(judge_calls(&out), ["judge_request", "judge_locate"]);
    let told = format!("{:?}", out.diagnostics);
    assert!(told.contains("semantic_verification"), "{told}");
    assert!(
        told.contains("order the rows by amount from highest to lowest and keep the first 2"),
        "{told}"
    );
    assert!(route(&out).contains("verify: not ready"), "{out:#?}");
}

#[tokio::test]
async fn a_faithful_native_candidate_is_ready_once_judged() {
    let provider = Rotating::new(vec![answer(&candidate(HIGHEST_FIRST))]);
    let judged = Judged::approving(&provider);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Only));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    assert_judged_ready(&out, judged.judged.load(Ordering::SeqCst));
    // The provider itself answered only the native call.
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1, "{out:#?}");
    assert!(out.provenance.plan.is_some(), "{out:#?}");
}

#[tokio::test]
async fn a_judge_that_cannot_settle_leaves_the_native_request_incomplete() {
    // The judge's answer is not one of its choices: nothing is settled, nothing is READY.
    let provider = Rotating::new(vec![
        answer(&candidate(HIGHEST_FIRST)),
        "I think it looks fine".to_owned(),
    ]);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Only));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(out.provenance.plan.is_none(), "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2, "{out:#?}");
    let told = format!("{:?}", out.diagnostics);
    assert!(told.contains("could not settle"), "{told}");
    assert!(route(&out).contains("verify: not ready"), "{out:#?}");
}

/// The authoring provider behind the authority a door grants when its caller names none: one
/// request. Every later request is refused locally, before any transport, as the admission
/// layer refuses it.
struct OneRequest(Rotating);

impl ProviderInferDyn for OneRequest {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        if self.0.calls.load(Ordering::SeqCst) >= 1 {
            return Err(ProviderError::AdmissionDenied {
                reason: "the authoring call ceiling of 1 calls is reached".to_owned(),
            });
        }
        self.0.infer(request).await
    }
}

#[tokio::test]
async fn a_judge_the_call_ceiling_refuses_leaves_the_native_request_incomplete() {
    // The native candidate spends the one request; the judge's is refused, never sent.
    let provider = OneRequest(Rotating::new(vec![answer(&candidate(HIGHEST_FIRST))]));
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(NativeMode::Only));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(out.provenance.plan.is_none(), "{out:#?}");
    assert_eq!(provider.0.calls.load(Ordering::SeqCst), 1, "{out:#?}");
    assert_eq!(judge_calls(&out), ["judge_request"]);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let refused = receipt
        .context
        .iter()
        .find(|entry| entry["call"] == "judge_request")
        .unwrap();
    assert_eq!(
        refused["result"]["failure_kind"],
        json!("admission_refused"),
        "{receipt:#?}"
    );
    let told = format!("{:?}", out.diagnostics);
    assert!(told.contains("could not settle"), "{told}");
}

const TICKETS: &str =
    "read ./tickets.json, keep only the open tickets and write them to ./out/open.json";

/// The sketch door's first answer for [`TICKETS`]: read, one program hole, write.
fn sketch() -> String {
    let task = |id: &str, tool: &str, extra: Value| {
        let mut task = json!({"id": id, "verb": "invoke", "purpose": id, "tool": tool});
        for (key, value) in extra.as_object().unwrap() {
            task[key] = value.clone();
        }
        task
    };
    json!({"name": "open-tickets", "tasks": [
        task("read_tickets", "nika:read", json!({"reads": ["./tickets.json"]})),
        task("keep_open", "nika:jq", json!({"with": [{"name": "document", "from": "read_tickets"}]})),
        task("write_open", "nika:write", json!({"writes": ["./out/open.json"], "with": [{"name": "text", "from": "keep_open"}]})),
    ], "questions": [], "gaps": [], "notes": "read, filter, write"})
    .to_string()
}

fn fills(expression: &str) -> String {
    json!({"fills": [{"task": "keep_open", "field": "expression", "value": expression}], "notes": "one hole"})
        .to_string()
}

#[tokio::test]
async fn a_sketch_candidate_that_keeps_the_wrong_rows_is_never_ready() {
    // The compiler emits the document from the sketch and its fills; the laws admit a program
    // that keeps the closed tickets. The judge reads the emitted bytes.
    let provider = Rotating::new(vec![
        sketch(),
        fills("fromjson | map(select(.status != \"open\"))"),
        json!({"choice": "unfaithful"}).to_string(),
        json!({"choice": "part-1"}).to_string(),
    ]);
    let request = CompileRequest::create(TICKETS).with_authoring_policy(policy(NativeMode::Sketch));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(out.provenance.plan.is_none(), "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 4, "{out:#?}");
    assert_eq!(judge_calls(&out), ["judge_request", "judge_locate"]);
    let told = format!("{:?}", out.diagnostics);
    assert!(told.contains("semantic_verification"), "{told}");
    assert!(told.contains("keep only the open tickets"), "{told}");
    assert!(route(&out).contains("verify: not ready"), "{out:#?}");
}

#[tokio::test]
async fn a_faithful_sketch_candidate_is_ready_once_judged() {
    let provider = Rotating::new(vec![
        sketch(),
        fills("fromjson | map(select(.status == \"open\"))"),
    ]);
    let judged = Judged::approving(&provider);
    let request = CompileRequest::create(TICKETS).with_authoring_policy(policy(NativeMode::Sketch));
    let out = compile_with_provider(&request, &judged).await.unwrap();
    assert_judged_ready(&out, judged.judged.load(Ordering::SeqCst));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2, "{out:#?}");
}
