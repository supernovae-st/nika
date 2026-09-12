// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The busy-queue shutdown contract (#1383), with gates instead of slow jobs.

use super::*;

#[cfg(unix)]
mod process;

const JOBS: usize = 40;
const WORKERS: usize = 4;

fn busy_limits(grace: Duration) -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(5),
        Duration::from_secs(60),
        grace,
        WORKERS,
        64,
        64,
        32,
    )
}

async fn wait_for_gated_calls(backend: &GatedBackend, count: usize) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while backend.calls() < count {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("workers entered the backend");
    assert_eq!(backend.calls(), count);
}

async fn fill_queue(server: &TestServer, backend: &GatedBackend) {
    for index in 0..JOBS {
        let response = server
            .request(&post_request(
                r#"{"workflow":"root.nika.yaml"}"#,
                &format!("busy-shutdown-{index}"),
                &auth_header(),
            ))
            .await;
        assert_eq!(response.status, 202, "{}", response.body);
    }
    wait_for_gated_calls(backend, WORKERS).await;
    assert_eq!(backend.peak(), WORKERS);
}

fn durable_jobs(root: &std::path::Path) -> Vec<Value> {
    let state: Value = serde_json::from_slice(
        &std::fs::read(root.join("jobs/state.json")).expect("durable state"),
    )
    .expect("state JSON");
    state["jobs"].as_array().expect("jobs").clone()
}

fn assert_statuses(jobs: &[Value], expected: &[(&str, usize)]) {
    assert_eq!(jobs.len(), JOBS);
    assert_eq!(expected.iter().map(|(_, count)| count).sum::<usize>(), JOBS);
    for (status, count) in expected {
        assert_eq!(
            jobs.iter()
                .filter(|job| job["record"]["status"] == *status)
                .count(),
            *count,
            "durable {status} count"
        );
    }
}

async fn restart_only_queued(root: &std::path::Path) {
    let before = durable_jobs(root);
    let interrupted_on_restart = before
        .iter()
        .any(|job| job["record"]["status"] == "running");
    assert_statuses(
        &before,
        &[
            (
                if interrupted_on_restart {
                    "running"
                } else {
                    "interrupted"
                },
                WORKERS,
            ),
            ("queued", JOBS - WORKERS),
        ],
    );
    let mut world = TestWorld::new();
    world.state = root.to_owned();
    // Restart must read the admission sidecars, even if the registry changed.
    std::fs::write(
        world.workflows.join("root.nika.yaml"),
        "no longer a workflow",
    )
    .expect("replace live source");
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world
        .start(backend.clone(), busy_limits(Duration::from_secs(30)))
        .await;
    tokio::time::timeout(Duration::from_secs(35), server.stop())
        .await
        .expect("restart drain watchdog")
        .expect("queued worlds finish");
    assert_eq!(backend.calls(), JOBS - WORKERS);
    let after = durable_jobs(root);
    assert_statuses(
        &after,
        &[("interrupted", WORKERS), ("succeeded", JOBS - WORKERS)],
    );
    for original in &before {
        let restored = after
            .iter()
            .find(|job| job["record"]["id"] == original["record"]["id"])
            .expect("same durable identity");
        if original["record"]["status"] == "running" {
            assert_eq!(restored["record"]["status"], "interrupted");
            for field in ["execution_id", "trace_id", "snapshot_digest"] {
                assert_eq!(restored["record"][field], original["record"][field]);
                assert_eq!(
                    restored["record"]["receipt"][field],
                    original["record"][field]
                );
            }
            let events = restored["events"].as_array().expect("recovered journal");
            assert_eq!(
                events.len(),
                original["events"].as_array().expect("old journal").len() + 1
            );
            assert_eq!(
                events.last().expect("recovery event")["payload"]["kind"],
                "interrupted"
            );
        } else if original["record"]["status"] == "interrupted" {
            assert_eq!(restored, original, "an interrupted job never re-executes");
        } else {
            assert_eq!(
                restored["record"]["idempotency_key"],
                original["record"]["idempotency_key"]
            );
            assert_eq!(
                restored["record"]["request_digest"],
                original["record"]["request_digest"]
            );
            assert!(restored["record"]["receipt"].is_object());
        }
    }
}

#[test]
fn the_documented_default_is_thirty_seconds_for_four_workers() {
    assert_eq!(
        ServerLimits::default().shutdown_grace(),
        Duration::from_secs(30)
    );
    assert_eq!(ServerLimits::default().max_concurrent_jobs(), WORKERS);
}

#[test]
fn the_shipped_supervisor_allows_cleanup_after_the_execution_grace() {
    let unit = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/ops/nika-serve.service"
    ));
    let seconds: u64 = unit
        .lines()
        .find_map(|line| line.strip_prefix("TimeoutStopSec="))
        .expect("explicit supervisor stop timeout")
        .trim_end_matches('s')
        .parse()
        .expect("supervisor timeout in seconds");
    assert!(
        Duration::from_secs(seconds) > ServerLimits::default().shutdown_grace(),
        "the supervisor must leave time for durable settlement after the execution grace"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_drains_all_forty_admissions_when_they_fit_the_shared_grace() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let server = world
        .start(backend.clone(), busy_limits(Duration::from_secs(30)))
        .await;
    fill_queue(&server, &backend).await;
    assert_statuses(
        &durable_jobs(&world.state),
        &[("running", WORKERS), ("queued", JOBS - WORKERS)],
    );
    let address = server.address;
    let probe = server.shutdown_probe();
    let stopped = server.signal_stop();
    probe.wait_shutdown_loop_observed().await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while tokio::net::TcpStream::connect(address).await.is_ok() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("HTTP listener closes while the admitted queue drains");
    backend.release(WORKERS);
    wait_for_gated_calls(&backend, WORKERS * 2).await;
    assert!(!stopped.is_finished(), "shutdown includes queued work");
    backend.release(JOBS - WORKERS);
    tokio::time::timeout(Duration::from_secs(35), stopped)
        .await
        .expect("drain watchdog")
        .expect("server task")
        .expect("all admitted jobs finish within the grace");
    assert_eq!(backend.calls(), JOBS);
    assert_eq!(backend.peak(), WORKERS);
    assert_statuses(&durable_jobs(&world.state), &[("succeeded", JOBS)]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_timeout_interrupts_four_and_preserves_thirty_six_for_restart() {
    let world = TestWorld::new();
    let backend = Arc::new(GatedBackend::new());
    let grace = Duration::from_millis(100);
    let server = world.start(backend.clone(), busy_limits(grace)).await;
    fill_queue(&server, &backend).await;
    let started = tokio::time::Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(5), server.stop())
        .await
        .expect("grace expiry plus local durable cleanup remains bounded in this fixture");
    assert!(
        matches!(result, Err(ServerError::ShutdownTimeout)),
        "{result:?}"
    );
    assert!(
        started.elapsed() >= grace,
        "running work receives its grace"
    );
    assert_eq!(backend.calls(), WORKERS, "unstarted work was not executed");
    for job in durable_jobs(&world.state) {
        if job["record"]["status"] == "interrupted" {
            assert!(job["record"]["receipt"].is_object());
            assert_eq!(
                job["events"]
                    .as_array()
                    .expect("events")
                    .last()
                    .expect("terminal")["payload"]["kind"],
                "execution.interrupted"
            );
        }
    }
    restart_only_queued(&world.state).await;
}
