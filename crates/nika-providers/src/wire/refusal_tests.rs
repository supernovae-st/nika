// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use crate::test_support::{FakeHttp, resolved_with};
use nika_kernel::ai::provider::{ContentBlock, InferRequest, ProviderInferDyn, StopReason};
use serde_json::{Value, json};

#[tokio::test]
async fn openai_refusal_overrides_finish_but_preserves_content_usage_and_raw() {
    for finish in ["stop", "length", "tool_calls"] {
        for refusal in [
            None,
            Some(Value::Null),
            Some(json!("")),
            Some(json!("refused")),
        ] {
            let explicit = refusal.as_ref().and_then(Value::as_str) == Some("refused");
            let mut body = json!({
                "id":"req-refusal", "model":"gpt-4o-mini",
                "choices":[{"message":{"content":"{\"value\":7}","tool_calls":[{
                    "id":"c", "function":{"name":"read","arguments":"{}"}
                }]},"finish_reason":finish}],
                "usage":{"prompt_tokens":81,"completion_tokens":11,
                    "prompt_tokens_details":{"cached_tokens":9},
                    "completion_tokens_details":{"reasoning_tokens":4}}
            });
            if let Some(refusal) = refusal {
                body["choices"][0]["message"]["refusal"] = refusal;
            }
            let http = FakeHttp::with_json(200, &body.to_string());
            let response = resolved_with(&http, "openai", "test-key")
                .infer(InferRequest::new("gpt-4o-mini", vec![]))
                .await
                .expect("successful HTTP response retains billing metadata");
            let expected = if explicit {
                StopReason::ContentFilter
            } else {
                match finish {
                    "length" => StopReason::MaxTokens,
                    "tool_calls" => StopReason::ToolUse,
                    _ => StopReason::EndTurn,
                }
            };
            assert_eq!(response.stop_reason, expected);
            assert_eq!(response.finish_reason_raw.as_deref(), Some(finish));
            assert!(matches!(response.content[0], ContentBlock::Text { .. }));
            assert!(matches!(response.content[1], ContentBlock::ToolUse { .. }));
            assert_eq!(response.usage.input_tokens, 81);
            assert_eq!(response.usage.output_tokens, 11);
            assert_eq!(response.usage.cache_read_tokens, Some(9));
            assert_eq!(response.usage.reasoning_tokens, Some(4));
            assert!(response.usage_reported);
            assert_eq!(response.request_id.as_deref(), Some("req-refusal"));
            assert_eq!(http.captured().len(), 1);
        }
    }
}

#[tokio::test]
async fn anthropic_refusal_preserves_cache_usage_and_raw() {
    for stop in ["refusal", "end_turn", "future_stop"] {
        let body = json!({
            "id":"req-refusal", "model":"claude-haiku-4-5",
            "content":[{"type":"text","text":"refusal-like text"}],
            "stop_reason":stop,
            "usage":{"input_tokens":69,"output_tokens":11,
                "cache_read_input_tokens":9,"cache_creation_input_tokens":3}
        });
        let http = FakeHttp::with_json(200, &body.to_string());
        let response = resolved_with(&http, "anthropic", "test-key")
            .infer(InferRequest::new("claude-haiku-4-5", vec![]))
            .await
            .expect("successful HTTP response retains billing metadata");
        assert_eq!(
            response.stop_reason,
            match stop {
                "refusal" => StopReason::ContentFilter,
                "end_turn" => StopReason::EndTurn,
                _ => StopReason::Unknown(stop.into()),
            }
        );
        assert_eq!(response.finish_reason_raw.as_deref(), Some(stop));
        assert_eq!(response.usage.input_tokens, 81);
        assert_eq!(response.usage.output_tokens, 11);
        assert_eq!(response.usage.cache_read_tokens, Some(9));
        assert_eq!(response.usage.cache_creation_tokens, Some(3));
        assert_eq!(http.captured().len(), 1);
    }
}
