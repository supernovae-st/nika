// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The served `OpenAPI` document declares what the server answers: each live answer below is
//! checked against the status and schema the document a client reads gives for it (its
//! `$ref`s resolved inside that document). The 25 October 2026 qualification found these
//! answers sent but undeclared.

use super::trace_journal::TestSeal;
use super::*;
use crate::server::production::ResidentExecutionBackend;

/// The document `server` serves.
pub(super) async fn served_document(server: &TestServer) -> Value {
    let answer = server.request(&get_request("/v1/openapi.json")).await;
    assert_eq!(answer.status, 200, "{}", answer.body);
    answer.json()
}

/// The document declares `status` on `method template`, and a JSON answer validates against
/// the schema it declares there.
pub(super) fn assert_declared_on(
    document: &Value,
    method: &str,
    template: &str,
    status: u16,
    body: &[u8],
) {
    let declared = &document["paths"][template][method]["responses"][status.to_string()];
    assert!(
        declared.is_object(),
        "{method} {template} answers {status} undeclared"
    );
    let Some(schema) = declared["content"]["application/json"]["schema"].as_object() else {
        return;
    };
    let mut root = Value::Object(schema.clone());
    root["components"] = document["components"].clone();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&root)
        .expect("the declared schema compiles");
    let instance: Value = serde_json::from_slice(body).expect("a JSON answer");
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|error| error.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "{method} {template} {status}: {errors:?}\n{instance}"
    );
}

/// The production backend's run leaves a sealed journal, so the verdict carries the CLI's
/// `tier`; a by-name check or job naming no served workflow answers 404; a schedule PUT
/// answers 412 with its current revision and 422 in both its shapes.
#[tokio::test(flavor = "multi_thread")]
async fn the_document_declares_what_the_qualification_found_undeclared() {
    let world = TestWorld::new();
    let key = minisign::KeyPair::generate_unencrypted_keypair().expect("test key");
    let public = key.pk.to_box().expect("public box").to_string();
    let backend = Arc::new(
        ResidentExecutionBackend::new(&world.workflows).with_journal_seal(Arc::new(TestSeal {
            secret: key.sk,
            public,
        })),
    );
    let generous = ServerLimits::new(
        1024,
        Duration::from_secs(2),
        Duration::from_secs(30),
        Duration::from_millis(200),
        4,
        16,
        64,
        32,
    );
    let server = world.start(backend, generous).await;
    let document = served_document(&server).await;

    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika"}"#,
            "declared-run",
            &auth_header(),
        ))
        .await;
    assert_eq!(created.status, 202, "{}", created.body);
    let id = created.json()["id"].as_str().expect("id").to_owned();
    let mut status = String::new();
    for _ in 0..600 {
        let observed = server
            .request(&get_request(&format!("/v1/jobs/{id}/status")))
            .await;
        status = observed.json()["status"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if status != "queued" && status != "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(status, "succeeded");
    let verdict = server
        .request(&get_request(&format!("/v1/jobs/{id}/trace/verify")))
        .await;
    assert_eq!(verdict.status, 200, "{}", verdict.body);
    assert!(verdict.json()["tier"].is_string(), "{}", verdict.body);
    let template = "/v1/jobs/{id}/trace/verify";
    assert_declared_on(&document, "get", template, 200, verdict.body.as_bytes());

    let by_name = r#"{"workflow":"workflows/missing.nika"}"#;
    let checked = server.request(&check_request(by_name)).await;
    assert_eq!(checked.status, 404, "{}", checked.body);
    assert_declared_on(&document, "post", "/v1/check", 404, checked.body.as_bytes());
    let admitted = server
        .request(&post_request(by_name, "missing-by-name", &auth_header()))
        .await;
    assert_eq!(admitted.status, 404, "{}", admitted.body);
    assert_declared_on(&document, "post", "/v1/jobs", 404, admitted.body.as_bytes());

    let put = super::super::schedule_tests::put_request;
    let zoneless = json!({
        "workflow": "root.nika", "when": {"kind": "cadence", "expression": "0 9 * * *"},
        "maxCostUsd": 0.01, "missed": "skip"
    })
    .to_string();
    let template = "/v1/schedules/{id}";
    let unconditional = server.request(&put("nightly", &zoneless, "", true)).await;
    assert_eq!(unconditional.status, 412, "{}", unconditional.body);
    assert_eq!(
        unconditional.json()["error"]["currentRevision"],
        Value::Null
    );
    let body = unconditional.body.as_bytes();
    assert_declared_on(&document, "put", template, 412, body);
    let create = "If-None-Match: *\r\n";
    let findings = server
        .request(&put("nightly", &zoneless, create, true))
        .await;
    assert_eq!(findings.status, 422, "{}", findings.body);
    assert!(findings.json()["findings"].is_array(), "{}", findings.body);
    assert_declared_on(&document, "put", template, 422, findings.body.as_bytes());
    let malformed = server
        .request(&put("nightly", "{\"when\":", create, true))
        .await;
    assert_eq!(malformed.status, 422, "{}", malformed.body);
    assert_eq!(malformed.json()["error"]["code"], "schedule.body");
    assert_declared_on(&document, "put", template, 422, malformed.body.as_bytes());
    server.stop().await.expect("clean stop");
}
