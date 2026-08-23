// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! W04.C structural extinction ratchets.

const RUN: &str = include_str!("mod.rs");
const EXECUTION_ADAPTER: &str = include_str!("execution_adapter.rs");
const DRY_RUN: &str = include_str!("dry_run.rs");
const PROVENANCE: &str = include_str!("provenance.rs");
const CHILD_RUNNER: &str = include_str!("child_runner.rs");
const RUNTIME_DRIVER: &str = include_str!("../../../../nika-service-execution/src/lib.rs");
const CLI_CARGO: &str = include_str!("../../../Cargo.toml");
const EXECUTION_CARGO: &str = include_str!("../../../../nika-execution/Cargo.toml");
const EXECUTION_SERVICE: &str = include_str!("../../../../nika-execution/src/service.rs");
const ARM_CARGO: &str = include_str!("../../../../nika-arm/Cargo.toml");
const ARM_FIRE: &str = include_str!("../../../../nika-arm/src/fire.rs");

#[test]
fn every_production_run_uses_the_execution_service_world() {
    assert!(EXECUTION_ADAPTER.contains("ExecutionService::default"));
    assert!(EXECUTION_ADAPTER.contains("admit_root_bytes"));
    for source in [RUN, EXECUTION_ADAPTER, PROVENANCE] {
        assert!(!source.contains("captured: Option<"));
        assert!(!source.contains("has_captured_source"));
        assert!(!source.contains("world: Option<&AdmittedWorld>"));
    }
}

#[test]
fn execution_service_does_not_accept_a_capability_capturing_runner() {
    assert!(!EXECUTION_SERVICE.contains("execute_with"));
    assert!(!EXECUTION_SERVICE.contains("FnOnce"));
    assert!(EXECUTION_SERVICE.contains("It is not a process sandbox"));
}

#[test]
fn dry_run_refusals_render_the_admitted_pair_without_path_reentry() {
    assert!(!DRY_RUN.contains("check::run("));
    assert!(DRY_RUN.contains("check::run_admitted_pair"));
    assert!(EXECUTION_ADAPTER.contains("world.driver.root_source()"));
}

#[test]
fn child_execution_has_no_post_admission_filesystem_reader() {
    assert!(!CHILD_RUNNER.contains("std::fs::read_to_string"));
    assert!(!CHILD_RUNNER.contains("snapshot: Option<"));
    assert!(!CHILD_RUNNER.contains("ProdChildRunner::new"));
    assert!(!CHILD_RUNNER.contains("closure_digest_fs"));
    assert!(!CHILD_RUNNER.contains("fn resolve_against"));
    assert!(!CHILD_RUNNER.contains("struct ProdChildRunner"));
    assert!(EXECUTION_ADAPTER.contains("ServiceExecutionDriver::for_local_interface"));
    assert!(RUNTIME_DRIVER.contains("impl ChildRunner for ServiceExecutionDriver"));
    assert!(!RUNTIME_DRIVER.contains("pub trait CapturedSnapshot"));
    assert!(!RUNTIME_DRIVER.contains("workflow: &RawWorkflow,\n        report:"));
    for forbidden in ["std::fs::read(", "read_to_string(", "nika_fs::OwnedDir"] {
        assert!(
            !RUNTIME_DRIVER.contains(forbidden),
            "L3 execution driver contains forbidden reader `{forbidden}`"
        );
    }
}

#[test]
fn dependency_direction_stays_toward_the_execution_service() {
    assert!(CLI_CARGO.contains("nika-execution"));
    assert!(ARM_CARGO.contains("nika-execution"));
    assert!(!EXECUTION_CARGO.contains("nika-cli"));
    assert!(!ARM_CARGO.contains("nika-cli"));
}

#[test]
fn arm_adapter_has_no_process_http_or_latest_trace_bridge() {
    assert!(ARM_FIRE.contains("ExecutionService"));
    for forbidden in [
        "std::process::Command",
        "Command::new",
        "localhost",
        "127.0.0.1",
        "latest_trace",
        "read_dir(",
    ] {
        assert!(
            !ARM_FIRE.contains(forbidden),
            "ARM execution bridge contains forbidden token `{forbidden}`"
        );
    }
}

#[test]
fn stdin_bytes_enter_the_same_admitted_world_without_a_temp_file() {
    let root = b"nika: stdin\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n";
    let directory = tempfile::tempdir().expect("stdin project");
    let project = nika_fs::OwnedDir::open(directory.path()).expect("held stdin project");
    let source = crate::verbs::RunSource::from_bytes("-", root.to_vec()).expect("UTF-8 source");
    let service = nika_execution::ExecutionService::default();

    let admitted = super::execution_adapter::admit_source(
        &service,
        &project,
        std::path::Path::new("-"),
        &source,
    )
    .expect("admitted stdin");

    assert!(!directory.path().join("-").exists());
    assert_eq!(admitted.snapshot().root(), "-");
    assert_eq!(admitted.snapshot().bytes("-"), Some(root.as_slice()));
}
