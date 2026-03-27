//! Editor detection, rule installation, hash protection, completions, and quick scan.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use colored::Colorize;

use super::status::{machine_toml_path, write_marker, SetupResult};

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

    // Summary: show each configured editor by name
    let ok_results: Vec<&SetupResult> = results.iter().filter(|r| r.success).collect();
    if ok_results.is_empty() {
        println!("  {} No editors detected", "\u{25cb}".dimmed());
    } else {
        println!("  {}", "AI Editors configured:".bold());
        for r in &ok_results {
            println!("    {} {}", "\u{2713}".green(), r.name);
        }
    }

    // 5. Detect env var API keys and suggest keychain migration
    detect_env_api_keys();

    results
}

/// Detect API keys in environment variables and hint about keychain migration.
fn detect_env_api_keys() {
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
        println!();
        println!(
            "  {} API keys detected in env: {}",
            "\u{1f511}".dimmed(),
            found.join(", ").bold()
        );
        println!(
            "    {} {}",
            "\u{2192}".dimmed(),
            "nika init --migrate-keys to move them to the secure keychain".dimmed()
        );
    }
}

fn setup_editors() -> Vec<SetupResult> {
    let mut results = Vec::new();

    let editors: &[(&str, &str, &str)] = &[
        ("VS Code", "code", "supernovae.nika-lang"),
        ("Cursor", "cursor", "supernovae.nika-lang"),
        ("Windsurf", "windsurf", "supernovae.nika-lang"),
    ];

    for (name, binary, ext_id) in editors {
        // Check binary in PATH or macOS .app
        let has_binary = which::which(binary).is_ok() || check_macos_app(name);
        if !has_binary {
            continue;
        }

        // Check if extension already installed
        let has_ext = Command::new(binary)
            .args(["--list-extensions"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|list| list.lines().any(|l| l.eq_ignore_ascii_case(ext_id)))
            .unwrap_or(false);

        if has_ext {
            println!("  {} {} + nika-lang extension", "\u{2713}".green(), name);
            results.push(SetupResult {
                name: name.to_string(),
                success: true,
                message: "already installed".into(),
            });
            continue;
        }

        // Install extension
        print!("  {} {} — installing nika-lang...", "\u{25c7}".cyan(), name);
        let install = Command::new(binary)
            .args(["--install-extension", ext_id])
            .output();

        match install {
            Ok(output) if output.status.success() => {
                println!(
                    "\r  {} {} — nika-lang installed       ",
                    "\u{2713}".green(),
                    name
                );
                results.push(SetupResult {
                    name: name.to_string(),
                    success: true,
                    message: "installed".into(),
                });
            }
            _ => {
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
                "\u{2713}".green()
            );
            results.push(SetupResult {
                name: "Agent Skills".into(),
                success: true,
                message: "installed".into(),
            });
        }
    } else {
        println!("  {} Agent Skills [~/.agents/skills/]", "\u{2713}".green());
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
            let disk_hash = hash_content(&disk_content);

            // Content already matches — nothing to do
            if disk_hash == expected_hash {
                if !silent {
                    println!("  {} {} — up to date", "\u{2713}".green(), name);
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
                println!("  {} {} — Nika rules installed", "\u{2713}".green(), name);
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
/// Daemon provides: keychain secrets, LLM cache, job scheduling, file watch.
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
            "installed + started (keychain, cache, jobs)".into()
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
                    println!(
                        "  {} {} completions installed",
                        "\u{2713}".green(),
                        shell_name
                    );
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
/// Skips if last scan was < 24h ago (cooldown to avoid repeated filesystem checks).
pub fn quick_editor_scan() {
    // Cooldown: skip if scanned recently
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

    if new_editors.is_empty() {
        return;
    }

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
            "cursor" => install_rule(
                &home.join(".cursor/rules/nika.mdc"),
                CURSOR_NIKA_RULES,
                "Cursor",
                "cursor",
                &hashes,
                &mut results,
                true,
            ),
            "copilot" => install_rule(
                &home.join(".github/copilot/nika.instructions.md"),
                COPILOT_RULES,
                "Copilot",
                "copilot",
                &hashes,
                &mut results,
                true,
            ),
            "windsurf" => install_rule(
                &home.join(".windsurf/rules/nika.md"),
                WINDSURF_RULES,
                "Windsurf",
                "windsurf",
                &hashes,
                &mut results,
                true,
            ),
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
