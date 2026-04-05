# Unified DX Journey Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Unify the new-user journey so `nika init` does machine auto-setup + project wizard in one flow, `nika doctor` shows a sectioned status board with `--fix`, and bare `nika` adapts to context.

**Architecture:** 3 changes to existing commands. No new crates. Machine state tracked via `~/.nika/machine.toml` marker file. `nika setup` stays as internal escape hatch but is no longer the primary flow.

**Tech Stack:** Rust, cliclack, colored, dirs, which, toml, std::process::Command

---

## Batch 1: Machine auto-setup in `nika init`

### Task 1: Create machine setup module with marker file

Create a new module `tools/nika-cli/src/machine.rs` that handles the machine-level auto-setup (Phase 1). This is the core new code.

**Files:**
- Create: `tools/nika-cli/src/machine.rs`
- Modify: `tools/nika-cli/src/lib.rs` (add `pub mod machine;`)

**Step 1: Create machine.rs**

```rust
//! Machine-level auto-setup for Nika.
//!
//! Detects installed editors/AI tools and configures them automatically.
//! Tracks setup state via `~/.nika/machine.toml` marker file.
//! Called by `nika init` before the project wizard (Phase 1).

use std::path::PathBuf;
use std::process::Command;

use colored::Colorize;

/// Machine setup state persisted at ~/.nika/machine.toml
#[derive(Debug)]
pub struct MachineState {
    pub setup_at: String,
    pub version: String,
    pub editors: Vec<String>,
    pub ai_tools: Vec<String>,
    pub completions: Option<String>,
}

/// Result of a single setup action.
#[derive(Debug)]
struct SetupResult {
    name: String,
    success: bool,
    message: String,
}

/// Check if machine setup has been done (marker file exists + version current).
pub fn is_machine_setup() -> bool {
    let marker = machine_toml_path();
    if !marker.exists() {
        return false;
    }
    // Check version matches current
    if let Ok(content) = std::fs::read_to_string(&marker) {
        content.contains(&format!("version = \"{}\"", env!("CARGO_PKG_VERSION")))
    } else {
        false
    }
}

/// Path to the machine marker file.
fn machine_toml_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".nika")
        .join("machine.toml")
}

/// Run the full machine auto-setup (Phase 1).
///
/// Detects editors and AI tools, installs extensions/rules/completions.
/// Prints progress as it goes. Writes marker file on success.
/// This is SILENT by design — no questions asked, just detect and install.
pub fn run_machine_setup() -> Vec<SetupResult> {
    let mut results = Vec::new();

    println!();
    println!("  {}", "Machine setup".bold().underline());

    // 1. Editors: detect + install extension
    results.extend(setup_editors());

    // 2. AI tools: detect + install rules
    results.extend(setup_ai_rules());

    // 3. Shell completions
    results.push(setup_completions());

    // Write marker file
    write_marker(&results);

    // Summary line
    let ok = results.iter().filter(|r| r.success).count();
    let total = results.len();
    println!();
    println!(
        "  {} Machine ready ({}/{} configured)",
        "\u{2713}".green(),
        ok,
        total
    );

    results
}

fn setup_editors() -> Vec<SetupResult> {
    let mut results = Vec::new();

    let editors: &[(&str, &str, &str)] = &[
        ("VS Code", "code", "supernovae-st.nika-lang"),
        ("Cursor", "cursor", "supernovae-st.nika-lang"),
        ("Windsurf", "windsurf", "supernovae-st.nika-lang"),
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
            println!(
                "  {} {} + nika-lang extension",
                "\u{2713}".green(),
                name
            );
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
                println!("\r  {} {} — nika-lang installed       ", "\u{2713}".green(), name);
                results.push(SetupResult {
                    name: name.to_string(),
                    success: true,
                    message: "installed".into(),
                });
            }
            _ => {
                println!("\r  {} {} — install failed          ", "\u{2717}".red(), name);
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

fn setup_ai_rules() -> Vec<SetupResult> {
    let mut results = Vec::new();
    let home = dirs::home_dir().unwrap_or_default();

    // Claude Code: install user-level rules
    if which::which("claude").is_ok() || home.join(".claude").exists() {
        let rules_dir = home.join(".claude/rules");
        let rules_file = rules_dir.join("nika.md");
        if rules_file.exists() {
            println!("  {} Claude Code + Nika rules", "\u{2713}".green());
            results.push(SetupResult {
                name: "Claude Code".into(),
                success: true,
                message: "rules present".into(),
            });
        } else {
            // Create rules
            std::fs::create_dir_all(&rules_dir).ok();
            let content = include_str!("../../nika-engine/src/init/ai_rules/claude_rules.md");
            if std::fs::write(&rules_file, content).is_ok() {
                println!("  {} Claude Code — Nika rules installed", "\u{2713}".green());
                results.push(SetupResult {
                    name: "Claude Code".into(),
                    success: true,
                    message: "installed".into(),
                });
            } else {
                results.push(SetupResult {
                    name: "Claude Code".into(),
                    success: false,
                    message: "could not write rules".into(),
                });
            }
        }
    }

    // Agent Skills: install to ~/.agents/skills/
    let skills_dir = home.join(".agents/skills");
    let has_skills = skills_dir.join("nika-workflow-syntax").exists();
    if !has_skills {
        // Try to install from embedded content
        let skill_dir = skills_dir.join("nika-workflow-syntax");
        std::fs::create_dir_all(&skill_dir).ok();
        let skill_content = "# Nika Workflow Syntax\n\nRefer to AGENTS.md in any Nika project for the complete workflow syntax reference.\n";
        if std::fs::write(skill_dir.join("SKILL.md"), skill_content).is_ok() {
            println!("  {} Agent Skills installed [~/.agents/skills/]", "\u{2713}".green());
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

fn setup_completions() -> SetupResult {
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
                    let home = dirs::home_dir().unwrap_or_default();
                    let zfunc = home.join(".zfunc");
                    std::fs::create_dir_all(&zfunc).ok();
                    Some(zfunc.join("_nika"))
                }
                "bash" => {
                    let home = dirs::home_dir().unwrap_or_default();
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

fn write_marker(results: &[SetupResult]) {
    let marker_path = machine_toml_path();
    let dir = marker_path.parent().unwrap();
    std::fs::create_dir_all(dir).ok();

    let editors: Vec<&str> = results
        .iter()
        .filter(|r| r.success && !["Agent Skills", "Claude Code", "Completions"].contains(&r.name.as_str()))
        .map(|r| r.name.as_str())
        .collect();
    let ai_tools: Vec<&str> = results
        .iter()
        .filter(|r| r.success && ["Claude Code", "Agent Skills"].contains(&r.name.as_str()))
        .map(|r| r.name.as_str())
        .collect();

    let content = format!(
        "[machine]\nsetup_at = \"{}\"\nversion = \"{}\"\neditors = {:?}\nai_tools = {:?}\n",
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        env!("CARGO_PKG_VERSION"),
        editors,
        ai_tools,
    );

    // Don't fail init if marker write fails
    std::fs::write(&marker_path, content).ok();
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
}
```

NOTE: The `chrono` dependency may not be available. If not, use a simple ISO timestamp:
```rust
// Instead of chrono, use:
let timestamp = std::process::Command::new("date")
    .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
    .output()
    .ok()
    .and_then(|o| String::from_utf8(o.stdout).ok())
    .unwrap_or_else(|| "unknown".to_string())
    .trim()
    .to_string();
```

Also check if `include_str!` for claude_rules.md exists. If not, inline a minimal rules string.

**Step 2: Register in lib.rs**

Add `pub mod machine;` to `tools/nika-cli/src/lib.rs`.

**Step 3: Run tests**

```bash
cargo check -p nika-cli
cargo test -p nika-cli --lib -- machine --nocapture
```

**Step 4: Commit**

```bash
git add tools/nika-cli/src/machine.rs tools/nika-cli/src/lib.rs
git commit -m "feat(cli): add machine auto-setup module with marker file

New module handles Phase 1 of unified init: detects editors/AI tools,
installs extensions/rules/completions automatically. Tracks state via
~/.nika/machine.toml. No questions asked — detect and install.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2: Wire machine setup into `nika init` as Phase 1

Make `nika init` call `machine::run_machine_setup()` before the project wizard, skipping if already done.

**Files:**
- Modify: `tools/nika-cli/src/init_wizard.rs` (remove setup_editors/setup_ai questions, add machine check)
- Modify: `tools/nika-cli/src/init.rs` (call machine setup before project generation)
- Modify: `tools/nika/src/main.rs` (update Init handler)

**Step 1: Update init_wizard.rs**

Remove the `setup_editors` and `setup_ai` fields from `WizardResult` (they're now handled by machine setup). Remove the two cliclack::confirm questions for editor/AI setup.

**Step 2: Update init.rs**

At the top of `init_project`, before the "Check if already initialized" block, add:

```rust
// Phase 1: Machine setup (silent, auto-detect + install)
if !crate::machine::is_machine_setup() {
    crate::machine::run_machine_setup();
    println!();
}
```

Remove the `setup_editors` and `setup_ai` parameters and the code that calls `handle_setup_command` at the bottom.

**Step 3: Update main.rs**

Remove the `false, false` arguments from the `init_project()` call.

**Step 4: Run tests + commit**

```bash
cargo check -p nika-cli -p nika
cargo test -p nika-cli --lib -- --nocapture
git add tools/nika-cli/src/init_wizard.rs tools/nika-cli/src/init.rs tools/nika/src/main.rs
git commit -m "feat(init): wire machine auto-setup as Phase 1 of nika init

nika init now runs machine setup automatically before project wizard.
Phase 1 (machine) skipped if ~/.nika/machine.toml exists and version matches.
Removed setup_editors/setup_ai questions — machine setup handles this silently.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 2: Doctor --fix flag

### Task 3: Add --fix flag to doctor

When `--fix` is passed, doctor runs the machine setup to fix warnings.

**Files:**
- Modify: `tools/nika-cli/src/doctor.rs` (add fix parameter + logic)
- Modify: `tools/nika/src/main.rs` (add --fix CLI arg to Doctor command)

**Step 1: Add --fix arg to Doctor command in main.rs**

Find the `Doctor` command variant and add:
```rust
/// Auto-fix issues (runs machine setup)
#[arg(long)]
fix: bool,
```

Update the call site:
```rust
Some(Commands::Doctor { full, format, fix }) => {
    cli::doctor::handle_doctor_command(full, &format, quiet, fix).await
}
```

**Step 2: Update handle_doctor_command signature**

Add `fix: bool` parameter. After running checks, if `fix` is true and there are warnings:

```rust
if fix {
    let has_issues = checks.iter().any(|c| c.status != DiagnosticStatus::Pass);
    if has_issues {
        println!();
        println!("  {}", "Auto-fixing...".bold());
        crate::machine::run_machine_setup();
        println!();
        println!("  {} Re-run {} to verify", "\u{2713}".green(), "nika doctor".bold());
    }
    return Ok(());
}
```

**Step 3: Run tests + commit**

```bash
cargo check -p nika-cli -p nika
git add tools/nika-cli/src/doctor.rs tools/nika/src/main.rs
git commit -m "feat(doctor): add --fix flag to auto-repair machine setup

nika doctor --fix runs machine auto-setup to fix missing extensions,
AI rules, and completions. Suggests re-running doctor to verify.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 3: Bare `nika` adaptive behavior

### Task 4: Make bare `nika` context-aware

Replace the `None =>` branch (currently prints help) with adaptive behavior.

**Files:**
- Modify: `tools/nika/src/main.rs` (the `None =>` match arm, ~line 691)

**Step 1: Replace the None match arm**

```rust
None => {
    // Adaptive behavior based on state
    if !cli::machine::is_machine_setup() {
        // First time ever: guide to init
        println!();
        println!(
            "  {} {}",
            "\u{1f98b}".magenta(), // butterfly
            format!("nika v{}", env!("CARGO_PKG_VERSION")).bold()
        );
        println!();
        println!("  Welcome! Run {} to get started.", "nika init".cyan().bold());
        println!("  This will set up your machine and create a project.");
        println!();
        Ok(())
    } else if !std::path::Path::new(".nika").exists() {
        // Machine setup done, but no project here
        println!();
        println!(
            "  {} No project in current directory.",
            "\u{25cb}".dimmed()
        );
        println!();
        println!("  {} {} start a new project", "nika init".cyan().bold(), "\u{2014}".dimmed());
        println!("  {} {} run a workflow file", "nika <file>".cyan().bold(), "\u{2014}".dimmed());
        println!("  {} {} check system health", "nika doctor".cyan().bold(), "\u{2014}".dimmed());
        println!("  {} {} all commands", "nika --help".cyan().bold(), "\u{2014}".dimmed());
        println!();
        Ok(())
    } else {
        // Project exists: show help (or TUI if enabled)
        use clap::CommandFactory;
        if let Err(e) = Cli::command().print_help() {
            eprintln!("Failed to print help: {e}");
            std::process::exit(1);
        }
        Ok(())
    }
}
```

**Step 2: Make machine module accessible from nika binary**

The nika binary uses `cli::machine::is_machine_setup()`. Since `nika-cli` exports `pub mod machine`, this should work via `use nika_cli as cli` or however the binary imports. Check the existing import pattern and match it.

**Step 3: Run tests + commit**

```bash
cargo check -p nika
git add tools/nika/src/main.rs
git commit -m "feat(cli): adaptive bare nika command based on context

Bare nika now shows:
- First time: welcome + guide to nika init
- No project: compact command reference (init/run/doctor/help)
- Has project: help (or TUI when compiled)

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Batch 4: Final verification + review

### Task 5: Full workspace test + clippy + code review

**Step 1: Run all tests**

```bash
cargo test -p nika-cli --lib
cargo check -p nika
cargo clippy -p nika-cli -- -D warnings
```

**Step 2: Fix any issues**

**Step 3: Commit if needed**

---

## Summary

| Task | Files | Change |
|------|-------|--------|
| T1 | NEW machine.rs, lib.rs | Machine auto-setup module + marker file |
| T2 | init_wizard.rs, init.rs, main.rs | Wire Phase 1 into nika init |
| T3 | doctor.rs, main.rs | --fix flag |
| T4 | main.rs | Adaptive bare nika |
| T5 | — | Verification |

**After:** `brew install nika && nika init` = everything works. One command.
