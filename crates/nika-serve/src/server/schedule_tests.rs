#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;

use super::store::ShutdownPhase;
use super::tests::{TestServer, TestWorld, auth_header, get_request, limits};
use super::{ExecutionBackend, ExecutionDisposition, ExecutionOutcome};

#[derive(Debug)]
struct NoopBackend;

impl ExecutionBackend for NoopBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async { ExecutionDisposition::Succeeded.into() })
    }
}

#[derive(Debug, Default)]
struct CountingBackend {
    calls: AtomicUsize,
    called: tokio::sync::Notify,
    gate: Option<tokio::sync::Semaphore>,
    max_cost_usd: Mutex<Option<f64>>,
    root_bytes: Mutex<Option<Vec<u8>>>,
}

#[derive(Debug)]
struct ManualClock {
    now: Mutex<jiff::Zoned>,
    generation: AtomicUsize,
    sleeps: AtomicUsize,
    changed: tokio::sync::Notify,
    sleep_started: tokio::sync::Notify,
}

impl ManualClock {
    fn new(now: &str) -> Self {
        Self {
            now: Mutex::new(now.parse().expect("manual zoned time")),
            generation: AtomicUsize::new(0),
            sleeps: AtomicUsize::new(0),
            changed: tokio::sync::Notify::new(),
            sleep_started: tokio::sync::Notify::new(),
        }
    }

    fn advance_to(&self, now: &str) {
        *self.now.lock().expect("manual clock") = now.parse().expect("advanced zoned time");
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.changed.notify_waiters();
    }

    async fn wait_for_sleeps(&self, minimum: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let started = self.sleep_started.notified();
                if self.sleeps.load(Ordering::SeqCst) >= minimum {
                    break;
                }
                started.await;
            }
        })
        .await
        .expect("scheduler clock wait");
    }
}

impl super::ResidentClock for ManualClock {
    fn now(&self) -> jiff::Zoned {
        self.now.lock().expect("manual clock").clone()
    }

    fn sleep(&self, _duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let generation = self.generation.load(Ordering::SeqCst);
        self.sleeps.fetch_add(1, Ordering::SeqCst);
        self.sleep_started.notify_waiters();
        Box::pin(async move {
            loop {
                let changed = self.changed.notified();
                if self.generation.load(Ordering::SeqCst) != generation {
                    break;
                }
                changed.await;
            }
        })
    }
}

impl CountingBackend {
    fn gated() -> Self {
        Self {
            gate: Some(tokio::sync::Semaphore::new(0)),
            ..Self::default()
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn max_cost_usd(&self) -> Option<f64> {
        *self.max_cost_usd.lock().expect("recorded max cost")
    }

    fn root_bytes(&self) -> Option<Vec<u8>> {
        self.root_bytes.lock().expect("recorded root bytes").clone()
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

    fn release(&self) {
        self.gate.as_ref().expect("gated backend").add_permits(1);
    }
}

impl ExecutionBackend for CountingBackend {
    fn execute<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        *self.root_bytes.lock().expect("record root bytes") = context
            .snapshot()
            .unit(context.snapshot().root())
            .map(|unit| unit.bytes().to_vec());
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.called.notify_waiters();
            if let Some(gate) = &self.gate {
                let permit = gate.acquire().await.expect("backend gate");
                permit.forget();
            }
            ExecutionDisposition::Succeeded.into()
        })
    }

    fn execute_with_max_cost<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        *self.max_cost_usd.lock().expect("record max cost") = max_cost_usd;
        self.execute(context)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_keeps_store_alive_until_scheduled_observation_finishes() {
    let world = TestWorld::new();
    let backend = Arc::new(CountingBackend::gated());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    let shutdown_probe = server.shutdown_probe();
    shutdown_probe.gate_observation();
    let created = server
        .request(&put_request(
            "shutdown-once",
            &body_at("root.nika.yaml", 0.25, "2026-09-01T09:00:00Z"),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(created.status, 200, "{}", created.body);
    clock.wait_for_sleeps(2).await;
    clock.advance_to("2026-09-01T09:00:00Z[UTC]");
    backend.wait_for_call().await;
    shutdown_probe.wait_observation_blocked().await;

    let stopped = server.signal_stop();
    shutdown_probe.wait_shutdown_loop_observed().await;
    backend.release();

    shutdown_probe.wait_terminal_settled().await;
    let first_phase = shutdown_probe.wait_first_phase().await;
    shutdown_probe.release_observation();

    let stop_result = tokio::time::timeout(Duration::from_secs(5), stopped)
        .await
        .expect("bounded server join")
        .expect("server join");
    if first_phase == ShutdownPhase::StoreShutdown {
        assert!(matches!(stop_result, Err(super::ServerError::BlockingTask)));
    }
    assert_eq!(
        first_phase,
        ShutdownPhase::SchedulerJoin,
        "store shutdown started before the scheduler join"
    );
    stop_result.expect("scheduled observation finishes before store shutdown");
}

fn body(workflow: &str, cost: f64) -> String {
    body_at(workflow, cost, "2099-09-01T07:00:00Z")
}

fn body_at(workflow: &str, cost: f64, at: &str) -> String {
    json!({
        "workflow": workflow,
        "when": {"kind": "once", "at": at},
        "maxCostUsd": cost,
        "missed": "catch-up-once",
        "maxLatenessSeconds": 3600,
        "overlap": "skip",
        "afterSkip": "next_slot"
    })
    .to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_once_wakes_executes_persists_and_does_not_rearm_after_restart() {
    let world = TestWorld::new();
    let backend = Arc::new(CountingBackend::gated());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    let settlement_probe = server.shutdown_probe();
    let at = "2026-09-01T09:00:00Z";
    let created = server
        .request(&put_request(
            "live-once",
            &body_at("root.nika.yaml", 0.25, at),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(created.status, 200, "{}", created.body);
    clock.wait_for_sleeps(2).await;
    std::fs::write(
        world.workflows.join("root.nika.yaml"),
        "nika: root-updated\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 2, expression: \".\" }\n",
    )
    .expect("mutate workflow before fire");
    clock.advance_to("2026-09-01T09:00:00Z[UTC]");
    backend.wait_for_call().await;
    assert_eq!(backend.calls(), 1);
    assert_eq!(backend.max_cost_usd(), Some(0.25));
    assert!(
        backend.root_bytes().is_some_and(|bytes| bytes
            .windows(b"input: 2".len())
            .any(|part| part == b"input: 2")),
        "fire captures exact current workflow bytes"
    );
    let status = server
        .request(&get_request("/v1/schedules/live-once"))
        .await;
    assert_eq!(status.status, 200, "{}", status.body);
    let status_body = status.json();
    assert_eq!(status_body["lastDecision"]["action"], "claimed");
    assert_eq!(status_body["due"]["kind"], "once_consumed");
    let claim = &status_body["lastDecision"]["claim"];
    let run_id = claim["runId"].as_str().expect("claimed run id");
    assert!(claim["executionId"].is_string());
    assert!(claim["traceId"].is_string());
    assert!(
        nika_cadence::ArmGeneration::from_wire(
            claim["generation"].as_str().expect("claim generation")
        )
        .is_some()
    );
    backend.release();
    tokio::time::timeout(
        Duration::from_secs(5),
        settlement_probe.wait_terminal_settled(),
    )
    .await
    .expect("durable terminal settlement");
    let terminal = server
        .request(&get_request(&format!("/v1/jobs/{run_id}")))
        .await;
    assert_eq!(terminal.json()["status"], "succeeded");
    assert_eq!(terminal.json()["receipt"]["job_id"], run_id);
    assert_eq!(
        terminal.json()["receipt"]["execution_id"],
        claim["executionId"]
    );
    assert_eq!(terminal.json()["receipt"]["trace_id"], claim["traceId"]);
    assert_eq!(
        terminal.json()["receipt"]["origin"]["arm_generation"],
        claim["generation"]
    );
    server.stop().await.expect("first stop");

    let restarted_backend = Arc::new(CountingBackend::default());
    let restarted = world
        .start_with_clock(restarted_backend.clone(), limits(), clock.clone())
        .await;
    clock.wait_for_sleeps(3).await;
    assert_eq!(restarted_backend.calls(), 0, "consumed once must not rearm");
    let recovered = restarted
        .request(&get_request("/v1/schedules/live-once"))
        .await;
    assert_eq!(recovered.json()["lastDecision"]["action"], "claimed");
    restarted.stop().await.expect("restart stop");
}

/// #1349 (b): the schedule's required `maxCostUsd` RESTRICTS the server
/// default, never widens it — a 5.00 declaration under the 1.00 server
/// default reaches the runtime clamped to 1.00, while the declaration
/// itself is stored untouched (the clamp lives at the execution edge).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_schedule_ceiling_above_the_server_default_is_clamped_never_widened() {
    let world = TestWorld::new();
    let backend = Arc::new(CountingBackend::gated());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    let created = server
        .request(&put_request(
            "clamped-once",
            &body_at("root.nika.yaml", 5.0, "2026-09-01T09:00:00Z"),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(created.status, 200, "{}", created.body);
    clock.wait_for_sleeps(2).await;
    clock.advance_to("2026-09-01T09:00:00Z[UTC]");
    backend.wait_for_call().await;
    assert_eq!(backend.calls(), 1);
    assert_eq!(
        backend.max_cost_usd(),
        Some(super::DEFAULT_MAX_COST_USD),
        "the lower ceiling wins: the server default clamps the declaration"
    );
    let status = server
        .request(&get_request("/v1/schedules/clamped-once"))
        .await;
    assert_eq!(
        status.json()["definition"]["maxCostUsd"],
        5.0,
        "the declaration is stored untouched"
    );
    backend.release();
    server.stop().await.expect("clean stop");
}

fn put_request(id: &str, body: &str, precondition: &str, authenticated: bool) -> String {
    let auth = authenticated.then(auth_header).unwrap_or_default();
    format!(
        "PUT /v1/schedules/{id} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{precondition}{auth}\r\n{body}",
        body.len()
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_get_lost_response_retry_and_exact_update_etag() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let first_body = body("root.nika.yaml", 0.25);
    let first = server
        .request(&put_request(
            "daily",
            &first_body,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(first.status, 200, "{}", first.body);
    assert_eq!(first.json()["applied"], true);
    assert_eq!(first.json()["changed"], true);
    let etag = first.header("etag").expect("etag").to_owned();

    let state_before_get =
        std::fs::read(world.state.join("schedules/state.json")).expect("state before read");
    let get = server.request(&get_request("/v1/schedules/daily")).await;
    assert_eq!(get.status, 200, "{}", get.body);
    assert_eq!(get.header("etag"), Some(etag.as_str()));
    assert_eq!(get.json()["origin"], "api");
    assert_eq!(get.json()["definition"]["maxCostUsd"], 0.25);
    assert!(get.json()["next"].as_array().expect("next").len() <= 8);
    assert_eq!(
        std::fs::read(world.state.join("schedules/state.json")).expect("state after read"),
        state_before_get,
        "GET is read-only"
    );

    let retry = server
        .request(&put_request(
            "daily",
            &first_body,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(retry.status, 200);
    assert_eq!(retry.json()["changed"], false);
    assert_eq!(retry.header("etag"), Some(etag.as_str()));

    let second_body = body("root.nika.yaml", 0.50);
    let stale = server
        .request(&put_request(
            "daily",
            &second_body,
            &format!("If-Match: \"sha256:{}\"\r\n", "0".repeat(64)),
            true,
        ))
        .await;
    assert_eq!(stale.status, 412, "{}", stale.body);
    assert_eq!(stale.header("etag"), Some(etag.as_str()));

    let update = server
        .request(&put_request(
            "daily",
            &second_body,
            &format!("If-Match: {etag}\r\n"),
            true,
        ))
        .await;
    assert_eq!(update.status, 200, "{}", update.body);
    assert_eq!(update.json()["changed"], true);
    assert_ne!(update.header("etag"), Some(etag.as_str()));
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_exact_updates_have_one_winner() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let first = server
        .request(&put_request(
            "race",
            &body("root.nika.yaml", 0.25),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    let etag = first.header("etag").expect("etag").to_owned();
    let left = put_request(
        "race",
        &body("root.nika.yaml", 0.50),
        &format!("If-Match: {etag}\r\n"),
        true,
    );
    let right = put_request(
        "race",
        &body("root.nika.yaml", 0.75),
        &format!("If-Match: {etag}\r\n"),
        true,
    );
    let (left, right) = tokio::join!(server.request(&left), server.request(&right));
    let mut statuses = [left.status, right.status];
    statuses.sort_unstable();
    assert_eq!(statuses, [200, 412]);
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_precedes_parsing_and_preconditions_are_mandatory() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let unauthorized = server
        .request(&put_request(
            "auth",
            "{not-json",
            "If-None-Match: *\r\n",
            false,
        ))
        .await;
    assert_eq!(unauthorized.status, 401);
    let missing = server
        .request(&put_request(
            "auth",
            &body("root.nika.yaml", 0.25),
            "",
            true,
        ))
        .await;
    assert_eq!(missing.status, 412);
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn bodies_paths_symlinks_and_secret_shapes_fail_closed() {
    let world = TestWorld::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = world.root.path().join("outside.nika.yaml");
        std::fs::write(&outside, "nika: outside\ntasks: {}\n").expect("outside");
        symlink(&outside, world.workflows.join("linked.nika.yaml")).expect("symlink");
    }
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let escape = server
        .request(&put_request(
            "escape",
            &body("../outside.nika.yaml", 0.25),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(escape.status, 422, "{}", escape.body);
    let absolute = server
        .request(&put_request(
            "absolute",
            &body("/tmp/outside.nika.yaml", 0.25),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(absolute.status, 422, "{}", absolute.body);
    #[cfg(unix)]
    {
        let symlinked = server
            .request(&put_request(
                "linked",
                &body("linked.nika.yaml", 0.25),
                "If-None-Match: *\r\n",
                true,
            ))
            .await;
        assert_eq!(symlinked.status, 422, "{}", symlinked.body);
    }
    let secret = json!({
        "workflow": "root.nika.yaml",
        "when": {"kind": "once", "at": "2099-09-01T07:00:00Z"},
        "maxCostUsd": 0.25,
        "missed": "skip",
        "secrets": {"token": "never-persist-this"}
    })
    .to_string();
    let refused = server
        .request(&put_request(
            "secret",
            &secret,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(refused.status, 422);
    let oversized = "x".repeat(9 * 1024);
    let too_large = server
        .request(&put_request(
            "large",
            &oversized,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(too_large.status, 413);
    let persisted =
        std::fs::read_to_string(world.state.join("schedules/state.json")).expect("schedule state");
    assert!(!persisted.contains("never-persist-this"));
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn lowering_requires_cost_and_refuses_arbitrary_vars() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let cases = [
        (
            "vars",
            json!({
                "workflow": "root.nika.yaml",
                "when": {"kind": "once", "at": "2099-09-01T07:00:00Z"},
                "maxCostUsd": 0.25,
                "missed": "skip",
                "vars": {"anything": "not accepted"}
            }),
        ),
        (
            "missing-cost",
            json!({
                "workflow": "root.nika.yaml",
                "when": {"kind": "once", "at": "2099-09-01T07:00:00Z"},
                "missed": "skip"
            }),
        ),
    ];
    for (id, value) in cases {
        let response = server
            .request(&put_request(
                id,
                &value.to_string(),
                "If-None-Match: *\r\n",
                true,
            ))
            .await;
        assert_eq!(response.status, 422, "{id}: {}", response.body);
    }
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn active_unplannable_schedule_is_refused_before_persistence() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let candidate = json!({
        "workflow": "root.nika.yaml",
        "when": {"kind": "once", "at": "2099-09-01T07:00:00Z"},
        "maxCostUsd": 0.25,
        "missed": "skip",
        "jitter": "hash"
    })
    .to_string();
    let refused = server
        .request(&put_request(
            "unplannable",
            &candidate,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(refused.json()["findings"][0]["code"], "schedule.jitter");
    let absent = server
        .request(&get_request("/v1/schedules/unplannable"))
        .await;
    assert_eq!(absent.status, 404, "schedule must not be persisted");
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn validation_and_fire_share_the_resident_workflow_root() {
    let world = TestWorld::new();
    let decoy = world.root.path().join("decoy-workflows");
    std::fs::create_dir(&decoy).expect("decoy root");
    std::fs::write(
        decoy.join("decoy.nika.yaml"),
        "nika: decoy\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n",
    )
    .expect("decoy workflow");
    let server = world
        .start_with_workflow_roots(Arc::new(NoopBackend), limits(), &world.workflows, &decoy)
        .await;
    let decoy_only = server
        .request(&put_request(
            "decoy",
            &body("decoy.nika.yaml", 0.25),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(decoy_only.status, 422, "{}", decoy_only.body);
    let resident = server
        .request(&put_request(
            "resident-root",
            &body("root.nika.yaml", 0.25),
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(resident.status, 200, "{}", resident.body);
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn no_schedule_list_delete_trigger_backfill_or_arm_routes_exist() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    for path in [
        "/v1/schedules",
        "/v1/schedules/id/trigger",
        "/v1/schedules/id/backfill",
        "/v1/arm",
    ] {
        assert_eq!(
            server.request(&get_request(path)).await.status,
            404,
            "{path}"
        );
    }
    let delete = format!(
        "DELETE /v1/schedules/id HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{}\r\n",
        auth_header()
    );
    assert_eq!(server.request(&delete).await.status, 404);
    server.stop().await.expect("stop");
}

/// A project beat due at 08:01 UTC daily: nothing is due at a 08:00:30 start,
/// the first slot arrives with the clock (the containment tests, #1351).
fn write_daily_beat(world: &TestWorld) {
    std::fs::write(
        world.workflows.join("nika.yaml"),
        "nika: proj\narm:\n  - workflow: root.nika.yaml\n    cadence: \"TZ=UTC 1 8 * * *\"\n    plafond: 0.25\n    manqué: sauter\n",
    )
    .expect("project nika.yaml");
}

fn write_project_beat(world: &TestWorld, extra: &str) {
    std::fs::write(
        world.workflows.join("nika.yaml"),
        format!(
            "nika: proj\narm:\n  - workflow: root.nika.yaml\n    cadence: \"TZ=UTC * * * * *\"\n    plafond: 0.25\n    manqué: sauter\n{extra}"
        ),
    )
    .expect("project nika.yaml");
}

async fn project_finding(server: &TestServer, id: &str) -> serde_json::Value {
    let response = server
        .request(&get_request(&format!("/v1/schedules/{id}")))
        .await;
    assert_eq!(response.status, 200, "{}", response.body);
    let body = response.json();
    assert_eq!(body["origin"], "project", "{body}");
    body["finding"].clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_beat_with_hash_jitter_surfaces_a_load_finding_and_never_fires() {
    let world = TestWorld::new();
    write_project_beat(&world, "    décalage: hash\n");
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    clock.wait_for_sleeps(1).await;
    let finding = project_finding(&server, "root").await;
    assert_eq!(finding["code"], "schedule.jitter", "{finding}");
    clock.advance_to("2026-09-01T08:03:00Z[UTC]");
    clock.wait_for_sleeps(2).await;
    assert_eq!(backend.calls(), 0, "a refused beat never fires");
    server.stop().await.expect("stop");
}

/// A broken `nika.yaml` edit retains the last valid registry: the beat keeps
/// firing, the retained schedule names the finding, a repaired file clears it
/// (#1351).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_broken_project_edit_retains_the_last_good_registry() {
    let world = TestWorld::new();
    write_daily_beat(&world);
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:30Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    clock.wait_for_sleeps(1).await;
    std::fs::write(world.workflows.join("nika.yaml"), "nika: [broken\n").expect("break the file");
    clock.advance_to("2026-09-01T08:01:01Z[UTC]");
    clock.wait_for_sleeps(2).await;
    let finding = project_finding(&server, "root").await;
    assert_eq!(finding["code"], "project.invalid", "{finding}");
    assert!(
        finding["detail"]
            .as_str()
            .is_some_and(|d| d.contains("retained")),
        "{finding}"
    );
    assert!(backend.calls() >= 1, "the retained beat keeps firing");
    write_daily_beat(&world);
    clock.advance_to("2026-09-01T08:02:01Z[UTC]");
    clock.wait_for_sleeps(3).await;
    let repaired = server.request(&get_request("/v1/schedules/root")).await;
    assert_eq!(
        repaired.status, 404,
        "a healthy project schedule carries no finding: {}",
        repaired.body
    );
    server.stop().await.expect("stop");
}

/// A fire-time admission failure (the workflow vanished after it was
/// scheduled) is a finding on that schedule and never fatal to the resident
/// (#1351).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fire_time_admission_failure_is_contained_to_the_schedule() {
    let world = TestWorld::new();
    write_daily_beat(&world);
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:30Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    clock.wait_for_sleeps(1).await;
    std::fs::remove_file(world.workflows.join("root.nika.yaml")).expect("the workflow vanishes");
    clock.advance_to("2026-09-01T08:01:01Z[UTC]");
    clock.wait_for_sleeps(2).await;
    let finding = project_finding(&server, "root").await;
    assert_eq!(finding["code"], "schedule.admission", "{finding}");
    assert_eq!(
        backend.calls(),
        0,
        "nothing fired without an admitted workflow"
    );
    let alive = server.request(&get_request("/v1/workflows")).await;
    assert_eq!(alive.status, 200, "the resident is alive: {}", alive.body);
    server
        .stop()
        .await
        .expect("a contained failure stops clean");
}

#[tokio::test(flavor = "multi_thread")]
async fn active_overlap_replace_is_refused_before_persistence() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let candidate = json!({
        "workflow": "root.nika.yaml",
        "when": {"kind": "once", "at": "2099-09-01T07:00:00Z"},
        "maxCostUsd": 0.25,
        "missed": "skip",
        "overlap": "replace"
    })
    .to_string();
    let refused = server
        .request(&put_request(
            "overlap-replace",
            &candidate,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(refused.json()["findings"][0]["code"], "schedule.overlap");
    assert!(
        refused.json()["findings"][0]["detail"]
            .as_str()
            .expect("finding detail")
            .contains("overlap=replace"),
        "{}",
        refused.body
    );
    let absent = server
        .request(&get_request("/v1/schedules/overlap-replace"))
        .await;
    assert_eq!(absent.status, 404, "schedule must not be persisted");
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn active_overlap_queue_is_refused_before_persistence() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let candidate = json!({
        "workflow": "root.nika.yaml",
        "when": {"kind": "once", "at": "2099-09-01T07:00:00Z"},
        "maxCostUsd": 0.25,
        "missed": "skip",
        "overlap": "queue"
    })
    .to_string();
    let refused = server
        .request(&put_request(
            "overlap-queue",
            &candidate,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(refused.json()["findings"][0]["code"], "schedule.overlap");
    assert!(
        refused.json()["findings"][0]["detail"]
            .as_str()
            .expect("finding detail")
            .contains("overlap=queue"),
        "{}",
        refused.body
    );
    let absent = server
        .request(&get_request("/v1/schedules/overlap-queue"))
        .await;
    assert_eq!(absent.status, 404, "schedule must not be persisted");
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn active_after_skip_on_completion_is_refused_before_persistence() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let candidate = json!({
        "workflow": "root.nika.yaml",
        "when": {"kind": "once", "at": "2099-09-01T07:00:00Z"},
        "maxCostUsd": 0.25,
        "missed": "skip",
        "overlap": "skip",
        "afterSkip": "on_completion"
    })
    .to_string();
    let refused = server
        .request(&put_request(
            "after-skip-completion",
            &candidate,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(refused.json()["findings"][0]["code"], "schedule.after-skip");
    assert!(
        refused.json()["findings"][0]["detail"]
            .as_str()
            .expect("finding detail")
            .contains("afterSkip=on_completion"),
        "{}",
        refused.body
    );
    let absent = server
        .request(&get_request("/v1/schedules/after-skip-completion"))
        .await;
    assert_eq!(absent.status, 404, "schedule must not be persisted");
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_beat_with_after_skip_on_completion_surfaces_a_load_finding() {
    let world = TestWorld::new();
    write_project_beat(&world, "    après_saut: à-complétion\n");
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    clock.wait_for_sleeps(1).await;
    let finding = project_finding(&server, "root").await;
    assert_eq!(finding["code"], "schedule.after-skip", "{finding}");
    assert!(
        finding["detail"]
            .as_str()
            .expect("finding detail")
            .contains("afterSkip=on_completion"),
        "{finding}"
    );
    assert_eq!(backend.calls(), 0, "a refused beat never fires");
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_beat_with_overlap_queue_surfaces_a_load_finding_and_never_fires() {
    let world = TestWorld::new();
    write_project_beat(&world, "    chevauchement: file\n");
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    clock.wait_for_sleeps(1).await;
    let finding = project_finding(&server, "root").await;
    assert_eq!(finding["code"], "schedule.overlap", "{finding}");
    assert!(
        finding["detail"]
            .as_str()
            .expect("finding detail")
            .contains("overlap=queue"),
        "{finding}"
    );
    assert_eq!(backend.calls(), 0, "a refused beat never fires");
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_beat_with_overlap_replace_surfaces_a_load_finding_and_never_fires() {
    let world = TestWorld::new();
    write_project_beat(&world, "    chevauchement: remplacer\n");
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    clock.wait_for_sleeps(1).await;
    let finding = project_finding(&server, "root").await;
    assert_eq!(finding["code"], "schedule.overlap", "{finding}");
    assert!(
        finding["detail"]
            .as_str()
            .expect("finding detail")
            .contains("overlap=replace"),
        "{finding}"
    );
    assert_eq!(backend.calls(), 0, "a refused beat never fires");
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn active_tolerance_is_refused_before_persistence() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(NoopBackend), limits()).await;
    let candidate = json!({
        "workflow": "root.nika.yaml",
        "when": {"kind": "once", "at": "2099-09-01T07:00:00Z"},
        "maxCostUsd": 0.25,
        "missed": "skip",
        "tolerance": "3/4"
    })
    .to_string();
    let refused = server
        .request(&put_request(
            "tolerance",
            &candidate,
            "If-None-Match: *\r\n",
            true,
        ))
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(refused.json()["findings"][0]["code"], "schedule.tolerance");
    assert!(
        refused.json()["findings"][0]["detail"]
            .as_str()
            .expect("finding detail")
            .contains("tolerance"),
        "{}",
        refused.body
    );
    let absent = server
        .request(&get_request("/v1/schedules/tolerance"))
        .await;
    assert_eq!(absent.status, 404, "schedule must not be persisted");
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_beat_with_tolerance_surfaces_a_load_finding_and_never_fires() {
    let world = TestWorld::new();
    write_project_beat(&world, "    tolérance: \"3/4\"\n");
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    clock.wait_for_sleeps(1).await;
    let finding = project_finding(&server, "root").await;
    assert_eq!(finding["code"], "schedule.tolerance", "{finding}");
    assert!(
        finding["detail"]
            .as_str()
            .expect("finding detail")
            .contains("tolerance"),
        "{finding}"
    );
    assert_eq!(backend.calls(), 0, "a refused beat never fires");
    server.stop().await.expect("stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_project_beat_whose_pause_until_passed_fires_and_a_bound_one_stays_paused() {
    let world = TestWorld::new();
    std::fs::write(
        world.workflows.join("report.nika.yaml"),
        "nika: report\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 2, expression: \".\" }\n",
    )
    .expect("report workflow");
    std::fs::write(
        world.workflows.join("nika.yaml"),
        concat!(
            "nika: proj\narm:\n",
            "  - workflow: root.nika.yaml\n",
            "    cadence: \"TZ=UTC * * * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "    actif: false\n",
            "    raison: \"maintenance\"\n",
            "    jusqu_au: \"2026-08-31\"\n",
            "  - workflow: report.nika.yaml\n",
            "    cadence: \"TZ=UTC * * * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "    actif: false\n",
            "    raison: \"maintenance\"\n",
            "    jusqu_au: \"2026-09-10\"\n",
        ),
    )
    .expect("project nika.yaml");
    let backend = Arc::new(CountingBackend::default());
    let clock = Arc::new(ManualClock::new("2026-09-01T08:00:00Z[UTC]"));
    let server = world
        .start_with_clock(backend.clone(), limits(), clock.clone())
        .await;
    backend.wait_for_call().await;
    clock.wait_for_sleeps(2).await;
    assert_eq!(backend.calls(), 1, "only the woke beat fires");
    assert!(
        backend.root_bytes().is_some_and(|bytes| bytes
            .windows(b"input: 1".len())
            .any(|part| part == b"input: 1")),
        "the fired run captured the root workflow"
    );
    server.stop().await.expect("stop");
}
