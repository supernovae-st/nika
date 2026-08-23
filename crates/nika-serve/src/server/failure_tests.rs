// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::tests::{
    TestWorld, auth_header, events_request, get_request, limits, parse_sse_data, post_request,
    wait_for_status,
};
use super::{ExecutionBackend, ExecutionContext, ExecutionDisposition, ExecutionOutcome};

struct FailingBackend;
struct SucceedingBackend;

impl ExecutionBackend for FailingBackend {
    fn execute<'a>(
        &'a self,
        _context: ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(
            async move { ExecutionOutcome::failed("NIKA-ASSERT-001", "task boom: expected true") },
        )
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_job_get_and_sse_name_the_redacted_nika_code() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(FailingBackend), limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "fail-code",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "failed")
        .await
        .expect("failed");

    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await;
    let body = job.json();
    assert_eq!(body["status"], "failed", "{}", job.body);
    assert_eq!(body["error"]["code"], "NIKA-ASSERT-001", "{}", job.body);
    assert_eq!(
        body["error"]["message"], "task boom: expected true",
        "{}",
        job.body
    );
    assert!(!job.body.contains("/tmp"), "{}", job.body);

    let streamed = server.request(&events_request(&id, None)).await;
    let events = parse_sse_data(&streamed.body);
    let settled = events
        .iter()
        .find(|event| event["kind"] == "execution.settled")
        .expect("settled frame");
    assert_eq!(settled["code"], "NIKA-ASSERT-001", "{settled}");
    assert_eq!(settled["message"], "task boom: expected true", "{settled}");
    assert!(settled.get("secret").is_none());
    server.stop().await.expect("clean stop");
}

impl ExecutionBackend for SucceedingBackend {
    fn execute<'a>(
        &'a self,
        _context: ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move { ExecutionDisposition::Succeeded.into() })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn succeeded_job_omits_error_object() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(SucceedingBackend), limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "ok-no-error",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("succeeded");
    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await;
    assert!(job.json().get("error").is_none(), "{}", job.body);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn parse_fatal_post_names_the_nika_parse_code() {
    let world = TestWorld::new();
    std::fs::write(
        world.workflows.join("bad.nika.yaml"),
        "nika: v1\nworkflow: nope\n",
    )
    .expect("parse-fatal fixture");
    let server = world.start(Arc::new(SucceedingBackend), limits()).await;
    let response = server
        .request(&post_request(
            r#"{"workflow":"bad.nika.yaml"}"#,
            "parse-fatal",
            &auth_header(),
        ))
        .await;
    assert_eq!(response.status, 422, "{}", response.body);
    let body = response.json();
    let code = body["error"]["code"].as_str().expect("code");
    assert!(code.starts_with("NIKA-PARSE-"), "{code} {}", response.body);
    assert!(!response.body.contains("/tmp"), "{}", response.body);
    assert!(response.json().get("id").is_none(), "{}", response.body);
    server.stop().await.expect("clean stop");
}
