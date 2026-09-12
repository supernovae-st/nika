// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

//! #1545: a model override must preserve admission and reach execution.
use std::process::{Command, Output};

const MISSING_MCP: &str = "nika: missing-mcp\nmodel: mock/echo\npermits:\n  tools: [nika:write, 'mcp:sandboxfs/write_file']\n  fs: { write: ['./marker'] }\ntasks:\n  first:\n    invoke: { tool: 'nika:write', args: { path: './marker', content: executed } }\n  w:\n    after: { first: success }\n    invoke: { tool: 'mcp:sandboxfs/write_file', args: {} }\n";
const MODEL: &str = "nika: override\nmodel: missing/model\ntasks:\n  answer:\n    infer: { prompt: legal-override-executed, max_tokens: 32 }\noutputs:\n  result: ${{ tasks.answer.output }}\n";

fn call(room: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room.join("home"))
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .current_dir(room)
        .output()
        .expect("isolated binary")
}

fn text(out: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn missing_mcp_refuses_with_and_without_model_on_json_and_human_doors() {
    for (door, dry_run) in [("run", false), ("run", true), ("check", false)] {
        for json in [true, false] {
            for override_model in [true, false] {
                let room = tempfile::tempdir().expect("room");
                std::fs::create_dir(room.path().join("home")).expect("home");
                std::fs::write(room.path().join("mix.nika.yaml"), MISSING_MCP).expect("workflow");
                let mut args = vec![door, "mix.nika.yaml"];
                if dry_run {
                    args.push("--dry-run");
                }
                if json {
                    args.push("--json");
                }
                if override_model {
                    args.extend(["--model", "mock/echo"]);
                }
                let out = call(room.path(), &args);
                let rendered = text(&out);
                assert_eq!(out.status.code(), Some(2), "{args:?}: {rendered}");
                assert!(rendered.contains("NIKA-INVOKE-001"), "{args:?}: {rendered}");
                assert!(
                    rendered.contains("not configured")
                        && rendered.contains(".nika/mcp_servers.json"),
                    "{args:?}: {rendered}"
                );
                assert!(!rendered.contains("not a canonical builtin"), "{rendered}");
                assert!(
                    !room.path().join("marker").exists(),
                    "refused before effects"
                );
                assert!(
                    !room.path().join(".nika/traces").exists(),
                    "no run was admitted"
                );
                if json {
                    let payload: serde_json::Value =
                        serde_json::from_str(rendered.trim()).expect("one refusal report");
                    assert_eq!(payload["clean"], false, "{payload}");
                }
            }
        }
    }
}

#[test]
fn legal_override_runs_and_invalid_override_still_refuses() {
    let room = tempfile::tempdir().expect("room");
    std::fs::create_dir(room.path().join("home")).expect("home");
    std::fs::write(room.path().join("model.nika.yaml"), MODEL).expect("workflow");
    let bad = call(
        room.path(),
        &[
            "run",
            "model.nika.yaml",
            "--model",
            "missing/other",
            "--json",
        ],
    );
    assert_eq!(bad.status.code(), Some(2), "{}", text(&bad));
    for json in [false, true] {
        let mut args = vec!["run", "model.nika.yaml", "--model", "mock/echo"];
        if json {
            args.push("--json");
        }
        let good = call(room.path(), &args);
        assert_eq!(good.status.code(), Some(0), "{}", text(&good));
        assert!(
            text(&good).contains("legal-override-executed"),
            "{}",
            text(&good)
        );
        if json {
            assert!(
                text(&good).contains("workflow_completed"),
                "{}",
                text(&good)
            );
        }
    }
}

#[test]
fn override_preserves_parse_permissions_and_composition_refusals() {
    for (body, code) in [
        ("nika: broken\ntasks: {}\nbogus: value\n", "PARSE"),
        (
            "nika: denied\npermits: { tools: ['nika:write'], fs: { write: ['./other'] } }\ntasks:\n  write:\n    invoke: { tool: 'nika:write', args: { path: './marker', content: denied } }\n",
            "NIKA-SEC-004",
        ),
        (
            "nika: parent\ntasks:\n  child:\n    invoke: { workflow: './missing.nika.yaml' }\n",
            "NIKA-COMP-001",
        ),
    ] {
        let room = tempfile::tempdir().expect("room");
        std::fs::create_dir(room.path().join("home")).expect("home");
        std::fs::write(room.path().join("bad.nika.yaml"), body).expect("workflow");
        for door in ["check", "run"] {
            let out = call(
                room.path(),
                &[door, "bad.nika.yaml", "--model", "mock/echo"],
            );
            assert_eq!(out.status.code(), Some(2), "{door}: {}", text(&out));
            assert!(text(&out).contains(code), "{door}: {}", text(&out));
            assert!(!room.path().join("marker").exists());
        }
    }
}
