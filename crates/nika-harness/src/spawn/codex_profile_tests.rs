// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The Codex profile read back on a scripted bundled codex before any session opens. The script
//! answers `features list` and `mcp list` from the very `-c` overrides it receives, so the judge
//! reads what the adapter would run: a conversation must read Codex's approval route to Nika on,
//! the completion profile must read it off.
use super::*;
use crate::authoring::acp::codex;
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt as _;

/// The measured npm layout (`<package>/dist/index.js`, its bundled
/// `<package>/node_modules/@openai/codex/bin/codex.js`) around a codex that lists each feature
/// as its `-c features.<name>=<bool>` override sets it (`true` when none does), the configuration's
/// own server `olympus` (on until an override disables it), and each server an override mounts.
/// `applies_route: false` is a codex whose route stays off whatever it is told.
fn package(dir: &std::path::Path, applies_route: bool) -> String {
    let entry = dir.join("codex-acp/dist/index.js");
    std::fs::create_dir_all(entry.parent().expect("dist")).expect("dist");
    std::fs::write(&entry, "// the adapter's entry point").expect("entry");
    let script = dir.join("codex-acp/node_modules/@openai/codex/bin/codex.js");
    std::fs::create_dir_all(script.parent().expect("bin")).expect("bin");
    let features = crate::infer::tool_free::DISABLED_FEATURES.join(" ");
    let applies = if applies_route { "True" } else { "False" };
    std::fs::write(
        &script,
        format!(
            r#"#!/usr/bin/env python3
import json, sys
args = sys.argv[1:]
told = dict(args[i + 1].split("=", 1) for i in range(len(args) - 1) if args[i] == "-c")
if not {applies}:
    told["features.{route}"] = "false"
words = [a for i, a in enumerate(args) if a != "-c" and (i == 0 or args[i - 1] != "-c")]
if words[:2] == ["features", "list"]:
    for name in "{features}".split():
        print(name, "stable", told.get("features." + name, "true"))
elif words[:2] == ["mcp", "list"]:
    servers = {{"olympus": True}}
    for key, value in told.items():
        parts = key.split(".")
        if parts[0] == "mcp_servers" and len(parts) == 3 and parts[2] == "enabled":
            servers[parts[1]] = value == "true"
        elif parts[0] == "mcp_servers" and len(parts) == 3 and parts[2] == "url":
            servers.setdefault(parts[1], True)
    print(json.dumps([{{"name": name, "enabled": on}} for name, on in servers.items()]))
"#,
            route = codex::APPROVAL_FEATURE,
        ),
    )
    .expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("mode");
    entry.to_string_lossy().into_owned()
}

fn nika_server() -> Value {
    json!({"url": "http://127.0.0.1:4242/mcp", "bearer_token_env_var": codex::BEARER_ENV})
}

/// The adapter environment's `CODEX_CONFIG`, parsed.
fn config_of(env: &[(String, String)]) -> Value {
    let (_, config) = (env.iter())
        .find(|(name, _)| name == "CODEX_CONFIG")
        .expect("CODEX_CONFIG");
    serde_json::from_str(config).expect("one JSON object")
}

/// A conversation opens on a profile whose approval route to Nika reads back on, beside the
/// configuration's own server disabled and Nika's mounted; the completion profile reads it off.
#[tokio::test]
async fn a_conversation_reads_codexs_approval_route_back_on() {
    let dir = tempfile::tempdir().expect("dir");
    let row = HarnessAdapter::new("codex", package(dir.path(), true)).expect("id");
    let harness = SpawnedHarness::new(row);
    let env = (harness.codex_profile_env(Some(("nika", nika_server()))))
        .await
        .expect("the conversation profile holds");
    let config = config_of(&env);
    assert_eq!(config["features"][codex::APPROVAL_FEATURE], json!(true));
    assert_eq!(config["mcp_servers"]["olympus"], json!({"enabled": false}));
    assert_eq!(config["mcp_servers"]["nika"], nika_server());
    let completion = (harness.codex_profile_env(None))
        .await
        .expect("the completion profile holds");
    let config = config_of(&completion);
    assert_eq!(config["features"][codex::APPROVAL_FEATURE], json!(false));
}

/// A codex whose route stays off opens no conversation, and the refusal names the feature: on
/// such a profile every ask about Nika's tools ends cancelled without reaching Nika (as a live
/// conversation showed on 2026-10-11).
#[tokio::test]
async fn a_route_that_reads_off_opens_no_conversation() {
    let dir = tempfile::tempdir().expect("dir");
    let row = HarnessAdapter::new("codex", package(dir.path(), false)).expect("id");
    let harness = SpawnedHarness::new(row);
    let refused = (harness.codex_profile_env(Some(("nika", nika_server()))))
        .await
        .expect_err("the route reads off");
    let HarnessError::Refused { reason } = refused else {
        panic!("a refusal: {refused:?}");
    };
    assert!(
        reason.contains("tool_call_mcp_elicitation=false"),
        "{reason}"
    );
    (harness.codex_profile_env(None))
        .await
        .expect("the completion profile needs the route off");
}
