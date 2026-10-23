// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite executes the real `nika` binary (CARGO_BIN_EXE): spawning it is
// its job (the resume_e2e.rs carve-out class).
#![allow(clippy::disallowed_types)]

//! NIK-131 at the binary plane: a step whose output is over 1 MiB keeps its
//! run's trace whole. The `task_completed` frame carries the value twice
//! (`output` · `outcome`); over the journal's 1 MiB line bound it used to be
//! refused, so the trace ended there and `--resume` had nothing to fold. Now
//! its largest values sit beside the journal by digest: the trace is
//! complete, `nika trace verify` passes, and a resume serves the step back
//! from the journal. A value altered beside the journal is refused by verify
//! and never served.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

/// 700,000 bytes on stdout: the frame that settles it is over 1.3 MiB.
const WORKFLOW: &str = r#"nika: big-page
permits: { exec: true, tools: ["nika:jq"] }
tasks:
  page:
    exec: { shell: "printf '%0700000d' 0 | tr 0 q" }
  size:
    with: { page: "${{ tasks.page.output }}" }
    invoke: { tool: "nika:jq", args: { input: { page: "${{ with.page }}" }, expression: ".page | length" } }
outputs:
  bytes: ${{ tasks.size.output }}
"#;

fn nika(room: &Path, args: &[&str]) -> Output {
    let home = room.join(".home");
    std::fs::create_dir_all(&home).expect("isolated home");
    Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(args)
        .current_dir(room)
        .stdin(Stdio::null())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", room.join("absent-key"))
        .env("NIKA_RUN_PUB_FILE", room.join("absent-pub"))
        .output()
        .expect("the binary runs")
}

fn said(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout)
            .chars()
            .take(4000)
            .collect::<String>(),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The one journal the run wrote under `.nika/traces`.
fn journal(room: &Path) -> PathBuf {
    let traces = room.join(".nika/traces");
    let mut found: Vec<PathBuf> = (std::fs::read_dir(&traces).expect("traces"))
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

/// The frames of a `--json` stream naming `task`, by kind.
fn kinds_for(output: &Output, task: &str) -> Vec<String> {
    (String::from_utf8_lossy(&output.stdout).lines())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|frame| {
            (frame["fields"].as_array().into_iter().flatten())
                .any(|kv| kv["key"] == "task" && kv["value"] == task)
        })
        .filter_map(|frame| frame["kind"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn a_step_over_one_mib_keeps_its_trace_whole_verified_and_resumable() {
    let room = tempfile::tempdir().expect("isolated room");
    let room = room.path();
    std::fs::write(room.join("page.nika"), WORKFLOW).expect("workflow");
    let first = nika(room, &["run", "page.nika", "--json", "--color", "never"]);
    assert_eq!(first.status.code(), Some(0), "{}", said(&first));
    let trace = journal(room);
    let raw = std::fs::read_to_string(&trace).expect("journal");
    assert!(
        raw.lines().all(|line| line.len() <= 1_048_576),
        "the line bound holds"
    );
    assert!(
        raw.lines()
            .any(|line| line.contains("\"workflow_completed\"")),
        "the trace reaches its terminal frame"
    );
    let mut values = trace.as_os_str().to_owned();
    values.push(".values");
    let kept: Vec<PathBuf> = (std::fs::read_dir(PathBuf::from(&values)).expect("values beside"))
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert!(!kept.is_empty(), "the large value sits beside the journal");

    let trace_arg = trace.to_string_lossy().into_owned();
    let verified = nika(room, &["trace", "verify", &trace_arg]);
    assert_eq!(verified.status.code(), Some(0), "{}", said(&verified));
    assert!(
        said(&verified).contains("chain intact"),
        "{}",
        said(&verified)
    );

    let resumed = nika(
        room,
        &[
            "run",
            "page.nika",
            "--resume",
            &trace_arg,
            "--json",
            "--color",
            "never",
        ],
    );
    assert_eq!(resumed.status.code(), Some(0), "{}", said(&resumed));
    let page = kinds_for(&resumed, "page");
    assert!(
        page.contains(&"task_cache_hit".to_owned()),
        "{page:?} · {}",
        said(&resumed)
    );
    assert!(
        !page.contains(&"task_started".to_owned()),
        "served, never re-run"
    );

    // An altered value is the tampered class: verify refuses it.
    let mut bytes = std::fs::read(&kept[0]).expect("kept value");
    bytes[0] ^= 1;
    std::fs::write(&kept[0], bytes).expect("alter");
    let refused = nika(room, &["trace", "verify", &trace_arg]);
    assert_eq!(refused.status.code(), Some(2), "{}", said(&refused));
    assert!(said(&refused).contains("BROKEN"), "{}", said(&refused));
}
