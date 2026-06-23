// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika wire` — consent-based agent/editor MCP wiring.
//!
//! Installers stay conservative and never mutate editor state implicitly. This
//! verb is the explicit, idempotent bridge: it adds or repairs the Nika MCP
//! entry while preserving every unrelated server in the host config.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::verbs::VerbOutput;

const NIKA_MCP_ARGS: [&str; 1] = ["mcp"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireTarget {
    Cursor,
    Vscode,
    Windsurf,
    Claude,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WireAction {
    Created(String),
    Updated(String),
    Migrated(String),
    Current(String),
    Skipped(String),
}

#[must_use]
pub fn run(target: WireTarget, dir: &str) -> VerbOutput {
    let mut actions = Vec::new();
    let mut failed = false;

    for one in expand_target(target) {
        match wire_one(one, dir) {
            Ok(action) => actions.push(action),
            Err(message) => {
                failed = true;
                actions.push(WireAction::Skipped(message));
            }
        }
    }

    let text = render(&actions);
    if failed {
        VerbOutput::env(text)
    } else {
        VerbOutput::ok(text)
    }
}

fn expand_target(target: WireTarget) -> Vec<WireTarget> {
    match target {
        WireTarget::All => vec![
            WireTarget::Cursor,
            WireTarget::Vscode,
            WireTarget::Windsurf,
            WireTarget::Claude,
        ],
        other => vec![other],
    }
}

fn wire_one(target: WireTarget, dir: &str) -> Result<WireAction, String> {
    match target {
        WireTarget::Cursor => patch_cursor_like(
            &home_path(&[".cursor", "mcp.json"])?,
            "mcpServers",
            "cursor",
            false,
        ),
        WireTarget::Vscode => patch_vscode(&Path::new(dir).join(".vscode").join("mcp.json")),
        WireTarget::Windsurf => patch_cursor_like(
            &home_path(&[".codeium", "windsurf", "mcp_config.json"])?,
            "mcpServers",
            "windsurf",
            false,
        ),
        WireTarget::Claude => patch_cursor_like(
            &home_path(&[".claude.json"])?,
            "mcpServers",
            "claude",
            false,
        ),
        WireTarget::All => unreachable!("expanded before dispatch"),
    }
}

fn patch_vscode(path: &Path) -> Result<WireAction, String> {
    patch_config(path, "servers", "vscode", true)
}

fn patch_cursor_like(
    path: &Path,
    server_key: &str,
    label: &str,
    include_type: bool,
) -> Result<WireAction, String> {
    patch_config(path, server_key, label, include_type)
}

fn patch_config(
    path: &Path,
    server_key: &str,
    label: &str,
    include_type: bool,
) -> Result<WireAction, String> {
    let existed = path.exists();
    let mut root = if existed {
        read_json(path)?
    } else {
        Value::Object(Map::new())
    };

    let object = root
        .as_object_mut()
        .ok_or_else(|| format!("{label}: {} is not a JSON object", path.display()))?;
    let servers_value = object
        .entry(server_key.to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !servers_value.is_object() {
        *servers_value = Value::Object(Map::new());
    }
    let servers = servers_value
        .as_object_mut()
        .ok_or_else(|| format!("{label}: {server_key} is not a JSON object"))?;

    let existing = servers.get("nika").cloned();
    let desired = nika_server(include_type);
    let migrated = existing.as_ref().is_some_and(is_stale_server);
    let current = existing.as_ref().is_some_and(|value| *value == desired);

    if current {
        return Ok(WireAction::Current(format!("{label}: {}", path.display())));
    }

    servers.insert("nika".to_owned(), desired);
    write_json(path, &root)?;

    let label_path = format!("{label}: {}", path.display());
    if migrated {
        Ok(WireAction::Migrated(label_path))
    } else if existed {
        Ok(WireAction::Updated(label_path))
    } else {
        Ok(WireAction::Created(label_path))
    }
}

fn nika_server(include_type: bool) -> Value {
    let mut server = Map::new();
    if include_type {
        server.insert("type".to_owned(), Value::String("stdio".to_owned()));
    }
    server.insert("command".to_owned(), Value::String("nika".to_owned()));
    server.insert(
        "args".to_owned(),
        Value::Array(
            NIKA_MCP_ARGS
                .iter()
                .map(|arg| Value::String((*arg).to_owned()))
                .collect(),
        ),
    );
    Value::Object(server)
}

fn is_stale_server(value: &Value) -> bool {
    let Some(args) = value.get("args").and_then(Value::as_array) else {
        return false;
    };
    args.len() == 3
        && args[0].as_str() == Some("mcp")
        && args[1].as_str() == Some("serve")
        && args[2].as_str() == Some("--stdio")
}

fn read_json(path: &Path) -> Result<Value, String> {
    let body = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&body).map_err(|e| format!("{}: malformed JSON: {e}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(value)
        .map_err(|e| format!("{}: cannot encode JSON: {e}", path.display()))?;
    std::fs::write(path, format!("{body}\n")).map_err(|e| format!("{}: {e}", path.display()))
}

#[allow(clippy::disallowed_methods)]
fn home_path(parts: &[&str]) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| "cannot find HOME/USERPROFILE for editor wiring".to_owned())?;
    Ok(parts.iter().fold(home, |path, part| path.join(part)))
}

fn render(actions: &[WireAction]) -> String {
    let mut lines = Vec::new();
    for action in actions {
        let line = match action {
            WireAction::Created(path) => format!("✔ created {path}"),
            WireAction::Updated(path) => format!("✔ updated {path}"),
            WireAction::Migrated(path) => {
                format!("✔ migrated {path} (`mcp serve --stdio` → `mcp`)")
            }
            WireAction::Current(path) => format!("· current {path}"),
            WireAction::Skipped(message) => format!("✖ skipped {message}"),
        };
        lines.push(line);
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn vscode_config_uses_stdio_and_mcp_arg() {
        let dir = temp_dir("vscode");
        let path = dir.join("vscode-mcp.json");
        let action = patch_vscode(&path).expect("wire");
        assert!(matches!(action, WireAction::Created(_)));
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["servers"]["nika"]["type"], "stdio");
        assert_eq!(doc["servers"]["nika"]["command"], "nika");
        assert_eq!(doc["servers"]["nika"]["args"], json!(["mcp"]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cursor_like_config_preserves_other_servers_and_migrates_stale_args() {
        let dir = temp_dir("cursor");
        let path = dir.join("mcp.json");
        std::fs::write(
            &path,
            r#"{
  "mcpServers": {
    "github": { "command": "gh", "args": ["mcp"] },
    "nika": { "command": "nika", "args": ["mcp", "serve", "--stdio"] }
  }
}
"#,
        )
        .expect("fixture");

        let action = patch_cursor_like(&path, "mcpServers", "cursor", false).expect("wire");
        assert!(matches!(action, WireAction::Migrated(_)));
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["mcpServers"]["github"]["command"], "gh");
        assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn current_config_is_idempotent() {
        let dir = temp_dir("idempotent");
        let path = dir.join("mcp.json");
        let _ = patch_cursor_like(&path, "mcpServers", "cursor", false).expect("wire");
        let action = patch_cursor_like(&path, "mcpServers", "cursor", false).expect("wire");
        assert!(matches!(action, WireAction::Current(_)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[allow(clippy::disallowed_methods)]
    fn temp_dir(name: &str) -> PathBuf {
        let base = std::env::var_os("CARGO_TARGET_TMPDIR").map_or_else(
            || {
                std::env::current_dir()
                    .expect("current dir")
                    .join("target")
                    .join("tmp")
            },
            PathBuf::from,
        );
        let dir = base.join(format!("nika-wire-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}
