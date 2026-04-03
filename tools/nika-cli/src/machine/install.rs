//! Editor detection, rule installation, hash protection, completions, and quick scan.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use colored::Colorize;

use nika_engine::display::StatusIcon;

use super::status::{machine_toml_path, write_marker, SetupResult};

// ─── Editor Definitions ─────────────────────────────────────────────────────

/// Definition of a VS Code-family editor for extension management.
pub struct EditorDef {
    /// Slug used in machine.toml (e.g., "vscode", "cursor")
    pub id: &'static str,
    /// Human-readable name (e.g., "VS Code", "Cursor")
    pub name: &'static str,
    /// CLI binary name (e.g., "code", "cursor")
    pub binary: &'static str,
    /// Extension ID on marketplace
    pub ext_id: &'static str,
}

/// All VS Code-family editors that support the nika-lang extension.
pub const VSCODE_EDITORS: &[EditorDef] = &[
    EditorDef { id: "vscode", name: "VS Code", binary: "code", ext_id: "supernovae.nika-lang" },
    EditorDef { id: "cursor", name: "Cursor", binary: "cursor", ext_id: "supernovae.nika-lang" },
    EditorDef {
        id: "windsurf",
        name: "Windsurf",
        binary: "windsurf",
        ext_id: "supernovae.nika-lang",
    },
];

/// Query the installed version of an extension for a given editor CLI.
/// Returns `None` if extension is not installed or CLI fails.
pub fn query_extension_version(cli: &Path, ext_id: &str) -> Option<String> {
    let output = Command::new(cli)
        .args(["--list-extensions", "--show-versions"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let list = String::from_utf8(output.stdout).ok()?;
    let prefix = format!("{}@", ext_id.to_lowercase());
    list.lines().find_map(|l| {
        let lower = l.trim().to_lowercase();
        lower.strip_prefix(&prefix).map(|v| v.to_string())
    })
}

/// Returns true if extension version is significantly behind the CLI version.
/// Compares major.minor — patch differences are OK.
pub fn is_version_outdated(ext_ver: &str, cli_ver: &str) -> bool {
    let parse = |v: &str| -> (u32, u32) {
        let parts: Vec<&str> = v.split('.').collect();
        let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        (major, minor)
    };
    let (ext_major, ext_minor) = parse(ext_ver);
    let (cli_major, cli_minor) = parse(cli_ver);
    (ext_major, ext_minor) < (cli_major, cli_minor)
}

// ─── Editor Detection ────────────────────────────────────────────────────────

/// Detect which AI-capable editors are installed on this machine.
///
/// Returns a list of editor IDs (lowercase slugs) used as keys for rule
/// installation and tracking in machine.toml.
pub(super) fn detect_editors() -> Vec<&'static str> {
    let mut editors = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // VS Code
    if which::which("code").is_ok() || check_macos_app("VS Code") {
        editors.push("vscode");
    }
    // Cursor
    if which::which("cursor").is_ok() || home.join(".cursor").exists() || check_macos_app("Cursor")
    {
        editors.push("cursor");
    }
    // Claude Code
    if which::which("claude").is_ok() || home.join(".claude").exists() {
        editors.push("claude");
    }
    // Windsurf
    if which::which("windsurf").is_ok() || check_macos_app("Windsurf") {
        editors.push("windsurf");
    }
    // Roo Code
    if home.join(".roo").exists() {
        editors.push("roo");
    }
    // Copilot (via GitHub CLI — the only reliable signal)
    if which::which("gh").is_ok() {
        editors.push("copilot");
    }

    editors
}

/// Run the full machine auto-setup (Phase 1).
///
/// Detects editors and AI tools, installs extensions/rules/completions.
/// Prints progress as it goes. Writes marker file on success.
/// This is SILENT by design — no questions asked, just detect and install.
pub fn run_machine_setup() -> Vec<SetupResult> {
    let mut results = Vec::new();

    // 1. Editors: detect + install extension
    results.extend(setup_editors());

    // 2. AI tools: detect + install rules
    results.extend(setup_ai_rules());

    // 3. Shell completions
    results.push(setup_completions());

    // 4. Daemon service: install + start (Unix only, opt-out via NIKA_NO_DAEMON=1)
    #[cfg(unix)]
    if std::env::var("NIKA_NO_DAEMON").is_err() {
        results.push(setup_daemon());
    }

    // Write marker file
    write_marker(&results);

    // Summary: show each configured tool by name (deduplicated)
    let mut seen = std::collections::HashSet::new();
    let ok_results: Vec<&SetupResult> = results
        .iter()
        .filter(|r| r.success && seen.insert(r.name.as_str()))
        .collect();
    if ok_results.is_empty() {
        println!("  {} No editors detected", "\u{25cb}".dimmed());
    } else {
        println!("  {}", "AI Editors configured:".bold());
        for r in &ok_results {
            println!("    {} {}", StatusIcon::Ok, r.name);
        }
    }

    // 5. Detect env var API keys and suggest vault migration
    let has_keys = detect_env_api_keys();

    // If no API key is configured at all, nudge the user toward setup
    if !has_keys {
        println!();
        println!(
            "  {} Tip: run {} to configure your API provider",
            "\u{2192}".cyan(),
            "`nika setup`".bold()
        );
    }

    results
}

/// Detect API keys in environment variables and hint about vault migration.
///
/// Returns `true` if at least one provider key was found.
fn detect_env_api_keys() -> bool {
    let known_keys: &[(&str, &str)] = &[
        ("ANTHROPIC_API_KEY", "anthropic"),
        ("OPENAI_API_KEY", "openai"),
        ("MISTRAL_API_KEY", "mistral"),
        ("GROQ_API_KEY", "groq"),
        ("DEEPSEEK_API_KEY", "deepseek"),
        ("GEMINI_API_KEY", "gemini"),
        ("XAI_API_KEY", "xai"),
    ];

    let found: Vec<&str> = known_keys
        .iter()
        .filter(|(env_var, _)| {
            std::env::var(env_var)
                .map(|v| !v.is_empty())
                .unwrap_or(false)
        })
        .map(|(_, provider)| *provider)
        .collect();

    if !found.is_empty() {
        println!(
            "    {} API keys: {}  {}",
            StatusIcon::Ok,
            found.join(", ").bold(),
            "(from env)".dimmed()
        );
        println!(
            "      {} encrypt at rest: {}",
            "\u{21b3}".dimmed(),
            "nika init --migrate-keys".cyan()
        );
        return true;
    }

    false
}

fn setup_editors() -> Vec<SetupResult> {
    let mut results = Vec::new();

    for def in VSCODE_EDITORS {
        let (name, binary, ext_id) = (def.name, def.binary, def.ext_id);
        // Resolve CLI path (macOS bundle first, then PATH)
        let Some(cli) = resolve_editor_cli(binary) else {
            continue;
        };

        // Check if extension already installed and current
        let ext_version = query_extension_version(&cli, ext_id);

        let cli_ver = env!("CARGO_PKG_VERSION");

        if let Some(ref ver) = ext_version {
            // Extension installed — check if it needs updating
            if !is_version_outdated(ver, cli_ver) {
                // Up to date
                println!("  {} {} + nika-lang v{}", StatusIcon::Ok, name, ver);
                update_extension_version(def.id, ver);
                results.push(SetupResult {
                    name: name.to_string(),
                    success: true,
                    message: format!("v{ver} up to date"),
                });
                continue;
            }

            // Outdated — force update
            print!(
                "  {} {} — updating nika-lang v{} → v{}...",
                "\u{25c7}".cyan(),
                name,
                ver,
                cli_ver
            );
            let install = Command::new(&cli)
                .args(["--install-extension", ext_id, "--force"])
                .output();

            match install {
                Ok(output) if output.status.success() => {
                    println!(
                        "\r  {} {} — nika-lang updated to v{}       ",
                        StatusIcon::Ok,
                        name,
                        cli_ver
                    );
                    update_extension_version(def.id, cli_ver);
                    results.push(SetupResult {
                        name: name.to_string(),
                        success: true,
                        message: "updated".into(),
                    });
                }
                _ => {
                    println!(
                        "\r  {} {} — update failed (v{} installed)       ",
                        "\u{26a0}".yellow(),
                        name,
                        ver
                    );
                    // Not a hard failure — extension still works, just outdated
                    results.push(SetupResult {
                        name: name.to_string(),
                        success: true,
                        message: format!("v{ver} installed, update failed"),
                    });
                }
            }
            continue;
        }

        // Not installed — try to install
        print!("  {} {} — installing nika-lang...", "\u{25c7}".cyan(), name);
        let install = Command::new(&cli)
            .args(["--install-extension", ext_id])
            .output();

        match install {
            Ok(output) if output.status.success() => {
                println!(
                    "\r  {} {} — nika-lang installed       ",
                    StatusIcon::Ok,
                    name
                );
                update_extension_version(def.id, cli_ver);
                results.push(SetupResult {
                    name: name.to_string(),
                    success: true,
                    message: "installed".into(),
                });
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // "not found" = not on this editor's registry (e.g. Open VSX)
                // Try sideloading from GitHub Releases before giving up
                if stderr.contains("not found") {
                    if try_vsix_sideload(&cli, cli_ver) {
                        println!(
                            "\r  {} {} — nika-lang sideloaded from release       ",
                            StatusIcon::Ok,
                            name
                        );
                        update_extension_version(def.id, cli_ver);
                        results.push(SetupResult {
                            name: name.to_string(),
                            success: true,
                            message: "sideloaded from release".into(),
                        });
                    } else {
                        println!(
                            "\r  {} {} — extension not available on marketplace       ",
                            "\u{25cb}".dimmed(),
                            name
                        );
                        results.push(SetupResult {
                            name: name.to_string(),
                            success: true,
                            message: "extension not on marketplace".into(),
                        });
                    }
                } else {
                    println!(
                        "\r  {} {} — install failed          ",
                        "\u{2717}".red(),
                        name
                    );
                    results.push(SetupResult {
                        name: name.to_string(),
                        success: false,
                        message: format!("run: {} --install-extension {}", binary, ext_id),
                    });
                }
            }
            Err(_) => {
                println!(
                    "\r  {} {} — install failed          ",
                    "\u{2717}".red(),
                    name
                );
                results.push(SetupResult {
                    name: name.to_string(),
                    success: false,
                    message: format!("run: {} --install-extension {}", binary, ext_id),
                });
            }
        }
    }

    if results.is_empty() {
        println!(
            "  {} No editors detected (VS Code, Cursor, Windsurf)",
            "\u{25cb}".dimmed()
        );
    }

    results
}

#[cfg(target_os = "macos")]
fn check_macos_app(name: &str) -> bool {
    let app_names: &[&str] = match name {
        "VS Code" => &["Visual Studio Code"],
        "Cursor" => &["Cursor"],
        "Windsurf" => &["Windsurf"],
        _ => return false,
    };
    app_names.iter().any(|app| {
        std::path::Path::new(&format!("/Applications/{}.app", app)).exists()
            || dirs::home_dir()
                .map(|h| h.join(format!("Applications/{}.app", app)).exists())
                .unwrap_or(false)
    })
}

/// Resolve the actual CLI binary path for a VS Code-family editor.
///
/// On macOS, checks the app bundle path FIRST to avoid symlink confusion
/// (e.g. `/usr/local/bin/code` might be a symlink to Cursor, not VS Code).
/// Falls back to `which` for non-macOS or if the bundle isn't installed.
pub fn resolve_editor_cli(binary: &str) -> Option<std::path::PathBuf> {
    // 1. macOS app bundle (preferred — avoids cross-editor symlink confusion)
    #[cfg(target_os = "macos")]
    {
        let (app_name, cli_rel) = match binary {
            "code" => ("Visual Studio Code", "Contents/Resources/app/bin/code"),
            "cursor" => ("Cursor", "Contents/Resources/app/bin/cursor"),
            "windsurf" => ("Windsurf", "Contents/Resources/app/bin/windsurf"),
            _ => ("", ""),
        };
        if !app_name.is_empty() {
            let candidates = [
                std::path::PathBuf::from(format!("/Applications/{}.app/{}", app_name, cli_rel)),
                dirs::home_dir()
                    .unwrap_or_default()
                    .join(format!("Applications/{}.app/{}", app_name, cli_rel)),
            ];
            for p in &candidates {
                if p.exists() {
                    return Some(p.clone());
                }
            }
        }
    }

    // 2. Binary in PATH (non-macOS, or app not installed in standard location)
    if let Ok(p) = which::which(binary) {
        return Some(p);
    }

    None
}

#[cfg(not(target_os = "macos"))]
fn check_macos_app(_name: &str) -> bool {
    false
}

/// Comprehensive Nika rules for Claude Code (~/.claude/rules/nika.md).
const CLAUDE_RULES_CONTENT: &str = include_str!("../../rules/claude.md");

/// Unified Nika rules for Cursor (~/.cursor/rules/nika.mdc).
///
/// Merges syntax, patterns, architecture, and security into one comprehensive
/// .mdc file triggered on *.nika.yaml files.
const CURSOR_NIKA_RULES: &str = include_str!("../../rules/cursor.mdc");

/// Nika rules for Copilot (~/.github/copilot/nika.instructions.md).
const COPILOT_RULES: &str = include_str!("../../rules/copilot.md");

/// Nika rules for Windsurf (~/.windsurf/rules/nika.md).
const WINDSURF_RULES: &str = include_str!("../../rules/windsurf.md");

/// Nika rules for Roo Code (~/.roo/rules/nika.md).
const ROO_RULES: &str = include_str!("../../rules/roo.md");

// ─── Content Hash Fingerprinting ─────────────────────────────────────────────

fn hash_content(content: &str) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(content.as_bytes()))
}

fn load_rule_hashes() -> HashMap<String, String> {
    let content = match std::fs::read_to_string(machine_toml_path()) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let mut in_section = false;
    let mut map = HashMap::new();
    for line in content.lines() {
        let t = line.trim();
        if t == "[rule_hashes]" {
            in_section = true;
            continue;
        }
        if t.starts_with('[') {
            in_section = false;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').to_string();
            if !key.is_empty() && !val.is_empty() {
                map.insert(key, val);
            }
        }
    }
    map
}

fn update_rule_hash(editor_key: &str, hash: &str) {
    let path = machine_toml_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let new_line = format!("{} = \"{}\"", editor_key, hash);
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let section_idx = lines.iter().position(|l| l.trim() == "[rule_hashes]");
    match section_idx {
        Some(idx) => {
            // Find existing key in section
            let key_idx = lines[idx + 1..]
                .iter()
                .position(|l| {
                    let t = l.trim();
                    t.starts_with(editor_key) && t[editor_key.len()..].trim_start().starts_with('=')
                })
                .map(|i| i + idx + 1);
            match key_idx {
                Some(ki) => lines[ki] = new_line,
                None => lines.insert(idx + 1, new_line),
            }
        }
        None => {
            lines.push(String::new());
            lines.push("[rule_hashes]".to_string());
            lines.push(new_line);
        }
    }
    std::fs::write(&path, lines.join("\n") + "\n").ok();
}

// ─── Extension Version Tracking ─────────────────────────────────────────────

/// Read stored extension versions from `[extensions]` section of machine.toml.
pub fn read_extension_versions() -> HashMap<String, String> {
    let content = match std::fs::read_to_string(machine_toml_path()) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let mut in_section = false;
    let mut map = HashMap::new();
    for line in content.lines() {
        let t = line.trim();
        if t == "[extensions]" {
            in_section = true;
            continue;
        }
        if t.starts_with('[') {
            in_section = false;
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').to_string();
            if !key.is_empty() && !val.is_empty() {
                map.insert(key, val);
            }
        }
    }
    map
}

/// Update a single extension version in the `[extensions]` section of machine.toml.
fn update_extension_version(editor_id: &str, version: &str) {
    let path = machine_toml_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let new_line = format!("{} = \"{}\"", editor_id, version);
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let section_idx = lines.iter().position(|l| l.trim() == "[extensions]");
    match section_idx {
        Some(idx) => {
            let key_idx = lines[idx + 1..]
                .iter()
                .position(|l| {
                    let t = l.trim();
                    t.starts_with(editor_id)
                        && t[editor_id.len()..].trim_start().starts_with('=')
                })
                .map(|i| i + idx + 1);
            match key_idx {
                Some(ki) => lines[ki] = new_line,
                None => lines.insert(idx + 1, new_line),
            }
        }
        None => {
            lines.push(String::new());
            lines.push("[extensions]".to_string());
            lines.push(new_line);
        }
    }
    std::fs::write(&path, lines.join("\n") + "\n").ok();
}

// ─── Fast Path: Silent Rule Update ──────────────────────────────────────────

/// Fast-path setup: silently update AI rules if CLI version changed.
///
/// Pure filesystem I/O, no subprocesses. Safe for headless commands like `nika run`.
/// Returns true if rules were actually updated.
pub fn fast_rule_update() -> bool {
    use super::status::machine_setup_status;
    use super::status::MachineStatus;

    // Only act on version mismatch (< 0.5ms when version matches)
    if machine_setup_status() != MachineStatus::NeedsUpdate {
        return false;
    }

    // Update rules silently (no println)
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return false,
    };
    let hashes = load_rule_hashes();
    let mut dummy_results = Vec::new();
    let editors = detect_editors();

    if editors.contains(&"claude") {
        install_rule(
            &home.join(".claude/rules/nika.md"),
            CLAUDE_RULES_CONTENT,
            "Claude Code",
            "claude",
            &hashes,
            &mut dummy_results,
            true, // silent
        );
    }
    if editors.contains(&"cursor") {
        install_rule(
            &home.join(".cursor/rules/nika.mdc"),
            CURSOR_NIKA_RULES,
            "Cursor",
            "cursor",
            &hashes,
            &mut dummy_results,
            true,
        );
    }
    if editors.contains(&"copilot") {
        install_rule(
            &home.join(".github/copilot/nika.instructions.md"),
            COPILOT_RULES,
            "Copilot",
            "copilot",
            &hashes,
            &mut dummy_results,
            true,
        );
    }
    if editors.contains(&"windsurf") {
        install_rule(
            &home.join(".windsurf/rules/nika.md"),
            WINDSURF_RULES,
            "Windsurf",
            "windsurf",
            &hashes,
            &mut dummy_results,
            true,
        );
    }
    if editors.contains(&"roo") {
        install_rule(
            &home.join(".roo/rules/nika.md"),
            ROO_RULES,
            "Roo Code",
            "roo",
            &hashes,
            &mut dummy_results,
            true,
        );
    }

    // Update only the version field (preserves everything else)
    super::status::update_marker_version();
    true
}

fn setup_ai_rules() -> Vec<SetupResult> {
    let home = dirs::home_dir();
    if home.is_none() {
        return vec![SetupResult {
            name: "AI Rules".into(),
            success: false,
            message: "cannot determine home directory".into(),
        }];
    }
    let home = home.unwrap();
    let mut results = Vec::new();
    let editors = detect_editors();
    let hashes = load_rule_hashes();

    // Claude Code
    if editors.contains(&"claude") {
        install_rule(
            &home.join(".claude/rules/nika.md"),
            CLAUDE_RULES_CONTENT,
            "Claude Code",
            "claude",
            &hashes,
            &mut results,
            false,
        );
    }

    // Cursor
    if editors.contains(&"cursor") {
        install_rule(
            &home.join(".cursor/rules/nika.mdc"),
            CURSOR_NIKA_RULES,
            "Cursor",
            "cursor",
            &hashes,
            &mut results,
            false,
        );
    }

    // Copilot
    if editors.contains(&"copilot") {
        install_rule(
            &home.join(".github/copilot/nika.instructions.md"),
            COPILOT_RULES,
            "Copilot",
            "copilot",
            &hashes,
            &mut results,
            false,
        );
    }

    // Windsurf
    if editors.contains(&"windsurf") {
        install_rule(
            &home.join(".windsurf/rules/nika.md"),
            WINDSURF_RULES,
            "Windsurf",
            "windsurf",
            &hashes,
            &mut results,
            false,
        );
    }

    // Roo Code
    if editors.contains(&"roo") {
        install_rule(
            &home.join(".roo/rules/nika.md"),
            ROO_RULES,
            "Roo Code",
            "roo",
            &hashes,
            &mut results,
            false,
        );
    }

    // Agent Skills: install to ~/.agents/skills/
    let skills_dir = home.join(".agents/skills");
    let has_skills = skills_dir.join("nika-workflow-syntax").exists();
    if !has_skills {
        let skill_dir = skills_dir.join("nika-workflow-syntax");
        std::fs::create_dir_all(&skill_dir).ok();
        let skill_content = CLAUDE_RULES_CONTENT;
        if std::fs::write(skill_dir.join("SKILL.md"), skill_content).is_ok() {
            println!(
                "  {} Agent Skills installed [~/.agents/skills/]",
                StatusIcon::Ok
            );
            results.push(SetupResult {
                name: "Agent Skills".into(),
                success: true,
                message: "installed".into(),
            });
        }
    } else {
        println!("  {} Agent Skills [~/.agents/skills/]", StatusIcon::Ok);
        results.push(SetupResult {
            name: "Agent Skills".into(),
            success: true,
            message: "present".into(),
        });
    }

    results
}

/// Install a rule file for an editor with content-hash protection.
///
/// - If file exists and content matches expected -> skip ("up to date")
/// - If file exists and disk hash doesn't match stored hash -> skip with warning
///   ("user-customized")
/// - Otherwise -> write + update hash
///
/// When `silent` is false, prints progress and pushes to `results`.
/// When `silent` is true, returns quietly (used by quick_editor_scan).
fn install_rule(
    path: &Path,
    content: &str,
    name: &str,
    editor_key: &str,
    hashes: &HashMap<String, String>,
    results: &mut Vec<SetupResult>,
    silent: bool,
) {
    let expected_hash = hash_content(content);

    // If file exists, check hashes before overwriting
    if path.exists() {
        if let Ok(disk_content) = std::fs::read_to_string(path) {
            // E3/BUG-009: Respect "DO NOT OVERWRITE" sentinel as a
            // fallback when machine.toml hashes are missing/corrupted.
            if disk_content.contains("DO NOT OVERWRITE") {
                if !silent {
                    println!(
                        "  {} {} — has DO NOT OVERWRITE sentinel, skipping",
                        "\u{26a0}".yellow(),
                        name
                    );
                    results.push(SetupResult {
                        name: name.into(),
                        success: true,
                        message: "sentinel-protected, preserved".into(),
                    });
                }
                return;
            }

            let disk_hash = hash_content(&disk_content);

            // Content already matches — nothing to do
            if disk_hash == expected_hash {
                if !silent {
                    println!("  {} {} — up to date", StatusIcon::Ok, name);
                    results.push(SetupResult {
                        name: name.into(),
                        success: true,
                        message: "up to date".into(),
                    });
                }
                return;
            }

            // File differs from expected. Was it customized by the user?
            if let Some(stored_hash) = hashes.get(editor_key) {
                if disk_hash != *stored_hash {
                    if !silent {
                        println!(
                            "  {} {} — user-customized, skipping",
                            "\u{26a0}".yellow(),
                            name
                        );
                        results.push(SetupResult {
                            name: name.into(),
                            success: true,
                            message: "user-customized, preserved".into(),
                        });
                    }
                    return;
                }
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    match std::fs::write(path, content) {
        Ok(()) => {
            update_rule_hash(editor_key, &expected_hash);
            if !silent {
                println!("  {} {} — Nika rules installed", StatusIcon::Ok, name);
                results.push(SetupResult {
                    name: name.into(),
                    success: true,
                    message: "installed".into(),
                });
            }
        }
        Err(e) => {
            if !silent {
                println!("  {} {} — write failed: {}", "\u{2717}".red(), name, e);
                results.push(SetupResult {
                    name: name.into(),
                    success: false,
                    message: format!("write failed: {}", e),
                });
            }
        }
    }
}

/// Install daemon as system service and start it.
/// LaunchAgent on macOS, systemd user service on Linux.
/// Daemon provides: vault secrets, LLM cache, job scheduling, file watch.
#[cfg(unix)]
fn setup_daemon() -> SetupResult {
    // Install the service file (plist or systemd unit)
    if let Err(e) = nika_daemon::install::install() {
        return SetupResult {
            name: "Daemon service".into(),
            success: false,
            message: format!("install failed: {e}"),
        };
    }

    // Start daemon in background (non-blocking)
    let nika_exe = std::env::current_exe().unwrap_or_else(|_| "nika".into());
    let started = Command::new(&nika_exe)
        .args(["daemon", "start"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .is_ok();

    SetupResult {
        name: "Daemon service".into(),
        success: true,
        message: if started {
            "installed + started (vault, cache, jobs)".into()
        } else {
            "installed (start manually: nika daemon start)".into()
        },
    }
}

fn setup_completions() -> SetupResult {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => {
            return SetupResult {
                name: "Completions".into(),
                success: false,
                message: "cannot determine home directory".into(),
            };
        }
    };
    let shell = std::env::var("SHELL").unwrap_or_default();
    let shell_name = if shell.contains("zsh") {
        "zsh"
    } else if shell.contains("bash") {
        "bash"
    } else if shell.contains("fish") {
        "fish"
    } else {
        return SetupResult {
            name: "Completions".into(),
            success: false,
            message: "unknown shell".into(),
        };
    };

    // Check if nika completion command exists
    let output = Command::new("nika")
        .args(["completion", shell_name])
        .output();

    match output {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => {
            // Write completions to appropriate location
            let target = match shell_name {
                "zsh" => {
                    let zfunc = home.join(".zfunc");
                    std::fs::create_dir_all(&zfunc).ok();
                    Some(zfunc.join("_nika"))
                }
                "bash" => {
                    let dir = home.join(".local/share/bash-completion/completions");
                    std::fs::create_dir_all(&dir).ok();
                    Some(dir.join("nika"))
                }
                "fish" => {
                    let dir = dirs::config_dir()
                        .unwrap_or_default()
                        .join("fish/completions");
                    std::fs::create_dir_all(&dir).ok();
                    Some(dir.join("nika.fish"))
                }
                _ => None,
            };

            if let Some(target) = target {
                if std::fs::write(&target, &o.stdout).is_ok() {
                    println!("  {} {} completions installed", StatusIcon::Ok, shell_name);
                    return SetupResult {
                        name: "Completions".into(),
                        success: true,
                        message: format!("{} completions at {}", shell_name, target.display()),
                    };
                }
            }

            SetupResult {
                name: "Completions".into(),
                success: false,
                message: "could not write completions".into(),
            }
        }
        _ => {
            println!(
                "  {} {} completions (nika completion not available)",
                "\u{25cb}".dimmed(),
                shell_name
            );
            SetupResult {
                name: "Completions".into(),
                success: false,
                message: "nika completion command not available".into(),
            }
        }
    }
}

// ─── Quick Editor Re-Scan ────────────────────────────────────────────────────

/// 24-hour cooldown in seconds for quick_editor_scan.
const SCAN_COOLDOWN_SECS: u64 = 14_400; // 4 hours — devs install editors during the day

/// Lightweight scan for newly installed editors. Called on every nika command
/// when machine_setup_status() == Ready. If a new editor is detected that
/// wasn't in machine.toml, install rules silently and update the stored list.
///
/// New editors bypass the cooldown entirely — rules are installed immediately.
/// Cooldown only applies when no new editors are found, to avoid repeated
/// filesystem overhead on every command.
pub fn quick_editor_scan() {
    let current = detect_editors();
    let stored = read_stored_editors();

    let new_editors: Vec<&&str> = current
        .iter()
        .filter(|e| !stored.iter().any(|s| s == *e))
        .collect();

    // Check for outdated extensions using stored versions (no subprocess)
    let cli_ver = env!("CARGO_PKG_VERSION");
    let stored_ext_versions = read_extension_versions();
    let outdated_editors: Vec<&EditorDef> = VSCODE_EDITORS
        .iter()
        .filter(|def| {
            current.contains(&def.id)
                && stored_ext_versions
                    .get(def.id)
                    .map(|v| is_version_outdated(v, cli_ver))
                    .unwrap_or(false)
        })
        .collect();

    if new_editors.is_empty() && outdated_editors.is_empty() {
        // Nothing to do — apply cooldown to avoid repeated filesystem work
        if let Some(last) = read_last_scan_at() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if now.saturating_sub(last) < SCAN_COOLDOWN_SECS {
                return;
            }
        }
        return;
    }

    // Update outdated extensions silently (subprocess only when needed)
    for def in &outdated_editors {
        if let Some(cli) = resolve_editor_cli(def.binary) {
            let result = Command::new(&cli)
                .args(["--install-extension", def.ext_id, "--force"])
                .output();
            if result.map(|o| o.status.success()).unwrap_or(false) {
                update_extension_version(def.id, cli_ver);
            }
        }
    }

    // New editors found — install rules immediately (bypass cooldown)
    let home = dirs::home_dir().unwrap_or_default();
    let hashes = load_rule_hashes();
    let mut results = Vec::new();
    for editor in &new_editors {
        match **editor {
            "claude" => install_rule(
                &home.join(".claude/rules/nika.md"),
                CLAUDE_RULES_CONTENT,
                "Claude Code",
                "claude",
                &hashes,
                &mut results,
                true,
            ),
            "cursor" => {
                install_rule(
                    &home.join(".cursor/rules/nika.mdc"),
                    CURSOR_NIKA_RULES,
                    "Cursor",
                    "cursor",
                    &hashes,
                    &mut results,
                    true,
                );
                install_vscode_extension("cursor", "supernovae.nika-lang");
            }
            "copilot" => install_rule(
                &home.join(".github/copilot/nika.instructions.md"),
                COPILOT_RULES,
                "Copilot",
                "copilot",
                &hashes,
                &mut results,
                true,
            ),
            "vscode" => {
                install_vscode_extension("code", "supernovae.nika-lang");
            }
            "windsurf" => {
                install_rule(
                    &home.join(".windsurf/rules/nika.md"),
                    WINDSURF_RULES,
                    "Windsurf",
                    "windsurf",
                    &hashes,
                    &mut results,
                    true,
                );
                install_vscode_extension("windsurf", "supernovae.nika-lang");
            }
            "roo" => install_rule(
                &home.join(".roo/rules/nika.md"),
                ROO_RULES,
                "Roo Code",
                "roo",
                &hashes,
                &mut results,
                true,
            ),
            _ => {}
        }
    }

    // Update machine.toml with new editors list + scan timestamp
    update_machine_toml_editors(&current);
    write_last_scan_at();
}

/// Download VSIX from GitHub Releases and sideload it.
/// Fallback when marketplace install fails (e.g., Open VSX missing the extension).
fn try_vsix_sideload(cli: &Path, version: &str) -> bool {
    let cache_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".nika")
        .join("cache");
    std::fs::create_dir_all(&cache_dir).ok();
    let vsix_path = cache_dir.join(format!("nika-lang-{}.vsix", version));
    let url = format!(
        "https://github.com/supernovae-st/nika/releases/download/v{}/nika-lang-{}.vsix",
        version, version
    );

    // Download via curl (available on macOS, Linux, Windows 10+)
    let download = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&vsix_path)
        .arg(&url)
        .output();

    if !download.map(|o| o.status.success()).unwrap_or(false) {
        std::fs::remove_file(&vsix_path).ok();
        return false;
    }

    let install = Command::new(cli)
        .arg("--install-extension")
        .arg(&vsix_path)
        .output();

    std::fs::remove_file(&vsix_path).ok();
    install.map(|o| o.status.success()).unwrap_or(false)
}

/// Silently install or update the nika-lang extension for a VS Code-compatible editor.
/// Used by quick_editor_scan when a new editor is detected.
fn install_vscode_extension(binary: &str, ext_id: &str) {
    let Some(cli) = resolve_editor_cli(binary) else {
        return;
    };

    if query_extension_version(&cli, ext_id).is_some() {
        return;
    }

    let _ = Command::new(&cli)
        .args(["--install-extension", ext_id])
        .output();
}

/// Read `last_scan_at` timestamp from machine.toml.
fn read_last_scan_at() -> Option<u64> {
    let content = std::fs::read_to_string(machine_toml_path()).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("last_scan_at") {
            let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
            return rest.trim_matches('"').parse().ok();
        }
    }
    None
}

/// Write `last_scan_at` timestamp to machine.toml.
fn write_last_scan_at() {
    let marker_path = machine_toml_path();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let content = std::fs::read_to_string(&marker_path).unwrap_or_default();
    let new_line = format!("last_scan_at = \"{}\"", now);

    // Replace existing last_scan_at or append
    if content.contains("last_scan_at") {
        let updated: String = content
            .lines()
            .map(|l| {
                if l.trim().starts_with("last_scan_at") {
                    new_line.as_str()
                } else {
                    l
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&marker_path, format!("{}\n", updated)).ok();
    } else {
        std::fs::write(&marker_path, format!("{}{}\n", content, new_line)).ok();
    }
}

/// Read the editors list from machine.toml.
fn read_stored_editors() -> Vec<String> {
    let marker = machine_toml_path();
    let content = match std::fs::read_to_string(&marker) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("editors") {
            let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
            // Parse TOML array: ["vscode", "claude", "cursor"]
            let inner = rest.trim_start_matches('[').trim_end_matches(']');
            return inner
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    Vec::new()
}

/// Update only the editors field in machine.toml, preserving version and
/// setup_at.
fn update_machine_toml_editors(editors: &[&str]) {
    let marker_path = machine_toml_path();
    let content = match std::fs::read_to_string(&marker_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let editors_toml: Vec<String> = editors.iter().map(|e| format!("\"{}\"", e)).collect();
    let new_editors_line = format!("editors = [{}]", editors_toml.join(", "));

    let mut updated = String::new();
    let mut found = false;
    for line in content.lines() {
        if line.trim().starts_with("editors") {
            updated.push_str(&new_editors_line);
            found = true;
        } else {
            updated.push_str(line);
        }
        updated.push('\n');
    }
    if !found {
        updated.push_str(&new_editors_line);
        updated.push('\n');
    }

    std::fs::write(&marker_path, updated).ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_toml_path_is_in_home() {
        let path = machine_toml_path();
        assert!(path.to_string_lossy().contains(".nika"));
        assert!(path.to_string_lossy().ends_with("machine.toml"));
    }

    /// All rule constants must use {{with.item}} not bare {{item}} in code
    /// examples (the "Wrong" column in mistake tables is exempt).
    #[test]
    fn all_rules_use_with_item_not_bare_item() {
        let rules: &[(&str, &str)] = &[
            ("CLAUDE_RULES_CONTENT", CLAUDE_RULES_CONTENT),
            ("CURSOR_NIKA_RULES", CURSOR_NIKA_RULES),
            ("COPILOT_RULES", COPILOT_RULES),
            ("WINDSURF_RULES", WINDSURF_RULES),
            ("ROO_RULES", ROO_RULES),
        ];
        for (name, content) in rules {
            for (i, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.contains("{{item}}")
                    && !trimmed.starts_with('|')
                    && !trimmed.contains("Wrong")
                {
                    panic!(
                        "{} line {} has bare {{{{item}}}} outside mistakes table: {}",
                        name,
                        i + 1,
                        trimmed
                    );
                }
            }
        }
    }

    /// All rule constants must reference schema @0.12.
    #[test]
    fn all_rules_reference_current_schema() {
        let rules: &[(&str, &str)] = &[
            ("CLAUDE_RULES_CONTENT", CLAUDE_RULES_CONTENT),
            ("CURSOR_NIKA_RULES", CURSOR_NIKA_RULES),
            ("COPILOT_RULES", COPILOT_RULES),
            ("WINDSURF_RULES", WINDSURF_RULES),
            ("ROO_RULES", ROO_RULES),
        ];
        for (name, content) in rules {
            assert!(content.contains("@0.12"), "{} missing schema @0.12", name);
        }
    }

    /// No rule constants should reference nonexistent models.
    #[test]
    fn all_rules_no_nonexistent_models() {
        let rules: &[(&str, &str)] = &[
            ("CLAUDE_RULES_CONTENT", CLAUDE_RULES_CONTENT),
            ("CURSOR_NIKA_RULES", CURSOR_NIKA_RULES),
            ("COPILOT_RULES", COPILOT_RULES),
            ("WINDSURF_RULES", WINDSURF_RULES),
            ("ROO_RULES", ROO_RULES),
        ];
        for (name, content) in rules {
            assert!(
                !content.contains("grok-4"),
                "{} references nonexistent model grok-4",
                name
            );
        }
    }

    /// detect_editors returns a Vec (may be empty in CI, but must not panic).
    #[test]
    fn detect_editors_does_not_panic() {
        let editors = detect_editors();
        // Just ensure it returns without panicking; contents depend on machine
        assert!(editors.len() <= 10, "unexpectedly many editors detected");
    }

    /// read_stored_editors parses a TOML editors array correctly.
    #[test]
    fn read_stored_editors_parses_toml_array() {
        let tmpdir = tempfile::tempdir().unwrap();
        let marker = tmpdir.path().join("machine.toml");
        std::fs::write(
            &marker,
            "[machine]\nversion = \"0.41.3\"\neditors = [\"vscode\", \"claude\", \"cursor\"]\n",
        )
        .unwrap();

        // read_stored_editors uses machine_toml_path() which reads from ~/.nika/,
        // so we test the parsing logic directly
        let content = std::fs::read_to_string(&marker).unwrap();
        let mut parsed = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("editors") {
                let rest = rest.trim().strip_prefix('=').unwrap_or("").trim();
                let inner = rest.trim_start_matches('[').trim_end_matches(']');
                parsed = inner
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        assert_eq!(parsed, vec!["vscode", "claude", "cursor"]);
    }

    /// Cursor rules .mdc must have valid frontmatter.
    #[test]
    fn cursor_rules_have_mdc_frontmatter() {
        assert!(
            CURSOR_NIKA_RULES.starts_with("---\n"),
            "CURSOR_NIKA_RULES must start with YAML frontmatter"
        );
        assert!(
            CURSOR_NIKA_RULES.contains("globs:"),
            "CURSOR_NIKA_RULES must have globs: in frontmatter"
        );
        assert!(
            CURSOR_NIKA_RULES.contains("alwaysApply:"),
            "CURSOR_NIKA_RULES must have alwaysApply: in frontmatter"
        );
    }

    /// Copilot rules must have applyTo frontmatter.
    #[test]
    fn copilot_rules_have_apply_to() {
        assert!(
            COPILOT_RULES.contains("applyTo:"),
            "COPILOT_RULES must have applyTo: frontmatter"
        );
    }

    /// Windsurf rules must have trigger frontmatter.
    #[test]
    fn windsurf_rules_have_trigger() {
        assert!(
            WINDSURF_RULES.contains("trigger:"),
            "WINDSURF_RULES must have trigger: frontmatter"
        );
    }

    /// install_rule respects DO NOT OVERWRITE sentinel.
    #[test]
    fn install_rule_respects_sentinel() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("nika.md");
        let custom_content = "# DO NOT OVERWRITE\n# My custom rules\n";
        std::fs::write(&path, custom_content).unwrap();

        let hashes = HashMap::new();
        let mut results = Vec::new();
        install_rule(
            &path,
            "# new content that should not be written\n",
            "test",
            "test",
            &hashes,
            &mut results,
            false,
        );
        // File should NOT be overwritten
        assert_eq!(std::fs::read_to_string(&path).unwrap(), custom_content);
    }

    /// install_rule with silent=true writes to disk.
    #[test]
    fn install_rule_silent_writes_file() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("sub/dir/rule.md");
        let hashes = HashMap::new();
        let mut results = Vec::new();
        install_rule(
            &path,
            "# test rule\n",
            "test",
            "test",
            &hashes,
            &mut results,
            true,
        );
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# test rule\n");
    }
}
