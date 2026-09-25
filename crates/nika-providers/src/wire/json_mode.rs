// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Capability-aware structured output on the OpenAI-compatible wire.
//!
//! `response_format: json_schema` is the FAMILY's dialect, not every seat's.
//! The catalog records a per-model [`JsonMode`]: `Schema` (the provider
//! enforces the schema), `Object` (`json_object` — valid JSON, no schema:
//! `DeepSeek`'s whole line), `Unavailable` (no JSON mode at all). A
//! `JsonSchema` request forwarded verbatim to an `Object` seat is refused
//! at the door — HTTP 400 `invalid_request_error`, no token sampled — which
//! is how the compiler's ONE seated authoring call died on
//! `deepseek/deepseek-chat` (« the authoring model did not answer » · four
//! of eight intents `no-proposal` on the product matrix, 2026-09-21).
//!
//! The wire shapes the request to what the seat can honour instead of
//! forwarding a promise it cannot keep: `Object` → `json_object` with the
//! schema rendered into the last user turn; `Unavailable` → no
//! `response_format` at all, the schema in the prompt. Enforcement then
//! lives where it always did for a non-native seat — the caller validates
//! the reply locally (the verb's NIKA-INFER-002 gate · the compiler's
//! decoder). An ABSENT catalog answer (a local server · a model the catalog
//! never heard of) leaves the request untouched: the family's native mode
//! stays the default until a positive fact says otherwise.

use std::borrow::Cow;

use nika_catalog::JsonMode;
use nika_kernel::ai::provider::{ContentBlock, InferRequest, Message, ResponseFormat, Role};

/// Cap on the schema text rendered into a prompt (the verb's own cap —
/// a schema past it is truncated with a visible marker, never silently).
const SCHEMA_RENDER_CAP: usize = 4096;

/// The request as the seat can honour it: borrowed when nothing changes,
/// rewritten when the catalog's `json_mode` says `json_schema` would be
/// refused at the door.
pub(crate) fn shape(req: &InferRequest, json_mode: Option<JsonMode>) -> Cow<'_, InferRequest> {
    let ResponseFormat::JsonSchema(schema) = &req.response_format else {
        return Cow::Borrowed(req);
    };
    let honoured = match json_mode {
        Some(JsonMode::Object) => ResponseFormat::Json,
        Some(JsonMode::Unavailable) => ResponseFormat::Text,
        // `Schema`, an unknown seat, or a future level: the native dialect stands.
        _ => return Cow::Borrowed(req),
    };
    let mut shaped = req.clone();
    shaped.response_format = honoured;
    append_schema_instruction(&mut shaped.messages, schema);
    Cow::Owned(shaped)
}

/// Keep a short structured answer's finite output budget useful on the exact
/// `DeepSeek` routes whose catalog advertises effort control. Their API defaults
/// to high thinking, which can consume the whole authoring cap before JSON.
/// Explicit caller choices and other models retain their own settings. Low is
/// an effort setting, not a guarantee on the number of reasoning tokens.
/// Source: api-docs.deepseek.com/guides/thinking_mode/ (2026-09-24).
pub(crate) fn bounded_reasoning(
    body: &mut serde_json::Value,
    req: &InferRequest,
    provider: &str,
    model: &str,
) {
    if provider != "deepseek"
        || !matches!(
            req.response_format,
            ResponseFormat::Json | ResponseFormat::JsonSchema(_)
        )
        || !req.max_tokens.is_some_and(|cap| cap > 0 && cap <= 8192)
        || req.thinking_budget.is_some()
        || !nika_catalog::model_capabilities(provider, model)
            .supported_parameters
            .contains(&nika_catalog::ParamFlag::ReasoningEffort)
    {
        return;
    }
    if let Some(object) = body.as_object_mut()
        && !object.contains_key("thinking")
        && !object.contains_key("reasoning_effort")
    {
        object.insert("reasoning_effort".into(), serde_json::json!("low"));
    }
}

/// The instruction a non-native seat needs: the schema, verbatim, on the
/// last user turn (a new user turn when the conversation has none). The
/// word JSON is load-bearing — `DeepSeek`'s `json_object` mode refuses a
/// prompt that never says it. The enum sentence is load-bearing too:
/// nothing enforces the schema on this seat, and without it
/// deepseek-chat answered the compiler's step `op` with `write` (an effect
/// verb) in 1 of 6 samples; with it, 0 of 6 (2026-09-21).
fn append_schema_instruction(messages: &mut Vec<Message>, schema: &serde_json::Value) {
    let instruction = format!(
        "Reply with ONLY a JSON value that satisfies this JSON Schema, no prose, no code \
         fences. Every property that lists an enum takes exactly one of the listed values, \
         spelled as listed; every required property is present; no property outside the \
         schema:\n{}",
        render_schema(schema)
    );
    let Some(user) = messages.iter_mut().rev().find(|m| m.role == Role::User) else {
        messages.push(Message::text(Role::User, instruction));
        return;
    };
    let last_text = user.content.iter_mut().rev().find_map(|block| match block {
        ContentBlock::Text { text } => Some(text),
        _ => None,
    });
    match last_text {
        Some(text) => {
            text.push_str("\n\n");
            text.push_str(&instruction);
        }
        None => user.content.push(ContentBlock::Text { text: instruction }),
    }
}

/// The schema for prompt injection, truncated at the cap on a char boundary.
fn render_schema(schema: &serde_json::Value) -> String {
    let mut rendered = schema.to_string();
    if rendered.len() > SCHEMA_RENDER_CAP {
        let mut cut = SCHEMA_RENDER_CAP;
        while !rendered.is_char_boundary(cut) {
            cut -= 1;
        }
        rendered.truncate(cut);
        rendered.push_str("…(schema truncated)");
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::ai::provider::ProviderInferDyn;
    use serde_json::{Value, json};

    fn schema() -> Value {
        json!({"type": "object", "required": ["choice"], "properties": {"choice": {"type": "string"}}})
    }

    fn structured(messages: Vec<Message>) -> InferRequest {
        let mut req = InferRequest::new("m", messages);
        req.response_format = ResponseFormat::JsonSchema(schema());
        req
    }

    fn last_user_text(req: &InferRequest) -> String {
        let user = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .expect("a user turn");
        user.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn a_schema_seat_or_an_unknown_seat_keeps_the_native_dialect() {
        let req = structured(vec![Message::text(Role::User, "pick one")]);
        for mode in [Some(JsonMode::Schema), None] {
            let shaped = shape(&req, mode);
            assert!(
                matches!(shaped, Cow::Borrowed(_)),
                "{mode:?} must not rewrite"
            );
            assert!(matches!(
                shaped.response_format,
                ResponseFormat::JsonSchema(_)
            ));
            assert_eq!(last_user_text(&shaped), "pick one");
        }
    }

    #[test]
    fn an_object_seat_gets_json_object_and_the_schema_in_the_prompt() {
        let req = structured(vec![
            Message::text(Role::System, "you are terse"),
            Message::text(Role::User, "pick one"),
        ]);
        let shaped = shape(&req, Some(JsonMode::Object));
        assert!(matches!(shaped, Cow::Owned(_)));
        assert_eq!(shaped.response_format, ResponseFormat::Json);
        let text = last_user_text(&shaped);
        assert!(
            text.starts_with("pick one\n\nReply with ONLY a JSON value"),
            "{text}"
        );
        assert!(
            text.contains(&schema().to_string()),
            "the schema travels verbatim"
        );
        // The system turn is untouched: the instruction lands on the user turn.
        assert_eq!(shaped.messages[0].role, Role::System);
        assert_eq!(shaped.messages.len(), 2);
    }

    #[test]
    fn an_unavailable_seat_gets_no_response_format_and_the_schema_in_the_prompt() {
        let req = structured(vec![Message::text(Role::User, "pick one")]);
        let shaped = shape(&req, Some(JsonMode::Unavailable));
        assert_eq!(shaped.response_format, ResponseFormat::Text);
        assert!(last_user_text(&shaped).contains("JSON Schema"));
    }

    #[test]
    fn a_plain_or_json_request_is_never_rewritten() {
        for format in [ResponseFormat::Text, ResponseFormat::Json] {
            let mut req = InferRequest::new("m", vec![Message::text(Role::User, "q")]);
            req.response_format = format.clone();
            for mode in [
                Some(JsonMode::Object),
                Some(JsonMode::Unavailable),
                Some(JsonMode::Schema),
                None,
            ] {
                let shaped = shape(&req, mode);
                assert!(matches!(shaped, Cow::Borrowed(_)));
                assert_eq!(shaped.response_format, format);
            }
        }
    }

    #[test]
    fn the_instruction_opens_a_user_turn_when_the_conversation_has_none() {
        let req = structured(vec![Message::text(Role::System, "only a system turn")]);
        let shaped = shape(&req, Some(JsonMode::Object));
        assert_eq!(shaped.messages.len(), 2);
        assert_eq!(shaped.messages[1].role, Role::User);
        assert!(last_user_text(&shaped).starts_with("Reply with ONLY a JSON value"));
    }

    #[test]
    fn the_instruction_lands_on_the_last_user_turn_of_a_repair_conversation() {
        // A schema-repair round-trip: user · assistant · user. The
        // instruction must follow the LAST user turn, never the first.
        let req = structured(vec![
            Message::text(Role::User, "first ask"),
            Message::text(Role::Assistant, "not json"),
            Message::text(Role::User, "fix it"),
        ]);
        let shaped = shape(&req, Some(JsonMode::Object));
        assert!(last_user_text(&shaped).starts_with("fix it\n\nReply with ONLY"));
        let first = &shaped.messages[0];
        assert!(matches!(&first.content[0], ContentBlock::Text { text } if text == "first ask"));
    }

    #[test]
    fn an_image_only_user_turn_gains_a_text_block() {
        let user = Message::new(
            Role::User,
            vec![ContentBlock::Image {
                source: "https://example.com/i.png".to_owned(),
                detail: None,
            }],
        );
        let req = structured(vec![user]);
        let shaped = shape(&req, Some(JsonMode::Object));
        assert_eq!(shaped.messages[0].content.len(), 2);
        assert!(last_user_text(&shaped).starts_with("Reply with ONLY"));
    }

    #[test]
    fn a_huge_schema_is_truncated_with_a_visible_marker() {
        let big = json!({"type": "object", "description": "x".repeat(10_000)});
        let rendered = render_schema(&big);
        assert!(rendered.len() < 4200, "{}", rendered.len());
        assert!(rendered.ends_with("…(schema truncated)"));
    }

    /// The catalog facts this module rides — a change there is a change here.
    #[test]
    fn the_catalog_places_the_seats_the_matrix_measured() {
        let mode = |p: &str, m: &str| nika_catalog::model_capabilities(p, m).json_mode;
        assert_eq!(mode("deepseek", "deepseek-chat"), Some(JsonMode::Object));
        assert_eq!(
            mode("deepseek", "deepseek-reasoner"),
            Some(JsonMode::Object)
        );
        assert_eq!(mode("openai", "gpt-4o-mini"), Some(JsonMode::Schema));
        assert_eq!(mode("openai", "gpt-5-mini"), Some(JsonMode::Schema));
        assert_eq!(mode("xai", "grok-3"), Some(JsonMode::Schema));
        // The five local servers carry NO rule: an absent fact never downgrades.
        for local in ["ollama", "lmstudio", "llamacpp", "localai", "vllm"] {
            assert_eq!(mode(local, "llama3.2"), None, "{local}");
        }
    }

    /// End to end through the wire: a `DeepSeek` seat never sees `json_schema`
    /// on the body, an `OpenAI` seat still does.
    #[tokio::test]
    async fn the_deepseek_body_carries_json_object_and_the_schema_prompt() {
        use crate::test_support::{FakeHttp, resolved_with};

        let reply =
            json!({"choices":[{"message":{"content":"{\"choice\":\"a\"}"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":10,"completion_tokens":5}})
            .to_string();
        let fake = FakeHttp::with_json(200, &reply);
        let rp = resolved_with(&fake, "deepseek/deepseek-chat", "sk-test");
        let req = structured(vec![Message::text(Role::User, "pick one")]);
        let response = rp.infer(req).await.expect("deepseek answers");
        assert!(
            matches!(&response.content[0], ContentBlock::Text { text } if text.contains("choice"))
        );

        let sent = fake.captured();
        assert_eq!(sent.len(), 1);
        let body: Value =
            serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("json");
        assert_eq!(body["response_format"]["type"], "json_object");
        assert!(body["response_format"].get("json_schema").is_none());
        let content = body["messages"][0]["content"]
            .as_str()
            .expect("text content");
        assert!(
            content.starts_with("pick one\n\nReply with ONLY a JSON value"),
            "{content}"
        );
        assert!(content.contains("\"required\":[\"choice\"]"), "{content}");
    }

    #[tokio::test]
    async fn short_deepseek_json_uses_low_effort_without_increasing_its_cap() {
        use crate::test_support::{FakeHttp, resolved_with};
        let fake = FakeHttp::with_json(
            200,
            r#"{"choices":[{"message":{"content":"{}"},"finish_reason":"stop"}]}"#,
        );
        let provider = resolved_with(&fake, "deepseek/deepseek-flash", "fixture");
        let mut req = structured(vec![Message::text(Role::User, "pick one")]);
        req.max_tokens = Some(8192);
        provider.infer(req).await.expect("answer");
        let sent = fake.captured();
        assert_eq!(sent.len(), 1);
        let body: Value =
            serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("json");
        assert_eq!(body["reasoning_effort"], "low");
        assert_eq!(body["max_tokens"], 8192);
        assert_eq!(body["model"], "deepseek-flash");
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    #[test]
    fn bounded_reasoning_preserves_explicit_settings_and_unqualified_routes() {
        let mut req = structured(vec![]);
        req.max_tokens = Some(8192);
        for mut body in [
            json!({"thinking":{"type":"disabled"}}),
            json!({"reasoning_effort":"max"}),
        ] {
            let before = body.clone();
            bounded_reasoning(&mut body, &req, "deepseek", "deepseek-flash");
            assert_eq!(body, before);
        }
        for (provider, model) in [
            ("openai", "deepseek-flash"),
            ("deepseek", "deepseek-future"),
            ("deepseek", "deepseek-reasoner"),
            ("deepseek", "deepseek-chat"),
        ] {
            let mut body = json!({});
            bounded_reasoning(&mut body, &req, provider, model);
            assert_eq!(body, json!({}), "{provider}/{model}");
        }
        for cap in [None, Some(0), Some(8193)] {
            req.max_tokens = cap;
            let mut body = json!({});
            bounded_reasoning(&mut body, &req, "deepseek", "deepseek-flash");
            assert_eq!(body, json!({}));
        }
        req.max_tokens = Some(8192);
        req.response_format = ResponseFormat::Text;
        let mut body = json!({});
        bounded_reasoning(&mut body, &req, "deepseek", "deepseek-flash");
        assert_eq!(body, json!({}));
    }

    #[tokio::test]
    async fn the_openai_body_keeps_json_schema() {
        use crate::test_support::{FakeHttp, resolved_with};

        let reply =
            json!({"choices":[{"message":{"content":"{\"choice\":\"a\"}"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":10,"completion_tokens":5}})
            .to_string();
        let fake = FakeHttp::with_json(200, &reply);
        let rp = resolved_with(&fake, "openai/gpt-4o-mini", "sk-test");
        let req = structured(vec![Message::text(Role::User, "pick one")]);
        rp.infer(req).await.expect("openai answers");
        let sent = fake.captured();
        let body: Value =
            serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("json");
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["messages"][0]["content"], "pick one");
    }
}
