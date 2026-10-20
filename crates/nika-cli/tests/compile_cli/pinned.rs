// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An `mcp:` step through the real CLI composition. The binary installs the pin lookup every
//! compile room consults, so a step whose server the project registered and approved, and whose
//! tool its pins hold, is judged on the pinned input schema, while an unapproved one stays held
//! with the exact approval command. The pins are read again on each round and belong to their
//! own project. No MCP server is started: the launcher the registry names would leave a marker
//! in the room, and none appears. Loopback seat only: no provider, credential or paid call.

use super::{LoopbackSeat, command, result};
use serde_json::{Value, json};
use std::path::Path;

const INTENT: &str = "Find the invoice email in my Gmail.";

/// The sketch of one `mcp:` step, its fill, and the approving judge: the whole request is
/// faithful, then its one part is carried (the last answer repeats for any further question).
fn authored() -> Vec<String> {
    vec![
        json!({"name": "invoice-email", "tasks": [{"id": "find_email", "verb": "invoke",
            "tool": "mcp:abench/gmail_find_email", "purpose": "find the invoice email",
            "writes": []}],
            "questions": [], "gaps": [], "notes": "graph"})
        .to_string(),
        json!({"fills": [{"task": "find_email", "field": "args", "value": {"query": "invoice"}}],
            "notes": "fills"})
        .to_string(),
        r#"{"choice":"faithful"}"#.to_owned(),
        r#"{"choice":"carried"}"#.to_owned(),
    ]
}

/// A project that registers `abench`, whose launcher would leave `spawned` in the room if it
/// ever started.
fn registered(room: &Path) {
    registered_as(room, "abench.sh");
}

/// [`registered`], `abench` launching `launcher` (a file of the room): another launcher
/// re-points the same name.
fn registered_as(room: &Path, launcher: &str) {
    std::fs::create_dir_all(room.join(".nika")).expect("the project state directory");
    let launcher = room.join(launcher);
    let marker = room.join("spawned");
    std::fs::write(
        &launcher,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .expect("the launcher");
    let registry = json!({"mcp_servers_format": 1, "servers": {"abench": {
        "command": "/bin/sh", "args": [launcher.to_string_lossy()]}}});
    std::fs::write(room.join(".nika/mcp_servers.json"), registry.to_string())
        .expect("the registry");
}

/// The operator's approval of `abench`, as `nika mcp approve` records it: each tool's snapshot
/// beside its own pin, which the lockfile's reader checks again.
fn approve(room: &Path) {
    approve_as(room, "abench.sh");
}

/// [`approve`] of `abench` launching `launcher`: the identity the approval records.
fn approve_as(room: &Path, launcher: &str) {
    let schema = json!({"type": "object", "properties": {"query": {"type": "string"}},
        "required": ["query"]});
    let pin = nika_mcp::pin::McpToolDef::new("gmail_find_email", "Find an email", schema.clone())
        .pin_hash();
    let launcher = room.join(launcher);
    let pins = json!({"mcp_pins_format": 1, "servers": {"abench": {
        "identity": {"command": "/bin/sh", "args": [launcher.to_string_lossy()]},
        "pinned_at": "2026-10-13T08:00:00Z",
        "tools": {"gmail_find_email": {"pin": pin, "description": "Find an email",
            "inputSchema": schema}}}}});
    std::fs::write(room.join(".nika/mcp_pins.json"), pins.to_string()).expect("the pins");
}

/// One real `nika compile --json --fresh` of [`INTENT`] on the loopback seat: its exit code and
/// its document.
fn compile(room: &Path) -> (Option<i32>, Value) {
    let seat = LoopbackSeat::start(authored());
    let out = command(room)
        .env("NIKA_VLLM_BASE_URL", seat.base())
        .args(["compile", INTENT, "--authoring-model", "vllm/loopback-seat"])
        .args(["--authoring-strategy", "sketch", "--authoring-timeout", "2"])
        .args(["--fresh", "--json"])
        .output()
        .expect("CLI");
    (out.status.code(), result(&out))
}

/// The judge's record of the one `mcp:` tool of the one task.
fn verdict(doc: &Value) -> Value {
    doc["provenance"]["decision"]["mcp"][0]["tools"][0].clone()
}

#[test]
fn the_binary_judges_a_pinned_mcp_step_and_holds_an_unapproved_one() {
    let room = tempfile::tempdir().expect("room");
    registered(room.path());

    // Registered, never approved: held, with the exact approval command, and recorded.
    let (code, doc) = compile(room.path());
    assert_eq!(code, Some(2), "held, never READY: {doc}");
    assert_eq!(doc["status"], "incomplete", "{doc}");
    let held = verdict(&doc);
    assert_eq!(held["verdict"], "held", "{doc}");
    let reason = held["reason"].as_str().unwrap_or_default();
    assert!(reason.contains("nika mcp approve abench"), "{doc}");
    assert!(
        doc["diagnostics"]
            .to_string()
            .contains("nika mcp approve abench"),
        "{doc}"
    );

    // Approved between rounds: the pins are read again, and the step is judged.
    approve(room.path());
    let (code, doc) = compile(room.path());
    assert_eq!(doc["status"], "ready", "nothing else is unsettled: {doc}");
    assert_eq!(code, Some(0), "{doc}");
    let judged = verdict(&doc);
    assert_eq!(judged["verdict"], "judged", "{doc}");
    assert_eq!(judged["approved_at"], "2026-10-13T08:00:00Z", "{doc}");

    // Another project shares no approval: its step stays held.
    let other = tempfile::tempdir().expect("another room");
    registered(other.path());
    let (code, doc) = compile(other.path());
    assert_eq!(code, Some(2), "{doc}");
    assert_eq!(verdict(&doc)["verdict"], "held", "{doc}");

    // No compile started an MCP server.
    assert!(!room.path().join("spawned").exists(), "no server started");
    assert!(!other.path().join("spawned").exists(), "no server started");
}

/// The registry re-pointed after approval: `abench` now launches another program than the one
/// approved, so the approval does not carry over, as the run's own verification refuses it. The
/// step stays held with the approval command; approving the new launcher judges it. No compile
/// starts either program.
#[test]
fn a_repointed_registry_keeps_the_hold_until_approved_again() {
    let room = tempfile::tempdir().expect("room");
    registered(room.path());
    approve(room.path());
    registered_as(room.path(), "abench-next.sh");

    let (code, doc) = compile(room.path());
    assert_eq!(code, Some(2), "held, never READY: {doc}");
    assert_eq!(doc["status"], "incomplete", "{doc}");
    let held = verdict(&doc);
    assert_eq!(held["verdict"], "held", "{doc}");
    let reason = held["reason"].as_str().unwrap_or_default();
    assert!(reason.contains("nika mcp approve abench"), "{doc}");

    approve_as(room.path(), "abench-next.sh");
    let (code, doc) = compile(room.path());
    assert_eq!(doc["status"], "ready", "{doc}");
    assert_eq!(code, Some(0), "{doc}");
    assert_eq!(verdict(&doc)["verdict"], "judged", "{doc}");

    assert!(!room.path().join("spawned").exists(), "no server started");
}
