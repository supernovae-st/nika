// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The interpreted request still forbids extra effects and grants no proposal authority.

use super::*;
use crate::cognition::verify_document_presented;

struct ExtraEffect(Mutex<Vec<ChoiceQuestion>>);

impl DecisionSeat for ExtraEffect {
    fn name(&self) -> &'static str {
        "fixture/extra-effect"
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.0.lock().unwrap().push(question.clone());
        let choice = match question.id.as_str() {
            "verify-part-0" => "carried",
            "verify-extra" => "task-extra",
            other => panic!("unexpected decision {other}"),
        };
        Box::pin(async move { Ok(ChoiceAnswer::new(choice, self.name())) })
    }
}

#[tokio::test]
async fn an_extra_effect_remains_a_defect_after_every_program_obligation_is_carried() {
    let source = format!(
        "{}  extra:\n    exec:\n      command: [\"echo\", \"extra\"]\n",
        SOURCE.replace("permits:\n", "permits:\n  exec: [\"echo\"]\n")
    );
    let provider = Author::new([Some(reading()), Some(audit())]);
    let seat = ExtraEffect(Mutex::new(Vec::new()));
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
    assert_eq!(
        provider.calls(),
        2,
        "the candidate must reach the interpreted verifier: {out:#?}"
    );
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let asked = seat.0.lock().unwrap();
    assert_eq!(
        asked.iter().map(|q| q.id.as_str()).collect::<Vec<_>>(),
        ["verify-part-0", "verify-extra"]
    );
    assert!(asked[1].keys().iter().any(|key| key == "task-extra"));
    let attempt = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0];
    assert_eq!(attempt["defects"], json!(["only what the request asks"]));
    assert_eq!(attempt["settled"], false);
    assert_eq!(attempt["rejected"], true);
}

#[tokio::test]
async fn pending_offers_reach_qualification_without_becoming_answers_or_authority() {
    let presentation = json!({"kind":"proposal", "selected":null, "pending_choices":[{
        "id":"greeting-current", "key":"greeting", "question":"Which greeting?",
        "state":"open", "after":[], "free_text":true, "multi_select":false,
        "options":[
            {"key":"hello", "label":"Hello", "recommended":true, "values":[]},
            {"key":"bonjour", "label":"Bonjour", "recommended":false, "values":[]}]}]});
    let request = request();
    let provider = Author::new([Some(reading()), Some(audit())]);
    let seat = Seat::new(false);
    let out = verify_document_presented(
        &request,
        SOURCE,
        (&[], &[]),
        &presentation,
        Cognition {
            provider: Some(&provider),
            seat: Some(&seat),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(request.answers, std::collections::BTreeMap::new());
    assert_eq!(provider.calls(), 2);
    // Interpretation and audit see the human sources, never the author's pending alternatives.
    for request in provider.requests.lock().unwrap().iter() {
        let ContentBlock::Text { text } = &request.messages[1].content[0] else {
            panic!()
        };
        let source: Value = serde_json::from_str(text).unwrap();
        assert!(!text.contains("greeting-current"));
        assert!(source.get("presentation").is_none());
        assert!(source.get("pending_choices").is_none());
    }
    let asked = seat.asked.lock().unwrap();
    let obligation = asked.iter().find(|q| q.id == "verify-part-0").unwrap();
    assert_eq!(obligation.state["presentation"], presentation);
    assert_eq!(obligation.state["answers"], json!({}));
    assert_eq!(obligation.state["request"], INTENT);
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(decision["presentation"], presentation);
    let claim = decision["semantic_verification"][0]["engine"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["claim"] == "audited program obligations")
        .unwrap();
    assert_eq!(claim["program_obligations"], json!([REQUIREMENT]));
    assert_eq!(claim["preparation_fulfillment"], false);
    assert_eq!(claim["authority"], false);
}
