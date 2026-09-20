//! The resident binds a schedule's declared `inputs` (#1370, the Serve door):
//! judged at `PUT` and at fire by the same law as `POST /v1/jobs`, for API
//! schedules and project beats alike. Split from `schedule_tests.rs` under the
//! file cap; the clock, the noop backend and the request helpers are shared.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use super::schedule_tests::{ManualClock, NoopBackend, put_request};
use super::tests::{TestWorld, get_request, limits};
use super::{ExecutionBackend, ExecutionDisposition, ExecutionOutcome};

/// A workflow that declares typed inputs: one required, one defaulted.
const INTAKE_WORKFLOW: &str = concat!(
    "nika: intake\n",
    "inputs:\n",
    "  tenant: { type: string, required: true }\n",
    "  limit: { type: integer, default: 5 }\n",
    "permits:\n",
    "  tools: [\"nika:jq\"]\n",
    "tasks:\n",
    "  value:\n",
    "    invoke:\n",
    "      tool: nika:jq\n",
    "      args: { input: { tenant: \"${{ inputs.tenant }}\", limit: \"${{ inputs.limit }}\" }, expression: \".\" }\n",
);

/// A backend that records the literal inputs the resident binds on a fire.
#[derive(Debug, Default)]
struct InputsBackend {
    calls: AtomicUsize,
    called: tokio::sync::Notify,
    inputs: Mutex<Option<std::collections::BTreeMap<String, serde_json::Value>>>,
}

impl InputsBackend {
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn inputs(&self) -> Option<std::collections::BTreeMap<String, serde_json::Value>> {
        self.inputs.lock().expect("recorded inputs").clone()
    }

    async fn wait_for_call(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let called = self.called.notified();
                if self.calls() != 0 {
                    break;
                }
                called.await;
            }
        })
        .await
        .expect("scheduled backend call");
    }
}

impl ExecutionBackend for InputsBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.called.notify_waiters();
            ExecutionDisposition::Succeeded.into()
        })
    }

    fn execute_with_inputs<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        _max_cost_usd: Option<f64>,
        _access_pin: Option<&str>,
        inputs: &std::collections::BTreeMap<String, serde_json::Value>,
        _cancel: nika_types::cancel::CancelCtx,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        *self.inputs.lock().expect("record inputs") = Some(inputs.clone());
        self.execute(context)
    }
}

fn intake_body(at: &str, inputs: &serde_json::Value) -> String {
    json!({
        "workflow": "intake.nika",
        "when": {"kind": "once", "at": at},
        "maxCostUsd": 0.25,
        "missed": "catch-up-once",
        "inputs": inputs
    })
    .to_string()
}

/// #1370 through the resident door: a schedule's declared `inputs` are
/// stored with the definition, enter its revision, survive a restart, and
/// are bound on the fire as the typed values the workflow declared — the
/// `--var` coercion (`"7"` on an `integer` becomes `7`) and the literal
/// admission validator of `POST /v1/jobs`, one law behind two doors.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_schedule_with_inputs_binds_them_typed_on_the_resident_fire() {
    let world = TestWorld::new();
    std::fs::write(world.workflows.join("intake.nika"), INTAKE_WORKFLOW).expect("intake workflow");
    let backend = Arc::new(InputsBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    let created = server
        .request(&put_request(
            "intake-inputs",
            &intake_body(
                "2026-09-01T09:00:00Z",
                &json!({"tenant": "acme", "limit": 7}),
            ),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(created.status, 200, "{}", created.body);
    let created_body = created.json();
    assert_eq!(
        created_body["status"]["definition"]["inputs"],
        json!({"tenant": "acme", "limit": "7"}),
        "the definition carries the bound inputs as --var text"
    );
    let revision = created_body["status"]["revision"]
        .as_str()
        .expect("revision")
        .to_owned();

    clock.wait_for_sleeps(2).await;
    clock.advance_to("2026-09-01T09:00:00Z[UTC]");
    backend.wait_for_call().await;
    assert_eq!(
        backend.inputs(),
        Some(std::collections::BTreeMap::from([
            ("tenant".to_owned(), json!("acme")),
            ("limit".to_owned(), json!(7))
        ])),
        "the resident binds the declared inputs coerced by type"
    );
    server.stop().await.expect("first stop");

    let restarted_backend = Arc::new(InputsBackend::default());
    let restarted = world
        .start_with_clock(restarted_backend.clone(), limits(), clock.clone())
        .await;
    let recovered = restarted
        .request(&get_request("/v1/schedules/intake-inputs"))
        .await;
    assert_eq!(recovered.status, 200, "{}", recovered.body);
    assert_eq!(
        recovered.json()["definition"]["inputs"],
        json!({"tenant": "acme", "limit": "7"}),
        "inputs survive a restart with the definition"
    );
    assert_eq!(recovered.json()["revision"], revision);
    restarted.stop().await.expect("restart stop");
}

/// The inputs are judged at `PUT`, before the first slot, by the same law
/// the fire applies: an unknown key names the declared set, a value the
/// declared type refuses names the type, a missing required input is the
/// run's `NIKA-1708`, the `@env:` channel stays the CLI edge's, and a
/// structured value never reaches the schedule. Nothing is persisted.
#[tokio::test(flavor = "multi_thread")]
async fn schedule_inputs_are_judged_at_put_by_the_admission_law() {
    let world = TestWorld::new();
    std::fs::write(world.workflows.join("intake.nika"), INTAKE_WORKFLOW).expect("intake workflow");
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let cases = [
        (
            "unknown-key",
            json!({"tenant": "acme", "nope": "x"}),
            "unknown_input",
        ),
        (
            "wrong-type",
            json!({"tenant": "acme", "limit": "many"}),
            "input_type_mismatch",
        ),
        ("missing-required", json!({"limit": 3}), "NIKA-1708"),
        (
            "env-channel",
            json!({"tenant": "@env:TENANT"}),
            "env_channel_unsupported",
        ),
        (
            "structured",
            json!({"tenant": {"name": "acme"}}),
            "schedule.body",
        ),
    ];
    for (id, inputs, expected) in cases {
        let response = server
            .request(&put_request(
                id,
                &intake_body("2099-09-01T07:00:00Z", &inputs),
                "If-None-Match: *\r\n",
                true,
            ))
            .await;
        assert_eq!(response.status, 422, "{id}: {}", response.body);
        assert!(
            response.body.contains(expected),
            "{id}: expected `{expected}` in {}",
            response.body
        );
        let absent = server
            .request(&get_request(&format!("/v1/schedules/{id}")))
            .await;
        assert_eq!(absent.status, 404, "{id} must not persist: {}", absent.body);
    }
    server.stop().await.expect("stop");
}

/// A project beat's `inputs:` ride the same law: the resident binds them
/// typed on the fire, and a beat whose workflow never declared the key is
/// refused at fire time and contained to that beat like any fire-time
/// admission failure — the sibling beat fires, the resident keeps running.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_beat_with_inputs_binds_them_and_an_undeclared_key_is_contained() {
    let world = TestWorld::new();
    std::fs::write(world.workflows.join("intake.nika"), INTAKE_WORKFLOW).expect("intake workflow");
    std::fs::write(
        world.workflows.join("nika.yaml"),
        concat!(
            "nika: proj\narm:\n",
            "  - workflow: intake.nika\n",
            "    cadence: \"TZ=UTC * * * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "    inputs: { tenant: acme, limit: 9 }\n",
            "  - workflow: root.nika\n",
            "    cadence: \"TZ=UTC * * * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "    inputs: { tenant: acme }\n",
        ),
    )
    .expect("project nika.yaml");
    let backend = Arc::new(InputsBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:30Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    clock.wait_for_sleeps(1).await;
    clock.advance_to("2026-09-01T08:01:01Z[UTC]");
    backend.wait_for_call().await;
    assert_eq!(
        backend.inputs(),
        Some(std::collections::BTreeMap::from([
            ("tenant".to_owned(), json!("acme")),
            ("limit".to_owned(), json!(9))
        ])),
        "the project beat's inputs are bound typed on the resident fire"
    );
    // A live project beat without a finding reads 404 on this door; the
    // refused fire is named on the beat once the scheduler has judged its
    // slot, so poll for the projection rather than a fixed sleep count.
    let finding = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let response = server.request(&get_request("/v1/schedules/root")).await;
            if response.status == 200 {
                let body = response.json();
                assert_eq!(body["origin"], "project", "{body}");
                break body["finding"].clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the refused fire is named on the beat");
    assert_eq!(finding["code"], "schedule.admission", "{finding}");
    assert_eq!(backend.calls(), 1, "an undeclared input never fires");
    let alive = server.request(&get_request("/v1/workflows")).await;
    assert_eq!(alive.status, 200, "the resident is alive: {}", alive.body);
    server
        .stop()
        .await
        .expect("a contained refusal stops clean");
}
