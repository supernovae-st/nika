// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Why the document door ended with no document accepted, stated as it happened: a repair that
//! named only findings already named stops on no progress (no repair count bounds the rounds by
//! default), an explicit repair limit ends on that limit, and a call that fails after a refusal
//! ends on that failure. None of them is a budget the operator never set.

use super::{STALE_INTENT, Scripted, calls, door, policy};
use serde_json::Value;

/// A document that writes the report elsewhere than the request states: the path law refuses it
/// the same way in every round.
const ELSEWHERE: &str = r#"nika: stale-tickets-report
permits:
  fs: { read: ["./in/tickets.json"], write: ["./out/other.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks:
  load:
    invoke: { tool: "nika:read", args: { path: "./in/tickets.json" } }
  keep:
    with: { raw: "${{ tasks.load.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.raw }}", expression: "fromjson | [.[] | select(.age_hours > 48)]" } }
  save:
    with: { content: "${{ tasks.keep.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/other.json", content: "${{ with.content }}" } }
"#;

/// What the outcome says of a budget: the conclusion of an exhausted repair limit.
const BUDGET: &str = "within the repair budget";

/// The compile of the stale-tickets request under `policy`, the author answering `answers`.
async fn concluded(answers: usize, policy: crate::AuthoringPolicy) -> crate::CompileOutcome {
    let author = Scripted::new(vec![door(ELSEWHERE, &[]); answers]);
    let request = crate::CompileRequest::create(STALE_INTENT).with_authoring_policy(policy);
    let cognition = crate::Cognition {
        provider: Some(&author),
        seat: None,
    };
    let out = crate::compile_with_cognition_composed(&request, cognition, None, None)
        .await
        .expect("compiles");
    assert_ne!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    out
}

/// The route the outcome records.
fn route(out: &crate::CompileOutcome) -> Vec<String> {
    (out.provenance.decision.as_ref())
        .and_then(|decision| decision["route"].as_array().cloned())
        .into_iter()
        .flatten()
        .filter_map(|step| step.as_str().map(str::to_owned))
        .collect()
}

/// The messages of the outcome's diagnostics that conclude the native door.
fn concluding(out: &crate::CompileOutcome) -> Vec<String> {
    (out.diagnostics.iter())
        .filter(|d| d.target == "authoring_native")
        .map(|d| d.message.clone())
        .filter(|message| message.starts_with("No candidate passed the checks"))
        .collect()
}

/// With no repair count, a repair that names only the findings already named stops the rounds
/// on no progress: that is the route's last step and the stated cause, never an exhausted budget.
#[tokio::test]
async fn a_repeated_refusal_concludes_on_no_progress_never_on_a_budget() {
    let out = concluded(2, policy()).await;
    assert_eq!(policy().repairs, None);
    assert_eq!(calls(&out), ["document", "document-repair"]);
    let route = route(&out);
    assert_eq!(
        route.last().map(String::as_str),
        Some("native: no progress"),
        "{route:?}"
    );
    assert!(!route.iter().any(|s| s == "native: exhausted"), "{route:?}");
    let said = concluding(&out);
    assert_eq!(said.len(), 1, "{:#?}", out.diagnostics);
    assert!(said[0].contains("no progress"), "{}", said[0]);
    assert!(!said[0].contains(BUDGET), "{}", said[0]);
}

/// An explicit repair limit the operator set ends the rounds on that limit: exhausted, so stated.
#[tokio::test]
async fn an_explicit_repair_limit_concludes_exhausted() {
    let out = concluded(1, policy().with_repairs(0)).await;
    assert_eq!(calls(&out), ["document"]);
    let route = route(&out);
    assert_eq!(
        route.last().map(String::as_str),
        Some("native: exhausted"),
        "{route:?}"
    );
    let said = concluding(&out);
    assert_eq!(said.len(), 1, "{:#?}", out.diagnostics);
    assert!(said[0].contains(BUDGET), "{}", said[0]);
}

/// A call that fails after a refusal ends the rounds on that failure: the route and the cause
/// say so, never an exhausted budget, and the refused round stays recorded.
#[tokio::test]
async fn a_failed_call_after_a_refusal_concludes_on_the_failure() {
    let out = concluded(1, policy()).await;
    assert_eq!(calls(&out), ["document", "document-repair"]);
    let route = route(&out);
    assert_eq!(
        route.last().map(String::as_str),
        Some("native: call failed"),
        "{route:?}"
    );
    assert!(!route.iter().any(|s| s == "native: exhausted"), "{route:?}");
    let said = concluding(&out);
    assert_eq!(said.len(), 1, "{:#?}", out.diagnostics);
    assert!(said[0].contains("call failed"), "{}", said[0]);
    assert!(!said[0].contains(BUDGET), "{}", said[0]);
    let rounds = (out.provenance.decision.as_ref())
        .and_then(|d| d["native"]["rounds"].as_array().cloned())
        .unwrap_or_default();
    let failed = rounds.iter().filter(|r| r["call"] == "failed").count();
    let judged = |r: &&Value| r["diagnostics"].as_array().is_some_and(|d| !d.is_empty());
    let refused = rounds.iter().filter(judged).count();
    assert_eq!((refused, failed), (1, 1), "{rounds:#?}");
}
