// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Signals target only an owned child with a disposable state root. SIGKILL
//! bypasses destructors; this is process recovery, not a power-loss claim.

#![allow(clippy::panic)]

use std::io::{BufRead as _, Write as _};
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Child, Stdio};

use super::*;

const WORKER: &str = "server::tests::shutdown::process::signal_worker";
const ROOT_ENV: &str = "NIKA_SHUTDOWN_TEST_STATE_ROOT";
const READY: &str = "NIKA_SHUTDOWN_TEST_FORTY_ADMITTED_FOUR_RUNNING";

struct Worker {
    child: Child,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn start_worker(root: &std::path::Path) -> Worker {
    // Test-owned process; the guard reaps it on every exit and panic path.
    #[allow(clippy::disallowed_types)]
    let child = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", WORKER, "--ignored", "--nocapture"])
        .env_clear()
        .env(ROOT_ENV, root)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn isolated signal worker");
    let mut worker = Worker {
        child,
        reader: None,
    };
    let stdout = worker.child.stdout.take().expect("stdout pipe");
    let (ready, receiver) = std::sync::mpsc::sync_channel(1);
    // Keep draining after the marker so the clean child's test report has a reader.
    #[allow(clippy::disallowed_methods)]
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            match line {
                Ok(line) if line == READY => {
                    let _ = ready.try_send(());
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    worker.reader = Some(reader);
    receiver
        .recv_timeout(Duration::from_secs(30))
        .expect("signal handler and busy queue ready");
    worker
}

fn wait_worker(worker: &mut Worker) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = worker.child.try_wait().expect("child status") {
            return status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "signal did not stop the owned child"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn restart(root: &std::path::Path) {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("restart runtime")
        .block_on(restart_only_queued(root));
}

#[test]
#[ignore = "child entry; invoked with an isolated state root by the signal tests"]
fn signal_worker() {
    #[allow(clippy::disallowed_methods)]
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("worker state root"));
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("worker runtime")
        .block_on(async {
            // The parent owns the whole fixture tree even after SIGKILL,
            // when this child's TempDir destructors cannot run.
            let mut world = TestWorld::from_tempdir(
                tempfile::tempdir_in(&root).expect("child workflow fixture"),
            );
            world.state = root;
            let backend = Arc::new(GatedBackend::new());
            let authority = ResidentAuthority::open(
                ResidentConfig::new(&world.state)
                    .with_limits(busy_limits(Duration::from_millis(100))),
                backend.clone(),
            )
            .await
            .expect("child authority");
            let bound = BoundServer::attach(
                ServerConfig::new(
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                    &world.workflows,
                    &world.token,
                ),
                &authority,
            )
            .await
            .expect("child HTTP");
            let address = bound.local_addr().expect("address");
            let shutdown_probe = authority.state.store.shutdown_test_probe();
            let (armed, registered) = oneshot::channel();
            let stop = async move {
                let mut signal = Box::pin(process_shutdown());
                let mut armed = Some(armed);
                std::future::poll_fn(move |cx| {
                    let polled = signal.as_mut().poll(cx);
                    if let Some(armed) = armed.take() {
                        let _ = armed.send(());
                    }
                    polled
                })
                .await;
            };
            let server = TestServer {
                address,
                shutdown: None,
                join: tokio::spawn(authority.serve_with_http(bound, stop)),
                shutdown_probe,
            };
            registered.await.expect("production signal future polled");
            fill_queue(&server, &backend).await;
            assert_statuses(
                &durable_jobs(&world.state),
                &[("running", WORKERS), ("queued", JOBS - WORKERS)],
            );
            writeln!(std::io::stdout(), "{READY}").expect("announce busy boundary");
            std::io::stdout().flush().expect("flush marker");
            let result = server.join.await.expect("server task");
            assert!(
                matches!(result, Err(ServerError::ShutdownTimeout)),
                "{result:?}"
            );
            assert_eq!(backend.calls(), WORKERS);
        });
}

#[test]
fn sigint_and_sigterm_share_the_busy_queue_grace_and_recovery_contract() {
    for signal in [
        nix::sys::signal::Signal::SIGINT,
        nix::sys::signal::Signal::SIGTERM,
    ] {
        let root = tempfile::tempdir().expect("state root");
        let mut worker = start_worker(root.path());
        let pid = nix::unistd::Pid::from_raw(i32::try_from(worker.child.id()).expect("child pid"));
        nix::sys::signal::kill(pid, signal).expect("signal only the owned child");
        assert!(
            wait_worker(&mut worker).success(),
            "{signal:?} follows graceful shutdown, not the default signal exit"
        );
        drop(worker);
        restart(root.path());
    }
}

#[test]
fn sigkill_keeps_unstarted_jobs_queued_and_restart_never_replays_running_jobs() {
    let root = tempfile::tempdir().expect("state root");
    let mut worker = start_worker(root.path());
    worker.child.kill().expect("kill only the owned child");
    assert_eq!(wait_worker(&mut worker).signal(), Some(9));
    drop(worker);
    assert_statuses(
        &durable_jobs(root.path()),
        &[("running", WORKERS), ("queued", JOBS - WORKERS)],
    );
    restart(root.path());
}
