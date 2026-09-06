// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

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
        json!([
            "check",
            "executionSnapshot",
            "eventStream",
            "cancel",
            "schedule"
        ]),
        "the live resident HTTP authority advertises its exact route subset"
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
        // ADR-131 · the by-name door: a traversal, a wrong extension, a
        // separator confusion or a ghost is NOT a served name — the 404 that
        // teaches where the names are, and never an execution.
        assert_eq!(response.status, 404, "{workflow}: {}", response.body);
        assert_eq!(response.json()["error"]["code"], "not_found", "{workflow}");
        assert!(response.json().get("id").is_none(), "{workflow}");
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

#[tokio::test(flavor = "multi_thread")]
async fn oversized_full_duplex_upload_delivers_typed_413_before_close() {
    const UPLOAD_BYTES: usize = 8 * 1024 * 1024;

    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let upload_limits = ServerLimits::new(
        1024,
        Duration::from_secs(30),
        Duration::from_secs(2),
        Duration::from_millis(200),
        4,
        16,
        64,
        32,
    );
    let server = world.start(backend.clone(), upload_limits).await;
    let stream = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("connect");
    let (mut reader, mut writer) = stream.into_split();
    let body = vec![b'x'; UPLOAD_BYTES];
    let head = format!(
        "POST /v1/check HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n",
        body.len(),
        auth_header()
    );

    let upload = async move {
        writer.write_all(head.as_bytes()).await?;
        writer.write_all(&body).await?;
        std::io::Result::Ok(writer)
    };
    let download = async move {
        let mut response = Vec::new();
        reader.read_to_end(&mut response).await?;
        std::io::Result::Ok(response)
    };
    let (upload, response) = tokio::join!(upload, download);

    let _writer = upload.expect("server must consume the declared upload before closing");
    let response = WireResponse::parse(&response.expect("response"));
    assert_eq!(response.status, 413, "{}", response.body);
    assert_eq!(response.json()["error"]["code"], "body_too_large");
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn chunked_body_still_refuses_at_the_same_bounded_collector() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let first = "x".repeat(1024);
    let request = format!(
        "POST /v1/check HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n{}\r\n400\r\n{first}\r\n1\r\nx\r\n0\r\n\r\n",
        auth_header()
    );

    let response = server.request(&request).await;

    assert_eq!(response.status, 413, "{}", response.body);
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
    // ADR-131 · a symlink out of the served registry is not a served name:
    // the descriptor-relative open refuses it, the door says not found, and
    // nothing executes.
    assert_eq!(response.status, 404, "{}", response.body);
    assert_eq!(response.json()["error"]["code"], "not_found");
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
async fn oversized_slow_body_remains_bounded_by_request_timeout() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), short_request_limits()).await;
    let mut stream = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("connect");
    let headers = format!(
        "POST /v1/check HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 2048\r\n{}\r\n{{",
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
async fn shutdown_aborts_an_open_header_without_waiting_for_its_timeout() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, one_connection_limits()).await;
    let mut open = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("connect");
    open.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n")
        .await
        .expect("partial headers");
    tokio::time::sleep(Duration::from_millis(20)).await;

    let join = server.signal_stop();
    tokio::time::timeout(Duration::from_millis(200), join)
        .await
        .expect("shutdown must abort open HTTP connections")
        .expect("server task")
        .expect("clean stop");
    let mut byte = [0_u8; 1];
    assert_eq!(open.read(&mut byte).await.expect("closed socket"), 0);
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
