// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The served registry (`GET /v1/workflows` · its metadata) under the
//! `--workflows` scope (#1369).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use super::ExecutionDisposition;
use super::tests::{TestBackend, TestWorld, WORKFLOW, get_request, limits};

/// `--workflows` scopes the served registry (#1369): with the resident at
/// the project root and the listener on `workflows/`, the listing names only
/// what lives under `workflows/` (from the project root), and a workflow
/// outside it has no metadata route.
#[tokio::test(flavor = "multi_thread")]
async fn served_registry_is_scoped_to_the_workflows_directory() {
    let world = TestWorld::new();
    std::fs::write(
        world.root.path().join("top.nika"),
        WORKFLOW.replace("nika: root", "nika: top"),
    )
    .expect("a workflow outside the served registry");
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let server = world
        .start_with_workflow_roots(backend, limits(), world.root.path(), &world.workflows)
        .await;

    let listed = server.request(&get_request("/v1/workflows")).await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    let names = listed.json()["workflows"]
        .as_array()
        .expect("workflow list")
        .iter()
        .map(|value| value.as_str().expect("name").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["workflows/root.nika".to_owned()], "{names:?}");

    let inside = server
        .request(&get_request("/v1/workflows/workflows/root.nika"))
        .await;
    assert_eq!(inside.status, 200, "{}", inside.body);
    let outside = server.request(&get_request("/v1/workflows/top.nika")).await;
    assert_eq!(outside.status, 404, "{}", outside.body);
    server.stop().await.expect("stop");
}

#[test]
fn registry_names_stay_owned_relative_slash_canonical() {
    use super::registry::valid_workflow_name;
    assert!(valid_workflow_name("workflows/root.nika"));
    assert!(!valid_workflow_name(r"workflows\root.nika"));
    assert!(!valid_workflow_name("root.nika.yaml"));
    assert!(!valid_workflow_name("/abs/root.nika"));
    assert!(!valid_workflow_name("root.nika/"));
}
