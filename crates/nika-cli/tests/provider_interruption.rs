// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! #1362: real HTTP disconnects through the CLI, provider and attempt loop.
//! All calls terminate at an owned listener; no operator credentials are read.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

#[derive(Clone, Copy)]
enum Fault {
    Headers,
    Body,
    Invalid,
    Auth,
    Recover,
    AgentAfterTool,
}

struct Room {
    dir: tempfile::TempDir,
    endpoint: String,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<usize>>,
}

impl Room {
    // The synchronous CLI fixture owns and joins this blocking socket thread.
    #[allow(clippy::disallowed_methods)]
    fn new(fault: Fault, retry: bool, budget: u32) -> Self {
        let dir = tempfile::tempdir().expect("isolated room");
        std::fs::create_dir(dir.path().join("home")).expect("home");
        let listener = TcpListener::bind("127.0.0.1:0").expect("owned endpoint");
        listener.set_nonblocking(true).expect("nonblocking");
        let endpoint = format!("http://{}", listener.local_addr().expect("address"));
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&stop);
        let server = std::thread::spawn(move || serve(&listener, &done, fault));
        let policy = if retry {
            format!(
                "    retry: {{ max_attempts: {budget}, backoff_ms: 1, backoff_strategy: fixed, jitter: false }}\n"
            )
        } else {
            String::new()
        };
        std::fs::write(dir.path().join("workflow.nika"), format!(
            "nika: interrupted-provider\nmodel: anthropic/claude-sonnet-5\npermits: {{}}\nrun: {{ clock: system }}\ntasks:\n  answer:\n    timeout: 3s\n{policy}    infer: {{ prompt: fixture, max_tokens: 256 }}\noutputs:\n  answer: \"${{{{ tasks.answer.output }}}}\"\n"
        )).expect("workflow");
        Self {
            dir,
            endpoint,
            stop,
            server: Some(server),
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
        command
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.dir.path().join("home"))
            .env("NIKA_KEYCHAIN", "off")
            .env("NO_COLOR", "1")
            .env(
                "ANTHROPIC_API_KEY",
                "sk-ant-api03-owned-fixture-not-a-real-key",
            )
            .env(
                "NIKA_ANTHROPIC_BASE_URL",
                format!("{}/private-path?token=owned-canary", self.endpoint),
            )
            .current_dir(self.dir.path());
        command
    }

    fn run(&self) -> (Output, Vec<Value>) {
        let output = self
            .command(&["run", "workflow.nika", "--json"])
            .output()
            .expect("CLI run");
        let text = String::from_utf8_lossy(&output.stdout);
        let events = text
            .lines()
            .map(|line| serde_json::from_str(line).expect("NDJSON frame"))
            .collect();
        (output, events)
    }

    fn finish(&mut self) -> usize {
        self.stop.store(true, Ordering::Release);
        self.server
            .take()
            .expect("server")
            .join()
            .expect("server joins")
    }

    fn verify(&self, events: &[Value]) {
        let settled = frame(events, "run_settled");
        let path = settled["receipt"]["trace_path"]
            .as_str()
            .expect("trace path");
        let output = self
            .command(&["trace", "verify", path, "--json"])
            .output()
            .expect("verify");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        serde_json::from_slice::<Value>(&output.stdout).expect("verify JSON");
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

#[allow(clippy::disallowed_methods)]
fn serve(listener: &TcpListener, done: &AtomicBool, fault: Fault) -> usize {
    let mut count = 0;
    while !done.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut socket, _)) => {
                socket
                    .set_nonblocking(false)
                    .expect("blocking accepted socket");
                read_request(&socket);
                count += 1;
                let response = match fault {
                    Fault::AgentAfterTool if count == 1 => {
                        let body = r#"{"id":"owned","model":"claude-sonnet-5","content":[{"type":"tool_use","id":"write_once","name":"nika_write","input":{"path":"evidence.txt","content":"written once"}}],"stop_reason":"tool_use","usage":{"input_tokens":2,"output_tokens":1}}"#;
                        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("tool response");
                        continue;
                    }
                    Fault::Headers | Fault::AgentAfterTool => "",
                    Fault::Body => {
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 4096\r\nConnection: close\r\n\r\n{\"id\":"
                    }
                    Fault::Invalid => "THIS IS NOT HTTP\r\n\r\n",
                    Fault::Auth => {
                        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    }
                    Fault::Recover if count == 1 => "",
                    Fault::Recover => {
                        let body = r#"{"id":"owned","model":"claude-sonnet-5","content":[{"type":"text","text":"provider recovered"}],"stop_reason":"end_turn","usage":{"input_tokens":2,"output_tokens":1}}"#;
                        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("response");
                        continue;
                    }
                };
                socket
                    .write_all(response.as_bytes())
                    .expect("fault response");
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("listener: {error}"),
        }
    }
    count
}

fn read_request(socket: &TcpStream) {
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read bound");
    let mut reader = BufReader::new(socket);
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
            length = value.trim().parse::<usize>().expect("length");
        }
    }
    assert!(length < 1_048_576);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("body");
    let request: Value = serde_json::from_slice(&body).expect("provider JSON");
    assert_eq!(
        request["stream"], false,
        "workflow inference uses the buffered provider door"
    );
}

fn frame<'a>(events: &'a [Value], kind: &str) -> &'a Value {
    events
        .iter()
        .find(|event| event["kind"] == kind)
        .expect("frame present")
}

fn outcome(events: &[Value]) -> Value {
    let fields = frame(events, "task_failed")["fields"]
        .as_array()
        .expect("fields");
    let value = fields
        .iter()
        .find(|field| field["key"] == "outcome")
        .expect("outcome");
    serde_json::from_str(value["value"].as_str().expect("outcome JSON")).expect("typed outcome")
}

#[test]
fn disconnects_retry_only_to_the_authored_attempt_limit_and_keep_evidence() {
    for fault in [Fault::Headers, Fault::Body] {
        let mut room = Room::new(fault, true, 3);
        let (output, events) = room.run();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(room.finish(), 3);
        let failure = outcome(&events);
        assert_eq!(failure["payload"]["attempts"], 3);
        assert_eq!(failure["payload"]["error"]["transient"], true);
        assert_eq!(failure["payload"]["error"]["code"], "NIKA-INFER-001");
        let message = failure["payload"]["error"]["message"]
            .as_str()
            .expect("message");
        for expected in [
            &room.endpoint,
            "connection closed",
            "retry.max_attempts",
            "unknown, not zero",
            "mock/echo",
        ] {
            assert!(message.contains(expected), "{message}");
        }
        for private in ["private-path", "owned-canary", "token="] {
            assert!(!message.contains(private), "{message}");
        }
        assert_eq!(
            frame(&events, "run_settled")["spend"]["qualifier"],
            "unmetered"
        );
        room.verify(&events);
    }
}

#[test]
fn a_recovered_provider_finishes_on_the_second_attempt() {
    let mut room = Room::new(Fault::Recover, true, 3);
    let (output, events) = room.run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(room.finish(), 2);
    assert_eq!(
        frame(&events, "run_settled")["outputs"]["answer"],
        "provider recovered"
    );
    room.verify(&events);
}

#[test]
fn no_retry_policy_or_single_attempt_never_replays_an_interruption() {
    for retry in [false, true] {
        let mut room = Room::new(Fault::Headers, retry, 1);
        let (output, events) = room.run();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(room.finish(), 1);
        assert_eq!(outcome(&events)["payload"]["error"]["transient"], true);
        assert_eq!(outcome(&events)["payload"]["attempts"], 1);
        room.verify(&events);
    }
}

#[test]
fn malformed_responses_and_auth_failures_remain_terminal() {
    for fault in [Fault::Invalid, Fault::Auth] {
        let mut room = Room::new(fault, true, 3);
        let (output, events) = room.run();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(room.finish(), 1);
        assert_eq!(outcome(&events)["payload"]["error"]["transient"], false);
        room.verify(&events);
    }
}

#[test]
fn an_agent_connection_failure_after_a_tool_never_replays_the_task() {
    for on_codes in ["", ", on_codes: [NIKA-INFER-001]"] {
        let mut room = Room::new(Fault::AgentAfterTool, true, 3);
        let workflow = format!(
            "nika: agent-interrupted\nmodel: anthropic/claude-sonnet-5\npermits: {{ tools: [\"nika:write\"], fs: {{ write: [\"./evidence.txt\"] }} }}\ntasks:\n  answer:\n    timeout: 3s\n    retry: {{ max_attempts: 3, backoff_ms: 1, jitter: false{on_codes} }}\n    agent: {{ prompt: fixture, tools: [\"nika:write\"], max_turns: 3, max_tokens_total: 2048 }}\n"
        );
        std::fs::write(room.dir.path().join("workflow.nika"), workflow).expect("agent workflow");
        let (output, events) = room.run();
        assert_eq!(
            output.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            room.finish(),
            2,
            "first turn writes, second disconnects, no whole-task retry"
        );
        assert_eq!(
            std::fs::read_to_string(room.dir.path().join("evidence.txt")).expect("tool effect"),
            "written once"
        );
        let failure = outcome(&events);
        assert_eq!(failure["payload"]["attempts"], 1);
        assert_eq!(
            failure["payload"]["error"]["transient"], true,
            "transport class stays honest"
        );
        assert!(
            failure["payload"]["error"]["message"]
                .as_str()
                .expect("message")
                .contains("automatic task replay is suppressed")
        );
        assert!(!events.iter().any(|event| event["kind"] == "task_retrying"));
        assert!(
            events.iter().any(|event| event["kind"] == "tool_invoked"),
            "prior tool evidence retained"
        );
        room.verify(&events);
    }
}

#[test]
fn an_agent_without_prior_tools_can_retry_a_connection_failure() {
    let mut room = Room::new(Fault::Recover, true, 3);
    let file = room.dir.path().join("workflow.nika");
    let workflow = std::fs::read_to_string(&file).expect("fixture").replace(
        "infer: { prompt: fixture, max_tokens: 256 }",
        "agent: { prompt: fixture, tools: [], max_turns: 3, max_tokens_total: 2048 }",
    );
    std::fs::write(file, workflow).expect("agent fixture");
    let (output, events) = room.run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(room.finish(), 2);
    assert_eq!(
        frame(&events, "run_settled")["outputs"]["answer"],
        "provider recovered"
    );
    room.verify(&events);
}
