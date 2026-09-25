// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! #1277: pins and child models remain selected, and the human learns the
//! override's scope before the provider answers. Every provider request goes to a
//! listener owned by this test; the child process has no operator credentials.
//! The explicit local vLLM lane admits the fixture without borrowing a native
//! API tariff for an overridden endpoint. Both streamed and single responses
//! still cross the real CLI and OpenAI-compatible transport.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const SEAT: &str = "vllm/scope-fixture";
const PREFIX: &str = "model override: --model `mock/echo`";

struct Room {
    dir: tempfile::TempDir,
    listener: TcpListener,
}

impl Room {
    fn new(kind: &str) -> Self {
        let dir = tempfile::tempdir().expect("isolated room");
        std::fs::create_dir(dir.path().join("home")).expect("home");
        let listener = TcpListener::bind("127.0.0.1:0").expect("owned endpoint");
        listener.set_nonblocking(true).expect("nonblocking");
        let action = match kind {
            "agent" => format!(
                "agent: {{ model: {SEAT}, prompt: hi, max_tokens_total: 1024, max_turns: 1 }}"
            ),
            "child" => "invoke: { workflow: ./child.nika }".to_owned(),
            _ => format!("infer: {{ model: {SEAT}, prompt: hi, max_tokens: 1024 }}"),
        };
        std::fs::write(
            dir.path().join("parent.nika"),
            format!("nika: scope\nmodel: mock/echo\ntasks:\n  inherited:\n    infer: {{ prompt: inherited, max_tokens: 1024 }}\n  selected:\n    timeout: 3s\n    {action}\n"),
        ).expect("parent");
        std::fs::write(
            dir.path().join("child.nika"),
            format!("nika: child\nmodel: {SEAT}\ntasks:\n  answer:\n    timeout: 3s\n    infer: {{ prompt: child, max_tokens: 1024 }}\n"),
        ).expect("child");
        Self { dir, listener }
    }

    fn call(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_nika"))
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.dir.path().join("home"))
            .env("NIKA_KEYCHAIN", "off")
            .env("NO_COLOR", "1")
            .env(
                "NIKA_VLLM_BASE_URL",
                self.listener.local_addr().expect("address").to_string(),
            )
            .stderr(File::create(self.dir.path().join("stderr.log")).expect("stderr file"))
            .current_dir(self.dir.path())
            .output()
            .expect("isolated CLI")
    }

    fn stderr(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("stderr.log")).expect("stderr")
    }

    fn no_model_request(&self) {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => assert!(
                    read_provider_request(&stream).is_none(),
                    "no model call (local reachability probes are not inference)"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("listener: {error}"),
            }
        }
    }

    #[allow(clippy::disallowed_methods)]
    fn serve_once(&self) -> JoinHandle<(serde_json::Value, String)> {
        let listener = self.listener.try_clone().expect("listener clone");
        let stderr = self.dir.path().join("stderr.log");
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let (mut stream, request) = loop {
                assert!(Instant::now() < deadline, "provider request missing");
                match listener.accept() {
                    Ok((stream, _)) => {
                        if let Some(request) = read_provider_request(&stream) {
                            break (stream, request);
                        }
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("provider request missing: {error}"),
                }
            };
            // A regular file avoids cross-pipe reader scheduling: the notice
            // must be visible when handling the request, before responding.
            let before_response = std::fs::read_to_string(stderr).expect("boot stderr");
            let (content_type, response) = if request["stream"] == true {
                (
                    "text/event-stream",
                    concat!(
                        "data: {\"id\":\"owned\",\"choices\":[{\"delta\":{\"content\":\"owned-provider-answer\"},\"finish_reason\":null}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1}}\n\n",
                        "data: [DONE]\n\n",
                    ),
                )
            } else {
                (
                    "application/json",
                    r#"{"id":"owned","model":"scope-fixture","choices":[{"index":0,"message":{"role":"assistant","content":"owned-provider-answer"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1}}"#,
                )
            };
            write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response}", response.len()).expect("response");
            (request, before_response)
        })
    }
}

/// Local census may connect or issue GET / before dispatch. Only POST is a model call.
fn read_provider_request(stream: &TcpStream) -> Option<serde_json::Value> {
    stream
        .set_nonblocking(false)
        .expect("blocking request socket");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("read bound");
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).expect("request line") == 0 {
        return None;
    }
    let mut length = 0;
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).expect("header") > 0);
        if line == "\r\n" {
            break;
        }
        if let Some((key, value)) = line.split_once(':')
            && key.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<usize>().expect("body length");
        }
    }
    if request_line.starts_with("GET / ") {
        return None;
    }
    assert!(
        request_line.starts_with("POST /v1/chat/completions "),
        "{request_line}"
    );
    assert!(length < 1_048_576);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("request body");
    Some(serde_json::from_slice(&body).expect("request JSON"))
}

#[test]
fn human_notice_is_visible_to_the_provider_handler_and_models_stay_selected() {
    for kind in ["infer", "agent", "child"] {
        let room = Room::new(kind);
        let server = room.serve_once();
        let out = room.call(&["run", "parent.nika", "--model", "mock/echo"]);
        let (request, early_stderr) = server.join().expect("canary");
        assert!(out.status.success(), "{kind}: {}", room.stderr());
        assert_eq!(request["model"], "scope-fixture", "{kind}");
        assert!(early_stderr.contains(PREFIX), "{kind}: {early_stderr}");
        assert!(
            early_stderr.contains(if kind == "child" { "parent-only" } else { SEAT }),
            "{early_stderr}"
        );
        room.no_model_request();
    }
}

#[test]
fn check_explains_pins_and_children_on_human_and_json_doors_without_model_calls() {
    for kind in ["infer", "agent", "child"] {
        let room = Room::new(kind);
        for json in [false, true] {
            let mut args = vec!["check", "parent.nika", "--model", "mock/echo"];
            if json {
                args.push("--json");
            }
            let out = room.call(&args);
            assert!(out.status.success(), "{kind}: {}", room.stderr());
            let text = String::from_utf8(out.stdout).expect("check output");
            assert!(
                text.contains(if kind == "child" { "parent-only" } else { SEAT }),
                "{text}"
            );
            if json {
                let payload: serde_json::Value =
                    serde_json::from_str(&text).expect("one JSON report");
                let hints = payload["hints"].as_array().expect("hints");
                let scope: Vec<_> = hints
                    .iter()
                    .filter(|h| h["kind"] == "envelope-model")
                    .collect();
                assert_eq!(scope.len(), 1, "{scope:?}");
                assert_eq!(scope[0]["task"], "selected");
            }
            room.no_model_request();
        }
    }
}

#[test]
fn both_machine_run_outputs_stay_parseable_and_keep_the_pinned_model() {
    for flags in [&["--json"][..], &["--output", "json"][..]] {
        let room = Room::new("infer");
        let server = room.serve_once();
        let mut args = vec!["run", "parent.nika", "--model", "mock/echo"];
        args.extend_from_slice(flags);
        let out = room.call(&args);
        let (request, early_stderr) = server.join().expect("canary");
        assert!(out.status.success(), "{}", room.stderr());
        assert_eq!(request["model"], "scope-fixture");
        assert!(!early_stderr.contains("model override:"));
        assert!(!room.stderr().contains("model override:"));
        let text = String::from_utf8(out.stdout).expect("machine output");
        if flags == ["--json"] {
            for line in text.lines() {
                serde_json::from_str::<serde_json::Value>(line).expect("NDJSON frame");
            }
            assert!(text.contains("workflow_completed"));
        } else {
            serde_json::from_str::<serde_json::Value>(&text).expect("one output object");
        }
        room.no_model_request();
    }
}

#[test]
fn task_scope_excludes_an_unused_pin_and_dry_run_never_calls_the_provider() {
    let room = Room::new("infer");
    let scoped = room.call(&[
        "run",
        "parent.nika",
        "--model",
        "mock/echo",
        "--task",
        "inherited",
    ]);
    assert!(scoped.status.success(), "{}", room.stderr());
    assert!(!room.stderr().contains("model override:"));
    room.no_model_request();
    let preview = room.call(&["run", "parent.nika", "--model", "mock/echo", "--dry-run"]);
    assert!(preview.status.success(), "{}", room.stderr());
    assert!(room.stderr().contains(PREFIX));
    room.no_model_request();
}

#[test]
fn no_override_and_quiet_mode_preserve_the_existing_announcement_policy() {
    for quiet in [false, true] {
        let room = Room::new("infer");
        let server = room.serve_once();
        let mut args = vec!["run", "parent.nika"];
        if quiet {
            args.extend(["--model", "mock/echo", "--quiet"]);
        }
        let out = room.call(&args);
        let (request, early_stderr) = server.join().expect("canary");
        assert!(out.status.success(), "{}", room.stderr());
        assert_eq!(request["model"], "scope-fixture");
        assert!(!early_stderr.contains("model override:"));
        assert!(!room.stderr().contains("model override:"));
        room.no_model_request();
    }
}
