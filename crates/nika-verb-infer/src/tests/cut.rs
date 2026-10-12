// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIKA-INFER-005: an answer the provider stopped at the output cap fails typed. It is never
//! the output and never validated against `schema:`; the cut text and the provider's stop
//! reason ride the record's details. A blank cut answer keeps its earlier failure: NIKA-INFER-004
//! on the text lane, the truncation fast-fail of the schema lane under `schema:`.

use super::*;
use nika_error::traits::NikaErrorCode;

/// The flash shape observed 2026-10-10: 4,096 of 4,096 output tokens, 3,860 of them reasoning,
/// and the visible answer cut mid-sentence.
const FLASH_CUT: &str = r#"{"choices":[{"message":{"content":"The three stories are: first, the"},"finish_reason":"length"}],"usage":{"prompt_tokens":900,"completion_tokens":4096,"completion_tokens_details":{"reasoning_tokens":3860}}}"#;

#[tokio::test]
async fn an_answer_cut_at_the_cap_fails_with_its_partial_text() {
    let seam = SeamHttp::with_json(&[FLASH_CUT]);
    let mut input = InferInput::new("brief me");
    input.max_tokens = Some(4096);
    let err = (openai_verb(&seam).run(input).await).expect_err("a cut answer is not an answer");
    let VerbInferError::AnswerCut {
        partial_output,
        stop_reason,
        detail,
        spend,
        ..
    } = &err
    else {
        panic!("expected AnswerCut, got {err:?}");
    };
    assert_eq!(partial_output, "The three stories are: first, the");
    assert_eq!(stop_reason, "length");
    assert!(detail.contains("the task's max_tokens 4096"), "{detail}");
    assert!(
        detail.contains("4096 output tokens, 3860 of them reasoning"),
        "{detail}"
    );
    assert!(spend.has_signal(), "the cut round-trip is billed");
    assert_eq!(err.spec_code(), "NIKA-INFER-005");
    assert_eq!(err.nika_code(), nika_error::codes::NIKA_436);
    assert!(!err.is_transient(), "the same cap cuts again");
    assert_eq!(
        VerbInferError::details_of(&err),
        Some(json!({
            "partial_output": "The three stories are: first, the",
            "stop_reason": "length",
        }))
    );
    assert!(err.to_string().contains("stop reason `length`"), "{err}");
}

#[tokio::test]
async fn a_cut_structured_answer_is_never_validated_nor_repaired() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"title\": \"Three"},"finish_reason":"length"}],"usage":{"prompt_tokens":9,"completion_tokens":64}}"#,
        r#"{"choices":[{"message":{"content":"{\"title\": \"Three\"}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":8}}"#,
    ]);
    let mut input = InferInput::new("extract the title");
    input.schema = Some(json!({"type": "object", "properties": {"title": {"type": "string"}}}));
    let err = (openai_verb(&seam).run(input).await).expect_err("a cut answer is not an answer");
    assert!(matches!(err, VerbInferError::AnswerCut { .. }), "{err:?}");
    // The cap is the provider's default here: never imputed to the task.
    assert!(
        err.to_string().contains("no max_tokens on the task"),
        "{err}"
    );
    assert_eq!(
        seam.captured().len(),
        1,
        "a repair under the same cap would be cut again"
    );
}

/// Every other answer is untouched: a complete one stays the output, and a cut one that shows
/// nothing stays the empty-answer gate's (`empty_answer_with_reasoning_spend_fails_typed`).
#[tokio::test]
async fn a_complete_answer_is_never_read_as_cut() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"Done."},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":2}}"#,
    ]);
    let out = (openai_verb(&seam).run(InferInput::new("finish"))).await;
    assert!(matches!(out.expect("complete").output, InferValue::Text(ref t) if t == "Done."));
}

/// An answer that parses, and even satisfies `schema:`, is still cut when the provider says it
/// stopped at the output cap: the finish field decides, never the text.
#[tokio::test]
async fn a_cut_answer_that_parses_and_matches_its_schema_is_still_cut() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"items\": [\"a\", \"b\"]}"},"finish_reason":"length"}],"usage":{"prompt_tokens":9,"completion_tokens":64}}"#,
    ]);
    let mut input = InferInput::new("list every item");
    input.schema = Some(json!({"type": "object", "properties": {"items": {"type": "array"}}}));
    let err = (openai_verb(&seam).run(input).await).expect_err("a cut answer is not an answer");
    assert!(matches!(err, VerbInferError::AnswerCut { .. }), "{err:?}");
    assert_eq!(
        VerbInferError::details_of(&err),
        Some(json!({"partial_output": "{\"items\": [\"a\", \"b\"]}", "stop_reason": "length"}))
    );
    assert_eq!(seam.captured().len(), 1, "never validated, never repaired");
}

/// An answer the provider interrupted on its side fails as a provider call (NIKA-INFER-001),
/// transient, in the provider's own word, with no partial output.
#[tokio::test]
async fn an_interrupted_answer_fails_as_a_transient_provider_call() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"The three"},"finish_reason":"insufficient_system_resource"}],"usage":{"prompt_tokens":9,"completion_tokens":3}}"#,
    ]);
    let err = (openai_verb(&seam).run(InferInput::new("brief me")).await)
        .expect_err("an interrupted answer is not an answer");
    assert!(
        matches!(err, VerbInferError::AnswerInterrupted { .. }),
        "{err:?}"
    );
    assert_eq!(err.spec_code(), "NIKA-INFER-001");
    assert_eq!(err.nika_code(), nika_error::codes::NIKA_430);
    assert!(err.is_transient(), "the author's retry: decides");
    assert_eq!(VerbInferError::details_of(&err), None);
    assert!(
        err.to_string().contains("insufficient_system_resource"),
        "{err}"
    );
    assert_eq!(seam.captured().len(), 1);
}
