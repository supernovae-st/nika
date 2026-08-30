// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

#[test]
fn plain_persistent_serve_uses_coordinated_durable_execution_without_http() {
    let dir = project("resident-authority", HOURLY_A);
    write_workflow(dir.path(), "doctor.nika.yaml");
    seed_last(dir.path(), "doctor", "2026-08-19T03:00:00Z");
    let registry = arm::load(dir.path()).expect("registry").1;
    let state_root = dir.path().join("serve-state");
    let mut args = serve_args();
    args.now = Some("2026-08-19T04:02:00Z".to_owned());
    args.until = Some("2026-08-19T04:03:00Z".to_owned());
    args.state_root = Some(state_root.clone());

    serve_resident_production(
        dir.path().to_path_buf(),
        registry,
        args,
        state_root.clone(),
        None,
    )
    .expect("plain persistent lifecycle");

    let fired = history(dir.path(), "doctor")
        .lines()
        .last()
        .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .expect("terminal ARM receipt");
    let run_id = fired["payload"]["run_id"]
        .as_str()
        .and_then(|id| nika_serve::JobId::parse(id).ok())
        .expect("normal run id");
    let store = nika_serve::JobStore::open(&state_root).expect("reopen state");
    let record = store
        .get(&run_id)
        .expect("read run")
        .expect("persisted normal run");
    assert_eq!(record.status(), nika_serve::JobStatus::Succeeded);
    let receipt = record.receipt().expect("persisted receipt");
    assert_eq!(receipt.job_id(), &run_id);
    assert!(
        receipt
            .origin()
            .and_then(nika_serve::JobOrigin::schedule_identity)
            .is_some()
    );
    assert!(
        !dir.path().join("serve.token").exists(),
        "plain resident authority performs no credential I/O"
    );
}

#[test]
fn http_attach_failure_closes_authority_without_activating_arm() {
    let dir = project("attach-failure", HOURLY_A);
    write_workflow(dir.path(), "doctor.nika.yaml");
    let state_root = dir.path().join("serve-state");
    let registry = arm::load(dir.path()).expect("registry").1;
    let result = serve_resident_production(
        dir.path().to_path_buf(),
        registry,
        serve_args(),
        state_root.clone(),
        Some(HttpDoor {
            bind: "127.0.0.1:0".parse().expect("address"),
            workflows: dir.path().join("workflows"),
            token_file: dir.path().join("missing.token"),
            allow_remote: false,
        }),
    );
    assert!(
        result.is_err_and(|error| error.contains("token file unreadable")),
        "credential refusal must win"
    );
    assert!(
        !dir.path().join(".nika/arm/doctor/history.ndjson").exists(),
        "ARM remains dormant when listener attachment fails"
    );
    nika_serve::JobStore::open(&state_root).expect("authority lock released after attach failure");
}
