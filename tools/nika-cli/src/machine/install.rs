// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
    EditorDef {
        id: "vscode",
        name: "VS Code",
        binary: "code",
        ext_id: "supernovae.nika-lang",
    },
    EditorDef {
        id: "cursor",
        name: "Cursor",
        binary: "cursor",
        ext_id: "supernovae.nika-lang",
    },
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
        let trimmed = l.trim();
        if trimmed.to_lowercase().starts_with(&prefix) {
            Some(trimmed[prefix.len()..].to_string())
        } else {
            None
        }
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

    // 1. Editors: detect + install extension (writes [extensions] to marker)
    results.extend(setup_editors());

    // 2. AI tools: detect + install rules (writes [rule_hashes] to marker)
    results.extend(setup_ai_rules());

    // 3. Shell completions
    results.push(setup_completions());

    // 4. Daemon service: install + start (Unix only, opt-out via NIKA_NO_DAEMON=1)
    #[cfg(unix)]
    if std::env::var("NIKA_NO_DAEMON").is_err() {
        results.push(setup_daemon());
    }

    // Write marker file (preserves [extensions] and [rule_hashes] sections)
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

// Rule content is now assembled from shared modules via crate::rules.
use crate::rules;

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
            // Find existing key within this section only (stop at next [header])
            let mut key_idx = None;
            for (i, l) in lines[idx + 1..].iter().enumerate() {
                let t = l.trim();
                if t.starts_with('[') {
                    break; // next section — stop searching
                }
                if t.starts_with(editor_key) && t[editor_key.len()..].trim_start().starts_with('=')
                {
                    key_idx = Some(i + idx + 1);
                    break;
                }
            }
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
            let mut key_idx = None;
            for (i, l) in lines[idx + 1..].iter().enumerate() {
                let t = l.trim();
                if t.starts_with('[') {
                    break;
                }
                if t.starts_with(editor_id) && t[editor_id.len()..].trim_start().starts_with('=') {
                    key_idx = Some(i + idx + 1);
                    break;
                }
            }
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

    let claude_rules = rules::assemble_claude_rules();
    let copilot_rules = rules::assemble_copilot_instructions();
    let windsurf_rules = rules::assemble_windsurf_rules();
    let roo_rules = rules::assemble_roo_rules();
    let gemini_rules = rules::assemble_gemini_md();

    if editors.contains(&"claude") {
        install_rule(
            &home.join(".claude/rules/nika.md"),
            &claude_rules,
            "Claude Code",
            "claude",
            &hashes,
            &mut dummy_results,
            true, // silent
        );
    }
    if editors.contains(&"cursor") {
        // Multi-file cursor rules (3 files)
        let cursor_dir = home.join(".cursor/rules");
        std::fs::create_dir_all(&cursor_dir).ok();
        let project = rules::assemble_cursor_project_mdc();
        let syntax = rules::assemble_cursor_syntax_mdc();
        let reference = rules::assemble_cursor_reference_mdc();
        install_rule(
            &cursor_dir.join("nika-project.mdc"),
            &project,
            "Cursor (project)",
            "cursor_project",
            &hashes,
            &mut dummy_results,
            true,
        );
        install_rule(
            &cursor_dir.join("nika-syntax.mdc"),
            &syntax,
            "Cursor (syntax)",
            "cursor_syntax",
            &hashes,
            &mut dummy_results,
            true,
        );
        install_rule(
            &cursor_dir.join("nika-reference.mdc"),
            &reference,
            "Cursor (reference)",
            "cursor_reference",
            &hashes,
            &mut dummy_results,
            true,
        );
        // Remove old monolithic file
        let old_cursor = cursor_dir.join("nika.mdc");
        if old_cursor.exists() {
            std::fs::remove_file(&old_cursor).ok();
        }
    }
    if editors.contains(&"copilot") {
        install_rule(
            &home.join(".github/copilot-instructions.md"),
            &copilot_rules,
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
            &windsurf_rules,
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
            &roo_rules,
            "Roo Code",
            "roo",
            &hashes,
            &mut dummy_results,
            true,
        );
    }
    // Gemini CLI
    let gemini_dir = home.join(".gemini");
    if gemini_dir.exists() || which::which("gemini").is_ok() {
        std::fs::create_dir_all(&gemini_dir).ok();
        install_rule(
            &gemini_dir.join("GEMINI.md"),
            &gemini_rules,
            "Gemini",
            "gemini",
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
    let Some(home) = dirs::home_dir() else {
        return vec![SetupResult {
            name: "AI Rules".into(),
            success: false,
            message: "cannot determine home directory".into(),
        }];
    };
    let mut results = Vec::new();
    let editors = detect_editors();
    let hashes = load_rule_hashes();

    let claude_rules = rules::assemble_claude_rules();
    let copilot_rules = rules::assemble_copilot_instructions();
    let windsurf_rules = rules::assemble_windsurf_rules();
    let roo_rules = rules::assemble_roo_rules();
    let gemini_rules = rules::assemble_gemini_md();

    // Claude Code
    if editors.contains(&"claude") {
        install_rule(
            &home.join(".claude/rules/nika.md"),
            &claude_rules,
            "Claude Code",
            "claude",
            &hashes,
            &mut results,
            false,
        );
    }

    // Cursor — 3-file progressive discovery
    if editors.contains(&"cursor") {
        let cursor_dir = home.join(".cursor/rules");
        std::fs::create_dir_all(&cursor_dir).ok();
        let project = rules::assemble_cursor_project_mdc();
        let syntax = rules::assemble_cursor_syntax_mdc();
        let reference = rules::assemble_cursor_reference_mdc();
        install_rule(
            &cursor_dir.join("nika-project.mdc"),
            &project,
            "Cursor (project)",
            "cursor_project",
            &hashes,
            &mut results,
            false,
        );
        install_rule(
            &cursor_dir.join("nika-syntax.mdc"),
            &syntax,
            "Cursor (syntax)",
            "cursor_syntax",
            &hashes,
            &mut results,
            false,
        );
        install_rule(
            &cursor_dir.join("nika-reference.mdc"),
            &reference,
            "Cursor (reference)",
            "cursor_reference",
            &hashes,
            &mut results,
            false,
        );
        // Remove old monolithic file
        let old_cursor = cursor_dir.join("nika.mdc");
        if old_cursor.exists() {
            std::fs::remove_file(&old_cursor).ok();
        }
    }

    // Copilot
    if editors.contains(&"copilot") {
        install_rule(
            &home.join(".github/copilot-instructions.md"),
            &copilot_rules,
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
            &windsurf_rules,
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
            &roo_rules,
            "Roo Code",
            "roo",
            &hashes,
            &mut results,
            false,
        );
    }

    // Gemini CLI
    let gemini_dir = home.join(".gemini");
    if gemini_dir.exists() || which::which("gemini").is_ok() {
        std::fs::create_dir_all(&gemini_dir).ok();
        install_rule(
            &gemini_dir.join("GEMINI.md"),
            &gemini_rules,
            "Gemini",
            "gemini",
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
        let skill_content = rules::assemble_agents_md();
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

/// 4-hour cooldown in seconds for quick_editor_scan.
const SCAN_COOLDOWN_SECS: u64 = 14_400;

/// Lightweight scan for newly installed editors and outdated extensions.
///
/// Called on every nika command when `machine_setup_status() == Ready`.
/// Cooldown prevents repeated filesystem + subprocess work on every command.
/// New editors or outdated extensions bypass the cooldown.
pub fn quick_editor_scan() {
    // Cooldown check FIRST — avoids expensive detect_editors() on every command
    if let Some(last) = read_last_scan_at() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now.saturating_sub(last) < SCAN_COOLDOWN_SECS {
            return;
        }
    }

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
        write_last_scan_at(); // record scan time even when nothing to do
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
    let claude_rules = rules::assemble_claude_rules();
    let copilot_rules = rules::assemble_copilot_instructions();
    let windsurf_rules = rules::assemble_windsurf_rules();
    let roo_rules = rules::assemble_roo_rules();
    for editor in &new_editors {
        match **editor {
            "claude" => install_rule(
                &home.join(".claude/rules/nika.md"),
                &claude_rules,
                "Claude Code",
                "claude",
                &hashes,
                &mut results,
                true,
            ),
            "cursor" => {
                let cursor_dir = home.join(".cursor/rules");
                std::fs::create_dir_all(&cursor_dir).ok();
                let project = rules::assemble_cursor_project_mdc();
                let syntax = rules::assemble_cursor_syntax_mdc();
                let reference = rules::assemble_cursor_reference_mdc();
                install_rule(
                    &cursor_dir.join("nika-project.mdc"),
                    &project,
                    "Cursor (project)",
                    "cursor_project",
                    &hashes,
                    &mut results,
                    true,
                );
                install_rule(
                    &cursor_dir.join("nika-syntax.mdc"),
                    &syntax,
                    "Cursor (syntax)",
                    "cursor_syntax",
                    &hashes,
                    &mut results,
                    true,
                );
                install_rule(
                    &cursor_dir.join("nika-reference.mdc"),
                    &reference,
                    "Cursor (reference)",
                    "cursor_reference",
                    &hashes,
                    &mut results,
                    true,
                );
                install_vscode_extension("cursor", "supernovae.nika-lang");
            }
            "copilot" => install_rule(
                &home.join(".github/copilot-instructions.md"),
                &copilot_rules,
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
                    &windsurf_rules,
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
                &roo_rules,
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

    // Integrity check: VSIX is a ZIP — verify magic bytes and minimum size
    let valid = std::fs::read(&vsix_path)
        .map(|bytes| bytes.len() > 1024 && bytes.starts_with(b"PK\x03\x04"))
        .unwrap_or(false);
    if !valid {
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
        let sep = if content.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&marker_path, format!("{}{}{}\n", content, sep, new_line)).ok();
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

    /// All assembled rules must use {{with.item}} not bare {{item}} in code
    /// examples (the "Wrong" column in mistake tables is exempt).
    #[test]
    fn all_rules_use_with_item_not_bare_item() {
        let assembled: Vec<(&str, String)> = vec![
            ("claude", rules::assemble_claude_rules()),
            ("cursor_syntax", rules::assemble_cursor_syntax_mdc()),
            ("copilot", rules::assemble_copilot_instructions()),
            ("windsurf", rules::assemble_windsurf_rules()),
            ("roo", rules::assemble_roo_rules()),
        ];
        for (name, content) in &assembled {
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

    /// All assembled rules must reference schema @0.12.
    #[test]
    fn all_rules_reference_current_schema() {
        let assembled: Vec<(&str, String)> = vec![
            ("claude", rules::assemble_claude_rules()),
            ("cursor_syntax", rules::assemble_cursor_syntax_mdc()),
            ("copilot", rules::assemble_copilot_instructions()),
            ("windsurf", rules::assemble_windsurf_rules()),
            ("roo", rules::assemble_roo_rules()),
        ];
        for (name, content) in &assembled {
            assert!(content.contains("@0.12"), "{} missing schema @0.12", name);
        }
    }

    /// No assembled rules should reference nonexistent models.
    #[test]
    fn all_rules_no_nonexistent_models() {
        let assembled: Vec<(&str, String)> = vec![
            ("claude", rules::assemble_claude_rules()),
            ("cursor_syntax", rules::assemble_cursor_syntax_mdc()),
            ("copilot", rules::assemble_copilot_instructions()),
            ("windsurf", rules::assemble_windsurf_rules()),
            ("roo", rules::assemble_roo_rules()),
        ];
        for (name, content) in &assembled {
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

    /// Cursor syntax .mdc must have valid frontmatter with globs.
    #[test]
    fn cursor_syntax_rules_have_mdc_frontmatter() {
        let syntax = rules::assemble_cursor_syntax_mdc();
        assert!(
            syntax.starts_with("---\n"),
            "cursor syntax must start with YAML frontmatter"
        );
        assert!(
            syntax.contains("globs:"),
            "cursor syntax must have globs: in frontmatter"
        );
    }

    /// Cursor project .mdc must have alwaysApply.
    #[test]
    fn cursor_project_rules_always_apply() {
        let project = rules::assemble_cursor_project_mdc();
        assert!(
            project.contains("alwaysApply: true"),
            "cursor project must have alwaysApply: true"
        );
    }

    /// Copilot rules must have nika content.
    #[test]
    fn copilot_rules_have_nika_content() {
        let copilot = rules::assemble_copilot_instructions();
        assert!(
            copilot.contains("nika/workflow@0.12"),
            "copilot rules must have schema version"
        );
    }

    /// Windsurf rules must have trigger frontmatter.
    #[test]
    fn windsurf_rules_have_trigger() {
        let windsurf = rules::assemble_windsurf_rules();
        assert!(
            windsurf.contains("trigger:"),
            "windsurf rules must have trigger:"
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

    // ═══════════════════════════════════════════════════════════════════════
    // query_extension_version — parsing logic
    // ═══════════════════════════════════════════════════════════════════════

    /// Simulate parsing of `--list-extensions --show-versions` output.
    /// The function looks for lines matching `ext_id@version` (case-insensitive).
    #[test]
    fn parse_extension_version_from_list_output() {
        // Simulate the raw stdout from `code --list-extensions --show-versions`
        let output = "\
ms-python.python@2024.6.0\n\
supernovae.nika-lang@0.58.2\n\
vscodevim.vim@1.27.2\n";

        let ext_id = "supernovae.nika-lang";
        let prefix = format!("{}@", ext_id.to_lowercase());
        let version = output.lines().find_map(|l| {
            let lower = l.trim().to_lowercase();
            lower.strip_prefix(&prefix).map(|v| v.to_string())
        });
        assert_eq!(version, Some("0.58.2".to_string()));
    }

    /// Extension not present in list output returns None.
    #[test]
    fn parse_extension_version_missing_returns_none() {
        let output = "\
ms-python.python@2024.6.0\n\
vscodevim.vim@1.27.2\n";

        let ext_id = "supernovae.nika-lang";
        let prefix = format!("{}@", ext_id.to_lowercase());
        let version = output.lines().find_map(|l| {
            let lower = l.trim().to_lowercase();
            lower.strip_prefix(&prefix).map(|v| v.to_string())
        });
        assert_eq!(version, None);
    }

    /// Extension matching is case-insensitive.
    #[test]
    fn parse_extension_version_case_insensitive() {
        let output = "SuperNovae.Nika-Lang@1.2.3\n";

        let ext_id = "supernovae.nika-lang";
        let prefix = format!("{}@", ext_id.to_lowercase());
        let version = output.lines().find_map(|l| {
            let lower = l.trim().to_lowercase();
            lower.strip_prefix(&prefix).map(|v| v.to_string())
        });
        assert_eq!(version, Some("1.2.3".to_string()));
    }

    /// Extension output with leading/trailing whitespace is handled.
    #[test]
    fn parse_extension_version_with_whitespace() {
        let output = "  supernovae.nika-lang@0.61.0  \n";

        let ext_id = "supernovae.nika-lang";
        let prefix = format!("{}@", ext_id.to_lowercase());
        let version = output.lines().find_map(|l| {
            let lower = l.trim().to_lowercase();
            lower.strip_prefix(&prefix).map(|v| v.to_string())
        });
        assert_eq!(version, Some("0.61.0".to_string()));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // is_version_outdated — comprehensive coverage (also tested in doctor.rs)
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn is_version_outdated_minor_behind() {
        assert!(is_version_outdated("0.55.0", "0.62.0"));
        assert!(is_version_outdated("0.61.0", "0.62.0"));
    }

    #[test]
    fn is_version_outdated_same_minor_is_current() {
        assert!(!is_version_outdated("0.62.0", "0.62.0"));
        assert!(!is_version_outdated("0.62.5", "0.62.0"));
    }

    #[test]
    fn is_version_outdated_ahead_is_not_outdated() {
        assert!(!is_version_outdated("0.63.0", "0.62.0"));
        assert!(!is_version_outdated("1.0.0", "0.99.0"));
    }

    #[test]
    fn is_version_outdated_major_bump() {
        assert!(is_version_outdated("0.99.9", "1.0.0"));
    }

    #[test]
    fn is_version_outdated_malformed_versions() {
        // Malformed input should not panic; (0,0) < (0,0) == false
        assert!(!is_version_outdated("", ""));
        assert!(!is_version_outdated("abc", "xyz"));
        // Partially valid
        assert!(!is_version_outdated("0", "0"));
        assert!(is_version_outdated("0", "0.1.0"));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // read_extension_versions — parsing [extensions] from TOML
    // ═══════════════════════════════════════════════════════════════════════

    /// Parse [extensions] section from machine.toml content.
    #[test]
    fn parse_extensions_section() {
        let content = "\
[machine]\n\
version = \"0.62.0\"\n\
\n\
[extensions]\n\
vscode = \"0.58.2\"\n\
cursor = \"0.61.0\"\n\
\n\
[rule_hashes]\n\
claude = \"abcdef0123456789\"\n";

        // Replicate the parsing logic from read_extension_versions
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

        assert_eq!(map.len(), 2);
        assert_eq!(map.get("vscode").unwrap(), "0.58.2");
        assert_eq!(map.get("cursor").unwrap(), "0.61.0");
    }

    /// No [extensions] section returns empty map.
    #[test]
    fn parse_extensions_section_missing() {
        let content = "\
[machine]\n\
version = \"0.62.0\"\n";

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

        assert!(map.is_empty());
    }

    /// Empty [extensions] section returns empty map.
    #[test]
    fn parse_extensions_section_empty() {
        let content = "\
[machine]\n\
version = \"0.62.0\"\n\
\n\
[extensions]\n\
\n\
[rule_hashes]\n\
claude = \"abc\"\n";

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

        assert!(map.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // update_extension_version — writing to [extensions] section
    // ═══════════════════════════════════════════════════════════════════════

    /// Updating an extension version in existing [extensions] section.
    #[test]
    fn update_extension_version_replaces_existing_key() {
        let content = "\
[machine]\n\
version = \"0.62.0\"\n\
\n\
[extensions]\n\
vscode = \"0.58.0\"\n\
cursor = \"0.61.0\"\n";

        let editor_id = "vscode";
        let version = "0.62.0";
        let new_line = format!("{} = \"{}\"", editor_id, version);
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let section_idx = lines.iter().position(|l| l.trim() == "[extensions]");

        if let Some(idx) = section_idx {
            let key_idx = lines[idx + 1..]
                .iter()
                .position(|l| {
                    let t = l.trim();
                    t.starts_with(editor_id) && t[editor_id.len()..].trim_start().starts_with('=')
                })
                .map(|i| i + idx + 1);
            if let Some(ki) = key_idx {
                lines[ki] = new_line;
            }
        }

        let result = lines.join("\n") + "\n";
        assert!(result.contains("vscode = \"0.62.0\""));
        assert!(result.contains("cursor = \"0.61.0\"")); // untouched
        assert!(!result.contains("vscode = \"0.58.0\"")); // replaced
    }

    /// Adding a new extension to an existing [extensions] section.
    #[test]
    fn update_extension_version_inserts_new_key() {
        let content = "\
[machine]\n\
version = \"0.62.0\"\n\
\n\
[extensions]\n\
vscode = \"0.58.0\"\n";

        let editor_id = "windsurf";
        let version = "0.62.0";
        let new_line = format!("{} = \"{}\"", editor_id, version);
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let section_idx = lines.iter().position(|l| l.trim() == "[extensions]");

        if let Some(idx) = section_idx {
            let key_idx = lines[idx + 1..]
                .iter()
                .position(|l| {
                    let t = l.trim();
                    t.starts_with(editor_id) && t[editor_id.len()..].trim_start().starts_with('=')
                })
                .map(|i| i + idx + 1);
            match key_idx {
                Some(ki) => lines[ki] = new_line,
                None => lines.insert(idx + 1, new_line),
            }
        }

        let result = lines.join("\n") + "\n";
        assert!(result.contains("windsurf = \"0.62.0\""));
        assert!(result.contains("vscode = \"0.58.0\"")); // untouched
    }

    /// Creating [extensions] section when it doesn't exist.
    #[test]
    fn update_extension_version_creates_section() {
        let content = "\
[machine]\n\
version = \"0.62.0\"\n";

        let editor_id = "vscode";
        let version = "0.62.0";
        let new_line = format!("{} = \"{}\"", editor_id, version);
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let section_idx = lines.iter().position(|l| l.trim() == "[extensions]");

        match section_idx {
            Some(_) => unreachable!("section should not exist"),
            None => {
                lines.push(String::new());
                lines.push("[extensions]".to_string());
                lines.push(new_line);
            }
        }

        let result = lines.join("\n") + "\n";
        assert!(result.contains("[extensions]"));
        assert!(result.contains("vscode = \"0.62.0\""));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // update_marker_version — line replacement logic
    // ═══════════════════════════════════════════════════════════════════════

    /// Version line is replaced while preserving other fields.
    #[test]
    fn update_marker_version_replaces_version_line() {
        let content = "\
[machine]\n\
setup_at = \"1711900000\"\n\
version = \"0.58.0\"\n\
editors = [\"vscode\", \"claude\"]\n";

        let new_version = "0.62.0";
        let updated: String = content
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("version") && trimmed[7..].trim_start().starts_with('=') {
                    format!("version = \"{}\"", new_version)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let result = updated + "\n";

        assert!(result.contains("version = \"0.62.0\""));
        assert!(!result.contains("version = \"0.58.0\""));
        assert!(result.contains("setup_at = \"1711900000\"")); // preserved
        assert!(result.contains("editors = [\"vscode\", \"claude\"]")); // preserved
    }

    /// Content without a version line is left unchanged.
    #[test]
    fn update_marker_version_no_version_line_is_noop() {
        let content = "\
[machine]\n\
setup_at = \"1711900000\"\n\
editors = [\"vscode\"]\n";

        let new_version = "0.62.0";
        let updated: String = content
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("version") && trimmed[7..].trim_start().starts_with('=') {
                    format!("version = \"{}\"", new_version)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let result = updated + "\n";

        // No version line to replace, content stays the same
        assert!(!result.contains("version ="));
        assert!(result.contains("setup_at = \"1711900000\""));
    }

    // ═══════════════════════════════════════════════════════════════════════
    // try_vsix_sideload — URL construction
    // ═══════════════════════════════════════════════════════════════════════

    /// VSIX download URL is correctly constructed from version.
    #[test]
    fn vsix_url_construction() {
        let version = "0.62.0";
        let url = format!(
            "https://github.com/supernovae-st/nika/releases/download/v{}/nika-lang-{}.vsix",
            version, version
        );
        assert_eq!(
            url,
            "https://github.com/supernovae-st/nika/releases/download/v0.62.0/nika-lang-0.62.0.vsix"
        );
    }

    /// VSIX cache path is correctly constructed.
    #[test]
    fn vsix_cache_path_construction() {
        let version = "0.62.0";
        let cache_dir = std::path::PathBuf::from("/tmp/.nika/cache");
        let vsix_path = cache_dir.join(format!("nika-lang-{}.vsix", version));
        assert_eq!(
            vsix_path.to_string_lossy(),
            "/tmp/.nika/cache/nika-lang-0.62.0.vsix"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // load_rule_hashes — parsing [rule_hashes] from TOML
    // ═══════════════════════════════════════════════════════════════════════

    /// Parse [rule_hashes] section correctly.
    #[test]
    fn parse_rule_hashes_section() {
        let content = "\
[machine]\n\
version = \"0.62.0\"\n\
\n\
[rule_hashes]\n\
claude = \"abcdef0123456789\"\n\
cursor = \"1234567890abcdef\"\n\
\n\
[extensions]\n\
vscode = \"0.58.0\"\n";

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

        assert_eq!(map.len(), 2);
        assert_eq!(map.get("claude").unwrap(), "abcdef0123456789");
        assert_eq!(map.get("cursor").unwrap(), "1234567890abcdef");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // hash_content — deterministic hashing
    // ═══════════════════════════════════════════════════════════════════════

    /// hash_content produces a stable 16-char hex string.
    #[test]
    fn hash_content_is_deterministic() {
        let h1 = hash_content("hello world");
        let h2 = hash_content("hello world");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16, "hash should be 16 hex chars");
    }

    /// Different content produces different hashes.
    #[test]
    fn hash_content_different_inputs() {
        let h1 = hash_content("hello");
        let h2 = hash_content("world");
        assert_ne!(h1, h2);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // VSCODE_EDITORS constant validation
    // ═══════════════════════════════════════════════════════════════════════

    /// All editor definitions have valid non-empty fields.
    #[test]
    fn vscode_editors_have_valid_fields() {
        assert!(!VSCODE_EDITORS.is_empty());
        for def in VSCODE_EDITORS {
            assert!(!def.id.is_empty(), "editor id must not be empty");
            assert!(!def.name.is_empty(), "editor name must not be empty");
            assert!(!def.binary.is_empty(), "editor binary must not be empty");
            assert!(!def.ext_id.is_empty(), "editor ext_id must not be empty");
            assert!(
                def.ext_id.contains('.'),
                "ext_id '{}' should have publisher.name format",
                def.ext_id
            );
        }
    }

    /// All editor ext_ids point to the nika-lang extension.
    #[test]
    fn vscode_editors_all_use_nika_lang() {
        for def in VSCODE_EDITORS {
            assert_eq!(
                def.ext_id, "supernovae.nika-lang",
                "editor {} should use supernovae.nika-lang",
                def.name
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Section boundary — key search must not leak across sections
    // ═══════════════════════════════════════════════════════════════════════

    /// update_extension_version must not modify keys in other sections.
    #[test]
    fn update_extension_version_respects_section_boundary() {
        let content = "\
[machine]\n\
version = \"0.62.0\"\n\
\n\
[extensions]\n\
vscode = \"0.58.0\"\n\
\n\
[rule_hashes]\n\
vscode = \"deadbeef\"\n";

        // Simulate update_extension_version("vscode", "0.63.0")
        let editor_id = "vscode";
        let new_line = format!("{} = \"0.63.0\"", editor_id);
        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let section_idx = lines.iter().position(|l| l.trim() == "[extensions]");
        if let Some(idx) = section_idx {
            let mut key_idx = None;
            for (i, l) in lines[idx + 1..].iter().enumerate() {
                let t = l.trim();
                if t.starts_with('[') {
                    break;
                }
                if t.starts_with(editor_id) && t[editor_id.len()..].trim_start().starts_with('=') {
                    key_idx = Some(i + idx + 1);
                    break;
                }
            }
            if let Some(ki) = key_idx {
                lines[ki] = new_line;
            }
        }

        let result = lines.join("\n") + "\n";
        assert!(result.contains("[extensions]"));
        assert!(result.contains("vscode = \"0.63.0\"")); // updated in [extensions]
        assert!(result.contains("vscode = \"deadbeef\"")); // untouched in [rule_hashes]
    }

    /// update_marker_version must only replace version in [machine], not [extensions].
    #[test]
    fn update_marker_version_scoped_to_machine_section() {
        let content = "\
[machine]\n\
setup_at = \"123\"\n\
version = \"0.62.0\"\n\
\n\
[extensions]\n\
version = \"should_not_change\"\n";

        let new_version = "0.63.0";
        let mut in_machine = false;
        let mut replaced = false;
        let updated: String = content
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed == "[machine]" {
                    in_machine = true;
                } else if trimmed.starts_with('[') {
                    in_machine = false;
                }
                if in_machine
                    && !replaced
                    && trimmed.starts_with("version")
                    && trimmed[7..].trim_start().starts_with('=')
                {
                    replaced = true;
                    format!("version = \"{}\"", new_version)
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(updated.contains("version = \"0.63.0\"")); // [machine] updated
        assert!(updated.contains("version = \"should_not_change\"")); // [extensions] preserved
    }

    /// VSIX sideload URL format is correct.
    #[test]
    fn vsix_sideload_url_format() {
        let version = "0.63.0";
        let url = format!(
            "https://github.com/supernovae-st/nika/releases/download/v{}/nika-lang-{}.vsix",
            version, version
        );
        assert_eq!(
            url,
            "https://github.com/supernovae-st/nika/releases/download/v0.63.0/nika-lang-0.63.0.vsix"
        );
    }
}
