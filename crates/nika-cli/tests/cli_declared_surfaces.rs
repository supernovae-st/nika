// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// These probes must cross argv and stdio of the shipped executable; the
// kernel spawn seam would exercise a library instead of the CLI dispatcher.
#![allow(clippy::disallowed_types)]

//! Behavioral witnesses for the six previously unprobed wiring rows.
//! Every child receives an empty environment, disposable HOME and disabled
//! keychain. Keys are minted only in that scratch HOME, never imported from
//! the operator. No provider, network service or workflow effect is invoked.
//! DAP coverage is protocol admission/refusal, not replay correctness; shell
//! execution coverage is Bash, with generation checks for the other shells.

use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

fn isolated(executable: impl AsRef<std::ffi::OsStr>, home: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .env_clear()
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_PASSWORD", "")
        .env("NO_COLOR", "1")
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn bin(home: &Path) -> Command {
    isolated(env!("CARGO_BIN_EXE_nika"), home)
}

// Drain both pipes while polling: catalog/schema output exceeds pipe capacity.
// A stuck protocol peer must fail this test, not hang the entire test suite.
fn collect(mut child: Child) -> Output {
    std::thread::scope(|scope| {
        let mut stdout = child.stdout.take().expect("stdout pipe");
        let mut stderr = child.stderr.take().expect("stderr pipe");
        let out_reader = scope.spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).expect("read stdout");
            bytes
        });
        let err_reader = scope.spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).expect("read stderr");
            bytes
        });
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll child") {
                break status;
            }
            if Instant::now() >= deadline {
                timed_out = true;
                child.kill().expect("kill stalled child");
                break child.wait().expect("reap stalled child");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let output = Output {
            status,
            stdout: out_reader.join().expect("stdout reader"),
            stderr: err_reader.join().expect("stderr reader"),
        };
        assert!(!timed_out, "CLI child did not terminate within 30 seconds");
        output
    })
}

fn run(home: &Path, args: &[&str]) -> Output {
    collect(bin(home).args(args).spawn().expect("spawn nika"))
}

fn expect_code(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if code == 0 {
        assert!(output.stderr.is_empty(), "success must leave stderr empty");
    } else if code == 3 {
        assert!(
            output.stdout.is_empty(),
            "environment failure must leave stdout empty"
        );
    }
    // Exit 2 has two distinct surfaces: clap usage errors use stderr;
    // FILE findings use stdout because the finding itself is the product.
}

fn expect_finding(output: &Output, message: &str) {
    expect_code(output, 2);
    assert!(output.stderr.is_empty(), "FILE findings belong on stdout");
    assert!(text(output).contains(message), "missing finding: {message}");
}

fn text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("UTF-8 stdout")
}

fn machine(home: &Path, args: &[&str]) -> Value {
    let output = run(home, args);
    expect_code(&output, 0);
    serde_json::from_slice(&output.stdout).expect("one complete JSON document")
}

#[test]
fn catalog_dispatches_provider_and_builtin_projections() {
    let home = tempfile::tempdir().expect("scratch HOME");
    let catalog = machine(home.path(), &["catalog", "--json"]);
    assert_eq!(catalog["catalog_version"], 1);
    let providers = catalog["providers"].as_array().expect("providers");
    let local = providers
        .iter()
        .find(|provider| provider["id"] == "ollama")
        .expect("local provider is present");
    assert_eq!(local["local"], true);
    assert_eq!(local["requires_key"], false);
    assert_eq!(local["resolves"], true);
    assert!(
        providers
            .iter()
            .any(|provider| provider["resolves"] == false)
    );

    let tools = machine(home.path(), &["catalog", "--tools", "--json"]);
    assert_eq!(tools["tools_version"], 1);
    assert!(tools.get("providers").is_none());
    let jq = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "nika:jq")
        .expect("jq builtin");
    assert_eq!(jq["parameters"]["type"], "object");
    assert!(jq["parameters"]["properties"].is_object());

    let human = run(home.path(), &["catalog", "--tools"]);
    expect_code(&human, 0);
    assert!(text(&human).contains("nika:jq"));
    expect_code(&run(home.path(), &["catalog", "--unknown-projection"]), 2);
}

#[test]
fn spec_dispatches_identity_canon_and_distinct_schemas() {
    let home = tempfile::tempdir().expect("scratch HOME");
    let identity = run(home.path(), &["spec"]);
    expect_code(&identity, 0);
    assert!(text(&identity).starts_with("nika language pack "));
    assert!(text(&identity).contains("embedded"));

    let canon = run(home.path(), &["spec", "--canon"]);
    expect_code(&canon, 0);
    let canon: serde_yaml_bw::Value = serde_yaml_bw::from_slice(&canon.stdout).expect("canon YAML");
    assert_eq!(canon["schema_version"].as_u64(), Some(1));
    assert_eq!(canon["counts"]["verbs"].as_u64(), Some(4));

    let workflow = machine(home.path(), &["spec", "--schema"]);
    let project = machine(home.path(), &["spec", "--schema", "--project"]);
    assert!(workflow["properties"]["tasks"].is_object());
    assert!(workflow["properties"]["nika"].is_object());
    assert!(project["properties"]["tasks"].is_null());
    assert!(project["properties"].is_object());
    assert_ne!(workflow, project, "--project must select its own schema");
    expect_code(&run(home.path(), &["spec", "--schema", "--canon"]), 2);
    expect_code(&run(home.path(), &["spec", "--project"]), 2);
}

#[test]
fn completions_generate_each_shell_and_refuse_unknown_shells() {
    let home = tempfile::tempdir().expect("scratch HOME");
    for (shell, marker) in [
        ("bash", "complete -F"),
        ("zsh", "compdef"),
        ("fish", "complete -c nika"),
        ("elvish", "arg-completer"),
        ("powershell", "Register-ArgumentCompleter"),
    ] {
        let output = run(home.path(), &["completions", shell]);
        expect_code(&output, 0);
        let script = text(&output);
        assert!(script.contains(marker), "{shell} lacks shell registration");
        assert!(script.contains("nika"));
        assert!(script.contains("check"));
    }
    expect_code(&run(home.path(), &["completions", "unknown-shell"]), 2);
}

#[cfg(unix)]
#[test]
fn bash_completion_script_executes_and_completes_a_real_verb() {
    let home = tempfile::tempdir().expect("scratch HOME");
    let output = run(home.path(), &["completions", "bash"]);
    expect_code(&output, 0);
    let script = home.path().join("nika.bash");
    std::fs::write(&script, output.stdout).expect("generated completion script");
    let output = collect(
        isolated("/bin/bash", home.path())
            .args(["--noprofile", "--norc", "-c"])
            .arg("source \"$1\"; COMP_WORDS=(nika ru); COMP_CWORD=1; _nika nika ru nika; printf '%s\\n' \"${COMPREPLY[@]}\"")
            .arg("nika-completion-probe")
            .arg(script)
            .spawn()
            .expect("spawn Bash"),
    );
    expect_code(&output, 0);
    assert_eq!(text(&output).trim(), "run");
}

#[test]
fn key_lifecycle_preserves_existing_custody_and_retires_public_keys() {
    let home = tempfile::tempdir().expect("scratch HOME");
    let absent = run(home.path(), &["key", "trust"]);
    expect_code(&absent, 0);
    assert!(text(&absent).contains("no run-signing key"));
    expect_finding(
        &run(home.path(), &["key", "rotate"]),
        "no run-signing key to rotate",
    );
    expect_code(&run(home.path(), &["key", "init"]), 0);
    let secret_path = home.path().join(".nika/keys/run-signing.key");
    let public_path = secret_path.with_extension("pub");
    let secret = std::fs::read(&secret_path).expect("scratch private key");
    let public = std::fs::read_to_string(&public_path).expect("scratch public key");
    let trusted = run(home.path(), &["key", "trust"]);
    expect_code(&trusted, 0);
    assert!(text(&trusted).contains(public.trim()));
    assert!(text(&trusted).contains("fingerprint"));
    expect_finding(
        &run(home.path(), &["key", "init"]),
        "run-signing key material already exists",
    );
    assert!(std::fs::read(&secret_path).expect("retained private key") == secret);
    assert_eq!(
        std::fs::read_to_string(&public_path).expect("retained public key"),
        public
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&secret_path)
                .expect("private metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    expect_code(&run(home.path(), &["key", "rotate"]), 0);
    let replacement = std::fs::read_to_string(&public_path).expect("rotated public key");
    assert_ne!(replacement, public);
    let retired = std::fs::read_to_string(home.path().join(".nika/keys/retired.pub"))
        .expect("retired ledger");
    assert!(retired.contains(public.trim()));
    let trusted = run(home.path(), &["key", "trust"]);
    expect_code(&trusted, 0);
    assert!(text(&trusted).contains(replacement.trim()));
}

#[test]
fn sign_verifies_exact_bytes_and_refuses_missing_custody_and_tampering() {
    let home = tempfile::tempdir().expect("scratch HOME");
    let workflow = home.path().join("signed.nika.yaml");
    let sidecar = home.path().join("signed.nika.yaml.minisig");
    let original = b"nika: signature-probe\ntasks: {}\n";
    std::fs::write(&workflow, original).expect("unsigned fixture");
    expect_code(&run(home.path(), &["sign", "signed.nika.yaml"]), 3);
    assert!(!sidecar.exists(), "no custody must not mint a sidecar");
    expect_code(
        &run(home.path(), &["sign", "signed.nika.yaml", "--check"]),
        3,
    );
    expect_code(&run(home.path(), &["key", "init"]), 0);
    expect_code(&run(home.path(), &["sign", "signed.nika.yaml"]), 0);
    assert!(sidecar.is_file());
    assert_eq!(std::fs::read(&workflow).expect("signed workflow"), original);
    let valid = run(home.path(), &["sign", "signed.nika.yaml", "--check"]);
    expect_code(&valid, 0);
    assert!(text(&valid).contains("valid signature"));
    std::fs::write(&workflow, b"nika: tampered\ntasks: {}\n").expect("tamper fixture");
    let invalid = run(home.path(), &["sign", "signed.nika.yaml", "--check"]);
    expect_finding(&invalid, "INVALID signature");
    std::fs::write(&workflow, original).expect("restore signed bytes");
    expect_code(
        &run(home.path(), &["sign", "signed.nika.yaml", "--check"]),
        0,
    );
    std::fs::write(sidecar, b"not a minisign sidecar\n").expect("corrupt signature");
    expect_finding(
        &run(home.path(), &["sign", "signed.nika.yaml", "--check"]),
        "INVALID signature",
    );
}

fn frame(value: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(value).expect("request JSON");
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend(body);
    bytes
}

fn frames(mut bytes: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    while !bytes.is_empty() {
        let end = bytes
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .expect("DAP header delimiter");
        let header = std::str::from_utf8(&bytes[..end]).expect("ASCII DAP header");
        let length: usize = header
            .strip_prefix("Content-Length: ")
            .expect("stdout is exclusively framed DAP")
            .parse()
            .expect("DAP byte length");
        bytes = &bytes[end + 4..];
        assert!(length <= bytes.len(), "truncated DAP body");
        messages.push(serde_json::from_slice(&bytes[..length]).expect("DAP JSON"));
        bytes = &bytes[length..];
    }
    messages
}

#[test]
fn dap_handshake_correlates_requests_and_refuses_invalid_session_actions() {
    let home = tempfile::tempdir().expect("scratch HOME");
    let mut child = bin(home.path())
        .arg("dap")
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn DAP");
    let mut input = frame(
        &json!({"seq": 7, "type": "request", "command": "initialize", "arguments": {"adapterID": "nika"}}),
    );
    // A client event has no command and must not kill the following requests.
    input.extend(frame(
        &json!({"seq": 8, "type": "event", "event": "ignored"}),
    ));
    for (seq, command) in [
        (9, "setBreakpoints"),
        (10, "launch"),
        (11, "unknown-command"),
        (12, "disconnect"),
    ] {
        input.extend(frame(
            &json!({"seq": seq, "type": "request", "command": command, "arguments": {}}),
        ));
    }
    child
        .stdin
        .take()
        .expect("DAP stdin")
        .write_all(&input)
        .expect("write DAP session");
    let output = collect(child);
    expect_code(&output, 0);
    let messages = frames(&output.stdout);
    assert_eq!(
        messages.len(),
        5,
        "failed launch must not emit initialized or stopped"
    );
    for (message, (seq, command, success)) in messages.iter().zip([
        (7, "initialize", true),
        (9, "setBreakpoints", false),
        (10, "launch", false),
        (11, "unknown-command", false),
        (12, "disconnect", true),
    ]) {
        assert_eq!(message["type"], "response");
        assert_eq!(message["request_seq"], seq);
        assert_eq!(message["command"], command);
        assert_eq!(message["success"], success);
    }
    assert_eq!(messages[0]["body"]["supportsStepBack"], true);
    assert_eq!(
        messages[0]["body"]["supportsConfigurationDoneRequest"],
        true
    );
    assert!(
        messages[1]["message"]
            .as_str()
            .expect("breakpoint refusal")
            .contains("before launch")
    );
    assert!(
        messages[2]["message"]
            .as_str()
            .expect("launch refusal")
            .contains("workflow")
    );
    let sequence: Vec<_> = messages
        .iter()
        .map(|message| message["seq"].as_i64().expect("outgoing sequence"))
        .collect();
    assert!(sequence.windows(2).all(|pair| pair[1] > pair[0]));
}
