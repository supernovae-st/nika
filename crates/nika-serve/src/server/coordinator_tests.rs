// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use jiff::Timestamp;
use nika_cadence::firing::{ArmGeneration, SlotId};
use nika_cadence::{ScheduleDecision, ScheduleOrigin, ScheduleRevision};
use nika_execution::{AdmittedExecution, ExecutionService, SnapshotLimits};
use nika_fs::OwnedDir;
use serde_json::Value;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::oneshot;

use super::tests::{auth_header, post_request};
use super::*;
use crate::{JobOrigin, JobStatus};

struct TestWorld {
    _root: tempfile::TempDir,
    workflows: PathBuf,
    state: PathBuf,
    token: PathBuf,
}

impl TestWorld {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("test root");
        let workflows = root.path().join("workflows");
        let state = root.path().join("state");
        let token = root.path().join("serve.token");
        std::fs::create_dir(&workflows).expect("workflow root");
        std::fs::create_dir(&state).expect("state root");
        std::fs::write(
            workflows.join("root.nika.yaml"),
            "nika: root\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n",
        )
        .expect("workflow");
        std::fs::write(&token, "remote-test-token-012345678901234567890123456789\n")
            .expect("token");
        secure_file(&token);
        Self {
            _root: root,
            workflows,
            state,
            token,
        }
    }

    async fn start(&self, backend: Arc<dyn ExecutionBackend>, limits: ServerLimits) -> TestServer {
        let resident = ResidentConfig::new(&self.state).with_limits(limits);
        let authority = ResidentAuthority::open(resident, backend)
            .await
            .expect("authority");
        let config = ServerConfig::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            &self.workflows,
            &self.token,
        );
        let bound = BoundServer::attach(config, &authority).await.expect("bind");
        let address = bound.local_addr().expect("local address");
        let coordinator = authority.execution_coordinator();
        let (shutdown, receiver) = oneshot::channel();
        let join = tokio::spawn(authority.serve_with_http(bound, async move {
            let _result = receiver.await;
        }));
        TestServer {
            address,
            coordinator,
            shutdown: Some(shutdown),
            join,
        }
    }
}

struct TestServer {
    address: SocketAddr,
    coordinator: ResidentExecutionCoordinator,
    shutdown: Option<oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<Result<(), ServerError>>,
}

impl TestServer {
    fn coordinator(&self) -> ResidentExecutionCoordinator {
        self.coordinator.clone()
    }

    async fn request(&self, request: &str) -> WireResponse {
        let mut stream = tokio::net::TcpStream::connect(self.address)
            .await
            .expect("connect");
        stream.write_all(request.as_bytes()).await.expect("write");
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.expect("read");
        WireResponse::parse(&response)
    }

    async fn stop(mut self) -> Result<(), ServerError> {
        self.shutdown.take().expect("shutdown sender").send(()).ok();
        self.join.await.expect("server join")
    }
}

struct WireResponse {
    status: u16,
    headers: String,
    body: String,
}

impl WireResponse {
    fn parse(bytes: &[u8]) -> Self {
        let response = String::from_utf8(bytes.to_vec()).expect("UTF-8 response");
        let (headers, body) = response.split_once("\r\n\r\n").expect("HTTP boundary");
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

    fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("JSON response")
    }
}

struct TestBackend {
    calls: AtomicUsize,
}

impl TestBackend {
    fn completes(_disposition: ExecutionDisposition) -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ExecutionBackend for TestBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            ExecutionDisposition::Succeeded.into()
        })
    }
}

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

fn limits() -> ServerLimits {
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

async fn wait_for_status(server: &TestServer, id: &str, expected: &str) -> Result<(), String> {
    for _ in 0..200 {
        let response = server.request(&get_job(id)).await;
        if response.status == 200 && response.json()["status"] == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(format!("job {id} never reached {expected}"))
}

#[cfg(unix)]
fn secure_file(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .expect("secure token mode");
}

#[cfg(not(unix))]
fn secure_file(_path: &Path) {}

fn admitted(world: &TestWorld) -> AdmittedExecution {
    let project = OwnedDir::open(&world.workflows).expect("held workflows");
    ExecutionService::new(SnapshotLimits::default())
        .admit(&project, Path::new("root.nika.yaml"))
        .expect("admitted workflow")
}

fn scheduled_origin(origin: ScheduleOrigin, schedule_id: &str, slot: char) -> JobOrigin {
    scheduled_origin_with_decision(origin, schedule_id, slot, ScheduleDecision::Scheduled)
}

fn scheduled_origin_with_decision(
    origin: ScheduleOrigin,
    schedule_id: &str,
    slot: char,
    decision: ScheduleDecision,
) -> JobOrigin {
    let revision =
        ScheduleRevision::from_wire(&format!("sha256:{}", "a".repeat(64))).expect("revision");
    let slot_id = SlotId::from_wire(&slot.to_string().repeat(64)).expect("slot id");
    let generation = ArmGeneration::from_wire(&"c".repeat(64)).expect("generation");
    JobOrigin::schedule(
        origin,
        schedule_id,
        &revision,
        &slot_id,
        decision,
        "2026-08-30T10:00:00Z"
            .parse::<Timestamp>()
            .expect("scheduled instant"),
        "2026-08-30T10:00:01Z"
            .parse::<Timestamp>()
            .expect("firing instant"),
        &generation,
    )
    .expect("scheduled origin")
}

#[test]
fn schedule_origin_deserialization_refuses_unknown_decision_provenance() {
    let mut wire = serde_json::to_value(scheduled_origin(
        ScheduleOrigin::Project,
        "closed-wire",
        '9',
    ))
    .expect("origin wire");
    assert_eq!(wire["decision"], "scheduled");
    wire["decision"] = serde_json::json!("late");
    assert!(
        serde_json::from_value::<JobOrigin>(wire).is_err(),
        "unchecked provenance must not enter durable state"
    );
}

async fn prepare(
    coordinator: ResidentExecutionCoordinator,
    admitted: AdmittedExecution,
    origin: JobOrigin,
) -> PreparedScheduledRun {
    tokio::task::spawn_blocking(move || coordinator.prepare_scheduled(admitted, origin))
        .await
        .expect("prepare task")
        .expect("scheduled preparation")
}

async fn execute(prepared: PreparedScheduledRun) -> crate::JobRecord {
    tokio::task::spawn_blocking(move || prepared.execute())
        .await
        .expect("execution observer")
        .expect("scheduled execution")
}

async fn wait_for_backend_calls(backend: &GatedBackend, expected: usize) {
    for _ in 0..200 {
        if backend.calls() == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(backend.calls(), expected, "backend call deadline");
}

#[tokio::test(flavor = "multi_thread")]
async fn resident_authority_without_http_persists_a_normal_run_and_receipt() {
    let world = TestWorld::new();
    let admitted = admitted(&world);
    std::fs::remove_file(&world.token).expect("prove no credential source is required");
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let authority = ResidentAuthority::open(ResidentConfig::new(&world.state), backend.clone())
        .await
        .expect("authority opens without HTTP");
    let coordinator = authority.execution_coordinator();
    let (shutdown, receiver) = oneshot::channel();
    let runner = tokio::spawn(authority.serve_until(async move {
        let _result = receiver.await;
    }));

    let prepared = prepare(
        coordinator,
        admitted,
        scheduled_origin(ScheduleOrigin::Project, "resident-only", 'a'),
    )
    .await;
    let run_id = prepared.run_id().clone();
    let record = execute(prepared).await;
    assert_eq!(record.status(), JobStatus::Succeeded);
    let receipt = record.receipt().expect("terminal receipt");
    assert_eq!(receipt.job_id(), &run_id);
    assert!(
        receipt
            .origin()
            .and_then(JobOrigin::schedule_key_parts)
            .is_some()
    );
    assert_eq!(backend.calls(), 1);

    shutdown.send(()).ok();
    runner
        .await
        .expect("authority join")
        .expect("clean authority shutdown");
    let reopened = crate::JobStore::open_fail_fast(&world.state).expect("reopen durable state");
    let persisted = reopened
        .get(&run_id)
        .expect("read persisted run")
        .expect("normal run remains");
    assert_eq!(persisted.status(), JobStatus::Succeeded);
    assert!(persisted.receipt().is_some(), "receipt survives restart");
}

fn get_job(id: &str) -> String {
    format!(
        "GET /v1/jobs/{id} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{}\r\n",
        auth_header()
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn http_and_arm_share_one_max_concurrent_lane() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let shared_limits = ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(2),
        Duration::from_millis(200),
        1,
        4,
        64,
        32,
    );
    let server = world.start(backend.clone(), shared_limits).await;
    let response = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "manual-capacity",
            &auth_header(),
        ))
        .await;
    assert_eq!(response.status, 202);
    let manual_id = response.json()["id"]
        .as_str()
        .expect("manual id")
        .to_owned();
    wait_for_backend_calls(&backend, 1).await;

    let prepared = prepare(
        server.coordinator(),
        admitted(&world),
        scheduled_origin(ScheduleOrigin::Project, "capacity", 'b'),
    )
    .await;
    let scheduled = tokio::spawn(execute(prepared));
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(backend.calls(), 1, "scheduled run waits behind HTTP");

    backend.release(1);
    wait_for_backend_calls(&backend, 2).await;
    backend.release(1);
    assert_eq!(
        scheduled.await.expect("scheduled observer").status(),
        JobStatus::Succeeded
    );
    assert_eq!(backend.peak(), 1, "both edges share the same semaphore");
    let manual = server.request(&get_job(&manual_id)).await.json();
    assert_eq!(manual["receipt"]["origin"]["kind"], "manual");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn slot_replay_returns_one_run_and_never_reexecutes() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let origin = scheduled_origin(ScheduleOrigin::Project, "nightly", 'd');

    let first = prepare(server.coordinator(), admitted(&world), origin.clone()).await;
    let run_id = first.run_id().clone();
    assert_eq!(execute(first).await.status(), JobStatus::Succeeded);
    let replay = prepare(server.coordinator(), admitted(&world), origin).await;
    assert_eq!(replay.run_id(), &run_id);
    assert_eq!(execute(replay).await.status(), JobStatus::Succeeded);
    assert_eq!(backend.calls(), 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn losing_claim_aborts_prepared_runs_and_origin_namespaces_do_not_collide() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let project = prepare(
        server.coordinator(),
        admitted(&world),
        scheduled_origin(ScheduleOrigin::Project, "shared", 'e'),
    )
    .await;
    let api = prepare(
        server.coordinator(),
        admitted(&world),
        scheduled_origin(ScheduleOrigin::Api, "shared", 'e'),
    )
    .await;
    let project_id = project.run_id().clone();
    let api_id = api.run_id().clone();
    assert_ne!(project_id, api_id);
    tokio::task::spawn_blocking(move || {
        drop(project);
        drop(api);
    })
    .await
    .expect("claim-loss aborts");

    let project_record = server.request(&get_job(project_id.as_str())).await.json();
    let api_record = server.request(&get_job(api_id.as_str())).await.json();
    assert_eq!(project_record["status"], "failed");
    assert_eq!(api_record["status"], "failed");
    assert_eq!(backend.calls(), 0, "a lost claim cannot reach effects");

    let manual = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "capacity-released",
            &auth_header(),
        ))
        .await;
    assert_eq!(
        manual.status, 202,
        "aborted preparations release queue slots"
    );
    let manual_id = manual.json()["id"].as_str().expect("manual id").to_owned();
    wait_for_status(&server, &manual_id, "succeeded")
        .await
        .expect("manual completion");
    assert_eq!(backend.calls(), 1);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_surfaces_running_schedule_as_interrupted_ambiguity() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let first = world.start(backend.clone(), limits()).await;
    let origin = scheduled_origin(ScheduleOrigin::Project, "restart", 'f');
    let prepared = prepare(first.coordinator(), admitted(&world), origin.clone()).await;
    let run_id = prepared.run_id().clone();
    prepared.abandon_for_restart_test();
    first.stop().await.expect("clean crash boundary");

    let replacement = world
        .start(
            Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded)),
            limits(),
        )
        .await;
    let record = replacement.request(&get_job(run_id.as_str())).await.json();
    assert_eq!(record["status"], "interrupted");
    assert_eq!(record["receipt"]["origin"]["kind"], "schedule");
    assert_eq!(record["receipt"]["origin"]["schedule_id"], "restart");
    let replay = prepare(replacement.coordinator(), admitted(&world), origin).await;
    assert_eq!(replay.run_id(), &run_id);
    assert_eq!(execute(replay).await.status(), JobStatus::Interrupted);
    assert_eq!(
        backend.calls(),
        0,
        "restart never guesses whether effects ran"
    );
    replacement.stop().await.expect("clean replacement stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn scheduled_run_uses_existing_sse_with_bound_receipt_origin() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, limits()).await;
    let prepared = prepare(
        server.coordinator(),
        admitted(&world),
        scheduled_origin(ScheduleOrigin::Api, "sse", '1'),
    )
    .await;
    let run_id = prepared.run_id().clone();
    assert_eq!(execute(prepared).await.status(), JobStatus::Succeeded);

    let response = server
        .request(&format!(
            "GET /v1/jobs/{}/events HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{}\r\n",
            run_id,
            auth_header()
        ))
        .await;
    assert_eq!(response.status, 200);
    assert!(response.headers.contains("text/event-stream"));
    assert!(response.body.contains("execution.prepared"));
    assert!(response.body.contains("execution.settled"));
    assert!(response.body.contains("schedule_origin"));
    assert!(response.body.contains("\"decision\":\"scheduled\""));
    assert!(response.body.contains("scheduled_for"));
    assert!(response.body.contains("arm_generation"));
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_receipts_preserve_scheduled_and_catch_up_decisions() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;

    for (decision, wire, slot) in [
        (ScheduleDecision::Scheduled, "scheduled", '7'),
        (ScheduleDecision::CatchUp, "catch_up", '8'),
    ] {
        let prepared = prepare(
            server.coordinator(),
            admitted(&world),
            scheduled_origin_with_decision(ScheduleOrigin::Project, wire, slot, decision),
        )
        .await;
        let record = execute(prepared).await;
        let origin = record
            .receipt()
            .and_then(crate::JobReceipt::origin)
            .expect("scheduled terminal origin");
        assert_eq!(origin.schedule_decision(), Some(decision));
        assert_eq!(
            serde_json::to_value(origin).expect("receipt origin wire")["decision"],
            wire
        );
    }
    assert_eq!(backend.calls(), 2);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_drains_more_queued_jobs_than_live_channel_capacity() {
    const QUEUE_CAPACITY: usize = 2;
    const QUEUED: usize = QUEUE_CAPACITY + 7;

    let world = TestWorld::new();
    let captured = admitted(&world);
    let encoded = captured
        .snapshot()
        .encode()
        .expect("encoded execution world");
    let store = crate::JobStore::open_fail_fast(&world.state).expect("initial durable store");
    let mut ids = Vec::with_capacity(QUEUED);
    for index in 0..QUEUED {
        let admission = store
            .create_or_replay_captured(
                crate::IdempotencyKey::new(format!("recovered-{index}")).expect("idempotency key"),
                crate::RequestDigest::from_bytes(
                    [u8::try_from(index).expect("test queue count fits one byte"); 32],
                ),
                QUEUED,
                "root.nika.yaml".to_owned(),
                &encoded,
            )
            .expect("queued durable run");
        ids.push(admission.record().id().clone());
    }
    drop(store);

    let backlog_limits = ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(2),
        Duration::from_secs(2),
        2,
        QUEUE_CAPACITY,
        4,
        32,
    )
    .with_max_jobs(QUEUED);
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let authority = ResidentAuthority::open(
        ResidentConfig::new(&world.state).with_limits(backlog_limits),
        backend.clone(),
    )
    .await
    .expect("recovered authority");
    let (shutdown, receiver) = oneshot::channel();
    let runner = tokio::spawn(authority.serve_until(async move {
        let _result = receiver.await;
    }));
    for _ in 0..400 {
        if backend.calls() == QUEUED {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        backend.calls(),
        QUEUED,
        "every recovered job reached effects"
    );
    shutdown.send(()).ok();
    tokio::time::timeout(Duration::from_secs(3), runner)
        .await
        .expect("shutdown remains bounded")
        .expect("authority task")
        .expect("clean authority shutdown");

    let reopened = crate::JobStore::open_fail_fast(&world.state).expect("reopen durable state");
    for id in ids {
        assert_eq!(
            reopened
                .get(&id)
                .expect("read recovered job")
                .expect("recovered job remains")
                .status(),
            JobStatus::Succeeded
        );
    }
}
