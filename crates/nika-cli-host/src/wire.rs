// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika wire` — consent-based agent/editor MCP wiring.
//!
//! Installers stay conservative and never mutate editor state implicitly. This
//! verb is the explicit, idempotent bridge: it adds or repairs the Nika MCP
//! entry while preserving every unrelated server in the host config.
//!
//! The door (H7 · audit UX 2026-07-30): preview (`--dry-run`) → detected
//! (`wire detected` — only what this machine shows) → one client by name.
//! `all` is the advanced door: it rewrites every known client config, so it
//! owes the operator a preview, a live `y` on a terminal, or an explicit
//! `--yes` in a pipe — never a silent sweep.

use std::path::{Path, PathBuf};

use clap::ValueEnum;
use serde_json::{Map, Value};

use crate::output::VerbOutput;

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
    Grok,
    Antigravity,
    Kimi,
    Kiro,
    Copilot,
    Amp,
    /// Only the clients THIS machine shows (the probe's presence truth)
    /// — the recommended door: `wire detected --dry-run`, then
    /// `wire detected`.
    Detected,
    /// Every supported client — the advanced door: preview with
    /// `--dry-run`, confirm live on a terminal, or pass `--yes` in a pipe.
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

/// The `wire` door's knobs (H7 · audit UX 2026-07-30): a preview that
/// never writes, and the consent `all` owes the operator.
#[derive(Clone, Copy, Debug, Default)]
pub struct WireOptions {
    /// Plan only — render created/updated/current/manual per client,
    /// write nothing (the `run --dry-run` law: plan only, zero effects).
    pub dry_run: bool,
    /// Consent to `all` without a prompt (scripts · CI — a terminal asks).
    pub yes: bool,
    /// Both ends of the conversation are a terminal: `all` may ask
    /// instead of refusing (the clig.dev interactivity rule).
    pub interactive: bool,
}

/// The compat door (init's injected wire effect): a terminal may consent
/// live, a pipe may not — everything else keeps the scripted defaults.
#[must_use]
pub fn run(target: WireTarget, dir: &str) -> VerbOutput {
    run_with(
        target,
        dir,
        WireOptions {
            interactive: interactive_terminal(),
            ..WireOptions::default()
        },
    )
}

/// The full door (H7): `detected` wires only what the machine shows,
/// `dry_run` renders the plan and writes nothing, and `all` — the
/// every-client sweep — owes a preview, a live `y`, or an explicit
/// `--yes`. A bare `all` in a pipe is refused, naming the right door.
#[must_use]
pub fn run_with(target: WireTarget, dir: &str, options: WireOptions) -> VerbOutput {
    if target == WireTarget::All {
        match all_consent(options.dry_run, options.yes, options.interactive) {
            AllConsent::Refuse => {
                return VerbOutput::env(format!(
                    "wire all rewrites {} client configs and nothing asked\n  \
                     preview:     nika wire all --dry-run\n  \
                     recommended: nika wire detected  (only what this machine has)\n  \
                     consent:     nika wire all --yes",
                    expand_target(WireTarget::All).len()
                ));
            }
            AllConsent::Prompt => {
                use std::io::Write as _;
                let targets = expand_target(target);
                let mut out = std::io::stdout().lock();
                let _ = writeln!(out, "{}", render_plan(&collect_plan(&targets, dir)));
                if !ask_consent(&mut out, targets.len()) {
                    return VerbOutput::ok(
                        "aborted · nothing written — `nika wire detected` wires only \
                         what this machine has"
                            .to_owned(),
                    );
                }
            }
            AllConsent::Proceed => {}
        }
    }

    let targets = resolve_targets(target, dir);
    if targets.is_empty() {
        return VerbOutput::ok(
            "no MCP client detected on this machine — `nika wire all --dry-run` \
             previews every supported client · `nika wire <client>` wires one by name"
                .to_owned(),
        );
    }

    let mut actions = Vec::new();
    let mut failed = false;

    for one in targets {
        match wire_one(one, dir, options.dry_run) {
            Ok(action) => actions.push(action),
            Err(message) => {
                failed = true;
                actions.push(WireAction::Skipped(message));
            }
        }
    }

    let text = if options.dry_run {
        render_plan(&actions)
    } else {
        render(&actions)
    };
    if failed {
        VerbOutput::env(text)
    } else {
        VerbOutput::ok(text)
    }
}

/// The `all` consent verdict (pure — the prompt I/O lives in
/// [`run_with`]; the `model pull` confirm-gate precedent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AllConsent {
    /// `--dry-run` (nothing to consent to) or an explicit `--yes`.
    Proceed,
    /// A terminal: show the plan, then ask.
    Prompt,
    /// No terminal, no `--yes`: refuse, naming the right door.
    Refuse,
}

fn all_consent(dry_run: bool, yes: bool, interactive: bool) -> AllConsent {
    if dry_run || yes {
        AllConsent::Proceed
    } else if interactive {
        AllConsent::Prompt
    } else {
        AllConsent::Refuse
    }
}

/// Both ends of a conversation present? (the prompt needs stdin AND a
/// terminal to ask on — the `model pull` rule).
fn interactive_terminal() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Ask on the conversation stream, read one line from stdin — `y`/`yes`
/// applies the plan, anything else (or an unreadable answer) aborts
/// without a write. The prompt rides the SAME `Write` seam the init
/// wizard uses (this crate bans the `eprint!` macros).
fn ask_consent(out: &mut impl std::io::Write, count: usize) -> bool {
    let _ = write!(out, "wire all: apply to {count} client configs? [y/N] ");
    let _ = out.flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    let answer = line.trim();
    answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes")
}

fn resolve_targets(target: WireTarget, dir: &str) -> Vec<WireTarget> {
    match target {
        WireTarget::Detected => detected_targets(dir),
        other => expand_target(other),
    }
}

/// The clients this machine SHOWS — the same presence truth `doctor`
/// reports. probe.rs owns its client list privately, so the wiring
/// registry re-derives it at the same paths through the probe's own
/// [`client_probe_any`](crate::probe::client_probe_any) machinery
/// (one detection semantics — presence only, never a guess).
fn detected_targets(dir: &str) -> Vec<WireTarget> {
    match home_path(&[]) {
        Ok(home) => detected_targets_in(&home, Path::new(dir)),
        Err(_) => Vec::new(),
    }
}

fn detected_targets_in(home: &Path, dir: &Path) -> Vec<WireTarget> {
    let mcp_servers: &[&str; 2] = &["mcpServers", "nika"];
    let probes = [
        crate::probe::client_probe_any(
            "cursor",
            &[
                home.join(".cursor").join("mcp.json"),
                dir.join(".cursor").join("mcp.json"),
            ],
            mcp_servers,
        ),
        crate::probe::client_probe_any(
            "windsurf",
            &[home
                .join(".codeium")
                .join("windsurf")
                .join("mcp_config.json")],
            mcp_servers,
        ),
        crate::probe::client_probe_any("claude", &[home.join(".claude.json")], mcp_servers),
        crate::probe::client_probe_any(
            "zed",
            &[home.join(".config").join("zed").join("settings.json")],
            &["context_servers", "nika"],
        ),
        // Hermes is YAML — the JSON probe never parses it, but DETECTION
        // is presence only, and `present` is exactly that.
        crate::probe::client_probe_any(
            "hermes",
            &[home.join(".hermes").join("config.yaml")],
            &["mcp_servers", "nika"],
        ),
        crate::probe::client_probe_any(
            "vscode",
            &[dir.join(".vscode").join("mcp.json")],
            &["servers", "nika"],
        ),
    ];
    probes
        .iter()
        .filter(|probe| probe.present)
        .filter_map(|probe| WireTarget::from_str(&probe.id, true).ok())
        .collect()
}

/// The plan half of a run: every outcome, zero writes (the interactive
/// `all` preview reads it before asking).
fn collect_plan(targets: &[WireTarget], dir: &str) -> Vec<WireAction> {
    targets
        .iter()
        .map(|one| match wire_one(*one, dir, true) {
            Ok(action) => action,
            Err(message) => WireAction::Skipped(message),
        })
        .collect()
}

fn expand_target(target: WireTarget) -> Vec<WireTarget> {
    match target {
        // Every concrete client, in declaration order (the clap registry
        // is the one list — no hand-maintained twin to drift). `All` and
        // `Detected` are DOORS, not clients.
        WireTarget::All => WireTarget::value_variants()
            .iter()
            .copied()
            .filter(|t| !matches!(t, WireTarget::All | WireTarget::Detected))
            .collect(),
        other => vec![other],
    }
}

fn wire_one(target: WireTarget, dir: &str, dry_run: bool) -> Result<WireAction, String> {
    match target {
        // The user-home `mcpServers` family — one shape, per-client paths
        // (gemini: shared settings.json, server names must not contain `_`
        // — upstream policy parser splits on it, `nika` is safe · qwen: the
        // gemini-cli fork, same key, its own dotdir).
        WireTarget::Cursor => patch_home_mcp(&[".cursor", "mcp.json"], "cursor", dry_run),
        WireTarget::Claude => patch_home_mcp(&[".claude.json"], "claude", dry_run),
        WireTarget::Windsurf => patch_home_mcp(
            &[".codeium", "windsurf", "mcp_config.json"],
            "windsurf",
            dry_run,
        ),
        WireTarget::Gemini => patch_home_mcp(&[".gemini", "settings.json"], "gemini", dry_run),
        WireTarget::Qwen => patch_home_mcp(&[".qwen", "settings.json"], "qwen", dry_run),
        WireTarget::Vscode => {
            patch_vscode(&Path::new(dir).join(".vscode").join("mcp.json"), dry_run)
        }
        // Claude DESKTOP is a different app with a different config file
        // than Claude Code's ~/.claude.json — the wave-3 double gap (#449).
        WireTarget::ClaudeDesktop => patch_cursor_like(
            &claude_desktop_config_path()?,
            "mcpServers",
            "claude-desktop",
            false,
            dry_run,
        ),
        // Cline hot-reloads its settings file (chokidar watcher), shape =
        // Cursor-style `mcpServers` record — the resolver below picks the
        // live globalStorage file when a host IDE has it, else the stable
        // `~/.cline/data/settings/` home (#449 wave-3b).
        WireTarget::Cline => patch_cursor_like(
            &cline_settings_path()?,
            "mcpServers",
            "cline",
            false,
            dry_run,
        ),
        WireTarget::Codex => {
            patch_toml_mcp(&home_path(&[".codex", "config.toml"])?, "codex", dry_run)
        }
        // Continue scans `~/.continue/mcpServers/*.json` (the Claude-Desktop
        // JSON shape, name-keyed) — an OWN-FILE write: the user's
        // comment-bearing config.yaml is never touched (the Zed lesson,
        // applied preemptively). External writes are NOT hot-reloaded —
        // the hint rides the verdict line (#449 wave-3b).
        WireTarget::Continue => {
            let path = home_path(&[".continue", "mcpServers", "nika.json"])?;
            patch_cursor_like(&path, "mcpServers", "continue", false, dry_run)
                .map(|a| with_hint(a, "reload Continue (or re-save config.yaml) to pick it up"))
        }
        // Zed keeps its settings under ~/.config on EVERY platform (macOS
        // included — deliberate upstream choice, zed.dev/docs).
        WireTarget::Zed => patch_zed(&home_path(&[".config", "zed", "settings.json"])?, dry_run),
        // OpenCode merges the PROJECT-local opencode.json over the global
        // one (opencode.ai/docs/mcp-servers) — the project file is the
        // least-privilege home (the repo the oracle serves).
        WireTarget::Opencode => patch_opencode(&Path::new(dir).join("opencode.json"), dry_run),
        WireTarget::Hermes => patch_hermes(&home_path(&[".hermes", "config.yaml"])?, dry_run),
        WireTarget::Lmstudio => patch_cursor_like(
            &lmstudio_mcp_path()?,
            "mcpServers",
            "lmstudio",
            false,
            dry_run,
        ),
        // Junie reads project-scope `.junie/mcp/mcp.json` (`mcpServers`
        // root · junie-cli-mcp-configuration.html); the global
        // `~/.junie/mcp/mcp.json` exists but the project file is the
        // least-privilege home, same reasoning as OpenCode above.
        WireTarget::Junie => patch_cursor_like(
            &Path::new(dir).join(".junie").join("mcp").join("mcp.json"),
            "mcpServers",
            "junie",
            false,
            dry_run,
        ),
        // Grok Build: the Codex TOML shape at `~/.grok/config.toml` — the
        // native table survives a `[compat.claude] mcps = false` toggle.
        WireTarget::Grok => patch_toml_mcp(&home_path(&[".grok", "config.toml"])?, "grok", dry_run),
        // Antigravity (`agy`): standalone mcp_config.json, global under
        // `~/.gemini/config/` (workspace `.agents/` = `nika init` land ·
        // the url→serverUrl rename touches remote servers only).
        WireTarget::Antigravity => patch_home_mcp(
            &[".gemini", "config", "mcp_config.json"],
            "antigravity",
            dry_run,
        ),
        // Kimi Code: two-level `mcp.json`; machine wiring writes the user
        // file (the trust-scoped project level stays the operator's move).
        WireTarget::Kimi => patch_home_mcp(&[".kimi-code", "mcp.json"], "kimi", dry_run),
        // Kiro (the Amazon Q rebrand): `~/.kiro/settings/mcp.json` — the
        // legacy `~/.aws/amazonq/` file is still read but `.kiro` wins.
        WireTarget::Kiro => patch_home_mcp(&[".kiro", "settings", "mcp.json"], "kiro", dry_run),
        // Copilot CLI: the desired matches THEIR writer byte-shape (tools
        // + type local) so a copilot-added entry reads Current, no churn.
        WireTarget::Copilot => {
            patch_copilot(&home_path(&[".copilot", "mcp-config.json"])?, dry_run)
        }
        // Amp: the LITERAL dotted "amp.mcpServers" key at the settings
        // root; a JSONC file gets the snippet (the Zed lossless contract).
        WireTarget::Amp => patch_amp(&home_path(&[".config", "amp", "settings.json"])?, dry_run),
        #[allow(
            clippy::unreachable,
            reason = "doors are expanded to concrete targets before dispatch"
        )]
        WireTarget::All | WireTarget::Detected => {
            unreachable!("wire doors must be resolved before dispatch")
        }
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
fn patch_zed(path: &Path, dry_run: bool) -> Result<WireAction, String> {
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
    if !dry_run {
        write_json(path, &root)?;
    }

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

fn patch_vscode(path: &Path, dry_run: bool) -> Result<WireAction, String> {
    patch_config(path, "servers", "vscode", true, dry_run)
}

/// The user-home `mcpServers` shape shared by the cursor-like family.
fn patch_home_mcp(parts: &[&str], label: &str, dry_run: bool) -> Result<WireAction, String> {
    patch_cursor_like(&home_path(parts)?, "mcpServers", label, false, dry_run)
}

fn patch_cursor_like(
    path: &Path,
    server_key: &str,
    label: &str,
    include_type: bool,
    dry_run: bool,
) -> Result<WireAction, String> {
    patch_config(path, server_key, label, include_type, dry_run)
}

fn patch_config(
    path: &Path,
    server_key: &str,
    label: &str,
    include_type: bool,
    dry_run: bool,
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
    if !dry_run {
        write_json(path, &root)?;
    }

    let label_path = format!("{label}: {}", path.display());
    if migrated {
        Ok(WireAction::Migrated(label_path))
    } else if existed {
        Ok(WireAction::Updated(label_path))
    } else {
        Ok(WireAction::Created(label_path))
    }
}

/// The `[mcp_servers.nika]` TOML family — Codex CLI (`~/.codex/config.toml`)
/// and Grok Build (`~/.grok/config.toml`) share the exact table shape.
/// `toml_edit` keeps the user's comments and unrelated tables intact.
fn patch_toml_mcp(path: &Path, label: &str, dry_run: bool) -> Result<WireAction, String> {
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
        .ok_or_else(|| format!("{label}: mcp_servers is not a TOML table"))?;
    servers.set_implicit(true);

    let existing = servers.get("nika");
    let current = existing.is_some_and(toml_entry_is_current);
    let migrated = existing.is_some_and(toml_entry_is_stale);

    let label_path = format!("{label}: {}", path.display());
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

    if !dry_run {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(path, doc.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    }

    if migrated {
        Ok(WireAction::Migrated(label_path))
    } else if existed {
        Ok(WireAction::Updated(label_path))
    } else {
        Ok(WireAction::Created(label_path))
    }
}

fn toml_args(item: &toml_edit::Item) -> Vec<String> {
    item.get("args")
        .and_then(toml_edit::Item::as_array)
        .map(|args| {
            args.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn toml_entry_is_current(item: &toml_edit::Item) -> bool {
    item.get("command").and_then(toml_edit::Item::as_str) == Some("nika")
        && toml_args(item) == NIKA_MCP_ARGS
}

fn toml_entry_is_stale(item: &toml_edit::Item) -> bool {
    toml_args(item) == ["mcp", "serve", "--stdio"]
}

/// `OpenCode` wires MCP through `opencode.json` — its OWN shape (`mcp.nika`
/// with `type: local` + the WHOLE argv in `command`), not the
/// `mcpServers` family — so it gets a dedicated desired-value while
/// reusing the same read/merge/write mechanics (#330).
fn patch_opencode(path: &Path, dry_run: bool) -> Result<WireAction, String> {
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
    if !dry_run {
        write_json(path, &root)?;
    }
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
fn patch_hermes(path: &Path, dry_run: bool) -> Result<WireAction, String> {
    if !path.exists() {
        if !dry_run {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::write(path, HERMES_SNIPPET).map_err(|e| format!("{}: {e}", path.display()))?;
        }
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

/// Copilot CLI's own `mcp add` shape — `tools: ["*"]` + `type: "local"`
/// alongside command/args. Matching their writer keeps a copilot-added
/// entry `Current` instead of churning it on every `nika wire copilot`.
fn copilot_server() -> Value {
    let mut server = Map::new();
    server.insert(
        "tools".to_owned(),
        Value::Array(vec![Value::String("*".to_owned())]),
    );
    server.insert("type".to_owned(), Value::String("local".to_owned()));
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

fn patch_copilot(path: &Path, dry_run: bool) -> Result<WireAction, String> {
    let existed = path.exists();
    let mut root = if existed {
        read_json(path)?
    } else {
        Value::Object(Map::new())
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| format!("copilot: {} is not a JSON object", path.display()))?;
    let servers_value = object
        .entry("mcpServers".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !servers_value.is_object() {
        *servers_value = Value::Object(Map::new());
    }
    let servers = servers_value
        .as_object_mut()
        .ok_or_else(|| "copilot: mcpServers is not a JSON object".to_owned())?;

    let existing = servers.get("nika").cloned();
    let desired = copilot_server();
    let migrated = existing.as_ref().is_some_and(is_stale_server);
    if existing.as_ref() == Some(&desired) {
        return Ok(WireAction::Current(format!("copilot: {}", path.display())));
    }
    servers.insert("nika".to_owned(), desired);
    if !dry_run {
        write_json(path, &root)?;
    }
    let label_path = format!("copilot: {}", path.display());
    if migrated {
        Ok(WireAction::Migrated(label_path))
    } else if existed {
        Ok(WireAction::Updated(label_path))
    } else {
        Ok(WireAction::Created(label_path))
    }
}

/// Amp's settings may be `.jsonc` (their docs allow it) — same lossless
/// contract as Zed: a file plain-JSON cannot round-trip gets the exact
/// snippet, byte-identical otherwise.
fn patch_amp(path: &Path, dry_run: bool) -> Result<WireAction, String> {
    let existed = path.exists();
    let mut root = if existed {
        match read_json(path) {
            Ok(value) => value,
            Err(_) => {
                return Ok(WireAction::Manual(format!(
                    "amp: {} is JSONC (comments) — add this under \
                     \"amp.mcpServers\" yourself:\n  \"nika\": {{ \
                     \"command\": \"nika\", \"args\": [\"mcp\"] }}",
                    path.display()
                )));
            }
        }
    } else {
        Value::Object(Map::new())
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| format!("amp: {} is not a JSON object", path.display()))?;
    let servers_value = object
        .entry("amp.mcpServers".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !servers_value.is_object() {
        *servers_value = Value::Object(Map::new());
    }
    let servers = servers_value
        .as_object_mut()
        .ok_or_else(|| "amp: amp.mcpServers is not a JSON object".to_owned())?;

    let existing = servers.get("nika").cloned();
    let desired = nika_server(false);
    let migrated = existing.as_ref().is_some_and(is_stale_server);
    if existing.as_ref() == Some(&desired) {
        return Ok(WireAction::Current(format!("amp: {}", path.display())));
    }
    servers.insert("nika".to_owned(), desired);
    if !dry_run {
        write_json(path, &root)?;
    }
    let label_path = format!("amp: {}", path.display());
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
            WireAction::Manual(message) => format!("✋ manual {message}"),
        };
        lines.push(line);
    }
    lines.join("\n")
}

/// The `--dry-run` render: the SAME per-client outcomes as [`render`],
/// phrased as intent (« would ») under a header that says nothing was
/// written — a preview that reads like a receipt would be a lie (H7).
fn render_plan(actions: &[WireAction]) -> String {
    let mut lines = vec!["dry-run · plan only · nothing written".to_owned()];
    for action in actions {
        let line = match action {
            WireAction::Created(path) => format!("+ would create {path}"),
            WireAction::Updated(path) => format!("+ would update {path}"),
            WireAction::Migrated(path) => {
                format!("+ would migrate {path} (`mcp serve --stdio` → `mcp`)")
            }
            WireAction::Current(path) => format!("· current {path}"),
            WireAction::Skipped(message) => format!("✖ skipped {message}"),
            WireAction::Manual(message) => format!("✋ manual {message}"),
        };
        lines.push(line);
    }
    lines.push("→ re-run without --dry-run to apply".to_owned());
    lines.join("\n")
}

#[cfg(test)]
mod tests;
