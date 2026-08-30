// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

#[test]
fn http_attach_failure_closes_authority_without_activating_arm() {
    let dir = project("attach-failure", HOURLY_A);
    write_workflow(dir.path(), "doctor.nika.yaml");
    let state_root = dir.path().join("serve-state");
    let result = serve_resident_production(
        dir.path(),
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
