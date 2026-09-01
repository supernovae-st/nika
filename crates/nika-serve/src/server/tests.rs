// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::future::{Future, pending};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::oneshot;

use super::route::SNAPSHOT_WIRE_UNIT_CEILING;
use super::test_support::assert_allowlisted;
use super::*;
use crate::{MAX_EXECUTION_SNAPSHOT_METADATA_BYTES, MAX_EXECUTION_SNAPSHOT_PATH_BYTES};

pub(super) const TOKEN: &str = "remote-test-token-012345678901234567890123456789";
const WORKFLOW: &str = "nika: root\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n";

pub(super) struct TestWorld {
    pub(super) root: tempfile::TempDir,
    pub(super) workflows: PathBuf,
    pub(super) state: PathBuf,
    pub(super) token: PathBuf,
}

impl TestWorld {
    pub(super) fn new() -> Self {
        let root = tempfile::tempdir().expect("test root");
        let workflows = root.path().join("workflows");
        let state = root.path().join("state");
        let token = root.path().join("serve.token");
        std::fs::create_dir(&workflows).expect("workflow root");
        std::fs::create_dir(&state).expect("state root");
        std::fs::write(workflows.join("root.nika.yaml"), WORKFLOW).expect("workflow");
        std::fs::write(&token, format!("{TOKEN}\n")).expect("token");
        secure_file(&token);
        Self {
            root,
            workflows,
            state,
            token,
        }
    }

    #[rustfmt::skip]
    pub(super) async fn start(&self, backend: Arc<dyn ExecutionBackend>, limits: ServerLimits) -> TestServer {
        self.start_with_snapshot_limits(backend, limits, SnapshotLimits::default()).await
    }

    #[rustfmt::skip]
    async fn start_with_snapshot_limits(&self, backend: Arc<dyn ExecutionBackend>, limits: ServerLimits, snapshot_limits: SnapshotLimits) -> TestServer {
        let resident = ResidentConfig::new(&self.state)
            .with_limits(limits)
            .with_snapshot_limits(snapshot_limits);
        self.start_with_config(backend, resident).await
    }

    #[rustfmt::skip]
    pub(super) async fn start_with_clock(&self, backend: Arc<dyn ExecutionBackend>, limits: ServerLimits, clock: Arc<dyn super::ResidentClock>) -> TestServer {
        let resident = ResidentConfig::new(&self.state)
            .with_limits(limits)
            .with_clock(clock);
        self.start_with_config(backend, resident).await
    }

    #[rustfmt::skip]
    pub(super) async fn start_with_workflow_roots(&self, backend: Arc<dyn ExecutionBackend>, limits: ServerLimits, resident_root: &std::path::Path, http_root: &std::path::Path) -> TestServer {
        let resident = ResidentConfig::new(&self.state)
            .with_limits(limits)
            .with_workflow_root(resident_root);
        self.start_with_config_and_http_root(backend, resident, http_root).await
    }

    async fn start_with_config(
        &self,
        backend: Arc<dyn ExecutionBackend>,
        resident: ResidentConfig,
    ) -> TestServer {
        self.start_with_config_and_http_root(backend, resident, &self.workflows)
            .await
    }

    async fn start_with_config_and_http_root(
        &self,
        backend: Arc<dyn ExecutionBackend>,
        resident: ResidentConfig,
        http_root: &std::path::Path,
    ) -> TestServer {
        let authority = ResidentAuthority::open(resident, backend)
            .await
            .expect("authority");
        let config = ServerConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            http_root,
            &self.token,
        );
        let bound = BoundServer::attach(config, &authority).await.expect("bind");
        let address = bound.local_addr().expect("local address");
        let shutdown_probe = authority.state.store.shutdown_test_probe();
        let shutdown_observer = Arc::clone(&shutdown_probe);
        let (shutdown, receiver) = oneshot::channel();
        let join = tokio::spawn(authority.serve_with_http(bound, async move {
            let _result = receiver.await;
            shutdown_observer.mark_shutdown_loop_observed();
        }));
        TestServer {
            address,
            shutdown: Some(shutdown),
            join,
            shutdown_probe,
        }
    }
}

pub(super) struct TestServer {
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<Result<(), ServerError>>,
    shutdown_probe: Arc<super::store::ShutdownTestProbe>,
}

impl TestServer {
    pub(super) async fn request(&self, request: &str) -> WireResponse {
        wire_request(self.address, request).await
    }

    pub(super) async fn stop(mut self) -> Result<(), ServerError> {
        self.shutdown.take().expect("shutdown sender").send(()).ok();
        self.join.await.expect("server join")
    }

    pub(super) fn shutdown_probe(&self) -> Arc<super::store::ShutdownTestProbe> {
        Arc::clone(&self.shutdown_probe)
    }

    pub(super) fn signal_stop(mut self) -> tokio::task::JoinHandle<Result<(), ServerError>> {
        self.shutdown.take().expect("shutdown sender").send(()).ok();
        self.join
    }
}

#[derive(Debug)]
struct TestBackend {
    calls: AtomicUsize,
    disposition: ExecutionDisposition,
    hang: bool,
}

#[derive(Debug)]
struct GatedBackend {
    calls: AtomicUsize,
    active: AtomicUsize,
    peak: AtomicUsize,
    permits: tokio::sync::Semaphore,
}

impl GatedBackend {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            permits: tokio::sync::Semaphore::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    fn release(&self, count: usize) {
        self.permits.add_permits(count);
    }
}

impl ExecutionBackend for GatedBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(active, Ordering::SeqCst);
            let permit = self.permits.acquire().await.expect("gate remains open");
            permit.forget();
            self.active.fetch_sub(1, Ordering::SeqCst);
            ExecutionDisposition::Succeeded.into()
        })
    }
}

impl TestBackend {
    fn completes(disposition: ExecutionDisposition) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            disposition,
            hang: false,
        }
    }

    fn hangs() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            disposition: ExecutionDisposition::Failed,
            hang: true,
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ExecutionBackend for TestBackend {
    fn execute<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                context
                    .snapshot()
                    .text(context.snapshot().root())
                    .expect("captured root"),
                WORKFLOW
            );
            if self.hang {
                pending::<ExecutionOutcome>().await
            } else {
                self.disposition.into()
            }
        })
    }
}

pub(super) fn limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(2),
        Duration::from_millis(200),
        4,
        16,
        64,
        32,
    )
}

#[test]
fn store_control_queue_tracks_bounded_http_fan_in() {
    let limits = limits();
    assert_eq!(store_control_capacity(limits), limits.max_connections());
}

fn short_shutdown_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(30),
        Duration::from_millis(20),
        2,
        8,
        64,
        32,
    )
}

fn short_request_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_millis(30),
        Duration::from_secs(2),
        Duration::from_millis(200),
        2,
        8,
        64,
        32,
    )
}

fn short_execution_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_millis(30),
        Duration::from_millis(200),
        2,
        8,
        64,
        32,
    )
}

fn one_connection_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_millis(200),
        2,
        8,
        1,
        32,
    )
}

fn bounded_queue_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(2),
        Duration::from_millis(200),
        2,
        2,
        16,
        32,
    )
}

fn one_active_queue_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(2),
        Duration::from_millis(200),
        1,
        4,
        16,
        32,
    )
}

fn one_job_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(2),
        Duration::from_millis(200),
        1,
        1,
        8,
        32,
    )
    .with_max_jobs(1)
}

fn four_header_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(2),
        Duration::from_millis(200),
        1,
        1,
        8,
        4,
    )
}

#[cfg(test)]
mod budget;
#[cfg(test)]
mod cancel_race;
#[cfg(test)]
mod request_lifecycle;

#[tokio::test(flavor = "multi_thread")]
async fn workflows_list_and_metadata_are_authenticated_and_contained() {
    let world = TestWorld::new();
    std::fs::create_dir(world.workflows.join("nested")).expect("nested dir");
    std::fs::write(
        world.workflows.join("nested/child.nika.yaml"),
        WORKFLOW.replace("nika: root", "nika: child"),
    )
    .expect("nested workflow");
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;

    let unauth = server
        .request("GET /v1/workflows HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await;
    assert_eq!(unauth.status, 401);
    let listed = server.request(&get_request("/v1/workflows")).await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    let names = listed.json()["workflows"]
        .as_array()
        .expect("workflow list")
        .iter()
        .map(|value| value.as_str().expect("name").to_owned())
        .collect::<Vec<_>>();
    assert!(names.contains(&"root.nika.yaml".to_owned()), "{names:?}");
    assert!(
        names.contains(&"nested/child.nika.yaml".to_owned()),
        "{names:?}"
    );
    let meta = server
        .request(&get_request("/v1/workflows/nested/child.nika.yaml"))
        .await;
    assert_eq!(meta.status, 200);
    assert_eq!(meta.json()["workflow"], "nested/child.nika.yaml");
    let traversal = server
        .request(&get_request("/v1/workflows/../root.nika.yaml"))
        .await;
    assert_eq!(traversal.status, 404);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_idempotency_key_and_deep_json_refuse_without_execution() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let missing = format!(
        "POST /v1/jobs HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 2\r\n{}\r\n{{}}",
        auth_header()
    );
    let missing = server.request(&missing).await;
    assert_eq!(missing.status, 400);
    assert_eq!(missing.json()["error"]["code"], "invalid_idempotency_key");

    let mut deep = String::from("{\"workflow\":");
    for _ in 0..128 {
        deep.push('[');
    }
    deep.push_str("\"root.nika.yaml\"");
    for _ in 0..128 {
        deep.push(']');
    }
    deep.push('}');
    let deep = server
        .request(&post_request(&deep, "deep-json", &auth_header()))
        .await;
    assert_eq!(deep.status, 422);
    assert_eq!(deep.json()["error"]["code"], "malformed_snapshot");
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

async fn wait_for_calls(backend: &TestBackend, expected: usize) -> Result<(), String> {
    for _ in 0..200 {
        if backend.calls() >= expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!("backend never reached {expected} calls"))
}

fn one_sse_limits() -> ServerLimits {
    limits().with_max_sse_clients(1)
}

fn live_sse_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_millis(500),
        Duration::from_secs(2),
        Duration::from_millis(200),
        2,
        8,
        64,
        32,
    )
    .with_sse_timing(Duration::from_millis(20), Duration::from_secs(1))
}

#[tokio::test(flavor = "multi_thread")]
async fn job_events_are_authenticated_allowlisted_and_resumable() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "sse-complete",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("succeeded");

    let unauth = server
        .request(&format!(
            "GET /v1/jobs/{id}/events HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        ))
        .await;
    assert_eq!(unauth.status, 401, "{}", unauth.body);
    assert!(!unauth.body.contains(TOKEN));

    let streamed = server.request(&events_request(&id, None)).await;
    assert_eq!(streamed.status, 200, "{}", streamed.body);
    assert!(
        streamed
            .headers
            .to_ascii_lowercase()
            .contains("content-type: text/event-stream")
    );
    let events = parse_sse_data(&streamed.body);
    assert!(!events.is_empty(), "{}", streamed.body);
    for event in &events {
        assert_allowlisted(event);
    }
    assert_eq!(events[0]["sequence"], 1);

    let resumed = server.request(&events_request(&id, Some("1"))).await;
    let resumed_events = parse_sse_data(&resumed.body);
    assert!(
        resumed_events.iter().all(|event| event["sequence"] != 1),
        "{}",
        resumed.body
    );

    let future = server.request(&events_request(&id, Some("99"))).await;
    assert_eq!(future.status, 400, "{}", future.body);
    assert_eq!(future.json()["error"]["code"], "cursor_beyond_latest");

    for cursor in ["nope", "01", "+1", "1.5", ""] {
        let invalid = server.request(&events_request(&id, Some(cursor))).await;
        assert_eq!(invalid.status, 400, "{cursor}: {}", invalid.body);
        assert_eq!(invalid.json()["error"]["code"], "invalid_cursor");
    }
    let duplicate = format!(
        "GET /v1/jobs/{id}/events HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{}Last-Event-ID: 1\r\nLast-Event-ID: 1\r\n\r\n",
        auth_header()
    );
    let duplicate = server.request(&duplicate).await;
    assert_eq!(duplicate.status, 400);
    assert_eq!(duplicate.json()["error"]["code"], "invalid_cursor");

    let cancel = server
        .request(&get_request(&format!("/v1/jobs/{id}/cancel")))
        .await;
    let artifacts = server
        .request(&get_request(&format!("/v1/jobs/{id}/artifacts")))
        .await;
    assert_eq!(cancel.status, 404);
    assert_eq!(artifacts.status, 404);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_outlives_request_timeout_and_disconnect_does_not_block_execution() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let server = world.start(backend.clone(), live_sse_limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "sse-live",
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "running")
        .await
        .expect("running");
    let running = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert!(running["execution_id"].as_str().is_some(), "{running}");
    assert!(running["trace_id"].as_str().is_some(), "{running}");
    assert!(running.get("outputs").is_none(), "{running}");
    assert!(running.get("receipt").is_none(), "{running}");
    let durable: Value = serde_json::from_slice(
        &std::fs::read(world.state.join("jobs/state.json")).expect("read running state"),
    )
    .expect("decode running state");
    let durable_job = durable["jobs"]
        .as_array()
        .expect("durable jobs")
        .iter()
        .find(|job| job["record"]["id"].as_str() == Some(id.as_str()))
        .expect("durable running job");
    assert_eq!(durable_job["record"]["status"], "running");
    assert_eq!(
        durable_job["record"]["snapshot_digest"]
            .as_str()
            .expect("durable snapshot digest")
            .len(),
        64
    );
    assert_eq!(
        durable_job["identity_digest"]
            .as_str()
            .expect("durable identity binding")
            .len(),
        64
    );

    let (mut stream, headers) = open_sse(server.address, &events_request(&id, None)).await;
    assert_eq!(headers.status, 200, "{}", headers.body);
    tokio::time::sleep(Duration::from_millis(600)).await;
    let events = collect_sse(&mut stream, headers.body, 1).await;
    assert_eq!(events[0]["kind"], "execution.started");
    assert_allowlisted(&events[0]);
    drop(stream);

    backend.release(1);
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("execution continued after sse drop");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_client_ceiling_refuses_the_next_stream() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let server = world.start(backend.clone(), one_sse_limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "sse-cap",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "running")
        .await
        .expect("running");

    let (stream, first) = open_sse(server.address, &events_request(&id, None)).await;
    assert_eq!(first.status, 200, "{}", first.body);
    let second = server.request(&events_request(&id, None)).await;
    assert_eq!(second.status, 503, "{}", second.body);
    assert_eq!(second.json()["error"]["code"], "sse_capacity");
    let status = server
        .request(&get_request(&format!("/v1/jobs/{id}/status")))
        .await;
    assert_eq!(status.status, 200);

    drop(stream);
    tokio::time::sleep(Duration::from_millis(30)).await;
    let (replacement, recovered) = open_sse(server.address, &events_request(&id, None)).await;
    assert_eq!(recovered.status, 200, "{}", recovered.body);
    drop(replacement);
    backend.release(1);
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("settled");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn interrupted_event_stream_redacts_payload_fields() {
    let world = TestWorld::new();
    let hanging = Arc::new(TestBackend::hangs());
    let first = world.start(hanging.clone(), short_shutdown_limits()).await;
    let created = first
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "sse-redact",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&first, &id, "running")
        .await
        .expect("running");
    wait_for_calls(hanging.as_ref(), 1)
        .await
        .expect("hanging backend entered execute");
    assert!(matches!(
        first.stop().await,
        Err(ServerError::ShutdownTimeout)
    ));

    let replacement = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let second = world.start(replacement, limits()).await;
    wait_for_status(&second, &id, "interrupted")
        .await
        .expect("interrupted");
    let streamed = second.request(&events_request(&id, None)).await;
    assert_eq!(streamed.status, 200, "{}", streamed.body);
    let events = parse_sse_data(&streamed.body);
    assert!(!events.is_empty(), "{}", streamed.body);
    for event in &events {
        assert_allowlisted(event);
        assert!(event.get("incarnation_generation").is_none());
        assert!(event.get("previous_incarnation_generation").is_none());
        assert!(!streamed.body.contains(TOKEN));
        assert!(!streamed.body.contains("/jobs"));
    }
    let job = second
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(job["status"], "interrupted");
    assert_eq!(job["receipt"]["job_id"], id);
    assert_eq!(job["receipt"]["execution_id"], job["execution_id"]);
    assert_eq!(job["receipt"]["trace_id"], job["trace_id"]);
    assert_eq!(
        job["receipt"]["snapshot_digest"]
            .as_str()
            .expect("snapshot digest")
            .len(),
        64
    );
    assert!(job.get("outputs").is_none());
    let terminal = events
        .iter()
        .find(|event| event["status"] == "interrupted")
        .expect("interrupted event");
    assert_eq!(terminal["receipt"], job["receipt"]);
    assert!(terminal.get("outputs").is_none());
    second.stop().await.expect("clean stop");
}

#[test]
fn missing_wire_boundary_diagnostic_omits_request_headers() {
    let request = format!("POST /v1/jobs/cancel HTTP/1.1\r\nAuthorization: Bearer {TOKEN}\r\n\r\n");
    let address = "127.0.0.1:1".parse().expect("test address");
    let panic = std::panic::catch_unwind(|| assert_http_response_boundary(address, &request, &[]))
        .expect_err("missing boundary must panic");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("string panic");

    assert!(message.contains("request=\"POST /v1/jobs/cancel HTTP/1.1\""));
    assert!(message.contains("bytes=0"));
    assert!(!message.contains(TOKEN));
    assert!(!message.contains("Authorization"));
}

#[tokio::test(flavor = "multi_thread")]
async fn queued_cancel_mints_identity_and_receipt_without_entering_the_backend() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let server = world
        .start(backend.clone(), one_active_queue_limits())
        .await;
    let first = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "cancel-queue-blocker",
            &auth_header(),
        ))
        .await;
    let first_id = first.json()["id"].as_str().expect("first id").to_owned();
    wait_for_status(&server, &first_id, "running")
        .await
        .expect("first running");

    let queued = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "cancel-queued",
            &auth_header(),
        ))
        .await;
    let queued_id = queued.json()["id"].as_str().expect("queued id").to_owned();
    assert_eq!(queued.json()["status"], "queued");
    let cancelled = server.request(&cancel_request(&queued_id)).await;
    assert_eq!(cancelled.status, 200, "{}", cancelled.body);
    let body = cancelled.json();
    assert_eq!(body["status"], "cancelled");
    assert_eq!(body["receipt"]["job_id"], queued_id);
    assert_eq!(body["receipt"]["execution_id"], body["execution_id"]);
    assert_eq!(
        backend.calls(),
        1,
        "queued cancellation never enters backend"
    );

    backend.release(1);
    wait_for_status(&server, &first_id, "succeeded")
        .await
        .expect("first settled");
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(backend.calls(), 1, "cancelled queue row stays inert");
    server.stop().await.expect("clean stop");
    let replacement = world
        .start(
            Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded)),
            limits(),
        )
        .await;
    let durable = replacement
        .request(&get_request(&format!("/v1/jobs/{queued_id}")))
        .await
        .json();
    assert_eq!(durable["status"], "cancelled");
    assert_eq!(durable["receipt"], body["receipt"]);
    replacement.stop().await.expect("replacement stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_authenticates_before_job_lookup() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;
    let response = server
        .request(
            "POST /v1/jobs/not-an-id/cancel HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        )
        .await;
    assert_eq!(response.status, 401);
    assert_eq!(response.json()["error"]["code"], "unauthorized");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_after_terminal_settlement_is_a_read_only_replay() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "cancel-after-terminal",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("succeeded");
    let before = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    let replay = server.request(&cancel_request(&id)).await;
    assert_eq!(replay.status, 200, "{}", replay.body);
    assert_eq!(replay.json(), before);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn sse_timing_bounds_refuse_before_state_io() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    for reconnect in [Duration::from_millis(99), Duration::from_millis(30_001)] {
        let config = ResidentConfig::new(&world.state)
            .with_limits(limits().with_sse_timing(Duration::from_secs(1), reconnect));
        assert!(matches!(
            ResidentAuthority::open(config, backend.clone()).await,
            Err(ServerError::InvalidConfig(_))
        ));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn live_sse_advertises_bounded_reconnect_and_heartbeats_without_advancing_cursor() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let server = world.start(backend.clone(), live_sse_limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "sse-heartbeat",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "running")
        .await
        .expect("running");

    let (mut stream, headers) = open_sse(server.address, &events_request(&id, Some("1"))).await;
    assert_eq!(headers.status, 200, "{}", headers.body);
    let mut body = headers.body;
    for _ in 0..100 {
        if body.contains("retry: ") && body.contains(": heartbeat\n\n") {
            break;
        }
        let mut chunk = [0; 512];
        if let Ok(Ok(n)) =
            tokio::time::timeout(Duration::from_millis(20), stream.read(&mut chunk)).await
        {
            if n == 0 {
                break;
            }
            body.push_str(std::str::from_utf8(&chunk[..n]).expect("utf8"));
        }
    }
    assert!(body.contains("retry: 1000\n\n"), "{body:?}");
    assert!(body.contains(": heartbeat\n\n"), "{body:?}");
    assert!(
        !body.contains("id: 1\n"),
        "heartbeat cannot advance replay cursor: {body:?}"
    );

    drop(stream);
    backend.release(1);
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("settled");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn trace_verification_refuses_honestly_without_a_remote_trace_authority() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "trace-verdict",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("settled");

    let verdict = server
        .request(&get_request(&format!("/v1/jobs/{id}/trace/verify")))
        .await;
    assert_eq!(verdict.status, 200, "{}", verdict.body);
    let body = verdict.json();
    assert_eq!(body["verdict"], "unavailable");
    assert_eq!(body["reason"], "trace_journal_unavailable");
    assert_eq!(body["trace_id"].as_str().map(str::len), Some(32));
    assert!(
        !verdict
            .body
            .contains(world.root.path().to_string_lossy().as_ref())
    );
    assert!(!verdict.body.contains(".nika/traces"));
    server.stop().await.expect("clean stop");
}

pub(super) async fn wait_for_status(
    server: &TestServer,
    id: &str,
    expected: &str,
) -> Result<(), String> {
    for _ in 0..200 {
        let response = server
            .request(&get_request(&format!("/v1/jobs/{id}/status")))
            .await;
        if response.status == 200 && response.json()["status"] == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!("job {id} never reached {expected}"))
}

async fn wire_request(address: SocketAddr, request: &str) -> WireResponse {
    let request_line = request.lines().next().unwrap_or("<empty>");
    let stream = tokio::net::TcpStream::connect(address).await;
    assert!(
        stream.is_ok(),
        "HTTP connect failed: peer={address} request={request_line:?}: {:?}",
        stream.as_ref().err()
    );
    let mut stream = stream.expect("connect result checked");
    let written = stream.write_all(request.as_bytes()).await;
    assert!(
        written.is_ok(),
        "HTTP write failed: peer={address} request={request_line:?}: {:?}",
        written.as_ref().err()
    );
    let mut response = Vec::new();
    let read = stream.read_to_end(&mut response).await;
    assert!(
        read.is_ok(),
        "HTTP read failed: peer={address} request={request_line:?}: {:?}",
        read.as_ref().err()
    );
    assert_http_response_boundary(address, request, &response);
    WireResponse::parse(&response)
}

fn assert_http_response_boundary(address: SocketAddr, request: &str, response: &[u8]) {
    let request_line = request.lines().next().unwrap_or("<empty>");
    let prefix_len = response.len().min(256);
    let prefix = String::from_utf8_lossy(&response[..prefix_len])
        .escape_default()
        .to_string();
    assert!(
        response.windows(4).any(|window| window == b"\r\n\r\n"),
        "HTTP response boundary missing: peer={address} request={request_line:?} bytes={} prefix={prefix:?}",
        response.len()
    );
}

pub(super) fn post_request(body: &str, key: &str, authorization: &str) -> String {
    let body = if body == r#"{"workflow":"root.nika.yaml"}"# {
        snapshot_body(WORKFLOW)
    } else {
        body.to_owned()
    };
    format!(
        "POST /v1/jobs HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\nIdempotency-Key: {key}\r\n{authorization}\r\n{body}",
        body.len()
    )
}

pub(super) fn snapshot_body(source: &str) -> String {
    let path = "root.nika.yaml";
    let unit_digest = hex_digest(source.as_bytes());
    let mut world = Sha256::new();
    world.update(b"nika-execution-snapshot\0");
    world.update(1_u32.to_be_bytes());
    hash_snapshot_field(&mut world, path.as_bytes());
    world.update([0]);
    hash_snapshot_field(&mut world, path.as_bytes());
    hash_snapshot_field(&mut world, source.as_bytes());
    json!({
        "format_version": 1,
        "root": path,
        "digest": format!("{:x}", world.finalize()),
        "units": [{
            "path": path,
            "kind": 0,
            "digest": unit_digest,
            "bytes_hex": encode_hex(source.as_bytes())
        }]
    })
    .to_string()
}

fn hash_snapshot_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_be_bytes());
    hasher.update(bytes);
}

fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

pub(super) fn get_request(path: &str) -> String {
    format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{}\r\n",
        auth_header()
    )
}

fn cancel_request(id: &str) -> String {
    format!(
        "POST /v1/jobs/{id}/cancel HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: 0\r\n{}\r\n",
        auth_header()
    )
}

fn check_request(body: &str) -> String {
    format!(
        "POST /v1/check HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n{body}",
        body.len(),
        auth_header()
    )
}

pub(super) fn events_request(id: &str, last_event_id: Option<&str>) -> String {
    let last = last_event_id
        .map(|cursor| format!("Last-Event-ID: {cursor}\r\n"))
        .unwrap_or_default();
    format!(
        "GET /v1/jobs/{id}/events HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{}{last}\r\n",
        auth_header()
    )
}

pub(super) fn parse_sse_data(body: &str) -> Vec<Value> {
    let mut events = Vec::new();
    for block in body.split("\n\n") {
        for line in block.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                events.push(serde_json::from_str(data).expect("sse json"));
            }
        }
    }
    events
}

async fn open_sse(address: SocketAddr, request: &str) -> (tokio::net::TcpStream, WireResponse) {
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect");
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut buf = Vec::new();
    while !buf.windows(4).any(|window| window == b"\r\n\r\n") {
        let mut chunk = [0; 512];
        let n = stream.read(&mut chunk).await.expect("header read");
        assert!(n > 0, "eof before headers");
        buf.extend_from_slice(&chunk[..n]);
    }
    (stream, WireResponse::parse(&buf))
}

async fn collect_sse(
    stream: &mut tokio::net::TcpStream,
    mut body: String,
    min: usize,
) -> Vec<Value> {
    for _ in 0..200 {
        let events = parse_sse_data(&body);
        if events.len() >= min {
            return events;
        }
        let mut chunk = [0; 512];
        match tokio::time::timeout(Duration::from_millis(20), stream.read(&mut chunk)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => body.push_str(std::str::from_utf8(&chunk[..n]).expect("utf8")),
            _ => {}
        }
    }
    parse_sse_data(&body)
}

pub(super) fn auth_header() -> String {
    format!("Authorization: Bearer {TOKEN}\r\n")
}

#[derive(Debug)]
pub(super) struct WireResponse {
    pub(super) status: u16,
    headers: String,
    pub(super) body: String,
}

impl WireResponse {
    fn parse(bytes: &[u8]) -> Self {
        let response = String::from_utf8(bytes.to_vec()).expect("UTF-8 HTTP response");
        let (headers, body) = response
            .split_once("\r\n\r\n")
            .expect("HTTP response boundary");
        let status = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse().ok())
            .expect("HTTP status");
        Self {
            status,
            headers: headers.to_owned(),
            body: body.to_owned(),
        }
    }

    pub(super) fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("JSON response")
    }

    pub(super) fn header(&self, name: &str) -> Option<&str> {
        self.headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then_some(value.trim())
        })
    }

    fn challenge(&self) -> bool {
        self.headers
            .to_ascii_lowercase()
            .contains("www-authenticate: bearer")
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn openapi_is_authenticated_and_lists_only_real_authorities() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;
    let unauth = server
        .request("GET /v1/openapi.json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await;
    assert_eq!(unauth.status, 401);
    assert!(!unauth.body.contains(TOKEN));
    let spec = server.request(&get_request("/v1/openapi.json")).await;
    assert_eq!(spec.status, 200, "{}", spec.body);
    let body = spec.json();
    assert_eq!(body, super::openapi::document());
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"].get("/v1/jobs").is_some());
    assert!(body["paths"].get("/v1/jobs/{id}/events").is_some());
    assert!(body["paths"].get("/v1/jobs/{id}/cancel").is_some());
    assert!(body["paths"].get("/v1/jobs/{id}/trace/verify").is_some());
    assert!(body["paths"].get("/v1/jobs/{id}/artifacts").is_none());
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn machine_check_and_job_admission_judge_identical_caller_bytes_not_server_paths() {
    let world = TestWorld::new();
    std::fs::write(
        world.workflows.join("root.nika.yaml"),
        "nika: divergent-server-copy\ntasks: {}\n",
    )
    .expect("divergent server-local path");
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let snapshot = snapshot_body(WORKFLOW);

    let checked = server.request(&check_request(&snapshot)).await;
    assert_eq!(checked.status, 200, "{}", checked.body);
    assert_eq!(checked.json()["status"], "accepted");

    let created = server
        .request(&post_request(&snapshot, "bytes-first", &auth_header()))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("job id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("caller snapshot executes");
    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(
        job["receipt"]["snapshot_digest"],
        checked.json()["snapshot_digest"]
    );
    assert_eq!(backend.calls(), 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn idempotency_is_bound_to_exact_snapshot_body_bytes() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let compact = snapshot_body(WORKFLOW);
    let pretty = serde_json::to_string_pretty(
        &serde_json::from_str::<Value>(&compact).expect("snapshot JSON"),
    )
    .expect("pretty snapshot");

    let first = server
        .request(&post_request(&compact, "same-world", &auth_header()))
        .await;
    let second = server
        .request(&post_request(&pretty, "same-world", &auth_header()))
        .await;
    let replay = server
        .request(&post_request(&compact, "same-world", &auth_header()))
        .await;
    assert_eq!(first.status, 202, "{}", first.body);
    assert_eq!(second.status, 409, "{}", second.body);
    assert_eq!(second.json()["error"]["code"], "idempotency_conflict");
    assert_eq!(replay.status, 200, "{}", replay.body);
    assert_eq!(first.json()["id"], replay.json()["id"]);
    let id = first.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("one execution");
    assert_eq!(backend.calls(), 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_wire_refusals_have_stable_typed_codes_and_never_execute() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let valid = serde_json::from_str::<Value>(&snapshot_body(WORKFLOW)).expect("snapshot");
    let mut cases = Vec::new();

    let mut hex = valid.clone();
    hex["units"][0]["bytes_hex"] = json!("0g");
    cases.push(("bad-hex", hex, "malformed_snapshot_hex"));
    let mut version = valid.clone();
    version["format_version"] = json!(2);
    cases.push(("bad-version", version, "unsupported_snapshot_version"));
    let mut tampered = valid;
    tampered["units"][0]["digest"] = json!("0".repeat(64));
    cases.push(("bad-digest", tampered, "snapshot_tampered"));

    for (key, payload, code) in cases {
        let response = server
            .request(&post_request(&payload.to_string(), key, &auth_header()))
            .await;
        assert_eq!(response.status, 422, "{key}: {}", response.body);
        assert_eq!(response.json()["error"]["code"], code, "{key}");
        assert!(response.json().get("id").is_none(), "{key}");
    }
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_resource_bounds_have_stable_typed_codes_and_never_execute() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), ServerLimits::default()).await;
    let mut long_path = serde_json::from_str::<Value>(&snapshot_body(WORKFLOW)).expect("snapshot");
    long_path["root"] = json!("p".repeat(MAX_EXECUTION_SNAPSHOT_PATH_BYTES + 1));
    let path_response = server
        .request(&post_request(
            &long_path.to_string(),
            "path-limit",
            &auth_header(),
        ))
        .await;
    assert_eq!(path_response.status, 413, "{}", path_response.body);
    assert_eq!(path_response.json()["error"]["code"], "snapshot_path_limit");

    let mut too_many = serde_json::from_str::<Value>(&snapshot_body(WORKFLOW)).expect("snapshot");
    let unit = too_many["units"][0].clone();
    too_many["units"] = Value::Array(vec![unit; SNAPSHOT_WIRE_UNIT_CEILING + 1]);
    let count_response = server
        .request(&post_request(
            &too_many.to_string(),
            "wire-count-limit",
            &auth_header(),
        ))
        .await;
    assert_eq!(count_response.status, 413, "{}", count_response.body);
    assert_eq!(
        count_response.json()["error"]["code"],
        "snapshot_unit_count_limit"
    );

    let compact = snapshot_body(WORKFLOW);
    let split = compact.len() - 1;
    let metadata_heavy = format!(
        "{}{}{}",
        &compact[..split],
        " ".repeat(MAX_EXECUTION_SNAPSHOT_METADATA_BYTES + 1),
        &compact[split..]
    );
    let metadata_response = server
        .request(&post_request(
            &metadata_heavy,
            "metadata-limit",
            &auth_header(),
        ))
        .await;
    assert_eq!(metadata_response.status, 413, "{}", metadata_response.body);
    assert_eq!(
        metadata_response.json()["error"]["code"],
        "snapshot_metadata_limit"
    );
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");

    for (name, snapshot_limits, expected) in [
        (
            "count",
            SnapshotLimits::new(64, 0, 1024 * 1024, 16 * 1024 * 1024),
            "snapshot_unit_count_limit",
        ),
        (
            "unit",
            SnapshotLimits::new(64, 256, 8, 16 * 1024 * 1024),
            "snapshot_unit_size_limit",
        ),
        (
            "aggregate",
            SnapshotLimits::new(64, 256, 1024 * 1024, 8),
            "snapshot_total_size_limit",
        ),
    ] {
        let world = TestWorld::new();
        let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
        let server = world
            .start_with_snapshot_limits(backend.clone(), limits(), snapshot_limits)
            .await;
        let response = server
            .request(&post_request(
                &snapshot_body(WORKFLOW),
                name,
                &auth_header(),
            ))
            .await;
        assert_eq!(response.status, 413, "{name}: {}", response.body);
        assert_eq!(response.json()["error"]["code"], expected, "{name}");
        assert_eq!(backend.calls(), 0);
        server.stop().await.expect("clean stop");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn post_admission_path_mutation_cannot_change_queued_world_across_restart() {
    let world = TestWorld::new();
    let hanging = Arc::new(GatedBackend::new());
    let first = world.start(hanging.clone(), bounded_queue_limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    let mut ids = Vec::new();
    for index in 0..4 {
        let response = first
            .request(&post_request(body, &format!("qr-{index}"), &auth_header()))
            .await;
        assert_eq!(response.status, 202, "{}", response.body);
        ids.push(response.json()["id"].as_str().expect("id").to_owned());
    }
    for _ in 0..200 {
        if hanging.calls() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(hanging.calls(), 2);
    assert!(matches!(
        first.stop().await,
        Err(ServerError::ShutdownTimeout)
    ));
    std::fs::write(world.workflows.join("root.nika.yaml"), "not-a-workflow")
        .expect("mutate server-local bytes after snapshot admission");
    let replacement = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let second = world.start(replacement.clone(), limits()).await;
    wait_for_calls(replacement.as_ref(), 2)
        .await
        .expect("queued pair");
    let mut succeeded = 0;
    let mut interrupted = 0;
    for id in &ids {
        let payload = second
            .request(&get_request(&format!("/v1/jobs/{id}/status")))
            .await
            .json();
        match payload["status"].as_str() {
            Some("succeeded") => succeeded += 1,
            Some("interrupted") => interrupted += 1,
            Some(other) => assert_eq!(other, "succeeded"),
            None => assert!(!payload["status"].is_null()),
        }
    }
    assert_eq!((interrupted, succeeded), (2, 2));
    second.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn first_run_creates_a_missing_state_root() {
    let world = TestWorld::new();
    std::fs::remove_dir_all(&world.state).expect("remove");
    let server = world
        .start(
            Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded)),
            limits(),
        )
        .await;
    assert!(world.state.is_dir());
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn queue_full_does_not_leave_a_durable_row() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let server = world.start(backend.clone(), bounded_queue_limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    for index in 0..4 {
        let response = server
            .request(&post_request(body, &format!("nl-{index}"), &auth_header()))
            .await;
        assert_eq!(response.status, 202, "{}", response.body);
    }
    let excess = server
        .request(&post_request(body, "nl-x", &auth_header()))
        .await;
    assert_eq!(excess.status, 503);
    assert!(excess.json().get("id").is_none());
    backend.release(4);
    let _stopped = server.stop().await;
}

#[cfg(unix)]
fn secure_file(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .expect("secure token mode");
}

#[cfg(not(unix))]
fn secure_file(_path: &std::path::Path) {}
