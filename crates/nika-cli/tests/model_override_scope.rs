// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! #1277: pins and child models remain selected, and the human learns the
//! override's scope before the provider answers. Every provider request goes to a
//! listener owned by this test; the child process has no operator credentials.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const SEAT: &str = "anthropic/claude-sonnet-5";
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
            "child" => "invoke: { workflow: ./child.nika.yaml }".to_owned(),
            _ => format!("infer: {{ model: {SEAT}, prompt: hi, max_tokens: 1024 }}"),
        };
        std::fs::write(
            dir.path().join("parent.nika.yaml"),
            format!("nika: scope\nmodel: mock/echo\ntasks:\n  inherited:\n    infer: {{ prompt: inherited, max_tokens: 1024 }}\n  selected:\n    timeout: 3s\n    {action}\n"),
        ).expect("parent");
        std::fs::write(
            dir.path().join("child.nika.yaml"),
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
                "ANTHROPIC_API_KEY",
                "sk-ant-api03-model-scope-fixture-not-a-real-key",
            )
            .env(
                "NIKA_ANTHROPIC_BASE_URL",
                format!(
                    "http://{}/v1/messages",
                    self.listener.local_addr().expect("address")
                ),
            )
            .stderr(File::create(self.dir.path().join("stderr.log")).expect("stderr file"))
            .current_dir(self.dir.path())
            .output()
            .expect("isolated CLI")
    }

    fn stderr(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("stderr.log")).expect("stderr")
    }

    fn no_request(&self) {
        assert_eq!(
            self.listener.accept().expect_err("no provider call").kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[allow(clippy::disallowed_methods)]
    fn serve_once(&self) -> JoinHandle<(serde_json::Value, String)> {
        let listener = self.listener.try_clone().expect("listener clone");
        let stderr = self.dir.path().join("stderr.log");
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("provider request missing: {error}"),
                }
            };
            stream
                .set_nonblocking(false)
                .expect("blocking request socket");
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .expect("read bound");
            let mut reader = BufReader::new(stream.try_clone().expect("socket clone"));
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
            assert!(length < 1_048_576);
            let mut body = vec![0; length];
            reader.read_exact(&mut body).expect("request body");
            let request: serde_json::Value = serde_json::from_slice(&body).expect("request JSON");
            // A regular file avoids cross-pipe reader scheduling: the notice
            // must be visible when handling the request, before responding.
            let before_response = std::fs::read_to_string(stderr).expect("boot stderr");
            let (content_type, response) = if request["stream"] == true {
                (
                    "text/event-stream",
                    concat!(
                        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"owned\",\"usage\":{\"input_tokens\":2}}}\n\n",
                        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"owned-provider-answer\"}}\n\n",
                        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
                        "data: {\"type\":\"message_stop\"}\n\n",
                    ),
                )
            } else {
                (
                    "application/json",
                    r#"{"id":"owned","model":"claude-sonnet-5","content":[{"type":"text","text":"owned-provider-answer"}],"stop_reason":"end_turn","usage":{"input_tokens":2,"output_tokens":1}}"#,
                )
            };
            write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response}", response.len()).expect("response");
            (request, before_response)
        })
    }
}

#[test]
fn human_notice_is_visible_to_the_provider_handler_and_models_stay_selected() {
    for kind in ["infer", "agent", "child"] {
        let room = Room::new(kind);
        let server = room.serve_once();
        let out = room.call(&["run", "parent.nika.yaml", "--model", "mock/echo"]);
        let (request, early_stderr) = server.join().expect("canary");
        assert!(out.status.success(), "{kind}: {}", room.stderr());
        assert_eq!(request["model"], "claude-sonnet-5", "{kind}");
        assert!(early_stderr.contains(PREFIX), "{kind}: {early_stderr}");
        assert!(
            early_stderr.contains(if kind == "child" { "parent-only" } else { SEAT }),
            "{early_stderr}"
        );
        room.no_request();
    }
}

#[test]
fn check_explains_pins_and_children_on_human_and_json_doors_without_io() {
    for kind in ["infer", "agent", "child"] {
        let room = Room::new(kind);
        for json in [false, true] {
            let mut args = vec!["check", "parent.nika.yaml", "--model", "mock/echo"];
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
            room.no_request();
        }
    }
}

#[test]
fn both_machine_run_outputs_stay_parseable_and_keep_the_pinned_model() {
    for flags in [&["--json"][..], &["--output", "json"][..]] {
        let room = Room::new("infer");
        let server = room.serve_once();
        let mut args = vec!["run", "parent.nika.yaml", "--model", "mock/echo"];
        args.extend_from_slice(flags);
        let out = room.call(&args);
        let (request, early_stderr) = server.join().expect("canary");
        assert!(out.status.success(), "{}", room.stderr());
        assert_eq!(request["model"], "claude-sonnet-5");
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
        room.no_request();
    }
}

#[test]
fn task_scope_excludes_an_unused_pin_and_dry_run_never_calls_the_provider() {
    let room = Room::new("infer");
    let scoped = room.call(&[
        "run",
        "parent.nika.yaml",
        "--model",
        "mock/echo",
        "--task",
        "inherited",
    ]);
    assert!(scoped.status.success(), "{}", room.stderr());
    assert!(!room.stderr().contains("model override:"));
    room.no_request();
    let preview = room.call(&[
        "run",
        "parent.nika.yaml",
        "--model",
        "mock/echo",
        "--dry-run",
    ]);
    assert!(preview.status.success(), "{}", room.stderr());
    assert!(room.stderr().contains(PREFIX));
    room.no_request();
}

#[test]
fn no_override_and_quiet_mode_preserve_the_existing_announcement_policy() {
    for quiet in [false, true] {
        let room = Room::new("infer");
        let server = room.serve_once();
        let mut args = vec!["run", "parent.nika.yaml"];
        if quiet {
            args.extend(["--model", "mock/echo", "--quiet"]);
        }
        let out = room.call(&args);
        let (request, early_stderr) = server.join().expect("canary");
        assert!(out.status.success(), "{}", room.stderr());
        assert_eq!(request["model"], "claude-sonnet-5");
        assert!(!early_stderr.contains("model override:"));
        assert!(!room.stderr().contains("model override:"));
        room.no_request();
    }
}
