// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods
)]

//! A provider call is never re-sent by the transport, and the receipt counts every request
//! actually sent: a loopback provider that answers 503 once (and would answer 200 after) sees
//! exactly the requests the compile's authoring receipt counts, and `compile --help` says the
//! same. On 2026-10-10 a 503 drew four identical 772 KB authoring requests while the receipt
//! said one call.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

const UNAVAILABLE: &str = r#"{"error":{"message":"busy","type":"server_error"}}"#;
const ANSWER: &str = r#"{"id":"c","object":"chat.completion","model":"deepseek-flash","choices":[{"index":0,"message":{"role":"assistant","content":"{}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2,"total_tokens":12}}"#;

/// A loopback provider counting every request: the first answered 503, every later one 200.
fn provider(seen: Arc<AtomicUsize>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                return;
            };
            read_request(&mut stream);
            let first = seen.fetch_add(1, Ordering::SeqCst) == 0;
            let (status, body) = if first {
                ("503 Service Unavailable", UNAVAILABLE)
            } else {
                ("200 OK", ANSWER)
            };
            let reply = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    port
}

/// Reads one whole request, its head and its `content-length` body: a reply sent over unread
/// request bytes can be reset on close before the client has read it.
fn read_request(stream: &mut TcpStream) {
    let (mut seen, mut chunk) = (Vec::new(), [0_u8; 8192]);
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
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
            return;
        }
    }
}

fn nika(room: &std::path::Path, args: &[&str], port: u16) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room.join("home"))
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .env("DEEPSEEK_API_KEY", "sk-no-resend-test")
        .env(
            "NIKA_DEEPSEEK_BASE_URL",
            format!("http://127.0.0.1:{port}/v1/chat/completions"),
        )
        .current_dir(room)
        .output()
        .expect("isolated CLI")
}

#[test]
fn a_503_is_sent_once_and_the_authoring_receipt_counts_what_was_sent() {
    let seen = Arc::new(AtomicUsize::new(0));
    let port = provider(Arc::clone(&seen));
    let room = tempfile::tempdir().expect("isolated room");
    std::fs::create_dir(room.path().join("home")).expect("home");
    std::fs::write(room.path().join("notes.txt"), "Ship the fix.\n").expect("notes");
    let intent = "Summarize notes.txt in three bullet points and write them to summary.md";
    let compile = nika(
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
    let record: serde_json::Value =
        serde_json::from_slice(&compile.stdout).expect("the versioned compile result");
    let sent = seen.load(Ordering::SeqCst);
    assert_eq!(sent, 1, "the 503 was not re-sent: {record}");
    assert_eq!(
        record["provenance"]["authoring"]["calls"],
        serde_json::json!(sent),
        "the receipt counts every request actually sent: {record}"
    );
    let help = nika(room.path(), &["compile", "--help"], port);
    let help = String::from_utf8(help.stdout).expect("help");
    assert!(
        help.contains("The transport never retries a request on its own"),
        "{help}"
    );
}
