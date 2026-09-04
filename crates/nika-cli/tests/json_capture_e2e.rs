// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used)]
#![allow(clippy::disallowed_types)]

//! Binary contracts for streamed JSON captures that do not use a disk trace.

use std::process::Command;

const CHAIN: &str = r#"
nika: json-capture
permits: { exec: ["echo"] }
tasks:
  a:
    exec: { command: ["echo", "alpha"] }
  b:
    with:
      alpha: ${{ tasks.a.output }}
    exec: { command: ["echo", "beta", "${{ with.alpha }}"] }
outputs:
  built: ${{ tasks.b.output }}
"#;

fn bin() -> Command {
    let home = std::env::temp_dir()
        .join("nika-json-capture-e2e")
        .join(format!("home-{}", std::process::id()));
    std::fs::create_dir_all(&home).expect("isolated home");
    let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
    command.env("HOME", home);
    command
}

#[test]
fn a_no_trace_json_capture_is_a_verifiable_terminal_journal() {
    let dir = std::env::temp_dir().join(format!("nika-no-trace-capture-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("run dir");
    std::fs::write(dir.join("chain.nika.yaml"), CHAIN).expect("workflow");
    let run = bin()
        .args([
            "run",
            "chain.nika.yaml",
            "--json",
            "--color",
            "never",
            "--no-trace-file",
        ])
        .current_dir(&dir)
        .output()
        .expect("binary runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "the no-trace run: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let raw = String::from_utf8(run.stdout).expect("utf8");
    let lines = raw.lines().collect::<Vec<_>>();
    assert!(
        lines.iter().all(|line| line.contains("\"chain\":\"")),
        "every no-trace streamed line carries the chain"
    );
    let terminal: serde_json::Value =
        serde_json::from_str(lines.last().expect("terminal line")).expect("terminal json");
    assert_eq!(terminal["kind"], "run_settled");
    assert!(
        terminal.get("receipt").is_none(),
        "no disk trace means no receipt: {terminal}"
    );

    let capture = dir.join("no-trace-capture.ndjson");
    std::fs::write(&capture, raw).expect("capture written");
    let verified = bin()
        .args(["trace", "verify", &capture.to_string_lossy()])
        .output()
        .expect("binary runs");
    assert_eq!(
        verified.status.code(),
        Some(0),
        "the no-trace capture is intact: {}",
        String::from_utf8_lossy(&verified.stderr)
    );
}
