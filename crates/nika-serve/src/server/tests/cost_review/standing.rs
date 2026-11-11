// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The operator's standing cost policy over real HTTP: health and the served
//! contract show it only when the backend holds one; a job on a route a rule
//! names runs through the production backend with no review, and its record,
//! its cost journal and its trace say how it was admitted; a route no rule
//! names fails in the policy's words; and the review door answers that a Run
//! the policy admits needs no review. The access plan is injected and the
//! fixture Run's only model call is gated off, so nothing is dispatched.

use super::super::declared::{assert_declared_on, served_document};
use super::super::trace_journal::TestSeal;
use super::*;
use crate::server::production::ResidentExecutionBackend;
use nika_providers::admission::{StandingCostPolicy, StandingRule};

const MODEL: &str = "deepseek/c6-unpriced-fixture";

/// The fixture Run a rule admits: one bounded model call, gated off, so the
/// Run is judged for one request and sends none.
const GATED: &str = "nika: standing\nmodel: deepseek/c6-unpriced-fixture\npermits: {}\ntasks:\n  ask:\n    when: false\n    infer: { prompt: hi, max_tokens: 16 }\n";

fn rule(model: &str) -> String {
    format!("{model}:requests=4,output-tokens=64,timeout=30,runs=3/hour")
}

fn policy(rule: &str) -> StandingCostPolicy {
    let config = nika_runtime::compose::config_from_env();
    StandingCostPolicy::resolve(&[rule.to_owned()], &config).expect("the fixture rule")
}

fn rule_id(rule: &str) -> String {
    StandingRule::parse(rule).expect("rule").id().to_owned()
}

/// The production backend holding `policy`, every job under the fixture plan.
fn production(world: &TestWorld, policy: StandingCostPolicy) -> Arc<dyn ExecutionBackend> {
    let key = minisign::KeyPair::generate_unencrypted_keypair().expect("test key");
    let public = key.pk.to_box().expect("public box").to_string();
    let seal = Arc::new(TestSeal {
        secret: key.sk,
        public,
    });
    Arc::new(
        ResidentExecutionBackend::new(&world.workflows)
            .with_journal_seal(seal)
            .with_cost_policy(policy)
            .with_access_plan(unpriced_plan()),
    )
}

/// A backend that runs nothing and reports the policy it holds: what the
/// door and the health document read.
struct Holding(StandingCostPolicy);

impl ExecutionBackend for Holding {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async { ExecutionOutcome::from(ExecutionDisposition::Succeeded) })
    }

    fn standing_cost_policy(&self) -> Option<StandingCostPolicy> {
        Some(self.0.clone())
    }
}

/// Limits that leave the production backend's composition time on a loaded
/// host, under the per-run ceiling `ceiling`.
fn generous(ceiling: Option<f64>) -> ServerLimits {
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
    .with_default_max_cost_usd(ceiling)
}

fn standing_world() -> TestWorld {
    let world = world();
    std::fs::write(world.workflows.join("standing.nika"), GATED).expect("workflow");
    world
}

fn ledger(world: &TestWorld) -> Value {
    let path = world
        .workflows
        .join(".nika")
        .join(nika_dap::cost_journal::STANDING_LEDGER);
    serde_json::from_str(&std::fs::read_to_string(path).expect("the ledger")).expect("JSON")
}

fn review_at(version: u64, body: &str, key: &str) -> String {
    format!(
        "POST /v{version}/cost-reviews HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\nIdempotency-Key: {key}\r\n{}\r\n{body}",
        body.len(),
        auth_header()
    )
}

fn capabilities(health: &Value) -> Vec<String> {
    let listed = health["supportedCapabilities"].as_array().expect("caps");
    listed
        .iter()
        .filter_map(|cap| cap.as_str().map(str::to_owned))
        .collect()
}

/// Defaults do not change: a server whose backend holds no policy advertises
/// nothing and serves the document it always served. The capability and the
/// contract appear only with a policy, and the door's answer only beside the door.
#[tokio::test(flavor = "multi_thread")]
async fn health_and_the_contract_show_the_policy_only_when_the_backend_holds_one() {
    let world = standing_world();
    let plain = Arc::new(ReviewedBackend::default());
    let (server, _state) = start(&world, plain, disarmed(), false).await;
    let health = server.request(&get_request("/health")).await.json();
    assert!(!capabilities(&health).contains(&"costStandingPolicyV1".to_owned()));
    let plain_contract = served_document(&server).await;
    assert_eq!(
        plain_contract,
        super::super::openapi::served(false, false, false)
    );
    assert!(
        plain_contract["components"]["schemas"]
            .get("CostAdmission")
            .is_none()
    );
    let job = &plain_contract["components"]["schemas"]["Job"]["properties"];
    assert!(job.get("cost_admission").is_none());
    server.stop().await.expect("stop");

    let holding = || Arc::new(Holding(policy(&rule(MODEL))));
    let (server, _state) = start(&world, holding(), disarmed(), false).await;
    let health = server.request(&get_request("/health")).await.json();
    let caps = capabilities(&health);
    assert!(
        caps.contains(&"costStandingPolicyV1".to_owned()),
        "{caps:?}"
    );
    assert!(
        !caps.contains(&"costReviewV2".to_owned()),
        "the policy seats no door"
    );
    assert!(
        !health.to_string().contains(MODEL) && !health.to_string().contains("scp-"),
        "health names no route, rule or bound"
    );
    let contract = served_document(&server).await;
    assert_ne!(contract, plain_contract);
    assert_eq!(
        contract,
        super::super::openapi::under_cost_policy(plain_contract, false),
        "the plain contract, and the policy's own additions"
    );
    let schemas = &contract["components"]["schemas"];
    assert_eq!(
        schemas["Job"]["properties"]["cost_admission"]["$ref"],
        "#/components/schemas/CostAdmission"
    );
    assert_eq!(
        schemas["JobEvent"]["properties"]["cost_admission"]["$ref"],
        "#/components/schemas/CostAdmission"
    );
    assert_eq!(
        schemas["CostAdmission"]["properties"]["authority"]["const"],
        "standing_policy"
    );
    assert!(schemas["StandingCostRule"].is_object());
    assert!(
        schemas.get("CostReviewNotRequired").is_none(),
        "the door's answer is described only beside the door"
    );
    server.stop().await.expect("stop");

    let (server, _state) = start(&world, holding(), disarmed(), true).await;
    let health = server.request(&get_request("/health")).await.json();
    let caps = capabilities(&health);
    for cap in ["costReviewV1", "costReviewV2", "costStandingPolicyV1"] {
        assert!(caps.contains(&cap.to_owned()), "{cap}: {caps:?}");
    }
    let contract = served_document(&server).await;
    for answer in ["CostReviewNotRequired", "CostReviewNotRequiredV2"] {
        assert_eq!(
            contract["components"]["schemas"][answer]["properties"]["standing_policy"]["$ref"],
            "#/components/schemas/StandingCostRule",
            "{answer}"
        );
    }
    let door = super::super::openapi::served(false, true, false);
    assert!(
        door["components"]["schemas"]["CostReviewNotRequired"]["properties"]
            .get("standing_policy")
            .is_none(),
        "a door without a policy keeps its contract"
    );
    assert_eq!(
        contract,
        super::super::openapi::under_cost_policy(door, true),
        "the door's contract, and the policy's own additions"
    );
    server.stop().await.expect("stop");
}

/// A job on the route a rule names runs through the production backend with
/// no review and no per-run decision. Its record says a standing rule admitted
/// it, that the price is unknown, and under which bounds; its cost journal and
/// its trace say the same; nothing was sent, and nothing reads as zero or free.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_on_a_named_route_runs_with_no_review_and_every_record_says_how() {
    let world = standing_world();
    let text = rule(MODEL);
    let backend = production(&world, policy(&text));
    let (server, _state) = start(&world, backend, generous(None), false).await;
    let document = served_document(&server).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"standing.nika"}"#,
            "standing-1",
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_patiently_for(&server, &id, "succeeded")
        .await
        .expect("the rule admits the job");
    let answered = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await;
    assert_eq!(answered.status, 200, "{}", answered.body);
    assert_declared_on(
        &document,
        "get",
        "/v1/jobs/{id}",
        200,
        answered.body.as_bytes(),
    );
    let job = answered.json();
    let admission = &job["cost_admission"];
    assert_eq!(admission["schema"], "nika/cost-admission@1");
    assert_eq!(admission["authority"], "standing_policy");
    assert_eq!(admission["price"], "unknown");
    let choice = &admission["choice"];
    assert_eq!(
        choice["standing"],
        json!({"policy": rule_id(&text), "max_runs": 3, "window_seconds": 3_600})
    );
    assert_eq!(choice["provider"], "deepseek");
    assert_eq!(choice["model"], "c6-unpriced-fixture");
    assert_eq!(choice["origin"], "https://api.deepseek.com:443");
    assert_eq!(choice["invocation"], job["execution_id"]);
    assert_eq!(choice["max_requests"], 1);
    assert_eq!(choice["max_output_tokens"], 64);
    assert_eq!(choice["timeout_ms"], 30_000);
    assert!(choice["declared_tariff"].is_null());
    assert_eq!(admission["host"]["allowed"], true);
    assert_eq!(admission["host"]["machine"]["cap"], "absent");
    assert_eq!(admission["host"]["policy"]["cap"], "absent");
    assert!(
        job["settlement"].get("total_cost_usd").is_none(),
        "nothing metered is never a zero: {}",
        job["settlement"]
    );
    let record = answered.body.to_lowercase();
    assert!(
        !record.contains("free") && !record.contains("$0"),
        "{record}"
    );
    // The cost journal: one Run, prepared then settled closed, nothing sent.
    let journal = rows(&world);
    let phases: Vec<Value> = journal.iter().map(|row| row["phase"].clone()).collect();
    assert_eq!(phases, ["prepared", "settled"]);
    for row in &journal {
        assert_eq!(row["invocation"], job["execution_id"]);
        assert_eq!(
            row["observation"]["unknown_cost"]["standing"]["policy"],
            rule_id(&text)
        );
        assert_eq!(row["observation"]["unknown_attempts"], json!([]));
        assert_eq!(row["observation"]["known_subtotal_nano_usd"], "0");
        assert!(row["observation"]["limit_nano_usd"].is_null());
    }
    assert_eq!(journal[1]["observation"]["state"], "Closed");
    assert_eq!(
        ledger(&world)["runs"][MODEL].as_array().map(Vec::len),
        Some(1)
    );
    assert!(lease_is_free(&world), "the settled Run released the lease");
    // The trace: its terminal frame carries the account's observation, the rule included.
    let traces = world.workflows.join(".nika").join("traces");
    let trace = std::fs::read_dir(&traces)
        .expect("the journal directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|e| e == "ndjson"))
        .expect("the run's trace");
    let text_of = std::fs::read_to_string(&trace).expect("trace");
    let terminal = text_of
        .lines()
        .find(|line| line.contains("inference_admission"))
        .expect("a frame carries the account's observation");
    assert!(terminal.contains(&rule_id(&text)), "{terminal}");
    assert!(terminal.contains("standing"), "{terminal}");
    // The terminal event of the job's stream carries the same admission.
    let events = server.request(&events_request(&id, None)).await;
    assert!(
        events.body.contains("\"cost_admission\""),
        "{}",
        events.body
    );
    assert!(events.body.contains(&rule_id(&text)), "{}", events.body);
    server.stop().await.expect("stop");
}

/// With a declared price the server's ceiling stays armed. A job whose
/// declared worst case fits under it runs with no review, and its record says
/// whose price that is: the operator's estimate, with the bound it rests on,
/// the worst case it gives and the ceiling it was judged under.
#[tokio::test(flavor = "multi_thread")]
async fn a_declared_price_runs_under_the_armed_ceiling_and_its_record_says_whose_it_is() {
    let world = standing_world();
    let text = format!(
        "{},request-bytes=2048,input-usd=1,output-usd=2",
        rule(MODEL)
    );
    let backend = production(&world, policy(&text));
    let (server, _state) = start(&world, backend, generous(Some(1.0)), false).await;
    let document = served_document(&server).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"standing.nika"}"#,
            "priced-1",
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_patiently_for(&server, &id, "succeeded")
        .await
        .expect("the declared worst case fits under one dollar");
    let answered = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await;
    assert_eq!(answered.status, 200, "{}", answered.body);
    assert_declared_on(
        &document,
        "get",
        "/v1/jobs/{id}",
        200,
        answered.body.as_bytes(),
    );
    let job = answered.json();
    let admission = &job["cost_admission"];
    assert_eq!(admission["authority"], "standing_policy");
    assert_eq!(admission["price"], "host_declared");
    let choice = &admission["choice"];
    assert_eq!(choice["max_request_bytes"], 2_048);
    // One request: 2,048 bytes at 1,000 nano-USD, 64 output tokens at 2,000.
    assert_eq!(
        choice["standing"],
        json!({"policy": rule_id(&text), "max_runs": 3, "window_seconds": 3_600,
            "declared_worst_case_nano_usd": "2176000"})
    );
    let tariff = &choice["declared_tariff"];
    assert_eq!(tariff["currency"], "USD");
    assert_eq!(tariff["nano_per_token"], json!([1_000, 2_000, 1_000]));
    assert_eq!(tariff["version"], rule_id(&text));
    assert_eq!(
        tariff["provenance"],
        "operator-declared in the host's standing cost policy; an estimate, never a provider invoice"
    );
    assert_eq!(
        admission["host"]["machine"]["cap"],
        json!({"capped_usd": "$1.000000"})
    );
    assert!(
        job["settlement"].get("total_cost_usd").is_none(),
        "a declared worst case is never a spend: {}",
        job["settlement"]
    );
    server.stop().await.expect("stop");
}

/// A route no rule names is still refused, in the policy's words, and its
/// record carries no admission; so is a Run under a ceiling its rule cannot
/// fit under, which names the remedy.
#[tokio::test(flavor = "multi_thread")]
async fn a_job_outside_the_policy_fails_and_carries_no_admission() {
    let world = standing_world();
    let elsewhere = policy(&rule("deepseek/another-unpriced-fixture"));
    let (server, _state) =
        start(&world, production(&world, elsewhere), generous(None), false).await;
    let failed = async |server: &TestServer, key: &str| {
        let created = server
            .request(&post_request(
                r#"{"workflow":"standing.nika"}"#,
                key,
                &auth_header(),
            ))
            .await;
        assert_eq!(created.status, 202, "{}", created.body);
        let id = created.json()["id"].as_str().expect("id").to_owned();
        wait_patiently_for(server, &id, "failed")
            .await
            .expect("the job is refused");
        server
            .request(&get_request(&format!("/v1/jobs/{id}")))
            .await
            .json()
    };
    let job = failed(&server, "unnamed-1").await;
    assert_eq!(job["error"]["code"], "admission_refused");
    assert_eq!(job["error"]["message"], super::super::cost_policy::UNNAMED);
    assert!(job.get("cost_admission").is_none());
    assert!(rows(&world).is_empty(), "nothing was prepared");
    server.stop().await.expect("stop");

    // The default per-run ceiling (1 USD) binds: a rule without a price refuses under it.
    let world = standing_world();
    let text = rule(MODEL);
    let capped = production(&world, policy(&text));
    let (server, _state) = start(&world, capped, generous(Some(1.0)), false).await;
    let job = failed(&server, "capped-1").await;
    assert_eq!(job["error"]["code"], "admission_refused");
    assert_eq!(
        job["error"]["message"],
        format!(
            "price unknown: a $1.000000 ceiling binds this job and the standing cost rule {} declares no price, so the ceiling cannot bound it: declare input-usd and output-usd on the rule, or start with --run-cost-ceiling none",
            rule_id(&text)
        )
    );
    assert!(job.get("cost_admission").is_none());
    assert!(rows(&world).is_empty(), "nothing was prepared");
    server.stop().await.expect("stop");
}

/// One slot of a schedule, as its firing reaches the coordinator.
fn fired_slot(slot: char) -> JobOrigin {
    use nika_cadence::firing::{ArmGeneration, SlotId};
    use nika_cadence::{ScheduleDecision, ScheduleOrigin, ScheduleRevision};
    let revision = format!("sha256:{}", "a".repeat(64));
    let revision = ScheduleRevision::from_wire(&revision).expect("revision");
    let slot = SlotId::from_wire(&slot.to_string().repeat(64)).expect("slot id");
    let generation = ArmGeneration::from_wire(&"c".repeat(64)).expect("generation");
    let at = |instant: &str| instant.parse::<jiff::Timestamp>().expect("instant");
    JobOrigin::schedule(
        ScheduleOrigin::Project,
        "standing",
        &revision,
        &slot,
        ScheduleDecision::Scheduled,
        at("2026-08-30T10:00:00Z"),
        at("2026-08-30T10:00:01Z"),
        &generation,
    )
    .expect("scheduled origin")
}

/// A scheduled occurrence reaches the same gate through the coordinator,
/// under the ceiling a schedule always carries. A rule that declares a price
/// admits it when its worst case fits, and the job's record names the
/// occurrence and that ceiling; a rule without a price refuses it and says
/// what a schedule needs.
#[tokio::test(flavor = "multi_thread")]
async fn a_scheduled_occurrence_runs_only_under_a_declared_price_and_is_recorded_as_one() {
    let fired = async |text: &str, slot: char| {
        let world = standing_world();
        let backend = production(&world, policy(text));
        let (server, state) = start(&world, backend, generous(None), false).await;
        let admitted = crate::server::route::admit_by_name("standing.nika", &state).await;
        let admitted = admitted.expect("a served workflow");
        let (coordinator, origin) = (state.coordinator.clone(), fired_slot(slot));
        let prepared = tokio::task::spawn_blocking(move || {
            let inputs = std::collections::BTreeMap::new();
            coordinator.prepare_scheduled_with_max_cost(admitted, origin, Some(0.05), inputs)
        });
        let prepared = prepared
            .await
            .expect("task")
            .expect("scheduled preparation");
        let id = prepared.run_id().to_string();
        let record = tokio::task::spawn_blocking(move || prepared.execute());
        let record = record.await.expect("task").expect("scheduled execution");
        let read = get_request(&format!("/v1/jobs/{id}"));
        let job = server.request(&read).await.json();
        server.stop().await.expect("stop");
        (record.status(), job)
    };
    let priced = format!(
        "{},request-bytes=2048,input-usd=1,output-usd=2",
        rule(MODEL)
    );
    let (status, job) = fired(&priced, 'a').await;
    assert_eq!(status, JobStatus::Succeeded, "{job}");
    assert_eq!(job["receipt"]["origin"]["kind"], "schedule", "{job}");
    let admission = &job["cost_admission"];
    assert_eq!(admission["price"], "host_declared", "{job}");
    assert_eq!(
        admission["host"]["occurrence"],
        json!({"class": "not_applicable", "origin": "scheduled occurrence: its required maxCostUsd is the per-run ceiling judged as the machine layer"})
    );
    assert_eq!(
        admission["host"]["machine"]["cap"],
        json!({"capped_usd": "$0.050000"})
    );
    let text = rule(MODEL);
    let (status, job) = fired(&text, 'b').await;
    assert_eq!(status, JobStatus::Failed, "{job}");
    assert_eq!(job["error"]["code"], "admission_refused", "{job}");
    assert_eq!(
        job["error"]["message"],
        format!(
            "price unknown: a $0.050000 ceiling binds this job and the standing cost rule {} declares no price, so the ceiling cannot bound it: a schedule always carries maxCostUsd, so its rule must declare input-usd and output-usd",
            rule_id(&text)
        )
    );
    assert!(job.get("cost_admission").is_none(), "{job}");
}

/// A Session's review is judged under the Session's own ceiling and never
/// reads the policy: where a manual review of the same Run is told it needs
/// none, the Session's is framed for its human and held.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_review_never_reads_the_policy() {
    let world = world();
    let held = policy(&rule(MODEL));
    let (server, state) = start(&world, Arc::new(Holding(held)), disarmed(), true).await;
    let door = state.cost_review.as_ref().expect("the door");
    let asked = async |invocation: Option<f64>| {
        let job = crate::server::route::named_job(REVIEW_BODY.as_bytes());
        let job = job.ok().flatten().expect("a job by name");
        let admitted = crate::server::route::admit_by_name(&job.workflow, &state).await;
        let admitted = admitted.expect("a served workflow");
        let asked = (2, invocation);
        crate::server::cost_review::review_admitted(&state, door, job, admitted, asked)
            .await
            .expect("the door judged it")
    };
    let manual = asked(None).await.expect_err("a manual review needs none");
    assert_eq!(manual["review_required"], false);
    assert!(manual["standing_policy"].is_object(), "{manual}");
    assert!(lease_is_free(&world), "nothing is held for it");
    let session = asked(Some(1.0))
        .await
        .expect("a Session's review is framed");
    assert!(session.starts_with("rev-"), "{session}");
    assert!(!lease_is_free(&world), "held for the Session's human");
    server.stop().await.expect("stop");
}

/// Version 1 of the door never answers for a Run it cannot describe. A fan
/// with an authored retry that the policy admits is still refused there, in
/// version 1's own words, which name version 2; version 2 says the Run needs
/// no review and names the rule.
#[tokio::test(flavor = "multi_thread")]
async fn version_1_keeps_refusing_a_fan_the_policy_admits() {
    const FAN: &str = "nika: fan\nmodel: deepseek/c6-unpriced-fixture\npermits: {}\ntasks:\n  review:\n    for_each: { items: [a, b, c], max_parallel: 2 }\n    retry: { max_attempts: 2 }\n    infer: { prompt: 'x ${{ item }}', max_tokens: 16 }\n";
    let world = world();
    std::fs::write(world.workflows.join("fan.nika"), FAN).expect("workflow");
    // Three items, two attempts each: six requests, inside a rule of six.
    let text = rule(MODEL).replace("requests=4", "requests=6");
    let backend = Arc::new(Holding(policy(&text)));
    let (server, _state) = start(&world, backend, disarmed(), true).await;
    let body = r#"{"workflow":"fan.nika"}"#;
    let v1 = server.request(&review_at(1, body, "fan-v1")).await;
    assert_eq!(v1.status, 422, "{}", v1.body);
    let refused = v1.json();
    let message = refused["error"]["message"].as_str().expect("message");
    assert!(message.contains("POST /v2/cost-reviews"), "{message}");
    assert!(!v1.body.contains("standing_policy"), "{}", v1.body);
    let v2 = server.request(&review_at(2, body, "fan-v2")).await;
    assert_eq!(v2.status, 200, "{}", v2.body);
    let answered = v2.json();
    assert_eq!(answered["review_required"], false);
    assert_eq!(answered["standing_policy"]["policy"], rule_id(&text));
    assert_eq!(answered["standing_policy"]["max_requests"], 6);
    assert!(rows(&world).is_empty(), "the door prepared nothing");
    assert!(lease_is_free(&world), "and holds nothing");
    server.stop().await.expect("stop");
}

/// Beside the review door, a Run the policy admits needs no review: the door
/// says so at both versions, names the rule, holds nothing, and a client that
/// asked for a review posts the job without one. A Run the policy does not
/// admit (another route, a full window) still gets its review.
#[tokio::test(flavor = "multi_thread")]
async fn the_review_door_says_a_run_the_policy_admits_needs_no_review() {
    let world = world();
    let text = rule(MODEL);
    let held = policy(&text);
    let backend = Arc::new(Holding(held.clone()));
    let (server, _state) = start(&world, backend, disarmed(), true).await;
    let document = served_document(&server).await;
    let route = nika_providers::admission::CostRoute::observe(
        MODEL,
        nika_runtime::compose::config_from_env(),
    )
    .expect("route");
    let rule_view = held.rule_for(&route).expect("rule").view();
    for version in [1, 2] {
        let answered = server
            .request(&review_at(
                version,
                REVIEW_BODY,
                &format!("standing-{version}"),
            ))
            .await;
        assert_eq!(answered.status, 200, "{}", answered.body);
        let template = format!("/v{version}/cost-reviews");
        assert_declared_on(&document, "post", &template, 200, answered.body.as_bytes());
        assert_eq!(
            answered.json(),
            json!({"cost_review_version": version, "review_required": false,
                "observer": false, "reason": super::super::cost_policy::NOT_REQUIRED,
                "standing_policy": rule_view})
        );
        let replayed = server
            .request(&review_at(
                version,
                REVIEW_BODY,
                &format!("standing-{version}"),
            ))
            .await;
        assert_eq!(
            replayed.json(),
            answered.json(),
            "a replay answers the same"
        );
    }
    assert!(rows(&world).is_empty(), "the door prepared nothing");
    assert!(lease_is_free(&world), "and holds nothing");
    // A Run in flight holds the project's cost lease: the answer does not wait for it,
    // where a review would be refused until that Run settles.
    let project = nika_fs::OwnedDir::open(&world.workflows).expect("project");
    let in_flight = nika_dap::cost_journal::clear(&project, "in-flight")
        .expect("journal")
        .expect("the lease");
    let answered = server
        .request(&review_at(2, REVIEW_BODY, "while-running"))
        .await;
    assert_eq!(answered.status, 200, "{}", answered.body);
    assert_eq!(answered.json()["standing_policy"], rule_view);
    drop(in_flight);
    let ledger_path = world
        .workflows
        .join(".nika")
        .join(nika_dap::cost_journal::STANDING_LEDGER);
    assert!(!ledger_path.exists(), "asking counts no Run");
    // A full window: the policy no longer admits the Run, and the door reviews it.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let full = json!({"schema": "nika/standing-cost-runs@1", "runs": {MODEL: [now, now, now]}});
    std::fs::write(&ledger_path, full.to_string()).expect("ledger");
    let reviewed = server
        .request(&review_at(2, REVIEW_BODY, "full-window"))
        .await;
    assert_eq!(reviewed.status, 201, "{}", reviewed.body);
    assert_eq!(reviewed.json()["state"], "pending");
    assert!(reviewed.json().get("standing_policy").is_none());
    server.stop().await.expect("stop");

    // A policy that names another route leaves this Run to its review, as before.
    let world = self::world();
    let elsewhere = Arc::new(Holding(policy(&rule("deepseek/another-unpriced-fixture"))));
    let (server, _state) = start(&world, elsewhere, disarmed(), true).await;
    let reviewed = server.request(&review_at(2, REVIEW_BODY, "unnamed")).await;
    assert_eq!(reviewed.status, 201, "{}", reviewed.body);
    assert_eq!(reviewed.json()["review_required"], Value::Null);
    assert_eq!(
        reviewed.json()["host"]["evidence"]["policy"]["origin"],
        "nika serve composition: no monetary policy source is supported by this build",
        "a review's own document is unchanged"
    );
    server.stop().await.expect("stop");

    // Under the server's ceiling a rule without a price admits nothing: the door
    // keeps its own refusal and its own remedy.
    let world = self::world();
    let capped = Arc::new(Holding(policy(&text)));
    let limits = limits().with_default_max_cost_usd(Some(1.0));
    let (server, _state) = start(&world, capped, limits, true).await;
    let refused = server.request(&review_at(2, REVIEW_BODY, "capped")).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(refused.json()["error"]["code"], "cost_review_refused");
    assert!(
        refused.body.contains("--run-cost-ceiling none"),
        "{}",
        refused.body
    );
    server.stop().await.expect("stop");
}
