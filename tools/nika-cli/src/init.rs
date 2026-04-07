//! Init subcommand handler — creates nika.toml + .nika/ + AGENTS.md + starter workflow

use std::fs;

use colored::Colorize;

use nika_engine::display::StatusIcon;
use nika_engine::error::NikaError;
use nika_engine::tools::PermissionMode;

use crate::rules;

/// Starter workflow created by `nika init` so the LSP activates immediately.
const STARTER_WORKFLOW: &str = r#"schema: "nika/workflow@0.12"
workflow: hello-nika
description: "Your first Nika workflow — edit me!"

tasks:
  - id: hello
    exec: "echo 'Hello from Nika! 🦋'"

  # Uncomment to try an LLM task (requires: nika keys set <name>)
  # - id: greet
  #   depends_on: [hello]
  #   provider: anthropic
  #   model: claude-sonnet-4-20250514
  #   infer: "Write a haiku about workflow automation"
"#;

/// Initialize a Nika project config in the current directory.
///
/// When `interactive` is true (TTY + no --yes), uses cliclack prompts.
/// When false, uses defaults from CLI flags directly.
///
/// Creates nika.toml, .nika/, hello.nika.yaml, AGENTS.md, and .gitignore.
/// Delegates to `init_project_at()` for the actual file creation.
pub async fn init_project(
    permission: &str,
    migrate_keys: bool,
    interactive: bool,
) -> Result<(), NikaError> {
    let cwd = std::env::current_dir()?;

    // Check if already initialized
    if cwd.join("nika.toml").exists() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Already initialized at {}. Edit nika.toml to change settings.",
                cwd.display()
            ),
        });
    }

    // Determine project name and permission — interactive or defaults
    let (project_name, final_permission) = if interactive {
        run_interactive_init(&cwd, permission)?
    } else {
        let dir_name = cwd
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "my-project".to_string());
        (dir_name, permission.to_string())
    };

    // Create project
    init_project_at_named(&cwd, &project_name, &final_permission, migrate_keys).await?;

    // Create CLAUDE.md symlink pointing to AGENTS.md
    let agents_md_path = cwd.join("AGENTS.md");
    let claude_md_path = cwd.join("CLAUDE.md");
    if agents_md_path.exists() && !claude_md_path.exists() {
        #[cfg(unix)]
        std::os::unix::fs::symlink("AGENTS.md", &claude_md_path).ok();
    }

    if interactive {
        cliclack::outro("Next: nika run hello.nika.yaml").ok();
    } else {
        // Non-interactive summary
        println!();
        println!("  {} nika.toml", StatusIcon::Ok);
        println!("  {} .nika/", StatusIcon::Ok);
        println!("  {} hello.nika.yaml", StatusIcon::Ok);
        println!("  {} AGENTS.md", StatusIcon::Ok);
        println!("  {} .gitignore", StatusIcon::Ok);
        println!();
        println!("  {}", "Next steps:".bold());
        println!(
            "    nika run hello.nika.yaml       {}",
            "# Run your first workflow".dimmed()
        );
        println!(
            "    nika keys set anthropic     {}",
            "# Configure an LLM provider".dimmed()
        );
        println!(
            "    nika showcase list              {}",
            "# Browse 115 example workflows".dimmed()
        );
        println!();
    }

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

/// Interactive init wizard using cliclack prompts.
///
/// Returns (project_name, permission_mode).
fn run_interactive_init(
    cwd: &std::path::Path,
    default_permission: &str,
) -> Result<(String, String), NikaError> {
    let dir_name = cwd
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "my-project".to_string());

    cliclack::intro("Initialize a new Nika project").map_err(|e| NikaError::ValidationError {
        reason: format!("Interactive init failed: {e}"),
    })?;

    let name: String = cliclack::input("Project name")
        .default_input(&dir_name)
        .interact()
        .map_err(|e| NikaError::ValidationError {
            reason: format!("Input cancelled: {e}"),
        })?;

    let permission: String = cliclack::select("Permission mode")
        .item(
            "plan".to_string(),
            "plan",
            "Show plan, ask before running (recommended)",
        )
        .item("deny".to_string(), "deny", "Block all exec/fetch (safest)")
        .item(
            "accept-edits".to_string(),
            "accept-edits",
            "Auto-approve file edits, ask for exec",
        )
        .item(
            "yolo".to_string(),
            "yolo",
            "Auto-approve everything (experts only)",
        )
        .initial_value(default_permission.to_string())
        .interact()
        .map_err(|e| NikaError::ValidationError {
            reason: format!("Selection cancelled: {e}"),
        })?;

    let spinner = cliclack::spinner();
    spinner.start("Creating project...");

    // Brief pause so the spinner feels intentional
    std::thread::sleep(std::time::Duration::from_millis(150));

    spinner.stop("Project created");

    Ok((name, permission))
}

/// Initialize a Nika project at a specific path (testable, no cwd dependency).
///
/// Uses the directory name as the project name.
/// Creates `nika.toml` (project config), `.nika/` (runtime dir),
/// `hello.nika.yaml` (starter workflow), `AGENTS.md`, and `.gitignore`.
pub async fn init_project_at(
    root: &std::path::Path,
    permission: &str,
    _migrate_keys: bool,
) -> Result<(), NikaError> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "my-project".to_string());
    init_project_at_named(root, &name, permission, _migrate_keys).await
}

/// Initialize a Nika project at a specific path with an explicit project name.
///
/// Creates `nika.toml` (project config), `.nika/` (runtime dir),
/// `hello.nika.yaml` (starter workflow), `AGENTS.md`, and `.gitignore`.
pub async fn init_project_at_named(
    root: &std::path::Path,
    project_name: &str,
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
        name = project_name,
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

    // Create .mcp.json with nika MCP server pre-configured
    let mcp_json_path = root.join(".mcp.json");
    if !mcp_json_path.exists() {
        let mcp_content = serde_json::json!({
            "mcpServers": {
                "nika": {
                    "command": "nika",
                    "args": ["mcp", "serve"]
                }
            }
        });
        fs::write(
            &mcp_json_path,
            serde_json::to_string_pretty(&mcp_content).unwrap() + "\n",
        )?;
    }

    // Create AGENTS.md (cross-tool, works with any AI assistant)
    let agents_md_path = root.join("AGENTS.md");
    if !agents_md_path.exists() {
        fs::write(&agents_md_path, rules::assemble_agents_md())?;
    }

    // GitHub Copilot: .github/copilot-instructions.md
    let copilot_dir = root.join(".github");
    let copilot_path = copilot_dir.join("copilot-instructions.md");
    if !copilot_path.exists() {
        fs::create_dir_all(&copilot_dir).ok();
        fs::write(&copilot_path, rules::assemble_copilot_instructions())?;
    }

    // Cursor: 3-file progressive discovery (.cursor/rules/)
    let cursor_dir = root.join(".cursor").join("rules");
    fs::create_dir_all(&cursor_dir).ok();
    // Remove old monolithic file if present
    let old_cursor = cursor_dir.join("nika.mdc");
    if old_cursor.exists() {
        fs::remove_file(&old_cursor).ok();
    }
    write_if_absent(
        &cursor_dir.join("nika-project.mdc"),
        &rules::assemble_cursor_project_mdc(),
    )?;
    write_if_absent(
        &cursor_dir.join("nika-syntax.mdc"),
        &rules::assemble_cursor_syntax_mdc(),
    )?;
    write_if_absent(
        &cursor_dir.join("nika-reference.mdc"),
        &rules::assemble_cursor_reference_mdc(),
    )?;

    // Cursor MCP config: .cursor/mcp.json
    let cursor_mcp = root.join(".cursor").join("mcp.json");
    if !cursor_mcp.exists() {
        let cursor_mcp_content = serde_json::json!({
            "mcpServers": {
                "nika": {
                    "command": "nika",
                    "args": ["mcp", "serve"]
                }
            }
        });
        fs::write(
            &cursor_mcp,
            serde_json::to_string_pretty(&cursor_mcp_content).unwrap() + "\n",
        )?;
    }

    // Windsurf: .windsurf/rules/nika.md (new location)
    let windsurf_dir = root.join(".windsurf").join("rules");
    fs::create_dir_all(&windsurf_dir).ok();
    write_if_absent(
        &windsurf_dir.join("nika.md"),
        &rules::assemble_windsurf_rules(),
    )?;
    // Remove old .windsurfrules if migrating
    let old_windsurf = root.join(".windsurfrules");
    if old_windsurf.exists() {
        fs::remove_file(&old_windsurf).ok();
    }

    // Gemini CLI: .gemini/GEMINI.md
    let gemini_dir = root.join(".gemini");
    fs::create_dir_all(&gemini_dir).ok();
    write_if_absent(
        &gemini_dir.join("GEMINI.md"),
        &rules::assemble_gemini_md(),
    )?;

    // Claude Code: .claude/settings.json with hooks + permissions
    let claude_dir = root.join(".claude");
    fs::create_dir_all(&claude_dir).ok();
    let claude_settings = claude_dir.join("settings.json");
    if !claude_settings.exists() {
        let settings = serde_json::json!({
            "permissions": {
                "allow": ["mcp__nika__*", "Bash(nika *)"]
            }
        });
        fs::write(
            &claude_settings,
            serde_json::to_string_pretty(&settings).unwrap() + "\n",
        )?;
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

/// Write a file only if it doesn't already exist.
fn write_if_absent(path: &std::path::Path, content: &str) -> Result<(), NikaError> {
    if !path.exists() {
        fs::write(path, content)?;
    }
    Ok(())
}

/// Handle `nika init --course` — generate interactive course files.
pub fn init_course() -> Result<(), NikaError> {
    use nika_init::course::generator::{generate_course, CourseConfig};

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
            Err(NikaError::ConfigError {
                reason: e.to_string(),
            })
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
        assert!(
            content.contains("[project]"),
            "should have [project] section"
        );
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
        assert!(
            content.contains("artifacts/"),
            "artifacts/ should be present"
        );
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
        assert!(
            err.contains("Already initialized"),
            "error should explain why"
        );
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

    // Test 23: init creates .mcp.json with nika MCP server pre-configured
    #[tokio::test]
    async fn init_creates_mcp_json_with_nika_server() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let mcp_json = temp.path().join(".mcp.json");
        assert!(mcp_json.exists(), ".mcp.json should be created");

        let content = std::fs::read_to_string(&mcp_json).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(
            parsed["mcpServers"]["nika"].is_object(),
            ".mcp.json should have nika server pre-configured"
        );
        assert_eq!(
            parsed["mcpServers"]["nika"]["command"], "nika",
            "nika server command should be 'nika'"
        );
    }

    // Test 24: init generates 3 Cursor files instead of 1
    #[tokio::test]
    async fn init_generates_cursor_multi_file() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let rules_dir = temp.path().join(".cursor/rules");
        assert!(
            rules_dir.join("nika-project.mdc").exists(),
            "nika-project.mdc should exist"
        );
        assert!(
            rules_dir.join("nika-syntax.mdc").exists(),
            "nika-syntax.mdc should exist"
        );
        assert!(
            rules_dir.join("nika-reference.mdc").exists(),
            "nika-reference.mdc should exist"
        );
        // Old monolithic file should NOT exist
        assert!(
            !rules_dir.join("nika.mdc").exists(),
            "old nika.mdc should not exist"
        );

        // Project file must be small and alwaysApply
        let project = std::fs::read_to_string(rules_dir.join("nika-project.mdc")).unwrap();
        assert!(project.contains("alwaysApply: true"));
        assert!(project.lines().count() < 25);
    }

    // Test 25: init generates .claude/settings.json with permissions
    #[tokio::test]
    async fn init_generates_claude_settings() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let settings_path = temp.path().join(".claude/settings.json");
        assert!(settings_path.exists(), ".claude/settings.json should exist");

        let settings: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&settings_path).unwrap()).unwrap();
        let allow = settings["permissions"]["allow"]
            .as_array()
            .expect("allow should be array");
        assert!(
            allow
                .iter()
                .any(|v| v.as_str().unwrap_or("").contains("nika")),
            "should have nika permission"
        );
    }

    // Test 26: init generates .cursor/mcp.json
    #[tokio::test]
    async fn init_generates_cursor_mcp_json() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let cursor_mcp = temp.path().join(".cursor/mcp.json");
        assert!(cursor_mcp.exists(), ".cursor/mcp.json should exist");

        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&cursor_mcp).unwrap()).unwrap();
        assert!(parsed["mcpServers"]["nika"].is_object());
    }

    // Test 27: init generates .gemini/GEMINI.md
    #[tokio::test]
    async fn init_generates_gemini_md() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let gemini = temp.path().join(".gemini/GEMINI.md");
        assert!(gemini.exists(), ".gemini/GEMINI.md should exist");

        let content = std::fs::read_to_string(&gemini).unwrap();
        assert!(content.contains("nika/workflow@0.12"));
    }

    // Test 28: AGENTS.md contains schema version
    #[tokio::test]
    async fn init_agents_md_has_schema_version() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let content =
            std::fs::read_to_string(temp.path().join("AGENTS.md")).unwrap();
        assert!(content.contains("nika/workflow@0.12"));
        assert!(content.contains("nika check"));
    }

    // Test 29: init generates .windsurf/rules/nika.md (new path)
    #[tokio::test]
    async fn init_generates_windsurf_rules() {
        let temp = tempdir().unwrap();
        init_project_at(temp.path(), "plan", false).await.unwrap();

        let windsurf = temp.path().join(".windsurf/rules/nika.md");
        assert!(windsurf.exists(), ".windsurf/rules/nika.md should exist");
        // Old .windsurfrules should not exist
        assert!(!temp.path().join(".windsurfrules").exists());
    }
}
