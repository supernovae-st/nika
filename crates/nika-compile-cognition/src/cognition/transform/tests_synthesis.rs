// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Every unstated computation is synthesized, including the third; the real request
//! authority and program verifier still decide whether its answer can become a rule.

use super::{AuthoringPolicy, Op, Plan, Step, run, synthesize, unstated_computations};
use crate::authority::{Envelope, Seat};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const COLUMNS: [&str; 3] = ["alpha", "beta", "gamma"];

struct Programs {
    asked: Mutex<Vec<String>>,
    invalid_last: bool,
}

impl ProviderInferDyn for Programs {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let text = request
            .messages
            .last()
            .unwrap()
            .content
            .iter()
            .find_map(|block| {
                if let ContentBlock::Text { text } = block {
                    Some(text)
                } else {
                    None
                }
            })
            .unwrap();
        let state: Value = serde_json::from_str(text).unwrap();
        let detail = state["computation"].as_str().unwrap();
        let column = COLUMNS
            .into_iter()
            .find(|column| detail.contains(*column))
            .unwrap();
        self.asked.lock().unwrap().push(column.to_owned());
        let jq = if self.invalid_last && column == "gamma" {
            "env".to_owned()
        } else {
            format!("[.records[].{column}] | reverse")
        };
        let answer = json!({"jq": jq, "columns_read": [column],
            "example_input": [{column: 1}, {column: 2}], "expected_output": [2, 1]});
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: answer.to_string(),
            }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

async fn prepared(
    max_calls: Option<u32>,
    invalid_last: bool,
) -> (Plan, crate::CompileOutcome, Value, Vec<String>) {
    let mut plan = Plan::default();
    for column in COLUMNS {
        let clause = format!("reverse {column} values independently");
        plan.steps
            .push(Step::new(Op::Compute, &clause, &clause, Vec::new()));
    }
    let intent = format!(
        "Read ./rows.json, {}",
        plan.steps
            .iter()
            .map(|step| step.evidence.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let request = crate::CompileRequest::create(&intent).with_knowledge(json!({
        "observed": [{"path": "./rows.json", "state": "observed", "complete": false,
            "kind": "json", "columns": COLUMNS, "common_columns": COLUMNS}]
    }));
    let hints = COLUMNS.map(str::to_owned);
    assert_eq!(unstated_computations(&plan, &hints).len(), 3);
    let authority = Arc::new(max_calls.map_or_else(
        || Envelope::uncapped("test authority"),
        |max| Envelope::new(max, "test authority"),
    ));
    let provider = Seat::new(
        Programs {
            asked: Mutex::new(Vec::new()),
            invalid_last,
        },
        Arc::clone(&authority),
    );
    let policy =
        AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2)).with_repairs(0);
    let mut out = crate::initial();
    assert!(
        synthesize(
            &intent,
            &mut plan,
            &policy,
            &provider,
            &request,
            &[],
            &mut out
        )
        .await
        .is_none()
    );
    let asked = provider.inner().asked.lock().unwrap().clone();
    (plan, out, authority.account(), asked)
}

#[tokio::test]
async fn every_unstated_computation_gets_a_verified_program_past_two() {
    let (plan, out, account, asked) = prepared(None, false).await;
    assert_eq!(asked, COLUMNS);
    assert_eq!(account, json!({"sent": 3, "refused": 0}));
    assert_eq!(plan.rules.len(), 3);
    // New values, distinct for each column: every admitted program runs under the same jq
    // capability policy, with an oracle independent of the seat's proposed example.
    let source = json!({"records": [
        {"alpha": 3, "beta": 7, "gamma": 2},
        {"alpha": 4, "beta": 1, "gamma": 8},
        {"alpha": 9, "beta": 5, "gamma": 6}]});
    for (rule, expected) in
        plan.rules
            .iter()
            .zip([json!([9, 4, 3]), json!([5, 1, 7]), json!([6, 8, 2])])
    {
        assert_eq!(
            run(&rule.verified_program().unwrap().jq, &source).unwrap(),
            expected
        );
    }
    let records = &out.provenance.decision.as_ref().unwrap()["transforms"];
    assert_eq!(records.as_array().unwrap().len(), 3);
    assert!(
        records
            .as_array()
            .unwrap()
            .iter()
            .all(|record| record["accepted"] == true)
    );
    assert_eq!(out.provenance.authoring.unwrap().calls, 3);
}

#[tokio::test]
async fn extra_computations_do_not_expand_an_explicit_request_authority() {
    let (plan, out, account, asked) = prepared(Some(2), false).await;
    assert_eq!(asked, ["alpha", "beta"]);
    assert_eq!(account, json!({"sent": 2, "refused": 1}));
    assert_eq!(plan.rules.len(), 2);
    let records = &out.provenance.decision.as_ref().unwrap()["transforms"];
    assert_eq!(records.as_array().unwrap().len(), 3);
    assert_eq!(records[2]["accepted"], false);
    let receipt = out.provenance.authoring.unwrap();
    assert_eq!(
        receipt.context[2]["result"]["failure_kind"],
        "admission_refused"
    );
}

#[tokio::test]
async fn a_third_program_still_must_pass_the_capability_verifier() {
    let (plan, out, account, asked) = prepared(None, true).await;
    assert_eq!(asked, COLUMNS);
    assert_eq!(account, json!({"sent": 3, "refused": 0}));
    assert_eq!(plan.rules.len(), 2);
    let records = &out.provenance.decision.as_ref().unwrap()["transforms"];
    assert_eq!(records[2]["accepted"], false);
    assert!(records[2]["why"].as_str().unwrap().contains("withheld"));
}
