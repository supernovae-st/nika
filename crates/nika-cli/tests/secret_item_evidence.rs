// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! Synthetic resolved values are masked before machine output and journaling.
//! These workflows use native tools, a disposable HOME and no provider.

use serde_json::Value;
use std::path::Path;
use std::process::Command;

const CANARY: &str = "827351";

fn command(room: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
    command
        .current_dir(room)
        .env_clear()
        .env("HOME", room.join("home"))
        .env("TMPDIR", room.join("tmp"))
        .env("PATH", "/usr/bin:/bin")
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .env("NIKA_SYNTHETIC_TEST_SECRET", CANARY);
    command
}

fn workflow(paged: bool, fails: bool) -> String {
    // Short identities survive the runtime's display bound. Their count
    // crosses the real page threshold without relying on unobserved bytes.
    let count = if paged { 2000 } else { 2 };
    let items = serde_json::to_string(&vec![CANARY; count]).expect("synthetic items");
    let expression = if fails {
        format!("error(\"{CANARY}\")")
    } else {
        ".".to_owned()
    };
    format!(
        "nika: secret-items\npermits: {{ tools: [\"nika:jq\"] }}\nsecrets:\n  synthetic: {{ source: env, key: NIKA_SYNTHETIC_TEST_SECRET }}\ntasks:\n  fan:\n    for_each: {{ items: {items}, max_parallel: 1, fail_fast: false }}\n    invoke:\n      tool: nika:jq\n      args: {{ input: \"${{{{ item }}}}\", expression: '{expression}' }}\n"
    )
}

fn assert_no_canary(bytes: &[u8], surface: &str) {
    assert!(
        !bytes
            .windows(CANARY.len())
            .any(|part| part == CANARY.as_bytes()),
        "synthetic content escaped on {surface}"
    );
}

fn assert_rows(rows: &[Value], paged: bool, fails: bool) {
    assert_eq!(rows.len(), if paged { 2000 } else { 2 });
    for (index, row) in rows.iter().enumerate() {
        assert!(row["index"] == index, "item index changed");
        assert!(row["item"] == "***", "item identity was not masked exactly");
        assert!(
            row["status"] == if fails { "failed" } else { "ok" },
            "item status changed"
        );
        if fails {
            assert!(
                row["code"]
                    .as_str()
                    .expect("failure code")
                    .starts_with("NIKA-"),
                "item failure code changed"
            );
            let message = row["message"].as_str().expect("failure message");
            assert_no_canary(message.as_bytes(), "item message");
            assert!(message.contains("***"), "item message lacks mask");
        }
    }
}

fn run_case(paged: bool, fails: bool) {
    let room = tempfile::tempdir().expect("room");
    for name in ["home", "tmp"] {
        std::fs::create_dir(room.path().join(name)).expect("isolated directory");
    }
    std::fs::write(room.path().join("probe.nika"), workflow(paged, fails)).expect("workflow");
    let check = command(room.path())
        .args(["check", "probe.nika", "--json", "--native-strict"])
        .output()
        .expect("check process");
    assert_no_canary(&check.stdout, "check stdout");
    assert_no_canary(&check.stderr, "check stderr");
    assert!(
        check.status.success(),
        "synthetic workflow was not admitted"
    );
    let run = command(room.path())
        .args(["run", "probe.nika", "--json"])
        .output()
        .expect("run process");
    assert_no_canary(&run.stdout, "run stdout");
    assert_no_canary(&run.stderr, "run stderr");
    assert_eq!(run.status.success(), !fails, "unexpected run disposition");
    let stream = std::str::from_utf8(&run.stdout).expect("UTF-8 stream");
    let mut rows = Vec::new();
    let mut page_count = 0;
    for line in stream.lines() {
        let event: Value = serde_json::from_str(line).expect("event JSON");
        page_count += usize::from(event["kind"] == "task_items");
        for field in event["fields"].as_array().into_iter().flatten() {
            if field["key"] == "items" {
                rows.extend(
                    serde_json::from_str::<Vec<Value>>(
                        field["value"].as_str().expect("item table"),
                    )
                    .expect("item JSON"),
                );
            }
        }
    }
    assert_eq!(page_count > 0, paged, "paging witness not reached");
    assert_rows(&rows, paged, fails);
    let traces: Vec<_> = std::fs::read_dir(room.path().join(".nika/traces"))
        .expect("trace directory")
        .map(|entry| entry.expect("trace entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .collect();
    assert_eq!(traces.len(), 1, "expected one run journal");
    let trace = &traces[0];
    let bytes = std::fs::read(trace).expect("journal bytes");
    assert_no_canary(&bytes, "complete journal");
    let verify = command(room.path())
        .args(["trace", "verify", "--json"])
        .arg(trace)
        .output()
        .expect("verify process");
    assert_no_canary(&verify.stdout, "verify stdout");
    assert_no_canary(&verify.stderr, "verify stderr");
    assert!(
        verify.status.success(),
        "masked journal must remain verifiable"
    );
    let outputs = command(room.path())
        .args(["trace", "outputs", "--json"])
        .arg(trace)
        .output()
        .expect("outputs process");
    assert_no_canary(&outputs.stdout, "outputs stdout");
    assert_no_canary(&outputs.stderr, "outputs stderr");
    assert!(outputs.status.success(), "outputs projection must succeed");
    let document: Value = serde_json::from_slice(&outputs.stdout).expect("outputs JSON");
    assert_rows(
        document["tasks"][0]["items"]
            .as_array()
            .expect("projected rows"),
        paged,
        fails,
    );
}

#[test]
fn inline_success_masks_item_identity() {
    run_case(false, false);
}

#[test]
fn inline_failure_masks_identity_and_message() {
    run_case(false, true);
}

#[test]
fn paged_success_keeps_masked_rows_verifiable() {
    run_case(true, false);
}

#[test]
fn paged_failure_keeps_masked_messages_verifiable() {
    run_case(true, true);
}
