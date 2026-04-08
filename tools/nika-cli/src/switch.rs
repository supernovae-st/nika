// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika switch` — dual channel management (dev / release)
//!
//! Manages two side-by-side installations of Nika with animated switching:
//! - **release**: installed via Homebrew (`/opt/homebrew/bin/nika`)
//! - **dev**: auto-built from the repo on every commit (`~/.nika/bin/nika-dev`)

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::Utc;
use clap::Subcommand;
use colored::{Colorize, CustomColor};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};

use nika_engine::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// SOLARIZED PALETTE
// ═══════════════════════════════════════════════════════════════════════════

const SOL_CYAN: CustomColor = CustomColor {
    r: 42,
    g: 161,
    b: 152,
};
const SOL_MAGENTA: CustomColor = CustomColor {
    r: 211,
    g: 54,
    b: 130,
};
const SOL_GREEN: CustomColor = CustomColor {
    r: 133,
    g: 153,
    b: 0,
};
const SOL_DIM: CustomColor = CustomColor {
    r: 88,
    g: 110,
    b: 117,
};
const SOL_YELLOW: CustomColor = CustomColor {
    r: 181,
    g: 137,
    b: 0,
};
const SOL_RED: CustomColor = CustomColor {
    r: 220,
    g: 50,
    b: 47,
};

// ═══════════════════════════════════════════════════════════════════════════
// CLI TYPES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Subcommand, Debug, Clone)]
pub enum SwitchAction {
    /// Switch to development channel (latest local build)
    Dev,
    /// Switch to release channel (Homebrew)
    Release,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BuildMeta {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub built_at: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub repo_dir: String,
}

#[derive(Debug)]
pub struct ChannelInfo {
    pub name: String,
    pub version: String,
    pub detail: String,
    pub binary_path: PathBuf,
    pub available: bool,
}

// ═══════════════════════════════════════════════════════════════════════════
// MAIN HANDLER
// ═══════════════════════════════════════════════════════════════════════════

pub fn handle_switch_command(action: Option<SwitchAction>, quiet: bool) -> Result<(), NikaError> {
    let nika_dir = nika_home()?;

    match action {
        None => render_status(&nika_dir, quiet),
        Some(SwitchAction::Dev) => do_switch(&nika_dir, "dev", quiet),
        Some(SwitchAction::Release) => do_switch(&nika_dir, "release", quiet),
    }
}

pub async fn do_setup(quiet: bool) -> Result<(), NikaError> {
    let nika_dir = nika_home()?;

    if !quiet {
        println!();
        println!(
            "  {} {}",
            "✦".custom_color(SOL_YELLOW),
            "Setting up Nika channels...".bold()
        );
    }

    // 1. Create directories
    let bin_dir = nika_dir.join("bin");
    let builds_dir = nika_dir.join("builds");
    std::fs::create_dir_all(&bin_dir)
        .map_err(|e| NikaError::Execution(format!("Failed to create ~/.nika/bin/: {e}")))?;
    std::fs::create_dir_all(&builds_dir)
        .map_err(|e| NikaError::Execution(format!("Failed to create ~/.nika/builds/: {e}")))?;
    if !quiet {
        println!("  {} Created ~/.nika/bin/", "✓".custom_color(SOL_GREEN));
    }

    // 2. Detect release binary
    let release = detect_release();
    if let Some(ref r) = release {
        if !quiet {
            println!(
                "  {} Found release: {} ({})",
                "✓".custom_color(SOL_GREEN),
                r.binary_path.display(),
                r.version.custom_color(SOL_CYAN)
            );
        }
    } else if !quiet {
        println!(
            "  {} Release not found (install: brew install supernovae-st/tap/nika)",
            "○".custom_color(SOL_DIM)
        );
    }

    // 3. Build dev
    let repo_dir = find_repo_root()?;
    if !quiet {
        let hash = current_git_hash(&repo_dir);
        println!(
            "  {} Building dev from main @ {}...",
            "⣾".custom_color(SOL_MAGENTA),
            hash.custom_color(SOL_MAGENTA)
        );
    }
    let duration = run_cargo_build(&repo_dir, quiet)?;
    let built_binary = repo_dir.join("tools/target/release/nika");
    let dev_binary = bin_dir.join("nika-dev");
    std::fs::copy(&built_binary, &dev_binary)
        .map_err(|e| NikaError::Execution(format!("Failed to copy dev binary: {e}")))?;
    write_build_meta(&builds_dir, &repo_dir)?;
    if !quiet {
        println!(
            "  {} Dev ready: {} ({:.0}s)",
            "✓".custom_color(SOL_GREEN),
            format!("v{}", env!("CARGO_PKG_VERSION")).custom_color(SOL_MAGENTA),
            duration.as_secs_f64()
        );
    }

    // 4. Install git hook
    install_git_hook(&repo_dir)?;
    if !quiet {
        println!("  {} Git hook installed", "✓".custom_color(SOL_GREEN));
    }

    // 5. Create symlink — default to dev
    let channel = "dev";
    let symlink_path = bin_dir.join("nika");
    let _ = std::fs::remove_file(&symlink_path);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&dev_binary, &symlink_path)
        .map_err(|e| NikaError::Execution(format!("Failed to create symlink: {e}")))?;
    std::fs::write(nika_dir.join("channel"), channel)
        .map_err(|e| NikaError::Execution(format!("Failed to write channel file: {e}")))?;
    if !quiet {
        println!(
            "  {} Active channel: {}",
            "✓".custom_color(SOL_GREEN),
            channel.custom_color(SOL_GREEN).bold()
        );
        println!();
        println!("  Add to ~/.zshrc:");
        println!(
            "    {}",
            "export PATH=\"$HOME/.nika/bin:$PATH\"".custom_color(SOL_CYAN)
        );
        println!();
    }

    Ok(())
}

pub async fn do_build(quiet: bool) -> Result<(), NikaError> {
    let nika_dir = nika_home()?;
    let repo_dir = find_repo_root()?;
    let bin_dir = nika_dir.join("bin");
    let builds_dir = nika_dir.join("builds");

    std::fs::create_dir_all(&bin_dir).ok();
    std::fs::create_dir_all(&builds_dir).ok();

    let duration = if quiet {
        run_cargo_build(&repo_dir, true)?
    } else {
        run_cargo_build_animated(&repo_dir)?
    };

    // Copy binary
    let built = repo_dir.join("tools/target/release/nika");
    let dest = bin_dir.join("nika-dev");
    std::fs::copy(&built, &dest)
        .map_err(|e| NikaError::Execution(format!("Failed to copy dev binary: {e}")))?;
    write_build_meta(&builds_dir, &repo_dir)?;

    if !quiet {
        let meta = read_build_meta(&builds_dir);
        print_banner(
            "Dev build complete",
            &format!(
                "v{}-dev @ {} · compiled in {:.0}s",
                meta.version,
                meta.hash.custom_color(SOL_MAGENTA),
                duration.as_secs_f64()
            ),
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// CHANNEL DETECTION
// ═══════════════════════════════════════════════════════════════════════════

fn detect_release() -> Option<ChannelInfo> {
    let candidates = [
        PathBuf::from("/opt/homebrew/bin/nika"),
        PathBuf::from("/usr/local/bin/nika"),
    ];

    for path in &candidates {
        if path.exists() {
            if let Some(version) = query_binary_version(path) {
                return Some(ChannelInfo {
                    name: "release".into(),
                    version,
                    detail: "homebrew".into(),
                    binary_path: path.clone(),
                    available: true,
                });
            }
        }
    }

    None
}

fn detect_dev() -> Option<ChannelInfo> {
    let nika_dir = nika_home().ok()?;
    let binary = nika_dir.join("bin/nika-dev");
    if !binary.exists() {
        return None;
    }

    let builds_dir = nika_dir.join("builds");
    let meta = read_build_meta(&builds_dir);

    let detail = if meta.status == "failed" {
        "✗ last build failed".to_string()
    } else if !meta.hash.is_empty() {
        let ago = relative_time(&meta.built_at);
        format!("{} · {}", meta.hash, ago)
    } else {
        "local build".into()
    };

    let version = if meta.version.is_empty() {
        query_binary_version(&binary).unwrap_or_else(|| "?".into())
    } else {
        meta.version
    };

    Some(ChannelInfo {
        name: "dev".into(),
        version,
        detail,
        binary_path: binary,
        available: meta.status != "failed",
    })
}

fn active_channel(nika_dir: &Path) -> String {
    std::fs::read_to_string(nika_dir.join("channel"))
        .unwrap_or_else(|_| "release".into())
        .trim()
        .to_string()
}

fn query_binary_version(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("--version").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .strip_prefix("nika ")
        .map(|s| s.split_whitespace().next().unwrap_or(s).to_string())
}

/// Compare two semver strings. Returns Ordering.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse =
        |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse::<u32>().ok()).collect() };
    parse(a).cmp(&parse(b))
}

// ═══════════════════════════════════════════════════════════════════════════
// SWITCH LOGIC
// ═══════════════════════════════════════════════════════════════════════════

fn do_switch(nika_dir: &Path, target: &str, quiet: bool) -> Result<(), NikaError> {
    let current = active_channel(nika_dir);
    let bin_dir = nika_dir.join("bin");
    let symlink_path = bin_dir.join("nika");

    if !bin_dir.exists() {
        return Err(NikaError::ValidationError {
            reason: "~/.nika/bin/ not found. Run `nika switch --setup` first.".into(),
        });
    }

    // Resolve + validate target binary
    #[allow(unused_variables)]
    let target_binary = match target {
        "dev" => {
            let p = bin_dir.join("nika-dev");
            if !p.exists() {
                return Err(NikaError::ValidationError {
                    reason: "Dev binary not found. Run `nika switch --setup` first.".into(),
                });
            }

            // Auto-rebuild if stale
            if let Ok(repo_dir) = find_repo_root() {
                let head = current_git_hash(&repo_dir);
                let builds_dir = nika_dir.join("builds");
                let meta = read_build_meta(&builds_dir);
                if !meta.hash.is_empty() && meta.hash != head {
                    if !quiet {
                        println!(
                            "\n  {} Dev binary is stale ({} → {}), rebuilding...",
                            "⚠".custom_color(SOL_YELLOW),
                            meta.hash.custom_color(SOL_DIM),
                            head.custom_color(SOL_MAGENTA),
                        );
                    }
                    let duration = if quiet {
                        run_cargo_build(&repo_dir, true)?
                    } else {
                        run_cargo_build_animated(&repo_dir)?
                    };
                    let built = repo_dir.join("tools/target/release/nika");
                    std::fs::copy(&built, &p).map_err(|e| {
                        NikaError::Execution(format!("Failed to copy dev binary: {e}"))
                    })?;
                    write_build_meta(&builds_dir, &repo_dir)?;
                    if !quiet {
                        println!(
                            "  {} Rebuilt in {:.0}s",
                            "✓".custom_color(SOL_GREEN),
                            duration.as_secs_f64()
                        );
                    }
                }
            }

            p
        }
        "release" => {
            let info = detect_release().ok_or_else(|| NikaError::ValidationError {
                reason:
                    "Release binary not found. Install with: brew install supernovae-st/tap/nika"
                        .into(),
            })?;
            info.binary_path
        }
        _ => {
            return Err(NikaError::ValidationError {
                reason: format!("Unknown channel '{target}'. Use 'dev' or 'release'."),
            });
        }
    };

    if current == target {
        if !quiet {
            render_status(nika_dir, false)?;
            println!(
                "  {}",
                format!("Already on {target} channel.").custom_color(SOL_DIM)
            );
        }
        return Ok(());
    }

    // Atomic swap
    let _ = std::fs::remove_file(&symlink_path);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_binary, &symlink_path)
        .map_err(|e| NikaError::Execution(format!("Failed to create symlink: {e}")))?;

    // Write channel
    std::fs::write(nika_dir.join("channel"), target)
        .map_err(|e| NikaError::Execution(format!("Failed to write channel: {e}")))?;

    if quiet {
        return Ok(());
    }

    // Animate transition
    animate_transition(&current, target, nika_dir)?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// STATUS DISPLAY
// ═══════════════════════════════════════════════════════════════════════════

fn render_status(nika_dir: &Path, _quiet: bool) -> Result<(), NikaError> {
    let release = detect_release();
    let dev = detect_dev();
    let current = active_channel(nika_dir);
    let has_repo = find_repo_root().is_ok();
    let has_dev = dev.is_some();
    let has_release = release.is_some();

    println!();
    println!(
        "    {}{}{}",
        "}{".custom_color(SOL_CYAN),
        "  N I K A  ".custom_color(SOL_CYAN).bold(),
        "}{".custom_color(SOL_CYAN),
    );
    println!();

    // Release line
    println!(
        "    {}",
        "stable — installed via Homebrew, updated with `brew upgrade`".custom_color(SOL_DIM),
    );
    print_channel_line("release", &release, &current);

    // Dev line (only if dev exists or we're in the repo)
    if has_dev || has_repo {
        println!("               {}", "│".custom_color(SOL_DIM));
        println!(
            "    {}",
            "dev — built from source, auto-rebuilds on commit".custom_color(SOL_DIM),
        );
        print_channel_line("dev", &dev, &current);
    }
    println!();

    // Version comparison hint
    if let (Some(ref rel), Some(ref d)) = (&release, &dev) {
        let cmp = compare_versions(&rel.version, &d.version);
        match cmp {
            std::cmp::Ordering::Greater if current == "dev" => {
                println!(
                    "    {} release {} is newer than dev {} — run {}",
                    "↑".custom_color(SOL_YELLOW),
                    format!("v{}", rel.version).custom_color(SOL_CYAN).bold(),
                    format!("v{}", d.version).custom_color(SOL_DIM),
                    "nika switch release".cyan().bold(),
                );
                println!();
            }
            std::cmp::Ordering::Less if current == "release" => {
                println!(
                    "    {} dev {} is newer than release {} — run {}",
                    "↑".custom_color(SOL_YELLOW),
                    format!("v{}", d.version).custom_color(SOL_MAGENTA).bold(),
                    format!("v{}", rel.version).custom_color(SOL_DIM),
                    "nika switch dev".cyan().bold(),
                );
                println!();
            }
            _ => {}
        }
    }

    // Staleness check for dev channel
    if current == "dev" {
        if let Ok(repo_dir) = find_repo_root() {
            let head = current_git_hash(&repo_dir);
            let builds_dir = nika_dir.join("builds");
            let meta = read_build_meta(&builds_dir);
            if !meta.hash.is_empty() && meta.hash != head {
                println!(
                    "    {} dev binary is {} ({} → {}), run {}",
                    "⚠".custom_color(SOL_YELLOW),
                    "stale".custom_color(SOL_YELLOW).bold(),
                    meta.hash.custom_color(SOL_DIM),
                    head.custom_color(SOL_MAGENTA),
                    "nika switch dev".cyan().bold(),
                );
                println!();
            }
        }
    }

    // Context-aware help
    match (has_dev, has_release, has_repo) {
        (true, true, _) => {
            let other = if current == "dev" { "release" } else { "dev" };
            println!(
                "    {} {}  {}",
                "↳".custom_color(SOL_DIM),
                format!("nika switch {other}").cyan(),
                format!("switch to {other}").custom_color(SOL_DIM),
            );
        }
        (false, true, true) => {
            println!(
                "    {} {}",
                "💡".custom_color(SOL_DIM),
                "Nika repo detected — build dev channel:".custom_color(SOL_DIM),
            );
            println!(
                "    {} {}",
                "↳".custom_color(SOL_DIM),
                "nika switch --setup".cyan(),
            );
        }
        (false, true, false) => {
            println!(
                "    {} {}",
                "↳".custom_color(SOL_DIM),
                "brew upgrade nika".cyan(),
            );
        }
        (true, false, true) => {
            println!(
                "    {} {}",
                "↳".custom_color(SOL_DIM),
                "nika switch dev".cyan(),
            );
        }
        (false, false, true) => {
            println!(
                "    {} {}",
                "↳".custom_color(SOL_DIM),
                "nika switch --setup".cyan(),
            );
        }
        _ => {}
    }
    println!();

    Ok(())
}

fn print_channel_line(name: &str, info: &Option<ChannelInfo>, active: &str) {
    let is_active = name == active;
    let dot = if is_active { "●" } else { "○" };
    let label = if is_active {
        "━━━━━━━ active"
    } else {
        ""
    };

    let name_color = if is_active {
        if name == "release" {
            SOL_CYAN
        } else {
            SOL_MAGENTA
        }
    } else {
        SOL_DIM
    };

    let dot_str = if is_active {
        dot.custom_color(SOL_GREEN).bold().to_string()
    } else {
        dot.custom_color(SOL_DIM).to_string()
    };

    let label_str = if is_active {
        label.custom_color(SOL_GREEN).bold().to_string()
    } else {
        String::new()
    };

    match info {
        Some(ch) => {
            let ver = format!("v{}", ch.version);
            let ver_str = if is_active {
                ver.custom_color(name_color).bold().to_string()
            } else {
                ver.custom_color(SOL_DIM).to_string()
            };
            let detail_str = if ch.detail.starts_with('✗') {
                ch.detail.custom_color(SOL_RED).to_string()
            } else {
                ch.detail.custom_color(SOL_DIM).to_string()
            };
            println!(
                "    {:<10} {:>10}  {} {} {}",
                name.custom_color(name_color).bold(),
                ver_str,
                dot_str,
                label_str,
                detail_str
            );
        }
        None => {
            println!(
                "    {:<10} {:>10}  {} {}",
                name.custom_color(SOL_DIM),
                "---".custom_color(SOL_DIM),
                dot.custom_color(SOL_DIM),
                "not installed".custom_color(SOL_DIM),
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ANIMATION
// ═══════════════════════════════════════════════════════════════════════════

fn animate_transition(_from: &str, to: &str, _nika_dir: &Path) -> Result<(), NikaError> {
    let release = detect_release();
    let dev = detect_dev();

    // Frame 1: spark traveling
    println!();
    print_compact_line_dim("release", &release);
    println!(
        "               {}  {}",
        "┃".custom_color(SOL_DIM),
        "✦".custom_color(SOL_YELLOW).bold()
    );
    print_compact_line_dim("dev", &dev);
    std::thread::sleep(Duration::from_millis(200));
    clear_lines(4);

    // Frame 2: arrived
    println!();
    print_compact_line("release", &release, to);
    print_compact_line("dev", &dev, to);
    std::thread::sleep(Duration::from_millis(200));
    clear_lines(3);

    // Frame 3: confirmation banner
    let target_info = if to == "dev" { &dev } else { &release };
    let detail = match target_info {
        Some(ch) => format!("v{}{}", ch.version, if to == "dev" { "-dev" } else { "" }),
        None => "unknown".into(),
    };
    let sub = match target_info {
        Some(ch) => format!("{} @ {}", detail, ch.detail),
        None => detail,
    };
    println!();
    print_banner(&format!("Switched to {to}"), &sub);

    Ok(())
}

fn print_compact_line(name: &str, info: &Option<ChannelInfo>, active: &str) {
    let is_active = name == active;
    let dot = if is_active { "●" } else { "○" };
    let label = if is_active { "━━━━ active" } else { "" };
    let nc = if name == "release" {
        SOL_CYAN
    } else {
        SOL_MAGENTA
    };

    match info {
        Some(ch) => {
            if is_active {
                println!(
                    "    {:<10} {:>10}  {} {}",
                    name.custom_color(nc).bold(),
                    format!("v{}", ch.version).custom_color(nc).bold(),
                    dot.custom_color(SOL_GREEN).bold(),
                    label.custom_color(SOL_GREEN).bold(),
                );
            } else {
                println!(
                    "    {:<10} {:>10}  {}",
                    name.custom_color(SOL_DIM),
                    format!("v{}", ch.version).custom_color(SOL_DIM),
                    dot.custom_color(SOL_DIM),
                );
            }
        }
        None => {
            println!(
                "    {:<10} {:>10}  {}",
                name.custom_color(SOL_DIM),
                "---".custom_color(SOL_DIM),
                dot.custom_color(SOL_DIM),
            );
        }
    }
}

fn print_compact_line_dim(name: &str, info: &Option<ChannelInfo>) {
    match info {
        Some(ch) => {
            println!(
                "    {:<10} {:>10}  {}",
                name.custom_color(SOL_DIM),
                format!("v{}", ch.version).custom_color(SOL_DIM),
                "○".custom_color(SOL_DIM),
            );
        }
        None => {
            println!(
                "    {:<10} {:>10}  {}",
                name.custom_color(SOL_DIM),
                "---".custom_color(SOL_DIM),
                "○".custom_color(SOL_DIM),
            );
        }
    }
}

fn print_banner(title: &str, subtitle: &str) {
    let max_len = title.len().max(subtitle.len()) + 4;
    let border = "─".repeat(max_len);
    println!(
        "    {}{}{}",
        "╭".custom_color(SOL_DIM),
        border.custom_color(SOL_DIM),
        "╮".custom_color(SOL_DIM)
    );
    println!(
        "    {}  {} {:<width$}{}",
        "│".custom_color(SOL_DIM),
        "✦".custom_color(SOL_YELLOW),
        title.bold(),
        "│".custom_color(SOL_DIM),
        width = max_len - 3,
    );
    println!(
        "    {}  {:<width$}{}",
        "│".custom_color(SOL_DIM),
        subtitle.custom_color(SOL_DIM),
        "│".custom_color(SOL_DIM),
        width = max_len - 1,
    );
    println!(
        "    {}{}{}",
        "╰".custom_color(SOL_DIM),
        border.custom_color(SOL_DIM),
        "╯".custom_color(SOL_DIM)
    );
    println!();
}

fn clear_lines(n: usize) {
    let mut stdout = std::io::stdout();
    for _ in 0..n {
        write!(stdout, "\x1b[A\x1b[2K").ok();
    }
    stdout.flush().ok();
}

// ═══════════════════════════════════════════════════════════════════════════
// BUILD HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn run_cargo_build(repo_dir: &Path, quiet: bool) -> Result<Duration, NikaError> {
    let start = Instant::now();
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(repo_dir.join("tools/nika"))
        .stdout(if quiet {
            Stdio::null()
        } else {
            Stdio::inherit()
        })
        .stderr(if quiet {
            Stdio::null()
        } else {
            Stdio::inherit()
        })
        .status()
        .map_err(|e| NikaError::Execution(format!("Failed to run cargo build: {e}")))?;

    if !status.success() {
        return Err(NikaError::Execution(
            "cargo build --release failed. Check ~/.nika/builds/last.log".into(),
        ));
    }
    Ok(start.elapsed())
}

fn run_cargo_build_animated(repo_dir: &Path) -> Result<Duration, NikaError> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"])
            .template("  {spinner:.magenta} {msg}")
            .unwrap(),
    );
    spinner.set_message("Building nika-dev from main...");
    spinner.enable_steady_tick(Duration::from_millis(80));

    let start = Instant::now();
    let child = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(repo_dir.join("tools/nika"))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| NikaError::Execution(format!("Failed to spawn cargo: {e}")))?;

    let output = child
        .wait_with_output()
        .map_err(|e| NikaError::Execution(format!("Build process error: {e}")))?;

    let duration = start.elapsed();
    spinner.finish_and_clear();

    if !output.status.success() {
        let log_dir = nika_home().ok().map(|d| d.join("builds"));
        if let Some(dir) = log_dir {
            std::fs::create_dir_all(&dir).ok();
            std::fs::write(dir.join("last.log"), &output.stderr).ok();
        }
        return Err(NikaError::Execution(
            "cargo build --release failed. Check ~/.nika/builds/last.log".into(),
        ));
    }

    Ok(duration)
}

fn write_build_meta(builds_dir: &Path, repo_dir: &Path) -> Result<(), NikaError> {
    let hash = current_git_hash(repo_dir);
    let version = env!("CARGO_PKG_VERSION").to_string();
    let meta = BuildMeta {
        version,
        hash,
        built_at: Utc::now().to_rfc3339(),
        status: "ok".into(),
        repo_dir: repo_dir.to_string_lossy().to_string(),
    };
    let json = serde_json::to_string_pretty(&meta)
        .map_err(|e| NikaError::Execution(format!("Failed to serialize build meta: {e}")))?;
    std::fs::write(builds_dir.join("dev.json"), json)
        .map_err(|e| NikaError::Execution(format!("Failed to write dev.json: {e}")))?;
    Ok(())
}

fn read_build_meta(builds_dir: &Path) -> BuildMeta {
    std::fs::read_to_string(builds_dir.join("dev.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

// ═══════════════════════════════════════════════════════════════════════════
// GIT HOOK
// ═══════════════════════════════════════════════════════════════════════════

const HOOK_MARKER: &str = "# Nika auto-build";

const HOOK_SCRIPT: &str = r#"
# Nika auto-build — background rebuild on commit
NIKA_DIR="$HOME/.nika"
REPO_DIR="$(git rev-parse --show-toplevel)"
LOG="$NIKA_DIR/builds/last.log"
LOCK="$NIKA_DIR/builds/build.lock"

# Debounce: skip if build started < 10s ago
if [ -f "$LOCK" ]; then
  lock_age=$(( $(date +%s) - $(cat "$LOCK") ))
  [ "$lock_age" -lt 10 ] && exit 0
fi
date +%s > "$LOCK"

nohup sh -c '
  cd "$1/tools/nika"
  cargo build --release > "$2" 2>&1
  if [ $? -eq 0 ]; then
    cp "$1/tools/target/release/nika" "$3/bin/nika-dev"
    hash=$(git -C "$1" rev-parse --short HEAD)
    version=$(grep "^version" "$1/tools/nika/Cargo.toml" | head -1 | cut -d\" -f2)
    printf "{\"version\":\"%s\",\"hash\":\"%s\",\"built_at\":\"%s\",\"status\":\"ok\"}" \
      "$version" "$hash" "$(date -u +%FT%TZ)" > "$3/builds/dev.json"
  else
    printf "{\"status\":\"failed\",\"built_at\":\"%s\"}" \
      "$(date -u +%FT%TZ)" > "$3/builds/dev.json"
  fi
  rm -f "$3/builds/build.lock"
' _ "$REPO_DIR" "$LOG" "$NIKA_DIR" > /dev/null 2>&1 &
"#;

pub fn install_git_hook(repo_dir: &Path) -> Result<(), NikaError> {
    let hooks_dir = repo_dir.join(".git/hooks");
    if !hooks_dir.exists() {
        return Err(NikaError::ValidationError {
            reason: "Not a git repository (no .git/hooks/ directory)".into(),
        });
    }

    let hook_path = hooks_dir.join("post-commit");

    if hook_path.exists() {
        let content = std::fs::read_to_string(&hook_path)
            .map_err(|e| NikaError::Execution(format!("Failed to read post-commit hook: {e}")))?;
        if content.contains(HOOK_MARKER) {
            return Ok(());
        }
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&hook_path)
            .map_err(|e| NikaError::Execution(format!("Failed to open post-commit hook: {e}")))?;
        writeln!(file, "\n{HOOK_SCRIPT}")
            .map_err(|e| NikaError::Execution(format!("Failed to append to hook: {e}")))?;
    } else {
        std::fs::write(&hook_path, format!("#!/bin/sh\n{HOOK_SCRIPT}"))
            .map_err(|e| NikaError::Execution(format!("Failed to write post-commit hook: {e}")))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| NikaError::Execution(format!("Failed to chmod hook: {e}")))?;
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// UTILITIES
// ═══════════════════════════════════════════════════════════════════════════

fn nika_home() -> Result<PathBuf, NikaError> {
    dirs::home_dir()
        .map(|h| h.join(".nika"))
        .ok_or_else(|| NikaError::ValidationError {
            reason: "Could not determine home directory".into(),
        })
}

fn find_repo_root() -> Result<PathBuf, NikaError> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| NikaError::Execution(format!("git not found: {e}")))?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Ok(PathBuf::from(path));
    }

    // Fallback: saved repo_dir from last build
    if let Ok(nika_dir) = nika_home() {
        let meta = read_build_meta(&nika_dir.join("builds"));
        if !meta.repo_dir.is_empty() {
            let saved = PathBuf::from(&meta.repo_dir);
            if saved.join(".git").exists() {
                return Ok(saved);
            }
        }
    }

    Err(NikaError::ValidationError {
    reason: "Not inside a git repository and no previous build found. Run from the nika repo or run `nika switch --setup` first.".into(),
  })
}

fn current_git_hash(repo_dir: &Path) -> String {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(repo_dir)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

pub fn relative_time(iso: &str) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(iso);
    match parsed {
        Ok(dt) => {
            let now = Utc::now();
            let delta = now.signed_duration_since(dt).num_seconds().max(0) as u64;
            if delta < 60 {
                "just now".into()
            } else if delta < 3600 {
                format!("{}min ago", delta / 60)
            } else if delta < 86400 {
                format!("{}h ago", delta / 3600)
            } else {
                format!("{}d ago", delta / 86400)
            }
        }
        Err(_) => "unknown".into(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_meta_parse() {
        let json = r#"{"version":"0.51.0","hash":"abc1234","built_at":"2026-03-30T14:23:00Z","status":"ok"}"#;
        let meta: BuildMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.version, "0.51.0");
        assert_eq!(meta.hash, "abc1234");
        assert_eq!(meta.status, "ok");
    }

    #[test]
    fn test_build_meta_failed() {
        let json = r#"{"status":"failed","built_at":"2026-03-30T14:23:00Z"}"#;
        let meta: BuildMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.status, "failed");
        assert!(meta.version.is_empty());
        assert!(meta.hash.is_empty());
    }

    #[test]
    fn test_build_meta_default() {
        let meta = BuildMeta::default();
        assert!(meta.version.is_empty());
        assert_eq!(meta.status, "");
    }

    #[test]
    fn test_relative_time_just_now() {
        let now = Utc::now().to_rfc3339();
        assert_eq!(relative_time(&now), "just now");
    }

    #[test]
    fn test_relative_time_minutes() {
        let past = (Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
        assert_eq!(relative_time(&past), "5min ago");
    }

    #[test]
    fn test_relative_time_hours() {
        let past = (Utc::now() - chrono::Duration::hours(3)).to_rfc3339();
        assert_eq!(relative_time(&past), "3h ago");
    }

    #[test]
    fn test_relative_time_days() {
        let past = (Utc::now() - chrono::Duration::days(2)).to_rfc3339();
        assert_eq!(relative_time(&past), "2d ago");
    }

    #[test]
    fn test_relative_time_invalid() {
        assert_eq!(relative_time("not-a-date"), "unknown");
    }

    #[test]
    fn test_active_channel_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(active_channel(tmp.path()), "release");
    }

    #[test]
    fn test_active_channel_reads_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("channel"), "dev\n").unwrap();
        assert_eq!(active_channel(tmp.path()), "dev");
    }

    #[test]
    fn test_hook_marker_in_script() {
        assert!(HOOK_SCRIPT.contains(HOOK_MARKER));
    }

    #[test]
    fn test_hook_has_debounce() {
        assert!(HOOK_SCRIPT.contains("build.lock"));
        assert!(HOOK_SCRIPT.contains("lock_age"));
    }

    #[test]
    fn test_install_hook_new() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join(".git/hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();

        install_git_hook(tmp.path()).unwrap();

        let hook = std::fs::read_to_string(hooks_dir.join("post-commit")).unwrap();
        assert!(hook.starts_with("#!/bin/sh"));
        assert!(hook.contains(HOOK_MARKER));
    }

    #[test]
    fn test_install_hook_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join(".git/hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();

        install_git_hook(tmp.path()).unwrap();
        install_git_hook(tmp.path()).unwrap();

        let hook = std::fs::read_to_string(hooks_dir.join("post-commit")).unwrap();
        assert_eq!(hook.matches(HOOK_MARKER).count(), 1);
    }

    #[test]
    fn test_install_hook_append() {
        let tmp = tempfile::tempdir().unwrap();
        let hooks_dir = tmp.path().join(".git/hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();

        let hook_path = hooks_dir.join("post-commit");
        std::fs::write(&hook_path, "#!/bin/sh\necho 'existing hook'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        install_git_hook(tmp.path()).unwrap();

        let hook = std::fs::read_to_string(&hook_path).unwrap();
        assert!(hook.contains("existing hook"));
        assert!(hook.contains(HOOK_MARKER));
    }

    #[test]
    fn test_compare_versions() {
        assert_eq!(
            compare_versions("0.65.1", "0.64.0"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_versions("0.64.0", "0.65.1"),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_versions("0.65.1", "0.65.1"),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            compare_versions("1.0.0", "0.99.99"),
            std::cmp::Ordering::Greater
        );
    }
}
