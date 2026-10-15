// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A candidate-free reading, independently audited by the permitted authoring provider.
//! This is neither a human clarification nor a judgment of any program or effect authority.

use nika_kernel::ai::provider::{Message, ProviderInferDyn, Role};
use serde_json::{Value, json};

use super::super::{call_with_schema, knowledge};
use crate::{AuthoringPolicy, CompileOutcome, CompileRequest};

const INTERPRET: &str = include_str!("interpretation/interpret.md");
const AUDIT: &str = include_str!("interpretation/audit.md");
const SCHEMA: &str = include_str!("interpretation/schema.json");
const AUDIT_SCHEMA: &str = include_str!("interpretation/audit.schema.json");
const REPAIR: &str = include_str!("interpretation/repair.md");
const VERSION: &str = "request-interpretation/2";

mod diagnostics;

/// Exact words and scoped answers only: no candidate, observations or pending author offers.
pub(in crate::cognition) fn input(intent: &str, request: &CompileRequest) -> Value {
    let state = nika_compile_seats::judge::state(intent, request, "");
    let mut spans = vec![json!({"id":"request", "start":0, "end":intent.len(), "text":intent})];
    if let Some(original) = state["original_request"].as_str() {
        spans.push(
            json!({"id":"original_request", "start":0, "end":original.len(), "text":original}),
        );
    }
    if let Some(answers) = state["answers"].as_object() {
        for (key, answer) in answers {
            let text = answer
                .as_str()
                .map_or_else(|| answer.to_string(), str::to_owned);
            spans
                .push(json!({"id":format!("answer:{key}"),"start":0,"end":text.len(),"text":text}));
        }
    }
    json!({"request":state["request"], "original_request":state["original_request"],
        "answers":state["answers"], "spans":spans})
}

/// A protocol or configured model change invalidates the reading before any transport reuse.
fn key(input: &Value, policy: &AuthoringPolicy) -> String {
    knowledge::sha256(
        &json!({"version":VERSION,"input":input,"model":policy.model,
        "reasoning":crate::cognition::reasoning_record(policy.reasoning, None),"interpret":INTERPRET,"audit":AUDIT,
        "schema":SCHEMA,"audit_schema":AUDIT_SCHEMA,"repair":REPAIR})
        .to_string(),
    )
}

/// Replayed records are data, not evidence that can carry READY. A verifier may use this
/// candidate-free reading as a proposal, but must audit it anew unless it only repeats a
/// terminal rejection without making a new judgment.
pub(in crate::cognition) fn earlier(
    input: &Value,
    policy: &AuthoringPolicy,
    request: &CompileRequest,
) -> Option<Value> {
    let key = key(input, policy);
    let record = request
        .declined
        .iter()
        .rev()
        .map(|attempt| &attempt["request_interpretation"])
        .find(|record| record["input_sha256"] == key)?;
    // A later rejected or unfinished audit invalidates an older proposal for these words.
    // Only an audited reading is reusable, and it still needs a fresh audit before judging.
    (covered(record) && valid(input, &record["reading"])).then(|| record.clone())
}

/// Only stable semantic inputs enter the verdict identity; call usage stays in its receipt.
pub(in crate::cognition) fn context(record: &Value) -> Value {
    json!({"version":VERSION,"input_sha256":record["input_sha256"],
        "model":record["model"],"valid":record["valid"],"reading":record["reading"],"audit":record["audit"]})
}

/// An accepted independent audit is required in addition to structural source coverage.
pub(in crate::cognition) fn covered(record: &Value) -> bool {
    record["audit"]["status"] == "covered"
        && record["audit"]["defects"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && record["valid"] == true
        && record["reading"]["runtime_obligations"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
}

/// Repair source-bound interpretation defects before asking a program judge. Every changed
/// reading is independently audited; cached readings remain untrusted proposals.
pub(in crate::cognition) async fn read<P: ProviderInferDyn>(
    input: &Value,
    policy: &AuthoringPolicy,
    provider: &P,
    proposed: Option<&Value>,
    out: &mut CompileOutcome,
) -> Value {
    repair::read(input, policy, provider, proposed, out).await
}

async fn attempt<P: ProviderInferDyn>(
    input: &Value,
    policy: &AuthoringPolicy,
    provider: &P,
    proposed: Option<&Value>,
    repair_state: Option<&Value>,
    out: &mut CompileOutcome,
) -> Value {
    let before = journal(out).len();
    let mut record = json!({"version":VERSION,"input_sha256":key(input, policy),
        "model":policy.model,"valid":false});
    let reading = if let Some(reading) = proposed
        .filter(|record| covered(record))
        .map(|record| &record["reading"])
        .filter(|r| valid(input, r))
    {
        Ok(reading.clone())
    } else {
        let instructions = if repair_state.is_some() {
            format!("{INTERPRET}\n{REPAIR}")
        } else {
            INTERPRET.to_owned()
        };
        call(
            policy,
            provider,
            "interpret_request",
            &instructions,
            SCHEMA,
            repair_state.unwrap_or(input),
            out,
        )
        .await
    };
    match reading {
        Err(failure) => record["failure"] = failure,
        Ok(reading) => {
            record["reading"] = reading;
            if let Err(failure) = diagnostics::validate(input, &record["reading"]) {
                record["failure"] = failure;
            } else {
                record["valid"] = json!(true);
                let state = json!({"source":input,"interpretation":record["reading"]});
                match call(
                    policy,
                    provider,
                    "audit_interpretation",
                    AUDIT,
                    AUDIT_SCHEMA,
                    &state,
                    out,
                )
                .await
                {
                    Err(failure) => record["failure"] = failure,
                    Ok(audit) => record_audit(&mut record, audit),
                }
            }
        }
    }
    record["calls"] = json!(&journal(out)[before.min(journal(out).len())..]);
    record
}

fn record_audit(record: &mut Value, audit: Value) {
    if !audit_valid(&audit) {
        record["failure"] = diagnostics::failure(
            "audit_interpretation",
            "invalid_structure",
            "audit",
            "Expected covered with no defects, defect with specific location, kind, evidence and note, or unknown.",
        );
        return;
    }
    record["audit"] = audit;
    if record["audit"]["status"] == "covered"
        && record["reading"]["runtime_obligations"]
            .as_array()
            .is_some_and(Vec::is_empty)
    {
        record["failure"] = diagnostics::failure(
            "interpret_request",
            "empty_runtime",
            "runtime_obligations",
            "No program obligation was extracted; no program claim can be qualified.",
        );
    }
}

fn journal(out: &CompileOutcome) -> &[Value] {
    out.provenance
        .authoring
        .as_ref()
        .map_or(&[], |r| r.context.as_slice())
}

async fn call<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    role: &'static str,
    instructions: &str,
    schema: &str,
    state: &Value,
    out: &mut CompileOutcome,
) -> Result<Value, Value> {
    let schema = serde_json::from_str(schema).map_err(|_| {
        diagnostics::failure(
            role,
            "schema",
            "schema",
            "The built-in response schema is not JSON.",
        )
    })?;
    let messages = vec![
        Message::text(Role::System, instructions),
        Message::text(Role::User, state.to_string()),
    ];
    let response = call_with_schema(policy, provider, role, messages, schema, out)
        .await
        .ok_or_else(|| {
            diagnostics::failure(
                role,
                "call",
                "response",
                "The authoring call returned no response; see its authoring_provider diagnostic.",
            )
        })?;
    let text = crate::decide::answer_text(&response).ok_or_else(|| {
        let (kind, detail) =
            if response.stop_reason == nika_kernel::ai::provider::StopReason::MaxTokens {
                (
                    "output_truncated",
                    "The authoring response reached its output limit before completing.",
                )
            } else {
                (
                    "response_shape",
                    "The authoring response was not one complete final Text block.",
                )
            };
        diagnostics::failure(role, kind, "response", detail)
    })?;
    serde_json::from_str(text).map_err(|error| {
        let mut failure = diagnostics::failure(
            role,
            "invalid_json",
            "response",
            "The authoring response is not valid JSON.",
        );
        failure["line"] = json!(error.line());
        failure["column"] = json!(error.column());
        failure["category"] = json!(format!("{:?}", error.classify()));
        failure
    })
}

/// Validate exact evidence and total reference coverage, independently of the semantic audit.
/// Coverage is necessary, never sufficient: an intact span can still be misinterpreted.
pub(in crate::cognition) fn valid(input: &Value, reading: &Value) -> bool {
    diagnostics::validate(input, reading).is_ok()
}

/// The precise preparation failure, suitable for the author and its durable tool result.
pub(in crate::cognition) fn failure_message(record: Option<&Value>) -> String {
    diagnostics::message(record)
}

fn audit_valid(audit: &Value) -> bool {
    let Some(defects) = audit["defects"].as_array() else {
        return false;
    };
    match audit["status"].as_str() {
        Some("covered") => defects.is_empty(),
        Some("defect") => {
            !defects.is_empty()
                && defects.iter().all(|defect| {
                    ["location", "evidence", "note"].iter().all(|key| {
                        defect[key]
                            .as_str()
                            .is_some_and(|text| !text.trim().is_empty())
                    }) && matches!(
                        defect["kind"].as_str(),
                        Some("missing" | "invented" | "changed" | "scope")
                    )
                })
        }
        Some("unknown") => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;

mod repair;
