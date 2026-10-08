// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Codex profile's pure halves: the configuration and its `-c` twin are
//! the same object, the listings are judged exactly, and the bundled binary
//! is found where the measured npm package puts it.

use std::collections::BTreeMap;

use serde_json::json;

use super::*;

fn servers() -> Vec<String> {
    vec!["olympus".to_owned(), "node_repl".to_owned()]
}

#[test]
fn every_tool_surface_is_off_and_each_configured_server_disabled() {
    let config = config(&servers());
    for feature in crate::infer::tool_free::DISABLED_FEATURES {
        assert_eq!(config["features"][*feature], json!(false), "{feature}");
    }
    assert_eq!(config["web_search"], json!("disabled"));
    assert_eq!(config["agents"]["enabled"], json!(false));
    assert_eq!(config["tools"]["update_plan"]["enabled"], json!(false));
    assert_eq!(
        config["mcp_servers"],
        json!({"olympus":{"enabled":false},"node_repl":{"enabled":false}})
    );
    assert_eq!(config["features"]["plugins"], json!(false));
    assert_eq!(config["features"]["shell_tool"], json!(false));
}

/// The read-back judges exactly what the adapter receives: the overrides
/// are the configuration flattened, with quoted keys where TOML needs them.
#[test]
fn the_overrides_are_the_configuration_flattened() {
    let mut names = servers();
    names.push("dotted.name".to_owned());
    let overrides = overrides(&config(&names));
    for expected in [
        "features.shell_tool=false",
        "features.plugins=false",
        "web_search=\"disabled\"",
        "agents.enabled=false",
        "tools.update_plan.enabled=false",
        "mcp_servers.olympus.enabled=false",
        "mcp_servers.node_repl.enabled=false",
        "mcp_servers.\"dotted.name\".enabled=false",
    ] {
        assert!(
            overrides.iter().any(|o| o == expected),
            "{expected}: {overrides:?}"
        );
    }
    assert!(
        !overrides.iter().any(|o| o.starts_with("mcp_servers=")),
        "an empty table merges instead of clearing: {overrides:?}"
    );
    let features = crate::infer::tool_free::DISABLED_FEATURES.len();
    assert_eq!(overrides.len(), features + 6 + names.len(), "{overrides:?}");
}

#[test]
fn the_listings_are_judged_exactly() {
    let listing =
        br#"[{"name":"olympus","enabled":true,"transport":{}},{"name":"beui","enabled":false}]"#;
    assert_eq!(
        defined_servers(listing).expect("names"),
        ["olympus", "beui"]
    );
    let refused = judge_servers(listing).expect_err("olympus still on");
    assert!(
        refused.contains("olympus") && !refused.contains("beui"),
        "{refused}"
    );
    assert!(judge_servers(br#"[{"name":"olympus","enabled":false}]"#).is_ok());
    assert!(judge_servers(b"[]").is_ok());
    assert!(
        judge_servers(br#"[{"name":"olympus"}]"#).is_err(),
        "no explicit off is on"
    );
    assert!(defined_servers(b"not json").is_err());
    assert!(defined_servers(br#"[{"enabled":true}]"#).is_err());
}

#[cfg(unix)]
#[test]
fn the_bundled_codex_is_found_next_to_the_adapter_entry_point() {
    let root = tempfile::tempdir().expect("tempdir");
    let package = root
        .path()
        .join("lib/node_modules/@agentclientprotocol/codex-acp");
    std::fs::create_dir_all(package.join("dist")).expect("dist");
    std::fs::write(package.join("dist/index.js"), "// adapter").expect("entry");
    let bin = root.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    std::os::unix::fs::symlink(package.join("dist/index.js"), bin.join("codex-acp")).expect("link");
    let env = BTreeMap::from([("PATH".to_owned(), bin.display().to_string())]);
    let missing = bundled_codex("codex-acp", &env).expect_err("no bundled codex yet");
    assert!(missing.contains("bundles no codex"), "{missing}");
    let codex = package.join("node_modules/@openai/codex/bin/codex.js");
    std::fs::create_dir_all(codex.parent().expect("parent")).expect("codex dir");
    std::fs::write(&codex, "// codex").expect("codex");
    let found = bundled_codex("codex-acp", &env).expect("found");
    assert_eq!(
        found,
        std::fs::canonicalize(&codex).expect("canonical"),
        "{}",
        found.display()
    );
    let absent = bundled_codex("codex-acp", &BTreeMap::new()).expect_err("no PATH");
    assert!(absent.contains("not on the adapter PATH"), "{absent}");
}
