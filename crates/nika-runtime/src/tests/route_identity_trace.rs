// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! E32 · the runtime's trace writers under `nika/route-identity@1`. A fake HTTP
//! seam answers a catalog-priced `DeepSeek` call, a declared-free `OpenRouter`
//! call and an `OpenAI`-compatible call whose configured endpoint path holds a
//! sentinel. The task frames carry the durable call and pricing projections, the
//! terminal attribution keys name origins, and the dispatched requests still use
//! the exact endpoints: identity and billing stay exact in memory.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use super::*;

/// Public synthetic sentinel planted in the configured endpoint path.
const S: &str = "e32pathsentinelfc162b12fffe2dd6";

const SOURCE: &str = "nika: identity\npermits: {}\ntasks:\n  paid:\n    infer: { model: 'deepseek/deepseek-v4-pro', prompt: 'say paid', max_tokens: 256 }\n  free:\n    infer: { model: 'openrouter/qwen/qwen3.8-27b:free', prompt: 'say free', max_tokens: 256 }\n  private:\n    infer: { model: 'openai/gpt-oss-120b', prompt: 'say private', max_tokens: 256 }\n";

/// Answers every post with complete usage for the model it names, and keeps the
/// URLs it was sent.
#[derive(Default)]
struct Wire {
    urls: Mutex<Vec<String>>,
}

impl HttpPostDyn for Wire {
    fn supports_single_attempt(&self) -> bool {
        false
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.urls.lock().expect("urls").push(req.url.clone());
        let sent: serde_json::Value =
            serde_json::from_slice(req.body.as_ref().expect("a body")).expect("json");
        let usage = if req.url.contains("openrouter") {
            serde_json::json!({"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
                "cost": 0, "is_byok": false})
        } else {
            serde_json::json!({"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
                "prompt_cache_hit_tokens": 0, "prompt_cache_miss_tokens": 10})
        };
        let body = serde_json::json!({
            "id": "trace-fixture", "model": sent["model"],
            "choices": [{"message": {"content": "observed"}, "finish_reason": "stop"}],
            "usage": usage
        });
        Ok(HttpResponse::new(
            200,
            BTreeMap::new(),
            body.to_string().into(),
            req.url.clone(),
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("no route here streams");
    }
}

async fn run(wire: &Arc<Wire>) -> (RunOutcome, Vec<Event>) {
    let wf = nika_schema::parse(
        SOURCE,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let config = ProvidersConfig::new()
        .with_key("deepseek", Secret::new("fixture"))
        .with_key("openrouter", Secret::new("fixture"))
        .with_key("openai", Secret::new("fixture"))
        .with_base_url(
            "openai",
            format!("https://api.scaleway.ai/{S}/v1/chat/completions"),
        );
    let registry = ProviderRegistry::new(Arc::clone(wire), config);
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(Arc::new(registry), "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        nika_clock::DeclaredClock::system(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles");
    (outcome, sink.into_events())
}

/// Every form of `sentinel` a durable text must not hold: raw, percent-encoded
/// byte by byte in upper and lower hex, and JSON `\u` escaped.
fn forms(sentinel: &str) -> [String; 4] {
    let (mut upper, mut lower, mut escaped) = (String::new(), String::new(), String::new());
    for byte in sentinel.bytes() {
        write!(upper, "%{byte:02X}").expect("string write");
        write!(lower, "%{byte:02x}").expect("string write");
    }
    for c in sentinel.chars() {
        write!(escaped, "\\u{:04x}", u32::from(c)).expect("string write");
    }
    [sentinel.to_owned(), upper, lower, escaped]
}

fn task_frame<'a>(events: &'a [Event], task: &str) -> &'a Event {
    events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted && e.str_field("task") == Some(task))
        .expect("a task frame")
}

fn json(event: &Event, key: &str) -> serde_json::Value {
    serde_json::from_str(event.str_field(key).expect("the field rides")).expect("JSON text")
}

#[tokio::test]
async fn task_frames_and_attribution_keys_name_origins_only() {
    let wire = Arc::new(Wire::default());
    let (outcome, events) = run(&wire).await;
    let urls = wire.urls.lock().expect("urls").clone();
    assert!(
        urls.iter().any(|url| url.contains(S)),
        "the exact endpoint is what was dispatched: {urls:?}"
    );
    for (task, origin, kind, known) in [
        (
            "paid",
            "https://api.deepseek.com:443",
            "catalog_estimate_not_invoice",
            true,
        ),
        (
            "free",
            "https://openrouter.ai:443",
            "catalog_estimate_not_invoice",
            true,
        ),
        ("private", "https://api.scaleway.ai:443", "unknown", false),
    ] {
        let frame = task_frame(&events, task);
        let calls = json(frame, "inference_calls");
        let [call] = calls.as_array().expect("an array").as_slice() else {
            panic!("{task}: {calls}");
        };
        assert_eq!(call["route"]["origin"], origin, "{task}");
        assert_eq!(call["requested_origin"], origin, "{task}");
        assert_eq!(call["estimate_known"], known, "{task}");
        assert_eq!(call["pricing"]["kind"], kind, "{task}");
        assert_eq!(call["withheld"], serde_json::json!([]), "{task}");
        assert_eq!(json(frame, "pricing_route"), call["pricing"], "{task}");
    }
    let terminal = events.iter().rfind(|e| e.is_terminal()).expect("terminal");
    let by_source: BTreeMap<String, f64> =
        serde_json::from_value(json(terminal, "cost_by_source")).expect("amounts");
    let paid = "deepseek/deepseek-v4-pro @ https://api.deepseek.com:443";
    let free = "openrouter/qwen/qwen3.8-27b:free @ https://openrouter.ai:443";
    assert_eq!(
        by_source.keys().map(String::as_str).collect::<Vec<_>>(),
        [paid, free]
    );
    assert!(by_source[free].abs() < f64::EPSILON, "a known zero");
    assert_eq!((outcome.priced_calls, outcome.unpriced_calls), (2, 1));
    let total = outcome.total_cost_usd.expect("metered");
    assert!(
        (total - by_source[paid]).abs() < 1e-9,
        "{total} vs {by_source:?}"
    );
    let settled = nika_event::settlement::RunSettlement::from_events(&events).expect("settles");
    assert_eq!(
        settled.spend.by_source.keys().collect::<Vec<_>>(),
        by_source.keys().collect::<Vec<_>>()
    );
    let journal = serde_json::to_string(&events).expect("events serialize");
    for form in forms(S) {
        assert!(!journal.contains(&form), "{form} reached the journal");
    }
}
