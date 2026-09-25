// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types)]

//! The preview's documented budget exemption matches the actual CLI doors.
use std::net::TcpListener;
use std::process::Command;

#[test]
fn dry_run_help_names_the_budget_exemption_that_the_real_run_does_not_take() {
    let room = tempfile::tempdir().expect("isolated room");
    let home = room.path().join("home");
    std::fs::create_dir(&home).expect("home");
    let canary = TcpListener::bind("127.0.0.1:0").expect("owned endpoint");
    canary.set_nonblocking(true).expect("nonblocking canary");
    let endpoint = format!(
        "http://{}/v1/messages",
        canary.local_addr().expect("address")
    );
    std::fs::write(
        room.path().join("budget.nika"),
        "nika: budget-preview\nmodel: anthropic/claude-sonnet-5\ntasks:\n  say:\n    timeout: 2s\n    infer: { prompt: hi, max_tokens: 1000 }\n",
    )
    .expect("fixture");
    // `canary_route` aims provider traffic at the owned canary. That override is an unpriced
    // route: run cost admission refuses it before the budget floor, which judges only the
    // provider's priced default route (no canary can observe it: the HTTP client ignores proxies).
    let call = |args: &[&str], canary_route: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nika"));
        command
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &home)
            .env("NIKA_KEYCHAIN", "off")
            // Synthetic readiness only; both doors stop before provider I/O.
            .env(
                "ANTHROPIC_API_KEY",
                "sk-ant-api03-budget-fixture-not-a-real-key",
            )
            .env("NO_COLOR", "1")
            .current_dir(room.path());
        if canary_route {
            command.env("NIKA_ANTHROPIC_BASE_URL", &endpoint);
        }
        command.output().expect("isolated CLI")
    };
    let preview = call(
        &[
            "run",
            "budget.nika",
            "--dry-run",
            "--json",
            "--max-cost-usd",
            "0.000001",
        ],
        true,
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&preview.stdout).expect("plan JSON");
    assert_eq!(plan["plan_version"], 1);
    assert_eq!(
        canary
            .accept()
            .expect_err("preview made no connection")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    let unpriced = call(&["run", "budget.nika", "--max-cost-usd", "0.000001"], true);
    assert!(!unpriced.status.success(), "{unpriced:?}");
    assert!(
        String::from_utf8_lossy(&unpriced.stdout).contains("unknown-cost admission"),
        "{unpriced:?}"
    );
    assert_eq!(
        canary
            .accept()
            .expect_err("refused run made no connection")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    let run = call(&["run", "budget.nika", "--max-cost-usd", "0.000001"], false);
    assert_eq!(run.status.code(), Some(2), "{run:?}");
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("NIKA-1709"),
        "{run:?}"
    );
    let help = call(&["run", "--help"], true);
    assert!(help.status.success());
    let text = String::from_utf8(help.stdout)
        .expect("help UTF-8")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("does not judge --max-cost-usd"), "{text}");
}
