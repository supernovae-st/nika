//! Init subcommand handler — creates .nika/config.toml + AGENTS.md

use std::fs;

use colored::Colorize;

use nika_engine::error::NikaError;
use nika_engine::tools::PermissionMode;

/// Nika workflow syntax reference — embedded at compile time.
/// Used for project-level AGENTS.md so teams work without running `nika setup`.
const AGENTS_MD_CONTENT: &str = include_str!("../rules/claude.md");

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
default = "claude"
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

    println!();
    println!("  {} {}", "\u{2713}".green(), config_path.display());
    if created_agents_md {
        println!("  {} {}", "\u{2713}".green(), agents_md_path.display());
    }
    println!();
    println!("  Permission: {}", permission_mode.display_name().cyan());
    println!("  Provider:   {}", "claude (auto-detect)".cyan());
    println!();
    println!("  Edit {} to change settings.", ".nika/config.toml".bold());
    println!();

    // Migrate API keys if requested
    if migrate_keys {
        use nika_engine::secrets::migrate_env_to_keyring;
        println!(
            "{}",
            "Migrating API keys from environment variables...".cyan()
        );
        let report = migrate_env_to_keyring();
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
