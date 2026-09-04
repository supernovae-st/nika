// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The trace store seam — the scan itself (workflow name · terminal
//! state · size · age per trace, ADR-100) lives in the forensics crate
//! (`nika_dap::store` · the 2026-07-09 W0 trace descent), re-exported
//! here so every `trace::store::` consumer reads unchanged (the
//! `trace_verify` pattern). This file keeps the one DISPLAY hand: the
//! compact age cell `trace ls` prints.

pub(crate) use nika_dap::store::{TRACE_DIR, TraceMeta, TraceState, fold_facts, scan};

/// A compact age cell (`42s` · `5m` · `3h` · `12d`) — the `trace ls`
/// column (durations elsewhere speak `fmt_wall_ms`).
pub(crate) fn fmt_age(elapsed: std::time::Duration) -> String {
    let s = elapsed.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3_600 {
        format!("{}m", s / 60)
    } else if s < 86_400 {
        format!("{}h", s / 3_600)
    } else {
        format!("{}d", s / 86_400)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    use nika_event::{Event, EventKind};
    use nika_types::id::EventId;
    use nika_types::resource::{KeyValue, Value};
    use nika_types::timestamp::Timestamp;
    use uuid::Uuid;

    /// One journal event (nil id · fixed clock) with a task/workflow field.
    pub(crate) fn event(kind: EventKind, key: &str, name: &str, ms: u64) -> Event {
        Event::new(EventId::new(Uuid::nil()), Timestamp::from_unix_ms(ms), kind)
            .with_field(KeyValue::new(key, Value::String(name.to_owned())))
    }

    /// Serialize events as one NDJSON trace body.
    pub(crate) fn ndjson(events: &[Event]) -> String {
        let mut body = String::new();
        for ev in events {
            body.push_str(&serde_json::to_string(ev).expect("event serializes"));
            body.push('\n');
        }
        body
    }

    /// The journal of one run: started(workflow) · a task · a terminal.
    pub(crate) fn run_events(workflow: &str, terminal: Option<EventKind>) -> Vec<Event> {
        let mut events = vec![
            event(EventKind::WorkflowStarted, "workflow", workflow, 0),
            event(EventKind::TaskCompleted, "task", "step", 10),
        ];
        if let Some(kind) = terminal {
            events.push(event(kind, "task", "gate", 20));
        }
        events
    }

    /// A fresh per-test trace directory under the cargo tmp root.
    pub(crate) fn temp_store(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join("nika-cli-trace-store");
        let dir = base.join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("store dir");
        dir
    }

    /// Stage one trace file and BACKDATE its mtime by `age` — the age
    /// clock the policy reads is the file's mtime.
    pub(crate) fn stage_trace(dir: &Path, name: &str, body: &str, age: Duration) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("trace staged");
        let mtime = SystemTime::now()
            .checked_sub(age)
            .expect("test age fits the clock");
        let file = std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("reopen for times");
        file.set_times(std::fs::FileTimes::new().set_modified(mtime))
            .expect("mtime set");
        path
    }

    /// The re-exported seam scans the staged store (the descended fold
    /// answers through the old path — the shim's whole contract).
    #[test]
    fn the_reexported_scan_reads_a_staged_store() {
        let dir = temp_store("shim");
        stage_trace(
            &dir,
            "ok.ndjson",
            &ndjson(&run_events("veille", Some(EventKind::WorkflowCompleted))),
            Duration::from_secs(60),
        );
        let traces = scan(&dir);
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].state, TraceState::Succeeded);
        assert_eq!(traces[0].workflow, "veille");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The age cell speaks the compact unit ladder.
    #[test]
    fn age_cell_speaks_compact_units() {
        assert_eq!(fmt_age(Duration::from_secs(42)), "42s");
        assert_eq!(fmt_age(Duration::from_secs(5 * 60)), "5m");
        assert_eq!(fmt_age(Duration::from_secs(3 * 3_600)), "3h");
        assert_eq!(fmt_age(Duration::from_secs(12 * 86_400)), "12d");
    }
}
