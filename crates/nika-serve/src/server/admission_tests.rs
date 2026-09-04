//! The resident's job door admits by NAME and computes absent digests
//! (ADR-131 · #1441) — the admission tests, beside the lifecycle ones.

use std::sync::Arc;

use serde_json::Value;

use super::tests::{
    TestBackend, TestWorld, WORKFLOW, auth_header, check_request, events_request, get_request,
    limits, parse_sse_data, post_request, snapshot_body, wait_for_status,
};
use super::{ExecutionBackend, ExecutionDisposition, ExecutionOutcome};

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

/// A backend that settles with the runtime's own settlement (ADR-128): the
/// resident projects it whole on the terminal event and the job's status
/// is the settlement's state.
#[derive(Debug)]
struct SettlingBackend {
    settlement: nika_event::settlement::RunSettlement,
}

impl ExecutionBackend for SettlingBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move {
            use nika_event::settlement::RunState;
            let disposition = match self.settlement.state {
                RunState::Succeeded => ExecutionDisposition::Succeeded,
                RunState::Paused => ExecutionDisposition::Paused,
                RunState::Cancelled => ExecutionDisposition::Cancelled,
                _ => ExecutionDisposition::Failed,
            };
            let mut outcome =
                ExecutionOutcome::from(disposition).with_settlement(self.settlement.clone());
            if let Some(error) = &self.settlement.error {
                outcome = outcome.with_error(error.code.clone(), error.message.clone());
            }
            outcome
        })
    }
}

/// ADR-128 · the terminal event carries the runtime's settlement whole and
/// the job's status is its state: the SDK's HTTP door reads the same cause,
/// tally and spend the CLI's `run_settled` carries, and an unmetered run
/// never shows a zero it did not meter.
#[tokio::test(flavor = "multi_thread")]
async fn the_terminal_event_carries_the_runtimes_settlement() {
    use nika_event::settlement::{RunCause, RunSettlement, RunState, Spend, TaskTally};
    let world = TestWorld::new();
    let mut tally = TaskTally::new();
    tally.total = 2;
    tally.ok = 2;
    let mut settlement = RunSettlement::new(RunState::Succeeded, RunCause::Normal)
        .with_spend(Spend::new(None, 0, 2));
    settlement.tasks = Some(tally);
    settlement.elapsed_ms = Some(12);
    let expected = serde_json::to_value(&settlement).expect("a settlement serializes");
    let server = world
        .start(Arc::new(SettlingBackend { settlement }), limits())
        .await;
    let response = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "settles",
            &auth_header(),
        ))
        .await;
    assert_eq!(response.status, 202, "{}", response.body);
    let id = response.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("the job settles");
    let streamed = server.request(&events_request(&id, None)).await;
    let events = parse_sse_data(&streamed.body);
    let terminal = events
        .iter()
        .find(|event| event["kind"] == "execution.settled")
        .expect("a settled terminal frame on the stream");
    assert_eq!(
        terminal["settlement"], expected,
        "the settlement rides the terminal frame whole: {terminal}"
    );
    assert_eq!(
        terminal["status"], terminal["settlement"]["status"],
        "the job's status IS the settlement's state: {terminal}"
    );
    assert_eq!(terminal["settlement"]["spend"]["qualifier"], "unpriced");
    assert!(
        terminal["settlement"]["spend"]
            .get("total_cost_usd")
            .is_none(),
        "unknown cost is never zero: {terminal}"
    );
    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert_eq!(job["status"], "succeeded", "{job}");
    server.stop().await.expect("clean stop");
}

/// ADR-132 · the freeze audit (G1) · a retry with the same key finds its
/// job BEFORE the resident reads the registry again: the workflow changes,
/// then vanishes, and the retry still replays the ORIGINAL job (200 · the
/// same id · one execution, ever); the same key with other bytes is a
/// typed conflict; the resident's own `schedule:` namespace is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_retry_replays_its_job_before_the_registry_is_read_again() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), limits()).await;
    let body = r#"{"workflow":"root.nika.yaml"}"#;
    let first = server
        .request(&post_request(body, "retry-k", &auth_header()))
        .await;
    assert_eq!(first.status, 202, "{}", first.body);
    let id = first.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("the job settles");
    // The file changes on disk (another valid workflow under the same name).
    std::fs::write(
        world.workflows.join("root.nika.yaml"),
        WORKFLOW.replace("value:", "other:"),
    )
    .expect("the workflow mutates");
    let retry = server
        .request(&post_request(body, "retry-k", &auth_header()))
        .await;
    assert_eq!(retry.status, 200, "{}", retry.body);
    assert_eq!(retry.json()["id"], id, "the ORIGINAL job replays");
    // The file vanishes: the retry still finds its job, never a 404.
    std::fs::remove_file(world.workflows.join("root.nika.yaml")).expect("the workflow vanishes");
    let again = server
        .request(&post_request(body, "retry-k", &auth_header()))
        .await;
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(again.json()["id"], id, "{}", again.body);
    assert_eq!(backend.calls(), 1, "one execution, ever");
    // The same key with other bytes is a typed conflict, still capture-free.
    let conflict = server
        .request(&post_request(
            r#"{"workflow":"other.nika.yaml"}"#,
            "retry-k",
            &auth_header(),
        ))
        .await;
    assert_eq!(conflict.status, 409, "{}", conflict.body);
    assert_eq!(conflict.json()["error"]["code"], "idempotency_conflict");
    // The resident's own key namespace is refused to a manual caller.
    let reserved = server
        .request(&post_request(body, "schedule:x:0", &auth_header()))
        .await;
    assert_eq!(reserved.status, 400, "{}", reserved.body);
    assert_eq!(reserved.json()["error"]["code"], "invalid_idempotency_key");
    assert_eq!(backend.calls(), 1);
    server.stop().await.expect("clean stop");
}
