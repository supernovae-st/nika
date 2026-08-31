// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::json;

use super::tests::{
    TestWorld, auth_header, events_request, get_request, limits, parse_sse_data, post_request,
    wait_for_status,
};
use super::{ExecutionBackend, ExecutionDisposition, ExecutionOutcome};

#[derive(Debug)]
struct ResultBackend;

#[derive(Debug)]
struct AbsentResultBackend;

impl ExecutionBackend for AbsentResultBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async { ExecutionDisposition::Succeeded.into() })
    }
}

impl ExecutionBackend for ResultBackend {
    fn execute<'a>(
        &'a self,
        _context: nika_execution::ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async {
            ExecutionOutcome::from(ExecutionDisposition::Succeeded)
                .with_outputs(BTreeMap::from([
                    ("answer".to_owned(), json!(42)),
                    ("empty".to_owned(), json!({})),
                ]))
                .with_chain_head("chain-head-42")
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn unavailable_adapter_outputs_remain_absent_without_losing_receipt_identity() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(AbsentResultBackend), limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "absent-result",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("succeeded");

    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await
        .json();
    assert!(
        job.get("outputs").is_none(),
        "an adapter that exposes no outputs must stay honestly absent: {job}"
    );
    assert_eq!(job["receipt"]["job_id"], id);
    assert_eq!(job["receipt"]["execution_id"], job["execution_id"]);
    assert_eq!(job["receipt"]["trace_id"], job["trace_id"]);
    assert_eq!(
        job["receipt"]["snapshot_digest"]
            .as_str()
            .expect("snapshot digest")
            .len(),
        64
    );
    assert!(job["receipt"].get("chain_head").is_none());
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn supplied_outputs_and_receipt_appear_only_on_terminal_get_and_sse() {
    let world = TestWorld::new();
    let server = world.start(Arc::new(ResultBackend), limits()).await;
    let created = server
        .request(&post_request(
            r#"{"workflow":"root.nika.yaml"}"#,
            "terminal-result",
            &auth_header(),
        ))
        .await;
    let id = created.json()["id"].as_str().expect("id").to_owned();
    assert!(created.json().get("outputs").is_none());
    assert!(created.json().get("receipt").is_none());
    wait_for_status(&server, &id, "succeeded")
        .await
        .expect("succeeded");

    let job = server
        .request(&get_request(&format!("/v1/jobs/{id}")))
        .await;
    let body = job.json();
    assert_eq!(body["outputs"]["answer"], 42);
    assert_eq!(body["outputs"]["empty"], json!({}));
    assert_eq!(body["receipt"]["job_id"], id);
    assert_eq!(body["receipt"]["execution_id"], body["execution_id"]);
    assert_eq!(body["receipt"]["trace_id"], body["trace_id"]);
    assert_eq!(body["receipt"]["chain_head"], "chain-head-42");

    let streamed = server.request(&events_request(&id, None)).await;
    assert_eq!(streamed.status, 200, "{}", streamed.body);
    let events = parse_sse_data(&streamed.body);
    let terminal = events
        .iter()
        .filter(|event| event.get("outputs").is_some() || event.get("receipt").is_some())
        .collect::<Vec<_>>();
    assert_eq!(terminal.len(), 1, "{events:?}");
    assert_eq!(terminal[0]["status"], "succeeded");
    assert_eq!(terminal[0]["outputs"], body["outputs"]);
    assert_eq!(terminal[0]["receipt"], body["receipt"]);
    server.stop().await.expect("clean stop");
}
