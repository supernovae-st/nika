// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::{ChoiceOption, ChoiceQuestion, answer_text, decoded};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
use serde_json::json;

fn text(value: &str) -> ContentBlock {
    ContentBlock::Text { text: value.into() }
}

fn thinking() -> ContentBlock {
    ContentBlock::Thinking {
        text: r#"{"choice":"none"}"#.into(),
    }
}

fn response(content: Vec<ContentBlock>) -> InferResponse {
    InferResponse::new(content, TokenUsage::new(12, 34), StopReason::EndTurn)
}

#[test]
fn final_text_is_borrowed_without_reading_or_mutating_separate_thinking() {
    for content in [
        vec![text("answer")],
        vec![thinking(), text("answer")],
        vec![text("answer"), thinking()],
        vec![thinking(), text("answer"), thinking()],
    ] {
        let answer = response(content);
        let original = format!("{answer:?}");
        assert_eq!(answer_text(&answer), Some("answer"));
        assert_eq!(
            format!("{answer:?}"),
            original,
            "projection cannot rewrite evidence"
        );
    }
}

#[test]
fn thinking_cannot_supply_an_answer_or_hide_ambiguous_or_effectful_blocks() {
    let extras = [
        text("a second answer"),
        ContentBlock::ToolUse {
            id: "t".into(),
            name: "write".into(),
            input: json!({}),
        },
        ContentBlock::ToolResult {
            tool_use_id: "t".into(),
            content: "answer".into(),
            is_error: false,
        },
        ContentBlock::Image {
            source: "cas:not-read".into(),
            detail: None,
        },
    ];
    for extra in extras {
        let answer = response(vec![thinking(), text("answer"), extra]);
        assert_eq!(answer_text(&answer), None);
    }
    for content in [vec![], vec![thinking()], vec![thinking(), thinking()]] {
        assert_eq!(answer_text(&response(content)), None);
    }
    for stop in [
        StopReason::MaxTokens,
        StopReason::ToolUse,
        StopReason::Unknown("unknown".into()),
    ] {
        let mut answer = response(vec![thinking(), text("answer")]);
        answer.stop_reason = stop;
        assert_eq!(
            answer_text(&answer),
            None,
            "a final text cannot erase the stop reason"
        );
    }
}

#[test]
fn the_closed_judge_uses_only_the_final_text_and_keeps_its_offered_choices() {
    let question = ChoiceQuestion::new(
        "whole",
        "judge",
        json!({}),
        vec![ChoiceOption {
            key: "faithful".into(),
            description: "the request is carried".into(),
        }],
    );
    let answer = response(vec![thinking(), text(r#"{"choice":"faithful"}"#)]);
    assert_eq!(decoded(&question, &answer).unwrap(), "faithful");
    let thought_only = response(vec![ContentBlock::Thinking {
        text: r#"{"choice":"faithful"}"#.into(),
    }]);
    assert!(decoded(&question, &thought_only).is_err());
    assert!(decoded(&question, &response(vec![thinking(), text("")])).is_err());
    assert!(
        decoded(
            &question,
            &response(vec![thinking(), text(r#"{"choice":"invented"}"#)])
        )
        .is_err()
    );
}
