// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's WHOLE JOB is to execute the
// real `nika-cli` binary (CARGO_BIN_EXE) — the bin_smoke carve-out
// class: the contract under test IS the binary's pipeline behavior.
#![allow(clippy::disallowed_types)]

//! A-4 (F-08) · pipe hygiene: a reader that closes early (`| head`)
//! must never spill a raw Rust panic. The honest death is silent
//! stderr + exit 141 — the code a SIGPIPE-killed process reports, what
//! every unix tool in a pipeline says.

#[cfg(unix)]
#[test]
fn a_closed_pipe_dies_clean_with_the_unix_code() {
    use std::fmt::Write as _;
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::process::{Command, Stdio};

    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join("tmp")
        .join(format!("pipe-hygiene-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");

    // Enough streaming frames that the renderer still writes after the
    // reader is gone — the live card redraws per task.
    let mut yaml = String::from("nika: pipe-drill\nmodel: mock/echo\npermits: {}\ntasks:\n");
    for i in 0..40 {
        let _ = write!(
            yaml,
            "  t{i:02}:\n    infer:\n      prompt: \"line {i} long enough to keep the frames coming for the drill\"\n      max_tokens: 32\n"
        );
    }
    let wf = dir.join("pipe-drill.nika.yaml");
    std::fs::File::create(&wf)
        .expect("fixture file")
        .write_all(yaml.as_bytes())
        .expect("fixture body");

    let mut child = Command::new(env!("CARGO_BIN_EXE_nika"))
        .arg("run")
        .arg(&wf)
        .args(["--model", "mock/echo"])
        .current_dir(&dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary spawns");
    // Read ONE line, then drop the handle: the pipe closes while the
    // renderer is mid-stream — the exact F-08 shape.
    {
        let stdout = child.stdout.take().expect("piped stdout");
        let mut first = String::new();
        let _ = BufReader::new(stdout).read_line(&mut first);
    }
    let out = child.wait_with_output().expect("child settles");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("panicked"),
        "a closed pipe is the caller's choice, never a crash:\n{stderr}"
    );
    // Either the run finished before the close (0) or it died the unix
    // way (141) — never the panic exit (101), never a raw abort.
    let code = out.status.code();
    assert!(
        code == Some(141) || code == Some(0),
        "honest pipe death (141) or a clean finish (0) · got {code:?}\nstderr: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
