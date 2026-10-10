// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Codex ACP completion profile: codex-acp 1.13.1 and the codex 0.156.1 it bundles.
//!
//! Codex has no empty-tools switch, so the profile closes each tool-bearing surface BEFORE
//! the adapter starts, and proves it on the exact binary the adapter will run:
//!
//! - every tool-bearing feature off (the measured direct-profile list, all present in 0.156.1)
//!   and the non-feature tool surfaces off (web search, agents, plan, user input, skills);
//! - the plugins feature off, then every MCP server the configuration still defines disabled
//!   by name (an empty ACP `mcpServers` list keeps inherited servers, and an empty table
//!   merges instead of clearing);
//! - the same overrides read back on that binary before the spawn: `features list` must show
//!   each feature off and `mcp list` no enabled server, or no session opens;
//! - the adapter receives them as `CODEX_CONFIG` and runs exactly that binary (`CODEX_PATH`);
//! - the ACP mode `read-only` is applied and read back before the prompt: codex-acp sends its
//!   sandbox and approval with every turn from that mode — the fresh scratch directory is the
//!   only writable root, there is no network, and every approval request reaches the client,
//!   which the completion profile always denies.
//!
//! `apply_patch` cannot be unregistered on this pair (the model catalogue registers it
//! whenever the turn has an environment); under `read-only` it can write only inside the
//! scratch, and any tool beat refuses the whole answer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// The adapter identity this profile was built and measured for.
pub(crate) const NAME: &str = "@agentclientprotocol/codex-acp";
/// The exact adapter version (it pins the bundled codex 0.156.1).
pub(crate) const VERSION: &str = "1.13.1";
/// The ACP mode applied and read back before the prompt.
pub(crate) const MODE: &str = "read-only";
/// The variable a conversation's Codex reads Nika's tool server bearer from
/// (`bearer_token_env_var`): the bearer rides the adapter's environment only, never the
/// configuration, a command line or a read-back.
pub(crate) const BEARER_ENV: &str = "NIKA_MCP_BEARER";

/// The configuration the adapter starts with, once `servers` (the MCP servers the user
/// configuration defines) are known: every tool-bearing surface off.
pub(crate) fn config(servers: &[String]) -> Value {
    let features: Map<String, Value> = crate::infer::tool_free::DISABLED_FEATURES
        .iter()
        .map(|feature| ((*feature).to_owned(), Value::Bool(false)))
        .collect();
    let mcp: Map<String, Value> = servers
        .iter()
        .map(|name| (name.clone(), json!({"enabled": false})))
        .collect();
    json!({
        "features": features,
        "web_search": "disabled",
        "agents": {"enabled": false},
        "tools": {
            "update_plan": {"enabled": false},
            "experimental_request_user_input": {"enabled": false}
        },
        "skills": {"include_instructions": false},
        "cloud": {"skills": {"enabled": false}},
        "mcp_servers": mcp
    })
}

/// [`config`] for a conversation: Nika's tool server `server` mounted as `name`, the one MCP
/// server the profile enables.
///
/// # Errors
/// The configuration already defines a server of that name: mounting Nika's under it would
/// merge both definitions.
pub(crate) fn conversation_config(
    servers: &[String],
    name: &str,
    server: Value,
) -> Result<Value, String> {
    if servers.iter().any(|defined| defined == name) {
        return Err(format!(
            "the Codex configuration defines an MCP server named `{name}`, the name Nika's \
             conversation tool server mounts under; rename that server"
        ));
    }
    let mut config = config(servers);
    if let Some(mcp) = config.get_mut("mcp_servers").and_then(Value::as_object_mut) {
        mcp.insert(name.to_owned(), server);
    }
    Ok(config)
}

/// The `-c key=value` overrides equivalent to [`config`] — the SAME object flattened, so the
/// read-back judges exactly what the adapter receives.
pub(crate) fn overrides(config: &Value) -> Vec<String> {
    let mut out = Vec::new();
    flatten("", config, &mut out);
    out
}

fn flatten(prefix: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let key = toml_key(key);
                let path = if prefix.is_empty() {
                    key
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&path, child, out);
            }
        }
        Value::String(text) => out.push(format!("{prefix}={}", Value::String(text.clone()))),
        other => out.push(format!("{prefix}={other}")),
    }
}

/// A bare TOML key when it is one, else a quoted one.
fn toml_key(key: &str) -> String {
    if !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        key.to_owned()
    } else {
        Value::String(key.to_owned()).to_string()
    }
}

/// The MCP servers the configuration defines (plugins already off), from `mcp list --json`.
///
/// # Errors
/// The listing is not the documented JSON array of `{name, …}` objects.
pub(crate) fn defined_servers(listing: &[u8]) -> Result<Vec<String>, String> {
    let rows: Vec<Value> = serde_json::from_slice(listing)
        .map_err(|_| "codex mcp list did not answer a JSON array".to_owned())?;
    rows.iter()
        .map(|row| {
            row.get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| "codex mcp list answered a server without a name".to_owned())
        })
        .collect()
}

/// Every MCP server still listed under the profile must be disabled.
///
/// # Errors
/// The servers still enabled, by name.
pub(crate) fn judge_servers(listing: &[u8]) -> Result<(), String> {
    let rows: Vec<Value> = serde_json::from_slice(listing)
        .map_err(|_| "codex mcp list did not answer a JSON array".to_owned())?;
    let enabled: Vec<&str> = rows
        .iter()
        .filter(|row| row.get("enabled").and_then(Value::as_bool) != Some(false))
        .map(|row| {
            row.get("name")
                .and_then(Value::as_str)
                .unwrap_or("(unnamed)")
        })
        .collect();
    if enabled.is_empty() {
        return Ok(());
    }
    Err(format!(
        "MCP servers still enabled under the profile: {}",
        enabled.join(" · ")
    ))
}

/// Under the conversation profile exactly one MCP server is enabled: Nika's, named `name`.
///
/// # Errors
/// The servers enabled instead, by name.
pub(crate) fn judge_conversation(listing: &[u8], name: &str) -> Result<(), String> {
    let rows: Vec<Value> = serde_json::from_slice(listing)
        .map_err(|_| "codex mcp list did not answer a JSON array".to_owned())?;
    let enabled: Vec<&str> = (rows.iter())
        .filter(|row| row.get("enabled").and_then(Value::as_bool) != Some(false))
        .map(|row| {
            row.get("name")
                .and_then(Value::as_str)
                .unwrap_or("(unnamed)")
        })
        .collect();
    if enabled == [name] {
        return Ok(());
    }
    Err(format!(
        "the conversation profile enables exactly Nika's MCP server `{name}`; enabled: {}",
        if enabled.is_empty() {
            "none".to_owned()
        } else {
            enabled.join(" · ")
        }
    ))
}

/// The codex binary the adapter bundles, next to its own entry point in the npm package
/// (`<package>/dist/index.js` → `<package>/node_modules/@openai/codex/bin/codex.js`). The
/// adapter command is resolved on the child's PATH, symlinks followed.
///
/// # Errors
/// The adapter or its bundled codex is not where the measured package puts them.
pub(crate) fn bundled_codex(
    command: &str,
    env: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let entry = resolve(command, env.get("PATH").map(String::as_str))
        .ok_or_else(|| format!("`{command}` is not on the adapter PATH"))?;
    let entry = std::fs::canonicalize(&entry)
        .map_err(|error| format!("cannot resolve `{}`: {error}", entry.display()))?;
    let package = entry
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| format!("`{}` sits in no package", entry.display()))?;
    let codex = package
        .join("node_modules")
        .join("@openai")
        .join("codex")
        .join("bin")
        .join("codex.js");
    if codex.is_file() {
        Ok(codex)
    } else {
        Err(format!(
            "codex-acp bundles no codex at `{}` (the measured npm layout)",
            codex.display()
        ))
    }
}

fn resolve(command: &str, path: Option<&str>) -> Option<PathBuf> {
    let direct = Path::new(command);
    if direct.components().count() > 1 {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    std::env::split_paths(path?)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests;
