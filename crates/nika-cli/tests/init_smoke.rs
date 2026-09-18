// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika init` founding-surface smoke — the REAL binary over the
//! scriptable twins (`--recipe` · `--theme`) and the plain `--yes`
//! file report and hand-off. Split from
//! `bin_smoke.rs` under the 1500-line file law.

#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::path::PathBuf;
use std::process::Command;

/// The compiled binary under test (the cargo-provided path).
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika"))
}

/// A unique scratch dir per test-process (workspace-independent).
fn workspace_tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// The first hour, end to end against the real binary: copy a lesson
/// home → the bare lazy door finds it → run it offline → found the
/// repo around an example via the scriptable twin — and `new <slug>`
/// resolves the SAME slug (one resolution, two handles).
/// 30S-W8 seed — THE JOURNEY: the exact path a stranger walks in their
/// first minutes (welcome → init → new → check → run offline → prove →
/// explain), as ONE test against the real binary in a fresh HOME. Every
/// step asserts its teaching surface AND that a seeded canary key VALUE
/// never reaches any output byte. The funnel can no longer silently rot.
#[test]
fn the_thirty_second_journey_holds_end_to_end() {
    let canary = "hunter2-JOURNEY-CANARY-never-printed";
    let home = std::env::temp_dir().join(format!("nika-journey-home-{}", std::process::id()));
    let dir = home.join("project");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let step = |args: &[&str]| {
        let out = bin()
            .args(args)
            .current_dir(&dir)
            .env("HOME", &home)
            .env("ANTHROPIC_API_KEY", canary)
            .env("OPENAI_API_KEY", canary)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("binary runs");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !text.contains(canary),
            "step {args:?} leaked a key VALUE: {text}"
        );
        (out.status.code(), text)
    };

    // 1 · welcome — the mirror greets, and with zero workflows the
    //     stranger SEES the language (the sample block).
    let (code, text) = step(&["welcome"]);
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("a whole workflow is one file"), "{text}");
    // The seat cascade landed above it; both must hold. The screen names
    // a reachable model AND shows what a workflow looks like.
    assert!(text.contains("Next:"), "one next step: {text}");

    // 2 · init — the repo gets briefed (editor + agents).
    let (code, text) = step(&["init", "--yes"]);
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("AGENTS.md"), "{text}");

    // Compile exposes the missing slot as data, then accepts an explicit answer.
    let (code, text) = step(&["compile", "chain", "--json"]);
    assert_eq!(code, Some(2), "{text}");
    let pending: serde_json::Value = serde_json::from_str(&text).expect("structured result");
    let key = pending["questions"][0]["key"]
        .as_str()
        .expect("question key");
    assert!(!dir.join("first.nika").exists());
    let answer = format!("{key}=\"Summarize the gathered text in one sentence.\"");
    let (code, text) = step(&["compile", "chain", "first.nika", "--answer", &answer]);
    assert_eq!(code, Some(0), "{text}");

    // 5 · check — the filled workflow passes before any token.
    let (code, text) = step(&["check", "first.nika"]);
    assert_eq!(code, Some(0), "audit before run: {text}");

    // 6 · run offline — the chain skeleton reads ./README.md (the file
    //     any real repo has · spec #68); this journey's temp repo writes
    //     its own, then runs under mock (zero keys, zero network — the
    //     canary env proves the run needed neither).
    std::fs::write(dir.join("README.md"), "# the stranger's repo\n").expect("readme");
    let (code, text) = step(&["run", "first.nika", "--model", "mock/echo"]);
    assert_eq!(code, Some(0), "first offline run is green: {text}");
    assert!(text.contains("done"), "{text}");

    // 7 · prove — the run left a hash-chained journal; verify it.
    let traces: Vec<_> = std::fs::read_dir(dir.join(".nika/traces"))
        .expect("trace dir exists")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .collect();
    assert_eq!(traces.len(), 1, "exactly one journal: {traces:?}");
    let trace = traces[0].to_string_lossy().into_owned();
    let (code, text) = step(&["trace", "verify", &trace]);
    assert_eq!(code, Some(0), "the chain verifies: {text}");

    // 8 · explain — the human story, now with the recorder section live.
    let (code, text) = step(&["explain", "first.nika"]);
    assert_eq!(code, Some(0), "{text}");
    for needle in [
        "the story",
        "cost before a token is spent",
        "flight recorder",
    ] {
        assert!(text.contains(needle), "missing `{needle}`: {text}");
    }

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_first_hour_walks_end_to_end() {
    let dir = std::env::temp_dir().join(format!("nika-first-hour-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");

    // 1 · the adoption gesture — the showroom file becomes yours.
    let copy = bin()
        .args(["compile", "01-hello", "01-hello.nika"])
        .current_dir(&dir)
        .output()
        .expect("copy runs");
    assert_eq!(copy.status.code(), Some(0), "copy is green");
    assert!(
        dir.join("01-hello.nika").is_file(),
        "the file is yours"
    );

    // 2 · the bare lazy door finds the only workflow and says so.
    let run = bin()
        .args(["run", "--model", "mock/echo", "--quiet", "--no-trace-file"])
        .current_dir(&dir)
        .output()
        .expect("bare run");
    assert_eq!(run.status.code(), Some(0), "the lazy run is green");
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains("the only workflow here"),
        "the announce names the pick: {err}"
    );

    // 3 · `new <example slug>` = the same source, the other handle.
    let new = bin()
        .args(["compile", "01-hello", "twin.nika"])
        .current_dir(&dir)
        .output()
        .expect("new runs");
    assert_eq!(new.status.code(), Some(0), "new-from-example is green");
    // One resolution, two handles — the SAME example, and each copy names
    // ITSELF. Byte-identity was the old assertion and it pinned a bug: the
    // copy landing as `twin.nika` still taught `nika run
    // 01-hello.nika`, a command that fails in the reader's own
    // directory. The self-reference follows the destination now, so the
    // two differ in exactly that way and no other.
    let twin = std::fs::read_to_string(dir.join("twin.nika")).expect("written");
    let orig = std::fs::read_to_string(dir.join("01-hello.nika")).expect("copied");
    let mut twin_yaml: serde_json::Value = serde_yaml_bw::from_str(&twin).expect("twin YAML");
    let orig_yaml: serde_json::Value = serde_yaml_bw::from_str(&orig).expect("original YAML");
    assert_eq!(twin_yaml["nika"].as_str(), Some("twin"));
    twin_yaml["nika"] = orig_yaml["nika"].clone();
    assert_eq!(
        twin_yaml, orig_yaml,
        "only the explicit destination identity differs"
    );
    assert!(String::from_utf8_lossy(&new.stdout).contains("nika run twin.nika"));

    // 4 · found a second repo around an example, scriptably.
    let home = dir.join("founded");
    std::fs::create_dir_all(&home).expect("mkdir");
    let init = bin()
        .args(["init", ".", "--example", "01-hello"])
        .current_dir(&home)
        .output()
        .expect("init runs");
    assert_eq!(init.status.code(), Some(0), "init --example is green");
    let out = String::from_utf8_lossy(&init.stdout);
    assert!(
        out.contains("created workflows/01-hello.nika"),
        "the lesson founds the repo: {out}"
    );
    assert!(out.contains("audited"), "the proof ladder ran: {out}");
    assert!(home.join("AGENTS.md").is_file(), "briefs landed");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_recipe_scaffolds_the_curriculum_and_audits_it() {
    let dir = workspace_tmp_dir("nika-init-recipe-smoke");
    let out = bin()
        .arg("init")
        .arg(&dir)
        .arg("--yes")
        .arg("--recipe")
        .arg("agentic")
        .arg("--theme")
        .arg("nika")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "founding succeeds");
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    // The 4-pattern curriculum lands on disk…
    for rel in [
        "workflows/01-hello-chain.nika",
        "workflows/02-parallel-fanout.nika",
        "workflows/03-gated-ship.nika",
        "workflows/04-agent-loop.nika",
    ] {
        assert!(dir.join(rel).is_file(), "{rel} written: {stdout}");
    }
    // …each one audited on the spot. Complete members are admitted;
    // slot-only members are named as honest drafts (and remain a refusal
    // at the check/run doors)…
    assert!(
        stdout.matches("audited").count() + stdout.matches("not a workflow yet").count() >= 4,
        "every workflow or draft was audited: {stdout}"
    );
    assert!(
        stdout.contains("not a workflow yet"),
        "drafts stay named: {stdout}"
    );
    // …the canvas theme is a REAL stamp in the settings JSON…
    let settings =
        std::fs::read_to_string(dir.join(".vscode/settings.json")).expect("settings written");
    let parsed: serde_json::Value = serde_json::from_str(&settings).expect("valid json");
    assert_eq!(
        parsed.get("nika.dag.theme").and_then(|v| v.as_str()),
        Some("nika"),
        "the DAG skin persisted"
    );
    // …and the hand-off names the FIRST scaffolded workflow.
    assert!(
        stdout.contains("$EDITOR workflows/01-hello-chain.nika")
            && stdout.contains("nika check workflows/01-hello-chain.nika")
            && stdout.contains("nika run workflows/01-hello-chain.nika --model mock/echo"),
        "the next block teaches edit → check → run: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_plain_yes_keeps_file_receipts_and_the_handoff() {
    // `--yes` with ZERO new flags renders the report rows, lays the
    // project file and the hello lesson (#1283) and hands over to THAT
    // file; `--recipe minimal` keeps the classic pre-wizard block scripts
    // have parsed since #158.
    let dir = workspace_tmp_dir("nika-init-stable-smoke");
    let out = bin()
        .arg("init")
        .arg(&dir)
        .arg("--yes")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains("✔ created"), "{stdout}");
    assert!(
        stdout.contains("nika.yaml — team defaults") && dir.join("nika.yaml").is_file(),
        "the project file is laid by default, with its why: {stdout}"
    );
    assert!(
        stdout.contains("created workflows/01-hello.nika — ")
            && stdout.contains("nika run workflows/01-hello.nika --model mock/echo"),
        "the hand-off names the founded lesson: {stdout}"
    );
    assert!(!stdout.contains('\x1b'), "piped init stays escape-free");
    let minimal = bin()
        .arg("init")
        .arg(&dir)
        .args(["--yes", "--recipe", "minimal"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    assert_eq!(minimal.status.code(), Some(0));
    let stdout = String::from_utf8(minimal.stdout).expect("utf8");
    assert!(
        stdout.contains("nika try 01-hello") && !stdout.contains("workflows/"),
        "minimal keeps the classic hand-off and scaffolds no workflow: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The lazy-hands resolver: `check`/`run` with NO file — one workflow
/// auto-resolves (announced on stderr, stdout contract untouched),
/// zero routes to the founding trio, several lists copy-paste lines.
#[test]
fn bare_check_and_run_resolve_the_lazy_way() {
    let base = workspace_tmp_dir("nika-lazy-smoke");
    let hello = "nika: solo\nmodel: mock/echo\ntasks:\n  greet:\n    infer: { prompt: \"hi\", max_tokens: 9 }\n";

    // ONE workflow → check runs it and says which (stderr).
    let one = base.join("one");
    std::fs::create_dir_all(&one).expect("mkdir");
    std::fs::write(one.join("solo.nika"), hello).expect("seed");
    let out = bin()
        .arg("check")
        .current_dir(&one)
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(0), "auto-resolved audit passes");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("solo.nika (the only workflow here)"),
        "the pick is announced: {err}"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("audited"),
        "stdout carries the audit only"
    );

    // ZERO → the founding trio, env exit.
    let none = base.join("none");
    std::fs::create_dir_all(&none).expect("mkdir");
    let out = bin()
        .arg("check")
        .current_dir(&none)
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    // The founding door is `nika compile hello` — `init` left the first-run
    // path when the first-wow cascade landed (a stranger writes ONE file,
    // never founds a repo). This gate follows the door it teaches.
    assert!(
        err.contains("nika compile hello"),
        "routes to founding: {err}"
    );

    // MANY → every candidate named, copy-paste ready.
    let many = base.join("many");
    std::fs::create_dir_all(&many).expect("mkdir");
    std::fs::write(many.join("a.nika"), hello).expect("seed");
    std::fs::write(many.join("b.nika"), hello).expect("seed");
    let out = bin()
        .arg("run")
        .current_dir(&many)
        .output()
        .expect("binary runs");
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("nika run a.nika") && err.contains("nika run b.nika"),
        "each candidate is a paste-ready command: {err}"
    );
    let _ = std::fs::remove_dir_all(&base);
}
