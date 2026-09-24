// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use nika_kernel::ai::provider::{Message, ToolDef};

use super::*;
use crate::test_support::{FakeHttp, collect, resolved_with};

fn req(messages: Vec<Message>) -> InferRequest {
    InferRequest::new("test-model", messages)
}

/// `request_body` with the tool-name map derived from the request's own
/// tools — exactly how `infer`/`infer_stream` build it in production.
fn body_of(
    model: &str,
    r: &InferRequest,
    stream: bool,
    cloud: bool,
    provider: &str,
) -> Result<Value, ProviderError> {
    let names = ToolNameMap::from_tools(&r.tools);
    request_body(model, r, stream, cloud, provider, &names)
}

#[tokio::test]
async fn infer_shapes_request_with_bearer_and_parses() {
    let fake = FakeHttp::with_json(
        200,
        r#"{"id":"cc_1","model":"llama-3.3-70b",
                "choices":[{"message":{"content":"salut"},"finish_reason":"stop"}],
                "usage":{"prompt_tokens":9,"completion_tokens":3}}"#,
    );
    let rp = resolved_with(&fake, "groq", "gsk-test");
    let resp = infer(&rp, req(vec![Message::text(Role::User, "hi")]))
        .await
        .expect("infer ok");

    assert!(matches!(resp.stop_reason, StopReason::EndTurn));
    assert_eq!(resp.usage.input_tokens, 9);
    // output == completion_tokens verbatim (the openai wire does NOT
    // fold any sub-count into output — reasoning rides its own field;
    // the gemini wire's thoughts-fold is provider-specific, not here).
    assert_eq!(resp.usage.output_tokens, 3);
    assert_eq!(
        resp.gen_ai.system,
        nika_kernel::genai::GenAiSystem::OpenAiCompatible
    );

    let sent = fake.captured();
    assert_eq!(
        sent[0].url,
        "https://api.groq.com/openai/v1/chat/completions"
    );
    assert_eq!(
        sent[0].headers.get("authorization").unwrap(),
        "Bearer gsk-test"
    );
    let body: Value = serde_json::from_slice(sent[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], "hi");
}

#[tokio::test]
async fn local_profile_sends_no_auth_header() {
    let fake = FakeHttp::with_json(
        200,
        r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{}}"#,
    );
    let rp = resolved_with(&fake, "ollama", "");
    let _ = infer(&rp, req(vec![Message::text(Role::User, "x")]))
        .await
        .expect("ok");
    let sent = fake.captured();
    assert!(
        !sent[0].headers.contains_key("authorization"),
        "keyless local call"
    );
    assert!(sent[0].url.starts_with("http://127.0.0.1:11434"));
}

#[tokio::test]
async fn openrouter_sends_app_attribution_headers() {
    let fake = FakeHttp::with_json(
        200,
        r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{}}"#,
    );
    let rp = resolved_with(&fake, "openrouter", "sk-or-test");
    let _ = infer(&rp, req(vec![Message::text(Role::User, "x")]))
        .await
        .expect("ok");
    let sent = fake.captured();
    assert_eq!(
        sent[0].headers.get("http-referer").map(String::as_str),
        Some("https://nika.sh"),
        "openrouter app attribution · referer"
    );
    assert_eq!(
        sent[0].headers.get("x-title").map(String::as_str),
        Some("Nika"),
        "openrouter app attribution · title"
    );
}

#[tokio::test]
async fn attribution_headers_stay_openrouter_only() {
    let fake = FakeHttp::with_json(
        200,
        r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{}}"#,
    );
    let rp = resolved_with(&fake, "groq", "gsk-test");
    let _ = infer(&rp, req(vec![Message::text(Role::User, "x")]))
        .await
        .expect("ok");
    let sent = fake.captured();
    assert!(
        !sent[0].headers.contains_key("http-referer") && !sent[0].headers.contains_key("x-title"),
        "attribution is an openrouter contract, not an openai-compat one"
    );
}

#[test]
fn tools_and_response_format_shape() {
    let mut r = req(vec![Message::text(Role::User, "calcule")]);
    r.tools = vec![ToolDef::new("add", "adds", json!({"type":"object"}))];
    r.tool_choice = ToolChoice::Specific("add".into());
    r.response_format = ResponseFormat::JsonSchema(json!({"type":"object"}));
    // groq is an OpenAI-compatible peer: the author's schema is taken
    // verbatim (only `openai` runs the strict-mode normalizer below) AND
    // `strict` is NOT claimed — the peer enforces on `json_schema` presence
    // and a strict claim over an un-normalized schema is a false contract.
    let body = body_of("m", &r, false, true, "groq").expect("body");
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "add");
    assert_eq!(body["tool_choice"]["function"]["name"], "add");
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert!(
        body["response_format"]["json_schema"]
            .get("strict")
            .is_none(),
        "a compat peer must not carry the OpenAI-only strict claim"
    );
    assert_eq!(
        body["response_format"]["json_schema"]["schema"],
        json!({"type":"object"}),
        "non-openai schema passes through unmodified"
    );
}

#[test]
fn openai_alone_carries_the_strict_claim() {
    // The strict flag rides ONLY the openai path — where the schema is also
    // normalized to actually satisfy strict mode (additionalProperties +
    // required). Sibling to `tools_and_response_format_shape` (the peer
    // case): together they pin strict to the one provider that earns it.
    let mut r = req(vec![Message::text(Role::User, "x")]);
    r.response_format = ResponseFormat::JsonSchema(json!({
        "type": "object",
        "properties": { "a": { "type": "string" } },
        "required": ["a"]
    }));
    let body = body_of("m", &r, false, true, "openai").expect("body");
    assert_eq!(
        body["response_format"]["json_schema"]["strict"], true,
        "openai claims strict"
    );
    assert_eq!(
        body["response_format"]["json_schema"]["schema"]["additionalProperties"], false,
        "openai schema is normalized to satisfy the strict claim"
    );
}

#[test]
fn openai_gpt5_uses_max_completion_tokens() {
    let mut r = req(vec![Message::text(Role::User, "hi")]);
    r.max_tokens = Some(64);
    let body = body_of("gpt-5.2", &r, false, true, "openai").expect("body");
    assert_eq!(body["max_completion_tokens"], 64);
    assert!(
        body.get("max_tokens").is_none(),
        "gpt-5 rejects max_tokens on the OpenAI wire"
    );
}

#[tokio::test]
async fn catalog_budget_reaches_the_resolved_provider_http_request() {
    let fake = FakeHttp::with_json(
        200,
        r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{}}"#,
    );
    let registry = crate::ProviderRegistry::new(
        std::sync::Arc::clone(&fake),
        crate::ProvidersConfig::new()
            .with_key("openai", nika_kernel::secret::Secret::new("test-key")),
    );
    let provider = registry.resolve("openai/gpt-6-astra").expect("resolve");
    let mut request = req(vec![Message::text(Role::User, "hi")]);
    request.max_tokens = Some(512);
    nika_kernel::ai::provider::ProviderInferDyn::infer(&provider, request)
        .await
        .expect("infer");
    let sent = fake.captured();
    let body: Value = serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("JSON");
    assert_eq!(body["max_completion_tokens"], 512);
    assert!(body.get("max_tokens").is_none());
    assert!(body.get("temperature").is_none());
}

#[test]
fn non_openai_compat_and_legacy_openai_keep_max_tokens() {
    let mut r = req(vec![Message::text(Role::User, "hi")]);
    r.max_tokens = Some(64);

    let legacy = body_of("gpt-4o-mini", &r, false, true, "openai").expect("legacy body");
    assert_eq!(legacy["max_tokens"], 64);
    assert!(legacy.get("max_completion_tokens").is_none());

    let peer = body_of("gpt-5.2", &r, false, true, "groq").expect("peer body");
    assert_eq!(peer["max_tokens"], 64);
    assert!(peer.get("max_completion_tokens").is_none());
}

#[test]
fn extras_max_tokens_never_rides_alongside_the_routed_budget_key() {
    // A raw extras `max_tokens` (the escape hatch's legacy spelling) must
    // not join a body whose budget was already routed to
    // `max_completion_tokens` — OpenAI rejects a request carrying both.
    let mut r = req(vec![Message::text(Role::User, "hi")]);
    r.max_tokens = Some(64);
    r.extra
        .params
        .insert("max_tokens".to_owned(), serde_json::json!(128));
    let body = body_of("gpt-5.2", &r, false, true, "openai").expect("body");
    assert_eq!(body["max_completion_tokens"], 64, "structural key wins");
    assert!(
        body.get("max_tokens").is_none(),
        "the sibling spelling must not ride alongside: {body}"
    );

    // The hatch stays verbatim when nothing structural was routed: an
    // extras-only budget passes through untouched (caller owns the wire).
    let mut hatch = req(vec![Message::text(Role::User, "hi")]);
    hatch
        .extra
        .params
        .insert("max_completion_tokens".to_owned(), serde_json::json!(99));
    let hatch_body = body_of("gpt-5.2", &hatch, false, true, "openai").expect("body");
    assert_eq!(hatch_body["max_completion_tokens"], 99);
    assert!(hatch_body.get("max_tokens").is_none());
}

#[test]
fn tool_round_trip_uses_tool_role_messages() {
    let messages = vec![
        Message::new(
            Role::Assistant,
            vec![ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "add".into(),
                input: json!({"a":1}),
            }],
        ),
        Message::new(
            Role::User,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".into(),
                content: "2".into(),
                is_error: false,
            }],
        ),
    ];
    let body = body_of("m", &req(messages), false, true, "openai").expect("body");
    assert_eq!(body["messages"][0]["tool_calls"][0]["id"], "call_1");
    assert_eq!(
        body["messages"][0]["tool_calls"][0]["function"]["arguments"],
        "{\"a\":1}"
    );
    assert_eq!(body["messages"][1]["role"], "tool");
    assert_eq!(body["messages"][1]["tool_call_id"], "call_1");
}

#[test]
fn parse_maps_tool_calls_and_finish_reasons() {
    let fake = FakeHttp::with_json(200, "{}");
    let rp = resolved_with(&fake, "openai", "sk-test");
    let body = br#"{"id":"cc_2",
            "choices":[{"message":{"content":null,
                "tool_calls":[{"id":"call_9","function":{"name":"add","arguments":"{\"a\":1}"}}]},
                "finish_reason":"tool_calls"}],
            "usage":{"prompt_tokens":1,"completion_tokens":2}}"#;
    let resp = parse_response(&rp, body, &ToolNameMap::default()).expect("parse");
    assert!(matches!(resp.stop_reason, StopReason::ToolUse));
    match &resp.content[0] {
        ContentBlock::ToolUse { id, name, input } => {
            assert_eq!(id, "call_9");
            assert_eq!(name, "add");
            assert_eq!(input["a"], 1);
        }
        other => panic!("expected tool_use, got {other:?}"),
    }
    assert!(matches!(map_finish(Some("length")), StopReason::MaxTokens));
    assert!(matches!(
        map_finish(Some("content_filter")),
        StopReason::ContentFilter
    ));
}

#[test]
fn roles_map_exactly_and_no_stop_key_when_empty() {
    let messages = vec![
        Message::text(Role::System, "sys"),
        Message::text(Role::User, "usr"),
        Message::text(Role::Assistant, "asst"),
    ];
    let body = body_of("m", &req(messages), false, true, "openai").expect("body");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][2]["role"], "assistant");
    assert!(body.get("stop").is_none(), "no stop_sequences → no key");
}

#[test]
fn stream_options_gated_to_cloud_and_stream_only() {
    let r = req(vec![Message::text(Role::User, "x")]);
    let cloud_nostream = body_of("m", &r, false, true, "openai").expect("body");
    assert!(
        cloud_nostream.get("stream_options").is_none(),
        "non-stream → no stream_options"
    );
    let local_stream = body_of("m", &r, true, false, "ollama").expect("body");
    assert!(
        local_stream.get("stream_options").is_none(),
        "local profile → no stream_options"
    );
    let cloud_stream = body_of("m", &r, true, true, "openai").expect("body");
    assert_eq!(cloud_stream["stream_options"]["include_usage"], true);
}

#[test]
fn extras_first_write_wins_and_custom_passes() {
    let mut r = req(vec![Message::text(Role::User, "x")]);
    r.extra
        .params
        .insert("custom_field".into(), serde_json::json!("v"));
    r.extra
        .params
        .insert("model".into(), serde_json::json!("evil"));
    let body = body_of("real", &r, false, true, "openai").expect("body");
    assert_eq!(body["custom_field"], "v");
    assert_eq!(body["model"], "real", "structural keys win");
}

#[test]
fn image_parts_http_and_https_accepted_cas_rejected() {
    let msg = Message::new(
        Role::User,
        vec![
            ContentBlock::Text {
                text: "look".into(),
            },
            ContentBlock::Image {
                source: "http://example.com/i.png".into(),
                detail: None,
            },
        ],
    );
    let body = body_of("m", &req(vec![msg]), false, true, "openai").expect("body");
    let parts = body["messages"][0]["content"]
        .as_array()
        .expect("multimodal parts array");
    assert!(parts.iter().any(|p| p["type"] == "image_url"));

    let data = Message::new(
        Role::User,
        vec![ContentBlock::Image {
            source: "data:image/png;base64,QUJD".into(),
            detail: None,
        }],
    );
    let data_body = body_of("m", &req(vec![data]), false, true, "openai").expect("data URL");
    let data_parts = data_body["messages"][0]["content"]
        .as_array()
        .expect("multimodal parts array");
    assert!(
        data_parts.iter().any(|p| {
            p["type"] == "image_url" && p["image_url"]["url"] == "data:image/png;base64,QUJD"
        }),
        "data: URLs ride as image_url: {data_parts:?}"
    );

    let bad = Message::new(
        Role::User,
        vec![ContentBlock::Image {
            source: "blake3:abc".into(),
            detail: None,
        }],
    );
    assert!(body_of("m", &req(vec![bad]), false, true, "openai").is_err());
}

#[test]
fn mapper_tool_args_emitted_and_finish_synthesizes_done() {
    let mut m = CompatMapper::new(ToolNameMap::default());
    let out = m.map(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"add","arguments":"{partial"}}]},"finish_reason":null}]}"#,
        );
    assert!(
        out.iter().any(|e| matches!(e,
                Ok(InferEvent::ToolUseDelta { partial_json, .. }) if partial_json == "{partial")),
        "non-empty args emit a delta: {out:?}"
    );
    // EOF without [DONE] → finish() synthesizes exactly one Done.
    let tail = m.finish();
    assert!(matches!(tail.first(), Some(Ok(InferEvent::Done { .. }))));
    assert!(m.finish().is_empty(), "done only once");
}

#[tokio::test]
async fn stream_maps_deltas_done_and_usage() {
    let sse = concat!(
        "data: {\"id\":\"cc_s\",\"choices\":[{\"delta\":{\"content\":\"bon\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"soir\"},\"finish_reason\":null}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2}}\n\n",
        "data: [DONE]\n\n",
    );
    let fake = FakeHttp::with_stream(200, sse, 11);
    let rp = resolved_with(&fake, "openai", "sk-test");
    let stream = infer_stream(&rp, req(vec![Message::text(Role::User, "x")]))
        .await
        .expect("opens");
    let events = collect(stream).await;

    let text: String = events
        .iter()
        .filter_map(|e| match e {
            Ok(InferEvent::Delta { text }) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "bonsoir");
    let usage_seen = events
        .iter()
        .any(|e| matches!(e, Ok(InferEvent::Usage(u)) if u.input_tokens == 4));
    assert!(usage_seen);
    match events.last() {
        Some(Ok(InferEvent::Done {
            stop_reason,
            request_id,
            ..
        })) => {
            assert!(matches!(stop_reason, StopReason::EndTurn));
            assert_eq!(request_id.as_deref(), Some("cc_s"));
        }
        other => panic!("expected Done last, got {other:?}"),
    }
}

// ── BUG#4 · OpenAI strict-mode schema normalization (Gate 2 parity) ──

/// Pull the wire `schema` out of an openai `JsonSchema` request body.
fn strict_schema(input: Value) -> Value {
    let mut r = req(vec![Message::text(Role::User, "x")]);
    r.response_format = ResponseFormat::JsonSchema(input);
    let body = body_of("m", &r, false, true, "openai").expect("body");
    body["response_format"]["json_schema"]["schema"].clone()
}

#[test]
fn strict_flat_object_gets_additional_props_and_all_required() {
    let out = strict_schema(json!({
        "type": "object",
        "properties": { "name": {"type": "string"}, "age": {"type": "integer"} }
    }));
    assert_eq!(out["additionalProperties"], false);
    let req: Vec<&str> = out["required"]
        .as_array()
        .expect("required array")
        .iter()
        .map(|v| v.as_str().expect("str"))
        .collect();
    // both keys required (no author `required` → all become nullable +
    // required, OpenAI's optional shape).
    assert!(req.contains(&"name") && req.contains(&"age"), "{req:?}");
    assert_eq!(out["properties"]["name"]["type"], json!(["string", "null"]));
    assert_eq!(out["properties"]["age"]["type"], json!(["integer", "null"]));
}

#[test]
fn strict_required_field_keeps_its_scalar_type() {
    let out = strict_schema(json!({
        "type": "object",
        "properties": { "id": {"type": "string"}, "note": {"type": "string"} },
        "required": ["id"]
    }));
    // `id` was author-required → type untouched; `note` was optional →
    // nullable; both end up in `required` (the strict invariant).
    assert_eq!(out["properties"]["id"]["type"], "string");
    assert_eq!(out["properties"]["note"]["type"], json!(["string", "null"]));
    let mut req: Vec<&str> = out["required"]
        .as_array()
        .expect("req")
        .iter()
        .map(|v| v.as_str().expect("str"))
        .collect();
    req.sort_unstable();
    assert_eq!(req, vec!["id", "note"]);
}

#[test]
fn strict_recurses_into_nested_objects() {
    let out = strict_schema(json!({
        "type": "object",
        "properties": {
            "address": {
                "type": "object",
                "properties": { "city": {"type": "string"} }
            }
        },
        "required": ["address"]
    }));
    let inner = &out["properties"]["address"];
    assert_eq!(
        inner["additionalProperties"], false,
        "inner object tightened"
    );
    assert_eq!(inner["required"], json!(["city"]));
    assert_eq!(
        inner["properties"]["city"]["type"],
        json!(["string", "null"])
    );
}

#[test]
fn strict_recurses_into_array_of_objects() {
    let out = strict_schema(json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "sku": {"type": "string"} }
                }
            }
        },
        "required": ["items"]
    }));
    let elem = &out["properties"]["items"]["items"];
    assert_eq!(
        elem["additionalProperties"], false,
        "array element tightened"
    );
    assert_eq!(elem["required"], json!(["sku"]));
}

#[test]
fn strict_recurses_into_defs() {
    let out = strict_schema(json!({
        "type": "object",
        "properties": { "node": {"$ref": "#/$defs/Node"} },
        "required": ["node"],
        "$defs": {
            "Node": {
                "type": "object",
                "properties": { "label": {"type": "string"} }
            }
        }
    }));
    let node = &out["$defs"]["Node"];
    assert_eq!(node["additionalProperties"], false, "$defs entry tightened");
    assert_eq!(node["required"], json!(["label"]));
}

#[test]
fn strict_make_nullable_is_idempotent_on_existing_union() {
    // An author who already wrote `["string","null"]` must not gain a
    // second `"null"`.
    let out = strict_schema(json!({
        "type": "object",
        "properties": { "maybe": {"type": ["string", "null"]} }
    }));
    assert_eq!(
        out["properties"]["maybe"]["type"],
        json!(["string", "null"])
    );
}

#[test]
fn strict_only_fires_for_openai_not_for_peers() {
    // The exact same nested schema reaches groq verbatim (no injected
    // `additionalProperties`, no widened types) — the regression guard
    // for "do not mutate non-openai providers".
    let nested = json!({
        "type": "object",
        "properties": {
            "inner": { "type": "object", "properties": { "x": {"type": "string"} } }
        }
    });
    let mut r = req(vec![Message::text(Role::User, "x")]);
    r.response_format = ResponseFormat::JsonSchema(nested.clone());
    let body = body_of("m", &r, false, true, "groq").expect("body");
    assert_eq!(
        body["response_format"]["json_schema"]["schema"], nested,
        "peer schema is byte-for-byte the author's"
    );
}

// ── BUG#7 · oneOf → anyOf for openai strict (Gate 2 parity) ──

#[test]
fn strict_rewrites_oneof_to_anyof_in_a_property() {
    // gemini accepts oneOf; openai 400s "'oneOf' is not permitted". The
    // property's oneOf must reach the wire as anyOf, with no oneOf left.
    let out = strict_schema(json!({
        "type": "object",
        "required": ["result"],
        "properties": {
            "result": {
                "oneOf": [
                    {"type": "object", "required": ["card_last4"],
                     "properties": {"card_last4": {"type": "string"}}},
                    {"type": "object", "required": ["bank_account"],
                     "properties": {"bank_account": {"type": "string"}}}
                ]
            }
        }
    }));
    let result = &out["properties"]["result"];
    assert!(result.get("oneOf").is_none(), "no oneOf survives: {result}");
    let branches = result["anyOf"].as_array().expect("anyOf array");
    assert_eq!(branches.len(), 2, "both branches preserved");
    // the branches were normalized too: each object got tightened.
    assert_eq!(branches[0]["additionalProperties"], false);
    assert_eq!(branches[0]["required"], json!(["card_last4"]));
}

#[test]
fn strict_rewrites_nested_oneof_recursively() {
    // a oneOf nested inside a oneOf branch is also rewritten.
    let out = strict_schema(json!({
        "type": "object",
        "required": ["x"],
        "properties": {
            "x": {
                "oneOf": [
                    {"oneOf": [
                        {"type": "string"},
                        {"type": "integer"}
                    ]},
                    {"type": "boolean"}
                ]
            }
        }
    }));
    let x = &out["properties"]["x"];
    assert!(x.get("oneOf").is_none());
    let inner = &x["anyOf"][0];
    assert!(
        inner.get("oneOf").is_none(),
        "nested oneOf rewritten: {inner}"
    );
    assert!(inner["anyOf"].is_array());
}

#[test]
fn strict_merges_oneof_into_existing_anyof() {
    // pathological author: both anyOf and oneOf on one node. The oneOf
    // branches fold into anyOf so no oneOf reaches the wire.
    let out = strict_schema(json!({
        "anyOf": [{"type": "string"}],
        "oneOf": [{"type": "integer"}, {"type": "boolean"}]
    }));
    assert!(out.get("oneOf").is_none(), "oneOf gone");
    assert_eq!(
        out["anyOf"].as_array().expect("anyOf").len(),
        3,
        "1 original anyOf + 2 folded oneOf branches"
    );
}

// ── const + uniqueItems · openai strict (Gate 2 parity) ──

#[test]
fn strict_rewrites_const_to_single_member_enum_preserving_type() {
    // `const` 400s on openai strict; rewrite to a one-member `enum`.
    // The integer const keeps its JSON number type (not stringified).
    let out = strict_schema(json!({
        "type": "object",
        "required": ["version", "name"],
        "properties": {
            "version": {"const": 1},
            "name": {"type": "string"}
        }
    }));
    let version = &out["properties"]["version"];
    assert!(version.get("const").is_none(), "no const survives");
    assert_eq!(version["enum"], json!([1]), "const → one-member enum");
    assert!(
        version["enum"][0].is_number(),
        "integer const stays a number, not a string"
    );
}

#[test]
fn strict_strips_unique_items() {
    // `uniqueItems` is array-only validation, not in the dialect → drop.
    let out = strict_schema(json!({
        "type": "object",
        "required": ["fruits"],
        "properties": {
            "fruits": {
                "type": "array",
                "uniqueItems": true,
                "items": {"type": "string"}
            }
        }
    }));
    let fruits = &out["properties"]["fruits"];
    assert!(fruits.get("uniqueItems").is_none(), "uniqueItems stripped");
    // the rest of the array schema is intact.
    assert_eq!(fruits["type"], "array");
    assert_eq!(fruits["items"]["type"], "string");
}

// ── negation/conditional strip + allOf flatten (live-verified 400s · 2026-07-08) ──

#[test]
fn strict_strips_negation_and_conditional_family() {
    // `not` / `if`/`then`/`else` / `dependentRequired` all 400 at
    // request time ("Unsupported keywords") — stripped at the wire,
    // held by the verb's local validation.
    let out = strict_schema(json!({
        "type": "object",
        "required": ["x"],
        "properties": {
            "x": {"type": "string", "not": {"enum": ["forbidden"]}}
        },
        "if": { "properties": { "x": { "enum": ["a"] } } },
        "then": { "required": ["x"] },
        "dependentRequired": { "x": ["y"] }
    }));
    assert!(out["properties"]["x"].get("not").is_none(), "{out}");
    assert!(out.get("if").is_none());
    assert!(out.get("then").is_none());
    assert!(out.get("dependentRequired").is_none());
}

#[test]
fn strict_flattens_single_branch_allof() {
    // "'allOf' is not permitted" — a single branch inlines into the
    // node and the merged object still earns the strict invariants.
    let out = strict_schema(json!({
        "type": "object",
        "required": ["p"],
        "properties": {
            "p": { "allOf": [{
                "type": "object",
                "required": ["name"],
                "properties": { "name": {"type": "string"} }
            }]}
        }
    }));
    let p = &out["properties"]["p"];
    assert!(p.get("allOf").is_none(), "no allOf survives: {p}");
    assert_eq!(p["type"], "object", "branch keys inlined");
    assert_eq!(p["properties"]["name"]["type"], "string");
    assert_eq!(p["additionalProperties"], false, "merged node tightened");
}

#[test]
fn strict_flattens_multi_branch_allof_with_unions() {
    // A conjunction of two object branches: properties union +
    // required union — the wire shape covers every branch's keys and
    // the local validator still enforces the authored conjunction.
    let out = strict_schema(json!({
        "type": "object",
        "required": ["p"],
        "properties": {
            "p": { "allOf": [
                {"type": "object", "required": ["a"],
                 "properties": {"a": {"type": "string"}}},
                {"type": "object", "required": ["b"],
                 "properties": {"b": {"type": "integer"}}}
            ]}
        }
    }));
    let p = &out["properties"]["p"];
    assert!(p.get("allOf").is_none());
    assert!(p["properties"]["a"].is_object() && p["properties"]["b"].is_object());
    let req = p["required"].as_array().expect("required union");
    assert!(
        req.contains(&json!("a")) && req.contains(&json!("b")),
        "conjunction requires both branches' keys: {req:?}"
    );
}

#[test]
fn peer_keeps_allof_and_not_verbatim() {
    // The strip is an OPENAI-dialect rewrite — a compat peer (groq ·
    // local servers) still receives the author's composition as
    // written.
    let schema = json!({
        "type": "object",
        "properties": { "p": { "allOf": [{"type": "object"}], "not": {"enum": [1]} } }
    });
    let mut r = req(vec![Message::text(Role::User, "x")]);
    r.response_format = ResponseFormat::JsonSchema(schema.clone());
    let body = body_of("m", &r, false, true, "groq").expect("body");
    assert_eq!(
        body["response_format"]["json_schema"]["schema"], schema,
        "peer schema byte-for-byte"
    );
}

// ── BUG#5 · tool-name sanitization on the openai-compat wire (NIKA-463) ──

#[test]
fn tool_names_with_colons_are_sanitized_on_send() {
    let mut r = req(vec![Message::text(Role::User, "go")]);
    r.tools = vec![
        ToolDef::new("nika:read", "", json!({"type":"object"})),
        ToolDef::new("mcp:git/diff", "", json!({"type":"object"})),
    ];
    r.tool_choice = ToolChoice::Specific("nika:read".into());
    let body = body_of("m", &r, false, true, "openai").expect("body");
    // the colon/slash names reach the wire in the legal charset
    assert_eq!(body["tools"][0]["function"]["name"], "nika_read");
    assert_eq!(body["tools"][1]["function"]["name"], "mcp_git_diff");
    // tool_choice's Specific name is sanitized identically
    assert_eq!(body["tool_choice"]["function"]["name"], "nika_read");
}

#[test]
fn resent_tool_use_name_is_sanitized() {
    // The assistant's prior ToolUse (stored canonical) must serialize
    // its function.name back to the sanitized form on the next turn.
    let mut r = req(vec![Message::new(
        Role::Assistant,
        vec![ContentBlock::ToolUse {
            id: "call_1".into(),
            name: "nika:read".into(),
            input: json!({"path": "x"}),
        }],
    )]);
    r.tools = vec![ToolDef::new("nika:read", "", json!({"type":"object"}))];
    let body = body_of("m", &r, false, true, "openai").expect("body");
    assert_eq!(
        body["messages"][0]["tool_calls"][0]["function"]["name"],
        "nika_read"
    );
}

#[test]
fn parse_reverse_maps_tool_name_to_canonical() {
    // The model echoes the SANITIZED name; the parse path restores the
    // canonical id the verb dispatches on.
    let fake = FakeHttp::with_json(200, "{}");
    let rp = resolved_with(&fake, "openai", "sk-test");
    let names =
        ToolNameMap::from_tools(&[ToolDef::new("mcp:git/diff", "", json!({"type":"object"}))]);
    let body = br#"{"id":"cc_3",
            "choices":[{"message":{"content":null,
                "tool_calls":[{"id":"c1","function":{"name":"mcp_git_diff","arguments":"{}"}}]},
                "finish_reason":"tool_calls"}],
            "usage":{}}"#;
    let resp = parse_response(&rp, body, &names).expect("parse");
    match &resp.content[0] {
        ContentBlock::ToolUse { name, .. } => assert_eq!(name, "mcp:git/diff"),
        other => panic!("expected tool_use, got {other:?}"),
    }
}

#[test]
fn stream_reverse_maps_tool_use_start_name() {
    let names = ToolNameMap::from_tools(&[ToolDef::new("nika:done", "", json!({"type":"object"}))]);
    let mut m = CompatMapper::new(names);
    let out = m.map(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"nika_done","arguments":""}}]},"finish_reason":null}]}"#,
        );
    assert!(
        out.iter().any(|e| matches!(e,
                Ok(InferEvent::ToolUseStart { name, .. }) if name == "nika:done")),
        "ToolUseStart name reverse-mapped to canonical: {out:?}"
    );
}

#[tokio::test]
async fn end_to_end_round_trips_a_colon_tool_name() {
    // A colon tool goes out sanitized; the model's sanitized echo comes
    // back as the canonical id — the whole NIKA-463 loop in one test.
    let fake = FakeHttp::with_json(
        200,
        r#"{"id":"cc_e2e","model":"gpt-4o-mini",
                "choices":[{"message":{"content":null,
                    "tool_calls":[{"id":"c1","function":{"name":"nika_read","arguments":"{\"path\":\"f\"}"}}]},
                    "finish_reason":"tool_calls"}],
                "usage":{"prompt_tokens":1,"completion_tokens":1}}"#,
    );
    let rp = resolved_with(&fake, "openai", "sk-test");
    let mut request = req(vec![Message::text(Role::User, "read f")]);
    request.tools = vec![ToolDef::new("nika:read", "reads", json!({"type":"object"}))];
    let resp = infer(&rp, request).await.expect("infer ok");

    // sent: sanitized name on the wire
    let sent = fake.captured();
    let body: Value = serde_json::from_slice(sent[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["tools"][0]["function"]["name"], "nika_read");
    // received: canonical id restored for the executor
    match &resp.content[0] {
        ContentBlock::ToolUse { name, .. } => assert_eq!(name, "nika:read"),
        other => panic!("expected tool_use, got {other:?}"),
    }
}

#[test]
fn unbounded_duplicate_handling_stays_compatible() {
    let fake = FakeHttp::with_json(200, "{}");
    let rp = resolved_with(&fake, "deepseek", "fixture");
    let raw = br#"{"model":"deepseek-flash","usage":{"prompt_tokens":999,"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"prompt_cache_hit_tokens":0,"prompt_cache_miss_tokens":1}}"#;
    let response = parse_response(&rp, raw, &ToolNameMap::from_tools(&[])).expect("legacy parse");
    assert_eq!(response.usage.input_tokens, 1);
}
