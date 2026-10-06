// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The compile door's published contract (S23): a default server serves the committed document;
//! a server that seats native authoring serves it with exactly the generation-2 contract merged
//! in. The schemas each server publishes validate the live, controlled payloads they describe,
//! refuse what the door refuses (injected authority, version confusion, forbidden pairings), and
//! state the ceilings and refusal codes the door enforces — no endpoint the server lacks.

use std::time::Duration;

use nika_providers::ProvidersConfig;

use super::refusals::operator;
use super::*;
use crate::server::openapi;

/// The request body schema of `POST /v1/compile`, as a JSON pointer into the document.
pub(super) const REQUEST: &str =
    "/paths/~1v1~1compile/post/requestBody/content/application~1json/schema";
/// The 200 answer schema of `POST /v1/compile`.
pub(super) const ANSWER: &str =
    "/paths/~1v1~1compile/post/responses/200/content/application~1json/schema";
/// The replay header's schema.
const TOKEN: &str = "/paths/~1v1~1compile/post/responses/200/headers/Nika-Compile-Replay/schema";

/// A validator for the schema at `pointer`: the whole document is its root, so every
/// `$ref` resolves inside the published contract and nowhere else.
pub(super) fn schema_at(document: &Value, pointer: &str) -> jsonschema::Validator {
    let mut root = document.clone();
    root["$ref"] = json!(format!("#{pointer}"));
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&root)
        .expect("the published schema compiles")
}

fn assert_valid(validator: &jsonschema::Validator, instance: &Value, what: &str) {
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|error| error.to_string())
        .collect();
    assert!(errors.is_empty(), "{what}: {errors:?}");
}

pub(super) async fn served(server: &TestServer) -> (String, Value) {
    let response = server.request(&get_request("/v1/openapi.json")).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    (response.body, document)
}

/// Validate a request against the published schema, send it, validate the live answer.
pub(super) async fn exchange(
    server: &TestServer,
    request: &jsonschema::Validator,
    answer: &jsonschema::Validator,
    body: &str,
) -> WireResponse {
    let instance: Value = serde_json::from_str(body).expect("a JSON body");
    assert_valid(request, &instance, body);
    let response = server.request(&compile_request(body)).await;
    assert_eq!(response.status, 200, "{body}: {}", response.body);
    assert_valid(answer, &response.json(), &response.body);
    response
}

/// Every parity-fixture request and its live answer validate against a published document. A
/// body whose literal lies outside `serde_json`'s number range cannot be held as a `Value`: the
/// door forwards such a literal untouched and the schema constrains no literal, so only its
/// answer is validated (the fixture's three literal-edge cases).
async fn parity_cases_validate(server: &TestServer, document: &Value) {
    let (request, answer) = (schema_at(document, REQUEST), schema_at(document, ANSWER));
    let fixture = fixture();
    let mut unrepresentable = Vec::new();
    for case in fixture["cases"].as_array().expect("cases") {
        let body = http_body(&fixture, case);
        if serde_json::from_str::<Value>(&body).is_ok() {
            exchange(server, &request, &answer, &body).await;
        } else {
            unrepresentable.push(case["name"].clone());
            let response = server.request(&compile_request(&body)).await;
            assert_eq!(response.status, 200, "{body}: {}", response.body);
            assert_valid(&answer, &response.json(), &response.body);
        }
    }
    assert!(unrepresentable.len() <= 3, "{unrepresentable:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_default_server_serves_the_committed_contract_and_refuses_generation_two() {
    let world = TestWorld::new();
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world.start(backend, compile_limits()).await;
    let (body, document) = served(&server).await;
    assert_eq!(document, openapi::document());
    assert_eq!(
        body,
        serde_json::to_string(&openapi::document()).expect("renders")
    );
    let committed: Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.json"))
            .expect("the committed document"),
    )
    .expect("JSON");
    let mut live = document.clone();
    live["info"]["version"] = committed["info"]["version"].clone();
    assert_eq!(live, committed, "the default contract is the committed one");
    assert!(
        document["components"]["schemas"]
            .get("CompileRequestV2")
            .is_none()
    );
    let post = &document["paths"]["/v1/compile"]["post"];
    assert!(post["responses"].get("409").is_none());
    assert!(post["responses"]["200"].get("headers").is_none());
    parity_cases_validate(&server, &document).await;
    // Generation 2 is not this server's contract: its published request refuses it.
    let request = schema_at(&document, REQUEST);
    let v2: Value = serde_json::from_str(&fresh(&json!({}))).expect("JSON");
    assert!(!request.is_valid(&v2));
    let refused = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(
        refused.json()["error"]["code"],
        "compile_version_unsupported"
    );
    server.stop().await.expect("clean stop");
}

/// A source revision is READY only after its distinct judge; both writes and the base model stay.
fn assert_judged_revision(revised: &Value, seat: &Seat) {
    assert_eq!(revised["status"], "ready", "{revised:#}");
    assert_eq!(revised["provenance"]["authoring"]["calls"], 2);
    assert!(
        revised["provenance"]["decision"]["semantic_verification"]
            .as_array()
            .is_some_and(|judgments| !judgments.is_empty()),
        "{revised:#}"
    );
    let source = revised["candidate"].as_str().expect("a judged revision");
    assert!(source.contains("./b.md") && source.contains("./c.md") && source.contains(RUN_MODEL));
    assert_eq!(
        seat.calls(),
        5,
        "the source revision and its judge both reached the seat"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_native_server_publishes_generation_two_and_its_live_payloads_validate() {
    let world = TestWorld::new();
    // A kept question round for creation, then typed revision links and their own judge.
    let mut script = question_round();
    script.extend([
        Reply::Text(copy_revision_answer()),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    let seat = Seat::start(script);
    let (server, _backend) = start_native(&world, compile_limits(), operator(&seat)).await;
    let (_, document) = served(&server).await;
    assert_eq!(document, openapi::live(true));
    parity_cases_validate(&server, &document).await;
    let (request, answer) = (schema_at(&document, REQUEST), schema_at(&document, ANSWER));
    // A fresh round: the plan, the sketch and its fill, generation 2, a kept round's token.
    let first = exchange(&server, &request, &answer, &fresh(&json!({}))).await;
    assert_eq!(first.json()["compile_version"], 2);
    assert_eq!(
        seat.calls(),
        3,
        "plan, sketch and fill; model question still open"
    );
    for (field, invalid) in [
        ("/provenance/authoring/backend/host", json!(false)),
        (
            "/provenance/authoring/backend/base_url_overridden",
            json!("yes"),
        ),
        (
            "/provenance/authoring/backend/endpoint_basis",
            json!("authenticated_peer"),
        ),
        (
            "/provenance/authoring/backend/usage_complete",
            json!("unknown"),
        ),
        (
            "/provenance/authoring/backend/authority/http_requests/sent",
            json!(-1),
        ),
        // A bound is positive, or null when none was configured.
        (
            "/provenance/authoring/backend/authority/max_calls",
            json!(0),
        ),
        (
            "/provenance/authoring/backend/authority/max_calls",
            json!("4"),
        ),
        (
            "/provenance/authoring/backend/observed_models",
            json!([false]),
        ),
    ] {
        let mut false_receipt = first.json();
        *false_receipt
            .pointer_mut(field)
            .expect("the live receipt has this fact") = invalid;
        assert!(
            !answer.is_valid(&false_receipt),
            "false receipt accepted: {field}"
        );
    }
    let token = token_of(&first);
    assert_valid(&schema_at(&document, TOKEN), &json!(token), "token");
    // Its replay: zero calls, generation 1.
    let answers = json!({"answers": {"model": RUN_MODEL}});
    let replayed = exchange(&server, &request, &answer, &replay(&token, &answers)).await;
    assert_eq!(replayed.json()["compile_version"], 1);
    assert_eq!(seat.calls(), 3, "the deterministic replay calls no seat");
    // Generation-1 replay keeps decision evidence and its public type. Without
    // this schema field generated SDK types erase an actually observed record.
    let replay_doc = replayed.json();
    assert!(replay_doc["provenance"]["decision"].is_object());
    let decision = &document["components"]["schemas"]["CompileOutcome"]["properties"]["provenance"]
        ["properties"]["decision"];
    assert_eq!(decision["type"], "object");
    let mut invalid_decision = replay_doc.clone();
    invalid_decision["provenance"]["decision"] = json!(false);
    assert!(!answer.is_valid(&invalid_decision));
    // A revision in words, and a generation-2 skeleton that needs no call.
    let revision = json!({
        "compile_version": 2, "mode": "edit", "cognition": "explicitProvider",
        "source": candidate(RUN_MODEL, false), "change": {"text": CHANGE},
        "original_intent": INTENT, "limits": {"repairs": 0, "max_tokens": 1024},
    });
    let revised = exchange(&server, &request, &answer, &revision.to_string()).await;
    let revised = revised.json();
    assert_judged_revision(&revised, &seat);
    let skeleton = exchange(
        &server,
        &request,
        &answer,
        &fresh(&json!({"intent": "hello"})),
    )
    .await;
    assert_eq!(skeleton.json()["compile_version"], 1);
    assert_eq!(
        seat.calls(),
        5,
        "the fresh round's three, the revision and its judgment, nothing else"
    );
    server.stop().await.expect("clean stop");
}

/// Requests the door refuses with 422 before any work, each with the code it answers: forms,
/// pairings, version confusion, and caller-named authority whatever the name.
fn refused_requests() -> Vec<(String, &'static str)> {
    let token = "a".repeat(64);
    let edit = |fields: &Value| {
        let mut body = json!({
            "compile_version": 2, "mode": "edit", "cognition": "explicitProvider",
            "source": candidate(RUN_MODEL, false), "change": {"text": "keep a copy"},
            "original_intent": INTENT,
        });
        merge(&mut body, fields);
        body.to_string()
    };
    let generation_one = |fields: &Value| {
        let mut body = json!({"compile_version": 1, "mode": "create", "intent": INTENT});
        merge(&mut body, fields);
        body.to_string()
    };
    let malformed = "malformed_compile_request";
    let explicit = json!({"cognition": "explicitProvider"});
    let mut refused = vec![
        (
            generation_one(&json!({"limits": {"repairs": 0}})),
            malformed,
        ),
        (generation_one(&json!({"replay_token": token})), malformed),
        (generation_one(&explicit), "compile_cognition_unsupported"),
        (fresh(&json!({"cognition": null})), malformed),
        (
            fresh(&json!({"compile_version": 3})),
            "compile_version_unsupported",
        ),
        (fresh(&json!({"cognition": "deterministicOnly"})), malformed),
        (fresh(&json!({"replay_token": token})), malformed),
        (
            replay(&token, &json!({"limits": {"repairs": 0}})),
            malformed,
        ),
        (replay(&token.to_uppercase(), &json!({})), malformed),
        (fresh(&json!({"source": "nika: x\ntasks: {}\n"})), malformed),
        (fresh(&json!({"original_intent": INTENT})), malformed),
        (edit(&json!({"original_intent": null})), malformed),
        (
            edit(&json!({"change": {"set_constant": {"name": "x", "value": 1}}})),
            malformed,
        ),
        (edit(&json!({"workflow_id": "renamed"})), malformed),
        (
            fresh(&json!({"answers": {"intent.clarification": "Write ./b.md"}})),
            "compile_new_intent_required",
        ),
        (
            fresh(&json!({"mode": "revise"})),
            "compile_mode_unsupported",
        ),
        (
            fresh(&json!({"cognition": "explicitDecision"})),
            "compile_cognition_unsupported",
        ),
        (
            fresh(&json!({"limits": {"max_tokens": 0}})),
            "compile_limit",
        ),
        (
            fresh(&json!({"limits": {"deadline_ms": 0}})),
            "compile_limit",
        ),
    ];
    // Caller-named authority, whatever the name: no model, endpoint, credential, path or plan.
    for field in [
        "model",
        "provider",
        "base_url",
        "endpoint",
        "api_key",
        "access",
        "harness",
        "snapshot",
        "knowledge",
        "knowledge_pack",
        "path",
        "strategy",
        "plan",
        "receipt",
        "seat",
    ] {
        refused.push((fresh(&json!({ field: "x" })), malformed));
    }
    refused
}

#[tokio::test(flavor = "multi_thread")]
async fn the_published_schema_refuses_what_the_door_refuses() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        RUN_MODEL, false,
    )))]);
    let (server, _backend) = start_native(&world, compile_limits(), operator(&seat)).await;
    let (_, document) = served(&server).await;
    let request = schema_at(&document, REQUEST);
    for (body, code) in refused_requests() {
        let instance: Value = serde_json::from_str(&body).expect("JSON");
        assert!(
            !request.is_valid(&instance),
            "the published schema must refuse {body}"
        );
        let response = server.request(&compile_request(&body)).await;
        assert_eq!(response.status, 422, "{body}: {}", response.body);
        assert_eq!(response.json()["error"]["code"], code, "{body}");
    }
    assert_eq!(
        seat.calls(),
        0,
        "nothing the contract refuses reaches the seat"
    );
    server.stop().await.expect("clean stop");
}

/// The published nesting ceiling of a literal is the one the door enforces: one array shallower
/// is read (a replay of a round this server never kept then answers 409), at the ceiling it is
/// refused as malformed before any lookup.
#[tokio::test(flavor = "multi_thread")]
async fn the_published_nesting_ceiling_is_the_enforced_one() {
    const CEILING: usize = 128;
    let world = TestWorld::new();
    let seat = Seat::start(Vec::new());
    let (server, _backend) = start_native(&world, compile_limits(), operator(&seat)).await;
    let (_, document) = served(&server).await;
    let described = document["components"]["schemas"]["CompileRequestV2"]["description"]
        .as_str()
        .expect("described");
    assert!(described.contains(&format!("nests {CEILING} or more arrays/objects deep")));
    let token = "a".repeat(64);
    let nested = |depth: usize| {
        let mut literal = json!(1);
        for _ in 0..depth {
            literal = json!([literal]);
        }
        replay(&token, &json!({"answers": {"const.deep": literal}}))
    };
    let read = server.request(&compile_request(&nested(CEILING - 1))).await;
    assert_eq!(read.status, 409, "{}", read.body);
    assert_eq!(read.json()["error"]["code"], "compile_replay_unavailable");
    let refused = server.request(&compile_request(&nested(CEILING))).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(refused.json()["error"]["code"], "malformed_compile_request");
    assert_eq!(seat.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[test]
fn large_explicit_bounds_are_not_refused_by_legacy_product_ceilings() {
    let document = openapi::live(true);
    let limits = &document["components"]["schemas"]["CompileRequestV2"]["properties"]["limits"]["properties"];
    let seated =
        |authoring: NativeAuthoring| crate::server::compile::Seat::open(&authoring).is_ok();
    let at = || NativeAuthoring::new(SEAT, ProvidersConfig::new());
    assert!(seated(at().with_max_tokens(393_216)));
    assert!(seated(at().with_call_timeout(Duration::from_secs(3601))));
    assert!(seated(at().with_deadline(Duration::from_secs(86_400))));
    assert!(seated(at().with_repairs(100)));
    assert!(seated(
        at().with_replay(2048, Duration::from_secs(7 * 86_400))
    ));
    assert!(!seated(at().with_max_tokens(0)));
    assert!(!seated(at().with_call_timeout(Duration::ZERO)));
    assert!(!seated(at().with_deadline(Duration::ZERO)));
    assert!(!seated(at().with_max_calls(0)));
    assert_eq!(limits["max_tokens"]["maximum"], u32::MAX);
    assert_eq!(limits["repairs"]["maximum"], u32::MAX);
    assert!(limits["call_timeout_ms"].get("maximum").is_none());
    assert!(limits["deadline_ms"].get("maximum").is_none());
}

#[test]
fn every_refusal_is_published_under_its_status_and_the_contract_only_adds() {
    let native = openapi::live(true);
    let default = openapi::document();
    let responses = &native["paths"]["/v1/compile"]["post"]["responses"];
    let published: [(&str, &[&str]); 5] = [
        ("408", &["request_timeout", "compile_deadline_exceeded"]),
        (
            "409",
            &[
                "compile_replay_unavailable",
                "compile_replay_input_changed",
                "compile_context_changed",
            ],
        ),
        (
            "422",
            &[
                "malformed_compile_request",
                "compile_version_unsupported",
                "compile_mode_unsupported",
                "compile_cognition_unsupported",
                "compile_limit",
                "compile_new_intent_required",
            ],
        ),
        ("500", &["internal_error", "compile_disclosure_refused"]),
        (
            "503",
            &["compile_busy", "compile_replay_capacity", "stopping"],
        ),
    ];
    for (status, codes) in published {
        let described = responses[status]["description"].as_str().expect(status);
        for code in codes {
            assert!(
                described.contains(code),
                "{status} names {code}: {described}"
            );
        }
    }
    // No endpoint the server lacks, and nothing else of the default contract moves.
    let names = |document: &Value, key: &str| -> Vec<String> {
        document[key]
            .as_object()
            .expect(key)
            .keys()
            .cloned()
            .collect()
    };
    assert_eq!(names(&native, "paths"), names(&default, "paths"));
    for (path, item) in default["paths"].as_object().expect("paths") {
        if path != "/v1/compile" {
            assert_eq!(&native["paths"][path], item, "{path}");
        }
    }
    let schemas = default["components"]["schemas"]
        .as_object()
        .expect("schemas");
    for (name, schema) in schemas {
        if name != "CompileRequest" {
            assert_eq!(&native["components"]["schemas"][name], schema, "{name}");
        }
    }
    // The generation-1 request only gains a word pointing at generation 2.
    let generation_one = &default["components"]["schemas"]["CompileRequest"];
    let mut request = native["components"]["schemas"]["CompileRequest"].clone();
    request["properties"]["cognition"]["description"] =
        generation_one["properties"]["cognition"]["description"].clone();
    assert_eq!(&request, generation_one);
    assert_eq!(
        native["components"]["securitySchemes"],
        default["components"]["securitySchemes"]
    );
    assert_eq!(
        native["components"]["parameters"],
        default["components"]["parameters"]
    );
}
