// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! Native #1683: real CLI → production runtime → outputs and boot provenance.
//! Hermetic pure jq workflows, no provider keys or process-global mutations.

use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Output, Stdio};

const WF: &str = r"nika: literal-inputs
inputs:
  text: {type: string, required: true}
  count: {type: integer, required: true}
  data: {type: {array: {object: {name: string, enabled: bool}}}, required: true}
  record: {type: {object: {url: string, weights: {array: number}}}, required: true}
  nothing: {type: null, required: true}
  region: {type: string, default: eu}
  optional: {type: string}
permits: {tools: ['nika:jq']}
tasks:
  echo:
    invoke:
      tool: nika:jq
      args:
        input:
          text: '${{ inputs.text }}'
          count: '${{ inputs.count }}'
          data: '${{ inputs.data }}'
          record: '${{ inputs.record }}'
          nothing: '${{ inputs.nothing }}'
          region: '${{ inputs.region }}'
        expression: '.'
outputs:
  value: '${{ tasks.echo.output }}'
";

fn command(dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
    cmd.current_dir(dir)
        .env_clear()
        .env("HOME", dir)
        .env("NIKA_KEYCHAIN", "off")
        .env("TERM", "dumb")
        // A synthetic canary; literal inputs must never consult it.
        .env("NIKA_TEST_LITERAL", "must-not-be-read")
        .env("CI", "true");
    cmd
}

fn execute(source: &str, args: &[&str], bytes: &[u8]) -> (tempfile::TempDir, Output) {
    let dir = tempfile::tempdir().expect("room");
    std::fs::write(dir.path().join("case.nika"), source).expect("source");
    let mut child = command(dir.path())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("nika");
    let mut stdin = child.stdin.take().expect("stdin");
    // Early refusal is allowed to close stdin. The SDK owns cancellation and
    // backpressure; its writer must handle this same BrokenPipe gracefully.
    if let Err(error) = stdin.write_all(bytes) {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    drop(stdin);
    let output = child.wait_with_output().expect("completed child");
    (dir, output)
}

fn literal(source: &str, bytes: &[u8], mode: &[&str]) -> (tempfile::TempDir, Output) {
    let mut args = vec![
        "run",
        "case.nika",
        "--inputs-json",
        "-",
        "--no-gc",
        "--color",
        "never",
    ];
    args.extend_from_slice(mode);
    execute(source, &args, bytes)
}

fn output_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("JSON {error}: {output:?}"))
}

fn origins(dir: &std::path::Path) -> Value {
    let traces = std::fs::read_dir(dir.join(".nika/traces")).expect("trace directory");
    for entry in traces {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|ext| ext != "ndjson") {
            continue;
        }
        let text = std::fs::read_to_string(path).expect("journal");
        for line in text.lines() {
            let row: Value = serde_json::from_str(line).expect("event");
            if row["kind"] != "workflow_started" {
                continue;
            }
            let field = row["fields"]
                .as_array()
                .expect("fields")
                .iter()
                .find(|field| field["key"] == "inputs")
                .expect("origins");
            return serde_json::from_str(field["value"].as_str().expect("encoded origins"))
                .expect("origins JSON");
        }
    }
    panic!("no workflow_started");
}

fn values() -> Value {
    json!({"text":"@env:NIKA_TEST_LITERAL ${{ inputs.region }} café 🦋 https://例.example/a?q=x%2Fy&x=2#frag", "count":42,
        "data":[{"name":"42", "enabled":true}, {"name":"雪", "enabled":false}], "record":{"url":"https://例.example/a?q=x%2Fy&x=2#雪", "weights":[1, 2.5]}, "nothing":null})
}

fn refuse(bytes: &[u8], code: &str) {
    for mode in [&["--json"][..], &["--output", "json"][..]] {
        let (dir, out) = literal(WF, bytes, mode);
        assert_eq!(out.status.code(), Some(3), "{out:?}");
        let frame = output_json(&out);
        assert_eq!(frame["error"]["code"], code, "{frame}");
        assert!(frame.get("kind").is_none(), "no admitted event");
        assert!(frame.get("receipt").is_none(), "no execution receipt");
        assert!(
            !dir.path().join(".nika/traces").exists(),
            "no admitted trace"
        );
    }
}

#[test]
fn literal_json_reaches_runtime_and_origin_manifest_without_interpretation() {
    let value = values();
    let bytes = serde_json::to_vec(&value).expect("JSON");
    let (dir, out) = literal(WF, &bytes, &["--output", "json"]);
    assert!(out.status.success(), "{out:?}");
    let mut expected = value;
    expected["region"] = json!("eu");
    assert_eq!(output_json(&out), json!({"value":expected}));
    assert_eq!(
        origins(dir.path()),
        json!({"text":"api-caller", "count":"api-caller", "data":"api-caller", "record":"api-caller", "nothing":"api-caller", "region":"file"})
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("case.nika")).expect("source"),
        WF
    );
}

#[test]
fn unknown_missing_and_wrong_type_values_refuse_before_any_event() {
    refuse(b"{}", "NIKA-1708");
    for (name, value, code) in [
        ("text", json!(42), "input_type_mismatch"),
        ("count", json!("42"), "input_type_mismatch"),
        ("data", json!({}), "input_type_mismatch"),
        ("text", Value::Null, "input_type_mismatch"),
        ("extra", json!(true), "unknown_input"),
    ] {
        let mut payload = values();
        payload[name] = value;
        refuse(&serde_json::to_vec(&payload).expect("JSON"), code);
    }
}

#[test]
fn empty_object_and_absent_channel_keep_defaults_and_optional_absence() {
    let source = "nika: defaults\ninputs:\n  region: {type: string, default: eu}\n  absent: {type: string}\npermits: {tools: ['nika:jq']}\ntasks:\n  echo:\n    invoke:\n      tool: nika:jq\n      args: {input: '${{ inputs.region }}', expression: '.'}\noutputs:\n  value: '${{ tasks.echo.output }}'\n";
    for flag in [vec!["--inputs-json", "-"], vec![]] {
        let mut args = vec!["run", "case.nika", "--output", "json", "--no-gc"];
        args.extend(flag);
        let (dir, out) = execute(source, &args, b"{}");
        assert!(out.status.success(), "{out:?}");
        assert_eq!(output_json(&out), json!({"value":"eu"}));
        assert_eq!(origins(dir.path()), json!({"region":"file"}));
    }
}

#[test]
fn malformed_duplicate_and_nonobject_payloads_have_machine_refusals() {
    for bytes in [b"null".as_slice(), b"[]", b"42", b"true", br#""str""#] {
        refuse(bytes, "invalid_inputs_root");
    }
    for bytes in [
        b"{".as_slice(),
        b"",
        b"{}{}",
        br#"{"text":"a","text":"b"}"#,
        br#"{"data":[{"x":1,"x":2}]}"#,
    ] {
        refuse(bytes, "invalid_inputs_json");
    }
    refuse(b"{\"x\":\"\xff\"}", "invalid_inputs_utf8");
    refuse(&vec![b' '; 1024 * 1024 + 1], "inputs_too_large");
}

#[test]
fn exactly_one_mib_is_accepted() {
    let mut bytes = serde_json::to_vec(&values()).expect("JSON");
    bytes.resize(1024 * 1024, b' ');
    let (_, out) = literal(WF, &bytes, &["--output", "json"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(output_json(&out)["value"]["count"], 42);
}

#[test]
fn channel_conflicts_and_inline_forms_refuse_without_consuming_stdin() {
    for args in [
        vec![
            "run",
            "case.nika",
            "--inputs-json",
            "-",
            "--var",
            "text=operator",
            "--json",
        ],
        vec!["run", "-", "--inputs-json", "-", "--json"],
        vec!["run", "case.nika", "--inputs-json", "{}", "--json"],
    ] {
        let code = if args.contains(&"{}") {
            "invalid_inputs_channel"
        } else {
            "input_channel_conflict"
        };
        // Keep the pipe OPEN: conflict must be decided without waiting for EOF.
        let dir = tempfile::tempdir().expect("room");
        std::fs::write(dir.path().join("case.nika"), WF).expect("fixture");
        let mut child = command(dir.path())
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");
        let stdin = child.stdin.take().expect("open pipe");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if child.try_wait().expect("poll").is_some() {
                break;
            }
            if std::time::Instant::now() > deadline {
                child.kill().expect("kill stuck reader");
                panic!("conflict blocked on stdin");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        drop(stdin);
        let out = child.wait_with_output().expect("wait");
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(output_json(&out)["error"]["code"], code);
        assert!(!dir.path().join(".nika/traces").exists());
    }
}

#[test]
fn operator_vars_still_resolve_explicit_environment_and_coerce_declared_types() {
    let source = WF.replace(
        "permits: {tools:",
        "permits: {env: [NIKA_TEST_LITERAL], tools:",
    );
    let args = [
        "run",
        "case.nika",
        "--no-gc",
        "--output",
        "json",
        "--var",
        "text=@env:NIKA_TEST_LITERAL",
        "--var",
        "count=42",
        "--var",
        "data=[]",
        "--var",
        "nothing=null",
        "--var",
        "record={\"url\":\"literal\",\"weights\":[]}",
    ];
    let (dir, out) = execute(&source, &args, b"");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(output_json(&out)["value"]["text"], "must-not-be-read");
    assert_eq!(output_json(&out)["value"]["count"], 42);
    assert_eq!(origins(dir.path())["text"], "env");
    assert_eq!(origins(dir.path())["count"], "ci-context");
}

#[test]
fn capability_help_and_source_only_check_match_the_channel() {
    let dir = tempfile::tempdir().expect("room");
    let identity = command(dir.path())
        .arg("--sdk-identity")
        .output()
        .expect("identity");
    assert!(identity.status.success());
    assert!(
        output_json(&identity)["supportedCapabilities"]
            .as_array()
            .expect("capabilities")
            .contains(&json!("inputsLiteral"))
    );
    let help = command(dir.path())
        .args(["run", "--help"])
        .output()
        .expect("help");
    assert!(help.status.success());
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("--inputs-json") && help.contains("1 MiB"));
    let (_, check) = execute(
        WF,
        &["check", "case.nika", "--json"],
        b"not input JSON",
    );
    assert!(
        check.status.success(),
        "check never requires runtime values: {check:?}"
    );
    let (_, unrelated) = execute(WF, &["test", "--help"], b"");
    assert!(!String::from_utf8_lossy(&unrelated.stdout).contains("--inputs-json"));
    let (_, check) = execute(
        WF,
        &["check", "case.nika", "--inputs-json", "-", "--json"],
        b"{}",
    );
    assert!(
        !check.status.success(),
        "Check does not accept launch bindings"
    );
}

#[test]
fn literal_inputs_resume_through_the_same_binding_without_an_argv_payload() {
    let source = WF.replace("['nika:jq']", "['nika:jq', 'nika:prompt']")
        .replace("tasks:\n  echo:", "tasks:\n  approve:\n    invoke:\n      tool: nika:prompt\n      args: {mode: input, message: 'continue?'}\n  echo:\n    after: {approve: success}");
    let bytes = serde_json::to_vec(&values()).expect("JSON");
    let (dir, paused) = literal(&source, &bytes, &["--output", "json"]);
    assert_eq!(paused.status.code(), Some(4), "{paused:?}");
    let pause = output_json(&paused);
    assert_eq!(pause["paused"]["resume_carry"], " --inputs-json -");
    let trace = std::fs::read_dir(dir.path().join(".nika/traces"))
        .expect("traces")
        .map(|entry| entry.expect("entry").path())
        .find(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
        .expect("trace");
    let mut resumed = command(dir.path())
        .args([
            "run",
            "case.nika",
            "--inputs-json",
            "-",
            "--no-gc",
            "--output",
            "json",
            "--answer",
            "approve=yes",
            "--resume",
        ])
        .arg(trace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("resume");
    resumed
        .stdin
        .take()
        .expect("pipe")
        .write_all(&bytes)
        .expect("inputs");
    let out = resumed.wait_with_output().expect("resumed");
    assert!(out.status.success(), "{out:?}");
    let mut expected = values();
    expected["region"] = json!("eu");
    assert_eq!(output_json(&out), json!({"value":expected}));
}

/// The journal's own workflow is the resume's source gate (#1586). A valid
/// literal map offered to ANOTHER workflow's paused journal grants nothing:
/// one machine refusal, no event, no new journal, the recorded one untouched.
#[test]
fn a_foreign_journal_refuses_the_literal_resume_before_any_event() {
    let gated = WF.replace("['nika:jq']", "['nika:jq', 'nika:prompt']")
        .replace("tasks:\n  echo:", "tasks:\n  approve:\n    invoke:\n      tool: nika:prompt\n      args: {mode: input, message: 'continue?'}\n  echo:\n    after: {approve: success}");
    let bytes = serde_json::to_vec(&values()).expect("JSON");
    let (dir, paused) = literal(&gated, &bytes, &["--output", "json"]);
    assert_eq!(paused.status.code(), Some(4), "{paused:?}");
    let journals = || -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir.path().join(".nika/traces"))
            .expect("traces")
            .map(|entry| entry.expect("entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "ndjson"))
            .collect()
    };
    let trace = journals().pop().expect("the paused journal");
    let recorded = std::fs::read(&trace).expect("journal bytes");
    std::fs::write(
        dir.path().join("other.nika"),
        gated.replace("nika: literal-inputs", "nika: another-workflow"),
    )
    .expect("foreign workflow");
    for mode in [&["--json"][..], &["--output", "json"][..]] {
        let mut child = command(dir.path())
            .args(["run", "other.nika", "--inputs-json", "-", "--no-gc"])
            .args(mode)
            .args(["--answer", "approve=yes", "--resume"])
            .arg(&trace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("resume");
        let mut stdin = child.stdin.take().expect("pipe");
        if let Err(error) = stdin.write_all(&bytes) {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        }
        drop(stdin);
        let out = child.wait_with_output().expect("refused");
        assert_eq!(out.status.code(), Some(3), "{out:?}");
        let frame = output_json(&out);
        let message = frame["error"]["message"].as_str().expect("message");
        assert!(
            message.contains("`literal-inputs`") && message.contains("`another-workflow`"),
            "{message}"
        );
        assert!(frame.get("kind").is_none(), "no admitted event");
        assert_eq!(journals(), vec![trace.clone()], "no new journal");
        assert_eq!(std::fs::read(&trace).expect("journal"), recorded);
    }
}
