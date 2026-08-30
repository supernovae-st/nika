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
use tokio::task::JoinSet;

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
        let authority = ResidentAuthority::open(resident, backend).await.expect("authority");
        let config = ServerConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            &self.workflows,
            &self.token,
        );
        let bound = BoundServer::attach(config, &authority).await.expect("bind");
        let address = bound.local_addr().expect("local address");
        let (shutdown, receiver) = oneshot::channel();
        let join = tokio::spawn(authority.serve_with_http(bound, async move {
            let _result = receiver.await;
        }));
        TestServer {
            address,
            shutdown: Some(shutdown),
            join,
        }
    }
}

pub(super) struct TestServer {
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<Result<(), ServerError>>,
}

impl TestServer {
    pub(super) async fn request(&self, request: &str) -> WireResponse {
        wire_request(self.address, request).await
    }

    pub(super) async fn stop(mut self) -> Result<(), ServerError> {
        self.shutdown.take().expect("shutdown sender").send(()).ok();
        self.join.await.expect("server join")
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

#[tokio::test(flavor = "multi_thread")]
async fn health_is_public_and_contains_only_compile_bound_identity() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;

    let response = server
        .request("GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await;

    assert_eq!(response.status, 200);
    let body = response.json();
    let object = body.as_object().expect("health object");
    let fields = [
        "status",
        "service",
        "engine_version",
        "build_sha",
        "spec_sha",
        "api_version",
        "engineVersion",
        "buildSha",
        "specSha",
        "machineProtocolVersion",
        "snapshotFormatVersion",
        "checkReportVersion",
        "eventFormatVersion",
        "traceFormatVersion",
        "supportedCapabilities",
    ];
    assert_eq!(object.len(), fields.len(), "health field allowlist: {body}");
    for field in fields {
        assert!(object.contains_key(field), "missing {field}: {body}");
    }
    assert_eq!(
        body["supportedCapabilities"],
        json!(["check", "executionSnapshot", "eventStream"]),
        "HTTP advertises only its snapshot check, admission, and SSE subset"
    );
    assert!(
        !body["supportedCapabilities"]
            .as_array()
            .expect("capability array")
            .iter()
            .any(|capability| capability == "trace"),
        "trace format identity must not claim a trace route"
    );
    assert!(
        !response
            .headers
            .to_ascii_lowercase()
            .contains("access-control")
    );
    assert!(
        !response
            .body
            .contains(world.workflows.to_string_lossy().as_ref())
    );
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_invalid_credential_has_one_uniform_bounded_401_shape() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    let variants = [
        String::new(),
        "Authorization: Basic nope\r\n".to_owned(),
        "Authorization: Bearer wrong-wrong-wrong-wrong-wrong-wrong\r\n".to_owned(),
        format!("Authorization: Bearer {}\r\n", "x".repeat(513)),
        format!("Authorization: Bearer {TOKEN}\r\nAuthorization: Bearer {TOKEN}\r\n"),
    ];
    let mut signatures = Vec::new();
    for headers in variants {
        let request = post_request(body, "uniform-auth", &headers);
        let response = server.request(&request).await;
        let challenge = response.challenge();
        signatures.push((response.status, response.body, challenge));
    }

    assert!(
        signatures
            .iter()
            .all(|signature| signature == &signatures[0])
    );
    assert_eq!(signatures[0].0, 401);
    assert!(signatures[0].1.len() < 160);
    assert!(signatures[0].2);
    assert!(!signatures[0].1.contains(TOKEN));
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn unauthenticated_invalid_json_is_refused_before_admission() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let request = post_request("{ definitely not json", "auth-before-parse", "");

    let response = server.request(&request).await;

    assert_eq!(response.status, 401);
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn authenticated_invalid_json_and_content_type_are_stable_refusals() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let malformed = server
        .request(&post_request(
            "{ definitely not json",
            "invalid-json",
            &auth_header(),
        ))
        .await;
    let wrong_type = format!(
        "POST /v1/jobs HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: 2\r\nIdempotency-Key: wrong-type\r\n{}\r\n{{}}",
        auth_header()
    );
    let wrong_type = server.request(&wrong_type).await;
    let compressed = format!(
        "POST /v1/jobs HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Encoding: gzip\r\nContent-Length: 2\r\nIdempotency-Key: compressed\r\n{}\r\n{{}}",
        auth_header()
    );
    let compressed = server.request(&compressed).await;

    assert_eq!(malformed.status, 422);
    assert_eq!(malformed.json()["error"]["code"], "malformed_snapshot");
    assert_eq!(wrong_type.status, 415);
    assert_eq!(wrong_type.json()["error"]["code"], "unsupported_media_type");
    assert_eq!(compressed.status, 415);
    assert_eq!(
        compressed.json()["error"]["code"],
        "unsupported_content_encoding"
    );
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn create_read_and_status_use_real_loopback_and_execution_service() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;

    let created = server
        .request(&post_request(body, "create-read", &auth_header()))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("job id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("succeeded status");

    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await;
    let status = server
        .request(&get_request(&format!("/v1/jobs/{id}/status")))
        .await;
    assert_eq!(job.status, 200);
    let job_body = job.json();
    assert_eq!(job_body["id"], id);
    let execution_id = job_body["execution_id"].as_str().expect("execution_id");
    let trace_id = job_body["trace_id"].as_str().expect("trace_id");
    assert!(execution_id.starts_with("exe-"), "{execution_id}");
    assert_eq!(trace_id.len(), 32, "{trace_id}");
    assert_eq!(status.json(), json!({"status": "succeeded"}));
    assert_eq!(backend.calls(), 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn paused_is_preserved_by_both_job_response_types() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Paused));
    let server = world.start(backend, limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "paused-job",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "paused")
        .await
        .expect("paused status");

    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await;
    let status = server
        .request(&get_request(&format!("/v1/jobs/{id}/status")))
        .await;
    assert_eq!(job.json()["status"], "paused");
    assert_eq!(status.json()["status"], "paused");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_jobs_and_absent_authorities_disclose_one_bounded_shape() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;
    let random = JobId::random();
    let unknown = server
        .request(&get_request(&format!("/v1/jobs/{random}")))
        .await;
    let malformed = server.request(&get_request("/v1/jobs/not-a-job-id")).await;
    let cancel = server
        .request(&get_request(&format!("/v1/jobs/{random}/cancel")))
        .await;
    let artifacts = server
        .request(&get_request(&format!("/v1/jobs/{random}/artifacts")))
        .await;

    assert_eq!(unknown.status, 404);
    assert_eq!(malformed.status, 404);
    assert_eq!(unknown.body, malformed.body);
    assert_eq!(cancel.status, 404);
    assert_eq!(artifacts.status, 404);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn traversal_extension_confusion_and_oversize_never_execute() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    for (index, workflow) in [
        "../root.nika.yaml",
        "/tmp/root.nika.yaml",
        "root.nika.yml",
        "nested\\root.nika.yaml",
        "root.nika.yaml%2fchild.nika.yaml",
        "ghost.nika.yaml",
    ]
    .into_iter()
    .enumerate()
    {
        let body = json!({"workflow": workflow}).to_string();
        let response = server
            .request(&post_request(
                &body,
                &format!("invalid-path-{index}"),
                &auth_header(),
            ))
            .await;
        assert_eq!(response.status, 422, "{workflow}: {}", response.body);
    }
    let oversized = "x".repeat(1100);
    let response = server
        .request(&post_request(&oversized, "oversized", &auth_header()))
        .await;
    assert_eq!(response.status, 413);
    assert_eq!(response.json()["error"]["code"], "body_too_large");
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn symlinked_workflow_refuses_before_backend_execution() {
    use std::os::unix::fs::symlink;

    let world = TestWorld::new();
    let outside = tempfile::NamedTempFile::new().expect("outside workflow");
    std::fs::write(outside.path(), WORKFLOW).expect("outside bytes");
    symlink(outside.path(), world.workflows.join("linked.nika.yaml")).expect("workflow symlink");
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let response = server
        .request(&post_request(
            r#"{"workflow":"linked.nika.yaml"}"#,
            "symlinked-workflow",
            &auth_header(),
        ))
        .await;
    assert_eq!(response.status, 422, "{}", response.body);
    assert_eq!(response.json()["error"]["code"], "malformed_snapshot");
    assert!(response.json().get("id").is_none());
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn slow_authenticated_body_times_out_without_execution() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), short_request_limits()).await;
    let mut stream = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("connect");
    let headers = format!(
        "POST /v1/jobs HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 64\r\nIdempotency-Key: slow-body\r\n{}\r\n{{",
        auth_header()
    );
    stream.write_all(headers.as_bytes()).await.expect("headers");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.expect("response");
    let response = WireResponse::parse(&response);

    assert_eq!(response.status, 408, "{}", response.body);
    assert_eq!(response.json()["error"]["code"], "request_timeout");
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn connection_ceiling_bounds_slow_header_tasks_and_recovers() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, one_connection_limits()).await;
    let mut slow = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("slow connect");
    slow.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n")
        .await
        .expect("partial headers");
    tokio::time::sleep(Duration::from_millis(20)).await;

    let blocked = tokio::time::timeout(
        Duration::from_millis(50),
        wire_request(
            server.address,
            "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        ),
    )
    .await;
    assert!(blocked.is_err(), "a second connection task must not start");

    drop(slow);
    tokio::time::sleep(Duration::from_millis(30)).await;
    let recovered = server
        .request("GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await;
    assert_eq!(recovered.status, 200);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn active_and_queue_boundaries_refuse_the_first_excess_job() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let server = world.start(backend.clone(), bounded_queue_limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    let mut accepted = Vec::new();

    for index in 0..4 {
        let response = server
            .request(&post_request(
                body,
                &format!("bounded-queue-{index}"),
                &auth_header(),
            ))
            .await;
        assert_eq!(response.status, 202, "{}", response.body);
        accepted.push(response.json()["id"].as_str().expect("id").to_owned());
    }
    for _ in 0..200 {
        if backend.calls() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(backend.calls(), 2, "two active slots");
    let excess = server
        .request(&post_request(body, "bounded-queue-excess", &auth_header()))
        .await;
    assert_eq!(excess.status, 503, "{}", excess.body);
    assert_eq!(excess.json()["error"]["code"], "queue_full");
    assert_eq!(backend.peak(), 2);

    backend.release(4);
    for id in accepted {
        wait_for_status(&server, &id, "succeeded")
            .await
            .expect("queued job drains");
    }
    assert_eq!(backend.calls(), 4);
    assert_eq!(backend.peak(), 2);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn durable_job_capacity_is_stable_and_replay_remains_available() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, one_job_limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    let first = server
        .request(&post_request(body, "capacity-first", &auth_header()))
        .await;
    assert_eq!(first.status, 202);
    let id = first.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("first job settles so the queue slot is free");
    let replay = server
        .request(&post_request(body, "capacity-first", &auth_header()))
        .await;
    assert_eq!(replay.status, 200);
    let refused = server
        .request(&post_request(body, "capacity-second", &auth_header()))
        .await;
    assert_eq!(refused.status, 507);
    assert_eq!(refused.json()["error"]["code"], "job_capacity");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn header_ceiling_accepts_exactly_the_limit_and_refuses_the_next() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, four_header_limits()).await;
    let exact = format!(
        "GET /v1/jobs/00000000-0000-4000-8000-000000000000 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{}X-Boundary: exact\r\n\r\n",
        auth_header()
    );
    let accepted = server.request(&exact).await;
    assert_eq!(accepted.status, 404);
    let excess = format!(
        "GET /v1/jobs/00000000-0000-4000-8000-000000000000 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{}X-Boundary: exact\r\nX-Excess: refused\r\n\r\n",
        auth_header()
    );
    let refused = server.request(&excess).await;
    assert_eq!(refused.status, 431);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn execution_deadline_interrupts_without_outliving_the_server() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::hangs());
    let server = world.start(backend.clone(), short_execution_limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    let created = server
        .request(&post_request(body, "execution-timeout", &auth_header()))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();

    wait_for_status(&server, &id, "interrupted")
        .await
        .expect("interrupted status");
    assert_eq!(backend.calls(), 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_identical_posts_start_exactly_one_execution() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let address = server.address;
    let request = post_request(
        r#"{"workflow":"root.nika.yaml"}"#,
        "concurrent-replay",
        &auth_header(),
    );

    let mut requests = JoinSet::new();
    for _ in 0..12 {
        let request = request.clone();
        requests.spawn(async move { wire_request(address, &request).await });
    }
    let mut responses = Vec::new();
    while let Some(result) = requests.join_next().await {
        responses.push(result.expect("request task"));
    }
    let ids = responses
        .iter()
        .filter(|response| matches!(response.status, 200 | 202))
        .map(|response| response.json()["id"].as_str().expect("id").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 12, "responses: {responses:#?}");
    assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
    wait_for_status(&server, &ids[0], "succeeded")
        .await
        .expect("concurrent success status");
    assert_eq!(backend.calls(), 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_settles_running_job_and_replay_never_reexecutes() {
    let world = TestWorld::new();
    let hanging = Arc::new(TestBackend::hangs());
    let first = world.start(hanging.clone(), short_shutdown_limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    let created = first
        .request(&post_request(body, "restart-interrupted", &auth_header()))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&first, &id, "running")
        .await
        .expect("running status");
    wait_for_calls(hanging.as_ref(), 1)
        .await
        .expect("hanging backend entered execute");
    assert!(matches!(
        first.stop().await,
        Err(ServerError::ShutdownTimeout)
    ));
    assert_eq!(hanging.calls(), 1);

    let replacement = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let second = world.start(replacement.clone(), limits()).await;
    wait_for_status(&second, &id, "interrupted")
        .await
        .expect("restart interrupted status");
    let replay = second
        .request(&post_request(body, "restart-interrupted", &auth_header()))
        .await;
    assert_eq!(replay.status, 200);
    assert_eq!(replay.json()["id"], id);
    assert_eq!(replay.json()["status"], "interrupted");
    assert_eq!(replacement.calls(), 0);
    second.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn non_loopback_bind_requires_explicit_acknowledgement_before_io() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let authority = ResidentAuthority::open(ResidentConfig::new(&world.state), backend)
        .await
        .expect("authority");
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        "/does/not/exist",
        "/does/not/exist",
    );

    assert!(matches!(
        BoundServer::attach(config, &authority).await,
        Err(ServerError::InvalidConfig(_))
    ));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn contended_store_fails_fast_and_shutdown_releases_the_incarnation() {
    use std::fs::File;

    use nix::fcntl::{Flock, FlockArg};

    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, short_request_limits()).await;
    let lock_file = File::options()
        .read(true)
        .write(true)
        .open(world.state.join("jobs/store.lock"))
        .expect("store lock file");
    let lease = Flock::lock(lock_file, FlockArg::LockExclusive).expect("external lease");
    let id = JobId::random();

    let response = tokio::time::timeout(
        Duration::from_millis(100),
        server.request(&get_request(&format!("/v1/jobs/{id}"))),
    )
    .await
    .expect("contended request remains bounded");
    assert_eq!(response.status, 503);
    assert_eq!(response.json()["error"]["code"], "store_busy");
    tokio::time::timeout(Duration::from_millis(100), server.stop())
        .await
        .expect("shutdown is not queued behind a store wait")
        .expect("clean stop");

    drop(lease);
    let replacement = world
        .start(
            Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded)),
            limits(),
        )
        .await;
    replacement.stop().await.expect("incarnation released");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn shutdown_contention_joins_owner_and_next_startup_settles_running() {
    use std::fs::File;

    use nix::fcntl::{Flock, FlockArg};

    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::hangs());
    let server = world.start(backend, short_shutdown_limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "shutdown-contention",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "running")
        .await
        .expect("running before contention");
    let lock_file = File::options()
        .read(true)
        .write(true)
        .open(world.state.join("jobs/store.lock"))
        .expect("store lock file");
    let lease = Flock::lock(lock_file, FlockArg::LockExclusive).expect("external lease");

    let stopped = tokio::time::timeout(Duration::from_millis(100), server.stop())
        .await
        .expect("shutdown remains bounded under contention");
    assert!(matches!(
        stopped,
        Err(ServerError::JobStore(crate::JobStoreError::Busy))
    ));

    drop(lease);
    let replacement = world
        .start(
            Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded)),
            limits(),
        )
        .await;
    wait_for_status(&replacement, &id, "interrupted")
        .await
        .expect("new incarnation settles the ownerless run");
    replacement.stop().await.expect("replacement stop");
}

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
    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect");
    stream.write_all(request.as_bytes()).await.expect("write");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.expect("read");
    WireResponse::parse(&response)
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

    fn challenge(&self) -> bool {
        self.headers
            .to_ascii_lowercase()
            .contains("www-authenticate: bearer")
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn openapi_is_authenticated_and_omits_absent_authorities() {
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
    assert!(body["paths"].get("/v1/jobs/{id}/cancel").is_none());
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
