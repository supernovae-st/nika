// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_error::traits::NikaErrorCode;

fn response(text: &str, refusal: &serde_json::Value) -> String {
    json!({"choices":[{"message":{"content":text,"refusal":refusal},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":81,"completion_tokens":11,
            "prompt_tokens_details":{"cached_tokens":9},
            "completion_tokens_details":{"reasoning_tokens":4}}})
    .to_string()
}

#[tokio::test]
async fn explicit_refusal_precedes_plain_or_valid_typed_output_and_repair() {
    for reask in [false, true] {
        for typed in [false, true] {
            if reask && !typed {
                continue;
            }
            for text in ["invalid JSON", r#"{"value":7}"#] {
                let mut bodies = Vec::new();
                if reask {
                    bodies.push(response("first invalid", &json!(null)));
                }
                bodies.push(response(text, &json!("synthetic refusal")));
                bodies.push(response(r#"{"value":7}"#, &json!(null)));
                let seam =
                    SeamHttp::with_json(&bodies.iter().map(String::as_str).collect::<Vec<_>>());
                let verb = openai_verb(&seam).with_schema_retry_budget(u8::from(reask));
                let mut input = InferInput::new("synthetic fixture");
                input.max_tokens = Some(11);
                if typed {
                    input.schema = Some(json!({"type":"object"}));
                }
                let err = verb
                    .run(input)
                    .await
                    .expect_err("refusal is not schema output");
                assert!(
                    matches!(err, VerbInferError::ProviderCall { .. }),
                    "{err:?}"
                );
                assert_eq!(err.spec_code(), "NIKA-INFER-001");
                assert!(!err.is_transient());
                let calls = 1 + u64::from(reask);
                let usage = &err.spend().expect("retain the billed response").usage;
                assert_eq!(usage.input_tokens, 81 * calls);
                assert_eq!(usage.output_tokens, 11 * calls);
                assert_eq!(usage.cache_read_tokens, Some(9 * calls));
                assert_eq!(usage.reasoning_tokens, Some(4 * calls));
                assert_eq!(seam.captured().len() as u64, calls);
            }
        }
    }
}

#[tokio::test]
async fn null_or_empty_refusal_preserves_plain_output_and_schema_repair() {
    for refusal in [json!(null), json!("")] {
        for typed in [false, true] {
            let first = response("I cannot answer: synthetic refusal text", &refusal);
            let second = response(r#"{"value":7}"#, &refusal);
            let seam = SeamHttp::with_json(&[&first, &second]);
            let mut input = InferInput::new("synthetic fixture");
            if typed {
                input.schema = Some(json!({"type":"object"}));
            }
            let out = openai_verb(&seam)
                .run(input)
                .await
                .expect("no refusal signal");
            let calls = if typed { 2 } else { 1 };
            assert_eq!(seam.captured().len() as u64, calls);
            assert_eq!(out.usage.input_tokens, 81 * calls);
        }
    }
}
