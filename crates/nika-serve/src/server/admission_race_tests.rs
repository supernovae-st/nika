// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Force the named route's lookup/capture interleaving without timing guesses.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::tests::{
    TestBackend, TestWorld, WORKFLOW, auth_header, get_request, limits, post_request,
    snapshot_body, wait_for_status,
};
use super::{ExecutionBackend, ExecutionDisposition, ExecutionOutcome, ResidentConfig};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_symlink_swap_after_named_lookup_never_admits_or_executes() {
    for directory in [false, true] {
        let world = TestWorld::new();
        let outside = tempfile::tempdir().expect("outside registry");
        let foreign = outside.path().join("root.nika.yaml");
        std::fs::write(
            &foreign,
            WORKFLOW.replace("nika: root", "nika: outside-secret"),
        )
        .expect("foreign workflow");
        let name = if directory {
            "nested/root.nika.yaml"
        } else {
            "root.nika.yaml"
        };
        let original = world.workflows.join(name);
        if directory {
            std::fs::create_dir(original.parent().expect("parent")).expect("nested dir");
            std::fs::write(&original, WORKFLOW).expect("nested workflow");
        }
        let victim = if directory {
            world.workflows.join("nested")
        } else {
            original
        };
        let destination = if directory {
            outside.path().to_owned()
        } else {
            foreign
        };
        let swapped = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&swapped);
        let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
        let server = world
            .start_with_capture_action(
                backend.clone(),
                ResidentConfig::new(&world.state).with_limits(limits()),
                &world.workflows,
                Some(Box::new(move || {
                    std::fs::rename(&victim, victim.with_extension("held"))
                        .expect("preserve original");
                    std::os::unix::fs::symlink(destination, victim).expect("replace after lookup");
                    observed.store(true, Ordering::SeqCst);
                })),
            )
            .await;
        let listed = server.request(&get_request("/v1/workflows")).await;
        assert_eq!(listed.status, 200);
        assert!(
            listed.json()["workflows"]
                .as_array()
                .expect("names")
                .contains(&json!(name))
        );
        let response = server
            .request(&post_request(
                &json!({"workflow": name}).to_string(),
                "swap-after-lookup",
                &auth_header(),
            ))
            .await;
        assert!(
            swapped.load(Ordering::SeqCst),
            "the race ran after existence succeeded"
        );
        assert_eq!(response.status, 422, "{}", response.body);
        assert!(!response.body.contains("outside-secret"));
        assert_eq!(backend.calls(), 0);
        let state: Value = serde_json::from_slice(
            &std::fs::read(world.state.join("jobs/state.json")).expect("state"),
        )
        .expect("JSON state");
        assert_eq!(
            state["jobs"],
            json!([]),
            "no durable admission from the escaped name"
        );
        server.stop().await.expect("clean stop");
    }
}

struct CaptureRecordingBackend {
    seen: Mutex<Option<(String, String)>>,
    path: std::path::PathBuf,
}

impl ExecutionBackend for CaptureRecordingBackend {
    fn execute<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move {
            let snapshot = context.snapshot();
            *self.seen.lock().expect("observation") = Some((
                snapshot.digest().to_owned(),
                snapshot
                    .text(snapshot.root())
                    .expect("captured text")
                    .to_owned(),
            ));
            std::fs::write(&self.path, "changed after capture").expect("replace disk bytes");
            ExecutionDisposition::Succeeded.into()
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_named_receipt_binds_the_bytes_the_backend_saw_despite_replacement() {
    let world = TestWorld::new();
    let path = world.workflows.join("root.nika.yaml");
    let backend = Arc::new(CaptureRecordingBackend {
        seen: Mutex::new(None),
        path: path.clone(),
    });
    let replacement = WORKFLOW.replace("input: 1", "input: 2");
    let written = replacement.clone();
    let server = world
        .start_with_capture_action(
            backend.clone(),
            ResidentConfig::new(&world.state).with_limits(limits()),
            &world.workflows,
            Some(Box::new(move || {
                std::fs::write(path, written).expect("replace regular file after lookup");
            })),
        )
        .await;
    let admitted = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "captured-digest",
            &auth_header(),
        ))
        .await;
    assert_eq!(admitted.status, 202, "{}", admitted.body);
    let job = admitted.json();
    let id = job["id"].as_str().expect("job id");
    wait_for_status(&server, id, "succeeded")
        .await
        .expect("settled");
    let finished = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    let (digest, bytes) = backend
        .seen
        .lock()
        .expect("observation")
        .clone()
        .expect("executed");
    assert_eq!(bytes, replacement);
    let expected: Value = serde_json::from_str(&snapshot_body(&replacement)).expect("snapshot");
    let original: Value = serde_json::from_str(&snapshot_body(WORKFLOW)).expect("old snapshot");
    assert_eq!(digest, expected["digest"]);
    assert_ne!(digest, original["digest"]);
    assert_eq!(finished["receipt"]["snapshot_digest"], digest);
    assert_eq!(
        std::fs::read_to_string(&backend.path).expect("disk"),
        "changed after capture"
    );
    server.stop().await.expect("clean stop");
}
