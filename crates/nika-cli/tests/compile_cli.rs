// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types, clippy::panic)]

//! The real CLI adapter: one core, explicit writes, honest incomplete, no ambient policy.
use nika_onboard::compile::{CompileRequest, CompileStatus, compile};
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn command(room: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room)
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .current_dir(room)
        .stdin(Stdio::null());
    cmd
}

fn call(room: &Path, args: &[&str]) -> Output {
    command(room).args(args).output().expect("CLI")
}

fn result(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[test]
fn preview_is_the_same_typed_core_and_questions_are_stable() {
    let room = tempfile::tempdir().expect("room");
    let request = CompileRequest::create("classify-and-route");
    let core = compile(&request).expect("core");
    let first = call(room.path(), &["compile", "classify-and-route", "--json"]);
    let second = call(room.path(), &["compile", "classify-and-route", "--json"]);
    assert_eq!(first.status.code(), Some(2));
    assert_eq!(first.stdout, second.stdout);
    let got = result(&first);
    assert_eq!(got["candidate"], core.candidate.expect("candidate"));
    assert_eq!(got["questions"][0]["key"], "const.request");
    assert_eq!(got["check_preview"]["scope"], "sourceOnly");
    assert_eq!(got["provenance"]["cognition"], "deterministicOnly");
    assert!(got["written"].is_null());
    assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
}

#[test]
fn cold_authoring_is_bounded_and_ambient_credentials_do_not_opt_in() {
    let room = tempfile::tempdir().expect("room");
    // The strict reader admits "review … and prepare a support reply" as validate + draft
    // (a correct reading that asks only for a model); this test needs a clause the reader
    // cannot consume, so the explicit provider is the one that answers.
    let intent = "Review this customer request and harmonise the tone of the support reply";
    let automatic = command(room.path())
        .env("OPENAI_API_KEY", "not-a-real-key")
        .args(["compile", intent, "--json"])
        .output()
        .expect("CLI");
    assert_eq!(result(&automatic)["compile_version"], 1);
    assert_eq!(
        result(&automatic)["provenance"]["cognition"],
        "deterministicOnly"
    );
    let explicit = call(
        room.path(),
        &[
            "compile",
            intent,
            "--authoring-model",
            "mock/echo",
            "--authoring-strategy",
            "off",
            "--authoring-max-tokens",
            "1024",
            "--authoring-timeout",
            "2",
            "--json",
        ],
    );
    let document = result(&explicit);
    assert_eq!(document["compile_version"], 2, "{document}");
    // One proposal call, plus at most ONE bounded repair call when the mock's evidence is
    // not an excerpt of the request (the verifier's counterexample goes back once).
    let calls = document["provenance"]["authoring"]["calls"].as_u64();
    assert!(matches!(calls, Some(1 | 2)), "{document}");
    assert_ne!(
        document["status"], "ready",
        "a schema mock is not a semantic witness"
    );
    let invalid = call(
        room.path(),
        &[
            "compile",
            intent,
            "--authoring-model",
            "mock/echo",
            "--authoring-max-tokens",
            "0",
            "--json",
        ],
    );
    assert_eq!(invalid.status.code(), Some(3));
    assert!(result(&invalid)["error"].is_object());
}

/// The CLI defaults to escalation after the cold plan fails. Its receipt must include
/// both phases; a schema mock cannot become an accepted native candidate.
#[test]
fn default_native_escalation_preserves_calls_and_honors_the_repair_bound() {
    let room = tempfile::tempdir().expect("room");
    let intent = "Review this customer request and harmonise the tone of the support reply";
    for repairs in [0_u64, 1, 3] {
        let out = call(
            room.path(),
            &[
                "compile",
                intent,
                "--authoring-model",
                "mock/echo",
                "--authoring-repairs",
                &repairs.to_string(),
                "--authoring-timeout",
                "2",
                "--json",
            ],
        );
        let doc = result(&out);
        assert_eq!(out.status.code(), Some(2), "{doc}");
        assert_eq!(doc["compile_version"], 2);
        assert_eq!(doc["status"], "incomplete");
        assert!(doc["candidate"].is_null());
        let provenance = &doc["provenance"];
        let context = provenance["authoring"]["context"]
            .as_array()
            .expect("calls");
        let phases: Vec<_> = context
            .iter()
            .map(|c| c["call"].as_str().expect("phase"))
            .collect();
        let expected: &[&str] = if repairs == 0 {
            &["plan", "repair", "native"]
        } else {
            &["plan", "repair", "native", "native-repair"]
        };
        assert_eq!(phases, expected, "{doc}");
        assert_eq!(
            provenance["authoring"]["calls"].as_u64(),
            Some(context.len() as u64)
        );
        let native = &provenance["decision"]["native"];
        let rounds = native["rounds"].as_array().expect("native rounds");
        assert!(rounds.len() as u64 <= 1 + repairs, "{doc}");
        assert_eq!(native["accepted"], false);
        assert_eq!(rounds.len() + 2, context.len(), "cold calls remain counted");
        // The repeated mock candidate stops on no progress, even with repairs left.
        if repairs > 1 {
            assert_eq!(rounds.len(), 2, "{doc}");
        }
        assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
    }
}

/// The authoring seat rides the PROVIDER client (the runtime's fixed endpoint allowlist,
/// no SSRF floor, a transport ceiling above the requested timeout), never the fetch
/// client: a local seat on `127.0.0.1` reaches its socket and reports the socket's own
/// refusal, not an SSRF refusal — and the same client no longer cuts a cloud authoring
/// call at the fetch client's 30 s idle-read guard (the grok-4.7 408 of the preflight).
#[test]
fn authoring_seat_uses_the_provider_client_not_the_fetch_client() {
    let room = tempfile::tempdir().expect("room");
    let intent = "Review this customer request and harmonise the tone of the support reply";
    let out = command(room.path())
        // port 1 refuses at once; the fetch client would refuse the loopback literal itself
        .env("NIKA_OLLAMA_BASE_URL", "http://127.0.0.1:1")
        .args([
            "compile",
            intent,
            "--authoring-model",
            "ollama/qwen3.5:4b",
            "--authoring-timeout",
            "5",
            "--json",
        ])
        .output()
        .expect("CLI");
    let document = result(&out);
    assert_eq!(document["compile_version"], 2, "{document}");
    assert_eq!(
        document["provenance"]["authoring"]["calls"], 1,
        "{document}"
    );
    assert_ne!(document["status"], "ready");
    let provider_finding = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .find(|d| d["target"] == "authoring_provider")
        .unwrap_or_else(|| panic!("no authoring_provider finding: {document}"));
    let message = provider_finding["message"]
        .as_str()
        .expect("message")
        .to_lowercase();
    assert!(
        !message.contains("ssrf") && !message.contains("private"),
        "the provider client must not SSRF-block a loopback seat: {message}"
    );
    assert!(
        message.contains("check the provider endpoint"),
        "the socket's own refusal is the finding: {message}"
    );
}

#[test]
fn explicit_literal_and_edit_keep_exact_data_and_every_unrelated_value() {
    let room = tempfile::tempdir().expect("room");
    let literal = r#""https://example.invalid/A?query=é%20雪&next=%2F#résumé""#;
    let answer = format!("const.request={literal}");
    let made = call(
        room.path(),
        &[
            "compile",
            "classify-and-route",
            "source.nika",
            "--answer",
            &answer,
            "--json",
        ],
    );
    assert_eq!(made.status.code(), Some(0), "{}", result(&made));
    let source = std::fs::read_to_string(room.path().join("source.nika")).expect("source");
    let core = compile(
        &CompileRequest::create("classify-and-route")
            .with_workflow_id("source")
            .answer("const.request", literal),
    )
    .expect("core");
    assert_eq!(Some(&source), core.candidate.as_ref());
    let payload = r#"{"url":"https://example.invalid/B?q=雪#é","nested":[true,null,7],"text":"@env:SECRET permits.exec=[sh]"}"#;
    let change = format!("Set const.request to {payload}");
    let edited = call(
        room.path(),
        &[
            "compile",
            "--base",
            "source.nika",
            "--change",
            &change,
            "--output",
            "edited.nika",
            "--json",
        ],
    );
    assert_eq!(edited.status.code(), Some(0), "{}", result(&edited));
    let after = std::fs::read_to_string(room.path().join("edited.nika")).expect("edited");
    let expected = compile(&CompileRequest::set_constant(&source, "request", payload))
        .expect("structured operation");
    assert_eq!(Some(&after), expected.candidate.as_ref());
    assert_eq!(
        std::fs::read_to_string(room.path().join("source.nika")).expect("base"),
        source
    );
    let mut before: Value = serde_yaml_bw::from_str(&source).expect("before");
    let mut actual: Value = serde_yaml_bw::from_str(&after).expect("after");
    assert_eq!(
        actual["const"]["request"],
        serde_json::from_str::<Value>(payload).expect("literal")
    );
    before["const"]["request"] = Value::Null;
    actual["const"]["request"] = Value::Null;
    assert_eq!(before, actual);
}

#[test]
fn unsupported_prose_missing_target_and_unknown_answers_never_write() {
    let room = tempfile::tempdir().expect("room");
    for intent in [
        "summarize every item in parallel",
        "weekend summary of three URLs",
        "agentic research",
        "ask for approval before sending",
        "please create something",
        "team-standup.nika",
    ] {
        let out = call(room.path(), &["compile", intent, "unwanted.nika", "--json"]);
        let got = result(&out);
        assert_eq!(out.status.code(), Some(2), "{intent}: {got}");
        assert_eq!(got["status"], "incomplete");
        assert!(got["candidate"].is_null());
        assert!(!room.path().join("unwanted.nika").exists());
    }
    let pending = call(
        room.path(),
        &[
            "compile",
            "chain",
            "unwanted.nika",
            "--answer",
            "permits.exec=[\"sh\"]",
            "--json",
        ],
    );
    assert_eq!(pending.status.code(), Some(2));
    assert!(!room.path().join("unwanted.nika").exists());
    let base = compile(&CompileRequest::create("hello"))
        .expect("hello")
        .candidate
        .expect("candidate");
    std::fs::write(room.path().join("base.nika"), &base).expect("base");
    for change in [
        "Set const.missing to 7",
        "Add a Graph approval gate",
        "Use Jev to research this",
    ] {
        let out = call(
            room.path(),
            &[
                "compile",
                "--base",
                "base.nika",
                "--change",
                change,
                "--output",
                "unwanted.nika",
                "--json",
            ],
        );
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(result(&out)["candidate"], base);
        assert!(!room.path().join("unwanted.nika").exists());
    }
}

#[test]
fn destination_conflict_preserves_bytes_and_force_is_explicit() {
    let room = tempfile::tempdir().expect("room");
    let dest = "team's notes.nika";
    std::fs::write(room.path().join(dest), b"existing\x00bytes").expect("seed");
    let out = call(room.path(), &["compile", "hello", dest, "--json"]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(result(&out)["error"]["code"], "destination");
    assert_eq!(
        std::fs::read(room.path().join(dest)).expect("bytes"),
        b"existing\x00bytes"
    );
    let out = call(room.path(), &["compile", "hello", dest, "--force"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("nika run 'team'\\''s notes.nika'"));
    let body: Value =
        serde_yaml_bw::from_str(&std::fs::read_to_string(room.path().join(dest)).expect("file"))
            .expect("yaml");
    assert_eq!(body["nika"], "team-s-notes");
    assert_eq!(body["model"], "mock/echo");
    let ignore = std::fs::read_to_string(room.path().join(".gitignore")).expect("trace protection");
    assert!(ignore.contains(".nika/traces/"));
    assert!(!room.path().join(".nika").exists(), "Compile did not run");
}

#[test]
fn hello_alias_is_the_same_core_and_ambient_keys_never_select_a_provider() {
    let room = tempfile::tempdir().expect("room");
    let hello = compile(&CompileRequest::create("hello")).expect("core");
    let numbered = compile(&CompileRequest::create("01-hello")).expect("core alias");
    assert_eq!(hello.status, CompileStatus::Ready);
    assert_eq!(hello.candidate, numbered.candidate);
    let out = command(room.path())
        .args(["compile", "hello", "hello.nika", "--json"])
        .env("TYPESAFE_API_KEY", "synthetic-typesafe-canary")
        .env("OPENAI_API_KEY", "synthetic-openai-canary")
        .env("ANTHROPIC_API_KEY", "synthetic-anthropic-canary")
        .env("XAI_API_KEY", "synthetic-xai-canary")
        .output()
        .expect("keyed Compile");
    assert!(out.status.success(), "{}", result(&out));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("canary"));
    let source = std::fs::read_to_string(room.path().join("hello.nika")).expect("hello");
    let yaml: Value = serde_yaml_bw::from_str(&source).expect("yaml");
    assert_eq!(yaml["model"], "mock/echo");
    assert!(!room.path().join(".nika").exists());
    let run = command(room.path())
        .args(["run", "hello.nika", "--output", "json"])
        .env("TYPESAFE_API_KEY", "synthetic-typesafe-canary")
        .env("OPENAI_API_KEY", "synthetic-openai-canary")
        .env("ANTHROPIC_API_KEY", "synthetic-anthropic-canary")
        .env("XAI_API_KEY", "synthetic-xai-canary")
        .output()
        .expect("keyed mock run");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        result(&run)["greeting"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
}

#[test]
fn no_model_template_uses_core_and_invalid_adapter_inputs_are_explicit() {
    let room = tempfile::tempdir().expect("room");
    let out = call(room.path(), &["compile", "gate-and-act", "--json"]);
    let core = compile(&CompileRequest::create("gate-and-act")).expect("core");
    assert_eq!(
        result(&out)["candidate"],
        core.candidate.expect("candidate")
    );
    assert_eq!(out.status.code(), Some(0));
    let bad = call(
        room.path(),
        &["compile", "hello", "--answer", "broken", "--json"],
    );
    assert_eq!(bad.status.code(), Some(2));
    assert_eq!(result(&bad)["error"]["code"], "invalid_answer");
    let missing = call(
        room.path(),
        &[
            "compile",
            "--base",
            "absent.nika",
            "--change",
            "Set const.x to 1",
            "--json",
        ],
    );
    assert_eq!(missing.status.code(), Some(3));
    assert_eq!(result(&missing)["error"]["code"], "read_base");
    assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
}

#[test]
fn noncanonical_destinations_refuse_without_any_file() {
    let room = tempfile::tempdir().expect("room");
    for dest in ["hello.nika.yml", "hello.yaml", "hello", "hello.NIKA.YAML"] {
        let out = call(room.path(), &["compile", "hello", dest, "--json"]);
        assert_eq!(out.status.code(), Some(2));
        assert_eq!(result(&out)["error"]["code"], "destination_name");
        assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
    }
}

#[test]
fn failed_trace_protection_preserves_existing_destination_and_drops_temporary_file() {
    let room = tempfile::tempdir().expect("room");
    std::fs::create_dir(room.path().join(".gitignore")).expect("unwritable protection target");
    std::fs::write(room.path().join("hello.nika"), "original bytes").expect("seed");
    let out = call(
        room.path(),
        &["compile", "hello", "hello.nika", "--force", "--json"],
    );
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(result(&out)["error"]["code"], "destination");
    assert_eq!(
        std::fs::read_to_string(room.path().join("hello.nika")).expect("original"),
        "original bytes"
    );
    assert_eq!(
        std::fs::read_dir(room.path()).expect("dir").count(),
        2,
        "no temporary file survives"
    );
    let out = call(room.path(), &["compile", "hello", "absent.nika", "--json"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(!room.path().join("absent.nika").exists());
}

#[test]
fn concurrent_creators_publish_exactly_one_complete_candidate() {
    let room = tempfile::tempdir().expect("room");
    let args = ["compile", "hello", "race.nika", "--json"];
    let mut first = command(room.path())
        .args(args)
        .stdout(Stdio::null())
        .spawn()
        .expect("first creator");
    let mut second = command(room.path())
        .args(args)
        .stdout(Stdio::null())
        .spawn()
        .expect("second creator");
    let mut codes = [
        first.wait().expect("first status").code(),
        second.wait().expect("second status").code(),
    ];
    codes.sort();
    assert_eq!(codes, [Some(0), Some(3)]);
    let expected =
        compile(&CompileRequest::create("hello").with_workflow_id("race")).expect("core");
    assert_eq!(
        std::fs::read_to_string(room.path().join("race.nika")).expect("complete file"),
        expected.candidate.expect("candidate")
    );
}

#[test]
fn create_identity_cannot_rename_an_edit_and_expression_answers_stay_refused() {
    let source = compile(
        &CompileRequest::create("classify-and-route").answer("const.request", "\"original\""),
    )
    .expect("core")
    .candidate
    .expect("candidate");
    let renamed =
        compile(&CompileRequest::edit(&source, "Set const.request to 1").with_workflow_id("other"))
            .expect("rename refusal");
    assert_eq!(renamed.status, CompileStatus::Refused);
    assert_eq!(renamed.candidate.as_deref(), Some(source.as_str()));
    let room = tempfile::tempdir().expect("room");
    std::fs::write(room.path().join("base.nika"), &source).expect("base");
    let out = call(
        room.path(),
        &[
            "compile",
            "--base",
            "base.nika",
            "--change",
            "Set const.request to \"${{ env.SECRET }}\"",
            "--output",
            "refused.nika",
            "--json",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(result(&out)["status"], "refused");
    assert_eq!(result(&out)["candidate"], source);
    assert!(!room.path().join("refused.nika").exists());
}

#[test]
fn source_only_preview_does_not_read_ambient_project_policy() {
    let room = tempfile::tempdir().expect("room");
    std::fs::write(
        room.path().join("nika.yaml"),
        "this is not valid project settings: [",
    )
    .expect("poison ambient policy");
    let out = call(room.path(), &["compile", "hello", "--json"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(result(&out)["check_preview"]["scope"], "sourceOnly");
    let expected = compile(&CompileRequest::create("hello")).expect("pure core");
    assert_eq!(
        result(&out)["candidate"],
        expected.candidate.expect("candidate")
    );
    assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 1);
}

#[test]
fn an_explicit_dash_prefixed_path_teaches_a_runnable_file_argument() {
    let room = tempfile::tempdir().expect("room");
    let out = call(room.path(), &["compile", "hello", "--output=-hello.nika"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("nika run ./-hello.nika"));
    let run = call(room.path(), &["run", "./-hello.nika", "--output", "json"]);
    assert!(run.status.success());
    assert!(result(&run)["greeting"].as_str().is_some());
}

/// zsh rewrites a bare word that STARTS with `=` (`=ls` becomes the path of
/// ls, an unknown name aborts the line); sh reads it literally. The taught
/// word must read back verbatim in both. Only side-effect-free names reach a
/// real shell here: `printf` prints, nothing else runs.
#[cfg(unix)]
#[test]
fn an_equals_prefixed_path_is_taught_as_a_word_no_shell_rewrites() {
    let room = tempfile::tempdir().expect("room");
    for name in ["=ls.nika", "=value.nika"] {
        let out = call(room.path(), &["compile", "hello", name]);
        assert!(out.status.success(), "{out:?}");
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let taught = text
            .lines()
            .find_map(|line| line.strip_prefix("next · nika run "))
            .expect("taught run line");
        for shell in [&["/bin/sh", "-c"][..], &["/bin/zsh", "-f", "-c"][..]] {
            if !Path::new(shell[0]).exists() {
                continue;
            }
            let read = Command::new(shell[0])
                .args(&shell[1..])
                .arg(format!("printf '%s' {taught}"))
                .env_clear()
                .current_dir(room.path())
                .stdin(Stdio::null())
                .output()
                .expect("shell");
            assert!(read.status.success(), "{} refused {taught}", shell[0]);
            assert_eq!(String::from_utf8_lossy(&read.stdout), name, "{}", shell[0]);
        }
        let run = call(room.path(), &["run", name, "--output", "json"]);
        assert!(run.status.success(), "{run:?}");
    }
}

/// The shared parity set (`nika-compile/tests/fixtures/compile_parity_v1.json`) also
/// drives the core's own wire test and the Serve door: three doors, one document.
#[test]
fn the_cli_door_prints_the_core_document_for_every_shared_parity_case() {
    use nika_onboard::compile::outcome_document;

    let fixture: Value = serde_json::from_str(include_str!(
        "../../nika-compile/tests/fixtures/compile_parity_v1.json"
    ))
    .expect("parity fixture");
    let mut judged = 0;
    for case in fixture["cases"].as_array().expect("cases") {
        let doors = case["doors"].as_array().expect("doors");
        if !doors.iter().any(|door| door == "cli") {
            continue;
        }
        let name = case["name"].as_str().expect("case name");
        let native = &case["native"];
        let text = |key: &str| native[key].as_str().expect("native text field");
        let room = tempfile::tempdir().expect("room");
        let mut args = vec!["compile".to_owned()];
        let mut request = if text("mode") == "create" {
            args.push(text("intent").to_owned());
            CompileRequest::create(text("intent"))
        } else {
            assert_eq!(
                text("mode"),
                "edit",
                "{name}: the CLI has no structured edit flag"
            );
            let source = fixture["sources"][text("source_ref")]
                .as_str()
                .expect("named source");
            std::fs::write(room.path().join("base.nika"), source).expect("base");
            for word in ["--base", "base.nika", "--change", text("change_text")] {
                args.push(word.to_owned());
            }
            CompileRequest::edit(source, text("change_text"))
        };
        for pair in native["answers"].as_array().into_iter().flatten() {
            let key = pair[0].as_str().expect("answer key");
            let literal = pair[1].as_str().expect("answer literal text");
            args.push("--answer".to_owned());
            args.push(format!("{key}={literal}"));
            request = request.answer(key, literal);
        }
        args.push("--json".to_owned());
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = call(room.path(), &args);
        let mut printed = result(&out);
        // `written` is the one fact only this door owns; the rest is the core's.
        let written = printed.as_object_mut().expect("document").remove("written");
        assert_eq!(
            written,
            Some(Value::Null),
            "{name}: a preview writes nothing"
        );
        // Both sides cross the same printer and parser, so a float in the Check report
        // cannot differ by a parse round trip alone.
        let core = outcome_document(&compile(&request).expect("core")).to_string();
        let core: Value = serde_json::from_str(&core).expect("core document");
        assert_eq!(printed, core, "{name}");
        let expected_exit = if core["status"] == "ready" { 0 } else { 2 };
        assert_eq!(out.status.code(), Some(expected_exit), "{name}");
        if let Some(status) = case["expect"]["status"].as_str() {
            assert_eq!(printed["status"], status, "{name}");
        }
        judged += 1;
    }
    assert!(judged >= 25, "the CLI parity set shrank: {judged}");
}

#[test]
fn the_native_identity_advertises_the_compile_wire_it_really_serves() {
    let room = tempfile::tempdir().expect("room");
    let identity = result(&call(room.path(), &["--sdk-identity"]));
    assert!(
        identity["supportedCapabilities"]
            .as_array()
            .expect("capabilities")
            .iter()
            .any(|token| token == "compile"),
        "{identity}"
    );
    // The token is a promise about THIS door: it answers the generation it names.
    let listed = result(&call(room.path(), &["compile", "--list", "--json"]));
    assert_eq!(listed["compile_version"], 1);
    let hello = result(&call(room.path(), &["compile", "hello", "--json"]));
    assert_eq!(hello["compile_version"], 1);
    assert_eq!(hello["status"], "ready");
}
