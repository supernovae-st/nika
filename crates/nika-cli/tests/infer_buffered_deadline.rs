// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

//! A buffered `infer:` whose provider answers after 35 s, with steady keep-alive bytes
//! meanwhile, completes its Run: no implicit total deadline shorter than the provider transport's
//! own cuts legitimate work the workflow never bounded. On 2026-10-10 (E1 run 2) a news summary
//! that declared no `timeout:` was cut at 30.0 s (NIKA-INFER-001, provider 408) and the Run
//! wrote nothing. The Run goes through a local route (a cloud route's admission needs an exact
//! HTTPS endpoint); the providers parity test holds every cloud wire to the same deadline.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::time::{Duration, Instant};

const ANSWER: &str = r#"{"id":"chatcmpl-slow","object":"chat.completion","model":"slow-model","choices":[{"index":0,"message":{"role":"assistant","content":"slow but whole"},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":4,"total_tokens":16}}"#;

const WORKFLOW: &str = "nika: slow-provider\nmodel: vllm/slow-model\npermits: {}\n\
tasks:\n  say:\n    infer:\n      max_tokens: 512\n      prompt: Reply with three words.\n\
outputs:\n  answer: ${{ tasks.say.output }}\n";

const LISTING: &str = r#"{"object":"list","data":[{"id":"slow-model","object":"model"}]}"#;

/// A loopback local provider serving every connection: a GET (a local route's model listing) is
/// answered at once; an inference POST gets the response headers at once, then a keep-alive
/// newline every 5 s for 35 s (the progress a provider shows while it works), then the answer.
fn slow_provider() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                return;
            };
            std::thread::spawn(move || answer(stream));
        }
    });
    port
}

fn answer(mut stream: TcpStream) {
    if read_request(&mut stream).starts_with("get ") {
        let reply = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
             connection: close\r\n\r\n{LISTING}",
            LISTING.len()
        );
        let _ = stream.write_all(reply.as_bytes());
        return;
    }
    let body = format!("{}{ANSWER}", "\n".repeat(7));
    let head = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    for _ in 0..7 {
        std::thread::sleep(Duration::from_secs(5));
        let _ = stream.write_all(b"\n");
        let _ = stream.flush();
    }
    let _ = stream.write_all(ANSWER.as_bytes());
}

/// Reads one whole request, its head and its `content-length` body (a reply sent over unread
/// request bytes can be reset on close before the client has read it), and returns the head.
fn read_request(stream: &mut TcpStream) -> String {
    let (mut seen, mut chunk) = (Vec::new(), [0_u8; 8192]);
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return String::from_utf8_lossy(&seen).to_lowercase(),
            Ok(n) => seen.extend_from_slice(&chunk[..n]),
        }
        let Some(end) = seen.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&seen[..end]).to_lowercase();
        let length: usize = (head.lines())
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0);
        if seen.len() >= end + 4 + length {
            return head;
        }
    }
}

#[test]
fn a_buffered_cloud_answer_after_35s_completes_the_run() {
    let port = slow_provider();
    let room = tempfile::tempdir().expect("isolated room");
    let home = room.path().join("home");
    std::fs::create_dir(&home).expect("home");
    std::fs::write(room.path().join("slow.nika"), WORKFLOW).expect("workflow");
    let started = Instant::now();
    let run = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args([
            "run",
            "slow.nika",
            "--output",
            "json",
            "--max-cost-usd",
            "0.02",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home)
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .env("NIKA_VLLM_BASE_URL", format!("http://127.0.0.1:{port}/v1"))
        .current_dir(room.path())
        .output()
        .expect("isolated CLI");
    let elapsed = started.elapsed();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(elapsed >= Duration::from_secs(35), "{elapsed:?}");
    let output: serde_json::Value = serde_json::from_slice(&run.stdout).expect("typed output");
    assert_eq!(output["answer"], "slow but whole", "{output}");
}
