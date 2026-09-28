// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! Public CLI readiness and actual fire share the required-input law, and
//! the version-2 receipt binds its evidence, status and exit to one verdict.

use serde_json::{Value, json};
use std::path::Path;
use std::process::{Command, Output, Stdio};

const WORKFLOW: &str = "nika: locale-report\ninputs:\n  locale: {type: string, required: true}\npermits:\n  tools: [nika:write]\n  fs: {write: [./locale.txt]}\ntasks:\n  save:\n    invoke:\n      tool: nika:write\n      args:\n        path: ./locale.txt\n        content: '${{ inputs.locale }}'\n        create_dirs: false\n";

fn registry(binding: &str) -> String {
    format!(
        "nika: schedule-probe\narm:\n  - workflow: report.nika\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.05\n    manqué: sauter\n{binding}"
    )
}

fn project(binding: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("project");
    std::fs::write(dir.path().join("nika.yaml"), registry(binding)).expect("registry");
    std::fs::write(dir.path().join("report.nika"), WORKFLOW).expect("workflow");
    dir
}

fn command(root: &Path) -> Command {
    // A fire may probe harness CLIs that write into HOME: keep it the fixture's.
    let home = root.join(".home");
    std::fs::create_dir_all(&home).expect("fixture home");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
    cmd.current_dir(root)
        .stdin(Stdio::null())
        .env("HOME", home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", root.join("absent-key"))
        .env("NIKA_RUN_PUB_FILE", root.join("absent-pub"));
    cmd
}

fn output(root: &Path, args: &[&str]) -> Output {
    command(root).args(args).output().expect("CLI")
}

fn report(root: &Path) -> Value {
    let out = output(root, &["arm", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("machine report")
}

#[test]
fn a_missing_input_is_unready_and_reading_never_creates_a_claim() {
    let dir = project("");
    let out = output(dir.path(), &["arm", "--plain"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("[UNREADY]") && text.contains("locale"),
        "{text}"
    );
    assert!(!text.contains("[armed]"), "{text}");
    let value = report(dir.path());
    let beat = &value["schedules"][0];
    assert_eq!(beat["program_ready"], true);
    assert_eq!(beat["required_inputs_ready"], false);
    assert_eq!(beat["arm_ready"], false);
    assert_eq!(beat["required_inputs"][0]["binding_source"], Value::Null);
    assert_eq!(beat["blockers"][0]["code"], "NIKA-1708");
    assert_eq!(beat["blockers"][0]["kind"], "input_unbound");
    assert_eq!(beat["unbound_inputs"], json!(["locale"]));
    assert_eq!(beat["status"], "UNREADY");
    assert_eq!(value["schedule_readiness_version"], 2);
    assert!(
        !dir.path().join(".nika").exists(),
        "inspection must stay read-only"
    );
    let fired = output(
        dir.path(),
        &["arm", "fire", "report", "--now", "2026-09-28T03:00:10Z"],
    );
    assert_eq!(fired.status.code(), Some(3));
    assert!(!dir.path().join("locale.txt").exists());
}

#[test]
fn explicit_binding_and_declared_default_match_the_real_business_result() {
    for default in [false, true] {
        let dir = project(if default {
            ""
        } else {
            "    inputs: {locale: fr}\n"
        });
        if default {
            std::fs::write(
                dir.path().join("report.nika"),
                WORKFLOW.replace("required: true", "required: true, default: fr"),
            )
            .expect("default");
        }
        let value = report(dir.path());
        let beat = &value["schedules"][0];
        assert_eq!(beat["required_inputs_ready"], true);
        assert_eq!(beat["status"], "READY", "{beat}");
        assert_eq!(beat["activation"]["status"], "not_verified");
        assert_eq!(beat["authority"], "not_acquired");
        assert_eq!(
            beat["arm_ready"],
            Value::Null,
            "binding proof never grants activation authority"
        );
        assert_eq!(
            beat["required_inputs"][0]["binding_source"],
            if default {
                "workflow_default"
            } else {
                "schedule_literal"
            }
        );
        assert!(!dir.path().join(".nika").exists());
        let fired = output(
            dir.path(),
            &["arm", "fire", "report", "--now", "2026-09-28T03:00:10Z"],
        );
        assert!(
            fired.status.success(),
            "{}",
            String::from_utf8_lossy(&fired.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("locale.txt")).expect("effect"),
            "fr"
        );
    }
}

#[test]
fn a_missing_or_invalid_workflow_cannot_be_reported_as_armed() {
    for source in [None, Some("nika: invalid\nunknown: true\n")] {
        let dir = project("    inputs: {locale: fr}\n");
        match source {
            Some(source) => {
                std::fs::write(dir.path().join("report.nika"), source).expect("invalid");
            }
            None => {
                std::fs::remove_file(dir.path().join("report.nika")).expect("remove own fixture");
            }
        }
        let value = report(dir.path());
        assert_eq!(value["schedules"][0]["program_ready"], false);
        assert_eq!(value["schedules"][0]["arm_ready"], false);
        let human = output(dir.path(), &["arm", "--plain"]);
        assert!(String::from_utf8_lossy(&human.stdout).contains("[UNREADY]"));
        assert!(!dir.path().join(".nika").exists());
    }
}

#[test]
fn binding_and_source_changes_invalidate_the_existing_firing_identity() {
    let dir = project("    inputs: {locale: fr}\n");
    let first = report(dir.path());
    std::fs::write(
        dir.path().join("nika.yaml"),
        registry("    inputs: {locale: en}\n"),
    )
    .expect("binding");
    let second = report(dir.path());
    assert_eq!(
        first["schedules"][0]["snapshot_digest"],
        second["schedules"][0]["snapshot_digest"]
    );
    assert_ne!(
        first["schedules"][0]["generation"],
        second["schedules"][0]["generation"]
    );
    std::fs::write(
        dir.path().join("report.nika"),
        WORKFLOW.replace(
            "inputs:\n",
            "inputs:\n  zone: {type: string, required: true}\n",
        ),
    )
    .expect("revision");
    let third = report(dir.path());
    assert_ne!(
        second["schedules"][0]["snapshot_digest"],
        third["schedules"][0]["snapshot_digest"]
    );
    assert_eq!(third["schedules"][0]["required_inputs_ready"], false);
    assert!(!dir.path().join(".nika").exists());
}

#[test]
fn invalid_environment_values_stay_private_in_both_reports() {
    let dir = project("    inputs: {locale: '@env:NIKA_TEST_ARM_PRIVATE'}\n");
    std::fs::write(
        dir.path().join("report.nika"),
        WORKFLOW.replace("type: string", "type: integer"),
    )
    .expect("numeric");
    for args in [["arm", "--json"], ["arm", "--plain"]] {
        let out = command(dir.path())
            .env("NIKA_TEST_ARM_PRIVATE", "private-not-a-number-sentinel")
            .args(args)
            .output()
            .expect("private probe");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!text.contains("private-not-a-number-sentinel"), "{text}");
        if args[1] == "--json" {
            let value: Value = serde_json::from_slice(&out.stdout).expect("JSON");
            assert_eq!(value["schedules"][0]["required_inputs_ready"], false);
        }
    }
}

#[test]
fn machine_inventory_cannot_accidentally_fire_or_emit_a_unit() {
    let dir = project("    inputs: {locale: fr}\n");
    let out = output(dir.path(), &["arm", "--json", "fire", "report"]);
    assert_eq!(out.status.code(), Some(2));
    let value: Value = serde_json::from_slice(&out.stdout).expect("structured refusal");
    assert_eq!(value["error"]["code"], json!("invalid_arm_options"));
    assert!(!dir.path().join("locale.txt").exists());
    assert!(!dir.path().join(".nika").exists());
}

#[test]
fn inspecting_history_neither_repairs_caches_nor_hides_corruption() {
    let dir = project("    inputs: {locale: fr}\n");
    let fired = output(
        dir.path(),
        &["arm", "fire", "report", "--now", "2026-09-28T03:00:10Z"],
    );
    assert!(fired.status.success());
    let sidecar = dir.path().join(".nika/arm/report");
    let ledger = sidecar.join("history.ndjson");
    let original = std::fs::read(&ledger).expect("real firing history");
    let last = sidecar.join("last.json");
    std::fs::remove_file(&last).expect("simulate missing projection cache");
    for args in [["arm", "--plain"], ["arm", "--json"]] {
        let out = output(dir.path(), &args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!last.exists(), "inspection must not rebuild a projection");
        assert_eq!(std::fs::read(&ledger).expect("unchanged history"), original);
        if args[1] == "--plain" {
            assert!(
                String::from_utf8_lossy(&out.stdout).contains("✓ PROUVÉ (génération + créneau")
            );
        }
    }
    let mut corrupt = original.clone();
    corrupt.extend_from_slice(b"invalid journal row\n");
    std::fs::write(&ledger, &corrupt).expect("inject corruption into own fixture");
    let out = output(dir.path(), &["arm", "--plain"]);
    assert_eq!(out.status.code(), Some(3));
    let json = output(dir.path(), &["arm", "--json"]);
    assert_eq!(json.status.code(), Some(3), "E16-5: both views exit alike");
    let value: Value = serde_json::from_slice(&json.stdout).expect("refused evidence JSON");
    assert_eq!(value["evidence_refused"], json!(["report"]));
    assert_eq!(value["schedules"][0]["arm_ready"], false);
    assert_eq!(
        value["schedules"][0]["firing_evidence"]["status"],
        "refused"
    );
    assert!(!last.exists());
    assert_eq!(
        std::fs::read(&ledger).expect("corruption preserved"),
        corrupt
    );
}

/// The human report as printed: stdout, or stderr under `nika:` when the
/// exit is ENV (3), the binary's emit law for every ENV outcome.
fn plain(root: &Path) -> (Option<i32>, String) {
    let out = output(root, &["arm", "--plain"]);
    let shown = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), shown)
}

fn machine(root: &Path) -> (Option<i32>, Value) {
    let out = output(root, &["arm", "--json"]);
    let value = serde_json::from_slice(&out.stdout).expect("machine report");
    (out.status.code(), value)
}

fn fire(root: &Path) -> Output {
    output(
        root,
        &["arm", "fire", "report", "--now", "2026-09-28T03:00:10Z"],
    )
}

/// Every file under `dir`, with its bytes, sorted.
fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("bytes");
                files.push((path.display().to_string(), bytes));
            }
        }
    }
    files.sort();
    files
}

/// E16-2 · a miss policy every fire refuses reads UNREADY in both views,
/// and the firer refuses exactly what readiness names.
#[test]
fn a_policy_every_fire_refuses_is_never_registered() {
    let dir = project("    inputs: {locale: fr}\n");
    let source =
        registry("    inputs: {locale: fr}\n").replace("manqué: sauter", "manqué: rattraper");
    std::fs::write(dir.path().join("nika.yaml"), source).expect("registry");
    let (code, value) = machine(dir.path());
    assert_eq!(code, Some(0));
    let beat = &value["schedules"][0];
    assert_eq!(beat["status"], "UNREADY");
    assert_eq!(beat["trigger_binding_ready"], false);
    assert_eq!(beat["blockers"][0]["kind"], "policy_unsupported");
    assert_eq!(beat["next_fire"], Value::Null);
    let (_, text) = plain(dir.path());
    assert!(
        text.contains("[UNREADY]") && text.contains("manqué: rattraper is not supported"),
        "{text}"
    );
    assert_eq!(fire(dir.path()).status.code(), Some(2));
    assert!(!dir.path().join("locale.txt").exists());
}

/// E16-1 · E16-5 · a sidecar copied onto another beat is refused as that
/// beat's evidence; both views render every beat and exit 3; nothing moves.
#[test]
fn a_copied_sidecar_is_unattributed_and_both_views_exit_three() {
    let dir = project("    inputs: {locale: fr}\n");
    let two = format!(
        "{}  - workflow: other.nika\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.05\n    manqué: sauter\n    inputs: {{locale: en}}\n",
        registry("    inputs: {locale: fr}\n")
    );
    std::fs::write(dir.path().join("nika.yaml"), two).expect("registry");
    std::fs::write(dir.path().join("other.nika"), WORKFLOW).expect("other workflow");
    let fired = fire(dir.path());
    assert!(
        fired.status.success(),
        "{}",
        String::from_utf8_lossy(&fired.stderr)
    );
    let from = dir.path().join(".nika/arm/report");
    let to = dir.path().join(".nika/arm/other");
    std::fs::create_dir(&to).expect("copy target");
    for entry in std::fs::read_dir(&from).expect("sidecar") {
        let entry = entry.expect("entry");
        if entry.file_type().expect("type").is_file() {
            std::fs::copy(entry.path(), to.join(entry.file_name())).expect("copy");
        }
    }
    let before = tree(&dir.path().join(".nika/arm"));
    let (code, text) = plain(dir.path());
    assert_eq!(code, Some(3), "{text}");
    assert!(text.contains("✓ PROUVÉ (génération + créneau"), "{text}");
    assert!(
        text.contains("✗ NON ATTRIBUÉ (créneau non dérivable de ce beat)"),
        "{text}"
    );
    assert!(
        text.contains("exit 3 · firing evidence refused or unattributed for other"),
        "{text}"
    );
    let (code, value) = machine(dir.path());
    assert_eq!(code, Some(3));
    assert_eq!(value["evidence_refused"], json!(["other"]));
    let schedules = value["schedules"].as_array().expect("every beat");
    assert_eq!(schedules.len(), 2);
    assert_eq!(
        schedules[0]["firing_evidence"]["binding"],
        "current_generation"
    );
    assert_eq!(
        schedules[0]["firing_evidence"]["project_authenticity"],
        "not_proven"
    );
    assert_eq!(schedules[1]["firing_evidence"]["binding"], "unattributed");
    assert_eq!(schedules[1]["status"], "UNREADY");
    assert_eq!(
        tree(&dir.path().join(".nika/arm")),
        before,
        "reads repair nothing"
    );
}

/// E16-6 · after a rebinding the old receipt is history, never PROUVÉ.
#[test]
fn a_rebinding_turns_the_proof_into_history() {
    let dir = project("    inputs: {locale: fr}\n");
    assert!(fire(dir.path()).status.success());
    std::fs::write(
        dir.path().join("nika.yaml"),
        registry("    inputs: {locale: en}\n"),
    )
    .expect("rebinding");
    let (code, value) = machine(dir.path());
    assert_eq!(code, Some(0));
    let evidence = &value["schedules"][0]["firing_evidence"];
    assert_eq!(evidence["binding"], "historical_generation");
    assert_ne!(
        evidence["recorded_generation"],
        evidence["current_generation"]
    );
    assert_eq!(
        evidence["current_generation"],
        value["schedules"][0]["generation"]
    );
    let (_, text) = plain(dir.path());
    assert!(
        text.contains("· HISTORIQUE (autre génération que la courante"),
        "{text}"
    );
    assert!(!text.contains("PROUVÉ"), "{text}");
}

/// E16-4 · an undeclared binding is named alone, with its reason; the
/// correctly bound required input stays bound; the fire refuses alike.
#[test]
fn each_refused_binding_is_named_alone() {
    let dir = project("    inputs: {locale: fr, extra: '1'}\n");
    let (code, value) = machine(dir.path());
    assert_eq!(code, Some(0));
    let beat = &value["schedules"][0];
    assert_eq!(
        beat["blockers"].as_array().expect("blockers").len(),
        1,
        "{beat}"
    );
    assert_eq!(beat["blockers"][0]["kind"], "input_refused");
    assert_eq!(beat["blockers"][0]["subject"], "extra");
    assert_eq!(beat["blockers"][0]["reason"], "unknown_input");
    assert_eq!(beat["required_inputs"][0]["binding_status"], "bound");
    assert_eq!(beat["undeclared_bindings"][0]["name"], "extra");
    let (_, text) = plain(dir.path());
    assert!(
        text.contains("binding `extra` names no declared input")
            && text.contains("(input_refused)"),
        "{text}"
    );
    assert_eq!(fire(dir.path()).status.code(), Some(2));
    assert!(!dir.path().join("locale.txt").exists());
}

/// E16-3 · a fire whose environment value misfits names the input and the
/// type, never the value — on its streams and in the ledger.
#[test]
fn a_fire_never_echoes_an_environment_value() {
    let dir = project("    inputs: {locale: '@env:NIKA_TEST_ARM_PRIVATE'}\n");
    std::fs::write(
        dir.path().join("report.nika"),
        WORKFLOW
            .replace("type: string", "type: integer")
            .replace("permits:\n", "permits:\n  env: [NIKA_TEST_ARM_PRIVATE]\n"),
    )
    .expect("numeric");
    let out = command(dir.path())
        .env("NIKA_TEST_ARM_PRIVATE", "private-not-a-number-sentinel")
        .args(["arm", "fire", "report", "--now", "2026-09-28T03:00:10Z"])
        .output()
        .expect("fire");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "{text}");
    assert!(!text.contains("private-not-a-number-sentinel"), "{text}");
    assert!(text.contains("withheld"), "{text}");
    assert!(!dir.path().join("locale.txt").exists());
    let arm = dir.path().join(".nika/arm");
    if arm.exists() {
        for (path, bytes) in tree(&arm) {
            assert!(
                !String::from_utf8_lossy(&bytes).contains("private-not-a-number-sentinel"),
                "{path}"
            );
        }
    }
}
