//! Init subcommand handler — creates .nika/config.toml + AGENTS.md + starter workflow

use std::fs;

use colored::Colorize;

use nika_engine::display::StatusIcon;
use nika_engine::error::NikaError;
use nika_engine::tools::PermissionMode;

/// Nika workflow syntax reference — embedded at compile time.
/// Used for project-level AGENTS.md so teams work without running `nika setup`.
const AGENTS_MD_CONTENT: &str = include_str!("../rules/claude.md");

/// Starter workflow created by `nika init` so the LSP activates immediately.
const STARTER_WORKFLOW: &str = r#"schema: "nika/workflow@0.12"
workflow: hello-nika
description: "Your first Nika workflow — edit me!"

tasks:
  - id: hello
    exec: "echo 'Hello from Nika! 🦋'"

  # Uncomment to try an LLM task (requires: nika provider set <name>)
  # - id: greet
  #   depends_on: [hello]
  #   provider: anthropic
  #   model: claude-sonnet-4-20250514
  #   infer: "Write a haiku about workflow automation"
"#;

/// Initialize a Nika project config in the current directory.
///
/// Creates `.nika/config.toml` with provider and permission settings.
/// Everything else (editors, AI rules, completions) is handled by auto-setup.
pub async fn init_project(permission: &str, migrate_keys: bool) -> Result<(), NikaError> {
    let cwd = std::env::current_dir()?;
    let nika_dir = cwd.join(".nika");

    // Check if already initialized
    if nika_dir.join("config.toml").exists() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Already initialized at {}. Edit .nika/config.toml to change settings.",
                nika_dir.display()
            ),
        });
    }

    // Parse permission mode
    let permission_mode = match permission.to_lowercase().as_str() {
        "deny" => PermissionMode::Deny,
        "plan" => PermissionMode::Plan,
        "accept-edits" | "acceptedits" => PermissionMode::AcceptEdits,
        "accept-all" | "acceptall" | "yolo" => PermissionMode::YoloMode,
        other => {
            return Err(NikaError::ValidationError {
                reason: format!(
                    "Invalid permission mode: '{other}'. Use: deny, plan, accept-edits, yolo"
                ),
            });
        }
    };

    // Create .nika/config.toml
    fs::create_dir_all(&nika_dir)?;

    let config_path = nika_dir.join("config.toml");
    let config_content = format!(
        r#"# Nika Project Configuration

[tools]
permission = "{}"

[provider]
default = "anthropic"
# model = "claude-sonnet-4-6"
"#,
        permission_mode
            .display_name()
            .to_lowercase()
            .replace(" (yolo)", "")
    );
    fs::write(&config_path, config_content)?;

    // Create AGENTS.md with embedded Nika workflow syntax reference.
    // This enables project-level AI context without requiring `nika setup`.
    let agents_md_path = cwd.join("AGENTS.md");
    let created_agents_md = if agents_md_path.exists() {
        false
    } else {
        fs::write(&agents_md_path, AGENTS_MD_CONTENT)?;
        // Create CLAUDE.md symlink pointing to AGENTS.md (20+ tools support both names)
        let claude_md_path = cwd.join("CLAUDE.md");
        if !claude_md_path.exists() {
            #[cfg(unix)]
            std::os::unix::fs::symlink("AGENTS.md", &claude_md_path).ok();
        }
        true
    };

    // Create starter workflow so the LSP activates immediately in editors
    let starter_path = cwd.join("hello.nika.yaml");
    let created_starter = if starter_path.exists() {
        false
    } else {
        fs::write(&starter_path, STARTER_WORKFLOW)?;
        true
    };

    println!();
    println!("  {} {}", StatusIcon::Ok, config_path.display());
    if created_agents_md {
        println!("  {} {}", StatusIcon::Ok, agents_md_path.display());
    }
    if created_starter {
        println!("  {} {}", StatusIcon::Ok, starter_path.display());
    }
    println!();
    println!("  Permission: {}", permission_mode.display_name().cyan());
    println!("  Provider:   {}", "claude (auto-detect)".cyan());
    println!();
    println!("  {}", "Next steps:".bold());
    println!(
        "    nika run hello.nika.yaml       {}",
        "# Run your first workflow".dimmed()
    );
    println!(
        "    nika provider set anthropic     {}",
        "# Configure an LLM provider".dimmed()
    );
    println!(
        "    nika showcase list              {}",
        "# Browse 115 example workflows".dimmed()
    );
    println!();

    // Migrate API keys if requested
    if migrate_keys {
        use nika_engine::secrets::migrate_env_to_vault;
        println!(
            "{}",
            "Migrating API keys from environment variables...".cyan()
        );
        let report = migrate_env_to_vault();
        println!("{}", report.summary());

        if report.migrated > 0 {
            println!(
                "{}",
                "You can now remove these env vars from your shell config.".yellow()
            );
        }
        println!();
    }

    Ok(())
}

/// Handle `nika init --course` — generate interactive course files.
pub fn init_course() -> Result<(), NikaError> {
    use nika_engine::init::course::generator::{generate_course, CourseConfig};

    let config = CourseConfig {
        dest: std::path::PathBuf::from("nika-course"),
        ..CourseConfig::default()
    };

    match generate_course(&config) {
        Ok(result) => {
            println!(
                "\n  {} Course generated! {} levels, {} exercises\n  Provider: {} (auto-detected)\n  Location: {}\n  Run: cd {} && nika course status\n",
                StatusIcon::Ok,
                result.levels,
                result.exercises,
                result.provider,
                result.root.display(),
                result.root.display(),
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("{} Course generation failed: {e}", "Error:".red().bold());
            Err(e.into())
        }
    }
}
