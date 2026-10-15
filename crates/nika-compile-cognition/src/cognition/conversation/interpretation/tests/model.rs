// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Audited meaning cannot substitute for the existing model-to-result dataflow law.
use super::*;

struct WeightedModel {
    fail: bool,
    asked: Mutex<Vec<ChoiceQuestion>>,
}

impl DecisionSeat for WeightedModel {
    fn name(&self) -> &'static str {
        "fixture/weighted-model"
    }
    fn weighs(&self) -> bool {
        true
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.asked.lock().unwrap().push(question.clone());
        let sink = question.id.starts_with("verify-sink-");
        if sink && self.fail {
            return Box::pin(async {
                Err(crate::decide::DecisionError("fixture offline".to_owned()))
            });
        }
        let choice = if sink {
            "model"
        } else if question.id.starts_with("verify-part-") {
            "carried"
        } else if question.id == "verify-extra" {
            "only_requested"
        } else {
            panic!("unexpected decision {}", question.id)
        };
        let mut answer = ChoiceAnswer::new(choice, self.name());
        answer.probabilities = question
            .keys()
            .into_iter()
            .map(|key| {
                let p = if key == choice { 1.0 } else { 0.0 };
                (key, p)
            })
            .collect();
        Box::pin(async move { Ok(answer) })
    }
}

#[tokio::test]
async fn a_decorative_model_stays_a_defect_after_obligations_are_carried() {
    let source = format!(
        "{}  draft:\n    infer:\n      max_tokens: 64\n      prompt: Write hello\n",
        SOURCE.replace("nika: greeting\n", "nika: greeting\nmodel: fixture/text\n")
    );
    let provider = Author::new([Some(reading()), Some(audit())]);
    let seat = WeightedModel {
        fail: false,
        asked: Mutex::new(Vec::new()),
    };
    let out = verify_document(
        &request(),
        &source,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(provider.calls(), 2, "{out:#?}");
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let attempt = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0];
    assert_eq!(
        attempt["model_parts"][0]["found"],
        json!({"kind":"decorative","task":"draft"})
    );
    assert_eq!(attempt["rejected"], true);
    assert!(
        attempt["engine"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["claim"] != "audited program obligations")
    );
    let asked = seat.asked.lock().unwrap();
    assert_eq!(
        asked
            .iter()
            .filter(|q| q.id.starts_with("verify-sink-"))
            .count(),
        2
    );
    assert!(asked.iter().any(|q| q.id == "verify-part-0"));
}

#[tokio::test]
async fn failed_sink_calls_stop_before_program_obligations_or_extra_questions() {
    let provider = Author::new([Some(reading()), Some(audit())]);
    let seat = WeightedModel {
        fail: true,
        asked: Mutex::new(Vec::new()),
    };
    let out = verify_document(
        &request(),
        SOURCE,
        (&[], &[]),
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let asked = seat.asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert!(asked.iter().all(|q| q.id.starts_with("verify-sink-")));
    let attempt = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0];
    assert_eq!(attempt["stopped"], true);
    assert_eq!(attempt["unknown"], json!([INTENT]));
    assert_eq!(attempt["settled"], false);
}
