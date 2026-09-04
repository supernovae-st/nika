//! The resident's job door admits by NAME and computes absent digests
//! (ADR-131 · #1441) — the admission tests, beside the lifecycle ones.

use std::sync::Arc;

use super::ExecutionDisposition;
use serde_json::Value;

use super::tests::{
    TestBackend, TestWorld, WORKFLOW, auth_header, check_request, get_request, limits,
    post_request, snapshot_body, wait_for_status,
};

/// ADR-131 · #1441 · a workflow the served registry lists is admitted by
/// NAME: the resident captures the world itself (the one owner), the job
/// runs, and a name the registry does not list is a 404 that teaches.
#[tokio::test(flavor = "multi_thread")]
async fn a_served_workflow_is_admitted_by_name_and_runs() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let response = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "by-name",
            &auth_header(),
        ))
        .await;
    assert_eq!(response.status, 202, "{}", response.body);
    let job = response.json();
    assert!(job["id"].is_string(), "{job}");
    let id = job["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("the job settles");
    assert_eq!(
        backend.calls(),
        1,
        "the resident captured and ran the world once"
    );
    let unknown = server
        .request(&post_request(
            r#"{"workflow":"nowhere.nika.yaml"}"#,
            "by-name-unknown",
            &auth_header(),
        ))
        .await;
    assert_eq!(unknown.status, 404, "{}", unknown.body);
    assert_eq!(unknown.json()["error"]["code"], "not_found");
    assert!(
        unknown.json()["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("GET /v1/workflows")),
        "the refusal teaches where the names are: {}",
        unknown.body
    );
    assert_eq!(backend.calls(), 1);
    server.stop().await.expect("clean stop");
}

/// ADR-131 · a snapshot whose digests are absent is admitted — the
/// resident computes them, and the receipt carries the engine's digest,
/// the same one an attested body would carry.
#[tokio::test(flavor = "multi_thread")]
async fn a_digest_less_snapshot_is_admitted_with_the_engines_digest() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let attested = serde_json::from_str::<Value>(&snapshot_body(WORKFLOW)).expect("snapshot");
    let engine_digest = attested["digest"].as_str().expect("digest").to_owned();
    let mut bare = attested.clone();
    bare.as_object_mut().expect("object").remove("digest");
    bare["units"][0]
        .as_object_mut()
        .expect("unit")
        .remove("digest");
    let response = server
        .request(&post_request(&bare.to_string(), "bare", &auth_header()))
        .await;
    assert_eq!(response.status, 202, "{}", response.body);
    let job = response.json();
    let id = job["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("the job settles");
    let settled = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(
        settled["receipt"]["snapshot_digest"], engine_digest,
        "the resident computed the engine's own digest: {settled}"
    );
    server.stop().await.expect("clean stop");
}

/// The check door and the job door judge the caller's exact bytes, never
/// a server-local path that happens to share the name.
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
