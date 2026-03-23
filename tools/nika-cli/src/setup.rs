//! `nika setup` — Machine-level IDE and AI tool integration
//!
//! Detects installed editors and AI coding tools, installs extensions,
//! configures Agent Skills, shell completions, and git hooks.

use clap::Subcommand;
use colored::Colorize;
use nika_engine::NikaError;
use std::path::{Path, PathBuf};
use std::process::Command;

// ─── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum SetupAction {
    /// Detect and display all installed editors and AI tools
    Check,
    /// Install Nika extensions in detected editors
    Editors,
    /// Install Agent Skills for detected AI coding tools
    Ai,
    /// Install shell completions for current shell
    Completions,
    /// Install git co-author hook in current repository
    Git,
}

pub async fn handle_setup_command(action: Option<SetupAction>) -> Result<(), NikaError> {
    match action {
        None => run_interactive_setup().await,
        Some(SetupAction::Check) => run_check(),
        Some(SetupAction::Editors) => setup_editors(),
        Some(SetupAction::Ai) => setup_ai_skills(),
        Some(SetupAction::Completions) => setup_completions(),
        Some(SetupAction::Git) => setup_git_hooks(),
    }
}

// ─── Editor Detection ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DetectedEditor {
    pub name: &'static str,
    pub binary: &'static str,
    pub path: Option<PathBuf>,
    pub has_extension: bool,
    pub extension_id: &'static str,
}

#[derive(Debug, Clone)]
pub struct DetectedAiTool {
    pub name: &'static str,
    pub detected: bool,
    pub has_rules: bool,
    pub config_path: Option<PathBuf>,
}

const EDITORS: &[(&str, &str, &str)] = &[
    ("VS Code", "code", "supernovae-studio.nika-lang"),
    ("Cursor", "cursor", "supernovae-studio.nika-lang"),
    ("Windsurf", "windsurf", "supernovae-studio.nika-lang"),
    ("Zed", "zed", ""),
    ("Neovim", "nvim", ""),
    ("Sublime Text", "subl", ""),
];

fn detect_editors() -> Vec<DetectedEditor> {
    EDITORS
        .iter()
        .filter_map(|(name, binary, ext_id)| {
            let path = which_binary(binary);
            if path.is_none() && !check_macos_app(name) {
                return None;
            }

            let has_extension = if !ext_id.is_empty() {
                check_extension_installed(binary, ext_id)
            } else {
                false
            };

            Some(DetectedEditor {
                name,
                binary,
                path,
                has_extension,
                extension_id: ext_id,
            })
        })
        .collect()
}

fn detect_ai_tools() -> Vec<DetectedAiTool> {
    let home = dirs::home_dir().unwrap_or_default();
    vec![
        DetectedAiTool {
            name: "Claude Code",
            detected: which_binary("claude").is_some(),
            has_rules: home.join(".claude").exists(),
            config_path: Some(home.join(".claude")),
        },
        DetectedAiTool {
            name: "Cursor AI",
            detected: which_binary("cursor").is_some(),
            has_rules: Path::new(".cursor/rules").exists(),
            config_path: Some(PathBuf::from(".cursor")),
        },
        DetectedAiTool {
            name: "GitHub Copilot",
            detected: check_extension_installed("code", "github.copilot"),
            has_rules: Path::new(".github/copilot").exists(),
            config_path: Some(PathBuf::from(".github/copilot")),
        },
        DetectedAiTool {
            name: "Roo Code",
            detected: check_extension_installed("code", "rooveterinaryinc.roo-cline"),
            has_rules: Path::new(".roo/rules").exists(),
            config_path: Some(PathBuf::from(".roo")),
        },
        DetectedAiTool {
            name: "Windsurf AI",
            detected: which_binary("windsurf").is_some(),
            has_rules: Path::new(".windsurf/rules").exists(),
            config_path: Some(PathBuf::from(".windsurf")),
        },
        DetectedAiTool {
            name: "Aider",
            detected: which_binary("aider").is_some(),
            has_rules: Path::new("CONVENTIONS.md").exists(),
            config_path: None,
        },
    ]
}

fn which_binary(name: &str) -> Option<PathBuf> {
    which::which(name).ok()
}

#[cfg(target_os = "macos")]
fn check_macos_app(name: &str) -> bool {
    let app_names: &[&str] = match name {
        "VS Code" => &["Visual Studio Code"],
        "Cursor" => &["Cursor"],
        "Windsurf" => &["Windsurf"],
        "Zed" => &["Zed"],
        "Sublime Text" => &["Sublime Text"],
        _ => return false,
    };

    app_names.iter().any(|app| {
        Path::new(&format!("/Applications/{}.app", app)).exists()
            || dirs::home_dir()
                .map(|h| h.join(format!("Applications/{}.app", app)).exists())
                .unwrap_or(false)
    })
}

#[cfg(not(target_os = "macos"))]
fn check_macos_app(_name: &str) -> bool {
    false
}

fn check_extension_installed(editor_binary: &str, extension_id: &str) -> bool {
    Command::new(editor_binary)
        .args(["--list-extensions"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|list| list.lines().any(|l| l.eq_ignore_ascii_case(extension_id)))
        .unwrap_or(false)
}

// ─── Setup Actions ────────────────────────────────────────────────────────────

fn run_check() -> Result<(), NikaError> {
    println!("\n{}", "  Nika Setup — System Check".bold());
    println!("{}", "  ─────────────────────────────────────".dimmed());

    // Editors
    println!("\n  {}", "Editors".bold().underline());
    let editors = detect_editors();
    if editors.is_empty() {
        println!("  {} No supported editors detected", "○".dimmed());
    }
    for ed in &editors {
        let ext_status = if ed.extension_id.is_empty() {
            "(no extension needed)".dimmed().to_string()
        } else if ed.has_extension {
            "nika-lang installed".green().to_string()
        } else {
            "nika-lang missing".yellow().to_string()
        };
        println!("  {} {} — {}", "✓".green(), ed.name, ext_status);
    }

    // AI Tools
    println!("\n  {}", "AI Coding Tools".bold().underline());
    let tools = detect_ai_tools();
    for tool in &tools {
        if tool.detected {
            let rules = if tool.has_rules {
                "rules present".green().to_string()
            } else {
                "no rules".yellow().to_string()
            };
            println!("  {} {} — {}", "✓".green(), tool.name, rules);
        }
    }
    if tools.iter().all(|t| !t.detected) {
        println!("  {} No AI coding tools detected", "○".dimmed());
    }

    // Agent Skills
    println!("\n  {}", "Agent Skills".bold().underline());
    let skills_dir = dirs::home_dir().unwrap_or_default().join(".agents/skills");
    let has_skills = skills_dir.join("nika-workflow-syntax").exists();
    if has_skills {
        println!(
            "  {} Nika skills installed at {}",
            "✓".green(),
            skills_dir.display()
        );
    } else {
        println!("  {} No Nika skills found", "⚠".yellow());
        println!("    → Run: {} to install", "nika setup ai".bold());
    }

    // Git hooks
    println!("\n  {}", "Git Integration".bold().underline());
    let hook_path = Path::new(".git/hooks/prepare-commit-msg");
    if hook_path.exists() {
        let content = std::fs::read_to_string(hook_path).unwrap_or_default();
        if content.contains("Nika") {
            println!("  {} Co-author hook installed", "✓".green());
        } else {
            println!("  {} Hook exists but not Nika's", "⚠".yellow());
        }
    } else if Path::new(".git").exists() {
        println!("  {} No co-author hook", "⚠".yellow());
        println!("    → Run: {} to install", "nika setup git".bold());
    }

    // Shell completions
    println!("\n  {}", "Shell".bold().underline());
    let shell = std::env::var("SHELL").unwrap_or_default();
    let shell_name = if shell.contains("zsh") {
        "zsh"
    } else if shell.contains("bash") {
        "bash"
    } else if shell.contains("fish") {
        "fish"
    } else {
        "unknown"
    };
    println!("  {} Shell: {}", "✓".green(), shell_name);

    // AGENTS.md
    println!("\n  {}", "Universal".bold().underline());
    if Path::new("AGENTS.md").exists() {
        println!("  {} AGENTS.md present", "✓".green());
    } else if Path::new("CLAUDE.md").exists() {
        println!(
            "  {} Only CLAUDE.md found (consider migrating to AGENTS.md)",
            "⚠".yellow()
        );
    } else {
        println!("  {} No AGENTS.md or CLAUDE.md", "○".dimmed());
    }

    println!();
    Ok(())
}

fn setup_editors() -> Result<(), NikaError> {
    println!("\n{}", "  Nika Setup — Editors".bold());
    println!("{}", "  ─────────────────────────────────────".dimmed());

    let editors = detect_editors();
    let mut installed = 0;

    for ed in &editors {
        if ed.extension_id.is_empty() {
            println!("  {} {} — manual LSP setup required", "○".dimmed(), ed.name);
            if ed.binary == "nvim" {
                println!("    Create ~/.config/nvim/lsp/nika.lua with:");
                println!("    return {{ cmd = {{ 'nika', 'lsp' }}, filetypes = {{ 'yaml' }} }}");
            }
            continue;
        }

        if ed.has_extension {
            println!("  {} {} — already installed", "✓".green(), ed.name);
            continue;
        }

        print!("  {} {} — installing...", "◇".cyan(), ed.name);
        let result = Command::new(ed.binary)
            .args(["--install-extension", ed.extension_id])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                println!("\r  {} {} — installed ✓       ", "✓".green(), ed.name);
                installed += 1;
            }
            _ => {
                println!("\r  {} {} — install failed    ", "✗".red(), ed.name);
                println!(
                    "    → Try manually: {} --install-extension {}",
                    ed.binary, ed.extension_id
                );
            }
        }
    }

    println!("\n  {} {} extension(s) installed\n", "✓".green(), installed);
    Ok(())
}

fn setup_ai_skills() -> Result<(), NikaError> {
    println!("\n{}", "  Nika Setup — AI Skills".bold());
    println!("{}", "  ─────────────────────────────────────".dimmed());

    let home = dirs::home_dir().unwrap_or_default();
    let skills_dir = home.join(".agents/skills");

    // Copy skills from embedded content
    let skills = [
        "nika-workflow-syntax",
        "nika-create",
        "nika-validate",
        "nika-run",
    ];

    for skill_name in &skills {
        let target = skills_dir.join(skill_name);
        if target.exists() {
            println!("  {} {} — already installed", "✓".green(), skill_name);
            continue;
        }

        // Check if we have the skill in the repo
        let source = PathBuf::from(format!("skills/{}", skill_name));
        if source.exists() {
            std::fs::create_dir_all(&target).map_err(NikaError::IoError)?;

            let src_skill = source.join("SKILL.md");
            let dst_skill = target.join("SKILL.md");
            if src_skill.exists() {
                std::fs::copy(&src_skill, &dst_skill).map_err(NikaError::IoError)?;
                println!("  {} {} — installed", "✓".green(), skill_name);
            }
        } else {
            println!(
                "  {} {} — source not found (run from nika repo root)",
                "⚠".yellow(),
                skill_name
            );
        }
    }

    println!("\n  Skills installed to: {}\n", skills_dir.display());
    Ok(())
}

fn setup_completions() -> Result<(), NikaError> {
    println!("\n{}", "  Nika Setup — Shell Completions".bold());
    println!("{}", "  ─────────────────────────────────────".dimmed());

    let shell = std::env::var("SHELL").unwrap_or_default();

    if shell.contains("zsh") {
        // Try homebrew path first
        let brew_path = Command::new("brew")
            .args(["--prefix"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|p| format!("{}/share/zsh/site-functions/_nika", p.trim()));

        let target = brew_path.unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_default();
            let zfunc = home.join(".zfunc");
            std::fs::create_dir_all(&zfunc).ok();
            zfunc.join("_nika").to_string_lossy().to_string()
        });

        let output = Command::new("nika").args(["completion", "zsh"]).output();

        match output {
            Ok(o) if o.status.success() => {
                std::fs::write(&target, o.stdout).map_err(NikaError::IoError)?;
                println!("  {} Zsh completions installed to {}", "✓".green(), target);
            }
            _ => {
                println!("  {} Failed to generate completions", "✗".red());
            }
        }
    } else if shell.contains("bash") {
        let home = dirs::home_dir().unwrap_or_default();
        let target = home.join(".local/share/bash-completion/completions/nika");
        std::fs::create_dir_all(target.parent().unwrap()).ok();

        let output = Command::new("nika").args(["completion", "bash"]).output();

        match output {
            Ok(o) if o.status.success() => {
                std::fs::write(&target, o.stdout).map_err(NikaError::IoError)?;
                println!(
                    "  {} Bash completions installed to {}",
                    "✓".green(),
                    target.display()
                );
            }
            _ => {
                println!("  {} Failed to generate completions", "✗".red());
            }
        }
    } else if shell.contains("fish") {
        let target = dirs::config_dir()
            .unwrap_or_default()
            .join("fish/completions/nika.fish");
        std::fs::create_dir_all(target.parent().unwrap()).ok();

        let output = Command::new("nika").args(["completion", "fish"]).output();

        match output {
            Ok(o) if o.status.success() => {
                std::fs::write(&target, o.stdout).map_err(NikaError::IoError)?;
                println!(
                    "  {} Fish completions installed to {}",
                    "✓".green(),
                    target.display()
                );
            }
            _ => {
                println!("  {} Failed to generate completions", "✗".red());
            }
        }
    } else {
        println!("  {} Unknown shell: {}", "⚠".yellow(), shell);
    }

    println!();
    Ok(())
}

fn setup_git_hooks() -> Result<(), NikaError> {
    println!("\n{}", "  Nika Setup — Git Hooks".bold());
    println!("{}", "  ─────────────────────────────────────".dimmed());

    // Check if we're in a git repo
    let git_dir = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output();

    let git_dir = match git_dir {
        Ok(o) if o.status.success() => String::from_utf8(o.stdout)
            .unwrap_or_default()
            .trim()
            .to_string(),
        _ => {
            println!("  {} Not a git repository", "⚠".yellow());
            return Ok(());
        }
    };

    let hooks_dir = Path::new(&git_dir).join("hooks");
    std::fs::create_dir_all(&hooks_dir).ok();

    let hook_path = hooks_dir.join("prepare-commit-msg");

    // Check if hook already exists
    if hook_path.exists() {
        let content = std::fs::read_to_string(&hook_path).unwrap_or_default();
        if content.contains("Nika co-author") {
            println!("  {} Co-author hook already installed", "✓".green());
            println!();
            return Ok(());
        }
        // Backup existing hook
        let backup = hooks_dir.join("prepare-commit-msg.backup");
        std::fs::copy(&hook_path, &backup).ok();
        println!("  {} Backed up existing hook", "○".dimmed());
    }

    let hook_content = r#"#!/bin/sh
# Nika co-author hook — adds co-author lines to commits touching .nika.yaml files
# Installed by: nika setup git

COMMIT_MSG_FILE=$1
COMMIT_SOURCE=$2

# Only modify regular commits (not merges, squashes, amends)
case "$COMMIT_SOURCE" in
    merge|squash)
        exit 0
        ;;
esac

# Check if co-author lines already exist
if grep -q "Co-Authored-By:" "$COMMIT_MSG_FILE" 2>/dev/null; then
    exit 0
fi

# Check if any .nika.yaml files are staged
if git diff --cached --name-only | grep -q '\.nika\.yaml$'; then
    printf '\n\nCo-Authored-By: Nika 🦋 <nika@supernovae.studio>\n' >> "$COMMIT_MSG_FILE"
fi
"#;

    std::fs::write(&hook_path, hook_content).map_err(NikaError::IoError)?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755)).ok();
    }

    println!("  {} Co-author hook installed", "✓".green());
    println!("    Commits touching .nika.yaml files will include:");
    println!("    Co-Authored-By: Nika 🦋 <nika@supernovae.studio>");
    println!();
    Ok(())
}

// ─── Interactive Wizard ───────────────────────────────────────────────────────

async fn run_interactive_setup() -> Result<(), NikaError> {
    println!("\n{}", "  🦋 Nika Setup".bold().magenta());
    println!("{}", "  ─────────────────────────────────────".dimmed());
    println!("  Configure your machine for Nika development.\n");

    // Step 1: Check
    run_check()?;

    // Step 2: Offer to fix
    let editors = detect_editors();
    let missing_ext: Vec<_> = editors
        .iter()
        .filter(|e| !e.extension_id.is_empty() && !e.has_extension)
        .collect();

    if !missing_ext.is_empty() {
        println!(
            "  {} editor(s) missing nika-lang extension.",
            missing_ext.len()
        );
        println!("  Run {} to install.\n", "nika setup editors".bold());
    }

    // Step 3: Suggest next steps
    println!("  {}", "Next Steps".bold().underline());
    println!(
        "  ├── {} — install editor extensions",
        "nika setup editors".bold()
    );
    println!(
        "  ├── {} — install AI skills globally",
        "nika setup ai".bold()
    );
    println!(
        "  ├── {} — install shell completions",
        "nika setup completions".bold()
    );
    println!(
        "  ├── {} — install git co-author hook",
        "nika setup git".bold()
    );
    println!("  └── {} — verify everything works", "nika doctor".bold());
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_editors_returns_valid_structs() {
        let editors = detect_editors();
        for ed in &editors {
            assert!(!ed.name.is_empty());
            assert!(!ed.binary.is_empty());
        }
    }

    #[test]
    fn test_detect_ai_tools_returns_six() {
        let tools = detect_ai_tools();
        assert_eq!(tools.len(), 6);
    }

    #[test]
    fn test_which_binary_finds_git() {
        // git should always be available
        assert!(which_binary("git").is_some());
    }

    #[test]
    fn test_which_binary_missing() {
        assert!(which_binary("nonexistent_binary_xyz_123").is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_check_macos_app_known() {
        // This test just verifies the function doesn't panic
        let _ = check_macos_app("VS Code");
        let _ = check_macos_app("Unknown Editor");
    }
}
