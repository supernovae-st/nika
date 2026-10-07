// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The parts of one step put to a decision seat together (A1). A provider seated for decisions
//! answers them in ONE request whose answer binds each part by its id; the verdict then takes
//! each answer in its turn, so its questions, records, defects and stops are the ones it makes
//! asking them one by one.

use super::*;
use crate::decide::ProviderChoice;

/// A provider seated for decisions: the whole request rejected, then, asked the three parts in
/// one request, the second missing; asked alone, the second part's task question names `keep`.
struct Batching {
    requests: Mutex<Vec<Value>>,
}

impl ProviderInferDyn for Batching {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let ResponseFormat::JsonSchema(schema) = &request.response_format else {
            panic!("a judge question is a closed choice");
        };
        let keys: Vec<String> = (schema["properties"].as_object().into_iter().flatten())
            .map(|(key, _)| key.clone())
            .collect();
        self.requests.lock().unwrap().push(json!(keys));
        let answer = match keys.as_slice() {
            [choice] if choice == "choice" => {
                let offered = schema["properties"]["choice"]["enum"].to_string();
                if offered.contains("\"faithful\"") {
                    json!({"choice": "unfaithful"})
                } else {
                    json!({"choice": "task-keep"})
                }
            }
            _ => json!({"verify-part-0": "carried", "verify-part-1": "missing",
                "verify-part-2": "carried"}),
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: answer.to_string(),
            }],
            TokenUsage::new(1000, 10),
            StopReason::EndTurn,
        ))
    }
}

/// The three parts of [`ORDERS`] ride one request; the missing part's task question follows in
/// its turn. Three requests in all, where asking one by one takes five; the records, the defect
/// and its note are the sequential ones, and the batch's usage rides its first part only.
#[tokio::test]
async fn a_provider_seat_answers_the_parts_of_one_step_in_one_request() {
    let provider = Batching {
        requests: Mutex::new(Vec::new()),
    };
    let seat = ProviderChoice::new(&provider, SEAT, Duration::from_secs(5), 512);
    let judge = Judge::<Scripted>::Seat(&seat);
    let request = CompileRequest::create(ORDERS);
    let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
    let requests = provider.requests.lock().unwrap().clone();
    let batch = json!(["verify-part-0", "verify-part-1", "verify-part-2"]);
    assert_eq!(
        requests,
        [json!(["choice"]), batch, json!(["choice"])],
        "the whole request, the three parts together, the one task question"
    );
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-point-1",
        "verify-part-2",
    ];
    assert_eq!(ids(&verdict), asked);
    let keep = "the judge points to the task keep";
    let located = found(&[(ORDER_PARTS[1], keep)], &[], &[], &["unfaithful"], &[]);
    assert_eq!(lists(&verdict), located);
    let usage: Vec<Value> = (verdict.records.iter())
        .map(|record| record["input_tokens"].clone())
        .collect();
    let charged = [
        json!(1000),
        json!(1000),
        Value::Null,
        json!(1000),
        Value::Null,
    ];
    assert_eq!(usage, charged, "the batch's usage rides its first part");
}
