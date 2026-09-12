// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_dap::liveness;
use std::path::PathBuf;

fn fixture(settled: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-output-liveness-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).expect("isolated directory");
    let path = dir.join("trace.ndjson");
    let mut events = crate::demo::success();
    if !settled {
        events.truncate(1);
    }
    let mut text = String::new();
    for event in &events {
        text.push_str(&serde_json::to_string(event).expect("event"));
        text.push('\n');
    }
    std::fs::write(&path, text).expect("journal");
    path
}

fn document(path: &std::path::Path) -> serde_json::Value {
    let output = outputs_json(path.to_str().expect("path"));
    assert_eq!(output.code, 0, "{}", output.text);
    serde_json::from_str(&output.text).expect("JSON document")
}

#[test]
fn outputs_liveness_unknown_and_settlement_are_distinct() {
    let path = fixture(false);
    let doc = document(&path);
    assert_eq!(doc["state"], "running");
    assert_eq!(doc["liveness"], "unknown");
    assert!(doc.get("settlement").is_none());
    let dir = path.parent().expect("directory");
    assert!(
        manage::ls_in(dir, Theme::new(false, true, false))
            .text
            .contains("running?")
    );
    let listed: serde_json::Value =
        serde_json::from_str(&manage::ls_json_in(dir).text).expect("listing");
    assert_eq!(listed["traces"][0]["state"], "running");
    assert_eq!(listed["traces"][0]["liveness"], "unknown");
    std::fs::remove_dir_all(dir).expect("cleanup fixture");

    let path = fixture(true);
    let lease = liveness::hold(&path).expect("stale lease must not override settlement");
    let doc = document(&path);
    assert_eq!(doc["state"], "succeeded");
    assert!(
        doc.get("liveness")
            .expect("explicit liveness field")
            .is_null()
    );
    assert_eq!(doc["settlement"]["status"], "succeeded");
    drop(lease);
    std::fs::remove_dir_all(path.parent().expect("directory")).expect("cleanup fixture");
}

#[cfg(unix)]
#[test]
fn outputs_liveness_observes_held_then_released_writer_lease() {
    let path = fixture(false);
    let journal = std::fs::read(&path).expect("original bytes");
    let lease = liveness::hold(&path).expect("writer lease");
    assert_eq!(document(&path)["liveness"], "alive");
    let dir = path.parent().expect("directory");
    assert!(
        !manage::ls_in(dir, Theme::new(false, true, false))
            .text
            .contains("running?")
    );
    assert!(
        liveness::hold(&path).is_err(),
        "reader must not steal the lease"
    );
    drop(lease);
    let doc = document(&path);
    assert_eq!(doc["liveness"], "dead");
    assert_eq!(
        doc["state"], "running",
        "writer death is not a run settlement"
    );
    assert!(doc.get("settlement").is_none());
    assert_eq!(std::fs::read(&path).expect("unchanged bytes"), journal);
    std::fs::remove_dir_all(dir).expect("cleanup fixture");
}

#[test]
fn outputs_liveness_foreign_malformed_and_torn_journals_stay_unknown() {
    for lease in [
        "not-json",
        "{\"pid\":1,\"host\":\"foreign-host-for-liveness-test.invalid\"}",
    ] {
        let path = fixture(false);
        std::fs::write(liveness::lease_path(&path), lease).expect("unjudgeable lease");
        let mut bytes = std::fs::read(&path).expect("journal");
        bytes.extend_from_slice(b"{torn-tail");
        std::fs::write(&path, &bytes).expect("torn valid prefix");
        let doc = document(&path);
        assert_eq!(doc["state"], "running");
        assert_eq!(doc["liveness"], "unknown");
        assert!(doc.get("settlement").is_none());
        assert_eq!(std::fs::read(&path).expect("unchanged bytes"), bytes);
        std::fs::remove_dir_all(path.parent().expect("directory")).expect("cleanup fixture");
    }
}

#[test]
fn outputs_liveness_is_null_for_every_terminal_state_despite_a_stale_lease() {
    use nika_event::EventKind;
    for (kind, state) in [
        (EventKind::WorkflowCompleted, "succeeded"),
        (EventKind::WorkflowFailed, "failed"),
        (EventKind::WorkflowCancelled, "cancelled"),
        (EventKind::WorkflowPaused, "paused"),
    ] {
        let path = fixture(false);
        let mut terminal = crate::demo::success()
            .last()
            .expect("terminal event")
            .clone();
        terminal.kind = kind;
        let mut bytes = std::fs::read_to_string(&path).expect("journal");
        bytes.push_str(&serde_json::to_string(&terminal).expect("event"));
        bytes.push('\n');
        std::fs::write(&path, bytes).expect("terminal journal");
        let lease = liveness::hold(&path).expect("stale held lease");
        for _ in 0..2 {
            let doc = document(&path);
            assert_eq!(doc["state"], state);
            assert_eq!(doc["settlement"]["status"], state);
            assert!(doc.get("liveness").expect("explicit null").is_null());
        }
        drop(lease);
        assert!(
            document(&path)
                .get("liveness")
                .expect("explicit null after release")
                .is_null()
        );
        std::fs::remove_dir_all(path.parent().expect("directory")).expect("cleanup fixture");
    }
}
