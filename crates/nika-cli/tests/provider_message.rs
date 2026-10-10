// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

//! A provider that refuses a call and explains why in prose is heard: at the Run door and at the
//! compile door the person reads the provider's own words, while the key in them is withheld from
//! everything shown and everything written: by value at the compile door, whose call sent it, and
//! by its shape at the Run door, a local route that sends no key (a cloud route's admission needs
//! an exact HTTPS endpoint). On 2026-10-10 an exhausted Anthropic balance (HTTP 400, "Your credit
//! balance is too low") read only `provider API error (HTTP 400); type=invalid_request_error`.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;

const KEY: &str = "sk-provider-message-test";

const REFUSAL: &str = r#"{"error":{"message":"Your credit balance is too low to access this API (key sk-provider-message-test).","type":"invalid_request_error"}}"#;

const WORKFLOW: &str = "nika: refused-provider\nmodel: vllm/refused-model\npermits: {}\n\
tasks:\n  say:\n    infer:\n      max_tokens: 512\n      prompt: Reply with three words.\n\
outputs:\n  answer: ${{ tasks.say.output }}\n";

const LISTING: &str = r#"{"object":"list","data":[{"id":"refused-model","object":"model"}]}"#;

/// A loopback provider: a GET (a local route's model listing) is answered at once, and every
/// inference request is refused with HTTP 400 and its reason in prose.
fn refusing_provider() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                return;
            };
            let (status, body) = if read_request(&mut stream).starts_with("get ") {
                ("200 OK", LISTING)
            } else {
                ("400 Bad Request", REFUSAL)
            };
            let reply = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    port
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

/// What the person is shown by one isolated command, and whether it succeeded.
fn nika(room: &Path, args: &[&str], port: u16) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room.join("home"))
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .env("DEEPSEEK_API_KEY", KEY)
        .env(
            "NIKA_DEEPSEEK_BASE_URL",
            format!("http://127.0.0.1:{port}/v1/chat/completions"),
        )
        .env("NIKA_VLLM_BASE_URL", format!("http://127.0.0.1:{port}/v1"))
        .current_dir(room)
        .output()
        .expect("isolated CLI");
    let shown = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), shown)
}

/// Every file under `dir` whose bytes hold `needle`.
fn files_holding(dir: &Path, needle: &[u8]) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).expect("readable room").flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_holding(&path, needle));
        } else if let Ok(bytes) = std::fs::read(&path)
            && bytes.windows(needle.len()).any(|window| window == needle)
        {
            found.push(path.display().to_string());
        }
    }
    found
}

#[test]
fn the_person_reads_the_provider_words_and_never_the_key() {
    let port = refusing_provider();
    let room = tempfile::tempdir().expect("isolated room");
    std::fs::create_dir(room.path().join("home")).expect("home");
    std::fs::write(room.path().join("refused.nika"), WORKFLOW).expect("workflow");
    std::fs::write(room.path().join("notes.txt"), "Ship the fix.\n").expect("notes");
    let (ran, run) = nika(
        room.path(),
        &["run", "refused.nika", "--max-cost-usd", "0.02"],
        port,
    );
    let intent = "Summarize notes.txt in three bullet points and write them to summary.md";
    let (_, compile) = nika(
        room.path(),
        &[
            "compile",
            intent,
            "--authoring-model",
            "deepseek/deepseek-flash",
            "--no-knowledge",
            "--json",
        ],
        port,
    );
    assert!(!ran, "the refusal fails the Run: {run}");
    for (door, shown) in [("run", &run), ("compile", &compile)] {
        assert!(
            shown.contains("Your credit balance is too low to access this API"),
            "{door}: {shown}"
        );
        assert!(!shown.contains(KEY), "{door}: {shown}");
    }
    let written = files_holding(room.path(), KEY.as_bytes());
    assert!(written.is_empty(), "the key was written to {written:?}");
}
