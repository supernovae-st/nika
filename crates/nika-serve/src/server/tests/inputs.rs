// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! HTTP-to-production input contracts. Pure jq tasks; no provider or signing keys.
use super::*;
use crate::server::production::{JournalSeal, ResidentExecutionBackend};

const INPUT_WORKFLOW: &str = r"nika: bound-inputs
inputs:
  ticket: {type: string, required: true}
  count: {type: integer, required: true}
  tags: {type: {array: string}, required: true}
  record: {type: {object: {name: string}}, required: true}
  region: {type: string, default: eu}
permits: {tools: ['nika:jq']}
tasks:
  echo:
    invoke:
      tool: nika:jq
      args:
        input:
          ticket: '${{ inputs.ticket }}'
          count: '${{ inputs.count }}'
          tags: '${{ inputs.tags }}'
          record: '${{ inputs.record }}'
          region: '${{ inputs.region }}'
        expression: '.'
outputs:
  value: '${{ tasks.echo.output }}'
";

struct NoCustody;
impl JournalSeal for NoCustody {
    fn seal(
        &self,
        _: &mut nika_dap::journal::TraceFileSink,
        _: Option<&str>,
        _: Option<&nika_dap::seal::SealTeardown>,
    ) -> bool {
        false
    }
}
fn production(world: &TestWorld) -> Arc<dyn ExecutionBackend> {
    Arc::new(ResidentExecutionBackend::new(&world.workflows).with_journal_seal(Arc::new(NoCustody)))
}
fn input_limits() -> ServerLimits {
    ServerLimits::new(
        4096,
        Duration::from_secs(5),
        Duration::from_secs(20),
        Duration::from_secs(2),
        4,
        16,
        64,
        32,
    )
}
fn input_world() -> TestWorld {
    let world = TestWorld::new();
    std::fs::write(world.workflows.join("root.nika"), INPUT_WORKFLOW).expect("workflow");
    world
}
fn values(ticket: &str) -> Value {
    json!({"ticket":ticket,"count":42,"tags":["é","東京"],"record":{"name":"🦋"}})
}
fn body(inputs: Value) -> String {
    let mut request = json!({"workflow":"root.nika"});
    request["inputs"] = inputs;
    request.to_string()
}
async fn result(server: &TestServer, id: &str) -> Value {
    let mut last = Value::Null;
    for _ in 0..800 {
        last = server
            .request(&get_request(&format!("/v1/jobs/{id}")))
            .await
            .json();
        if !matches!(last["status"].as_str(), Some("queued" | "running")) {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        !matches!(last["status"].as_str(), Some("queued" | "running")),
        "job did not settle: {last}"
    );
    last
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inputs_reach_real_runtime_with_defaults_literal_strings_and_honest_origins() {
    let world = input_world();
    let server = world.start(production(&world), input_limits()).await;
    let mut jobs = Vec::new();
    for (i, ticket) in [
        "@env:SERVER_SECRET",
        "${{ secrets.server }}",
        "https://example.invalid/東京?q=é#🦋",
    ]
    .iter()
    .enumerate()
    {
        let response = server
            .request(&post_request(
                &body(values(ticket)),
                &format!("literal-{i}"),
                &auth_header(),
            ))
            .await;
        assert_eq!(response.status, 202, "{}", response.body);
        let id = response.json()["id"].as_str().expect("id").to_owned();
        let job = result(&server, &id).await;
        assert_eq!(job["status"], "succeeded", "{job}");
        let mut expected = values(ticket);
        expected["region"] = json!("eu");
        assert_eq!(job["outputs"]["value"], expected, "{job}");
        assert_eq!(job["receipt"]["job_id"], id);
        assert_eq!(job["receipt"]["execution_id"], job["execution_id"]);
        jobs.push(job);
    }
    assert_ne!(jobs[0]["outputs"], jobs[1]["outputs"]);
    assert_ne!(jobs[0]["execution_id"], jobs[1]["execution_id"]);
    assert_eq!(
        jobs[0]["receipt"]["snapshot_digest"], jobs[1]["receipt"]["snapshot_digest"],
        "values never rewrite world bytes"
    );
    server.stop().await.expect("stop");
    let store = JobStore::open(&world.state).expect("validated store");
    let mut digests = Vec::new();
    for job in &jobs {
        let id = crate::JobId::parse(job["id"].as_str().expect("id")).expect("job id");
        let record = store.get(&id).expect("get").expect("record");
        let ticket = job["outputs"]["value"]["ticket"].as_str().expect("ticket");
        assert_eq!(
            serde_json::to_value(&record.inputs).expect("inputs"),
            values(ticket)
        );
        let digest = format!("{:x}", Sha256::digest(body(values(ticket)).as_bytes()));
        assert_eq!(record.request_digest().as_str(), digest);
        digests.push(digest);
    }
    assert_ne!(digests[0], digests[1]);
    drop(store);
    let dir = world.workflows.join(nika_dap::store::TRACE_DIR);
    let mut manifests = 0;
    for entry in std::fs::read_dir(dir).expect("journals") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|x| x.to_str()) != Some("ndjson") {
            continue;
        }
        let raw = std::fs::read_to_string(path).expect("journal");
        for line in raw.lines() {
            let row: Value = serde_json::from_str(line).expect("event");
            if row["kind"] != "workflow_started" {
                continue;
            }
            let fields = row["fields"].as_array().expect("fields");
            let origins = fields
                .iter()
                .find(|f| f["key"] == "inputs")
                .expect("origin field");
            let origins: Value =
                serde_json::from_str(origins["value"].as_str().expect("encoded origins"))
                    .expect("origins");
            assert_eq!(
                origins,
                json!({"ticket":"api-caller","count":"api-caller","tags":"api-caller","record":"api-caller","region":"file"})
            );
            manifests += 1;
        }
    }
    assert_eq!(manifests, 3);
    export_evidence(&world, &jobs);
}

fn export_evidence(world: &TestWorld, jobs: &[Value]) {
    // Optional operator-requested capture of the real engine journal for the
    // external spec witness. No synthetic frames or private paths in source.
    #[allow(clippy::disallowed_methods)]
    if let Some(output) = std::env::var_os("NIKA_SERVE_INPUT_EVIDENCE") {
        let output = PathBuf::from(output);
        std::fs::create_dir_all(&output).expect("evidence directory");
        std::fs::write(output.join("workflow.nika"), INPUT_WORKFLOW).expect("workflow evidence");
        std::fs::write(
            output.join("results.json"),
            serde_json::to_vec_pretty(&jobs).expect("results"),
        )
        .expect("result evidence");
        for entry in
            std::fs::read_dir(world.workflows.join(nika_dap::store::TRACE_DIR)).expect("journals")
        {
            let path = entry.expect("entry").path();
            if path.extension().and_then(|x| x.to_str()) == Some("ndjson") {
                std::fs::copy(&path, output.join(path.file_name().expect("name")))
                    .expect("copy actual journal");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inputs_refuse_missing_unknown_wrong_type_null_and_unknown_envelopes_before_job_creation() {
    let world = input_world();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), input_limits()).await;
    assert_snapshot_required_parity(&server).await;
    let mut unknown = values("x");
    unknown["ninja"] = json!(true);
    let mut wrong = values("x");
    wrong["count"] = json!("42");
    let mut wrong_array = values("x");
    wrong_array["tags"] = json!([1]);
    let mut wrong_object = values("x");
    wrong_object["record"] = json!({"name":1});
    let mut null_value = values("x");
    null_value["ticket"] = Value::Null;
    let cases = [
        (r#"{"workflow":"root.nika"}"#.to_owned(), "NIKA-1708"),
        (body(json!({})), "NIKA-1708"),
        (body(unknown), "unknown_input"),
        (body(wrong), "input_type_mismatch"),
        (body(wrong_array), "input_type_mismatch"),
        (body(wrong_object), "input_type_mismatch"),
        (body(null_value), "input_type_mismatch"),
        (body(Value::Null), "malformed_snapshot"),
        (body(json!([])), "malformed_snapshot"),
        (
            json!({"workflow":"root.nika","inputs":values("x"),"access":null}).to_string(),
            "malformed_snapshot",
        ),
    ];
    for (i, (body, code)) in cases.iter().enumerate() {
        let response = server
            .request(&post_request(body, &format!("bad-{i}"), &auth_header()))
            .await;
        assert_eq!(response.status, 422, "{}", response.body);
        assert!(response.body.contains(code), "{}", response.body);
        assert!(response.json().get("id").is_none());
    }
    let checked = server
        .request(&check_request(r#"{"workflow":"root.nika"}"#))
        .await;
    assert_eq!(
        checked.status, 200,
        "source-only Check still accepts required input declarations: {}",
        checked.body
    );
    let checked = server
        .request(&check_request(&body(values("ignored"))))
        .await;
    assert_eq!(
        checked.status, 422,
        "Check cannot silently drop launch inputs"
    );
    assert!(checked.body.contains("check_inputs_unsupported"));
    for field in [
        "model",
        "maxCostUsd",
        "permits",
        "source",
        "secrets",
        "unexpected",
    ] {
        let mut request = json!({"workflow":"root.nika","inputs":values("x")});
        request[field] = Value::Null;
        let response = server
            .request(&post_request(&request.to_string(), field, &auth_header()))
            .await;
        assert_eq!(response.status, 422, "{}", response.body);
    }
    let too_large = server
        .request(&post_request(
            &body(values(&"x".repeat(5000))),
            "large",
            &auth_header(),
        ))
        .await;
    assert_eq!(too_large.status, 413);
    assert_eq!(server.request(&get_request("/v1/jobs")).await.status, 404);
    server.stop().await.expect("stop");
    assert_eq!(backend.calls(), 0);
    let state: Value = serde_json::from_str(
        &std::fs::read_to_string(world.state.join("jobs/state.json")).expect("state"),
    )
    .expect("state JSON");
    assert_eq!(state["jobs"], json!([]));
}

async fn assert_snapshot_required_parity(server: &TestServer) {
    let required_snapshot = snapshot_body(INPUT_WORKFLOW);
    let refusal = server
        .request(&post_request(
            &required_snapshot,
            "snapshot-required",
            &auth_header(),
        ))
        .await;
    assert_eq!(refusal.status, 422, "{}", refusal.body);
    assert_eq!(refusal.json()["error"]["code"], "NIKA-1708");
    assert!(refusal.json().get("id").is_none());
    assert_eq!(
        server
            .request(&check_request(&required_snapshot))
            .await
            .status,
        200
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn frozen_snapshot_overlays_are_refused_including_null_and_empty_inputs() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), input_limits()).await;
    for (i, value) in [json!({}), json!({"ticket":"42"}), Value::Null]
        .into_iter()
        .enumerate()
    {
        let mut request: Value = serde_json::from_str(&snapshot_body(WORKFLOW)).expect("snapshot");
        request["inputs"] = value;
        for check in [false, true] {
            let body = request.to_string();
            let response = server
                .request(&if check {
                    check_request(&body)
                } else {
                    post_request(&body, &format!("snapshot-{i}"), &auth_header())
                })
                .await;
            assert_eq!(response.status, 422, "{}", response.body);
        }
    }
    server.stop().await.expect("stop");
    assert_eq!(backend.calls(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exact_input_request_replays_after_source_change_and_restart_and_other_values_conflict() {
    let world = input_world();
    let first = world.start(production(&world), input_limits()).await;
    let request = body(values("first"));
    let response = first
        .request(&post_request(&request, "retry", &auth_header()))
        .await;
    assert_eq!(response.status, 202, "{}", response.body);
    let id = response.json()["id"].as_str().expect("id").to_owned();
    let original = result(&first, &id).await;
    assert_eq!(original["status"], "succeeded", "{original}");
    std::fs::write(world.workflows.join("root.nika"), "broken").expect("changed workflow");
    let replay = first
        .request(&post_request(&request, "retry", &auth_header()))
        .await;
    assert_eq!(replay.status, 200);
    assert_eq!(replay.json()["id"], id);
    let conflict = first
        .request(&post_request(
            &body(values("other")),
            "retry",
            &auth_header(),
        ))
        .await;
    assert_eq!(conflict.status, 409);
    first.stop().await.expect("stop first");
    let second = world.start(production(&world), input_limits()).await;
    let replay = second
        .request(&post_request(&request, "retry", &auth_header()))
        .await;
    assert_eq!(replay.status, 200);
    assert_eq!(replay.json(), original);
    second.stop().await.expect("stop second");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queued_inputs_survive_restart_without_live_source_and_tampering_is_detected() {
    let world = input_world();
    let owned = nika_fs::OwnedDir::open(&world.workflows).expect("owned root");
    let admitted = ExecutionService::default()
        .admit(&owned, std::path::Path::new("root.nika"))
        .expect("admit");
    let inputs: BTreeMap<String, Value> = serde_json::from_value(values("queued")).expect("values");
    super::super::inputs::validate(&admitted, &inputs).expect("input validation");
    let store = JobStore::open(&world.state).expect("store");
    let request = body(values("queued"));
    let admission = store
        .create_or_replay_captured_inputs(
            crate::IdempotencyKey::new("queued-inputs".to_owned()).expect("key"),
            crate::RequestDigest::from_bytes(Sha256::digest(request.as_bytes()).into()),
            64,
            "root.nika".to_owned(),
            &admitted.snapshot().encode().expect("world"),
            None,
            inputs,
        )
        .expect("persist queued");
    let id = admission.record().id().as_str().to_owned();
    drop(store);
    std::fs::write(world.workflows.join("root.nika"), "broken").expect("mutate live source");
    let state_path = world.state.join("jobs/state.json");
    let original = std::fs::read(&state_path).expect("state");
    let mut stripped: Value = serde_json::from_slice(&original).expect("state JSON");
    stripped["jobs"][0]["events"] = json!([]);
    stripped["jobs"][0]["event_count"] = json!(0);
    stripped["jobs"][0]["event_head"] = Value::Null;
    std::fs::write(&state_path, serde_json::to_vec(&stripped).expect("encode"))
        .expect("strip fixture admission history");
    assert!(
        JobStore::open(&world.state).is_err(),
        "bindings need an admission event"
    );
    for remove in [false, true] {
        let mut state: Value = serde_json::from_slice(&original).expect("state JSON");
        if remove {
            state["jobs"][0]["record"]
                .as_object_mut()
                .expect("record")
                .remove("inputs");
        } else {
            state["jobs"][0]["record"]["inputs"]["ticket"] = json!("tampered");
        }
        std::fs::write(&state_path, serde_json::to_vec(&state).expect("encode"))
            .expect("tamper own fixture");
        assert!(
            JobStore::open(&world.state).is_err(),
            "queued inputs must be hash-bound"
        );
    }
    std::fs::write(&state_path, original).expect("restore own fixture");
    let server = world.start(production(&world), input_limits()).await;
    let job = result(&server, &id).await;
    assert_eq!(job["status"], "succeeded", "{job}");
    assert_eq!(job["outputs"]["value"]["ticket"], "queued");
    assert_eq!(
        server
            .request(&post_request(&request, "queued-inputs", &auth_header()))
            .await
            .status,
        200
    );
    server.stop().await.expect("stop");
}
