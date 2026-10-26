// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used)]
// This suite executes the real `nika` binary (CARGO_BIN_EXE): spawning it is
// its job (the resume_e2e.rs carve-out class).
#![allow(clippy::disallowed_types)]

//! NIK-271 at the binary plane: `nika trace verify` weighs the run key a run
//! sealed with, in the order the signer takes it. A run sealed with an explicit
//! `NIKA_RUN_KEY_FILE` · `NIKA_RUN_PUB_FILE` pair kept outside `~/.nika/keys`
//! verifies SEALED under that pair with or without `--key`, `--key` first; a
//! host that names no pair and holds no key still cannot attribute it (exit 3,
//! never a forgery); the default custody under `~/.nika/keys` verifies as
//! before. The key pairs are minted by `nika key init` itself.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::{Value, json};

/// A rehearsal run: `mock/echo` needs no key and no network.
const WORKFLOW: &str = "nika: hello\nmodel: mock/echo\npermits: {}\ntasks:\n  greeting:\n    infer:\n      prompt: \"Say hello.\"\n      max_tokens: 32\noutputs:\n  greeting: ${{ tasks.greeting.output }}\n";

/// `nika <args>` in `room` under its own home, the keychain off, naming the
/// explicit pair in `pair` when one is given.
fn nika(room: &Path, pair: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
    command
        .args(args)
        .current_dir(room)
        .stdin(Stdio::null())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room.join(".home"))
        .env("NIKA_KEYCHAIN", "off");
    if let Some(keys) = pair {
        command
            .env("NIKA_RUN_KEY_FILE", keys.join("run.key"))
            .env("NIKA_RUN_PUB_FILE", keys.join("run.pub"));
    }
    command.output().expect("the binary runs")
}

fn said(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// A room with its own empty home and the rehearsal workflow.
fn room() -> tempfile::TempDir {
    let room = tempfile::tempdir().expect("isolated room");
    std::fs::create_dir_all(room.path().join(".home")).expect("isolated home");
    std::fs::write(room.path().join("hello.nika"), WORKFLOW).expect("workflow");
    room
}

/// `nika key init` under `pair`: the fingerprint it minted.
fn mint(room: &Path, pair: Option<&Path>) -> String {
    let minted = nika(room, pair, &["key", "init"]);
    assert_eq!(minted.status.code(), Some(0), "{}", said(&minted));
    let stdout = String::from_utf8_lossy(&minted.stdout);
    let (_, rest) = stdout
        .split_once("fingerprint ")
        .expect("key init names its fingerprint");
    let key_id: String = rest.chars().take(16).collect();
    assert!(
        key_id.len() == 16 && key_id.bytes().all(|b| b.is_ascii_hexdigit()),
        "{stdout}"
    );
    key_id
}

/// Run the rehearsal under `pair`: the one journal it wrote and sealed.
fn sealed_run(room: &Path, pair: Option<&Path>) -> PathBuf {
    let run = nika(room, pair, &["run", "hello.nika", "--color", "never"]);
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let traces = room.join(".nika/traces");
    let mut found: Vec<PathBuf> = (std::fs::read_dir(&traces).expect("traces"))
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    let journal = found.remove(0);
    let raw = std::fs::read_to_string(&journal).expect("journal");
    let last: Value =
        serde_json::from_str(raw.lines().last().expect("a line")).expect("a JSON line");
    assert_eq!(last["kind"], "run_sealed", "the run sealed its journal");
    journal
}

/// `nika trace verify <journal> --json <extra>` under `pair`: the exit code and
/// the document (on stderr behind the `nika: ` prefix when the exit is not 0).
fn verify(room: &Path, pair: Option<&Path>, journal: &Path, extra: &[&str]) -> (i32, Value) {
    let journal = journal.to_string_lossy().into_owned();
    let mut args = vec!["trace", "verify", journal.as_str(), "--json"];
    args.extend_from_slice(extra);
    let output = nika(room, pair, &args);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stdout.trim().is_empty() {
        stderr.trim().trim_start_matches("nika: ")
    } else {
        stdout.trim()
    };
    let document = serde_json::from_str(text).unwrap_or_else(|_| json!({ "said": said(&output) }));
    (output.status.code().expect("an exit code"), document)
}

#[test]
fn a_run_sealed_with_an_explicit_pair_outside_the_home_verifies_under_that_pair() {
    let room = room();
    let room = room.path();
    let keys = room.join("etc-nika");
    std::fs::create_dir_all(&keys).expect("a key directory outside ~/.nika/keys");
    let key_id = mint(room, Some(&keys));
    assert!(keys.join("run.key").is_file() && keys.join("run.pub").is_file());
    assert!(
        !room.join(".home/.nika/keys").exists(),
        "the pair lives outside ~/.nika/keys"
    );
    let journal = sealed_run(room, Some(&keys));
    let public = keys.join("run.pub").to_string_lossy().into_owned();

    // The pair the run sealed with is weighed without being named again.
    let (code, document) = verify(room, Some(&keys), &journal, &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["tier"], "sealed", "{document}");
    assert_eq!(
        document["seal"],
        json!({"tier": "sealed", "key_id": key_id, "source": "NIKA_RUN_PUB_FILE"}),
    );

    // `--key` is weighed first: the same key, attributed to the file named.
    let (code, document) = verify(room, Some(&keys), &journal, &["--key", &public]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(
        document["seal"],
        json!({"tier": "sealed", "key_id": key_id, "source": public}),
    );

    // A host that names no pair and holds no key cannot attribute the seal:
    // a missing input (exit 3), never a forgery.
    let (code, document) = verify(room, None, &journal, &[]);
    assert_eq!(code, 3, "{document}");
    assert_eq!(document["seal"]["tier"], "unattributable", "{document}");
    let reason = document["seal"]["reason"].as_str().unwrap_or_default();
    assert!(
        reason.contains(&key_id) && reason.contains("NIKA_RUN_PUB_FILE"),
        "the refusal names the key and where a verifier looks: {reason}"
    );

    // ... unless `--key` names its public half.
    let (code, document) = verify(room, None, &journal, &["--key", &public]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(
        document["seal"],
        json!({"tier": "sealed", "key_id": key_id, "source": public}),
    );
}

#[test]
fn a_run_sealed_with_the_default_custody_verifies_as_before() {
    let room = room();
    let room = room.path();
    let key_id = mint(room, None);
    let custody = room.join(".home/.nika/keys");
    assert!(custody.join("run-signing.key").is_file() && custody.join("run-signing.pub").is_file());
    let journal = sealed_run(room, None);
    let (code, document) = verify(room, None, &journal, &[]);
    assert_eq!(code, 0, "{document}");
    assert_eq!(document["tier"], "sealed", "{document}");
    assert_eq!(
        document["seal"],
        json!({"tier": "sealed", "key_id": key_id, "source": "~/.nika/keys/run-signing.pub"}),
    );
}
