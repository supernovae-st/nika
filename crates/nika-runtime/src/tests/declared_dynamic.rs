// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A model RENDERED at dispatch — from an input, or from an earlier task's
//! output — is bound by the file's declared route exactly like a static
//! one. Another provider is refused before any request (the wire and the
//! agent provider both count zero), with `fallback: none`; the route's
//! own call keeps its full receipt on the terminal.

use std::sync::{Arc, Mutex};

use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
use nika_providers::{ProviderRegistry, ProvidersConfig, VerbNeeds};
use nika_types::access::AccessClass;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::{Value, json};

use super::*;

/// The file's route serves this model; [`OTHER`] is another ready provider.
const ROUTE: &str = "deepseek/deepseek-flash";
const OTHER: &str = "mistral/mistral-small-latest";

/// Every post by URL; each answer names the model it was sent, suffixed,
/// so a responder read from the answer is told apart from the request.
#[derive(Default)]
struct Wire {
    posts: Mutex<Vec<String>>,
}

impl HttpPostDyn for Wire {
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.posts.lock().expect("posts").push(req.url.clone());
        let sent: Value = serde_json::from_slice(req.body.as_ref().expect("a body")).expect("json");
        let served = format!("{}-served", sent["model"].as_str().unwrap_or_default());
        let body = json!({"id": "fixture", "model": served,
            "choices": [{"message": {"content": "observed"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 9, "completion_tokens": 3, "total_tokens": 12}});
        Ok(HttpResponse::new(
            200,
            std::collections::BTreeMap::new(),
            body.to_string().into(),
            req.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("no route here streams");
    }
}

fn api(id: &str) -> ProviderProbe {
    ProviderProbe::new(
        id,
        true,
        true,
        format!("{}_API_KEY", id.to_uppercase()),
        false,
        ProviderReadiness::new(
            true,
            true,
            None,
            None,
            true,
            ExecutionLocus::Cloud,
            AccessClass::Api,
        ),
        "https://api.example.com",
    )
}

const ACCESS: &str = "run:\n  access: { via: deepseek, protocol: api, fallback: none }\n";

/// The model comes from the operator's `--var m`; `run_block` is the run
/// declaration (empty: none).
fn from_input_under(verb: &str, run_block: &str) -> String {
    format!(
        "nika: dynamic\ninputs:\n  m: {{ type: string, required: true }}\npermits: {{}}\n\
         {run_block}tasks:\n  ask:\n    {verb}: {{ prompt: hello, model: \"${{{{ inputs.m }}}}\" \
         }}\n"
    )
}

/// The model comes from an earlier task's output; `run_block` as above.
fn from_task_under(verb: &str, run_block: &str) -> String {
    format!(
        "nika: dynamic\npermits: {{ exec: [\"true\"] }}\n{run_block}tasks:\n  pick:\n    exec: \
         {{ command: [\"true\"] }}\n  ask:\n    with: {{ m: \"${{{{ tasks.pick.output }}}}\" \
         }}\n    {verb}: {{ prompt: hello, model: \"${{{{ with.m }}}}\" }}\n"
    )
}

fn from_input(verb: &str) -> String {
    from_input_under(verb, ACCESS)
}

fn from_task(verb: &str) -> String {
    from_task_under(verb, ACCESS)
}

/// [`run_pinned`] with no operator flag.
async fn run(source: &str, model: &str) -> (RunOutcome, Vec<Event>, Vec<String>, usize) {
    run_pinned(source, model, None).await
}

/// Run `source` under the plan the door resolves for its declaration (if
/// any) and the operator's `--access` `pin`, on a machine with BOTH
/// providers ready; `model` is the `--var m` and the queued `exec` output.
/// Returns the wire posts and agent-provider calls.
async fn run_pinned(
    source: &str,
    model: &str,
    pin: Option<&str>,
) -> (RunOutcome, Vec<Event>, Vec<String>, usize) {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let requirement = wf
        .run
        .as_ref()
        .and_then(|run| run.value.access_requirement());
    let probes = [api("deepseek"), api("mistral")];
    let plan = nika_providers::resolve_execution_plan_declared(
        &[],
        &probes,
        pin,
        VerbNeeds::new(true, true),
        requirement.as_ref(),
    );
    assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
    let wire = Arc::new(Wire::default());
    let config = ProvidersConfig::new()
        .with_key("deepseek", Secret::new("fixture"))
        .with_key("mistral", Secret::new("fixture"));
    let agent_api = Arc::new(MockProvider::new("mock").enqueue_text("observed"));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new().enqueue_ok(model))),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(ProviderRegistry::new(Arc::clone(&wire), config)),
            ROUTE,
        ),
        AgentVerb::new(
            Arc::clone(&agent_api),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            ROUTE,
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
    .with_var_overrides([("m".to_owned(), Value::from(model))].into_iter().collect())
    .with_access_plan(plan);
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles");
    let posts = wire.posts.lock().expect("posts").clone();
    (
        outcome,
        sink.into_events(),
        posts,
        agent_api.captured_requests().len(),
    )
}

fn field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| match &kv.value {
            FieldValue::String(value) => Some(value.as_str()),
            _ => None,
        })
}

fn terminal(events: &[Event], kind: EventKind) -> &Event {
    events
        .iter()
        .find(|e| e.kind == kind && field(e, "task") == Some("ask"))
        .unwrap_or_else(|| panic!("no {kind:?} for `ask` in {events:?}"))
}

fn json_field(event: &Event, key: &str) -> Value {
    serde_json::from_str(field(event, key).unwrap_or_else(|| panic!("no `{key}` on {event:?}")))
        .expect("json")
}

#[tokio::test]
async fn a_rendered_model_on_another_provider_is_refused_before_any_request() {
    for (name, source) in [
        ("infer from an input", from_input("infer")),
        ("infer from a task", from_task("infer")),
        ("agent from an input", from_input("agent")),
        ("agent from a task", from_task("agent")),
    ] {
        let (outcome, events, posts, agent_calls) = run(&source, OTHER).await;
        assert!(!outcome.ok, "{name}: {events:?}");
        assert!(posts.is_empty(), "{name}: a provider was dialed: {posts:?}");
        assert_eq!(agent_calls, 0, "{name}: the agent provider was asked");
        let detail = field(terminal(&events, EventKind::TaskFailed), "detail")
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("NIKA-1801")
                && detail.contains("`run.access.via: deepseek`")
                && detail.contains(OTHER),
            "{name}: {detail}"
        );
    }
}

#[tokio::test]
async fn a_rendered_model_on_the_declared_route_keeps_its_full_receipt() {
    let requirement = json!({"via": "deepseek", "protocol": "api", "fallback": "none"});
    let (outcome, events, posts, _) = run(&from_input("infer"), ROUTE).await;
    assert!(outcome.ok, "{events:?}");
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert!(posts[0].contains("deepseek"), "{posts:?}");
    let done = terminal(&events, EventKind::TaskCompleted);
    assert_eq!(field(done, "access_id"), Some("deepseek"));
    assert_eq!(field(done, "access"), Some("api"));
    assert_eq!(json_field(done, "access_requirement"), requirement);
    let selection = json_field(done, "access_selection");
    assert_eq!(selection["protocol"], "api");
    let sent = selection["model"]["transmitted"]
        .as_str()
        .unwrap_or_default();
    assert!(!sent.is_empty(), "{selection}");
    assert_eq!(
        selection["responder"],
        json!({"model": format!("{sent}-served"), "evidence": "api_response"}),
        "the responder is read from the answer, never copied from the request"
    );

    let (outcome, events, posts, agent_calls) = run(&from_task("agent"), ROUTE).await;
    assert!(outcome.ok, "{events:?}");
    assert!(
        posts.is_empty() && agent_calls == 1,
        "{posts:?} · {agent_calls}"
    );
    let done = terminal(&events, EventKind::TaskCompleted);
    assert_eq!(field(done, "access_id"), Some("deepseek"));
    assert_eq!(json_field(done, "access_requirement"), requirement);
    assert_eq!(json_field(done, "access_selection")["protocol"], "api");
}

/// The operator's `--access deepseek` on a file that declares nothing binds
/// a rendered model exactly as the file's `via` would: another provider is
/// refused before any request, and the pinned provider's call names its
/// route. With neither flag nor declaration the run is today's, unchanged.
#[tokio::test]
async fn an_operator_pin_binds_a_rendered_model_and_no_selection_keeps_today() {
    for (name, source) in [
        ("infer from an input", from_input_under("infer", "")),
        ("agent from a task", from_task_under("agent", "")),
    ] {
        let (outcome, events, posts, agent_calls) =
            run_pinned(&source, OTHER, Some("deepseek")).await;
        assert!(!outcome.ok, "{name}: {events:?}");
        assert!(
            posts.is_empty() && agent_calls == 0,
            "{name}: {posts:?} · {agent_calls}"
        );
        let detail = field(terminal(&events, EventKind::TaskFailed), "detail")
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("NIKA-1801")
                && detail.contains("`--access deepseek`")
                && detail.contains(OTHER),
            "{name}: {detail}"
        );
    }
    let (outcome, events, posts, _) =
        run_pinned(&from_input_under("infer", ""), ROUTE, Some("deepseek")).await;
    assert!(outcome.ok, "{events:?}");
    assert!(
        posts.len() == 1 && posts[0].contains("deepseek"),
        "{posts:?}"
    );
    let done = terminal(&events, EventKind::TaskCompleted);
    assert_eq!(
        (field(done, "access"), field(done, "access_id")),
        (Some("api"), Some("deepseek"))
    );
    assert_eq!(field(done, "access_requirement"), None, "no declaration");

    let (outcome, events, posts, _) = run_pinned(&from_input_under("infer", ""), OTHER, None).await;
    assert!(
        outcome.ok,
        "no flag, no declaration: today's run · {events:?}"
    );
    assert!(
        posts.len() == 1 && posts[0].contains("mistral"),
        "{posts:?}"
    );
    let done = terminal(&events, EventKind::TaskCompleted);
    assert_eq!(
        field(done, "access_id"),
        None,
        "today's prefix stamp, unchanged"
    );
}
