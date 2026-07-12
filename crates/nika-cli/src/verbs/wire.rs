// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika wire` — consent-based agent/editor MCP wiring.
//!
//! Installers stay conservative and never mutate editor state implicitly. This
//! verb is the explicit, idempotent bridge: it adds or repairs the Nika MCP
//! entry while preserving every unrelated server in the host config.

use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde_json::{Map, Value};

use crate::verbs::VerbOutput;

const NIKA_MCP_ARGS: [&str; 1] = ["mcp"];

/// One variant per client `nika wire` knows how to write — the clap
/// surface IS this enum (`ValueEnum` · kebab-case), so a new client lands
/// in this one file: variant + writer arm + tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum WireTarget {
    Cursor,
    Vscode,
    Windsurf,
    Claude,
    ClaudeDesktop,
    Cline,
    Codex,
    Continue,
    Zed,
    Opencode,
    Hermes,
    Gemini,
    Qwen,
    Lmstudio,
    Junie,
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
        // Every concrete client, in declaration order (the clap registry
        // is the one list — no hand-maintained twin to drift).
        WireTarget::All => WireTarget::value_variants()
            .iter()
            .copied()
            .filter(|t| *t != WireTarget::All)
            .collect(),
        other => vec![other],
    }
}

fn wire_one(target: WireTarget, dir: &str) -> Result<WireAction, String> {
    match target {
        // The user-home `mcpServers` family — one shape, per-client paths
        // (gemini: shared settings.json, server names must not contain `_`
        // — upstream policy parser splits on it, `nika` is safe · qwen: the
        // gemini-cli fork, same key, its own dotdir).
        WireTarget::Cursor => patch_home_mcp(&[".cursor", "mcp.json"], "cursor"),
        WireTarget::Claude => patch_home_mcp(&[".claude.json"], "claude"),
        WireTarget::Windsurf => {
            patch_home_mcp(&[".codeium", "windsurf", "mcp_config.json"], "windsurf")
        }
        WireTarget::Gemini => patch_home_mcp(&[".gemini", "settings.json"], "gemini"),
        WireTarget::Qwen => patch_home_mcp(&[".qwen", "settings.json"], "qwen"),
        WireTarget::Vscode => patch_vscode(&Path::new(dir).join(".vscode").join("mcp.json")),
        // Claude DESKTOP is a different app with a different config file
        // than Claude Code's ~/.claude.json — the wave-3 double gap (#449).
        WireTarget::ClaudeDesktop => patch_cursor_like(
            &claude_desktop_config_path()?,
            "mcpServers",
            "claude-desktop",
            false,
        ),
        // Cline hot-reloads its settings file (chokidar watcher), shape =
        // Cursor-style `mcpServers` record — the resolver below picks the
        // live globalStorage file when a host IDE has it, else the stable
        // `~/.cline/data/settings/` home (#449 wave-3b).
        WireTarget::Cline => {
            patch_cursor_like(&cline_settings_path()?, "mcpServers", "cline", false)
        }
        WireTarget::Codex => patch_codex(&home_path(&[".codex", "config.toml"])?),
        // Continue scans `~/.continue/mcpServers/*.json` (the Claude-Desktop
        // JSON shape, name-keyed) — an OWN-FILE write: the user's
        // comment-bearing config.yaml is never touched (the Zed lesson,
        // applied preemptively). External writes are NOT hot-reloaded —
        // the hint rides the verdict line (#449 wave-3b).
        WireTarget::Continue => {
            let path = home_path(&[".continue", "mcpServers", "nika.json"])?;
            patch_cursor_like(&path, "mcpServers", "continue", false)
                .map(|a| with_hint(a, "reload Continue (or re-save config.yaml) to pick it up"))
        }
        // Zed keeps its settings under ~/.config on EVERY platform (macOS
        // included — deliberate upstream choice, zed.dev/docs).
        WireTarget::Zed => patch_zed(&home_path(&[".config", "zed", "settings.json"])?),
        // OpenCode merges the PROJECT-local opencode.json over the global
        // one (opencode.ai/docs/mcp-servers) — the project file is the
        // least-privilege home (the repo the oracle serves).
        WireTarget::Opencode => patch_opencode(&Path::new(dir).join("opencode.json")),
        WireTarget::Hermes => patch_hermes(&home_path(&[".hermes", "config.yaml"])?),
        WireTarget::Lmstudio => {
            patch_cursor_like(&lmstudio_mcp_path()?, "mcpServers", "lmstudio", false)
        }
        // Junie reads project-scope `.junie/mcp/mcp.json` (`mcpServers`
        // root · junie-cli-mcp-configuration.html); the global
        // `~/.junie/mcp/mcp.json` exists but the project file is the
        // least-privilege home, same reasoning as OpenCode above.
        WireTarget::Junie => patch_cursor_like(
            &Path::new(dir).join(".junie").join("mcp").join("mcp.json"),
            "mcpServers",
            "junie",
            false,
        ),
        #[allow(
            clippy::unreachable,
            reason = "All is expanded to concrete targets before dispatch"
        )]
        WireTarget::All => unreachable!("WireTarget::All must be expanded before dispatch"),
    }
}

/// LM Studio documents `~/.lmstudio/mcp.json` (blog v0.3.17 · macOS+Linux ·
/// `%USERPROFILE%\.lmstudio` on Windows), but on some macOS installs the app
/// actually keeps it under `~/.cache/lm-studio/` (lmstudio-bug-tracker#1371,
/// open) — writing the documented path there would wire nothing. Resolution:
/// whichever app-created directory EXISTS wins, documented location first;
/// neither existing falls back to the documented default (created on write).
fn lmstudio_mcp_path() -> Result<PathBuf, String> {
    Ok(lmstudio_mcp_path_from(&home_path(&[])?))
}

/// Claude Desktop keeps `claude_desktop_config.json` under the app-config
/// dir (modelcontextprotocol.io quickstart/user · the one client with an
/// official Anthropic connectors directory): macOS
/// `~/Library/Application Support/Claude/` · Windows `%APPDATA%\Claude\` ·
/// Linux `~/.config/Claude/` (community builds — same dir the app uses).
#[allow(clippy::disallowed_methods)]
fn claude_desktop_config_path() -> Result<PathBuf, String> {
    if cfg!(target_os = "windows")
        && let Some(appdata) = std::env::var_os("APPDATA")
    {
        return Ok(PathBuf::from(appdata)
            .join("Claude")
            .join("claude_desktop_config.json"));
    }
    Ok(claude_desktop_config_path_from(&home_path(&[])?))
}

fn claude_desktop_config_path_from(home: &Path) -> PathBuf {
    let dir = if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Claude")
    } else {
        home.join(".config").join("Claude")
    };
    dir.join("claude_desktop_config.json")
}

/// Cline's settings live in TWO generations of home (cline/cline v4.0.8 vs
/// main): today's extension keeps `settings/cline_mcp_settings.json` under
/// the HOST IDE's globalStorage (`saoudrizwan.claude-dev` — the dir name is
/// stable across VS Code · Cursor · Windsurf · `VSCodium` · Insiders); the
/// CLI already reads — and the next extension release migrates to — the
/// stable `~/.cline/data/settings/cline_mcp_settings.json`. Resolution:
/// a host IDE's EXISTING extension dir wins (the live, hot-reloaded file);
/// none existing falls back to the stable home (created on write — correct
/// for the CLI today and the extension after its migration).
fn cline_settings_path() -> Result<PathBuf, String> {
    Ok(cline_settings_path_from(&home_path(&[])?))
}

fn cline_settings_path_from(home: &Path) -> PathBuf {
    const HOSTS: [&str; 5] = ["Code", "Cursor", "Windsurf", "VSCodium", "Code - Insiders"];
    for host in HOSTS {
        let root = if cfg!(target_os = "macos") {
            home.join("Library").join("Application Support").join(host)
        } else {
            home.join(".config").join(host)
        };
        let ext = root
            .join("User")
            .join("globalStorage")
            .join("saoudrizwan.claude-dev");
        if ext.is_dir() {
            return ext.join("settings").join("cline_mcp_settings.json");
        }
    }
    home.join(".cline")
        .join("data")
        .join("settings")
        .join("cline_mcp_settings.json")
}

/// Append a post-write hint to a SUCCESSFUL write verdict (`Created` /
/// `Updated`) — for clients that do not hot-reload external writes.
/// `Current` stays bare (already wired, nothing to reload).
fn with_hint(action: WireAction, hint: &str) -> WireAction {
    match action {
        WireAction::Created(label) => WireAction::Created(format!("{label} — {hint}")),
        WireAction::Updated(label) => WireAction::Updated(format!("{label} — {hint}")),
        other => other,
    }
}

fn lmstudio_mcp_path_from(home: &Path) -> PathBuf {
    let documented = home.join(".lmstudio");
    let cache = home.join(".cache").join("lm-studio");
    if !documented.is_dir() && cache.is_dir() {
        return cache.join("mcp.json");
    }
    documented.join("mcp.json")
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

/// The user-home `mcpServers` shape shared by the cursor-like family.
fn patch_home_mcp(parts: &[&str], label: &str) -> Result<WireAction, String> {
    patch_cursor_like(&home_path(parts)?, "mcpServers", label, false)
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

/// `OpenCode` wires MCP through `opencode.json` — its OWN shape (`mcp.nika`
/// with `type: local` + the WHOLE argv in `command`), not the
/// `mcpServers` family — so it gets a dedicated desired-value while
/// reusing the same read/merge/write mechanics (#330).
fn patch_opencode(path: &Path) -> Result<WireAction, String> {
    let existed = path.exists();
    let mut root = if existed {
        read_json(path)?
    } else {
        Value::Object(Map::new())
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| format!("opencode: {} is not a JSON object", path.display()))?;
    let servers_value = object
        .entry("mcp".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !servers_value.is_object() {
        *servers_value = Value::Object(Map::new());
    }
    let servers = servers_value
        .as_object_mut()
        .ok_or_else(|| "opencode: mcp is not a JSON object".to_owned())?;

    let desired = serde_json::json!({
        "type": "local",
        "command": ["nika", "mcp"],
        "enabled": true,
    });
    let existing = servers.get("nika").cloned();
    if existing.as_ref() == Some(&desired) {
        return Ok(WireAction::Current(format!("opencode: {}", path.display())));
    }
    servers.insert("nika".to_owned(), desired);
    write_json(path, &root)?;
    let label_path = format!("opencode: {}", path.display());
    if existed {
        Ok(WireAction::Updated(label_path))
    } else {
        Ok(WireAction::Created(label_path))
    }
}

/// The exact Hermes MCP block (`~/.hermes/config.yaml` ·
/// `hermes_cli/mcp_config.py` accepts any PATH binary · `/reload-mcp`
/// refreshes).
const HERMES_SNIPPET: &str =
    "mcp_servers:\n  nika:\n    command: nika\n    args: [mcp]\n    timeout: 120\n";

/// Hermes reads `~/.hermes/config.yaml` — YAML, where user files carry
/// comments and anchors no serializer round-trips. Same contract as Zed
/// (the JSONC precedent): CREATE the file when missing, recognize a
/// CURRENT entry, otherwise hand back the exact snippet
/// ([`WireAction::Manual`]) and leave the user's file byte-identical.
fn patch_hermes(path: &Path) -> Result<WireAction, String> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(path, HERMES_SNIPPET).map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(WireAction::Created(format!("hermes: {}", path.display())));
    }
    let body = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // CURRENT means the canonical block is present verbatim-modulo-indent:
    // a `nika:` server whose command line names the binary. Anything else
    // is the operator\u2019s file to edit \u2014 print the snippet, touch nothing.
    let has_nika = body.contains("nika:") && body.contains("command: nika");
    if has_nika {
        return Ok(WireAction::Current(format!("hermes: {}", path.display())));
    }
    Ok(WireAction::Manual(format!(
        "hermes: {} exists — add under `mcp_servers:` (then /reload-mcp):\n{}",
        path.display(),
        HERMES_SNIPPET
    )))
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

    /// #330 · opencode: its OWN shape (`mcp.nika` · type local · argv in
    /// `command`) — created project-local, idempotent on re-run, other
    /// servers preserved.
    #[test]
    fn opencode_project_config_created_and_idempotent() {
        let dir = temp_dir("opencode");
        let path = dir.join("opencode.json");
        let first = patch_opencode(&path).expect("create");
        assert!(matches!(first, WireAction::Created(_)), "{first:?}");
        let body: Value = read_json(&path).expect("json");
        assert_eq!(body["mcp"]["nika"]["type"], "local");
        assert_eq!(body["mcp"]["nika"]["command"][0], "nika");
        assert_eq!(body["mcp"]["nika"]["command"][1], "mcp");
        assert_eq!(body["mcp"]["nika"]["enabled"], true);
        let second = patch_opencode(&path).expect("re-run");
        assert!(matches!(second, WireAction::Current(_)), "{second:?}");
    }

    #[test]
    fn opencode_preserves_other_servers_and_repairs_nika() {
        let dir = temp_dir("opencode-merge");
        let path = dir.join("opencode.json");
        std::fs::write(
            &path,
            r#"{"mcp":{"other":{"type":"remote"},"nika":{"type":"local","command":["nika","mcp","serve"]}},"theme":"dark"}"#,
        )
        .expect("seed");
        let action = patch_opencode(&path).expect("repair");
        assert!(matches!(action, WireAction::Updated(_)), "{action:?}");
        let body: Value = read_json(&path).expect("json");
        assert_eq!(
            body["mcp"]["other"]["type"], "remote",
            "unrelated server kept"
        );
        assert_eq!(body["theme"], "dark", "unrelated key kept");
        assert_eq!(body["mcp"]["nika"]["command"][1], "mcp", "argv repaired");
    }

    /// #330 · hermes: YAML — the Zed contract (create-if-missing ·
    /// current-detect · Manual otherwise, file byte-identical).
    #[test]
    fn hermes_yaml_creates_detects_and_never_rewrites() {
        let dir = temp_dir("hermes");
        let path = dir.join("config.yaml");
        let first = patch_hermes(&path).expect("create");
        assert!(matches!(first, WireAction::Created(_)), "{first:?}");
        let second = patch_hermes(&path).expect("re-run");
        assert!(matches!(second, WireAction::Current(_)), "{second:?}");
        // A pre-existing file WITHOUT our entry: snippet handed back,
        // bytes untouched (comments/anchors are the operator's).
        let foreign = "# my hermes\nmodel: hermes-4\nmcp_servers:\n  other: { command: x }\n";
        std::fs::write(&path, foreign).expect("seed");
        let third = patch_hermes(&path).expect("manual");
        let WireAction::Manual(msg) = &third else {
            unreachable!("expected Manual, got {third:?}");
        };
        assert!(msg.contains("mcp_servers:") && msg.contains("command: nika"));
        let after = std::fs::read_to_string(&path).expect("read");
        assert_eq!(after, foreign, "the operator file stays byte-identical");
    }

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

    /// #384 · gemini: `mcpServers` lives inside the SHARED settings.json —
    /// every unrelated setting survives, stale argv migrates.
    #[test]
    fn gemini_settings_preserves_unrelated_keys_and_migrates_stale() {
        let dir = temp_dir("gemini");
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{
  "theme": "GitHub",
  "autoAccept": false,
  "mcpServers": {
    "context7": { "command": "npx", "args": ["-y", "@upstash/context7-mcp"] },
    "nika": { "command": "nika", "args": ["mcp", "serve", "--stdio"] }
  }
}
"#,
        )
        .expect("fixture");

        let action = patch_cursor_like(&path, "mcpServers", "gemini", false).expect("wire");
        assert!(matches!(action, WireAction::Migrated(_)), "{action:?}");
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["theme"], "GitHub", "unrelated settings preserved");
        assert_eq!(doc["autoAccept"], false, "unrelated settings preserved");
        assert_eq!(doc["mcpServers"]["context7"]["command"], "npx");
        assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

        let again = patch_cursor_like(&path, "mcpServers", "gemini", false).expect("re-run");
        assert!(matches!(again, WireAction::Current(_)));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// #384 · lmstudio: the documented dir wins, the cache dir
    /// (lmstudio-bug-tracker#1371) is honoured only when it is the ONLY
    /// app-created root, and a bare home falls back to the documented
    /// default.
    #[test]
    fn lmstudio_path_resolution_truth_table() {
        let home = temp_dir("lmstudio-home");

        // Bare home — documented default (created on write).
        assert_eq!(
            lmstudio_mcp_path_from(&home),
            home.join(".lmstudio").join("mcp.json")
        );

        // Only the cache root exists — the app lives there, follow it.
        std::fs::create_dir_all(home.join(".cache").join("lm-studio")).expect("cache root");
        assert_eq!(
            lmstudio_mcp_path_from(&home),
            home.join(".cache").join("lm-studio").join("mcp.json")
        );

        // Documented root exists — it wins even beside the cache root.
        std::fs::create_dir_all(home.join(".lmstudio")).expect("documented root");
        assert_eq!(
            lmstudio_mcp_path_from(&home),
            home.join(".lmstudio").join("mcp.json")
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// #384 · lmstudio: dedicated `mcp.json` (Cursor-style `mcpServers`) —
    /// created then idempotent at the resolved path.
    #[test]
    fn lmstudio_config_created_and_idempotent() {
        let home = temp_dir("lmstudio-cfg");
        std::fs::create_dir_all(home.join(".lmstudio")).expect("root");
        let path = lmstudio_mcp_path_from(&home);

        let action = patch_cursor_like(&path, "mcpServers", "lmstudio", false).expect("wire");
        assert!(matches!(action, WireAction::Created(_)), "{action:?}");
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
        assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

        let again = patch_cursor_like(&path, "mcpServers", "lmstudio", false).expect("re-run");
        assert!(matches!(again, WireAction::Current(_)));
        let _ = std::fs::remove_dir_all(home);
    }

    /// #449 wave-3b · cline: a host IDE's EXISTING extension dir wins (the
    /// live hot-reloaded file); bare home falls back to the stable
    /// `~/.cline/data/settings/` (the CLI's today + extension's next home).
    #[test]
    fn cline_path_resolution_truth_table() {
        let home = temp_dir("cline-home");

        // Bare home — the stable cross-generation path (created on write).
        assert_eq!(
            cline_settings_path_from(&home),
            home.join(".cline")
                .join("data")
                .join("settings")
                .join("cline_mcp_settings.json")
        );

        // A host IDE has the extension — its live globalStorage file wins.
        let host_root = if cfg!(target_os = "macos") {
            home.join("Library")
                .join("Application Support")
                .join("Cursor")
        } else {
            home.join(".config").join("Cursor")
        };
        let ext = host_root
            .join("User")
            .join("globalStorage")
            .join("saoudrizwan.claude-dev");
        std::fs::create_dir_all(&ext).expect("ext dir");
        assert_eq!(
            cline_settings_path_from(&home),
            ext.join("settings").join("cline_mcp_settings.json")
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// #449 wave-3b · cline: Cursor-style `mcpServers` record at the
    /// resolved path — created with parents, then idempotent.
    #[test]
    fn cline_config_created_and_idempotent() {
        let home = temp_dir("cline-cfg");
        let path = cline_settings_path_from(&home);

        let action = patch_cursor_like(&path, "mcpServers", "cline", false).expect("wire");
        assert!(matches!(action, WireAction::Created(_)), "{action:?}");
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
        assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

        let again = patch_cursor_like(&path, "mcpServers", "cline", false).expect("re-run");
        assert!(matches!(again, WireAction::Current(_)));
        let _ = std::fs::remove_dir_all(home);
    }

    /// #449 wave-3b · continue: the OWN-FILE drop-dir write
    /// (`~/.continue/mcpServers/nika.json` · Claude-Desktop shape) — the
    /// user's config.yaml is never touched; the reload hint rides Created/
    /// Updated and stays OFF the idempotent re-run.
    #[test]
    fn continue_own_file_created_idempotent_and_hinted() {
        let home = temp_dir("continue");
        let path = home.join(".continue").join("mcpServers").join("nika.json");

        let action = patch_cursor_like(&path, "mcpServers", "continue", false)
            .map(|a| with_hint(a, "reload Continue"))
            .expect("wire");
        assert!(
            matches!(&action, WireAction::Created(label) if label.contains("reload Continue")),
            "created carries the hint: {action:?}"
        );
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
        assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

        let again = patch_cursor_like(&path, "mcpServers", "continue", false)
            .map(|a| with_hint(a, "reload Continue"))
            .expect("re-run");
        assert!(
            matches!(&again, WireAction::Current(label) if !label.contains("reload")),
            "current stays bare: {again:?}"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// #449 · claude-desktop: the app-config dir differs per platform —
    /// macOS `Library/Application Support/Claude/`, elsewhere
    /// `.config/Claude/` — and is NEVER Claude Code's `~/.claude.json`.
    #[test]
    fn claude_desktop_path_is_the_app_config_dir() {
        let home = Path::new("/probe-home");
        let path = claude_desktop_config_path_from(home);
        let expected = if cfg!(target_os = "macos") {
            home.join("Library")
                .join("Application Support")
                .join("Claude")
        } else {
            home.join(".config").join("Claude")
        };
        assert_eq!(path, expected.join("claude_desktop_config.json"));
    }

    /// #449 · claude-desktop: `mcpServers` root (the modelcontextprotocol.io
    /// quickstart shape) — created with parent dirs, then idempotent, other
    /// servers preserved.
    #[test]
    fn claude_desktop_config_created_and_idempotent() {
        let home = temp_dir("claude-desktop");
        let path = claude_desktop_config_path_from(&home);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("app dir");
        std::fs::write(&path, r#"{"mcpServers":{"other":{"command":"x"}}}"#).expect("seed");

        let action = patch_cursor_like(&path, "mcpServers", "claude-desktop", false).expect("wire");
        assert!(matches!(action, WireAction::Updated(_)), "{action:?}");
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
        assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));
        assert_eq!(doc["mcpServers"]["other"]["command"], "x");

        let again =
            patch_cursor_like(&path, "mcpServers", "claude-desktop", false).expect("re-run");
        assert!(matches!(again, WireAction::Current(_)));
        let _ = std::fs::remove_dir_all(home);
    }

    /// #449 · qwen: gemini-cli fork — same `mcpServers` key in a user-scope
    /// `settings.json`, its own `.qwen` dotdir. Created then idempotent.
    #[test]
    fn qwen_settings_created_and_idempotent() {
        let home = temp_dir("qwen");
        let path = home.join(".qwen").join("settings.json");

        let action = patch_cursor_like(&path, "mcpServers", "qwen", false).expect("wire");
        assert!(matches!(action, WireAction::Created(_)), "{action:?}");
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
        assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

        let again = patch_cursor_like(&path, "mcpServers", "qwen", false).expect("re-run");
        assert!(matches!(again, WireAction::Current(_)));
        let _ = std::fs::remove_dir_all(home);
    }

    /// #384 · junie: project-scope `.junie/mcp/mcp.json` — nested dirs
    /// created, `mcpServers` root, idempotent.
    #[test]
    fn junie_project_config_created_nested_and_idempotent() {
        let project = temp_dir("junie");
        let path = project.join(".junie").join("mcp").join("mcp.json");

        let action = patch_cursor_like(&path, "mcpServers", "junie", false).expect("wire");
        assert!(matches!(action, WireAction::Created(_)), "{action:?}");
        let doc = read_json(&path).expect("json");
        assert_eq!(doc["mcpServers"]["nika"]["command"], "nika");
        assert_eq!(doc["mcpServers"]["nika"]["args"], json!(["mcp"]));

        let again = patch_cursor_like(&path, "mcpServers", "junie", false).expect("re-run");
        assert!(matches!(again, WireAction::Current(_)));
        let _ = std::fs::remove_dir_all(project);
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
