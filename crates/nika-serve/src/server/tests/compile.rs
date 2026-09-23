// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `POST /v1/compile` over real loopback: one core, authoring data, zero effects (#1670).
//!
//! The native side of every parity assertion is `nika_onboard::compile` called in
//! process through its public builders only; the HTTP side is the real listener. The
//! shared fixture also drives the core's own test and the CLI door.
use std::collections::BTreeMap;
use std::path::Path;

use nika_onboard::compile::{CompileRequest, compile as native_compile, outcome_document};

use super::*;

mod native;
mod refusals;

const FIXTURE: &str =
    include_str!("../../../../nika-compile/tests/fixtures/compile_parity_v1.json");

/// The listener ceiling sits above the compile ceiling, so the compile bounds bind.
fn compile_limits() -> ServerLimits {
    ServerLimits::new(
        2 * 1024 * 1024,
        Duration::from_secs(20),
        Duration::from_secs(5),
        Duration::from_millis(200),
        4,
        16,
        64,
        32,
    )
}

fn compile_request(body: &str) -> String {
    format!(
        "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n{body}",
        body.len(),
        auth_header()
    )
}

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("parity fixture is JSON")
}

fn fixture_case(fixture: &Value, name: &str) -> Value {
    fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .find(|case| case["name"] == name)
        .expect("named fixture case")
        .clone()
}

/// The verbatim HTTP body: only the source placeholder is substituted, so every
/// literal travels exactly as the fixture spells it.
fn http_body(fixture: &Value, case: &Value) -> String {
    let mut body = case["http_body"].as_str().expect("http_body").to_owned();
    for (name, source) in fixture["sources"].as_object().expect("sources") {
        let encoded = serde_json::to_string(source).expect("encoded source");
        body = body.replace(&format!("@@SOURCE:{name}@@"), &encoded);
    }
    body
}

/// The native recipe through the public builders, sharing no code with the route.
fn native_document(fixture: &Value, case: &Value) -> Value {
    let native = &case["native"];
    let text = |key: &str| native[key].as_str().expect("native text field");
    let source = || {
        fixture["sources"][text("source_ref")]
            .as_str()
            .expect("named source")
    };
    let mut request = match text("mode") {
        "create" => CompileRequest::create(text("intent")),
        "edit" => CompileRequest::edit(source(), text("change_text")),
        mode => {
            assert_eq!(mode, "set_constant", "unknown native mode");
            CompileRequest::set_constant(source(), text("name"), text("literal"))
        }
    };
    if let Some(id) = native["workflow_id"].as_str() {
        request = request.with_workflow_id(id);
    }
    for pair in native["answers"].as_array().into_iter().flatten() {
        request = request.answer(
            pair[0].as_str().expect("answer key"),
            pair[1].as_str().expect("answer literal text"),
        );
    }
    // Printed and parsed like the HTTP body, so a float in the Check report cannot
    // differ by a parse round trip alone.
    let printed = outcome_document(&native_compile(&request).expect("compile machinery"));
    serde_json::from_str(&printed.to_string()).expect("native document")
}

/// Every regular file under `root`, by relative path.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                let name = path.strip_prefix(root).expect("under root");
                let bytes = std::fs::read(&path).expect("file bytes");
                files.insert(name.to_string_lossy().into_owned(), bytes);
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}

#[tokio::test(flavor = "multi_thread")]
async fn compile_is_authenticated_before_any_byte_is_judged() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), compile_limits()).await;
    let sentinel = "{ private-intent-sentinel definitely not json";
    for authorization in [
        String::new(),
        "Authorization: Bearer foreign-token-012345678901234567890123456789\r\n".to_owned(),
    ] {
        let request = format!(
            "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{authorization}\r\n{sentinel}",
            sentinel.len()
        );
        let response = server.request(&request).await;
        assert_eq!(response.status, 401, "{}", response.body);
        assert!(response.challenge());
        assert_eq!(response.json()["error"]["code"], "unauthorized");
        assert!(!response.body.contains("private-intent-sentinel"));
        assert!(!response.body.contains(TOKEN));

        // Withhold every body byte: authentication must answer without waiting
        // for collection or the authenticated handler's request deadline.
        let mut caller = tokio::net::TcpStream::connect(server.address)
            .await
            .expect("connect");
        let headers = format!(
            "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 100\r\n{authorization}\r\n"
        );
        caller.write_all(headers.as_bytes()).await.expect("headers");
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), caller.read_to_end(&mut bytes))
            .await
            .expect("auth does not await the body")
            .expect("response");
        assert_eq!(WireResponse::parse(&bytes).status, 401);
    }
    // The same bytes, authenticated, are a protocol refusal: auth was judged first.
    let authenticated = server.request(&compile_request(sentinel)).await;
    assert_eq!(authenticated.status, 422, "{}", authenticated.body);
    assert_eq!(
        authenticated.json()["error"]["code"],
        "malformed_compile_request"
    );
    assert!(!authenticated.body.contains("private-intent-sentinel"));
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_parity_case_answers_the_native_core_document_as_data() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), compile_limits()).await;
    let fixture = fixture();
    let cases = fixture["cases"].as_array().expect("cases");
    assert!(cases.len() >= 30, "the parity set shrank: {}", cases.len());
    for case in cases {
        let name = case["name"].as_str().expect("case name");
        let response = server
            .request(&compile_request(&http_body(&fixture, case)))
            .await;
        // ready, incomplete and refused are authoring DATA: never a 4xx or 5xx.
        assert_eq!(response.status, 200, "{name}: {}", response.body);
        let document = response.json();
        assert_eq!(
            document,
            native_document(&fixture, case),
            "{name}: the HTTP door must print the native core's document"
        );
        assert!(
            document.get("written").is_none(),
            "{name}: this door cannot materialize, so it states no `written`"
        );
        if let Some(status) = case["expect"]["status"].as_str() {
            assert_eq!(document["status"], status, "{name}");
        }
    }
    assert_eq!(backend.calls(), 0, "authoring never executes");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn compiling_creates_no_job_run_trace_or_file_and_never_reaches_the_backend() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend.clone(), compile_limits()).await;
    let fixture = fixture();
    let before = tree(world.root.path());
    let listed_before = server.request(&get_request("/v1/workflows")).await.json();

    for name in [
        "ready_hello",
        "create_names_the_workflow",
        "questions_answered",
        "edit_changed_constant",
        "refused_expression_literal",
        "incomplete_unknown_intent",
        "mcp_source_stays_unresolved_without_io",
    ] {
        let case = fixture_case(&fixture, name);
        let response = server
            .request(&compile_request(&http_body(&fixture, &case)))
            .await;
        assert_eq!(response.status, 200, "{name}: {}", response.body);
        let document = response.json();
        for key in ["id", "execution_id", "trace_id", "receipt", "settlement"] {
            assert!(document.get(key).is_none(), "{name}: {key} is a job fact");
        }
    }

    let after = tree(world.root.path());
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "no candidate, destination, trace or job file may appear anywhere"
    );
    assert_eq!(
        before.get("state/jobs/state.json"),
        after.get("state/jobs/state.json"),
        "the durable job state is byte-identical"
    );
    assert!(before.contains_key("state/jobs/state.json"));
    assert_eq!(
        server.request(&get_request("/v1/workflows")).await.json(),
        listed_before,
        "a named candidate is never published into the served registry"
    );
    assert_eq!(backend.calls(), 0, "authoring never executes");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ready_candidate_is_review_material_that_the_job_door_judges_again() {
    const GATED: &str = "nika: gated-inputs\ninputs:\n  ticket: {type: string, required: true}\nconst:\n  region: eu\npermits: {tools: ['nika:jq']}\ntasks:\n  echo:\n    invoke:\n      tool: nika:jq\n      args:\n        input: {ticket: '${{ inputs.ticket }}', region: '${{ const.region }}'}\n        expression: '.'\n";
    let world = TestWorld::new();
    let fixture = fixture();
    let case = fixture_case(&fixture, "edit_changed_constant");
    let expected = native_document(&fixture, &case)["candidate"]
        .as_str()
        .expect("candidate")
        .to_owned();
    let backend = Arc::new(CandidateBackend {
        expected,
        calls: AtomicUsize::new(0),
    });
    let server = world.start(backend.clone(), compile_limits()).await;

    // Ready under the source-only preview, yet the launch still lacks a required input.
    let body = json!({
        "compile_version": 1, "mode": "edit", "source": GATED,
        "change": {"set_constant": {"name": "region", "value": "us"}}
    })
    .to_string();
    let compiled = server.request(&compile_request(&body)).await;
    assert_eq!(compiled.status, 200, "{}", compiled.body);
    let compiled = compiled.json();
    assert_eq!(compiled["status"], "ready", "{compiled}");
    assert_eq!(compiled["check_preview"]["scope"], "sourceOnly");
    let candidate = compiled["candidate"].as_str().expect("candidate");
    assert_eq!(backend.calls(), 0, "a ready preview admitted nothing");

    let refused = server
        .request(&post_request(
            &snapshot_body(candidate),
            "compile-candidate-needs-inputs",
            &auth_header(),
        ))
        .await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(refused.json()["error"]["code"], "NIKA-1708");
    assert!(refused.json().get("id").is_none());
    assert_eq!(backend.calls(), 0, "ready is not admission");

    // A candidate the job door does admit runs only through that explicit door.
    let policy = server
        .request(&compile_request(&http_body(&fixture, &case)))
        .await
        .json();
    let candidate = policy["candidate"].as_str().expect("candidate");
    let created = server
        .request(&post_request(
            &snapshot_body(candidate),
            "compile-candidate-admitted",
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("job id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("the admitted candidate runs");
    assert_eq!(backend.calls(), 1, "only POST /v1/jobs executes");
    server.stop().await.expect("clean stop");
}

/// Re-admission must hand exactly the reviewed candidate to execution.
struct CandidateBackend {
    expected: String,
    calls: AtomicUsize,
}

impl CandidateBackend {
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ExecutionBackend for CandidateBackend {
    fn execute<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                context
                    .snapshot()
                    .text(context.snapshot().root())
                    .expect("root"),
                self.expected
            );
            ExecutionDisposition::Succeeded.into()
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_capability_and_the_published_contract_name_the_door_and_its_bounds() {
    use super::super::compile as door;

    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, compile_limits()).await;
    let health = server
        .request("GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .json();
    assert!(
        health["supportedCapabilities"]
            .as_array()
            .expect("capabilities")
            .iter()
            .any(|token| token == "compile"),
        "{health}"
    );

    let spec = server
        .request(&get_request("/v1/openapi.json"))
        .await
        .json();
    let post = &spec["paths"]["/v1/compile"]["post"];
    for status in ["200", "401", "408", "413", "415", "422", "500", "503"] {
        assert!(post["responses"].get(status).is_some(), "missing {status}");
    }
    let request = &spec["components"]["schemas"]["CompileRequest"];
    assert_eq!(request["additionalProperties"], false);
    assert_eq!(request["properties"]["compile_version"]["const"], 1);
    assert_eq!(
        request["properties"]["mode"]["enum"],
        json!(["create", "edit"])
    );
    assert_eq!(
        request["properties"]["cognition"]["const"],
        "deterministicOnly"
    );
    // The published bounds ARE the enforced constants.
    let properties = &request["properties"];
    assert_eq!(
        properties["intent"]["maxLength"],
        door::MAX_COMPILE_TEXT_BYTES
    );
    assert_eq!(
        properties["source"]["maxLength"],
        door::MAX_COMPILE_SOURCE_BYTES
    );
    assert_eq!(
        properties["workflow_id"]["maxLength"],
        door::MAX_COMPILE_NAME_BYTES
    );
    assert_eq!(
        properties["answers"]["maxProperties"],
        door::MAX_COMPILE_ANSWERS
    );
    assert_eq!(
        properties["answers"]["propertyNames"]["maxLength"],
        door::MAX_COMPILE_ANSWER_KEY_BYTES
    );
    assert!(
        properties.get("path").is_none() && properties.get("base").is_none(),
        "no field of this door may name a host path"
    );
    let outcome = &spec["components"]["schemas"]["CompileOutcome"];
    assert_eq!(
        outcome["properties"]["status"]["enum"],
        json!(["ready", "incomplete", "refused"])
    );
    assert!(
        outcome["properties"].get("written").is_none(),
        "`written` is a CLI-only fact"
    );
    server.stop().await.expect("clean stop");
}
