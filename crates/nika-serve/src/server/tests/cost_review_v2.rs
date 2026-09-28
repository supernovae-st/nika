// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! B12 · the cost-review door at version 2 over real HTTP: a finite fan and an
//! authored retry are reviewed with their typed dispatch bound, which the
//! witness covers and one approval confirms for one `POST /v1/jobs`. Version 1
//! keeps its closed document and refuses what only version 2 can show; the
//! versions never cross; the hard-cap remedy follows only its own refusal. The
//! review's plan is injected; nothing is dispatched to any provider.

use super::cost_review::{
    ReviewedBackend, decision_request, disarmed, lease_is_free, rows, start, world,
};
use super::*;

const FAN: &str = "nika: fan\nmodel: deepseek/c6-unpriced-fixture\npermits: {}\ntasks:\n  review:\n    for_each: { items: [a, b, c], max_parallel: 2 }\n    retry: { max_attempts: 2 }\n    infer: { prompt: 'x ${{ item }}', max_tokens: 16 }\n";

fn request(version: u8, body: &str, key: &str) -> String {
    format!(
        "POST /v{version}/cost-reviews HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\nIdempotency-Key: {key}\r\n{}\r\n{body}",
        body.len(),
        auth_header()
    )
}

fn named(workflow: &str) -> String {
    json!({"workflow": workflow}).to_string()
}

/// A world that serves `fan.nika` (with or without its retry) beside `review.nika`.
fn fan_world(source: &str) -> TestWorld {
    let world = world();
    std::fs::write(world.workflows.join("fan.nika"), source).expect("workflow");
    world
}

fn decide(version: u8, review: &Value, decision: &str) -> String {
    let id = review["review_id"].as_str().expect("id");
    let witness = review["witness_sha256"].as_str().expect("witness");
    decision_request(id, witness, decision).replacen("/v1/", &format!("/v{version}/"), 1)
}

/// Only the operator seats the door; seated, health lists both versions and
/// the served contract carries both, each version's review closed.
#[tokio::test(flavor = "multi_thread")]
async fn version_2_is_served_beside_version_1_only_when_seated() {
    let world = fan_world(FAN);
    let backend = Arc::new(ReviewedBackend::default());
    let (server, _state) = start(&world, backend, disarmed(), false).await;
    let refused = server.request(&request(2, &named("fan.nika"), "k-0")).await;
    assert_eq!(refused.status, 403, "{}", refused.body);
    let health = server.request(&get_request("/health")).await.json();
    assert!(!health.to_string().contains("costReviewV2"));
    server.stop().await.expect("stop");
    let backend = Arc::new(ReviewedBackend::default());
    let (server, _state) = start(&world, backend, disarmed(), true).await;
    let health = server.request(&get_request("/health")).await.json();
    let caps = health["supportedCapabilities"].as_array().expect("caps");
    assert!(caps.contains(&json!("costReviewV1")) && caps.contains(&json!("costReviewV2")));
    let contract = server
        .request(&get_request("/v1/openapi.json"))
        .await
        .json();
    for path in [
        "/v2/cost-reviews",
        "/v2/cost-reviews/{id}",
        "/v2/cost-reviews/{id}/decision",
        "/v1/cost-reviews",
    ] {
        assert!(contract["paths"].get(path).is_some(), "{path}");
    }
    let schemas = &contract["components"]["schemas"];
    for (name, version) in [("CostReview", 1), ("CostReviewV2", 2)] {
        assert_eq!(schemas[name]["additionalProperties"], false, "{name}");
        let pinned = &schemas[name]["properties"]["cost_review_version"]["const"];
        assert_eq!(pinned, &json!(version), "{name}");
    }
    assert!(
        schemas["CostReview"]["properties"]
            .get("dispatch")
            .is_none()
    );
    server.stop().await.expect("stop");
}

/// A fan with an authored retry: the v2 review shows the typed bound, the
/// witness covers it, one approval admits one job, and the job's account
/// carries exactly the reviewed total, width and retry law.
#[tokio::test(flavor = "multi_thread")]
async fn a_fan_review_carries_its_typed_bound_into_one_job() {
    let world = fan_world(FAN);
    let backend = Arc::new(ReviewedBackend::default());
    let (server, _state) = start(&world, backend.clone(), disarmed(), true).await;
    let created = server.request(&request(2, &named("fan.nika"), "k-1")).await;
    assert_eq!(created.status, 201, "{}", created.body);
    let review = created.json();
    assert_eq!(review["cost_review_version"], 2);
    assert_eq!(
        review["bounds"],
        json!({"max_requests": 6, "max_in_flight": 2, "max_output_tokens": 8192,
            "request_timeout_seconds": 120, "transport_retries": 0})
    );
    assert_eq!(
        review["dispatch"],
        json!({"requests": 6, "max_in_flight": 2, "authored_retry": true, "tasks": [
            {"task": "review", "items": 3, "attempts": 2, "calls_per_attempt": 1,
             "max_parallel": 2, "requests": 6}]})
    );
    let question = review["question"].as_str().expect("question");
    assert!(question.contains("`review`: 3 items × 2 attempts × 1 call = 6 requests"));
    assert!(question.contains("Task retries authored in the workflow"));
    let approved = server.request(&decide(2, &review, "approve_once")).await;
    assert_eq!(approved.status, 200, "{}", approved.body);
    let body = json!({"workflow": "fan.nika", "cost_review": {
        "review_id": review["review_id"], "witness_sha256": review["witness_sha256"]}});
    let job = server
        .request(&post_request(&body.to_string(), "job-1", &auth_header()))
        .await;
    assert_eq!(job.status, 202, "{}", job.body);
    let id = job.json()["id"].as_str().expect("job id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("the reviewed job settles");
    let choices = backend.choices.lock().expect("choices").clone();
    assert_eq!(choices.len(), 1);
    let choice = &choices[0];
    assert_eq!(
        (&choice["max_requests"], &choice["max_in_flight"]),
        (&json!(6), &json!(2))
    );
    assert_eq!(choice["authored_retry"], true);
    let phases: Vec<Value> = rows(&world).iter().map(|r| r["phase"].clone()).collect();
    assert_eq!(phases, ["prepared", "settled"]);
    server.stop().await.expect("stop");
}

/// Public-wire controls: a single-attempt Run and a fan without retries keep
/// the retry law off; the single Run's v2 question is v1's, byte for byte.
#[tokio::test(flavor = "multi_thread")]
async fn single_attempt_and_unretried_fan_keep_the_retry_law_off() {
    let world = fan_world(&FAN.replace("    retry: { max_attempts: 2 }\n", ""));
    let backend = Arc::new(ReviewedBackend::default());
    let (server, _state) = start(&world, backend, disarmed(), true).await;
    let v1 = server
        .request(&request(1, &named("review.nika"), "k-1"))
        .await;
    assert_eq!(v1.status, 201, "{}", v1.body);
    let v1 = v1.json();
    assert!(v1.get("dispatch").is_none(), "version 1 stays closed");
    server.request(&decide(1, &v1, "decline")).await;
    let single = server
        .request(&request(2, &named("review.nika"), "k-2"))
        .await;
    assert_eq!(single.status, 201, "{}", single.body);
    let single = single.json();
    assert_eq!(single["question"], v1["question"]);
    assert_eq!(
        single["dispatch"],
        json!({"requests": 1, "max_in_flight": 1, "authored_retry": false, "tasks": [
            {"task": "ask", "items": null, "attempts": 1, "calls_per_attempt": 1,
             "max_parallel": 1, "requests": 1}]})
    );
    server.request(&decide(2, &single, "decline")).await;
    let fan = server.request(&request(2, &named("fan.nika"), "k-3")).await;
    assert_eq!(fan.status, 201, "{}", fan.body);
    let fan = fan.json();
    assert_eq!(fan["dispatch"]["requests"], 3);
    assert_eq!(fan["dispatch"]["authored_retry"], false);
    assert!(
        !fan["question"]
            .as_str()
            .expect("q")
            .contains("Task retries")
    );
    server.request(&decide(2, &fan, "decline")).await;
    assert!(lease_is_free(&world));
    server.stop().await.expect("stop");
}

/// A mutated witness or request never admits: the decision and the job both
/// refuse, and a changed program spends the approved review with no job.
#[tokio::test(flavor = "multi_thread")]
async fn witness_and_choice_mutations_are_rejected() {
    let world = fan_world(FAN);
    let backend = Arc::new(ReviewedBackend::default());
    let (server, _state) = start(&world, backend.clone(), disarmed(), true).await;
    let review = server
        .request(&request(2, &named("fan.nika"), "k-1"))
        .await
        .json();
    let mut forged = review.clone();
    forged["witness_sha256"] = json!("0".repeat(64));
    let refused = server.request(&decide(2, &forged, "approve_once")).await;
    assert_eq!(refused.json()["error"]["code"], "review_witness_mismatch");
    assert_eq!(
        server
            .request(&decide(2, &review, "approve_once"))
            .await
            .status,
        200
    );
    let job = |witness: &Value, extra: Value| {
        let mut body = json!({"workflow": "fan.nika", "cost_review": {
            "review_id": review["review_id"], "witness_sha256": witness}});
        body.as_object_mut()
            .expect("body")
            .extend(extra.as_object().cloned().unwrap_or_default());
        body.to_string()
    };
    let forged_job = job(&json!("f".repeat(64)), json!({}));
    let forged_job = server
        .request(&post_request(&forged_job, "job-w", &auth_header()))
        .await;
    assert_eq!(
        forged_job.json()["error"]["code"],
        "review_witness_mismatch"
    );
    let other = job(&review["witness_sha256"], json!({"access": "api"}));
    let other = server
        .request(&post_request(&other, "job-r", &auth_header()))
        .await;
    assert_eq!(other.json()["error"]["code"], "review_request_mismatch");
    std::fs::write(
        world.workflows.join("fan.nika"),
        FAN.replace("[a, b, c]", "[a, b]"),
    )
    .expect("edit");
    let changed = job(&review["witness_sha256"], json!({}));
    let changed = server
        .request(&post_request(&changed, "job-c", &auth_header()))
        .await;
    assert_eq!(changed.json()["error"]["code"], "review_witness_changed");
    assert!(backend.runs.lock().expect("runs").is_empty(), "no job ran");
    assert!(lease_is_free(&world));
    server.stop().await.expect("stop");
}

/// Version 1 refuses what only version 2 can show, naming the route whole;
/// version 2 answers zero work as no review; versions never cross.
#[tokio::test(flavor = "multi_thread")]
async fn version_1_refuses_multiplied_runs_and_versions_never_cross() {
    let world = fan_world(FAN);
    let empty = FAN.replace("[a, b, c]", "[]");
    std::fs::write(world.workflows.join("empty.nika"), &empty).expect("workflow");
    let backend = Arc::new(ReviewedBackend::default());
    let (server, _state) = start(&world, backend, disarmed(), true).await;
    for (workflow, key) in [("fan.nika", "v1-fan"), ("empty.nika", "v1-empty")] {
        let v1 = server.request(&request(1, &named(workflow), key)).await;
        assert_eq!(v1.status, 422, "{}", v1.body);
        let message = v1.json()["error"]["message"]
            .as_str()
            .expect("m")
            .to_owned();
        assert!(message.contains("POST /v2/cost-reviews"), "{message}");
        assert!(!message.contains("--run-cost-ceiling"), "{message}");
        assert!(lease_is_free(&world), "{workflow}");
    }
    let zero = server
        .request(&request(2, &named("empty.nika"), "k-z"))
        .await;
    assert_eq!(zero.status, 200, "{}", zero.body);
    let zero = zero.json();
    assert_eq!(
        (
            &zero["cost_review_version"],
            &zero["review_required"],
            &zero["observer"]
        ),
        (&json!(2), &json!(false), &json!(true))
    );
    assert!(zero["reason"].as_str().expect("reason").contains("zero"));
    assert!(lease_is_free(&world), "zero work holds nothing");
    let review = server
        .request(&request(2, &named("fan.nika"), "k-2"))
        .await
        .json();
    let id = review["review_id"].as_str().expect("id");
    let crossed = server
        .request(&get_request(&format!("/v1/cost-reviews/{id}")))
        .await;
    assert_eq!(crossed.json()["error"]["code"], "review_unknown");
    let crossed = server.request(&decide(1, &review, "decline")).await;
    assert_eq!(crossed.json()["error"]["code"], "review_unknown");
    let replay = server.request(&request(1, &named("fan.nika"), "k-2")).await;
    assert_eq!(replay.json()["error"]["code"], "idempotency_conflict");
    let same = server.request(&request(2, &named("fan.nika"), "k-2")).await;
    assert_eq!(
        (same.status, &same.json()["review_id"]),
        (200, &review["review_id"])
    );
    let pending = server
        .request(&get_request(&format!("/v2/cost-reviews/{id}")))
        .await
        .json();
    assert_eq!(pending["state"], "pending");
    server.stop().await.expect("stop");
}

/// Under a present server ceiling, a v2 fan is refused by the cap and names
/// its remedy; a count only the run decides is refused in its own words.
#[tokio::test(flavor = "multi_thread")]
async fn a_v2_review_names_the_cap_remedy_only_for_the_cap() {
    let world = fan_world(FAN);
    let upstream = FAN
        .replace("permits: {}", "permits: { tools: ['nika:jq'] }")
        .replace(
            "  review:\n    for_each: { items: [a, b, c], max_parallel: 2 }\n",
            "  load:\n    invoke: { tool: 'nika:jq', args: { input: [1], expression: '.' } }\n  review:\n    with: { list: '${{ tasks.load.output }}' }\n    for_each: { items: '${{ with.list }}' }\n",
        );
    std::fs::write(world.workflows.join("upstream.nika"), upstream).expect("workflow");
    let limits = limits().with_default_max_cost_usd(Some(1.0));
    let (server, _state) = start(&world, Arc::new(ReviewedBackend::default()), limits, true).await;
    let capped = server.request(&request(2, &named("fan.nika"), "k-1")).await;
    assert_eq!(capped.status, 422, "{}", capped.body);
    assert!(
        capped.body.contains("--run-cost-ceiling none"),
        "{}",
        capped.body
    );
    let decided = server
        .request(&request(2, &named("upstream.nika"), "k-2"))
        .await;
    assert_eq!(decided.status, 422, "{}", decided.body);
    assert!(
        decided.body.contains("a count only the run decides"),
        "{}",
        decided.body
    );
    assert!(
        !decided.body.contains("--run-cost-ceiling"),
        "{}",
        decided.body
    );
    assert!(lease_is_free(&world));
    server.stop().await.expect("stop");
}
