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
    Codex,
    Zed,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WireAction {
    Created(String),
    Updated(String),
    Migrated(String),
    Current(String),
    Skipped(String),
    /// The config exists but cannot be rewritten losslessly (Zed's
    /// settings.json is JSONC — comments would be destroyed): hand the
    /// user the exact snippet instead. A successful outcome, not a skip.
    Manual(String),
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
            WireTarget::Codex,
            WireTarget::Zed,
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
        WireTarget::Codex => patch_codex(&home_path(&[".codex", "config.toml"])?),
        // Zed keeps its settings under ~/.config on EVERY platform (macOS
        // included — deliberate upstream choice, zed.dev/docs).
        WireTarget::Zed => patch_zed(&home_path(&[".config", "zed", "settings.json"])?),
        WireTarget::All => unreachable!("expanded before dispatch"),
    }
}

/// Zed reads MCP servers from `context_servers` in `settings.json`
/// (zed.dev/docs/ai/mcp · the `{"command","args","env"}` shape). The file
/// is JSONC — Zed's DEFAULT settings ship with comments — so the contract
/// is: only rewrite a file we can round-trip losslessly (a plain-JSON parse
/// succeeding ⇒ no comments existed); otherwise return
/// [`WireAction::Manual`] with the exact snippet and leave the user's file
/// byte-identical.
fn patch_zed(path: &Path) -> Result<WireAction, String> {
    let existed = path.exists();
    let label_path = format!("zed: {}", path.display());
    let mut root = if existed {
        match read_json(path) {
            Ok(value) => value,
            Err(_) => {
                return Ok(WireAction::Manual(format!(
                    "{label_path} is JSONC (comments) — add this to \
                     \"context_servers\" yourself:\n  \"nika\": {{ \
                     \"command\": \"nika\", \"args\": [\"mcp\"], \"env\": {{}} }}"
                )));
            }
        }
    } else {
        Value::Object(Map::new())
    };

    let object = root
        .as_object_mut()
        .ok_or_else(|| format!("zed: {} is not a JSON object", path.display()))?;
    let servers_value = object
        .entry("context_servers".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !servers_value.is_object() {
        *servers_value = Value::Object(Map::new());
    }
    let servers = servers_value
        .as_object_mut()
        .ok_or_else(|| "zed: context_servers is not a JSON object".to_owned())?;

    let existing = servers.get("nika").cloned();
    let desired = zed_server();
    let migrated = existing.as_ref().is_some_and(is_stale_server);
    if existing.as_ref() == Some(&desired) {
        return Ok(WireAction::Current(label_path));
    }

    servers.insert("nika".to_owned(), desired);
    write_json(path, &root)?;

    if migrated {
        Ok(WireAction::Migrated(label_path))
    } else if existed {
        Ok(WireAction::Updated(label_path))
    } else {
        Ok(WireAction::Created(label_path))
    }
}

/// The documented Zed entry: `command` + `args` + `env` (the `env` key is
/// in the upstream example for command-based servers — kept for fidelity).
fn zed_server() -> Value {
    let mut server = Map::new();
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
    server.insert("env".to_owned(), Value::Object(Map::new()));
    Value::Object(server)
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

/// Codex CLI reads `~/.codex/config.toml` (`[mcp_servers.nika]` · TOML, not
/// JSON). `toml_edit` keeps the user's comments and unrelated tables intact.
fn patch_codex(path: &Path) -> Result<WireAction, String> {
    let existed = path.exists();
    let body = if existed {
        std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?
    } else {
        String::new()
    };
    let mut doc: toml_edit::DocumentMut = body
        .parse()
        .map_err(|e| format!("{}: malformed TOML: {e}", path.display()))?;

    let servers = doc
        .entry("mcp_servers")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(|| "codex: mcp_servers is not a TOML table".to_owned())?;
    servers.set_implicit(true);

    let existing = servers.get("nika");
    let current = existing.is_some_and(codex_entry_is_current);
    let migrated = existing.is_some_and(codex_entry_is_stale);

    let label_path = format!("codex: {}", path.display());
    if current {
        return Ok(WireAction::Current(label_path));
    }

    let mut entry = toml_edit::Table::new();
    entry.insert("command", toml_edit::value("nika"));
    let mut args = toml_edit::Array::new();
    for arg in NIKA_MCP_ARGS {
        args.push(arg);
    }
    entry.insert("args", toml_edit::value(args));
    servers.insert("nika", toml_edit::Item::Table(entry));

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, doc.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;

    if migrated {
        Ok(WireAction::Migrated(label_path))
    } else if existed {
        Ok(WireAction::Updated(label_path))
    } else {
        Ok(WireAction::Created(label_path))
    }
}

fn codex_args(item: &toml_edit::Item) -> Vec<String> {
    item.get("args")
        .and_then(toml_edit::Item::as_array)
        .map(|args| {
            args.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn codex_entry_is_current(item: &toml_edit::Item) -> bool {
    item.get("command").and_then(toml_edit::Item::as_str) == Some("nika")
        && codex_args(item) == NIKA_MCP_ARGS
}

fn codex_entry_is_stale(item: &toml_edit::Item) -> bool {
    codex_args(item) == ["mcp", "serve", "--stdio"]
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
            WireAction::Manual(message) => format!("✋ manual {message}"),
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
    fn codex_config_toml_created_with_mcp_servers_table() {
        let dir = temp_dir("codex-create");
        let path = dir.join("config.toml");
        let action = patch_codex(&path).expect("wire");
        assert!(matches!(action, WireAction::Created(_)));
        let body = std::fs::read_to_string(&path).expect("read");
        let doc: toml_edit::DocumentMut = body.parse().expect("toml");
        assert_eq!(doc["mcp_servers"]["nika"]["command"].as_str(), Some("nika"));
        let args: Vec<&str> = doc["mcp_servers"]["nika"]["args"]
            .as_array()
            .expect("args array")
            .iter()
            .filter_map(toml_edit::Value::as_str)
            .collect();
        assert_eq!(args, ["mcp"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_config_preserves_other_tables_comments_and_is_idempotent() {
        let dir = temp_dir("codex-preserve");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"# my codex setup
model = "gpt-5.2"

[projects."/Users/me/repo"]
trust_level = "trusted"

[mcp_servers.github]
command = "gh"
args = ["mcp"]
"#,
        )
        .expect("fixture");

        let action = patch_codex(&path).expect("wire");
        assert!(matches!(action, WireAction::Updated(_)));
        let body = std::fs::read_to_string(&path).expect("read");
        assert!(body.contains("# my codex setup"), "comment preserved");
        assert!(body.contains("trust_level = \"trusted\""));
        assert!(body.contains("[mcp_servers.github]"));

        let again = patch_codex(&path).expect("wire twice");
        assert!(matches!(again, WireAction::Current(_)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn codex_stale_serve_args_are_migrated() {
        let dir = temp_dir("codex-migrate");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[mcp_servers.nika]\ncommand = \"nika\"\nargs = [\"mcp\", \"serve\", \"--stdio\"]\n",
        )
        .expect("fixture");

        let action = patch_codex(&path).expect("wire");
        assert!(matches!(action, WireAction::Migrated(_)));
        let body = std::fs::read_to_string(&path).expect("read");
        assert!(body.contains("args = [\"mcp\"]"));
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

    #[test]
    fn zed_config_created_under_context_servers() {
        let dir = temp_dir("zed-create");
        let path = dir.join("settings.json");
        let action = patch_zed(&path).expect("wire");
        assert!(matches!(action, WireAction::Created(_)));
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["context_servers"]["nika"]["command"], "nika");
        assert_eq!(doc["context_servers"]["nika"]["args"], json!(["mcp"]));
        assert_eq!(doc["context_servers"]["nika"]["env"], json!({}));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn zed_jsonc_settings_are_never_destroyed() {
        // Zed's settings.json is JSONC — the DEFAULT file ships with comments.
        // A naive parse-rewrite would strip them; the contract is: never
        // rewrite a file we cannot round-trip losslessly — hand the user the
        // exact snippet instead.
        let dir = temp_dir("zed-jsonc");
        let path = dir.join("settings.json");
        let jsonc = "{\n  // Zed settings\n  \"theme\": \"One Dark\"\n}\n";
        std::fs::write(&path, jsonc).expect("fixture");

        let action = patch_zed(&path).expect("wire");
        assert!(
            matches!(
                &action,
                WireAction::Manual(message)
                    if message.contains("context_servers")
                        && message.contains("\"command\": \"nika\"")
            ),
            "expected Manual with the snippet, got {action:?}"
        );
        let body = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            body, jsonc,
            "JSONC file must be byte-identical (never destroyed)"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn zed_preserves_other_context_servers_and_migrates_stale() {
        let dir = temp_dir("zed-migrate");
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{
  "theme": "One Dark",
  "context_servers": {
    "github": { "command": "gh", "args": ["mcp"] },
    "nika": { "command": "nika", "args": ["mcp", "serve", "--stdio"] }
  }
}
"#,
        )
        .expect("fixture");

        let action = patch_zed(&path).expect("wire");
        assert!(matches!(action, WireAction::Migrated(_)));
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["theme"], "One Dark", "unrelated settings preserved");
        assert_eq!(doc["context_servers"]["github"]["command"], "gh");
        assert_eq!(doc["context_servers"]["nika"]["args"], json!(["mcp"]));

        let again = patch_zed(&path).expect("wire twice");
        assert!(matches!(again, WireAction::Current(_)));
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
