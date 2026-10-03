// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A captured journal is judged exactly as its file is, prose and `--json`
//! alike; the capture is the only bytes judged (a later edit of the file
//! changes nothing); a capture over the bound and a replay are refused.

use super::*;
use crate::exit;
use nika_dap::chain::CHAIN_GENESIS;
use nika_event::source_id::sha256_hex;

/// A chained journal the way the sink writes it (the verify tests' idiom).
fn chained(kinds: &[&str]) -> String {
    let mut chain = sha256_hex(CHAIN_GENESIS);
    let mut out = String::new();
    for kind in kinds {
        let mut v = serde_json::json!({
            "id": {"uuid": "01912345-0000-7000-8000-000000000001"},
            "timestamp": 1000, "kind": kind, "run": null,
            "correlation": null, "fields": []
        });
        v["chain"] = serde_json::Value::String(chain.clone());
        let line = serde_json::to_string(&v).expect("test json");
        chain = sha256_hex(line.as_bytes());
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// A journal staged under the temp dir (the verify tests' idiom), named per
/// test and process so parallel tests never share it.
fn staged(name: &str, raw: &str) -> String {
    let file = format!("nika-captured-{}-{name}.ndjson", std::process::id());
    let path = std::env::temp_dir().join(file);
    std::fs::write(&path, raw).expect("journal");
    path.display().to_string()
}

#[test]
fn a_capture_is_judged_as_its_file_in_prose_and_json() {
    for kinds in [
        &["workflow_started", "workflow_completed"][..],
        &["workflow_started", "task_started"][..],
    ] {
        let raw = chained(kinds);
        let trace = staged(kinds[1], &raw);
        for json in [false, true] {
            let opts = VerifyOptions {
                json,
                ..VerifyOptions::default()
            };
            let file = super::verify_with(&trace, &opts);
            let capture = verify_captured(&trace, &raw, &opts);
            assert_eq!((capture.code, &capture.text), (file.code, &file.text));
        }
        let _ = std::fs::remove_file(&trace);
    }
}

#[test]
fn only_the_captured_bytes_are_judged() {
    let raw = chained(&["workflow_started", "task_completed", "workflow_completed"]);
    let trace = staged("edited", &raw);
    std::fs::write(&trace, raw.replacen("task_completed", "task_failedxx", 1)).expect("edit");
    let file = super::verify_with(&trace, &VerifyOptions::default());
    assert_eq!(file.code, exit::FILE, "{}", file.text);
    let capture = verify_captured(&trace, &raw, &VerifyOptions::default());
    assert_eq!(capture.code, exit::OK, "{}", capture.text);
    assert!(
        capture.text.starts_with("OK — 3 events"),
        "{}",
        capture.text
    );
    let _ = std::fs::remove_file(&trace);
}

#[test]
fn a_capture_over_the_bound_is_refused_with_the_file_words() {
    let raw = "a".repeat(JOURNAL_BOUND + 1);
    let out = verify_captured("big.ndjson", &raw, &VerifyOptions::default());
    assert_eq!(out.code, exit::ENV);
    assert!(out.text.contains("over the journal bound"), "{}", out.text);
}

#[test]
fn a_replay_is_refused_for_a_capture() {
    let raw = chained(&["workflow_started", "workflow_completed"]);
    let trace = staged("replay", &raw);
    let opts = VerifyOptions {
        replay: Some(std::path::PathBuf::from(&trace)),
        ..VerifyOptions::default()
    };
    let out = verify_captured(&trace, &raw, &opts);
    assert_eq!(out.code, exit::ENV);
    assert!(out.text.contains("without --replay"), "{}", out.text);
    let _ = std::fs::remove_file(&trace);
}
