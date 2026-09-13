// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! #1458: real large fan-outs leave verifiable journals and retain every
//! item through the public trace reader. No provider, credentials or shell.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn command(room: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
    command
        .current_dir(room)
        .env_clear()
        .env("HOME", room.join("home"))
        .env("PATH", "/usr/bin:/bin")
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1");
    command
}

fn workflow(count: usize, fails: bool, fail_fast: bool) -> String {
    let items = (0..count)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let expression = if fails {
        "error(\"item failure\")"
    } else {
        "."
    };
    format!(
        "nika: paged-fan\npermits: {{ tools: [\"nika:jq\"] }}\ntasks:\n  fan:\n    for_each: {{ items: [{items}], max_parallel: 1, fail_fast: {fail_fast} }}\n    invoke:\n      tool: nika:jq\n      args: {{ input: \"${{{{ item }}}}\", expression: '{expression}' }}\n"
    )
}

fn field<'a>(event: &'a Value, name: &str) -> Option<&'a Value> {
    event["fields"]
        .as_array()?
        .iter()
        .find(|field| field["key"] == name)?
        .get("value")
}

fn journal(room: &Path) -> PathBuf {
    let files: Vec<_> = std::fs::read_dir(room.join(".nika/traces"))
        .expect("trace dir")
        .filter_map(Result::ok)
        .map(|file| file.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .collect();
    assert_eq!(files.len(), 1, "one run, one journal");
    files[0].clone()
}

fn assert_items(room: &Path, trace: &Path, count: usize, fails: bool, fail_fast: bool) {
    let output = command(room)
        .args(["trace", "outputs", "--json"])
        .arg(trace)
        .output()
        .expect("trace outputs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: Value = serde_json::from_slice(&output.stdout).expect("outputs json");
    let rows = document["tasks"][0]["items"]
        .as_array()
        .expect("complete table");
    assert_eq!(rows.len(), count);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["index"], index);
        assert_eq!(row["item"], index.to_string());
        let status = if !fails {
            "ok"
        } else if fail_fast && index > 0 {
            "never_started"
        } else {
            "failed"
        };
        assert_eq!(row["status"], status, "item {index}");
        if status == "failed" {
            assert!(
                row["code"]
                    .as_str()
                    .is_some_and(|code| code.starts_with("NIKA-"))
            );
            assert!(
                row["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("item failure"))
            );
        }
    }
}

fn run_case(count: usize, fails: bool, fail_fast: bool) {
    let room = tempfile::tempdir().expect("isolated room");
    std::fs::create_dir(room.path().join("home")).expect("isolated home");
    std::fs::write(
        room.path().join("fan.nika.yaml"),
        workflow(count, fails, fail_fast),
    )
    .expect("workflow");
    let output = command(room.path())
        .args(["run", "fan.nika.yaml", "--json", "--max-cost-usd", "0.01"])
        .output()
        .expect("real run");
    assert_eq!(
        output.status.success(),
        !fails,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stream = String::from_utf8(output.stdout).expect("stream");
    let settled: Value =
        serde_json::from_str(stream.lines().last().expect("settlement")).expect("json");
    assert_eq!(
        settled["status"],
        if fails { "failed" } else { "succeeded" }
    );
    assert_ne!(settled["evidence"], "lost");
    let trace = journal(room.path());
    let raw = std::fs::read_to_string(&trace).expect("journal");
    for body in [&raw, &stream] {
        assert!(
            body.lines()
                .all(|line| line.len() <= nika_dap::chain::MAX_LINE_BYTES)
        );
    }
    let events: Vec<Value> = raw
        .lines()
        .map(|line| serde_json::from_str(line).expect("event"))
        .collect();
    let pages = events
        .iter()
        .filter(|event| event["kind"] == "task_items")
        .count();
    let terminal = events
        .iter()
        .find(|event| {
            matches!(
                event["kind"].as_str(),
                Some("task_completed" | "task_failed")
            )
        })
        .expect("terminal");
    if count > 1000 {
        assert!(pages > 1);
        assert_eq!(field(terminal, "items_pages"), Some(&Value::from(pages)));
        assert_eq!(field(terminal, "items_total"), Some(&Value::from(count)));
        assert!(field(terminal, "items").is_none());
    } else {
        assert_eq!(pages, 0);
        assert!(field(terminal, "items").is_some());
    }
    let verify = command(room.path())
        .args(["trace", "verify", "--json"])
        .arg(&trace)
        .output()
        .expect("verify");
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stdout)
    );
    assert_items(room.path(), &trace, count, fails, fail_fast);
    if !fails && count > 1000 {
        let resumed = command(room.path())
            .args(["run", "fan.nika.yaml", "--json", "--resume"])
            .arg(&trace)
            .output()
            .expect("resume");
        assert!(
            resumed.status.success(),
            "{}",
            String::from_utf8_lossy(&resumed.stderr)
        );
        assert!(String::from_utf8_lossy(&resumed.stdout).contains("\"kind\":\"task_cache_hit\""));
    }
}

#[test]
fn large_successful_fan_out_keeps_verifiable_resumable_item_evidence() {
    run_case(17000, false, false);
}

#[test]
fn large_failed_fan_out_keeps_every_error_and_verifies() {
    run_case(2000, true, false);
}

#[test]
fn stopped_fan_out_keeps_the_never_started_tail_and_verifies() {
    run_case(17000, true, true);
}

#[test]
fn small_fan_out_keeps_the_inline_wire_shape() {
    run_case(8, false, false);
}
