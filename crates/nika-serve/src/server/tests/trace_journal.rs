// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The serve door's journal, measured on the real resident backend: the
//! verify route reads the CLI's verdict for the journal the resident wrote,
//! the journal is sealed at settlement, and an interrupted job closes its
//! journal and releases its lease.

use std::path::{Path, PathBuf};

use nika_dap::journal::TraceFileSink;
use nika_dap::seal::SealTeardown;
use nika_trace::trace_verify::{VerifyOptions, verify_with};

use super::*;
use crate::server::production::{JournalSeal, ResidentExecutionBackend};

/// A run the operator's cancel cannot reach at a wave boundary: one task,
/// one long wait — the resident's grace expires and it interrupts the run.
const SLOW_WORKFLOW: &str = "nika: slow\npermits:\n  tools: [\"nika:wait\"]\ntasks:\n  linger:\n    invoke:\n      tool: nika:wait\n      args: { duration: \"25s\" }\n";

fn journal_dir(world: &TestWorld) -> PathBuf {
    world.workflows.join(nika_dap::store::TRACE_DIR)
}

fn files_with(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some(extension))
        .collect();
    files.sort();
    files
}

/// `nika trace verify --json` on one journal — the CLI's own document.
fn cli_document(path: &Path, key: Option<&Path>) -> Value {
    let out = verify_with(
        &path.to_string_lossy(),
        &VerifyOptions {
            json: true,
            key: key.map(Path::to_path_buf),
            ..Default::default()
        },
    );
    serde_json::from_str(out.text.trim_end()).expect("the CLI's json document")
}

/// The journal's last complete line — `None` on a file just created and
/// not yet written (the sink creates, then writes its first line).
fn last_line(path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(raw.lines().last()?).ok()
}

/// The custody seal on a machine that holds a run key opens the key box
/// through its KDF (seconds); the status poll waits for that too.
async fn wait_for_settled(server: &TestServer, id: &str, expected: &str) -> Result<(), String> {
    let mut last = String::new();
    for _ in 0..600 {
        let response = server
            .request(&get_request(&format!("/v1/jobs/{id}/status")))
            .await;
        if response.status == 200 && response.json()["status"] == expected {
            return Ok(());
        }
        last = response.body;
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err(format!("job {id} never reached {expected}: {last}"))
}

/// The grace a cancelled run gets is 5 s: the execution ceiling must
/// outlive it, or the timeout path would interrupt first.
fn long_execution_limits() -> ServerLimits {
    ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(30),
        Duration::from_millis(200),
        4,
        16,
        64,
        32,
    )
}

async fn run_by_name(server: &TestServer, workflow: &str, key: &str) -> String {
    let created = server
        .request(&post_request(
            &format!(r#"{{"workflow":"{workflow}"}}"#),
            key,
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    created.json()["id"].as_str().expect("id").to_owned()
}

async fn wait_until(mut holds: impl FnMut() -> bool, within: Duration) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < within {
        if holds() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    holds()
}

/// The verify route answers the CLI's verdict for the journal the resident
/// wrote — the same document `nika trace verify --json` prints for that
/// file, minus the path — and the receipt names the head it recomputed.
#[tokio::test(flavor = "multi_thread")]
async fn verify_route_reports_the_cli_verdict_for_the_jobs_journal() {
    let world = TestWorld::new();
    let backend = Arc::new(ResidentExecutionBackend::new(&world.workflows));
    // The custody seal runs inside the run's execution ceiling: on a machine
    // holding a run key its KDF takes seconds, so the ceiling is the long one.
    let server = world.start(backend, long_execution_limits()).await;
    let id = run_by_name(&server, "root.nika.yaml", "journal-verdict").await;
    wait_for_settled(&server, &id, "succeeded")
        .await
        .expect("settled");
    let journals = files_with(&journal_dir(&world), "ndjson");
    assert_eq!(journals.len(), 1, "one job, one journal: {journals:?}");
    let expected = cli_document(&journals[0], None);
    let verdict = server
        .request(&get_request(&format!("/v1/jobs/{id}/trace/verify")))
        .await;
    assert_eq!(verdict.status, 200, "{}", verdict.body);
    let body = verdict.json();
    assert_eq!(expected["chain"]["headline"], "intact", "{expected}");
    assert_eq!(body["verdict"], expected["tier"], "{body}");
    assert_eq!(body["reason"], expected["seal"]["tier"], "{body}");
    assert_eq!(body["exit"], expected["exit"]);
    assert_eq!(body["chain"], expected["chain"]);
    assert_eq!(body["seal"]["tier"], expected["seal"]["tier"]);
    assert_eq!(body["verify_version"], expected["verify_version"]);
    assert_eq!(body["trace_id"].as_str().map(str::len), Some(32));
    assert!(
        body.get("trace").is_none(),
        "the CLI's path field never crosses"
    );
    assert!(
        !verdict
            .body
            .contains(world.root.path().to_string_lossy().as_ref())
    );
    assert!(!verdict.body.contains(".nika/traces"));
    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(
        job["receipt"]["chain_head"], expected["chain"]["head"],
        "the receipt names the head the verifier recomputed: {job}"
    );
    assert!(
        files_with(&journal_dir(&world), "lock").is_empty(),
        "a settled run leaves no lease"
    );
    server.stop().await.expect("clean stop");
}

/// The seal the test holds the key to — custody stays out of the test.
struct TestSeal {
    secret: minisign::SecretKey,
    public: String,
}

impl JournalSeal for TestSeal {
    fn seal(
        &self,
        trace: &mut TraceFileSink,
        workflow_hash: Option<&str>,
        teardown: Option<&SealTeardown>,
    ) -> bool {
        let (secret, public) = (self.secret.clone(), self.public.clone());
        nika_dap::journal::seal_journal_with_key(trace, workflow_hash, teardown, move || {
            Some((secret, public))
        })
    }
}

/// A door run's journal ends on the `run_sealed` frame and verifies SEALED
/// against the key that signed it — the door seals where the settlement is
/// built, like the CLI's `surface_trace`.
#[tokio::test(flavor = "multi_thread")]
async fn a_door_run_seals_its_journal_at_settlement() {
    let world = TestWorld::new();
    let key = minisign::KeyPair::generate_unencrypted_keypair().expect("test key");
    let public = key.pk.to_box().expect("public box").to_string();
    let pubkey_file = world.root.path().join("run-signing.pub");
    std::fs::write(&pubkey_file, &public).expect("pubkey file");
    let backend = Arc::new(
        ResidentExecutionBackend::new(&world.workflows).with_journal_seal(Arc::new(TestSeal {
            secret: key.sk,
            public,
        })),
    );
    let server = world.start(backend, limits()).await;
    let id = run_by_name(&server, "root.nika.yaml", "journal-seal").await;
    wait_for_settled(&server, &id, "succeeded")
        .await
        .expect("settled");
    let journals = files_with(&journal_dir(&world), "ndjson");
    assert_eq!(journals.len(), 1, "{journals:?}");
    let last = last_line(&journals[0]).expect("a written journal");
    assert_eq!(
        last["kind"], "run_sealed",
        "the last line is the seal: {last}"
    );
    assert_eq!(last["fields"].as_array().map(Vec::is_empty), Some(false));
    let doc = cli_document(&journals[0], Some(&pubkey_file));
    assert_eq!(doc["tier"], "sealed", "{doc}");
    assert_eq!(doc["seal"]["tier"], "sealed", "{doc}");
    assert_eq!(doc["exit"], 0);
    let body = server
        .request(&get_request(&format!("/v1/jobs/{id}/trace/verify")))
        .await
        .json();
    assert_eq!(body["chain"]["events"], doc["chain"]["events"], "{body}");
    assert_ne!(
        body["seal"]["tier"], "unsealed",
        "the door sees the seal: {body}"
    );
    assert!(
        files_with(&journal_dir(&world), "lock").is_empty(),
        "a sealed run leaves no lease"
    );
    server.stop().await.expect("clean stop");
}

/// A cancelled run the grace cannot settle is interrupted by the resident:
/// its journal ends on the resident's `run_settled` record (interrupted ·
/// the operator's cause), the walk reads it as a lifecycle end, and the
/// lease sidecar is gone.
#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_door_job_closes_its_journal_and_releases_its_lease() {
    let world = TestWorld::new();
    std::fs::write(world.workflows.join("slow.nika.yaml"), SLOW_WORKFLOW).expect("slow");
    let backend = Arc::new(ResidentExecutionBackend::new(&world.workflows));
    let server = world.start(backend, long_execution_limits()).await;
    let id = run_by_name(&server, "slow.nika.yaml", "journal-interrupt").await;
    wait_for_status(&server, &id, "running")
        .await
        .expect("running");
    let dir = journal_dir(&world);
    assert!(
        wait_until(
            || !files_with(&dir, "ndjson").is_empty(),
            Duration::from_secs(3)
        )
        .await,
        "the run's first frames land before the cancel"
    );
    let cancelled = server.request(&cancel_request(&id)).await;
    assert_eq!(cancelled.status, 202, "{}", cancelled.body);
    let interrupted = wait_until(
        || {
            let journals = files_with(&dir, "ndjson");
            journals.len() == 1
                && last_line(&journals[0]).is_some_and(|last| last["kind"] == "run_settled")
        },
        Duration::from_secs(9),
    )
    .await;
    assert!(
        interrupted,
        "the resident wrote the run's END after its grace"
    );
    assert!(
        wait_until(
            || files_with(&dir, "lock").is_empty(),
            Duration::from_secs(3)
        )
        .await,
        "the lease leaves with the closed journal: {:?}",
        files_with(&dir, "lock")
    );
    wait_for_settled(&server, &id, "interrupted")
        .await
        .expect("interrupted");
    let journals = files_with(&dir, "ndjson");
    let last = last_line(&journals[0]).expect("the closed journal");
    assert_eq!(last["status"], "interrupted", "{last}");
    assert_eq!(last["cause"], "operator", "{last}");
    assert!(last["execution"]["uuid"].is_string(), "{last}");
    let doc = cli_document(&journals[0], None);
    assert_eq!(doc["chain"]["headline"], "intact", "{doc}");
    assert_eq!(doc["exit"], 0, "{doc}");
    let body = server
        .request(&get_request(&format!("/v1/jobs/{id}/trace/verify")))
        .await
        .json();
    assert_eq!(body["verdict"], "ok", "{body}");
    assert_eq!(body["chain"]["headline"], "intact", "{body}");
    server.stop().await.expect("clean stop");
}
