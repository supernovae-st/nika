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

/// Initialize a Nika project at a specific path (testable, no cwd dependency).
///
/// Creates `nika.toml` (project config), `.nika/` (runtime dir),
/// `hello.nika.yaml` (starter workflow), `AGENTS.md`, and `.gitignore`.
pub async fn init_project_at(
    root: &std::path::Path,
    permission: &str,
    _migrate_keys: bool,
) -> Result<(), NikaError> {
    // Check if already initialized
    if root.join("nika.toml").exists() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Already initialized at {}. Edit nika.toml to change settings.",
                root.display()
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

    let perm_str = permission_mode
        .display_name()
        .to_lowercase()
        .replace(" (yolo)", "");

    // Create nika.toml (project config — versioned, committed)
    let nika_toml_content = format!(
        r#"# Nika Project Configuration
# Docs: https://docs.supernovae.studio

[project]
name = "{name}"

[tools]
permission = "{perm}"

[provider]
default = "anthropic"
# model = "claude-sonnet-4-6"
"#,
        name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "my-project".to_string()),
        perm = perm_str,
    );
    fs::write(root.join("nika.toml"), nika_toml_content)?;

    // Create .nika/ directory (runtime — gitignored)
    fs::create_dir_all(root.join(".nika"))?;

    // Create starter workflow
    let starter_path = root.join("hello.nika.yaml");
    if !starter_path.exists() {
        fs::write(&starter_path, STARTER_WORKFLOW)?;
    }

    // Create AGENTS.md
    let agents_md_path = root.join("AGENTS.md");
    if !agents_md_path.exists() {
        fs::write(&agents_md_path, AGENTS_MD_CONTENT)?;
    }

    // Create or append .gitignore
    let gitignore_path = root.join(".gitignore");
    let gitignore_entries = ".nika/\nartifacts/\n";
    if gitignore_path.exists() {
        let existing = fs::read_to_string(&gitignore_path)?;
        if !existing.contains(".nika/") {
            let mut content = existing;
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(gitignore_entries);
            fs::write(&gitignore_path, content)?;
        }
    } else {
        fs::write(&gitignore_path, gitignore_entries)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ═══════════════════════════════════════════════════════════════════════
    // Phase 1: init_project_at() tests (TDD RED)
    // ═══════════════════════════════════════════════════════════════════════

    // Test 15: init creates nika.toml with [project] + [tools]
    #[tokio::test]
    async fn init_creates_nika_toml_at_project_root() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let toml_path = temp.path().join("nika.toml");
        assert!(toml_path.exists(), "nika.toml should be created");

        let content = std::fs::read_to_string(&toml_path).unwrap();
        assert!(content.contains("[project]"), "should have [project] section");
        assert!(content.contains("[tools]"), "should have [tools] section");
        assert!(
            content.contains("permission = \"plan\""),
            "should have permission"
        );
    }

    // Test 16: init creates .nika/ directory
    #[tokio::test]
    async fn init_creates_dot_nika_directory() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        assert!(
            temp.path().join(".nika").is_dir(),
            ".nika/ directory should exist"
        );
    }

    // Test 17: init creates hello.nika.yaml with schema
    #[tokio::test]
    async fn init_creates_hello_workflow() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let starter = temp.path().join("hello.nika.yaml");
        assert!(starter.exists(), "hello.nika.yaml should be created");

        let content = std::fs::read_to_string(&starter).unwrap();
        assert!(
            content.contains("nika/workflow@0.12"),
            "should have schema declaration"
        );
    }

    // Test 18: init creates AGENTS.md (non-empty)
    #[tokio::test]
    async fn init_creates_agents_md() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let agents = temp.path().join("AGENTS.md");
        assert!(agents.exists(), "AGENTS.md should be created");

        let content = std::fs::read_to_string(&agents).unwrap();
        assert!(!content.is_empty(), "AGENTS.md should not be empty");
    }

    // Test 19: init appends to existing .gitignore (preserves content)
    #[tokio::test]
    async fn init_appends_to_existing_gitignore() {
        let temp = tempdir().unwrap();
        let gitignore = temp.path().join(".gitignore");
        std::fs::write(&gitignore, "node_modules/\n*.log\n").unwrap();

        init_project_at(temp.path(), "plan", false).await.unwrap();

        let content = std::fs::read_to_string(&gitignore).unwrap();
        assert!(
            content.contains("node_modules/"),
            "existing entries preserved"
        );
        assert!(content.contains(".nika/"), ".nika/ should be appended");
        assert!(
            content.contains("artifacts/"),
            "artifacts/ should be appended"
        );
    }

    // Test 20: init creates .gitignore with .nika/ + artifacts/
    #[tokio::test]
    async fn init_creates_gitignore_with_defaults() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let gitignore = temp.path().join(".gitignore");
        assert!(gitignore.exists(), ".gitignore should be created");

        let content = std::fs::read_to_string(&gitignore).unwrap();
        assert!(content.contains(".nika/"), ".nika/ should be present");
        assert!(content.contains("artifacts/"), "artifacts/ should be present");
    }

    // Test 21: init fails if nika.toml already exists (idempotency)
    #[tokio::test]
    async fn init_fails_if_nika_toml_exists() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("nika.toml"),
            "[project]\nname = \"existing\"\n",
        )
        .unwrap();

        let result = init_project_at(temp.path(), "plan", false).await;
        assert!(result.is_err(), "should fail if nika.toml exists");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Already initialized"), "error should explain why");
    }

    // Test 22: init does NOT create .nika/config.toml (zero legacy)
    #[tokio::test]
    async fn init_does_not_create_legacy_config() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        assert!(
            !temp.path().join(".nika").join("config.toml").exists(),
            "should NOT create legacy .nika/config.toml"
        );
    }
}
