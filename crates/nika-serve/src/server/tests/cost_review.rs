// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! C6 · the cost-review door over real HTTP: seated only by the operator, one
//! explicit decision for one job, refusals that keep or spend a review
//! honestly, a lifetime that ends in release, a server ceiling that stays a
//! hard cap, idempotent retries that never mint new authority, and a public
//! document that never shows the endpoint's path or the private nonce. The
//! review's plan is injected (the machine's probes otherwise); nothing is
//! dispatched to any provider.

use std::sync::Mutex;

use nika_dap::cost_journal::JOURNAL;

use super::*;

const REVIEWED: &str = "nika: reviewed\nmodel: deepseek/c6-unpriced-fixture\npermits: {}\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 16 }\n";

/// An admitted API lane for the unpriced route the review workflow names.
fn unpriced_plan() -> nika_service_execution::ExecutionAccessPlan {
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
    let ready = ProviderReadiness::new(
        true,
        true,
        None,
        None,
        true,
        ExecutionLocus::Cloud,
        nika_types::access::AccessClass::Api,
    );
    nika_providers::resolve_execution_plan(
        &[nika_providers::ModelNeed::new(
            "deepseek/c6-unpriced-fixture",
            true,
            false,
        )],
        &[ProviderProbe::new(
            "deepseek",
            true,
            true,
            "DEEPSEEK_API_KEY",
            false,
            ready,
            "https://api.deepseek.com",
        )],
        Some("api"),
    )
}

/// Runs a reviewed job the way production does with its authority: the
/// account is the review's, and the backend settles it (nothing is sent).
#[derive(Default)]
struct ReviewedBackend {
    runs: Mutex<Vec<String>>,
}

impl ExecutionBackend for ReviewedBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async { ExecutionOutcome::from(ExecutionDisposition::Succeeded) })
    }

    fn execute_reviewed<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        _inputs: &BTreeMap<String, Value>,
        _cancel: nika_types::cancel::CancelCtx,
        authority: CostAuthority,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        let execution = context.execution_id().to_string();
        self.runs.lock().expect("runs").push(execution);
        let settled = authority.cost.finish();
        Box::pin(async move {
            match settled {
                Ok(()) => ExecutionOutcome::from(ExecutionDisposition::Succeeded),
                Err(why) => ExecutionOutcome::failed("cost_observation_failed", why),
            }
        })
    }
}

/// A server over `world`, the door seated or not, the review plan injected.
async fn start(
    world: &TestWorld,
    backend: Arc<dyn ExecutionBackend>,
    limits: ServerLimits,
    seat: bool,
) -> (TestServer, Arc<AppState>) {
    let resident = ResidentConfig::new(&world.state).with_limits(limits);
    let authority = ResidentAuthority::open(resident, backend)
        .await
        .expect("authority");
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &world.workflows,
        &world.token,
    )
    .with_cost_review(seat);
    let bound = BoundServer::attach(config, &authority).await.expect("bind");
    let state = Arc::clone(&bound.state);
    if let Some(door) = &state.cost_review {
        *door.plan.lock().expect("plan") = Some(unpriced_plan());
    }
    let address = bound.local_addr().expect("local address");
    let shutdown_probe = authority.state.store.shutdown_test_probe();
    let observer = Arc::clone(&shutdown_probe);
    let (shutdown, receiver) = oneshot::channel();
    let join = tokio::spawn(authority.serve_with_http(bound, async move {
        let _result = receiver.await;
        observer.mark_shutdown_loop_observed();
    }));
    let server = TestServer {
        address,
        shutdown: Some(shutdown),
        join,
        shutdown_probe,
    };
    (server, state)
}

fn disarmed() -> ServerLimits {
    limits().with_default_max_cost_usd(None)
}

fn world() -> TestWorld {
    let world = TestWorld::new();
    std::fs::write(world.workflows.join("review.nika"), REVIEWED).expect("workflow");
    world
}

fn review_request(body: &str, key: &str) -> String {
    format!(
        "POST /v1/cost-reviews HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\nIdempotency-Key: {key}\r\n{}\r\n{body}",
        body.len(),
        auth_header()
    )
}

fn decision_request(id: &str, witness: &str, decision: &str) -> String {
    let body = json!({"witness_sha256": witness, "decision": decision}).to_string();
    format!(
        "POST /v1/cost-reviews/{id}/decision HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n{body}",
        body.len(),
        auth_header()
    )
}

fn reviewed_job(review: &Value, key: &str) -> String {
    let body = json!({"workflow": "review.nika", "cost_review": {
        "review_id": review["review_id"], "witness_sha256": review["witness_sha256"]}});
    post_request(&body.to_string(), key, &auth_header())
}

const REVIEW_BODY: &str = r#"{"workflow":"review.nika"}"#;

fn rows(world: &TestWorld) -> Vec<Value> {
    std::fs::read_to_string(world.workflows.join(".nika").join(JOURNAL))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("row"))
        .collect()
}

fn lease_is_free(world: &TestWorld) -> bool {
    let root = nika_fs::OwnedDir::open(&world.workflows).expect("root");
    nika_dap::cost_journal::clear(&root, "probe")
        .expect("journal")
        .is_ok()
}

async fn open_review(server: &TestServer, key: &str) -> Value {
    let created = server.request(&review_request(REVIEW_BODY, key)).await;
    assert_eq!(created.status, 201, "{}", created.body);
    created.json()
}

/// Only the operator seats the door: without `--cost-review` health lacks
/// the capability, the served contract lacks the routes, and every door route
/// and a job carrying a review refuse 403 without touching any state.
#[tokio::test(flavor = "multi_thread")]
async fn the_door_exists_only_when_the_operator_seats_it() {
    let world = world();
    let backend = Arc::new(ReviewedBackend::default());
    let (server, _state) = start(&world, backend, disarmed(), false).await;
    let health = server.request(&get_request("/health")).await.json();
    assert!(
        !health["supportedCapabilities"]
            .as_array()
            .expect("caps")
            .contains(&json!("costReviewV1"))
    );
    let contract = server
        .request(&get_request("/v1/openapi.json"))
        .await
        .json();
    assert!(contract["paths"].get("/v1/cost-reviews").is_none());
    for request in [
        review_request(REVIEW_BODY, "k-1"),
        get_request("/v1/cost-reviews/rev-x"),
        decision_request("rev-x", "00", "approve_once"),
        reviewed_job(
            &json!({"review_id": "rev-x", "witness_sha256": "00"}),
            "k-2",
        ),
    ] {
        let refused = server.request(&request).await;
        assert_eq!(refused.status, 403, "{}", refused.body);
        assert_eq!(refused.json()["error"]["code"], "cost_review_unavailable");
    }
    let nika = world.workflows.join(".nika");
    assert!(!nika.join(JOURNAL).exists(), "no journal");
    assert!(!nika.join(format!("{JOURNAL}.lock")).exists(), "no lease");
    server.stop().await.expect("stop");
    let (seated, _state) = start(
        &world,
        Arc::new(ReviewedBackend::default()),
        disarmed(),
        true,
    )
    .await;
    let health = seated.request(&get_request("/health")).await.json();
    assert!(
        health["supportedCapabilities"]
            .as_array()
            .expect("caps")
            .contains(&json!("costReviewV1"))
    );
    let contract = seated
        .request(&get_request("/v1/openapi.json"))
        .await
        .json();
    assert_eq!(contract, super::openapi::served(false, true));
    for path in [
        "/v1/cost-reviews",
        "/v1/cost-reviews/{id}",
        "/v1/cost-reviews/{id}/decision",
    ] {
        assert!(contract["paths"].get(path).is_some(), "{path}");
    }
    seated.stop().await.expect("stop");
}

/// A1 at the door: one pending review holds the lease and writes no Run row;
/// one explicit approval; one job consumes it and runs the reviewed world
/// under the review's execution identity and account, which settles once; a
/// second job with the same review is refused; the lease is free after.
#[tokio::test(flavor = "multi_thread")]
async fn a_review_is_one_explicit_decision_for_one_job() {
    let world = world();
    let backend = Arc::new(ReviewedBackend::default());
    let (server, _state) = start(&world, backend.clone(), disarmed(), true).await;
    let review = open_review(&server, "review-1").await;
    assert_eq!(review["state"], "pending");
    assert_eq!(review["route"]["origin"], "https://api.deepseek.com:443");
    assert_eq!(review["host"]["credential_custody"], "HOST_SERVER_MEMORY");
    assert_eq!(review["host"]["evidence"]["machine"]["cap"], "absent");
    assert!(rows(&world).is_empty(), "a review writes no Run row");
    assert!(!lease_is_free(&world), "a pending review holds the lease");
    let pending = server.request(&reviewed_job(&review, "job-0")).await;
    assert_eq!(pending.status, 409);
    assert_eq!(pending.json()["error"]["code"], "review_not_approved");
    let id = review["review_id"].as_str().expect("id");
    let witness = review["witness_sha256"].as_str().expect("witness");
    let approved = server
        .request(&decision_request(id, witness, "approve_once"))
        .await;
    assert_eq!(approved.status, 200, "{}", approved.body);
    assert_eq!(approved.json()["state"], "approved");
    let job = server.request(&reviewed_job(&review, "job-1")).await;
    assert_eq!(job.status, 202, "{}", job.body);
    let job_id = job.json()["id"].as_str().expect("job id").to_owned();
    wait_for_status(&server, &job_id, "succeeded")
        .await
        .expect("the reviewed job settles");
    assert_eq!(
        *backend.runs.lock().expect("runs"),
        [review["execution_id"].as_str().expect("exe").to_owned()]
    );
    let again = server.request(&reviewed_job(&review, "job-2")).await;
    assert_eq!(again.status, 409);
    assert_eq!(again.json()["error"]["code"], "review_consumed");
    let consumed = server
        .request(&get_request(&format!("/v1/cost-reviews/{id}")))
        .await
        .json();
    assert_eq!(consumed["state"], "consumed");
    assert_eq!(consumed["job"]["id"], job_id);
    assert_eq!(consumed["account"]["attempts"], 0);
    assert_eq!(
        consumed["account"]["basis"],
        "admission-account accounting, not proof of physical dispatch"
    );
    let phases: Vec<Value> = rows(&world)
        .iter()
        .map(|row| row["phase"].clone())
        .collect();
    assert_eq!(phases, ["prepared", "settled"]);
    assert_eq!(rows(&world)[0]["invocation"], review["execution_id"]);
    assert!(lease_is_free(&world), "the settled Run released the lease");
    server.stop().await.expect("stop");
}

/// A wrong witness moves nothing; another request than the reviewed one keeps
/// the approval; a second, different decision refuses; the held lease refuses
/// a second review; unknown ids and malformed decisions are named; a decline
/// ends a review and refuses its job.
#[tokio::test(flavor = "multi_thread")]
async fn refusals_keep_or_spend_the_review_honestly() {
    let world = world();
    let (server, state) = start(
        &world,
        Arc::new(ReviewedBackend::default()),
        disarmed(),
        true,
    )
    .await;
    let review = open_review(&server, "review-1").await;
    let id = review["review_id"].as_str().expect("id");
    let witness = review["witness_sha256"].as_str().expect("witness");
    let forged = server
        .request(&decision_request(id, &"0".repeat(64), "approve_once"))
        .await;
    assert_eq!(
        (forged.status, forged.json()["error"]["code"].clone()),
        (409, json!("review_witness_mismatch"))
    );
    let malformed = server
        .request(&decision_request(id, witness, "approve"))
        .await;
    assert_eq!(malformed.status, 422);
    assert_eq!(
        server
            .request(&decision_request(id, witness, "approve_once"))
            .await
            .status,
        200
    );
    let replayed = server
        .request(&decision_request(id, witness, "approve_once"))
        .await;
    assert_eq!(
        (replayed.status, replayed.json()["state"].clone()),
        (200, json!("approved"))
    );
    let other = json!({"workflow": "review.nika", "inputs": {}, "access": "api", "cost_review": {
        "review_id": id, "witness_sha256": witness}});
    let mismatch = server
        .request(&post_request(&other.to_string(), "job-x", &auth_header()))
        .await;
    assert_eq!(mismatch.json()["error"]["code"], "review_request_mismatch");
    let still = server
        .request(&get_request(&format!("/v1/cost-reviews/{id}")))
        .await
        .json();
    assert_eq!(
        still["state"], "approved",
        "another request never spends the review"
    );
    let declined = server
        .request(&decision_request(id, witness, "decline"))
        .await;
    assert_eq!(
        (declined.status, declined.json()["error"]["code"].clone()),
        (409, json!("review_decided"))
    );
    let busy = server
        .request(&review_request(REVIEW_BODY, "review-busy"))
        .await;
    assert_eq!(busy.status, 422, "{}", busy.body);
    assert!(
        busy.body.contains("holds this project's cost lease"),
        "{}",
        busy.body
    );
    let unknown = server
        .request(&get_request(
            "/v1/cost-reviews/rev-00000000-0000-0000-0000-000000000000",
        ))
        .await;
    assert_eq!(
        (unknown.status, unknown.json()["error"]["code"].clone()),
        (404, json!("review_unknown"))
    );
    let door = state.cost_review.as_ref().expect("door");
    door.skew.store(301, std::sync::atomic::Ordering::SeqCst);
    let second = open_review(&server, "review-2").await;
    let (id2, witness2) = (
        second["review_id"].as_str().expect("id"),
        second["witness_sha256"].as_str().expect("w"),
    );
    let gone = server
        .request(&decision_request(id2, witness2, "decline"))
        .await;
    assert_eq!(gone.json()["state"], "declined");
    assert!(lease_is_free(&world), "a decline released the lease");
    let job = server.request(&reviewed_job(&second, "job-d")).await;
    assert_eq!(job.json()["error"]["code"], "review_declined");
    server.stop().await.expect("stop");
}

/// An expired review admits nothing (410) and its lease is released; the
/// monotonic lifetime is the server's, never the caller's.
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_review_admits_nothing_and_frees_the_lease() {
    let world = world();
    let (server, state) = start(
        &world,
        Arc::new(ReviewedBackend::default()),
        disarmed(),
        true,
    )
    .await;
    let review = open_review(&server, "review-1").await;
    let id = review["review_id"].as_str().expect("id");
    let witness = review["witness_sha256"].as_str().expect("witness");
    assert_eq!(
        server
            .request(&decision_request(id, witness, "approve_once"))
            .await
            .status,
        200
    );
    let door = state.cost_review.as_ref().expect("door");
    door.skew.store(301, std::sync::atomic::Ordering::SeqCst);
    let job = server.request(&reviewed_job(&review, "job-late")).await;
    assert_eq!(
        (job.status, job.json()["error"]["code"].clone()),
        (410, json!("review_expired"))
    );
    let expired = server
        .request(&get_request(&format!("/v1/cost-reviews/{id}")))
        .await
        .json();
    assert_eq!(expired["state"], "expired");
    assert!(lease_is_free(&world), "expiry released the lease");
    assert!(rows(&world).is_empty(), "nothing was prepared");
    server.stop().await.expect("stop");
}

/// The server's per-run ceiling (the default 1 USD) is a hard cap: a review
/// refuses before any question, names the explicit disarm, and holds nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_present_server_ceiling_is_a_hard_cap() {
    let world = world();
    let limits = limits().with_default_max_cost_usd(Some(1.0));
    let (server, _state) = start(&world, Arc::new(ReviewedBackend::default()), limits, true).await;
    let refused = server
        .request(&review_request(REVIEW_BODY, "review-1"))
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    let error = refused.json();
    assert_eq!(error["error"]["code"], "cost_review_refused");
    assert!(
        error["error"]["message"]
            .as_str()
            .expect("message")
            .contains("--run-cost-ceiling none")
    );
    assert!(lease_is_free(&world), "a refused review holds nothing");
    server.stop().await.expect("stop");
}

/// A lost response is observed, never re-granted: the same key and bytes
/// answer the same review; other bytes conflict; a lost job response replays
/// the job before the review is read, and the review stays consumed once.
#[tokio::test(flavor = "multi_thread")]
async fn retries_observe_the_same_state_and_never_mint_authority() {
    let world = world();
    let (server, _state) = start(
        &world,
        Arc::new(ReviewedBackend::default()),
        disarmed(),
        true,
    )
    .await;
    let review = open_review(&server, "review-1").await;
    let replay = server
        .request(&review_request(REVIEW_BODY, "review-1"))
        .await;
    assert_eq!(replay.status, 200);
    assert_eq!(replay.json()["review_id"], review["review_id"]);
    assert_eq!(replay.json()["witness_sha256"], review["witness_sha256"]);
    let changed = server
        .request(&review_request(
            r#"{"workflow":"review.nika","inputs":{}}"#,
            "review-1",
        ))
        .await;
    assert_eq!(
        (changed.status, changed.json()["error"]["code"].clone()),
        (409, json!("idempotency_conflict"))
    );
    let id = review["review_id"].as_str().expect("id");
    let witness = review["witness_sha256"].as_str().expect("witness");
    assert_eq!(
        server
            .request(&decision_request(id, witness, "approve_once"))
            .await
            .status,
        200
    );
    let first = server.request(&reviewed_job(&review, "job-1")).await;
    assert_eq!(first.status, 202, "{}", first.body);
    let lost = server.request(&reviewed_job(&review, "job-1")).await;
    assert_eq!(lost.status, 200, "{}", lost.body);
    assert_eq!(lost.json()["id"], first.json()["id"]);
    let observed = server
        .request(&get_request(&format!("/v1/cost-reviews/{id}")))
        .await
        .json();
    assert_eq!(observed["state"], "consumed");
    server.stop().await.expect("stop");
}

/// A program changed between approval and admission spends the review with
/// no job; a workflow that needs no review answers so without holding anything.
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_program_spends_the_review_and_a_priced_one_needs_none() {
    let world = world();
    let (server, state) = start(
        &world,
        Arc::new(ReviewedBackend::default()),
        disarmed(),
        true,
    )
    .await;
    let review = open_review(&server, "review-1").await;
    let id = review["review_id"].as_str().expect("id");
    let witness = review["witness_sha256"].as_str().expect("witness");
    assert_eq!(
        server
            .request(&decision_request(id, witness, "approve_once"))
            .await
            .status,
        200
    );
    std::fs::write(
        world.workflows.join("review.nika"),
        REVIEWED.replace("hi", "changed"),
    )
    .expect("edit");
    let job = server.request(&reviewed_job(&review, "job-1")).await;
    assert_eq!(
        (job.status, job.json()["error"]["code"].clone()),
        (409, json!("review_witness_changed"))
    );
    let spent = server
        .request(&get_request(&format!("/v1/cost-reviews/{id}")))
        .await
        .json();
    assert_eq!(spent["state"], "refused");
    assert!(lease_is_free(&world));
    *state
        .cost_review
        .as_ref()
        .expect("door")
        .plan
        .lock()
        .expect("plan") = None;
    let none = server
        .request(&review_request(r#"{"workflow":"root.nika"}"#, "review-2"))
        .await;
    assert_eq!(none.status, 200, "{}", none.body);
    assert_eq!(none.json()["review_required"], false);
    assert_eq!(none.json()["observer"], false);
    server.stop().await.expect("stop");
}

/// The public document names where the request goes, never how: no endpoint
/// path, no candidate or private nonce; the witness is 64 hex characters.
#[tokio::test(flavor = "multi_thread")]
async fn the_public_review_never_shows_the_endpoint_path_or_the_nonce() {
    let world = world();
    let (server, _state) = start(
        &world,
        Arc::new(ReviewedBackend::default()),
        disarmed(),
        true,
    )
    .await;
    let created = server
        .request(&review_request(REVIEW_BODY, "review-1"))
        .await;
    let config = nika_runtime::compose::config_from_env();
    let route =
        nika_providers::admission::CostRoute::observe("deepseek/c6-unpriced-fixture", config)
            .expect("route");
    let path = route
        .endpoint
        .trim_start_matches(&route.origin().replace(":443", ""));
    assert!(
        !path.is_empty() && !created.body.contains(path),
        "{}",
        created.body
    );
    for private in ["nonce", "candidate", "review_details", "invocation\""] {
        assert!(
            !created.body.contains(private),
            "{private}: {}",
            created.body
        );
    }
    let witness = created.json()["witness_sha256"]
        .as_str()
        .expect("witness")
        .to_owned();
    assert!(witness.len() == 64 && witness.chars().all(|c| c.is_ascii_hexdigit()));
    server.stop().await.expect("stop");
}

/// The served contract is the one the door enforces: without the door the
/// served document is exactly the live one; with it, the review's lifetime,
/// retention, states, decisions and job refusals are the door's own words.
#[test]
fn the_served_contract_pins_the_door_s_enforced_words() {
    use nika_dap::cost_journal::{REVIEW_TTL as TTL, REVIEWS_RETAINED as RETAINED};
    assert_eq!(
        super::openapi::served(false, false),
        super::openapi::live(false)
    );
    assert_eq!(
        super::openapi::served(true, false),
        super::openapi::live(true)
    );
    let served = super::openapi::served(false, true);
    let review = &served["components"]["schemas"]["CostReview"];
    let text = review["description"].as_str().expect("description");
    assert!(
        text.contains(&format!("{} seconds", TTL.as_secs())),
        "{text}"
    );
    assert!(text.contains(&format!("newest {RETAINED}")), "{text}");
    let states = [
        "pending",
        "approved",
        "declined",
        "expired",
        "admitting",
        "consumed",
        "refused",
        "failed",
    ];
    assert_eq!(review["properties"]["state"]["enum"], json!(states));
    let decision = &served["components"]["schemas"]["CostReviewDecision"]["properties"]["decision"];
    assert_eq!(decision["enum"], json!(["approve_once", "decline"]));
    let jobs = served["paths"]["/v1/jobs"]["post"]["responses"]["409"]["description"]
        .as_str()
        .expect("jobs 409");
    for code in [
        "review_witness_mismatch",
        "review_not_approved",
        "review_declined",
        "review_busy",
        "review_consumed",
        "review_request_mismatch",
        "review_witness_changed",
    ] {
        assert!(jobs.contains(code), "{code}");
    }
    let job_by_name = &served["components"]["schemas"]["JobByName"]["properties"];
    assert!(job_by_name.get("cost_review").is_some());
    assert!(
        served["components"]["schemas"]["Error"]
            == super::openapi::live(false)["components"]["schemas"]["Error"],
        "the error envelope stays closed and unchanged"
    );
}

/// A zero ceiling is a binding veto, never a disarm (C6): the server starts
/// under it; a review refuses before any question (never an `approve_once`);
/// a priced job through the production backend fails before its first event.
#[tokio::test(flavor = "multi_thread")]
async fn a_zero_ceiling_is_a_binding_veto() {
    let world = world();
    let priced = "nika: priced\nmodel: deepseek/deepseek-flash\npermits: {}\ntasks:\n  ask:\n    infer: { prompt: hi, max_tokens: 512 }\n";
    std::fs::write(world.workflows.join("priced.nika"), priced).expect("workflow");
    let zero = limits().with_default_max_cost_usd(Some(0.0));
    assert!(zero.valid(), "zero is a valid, binding ceiling");
    let backend = Arc::new(ResidentExecutionBackend::new(&world.workflows));
    let (server, _state) = start(&world, backend, zero, true).await;
    let review = server
        .request(&review_request(REVIEW_BODY, "review-0"))
        .await;
    assert_eq!(review.status, 422, "{}", review.body);
    assert_eq!(review.json()["error"]["code"], "cost_review_refused");
    assert!(lease_is_free(&world));
    let job = server
        .request(&post_request(
            r#"{"workflow":"priced.nika"}"#,
            "job-0",
            &auth_header(),
        ))
        .await;
    assert_eq!(job.status, 202, "{}", job.body);
    let id = job.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "failed")
        .await
        .expect("the veto fails the job");
    let failed = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    let message = failed["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        message.contains("cost floor") && message.contains("0.000000"),
        "{failed}"
    );
    let traces = world.workflows.join(".nika").join("traces");
    let events = std::fs::read_dir(&traces).map(Iterator::count).unwrap_or(0);
    assert_eq!(
        events, 0,
        "refused before the prologue: no journal, no event"
    );
    server.stop().await.expect("stop");
}

/// A project `ceiling:` of zero is refused by the project file itself (a
/// ceiling bounds at the positive real), so the door frames no review, holds
/// nothing, and no decision could ever override it.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_zero_ceiling_refuses_the_review_before_any_question() {
    let world = world();
    std::fs::write(
        world.workflows.join("nika.yaml"),
        "nika: zero\nceiling: 0\n",
    )
    .expect("project");
    let (server, _state) = start(
        &world,
        Arc::new(ReviewedBackend::default()),
        disarmed(),
        true,
    )
    .await;
    let refused = server
        .request(&review_request(REVIEW_BODY, "review-0"))
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(refused.json()["error"]["code"], "cost_review_refused");
    assert!(refused.body.contains("positive real"), "{}", refused.body);
    assert!(lease_is_free(&world), "a refused review holds nothing");
    assert!(rows(&world).is_empty(), "nothing was prepared");
    server.stop().await.expect("stop");
}
