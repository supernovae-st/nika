use super::*;
use crate::test_support::{FakeHttp, collect, resolved_with};
use nika_kernel::ai::provider::{
    InferEvent, Message, ProviderInferDyn as _, ProviderStreamDyn as _, ReasoningEffort,
    ResponseFormat, Role, ToolChoice, ToolDef,
};
use serde_json::json;

const ANSWER: &str = r#"{"model":"qwen3.5:4b-served","message":{"role":"assistant","content":"ok"},"done":true,"done_reason":"stop","prompt_eval_count":12,"eval_count":2}"#;
fn req() -> InferRequest {
    let mut req = InferRequest::new(
        "ollama/qwen3.5:4b",
        vec![Message::text(Role::User, "complete input")],
    );
    req.max_tokens = Some(16_384);
    req
}

#[tokio::test]
async fn buffered_native_context_route_model_and_usage_are_preserved() {
    let fake = FakeHttp::with_json(200, ANSWER);
    let rp = resolved_with(&fake, "ollama/qwen3.5:4b", "");
    let (answer, report) = rp.infer_reported(req()).await.expect("native answer");
    let captured = fake.captured();
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].url, "http://127.0.0.1:11434/api/chat");
    assert!(!captured[0].follow_redirects);
    assert!(!captured[0].headers.contains_key("authorization"));
    let body: Value =
        serde_json::from_slice(captured[0].body.as_ref().expect("body")).expect("JSON");
    assert_eq!(body["model"], "qwen3.5:4b");
    assert_eq!(body["messages"][0]["content"], "complete input");
    assert_eq!(body["stream"], false);
    assert_eq!(body["truncate"], false);
    assert_eq!(body["shift"], false);
    assert_eq!(body["options"]["num_ctx"], 65_536);
    assert_eq!(body["options"]["num_predict"], 16_384);
    assert!(answer.usage_reported);
    assert_eq!(answer.usage.input_tokens, 12);
    assert_eq!(
        answer.gen_ai.response_model.as_deref(),
        Some("qwen3.5:4b-served")
    );
    assert_eq!(answer.request_id, None, "native API supplies no request id");
    assert_eq!(report.attempts, 1);
    assert_eq!(
        report.inference_calls[0].response_model.as_deref(),
        Some("qwen3.5:4b-served")
    );
    assert_eq!(
        report.inference_calls[0].requested_endpoint.as_deref(),
        Some("http://127.0.0.1:11434/api/chat")
    );
    assert_eq!(report.inference_calls[0].estimated_usd, None);
}

#[tokio::test]
async fn incomplete_missing_and_zero_usage_are_distinct() {
    for (counts, reported) in [
        (json!({}), false),
        (json!({"prompt_eval_count":12}), false),
        (json!({"prompt_eval_count":0,"eval_count":0}), true),
        (json!({"prompt_eval_count":12,"eval_count":-1}), false),
    ] {
        let mut value: Value = serde_json::from_str(ANSWER).expect("fixture");
        value
            .as_object_mut()
            .expect("object")
            .remove("prompt_eval_count");
        value.as_object_mut().expect("object").remove("eval_count");
        value
            .as_object_mut()
            .expect("object")
            .extend(counts.as_object().expect("counts").clone());
        let fake = FakeHttp::with_json(200, &value.to_string());
        let rp = resolved_with(&fake, "ollama/qwen3.5:4b", "");
        let answer = rp.infer(req()).await.expect("content");
        assert_eq!(answer.usage_reported, reported);
    }
}

#[tokio::test]
async fn native_refusal_never_falls_back_to_compat_or_invents_zero_cost() {
    for (status, body) in [
        (400, r#"{"error":"input exceeds context length"}"#),
        (500, "private server diagnostic"),
    ] {
        let fake = FakeHttp::with_json(status, body);
        let rp = resolved_with(&fake, "ollama/qwen3.5:4b", "");
        let (error, report) = rp.infer_reported(req()).await.expect_err("refused");
        assert!(matches!(error, ProviderError::HttpResponse { .. }));
        assert!(!error.to_string().contains(body));
        assert_eq!(fake.captured().len(), 1);
        assert_eq!(report.attempts, 1);
        assert_eq!(report.inference_calls[0].estimated_usd, None);
    }
}

#[tokio::test]
async fn native_guards_refuse_before_dispatch() {
    let mut requests = Vec::new();
    let mut effort = req();
    effort.reasoning_effort = Some(ReasoningEffort::High);
    requests.push(effort);
    let mut choice = req();
    choice.tool_choice = ToolChoice::Required;
    requests.push(choice);
    let mut truncate = req();
    truncate.extra.params.insert("truncate".into(), json!(true));
    requests.push(truncate);
    let mut output = req();
    output
        .extra
        .params
        .insert("options".into(), json!({"num_predict":-1}));
    requests.push(output);
    let mut context = req();
    context
        .extra
        .params
        .insert("options".into(), json!({"num_ctx":4096}));
    requests.push(context);
    for request in requests {
        let fake = FakeHttp::with_json(200, ANSWER);
        let rp = resolved_with(&fake, "ollama/qwen3.5:4b", "");
        assert!(rp.infer(request).await.is_err());
        assert!(fake.captured().is_empty());
    }
}

#[test]
fn native_body_carries_schema_tools_history_and_requested_capacity() {
    let mut req = req();
    req.response_format =
        ResponseFormat::JsonSchema(json!({"type":"object","properties":{"ok":{"type":"boolean"}}}));
    req.tools = vec![ToolDef::new("nika:read", "read", json!({"type":"object"}))];
    req.messages.push(Message::new(
        Role::Assistant,
        vec![ContentBlock::ToolUse {
            id: "c1".into(),
            name: "nika:read".into(),
            input: json!({"path":"a"}),
        }],
    ));
    req.messages.push(Message::new(
        Role::User,
        vec![ContentBlock::ToolResult {
            tool_use_id: "c1".into(),
            content: "data".into(),
            is_error: false,
        }],
    ));
    req.extra
        .params
        .insert("options".into(), json!({"num_ctx":32768}));
    let names = ToolNameMap::from_tools(&req.tools);
    let body = request::body("qwen3.5:4b", &req, false, &names).expect("body");
    assert_eq!(body["format"]["type"], "object");
    assert_eq!(body["tools"][0]["function"]["name"], "nika_read");
    assert_eq!(body["messages"][1]["tool_calls"][0]["id"], "c1");
    assert_eq!(
        body["messages"][1]["tool_calls"][0]["function"]["arguments"],
        json!({"path":"a"})
    );
    assert_eq!(body["messages"][2]["tool_call_id"], "c1");
    assert_eq!(body["options"]["num_ctx"], 32768);
    let value = json!({"message":{"tool_calls":[{"id":"c1","function":{"name":"nika_read","arguments":{"path":"a"}}}]}});
    assert!(
        matches!(&contents(&value, &names, &mut BTreeSet::new()).expect("tool")[0], ContentBlock::ToolUse {id,name,input} if id=="c1" && name=="nika:read" && input==&json!({"path":"a"}))
    );
}

#[tokio::test]
async fn ndjson_across_utf8_boundaries_has_one_terminal_and_preserves_thinking() {
    let wire = concat!(
        "{\"message\":{\"thinking\":\"réfléchir\",\"content\":\"\"},\"done\":false}\n",
        "{\"message\":{\"content\":\"été\"},\"done\":false}\n",
        "{\"message\":{},\"done\":true,\"done_reason\":\"stop\",\"prompt_eval_count\":0,\"eval_count\":0}"
    );
    for size in [1, 7, wire.len()] {
        let fake = FakeHttp::with_stream(200, wire, size);
        let rp = resolved_with(&fake, "ollama/qwen3.5:4b", "");
        let events = collect(rp.infer_stream(req()).await.expect("stream")).await;
        assert!(events.iter().all(Result::is_ok));
        assert!(
            events
                .iter()
                .any(|e| matches!(e,Ok(InferEvent::Thinking{text}) if text=="réfléchir"))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e,Ok(InferEvent::Delta{text}) if text=="été"))
        );
        assert!(events.iter().any(
            |e| matches!(e,Ok(InferEvent::Usage(u)) if u.input_tokens==0 && u.output_tokens==0)
        ));
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Ok(InferEvent::Done { .. })))
                .count(),
            1
        );
        assert!(matches!(events.last(), Some(Ok(InferEvent::Done { .. }))));
    }
}

#[tokio::test]
async fn ndjson_missing_terminal_error_duplicate_or_trailing_data_cannot_succeed() {
    for wire in [
        "{\"message\":{\"content\":\"partial\"},\"done\":false}\n",
        "{\"error\":\"private failure\"}\n",
        "{\"message\":{},\"done\":false,\"done\":true}\n",
        "{\"message\":{},\"done\":true}\n{\"message\":{},\"done\":false}\n",
    ] {
        let fake = FakeHttp::with_stream(200, wire, 3);
        let rp = resolved_with(&fake, "ollama/qwen3.5:4b", "");
        let events = collect(rp.infer_stream(req()).await.expect("open")).await;
        assert!(matches!(events.last(), Some(Err(_))), "{events:?}");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Ok(InferEvent::Done { .. }))),
            "{events:?}"
        );
    }
}

#[test]
fn endpoint_conversion_preserves_override_authority_and_refuses_ambiguity() {
    assert_eq!(
        request::endpoint("https://gpu.lan/proxy/v1/chat/completions").expect("native"),
        "https://gpu.lan/proxy/api/chat"
    );
    assert_eq!(
        request::endpoint("http://127.0.0.1:11434/api/chat").expect("native"),
        "http://127.0.0.1:11434/api/chat"
    );
    for url in [
        "https://gpu.lan/other",
        "https://gpu.lan/v1?key=private",
        "https://key@gpu.lan/v1",
    ] {
        assert!(request::endpoint(url).is_err());
    }
}

#[tokio::test]
async fn ndjson_frame_limit_is_a_refusal_not_a_truncation() {
    let wire = "x".repeat(1024 * 1024 + 1);
    let fake = FakeHttp::with_stream(200, &wire, 17_000);
    let rp = resolved_with(&fake, "ollama/qwen3.5:4b", "");
    let events = collect(rp.infer_stream(req()).await.expect("open")).await;
    assert_eq!(events.len(), 1);
    assert!(
        events[0]
            .as_ref()
            .expect_err("bound")
            .to_string()
            .contains("1 MiB")
    );
}

#[tokio::test]
async fn streaming_native_tools_restore_names_and_settle_as_tool_use() {
    let wire = concat!(
        "{\"message\":{\"tool_calls\":[{\"id\":\"call1\",\"function\":{\"name\":\"nika_read\",\"arguments\":{\"path\":\"a\"}}}]},\"done\":false}\n",
        "{\"message\":{},\"done\":true,\"done_reason\":\"stop\"}\n"
    );
    let fake = FakeHttp::with_stream(200, wire, 1);
    let rp = resolved_with(&fake, "ollama/qwen3.5:4b", "");
    let mut request = req();
    request.tools = vec![ToolDef::new("nika:read", "read", json!({"type":"object"}))];
    let events = collect(rp.infer_stream(request).await.expect("open")).await;
    assert!(events.iter().any(
        |e| matches!(e, Ok(InferEvent::ToolUseStart{id,name}) if id=="call1" && name=="nika:read")
    ));
    assert!(events.iter().any(|e| matches!(e, Ok(InferEvent::ToolUseDelta{id,partial_json}) if id=="call1" && partial_json=="{\"path\":\"a\"}")));
    assert!(matches!(
        events.last(),
        Some(Ok(InferEvent::Done {
            stop_reason: StopReason::ToolUse,
            ..
        }))
    ));
    assert!(!events.iter().any(|e| matches!(e, Ok(InferEvent::Usage(_)))));
}

#[test]
fn duplicate_tool_ids_refuse_instead_of_aliasing_calls() {
    let value = json!({"message":{"tool_calls":[
        {"id":"same","function":{"name":"read","arguments":{}}},
        {"id":"same","function":{"name":"write","arguments":{}}}]}});
    assert!(contents(&value, &ToolNameMap::default(), &mut BTreeSet::new()).is_err());
}
