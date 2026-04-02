//! Configuration management subcommand handler

use clap::Subcommand;
use std::fs;
use std::path::{Path, PathBuf};

use nika_engine::display::{section_header, separator, StatusIcon};
use nika_engine::error::NikaError;

// ═══════════════════════════════════════════════════════════════════════════
// Project root discovery
// ═══════════════════════════════════════════════════════════════════════════

/// Result of project root discovery.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectRoot {
    pub root: PathBuf,
    pub source: ProjectRootSource,
}

/// How the project root was determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectRootSource {
    /// Found nika.toml (new standard)
    NikaToml,
    /// Found .nika/ directory (legacy fallback)
    DotNika,
    /// Nothing found, using start directory
    Fallback,
}

/// Walk up from `start` to find project root.
///
/// Priority: nika.toml (primary) > .nika/ (legacy fallback) > start_dir
pub fn find_project_root_from(start: &Path) -> Result<ProjectRoot, NikaError> {
    // Pass 1: walk up looking for nika.toml
    let mut dir = start;
    loop {
        if dir.join("nika.toml").exists() {
            return Ok(ProjectRoot {
                root: dir.to_path_buf(),
                source: ProjectRootSource::NikaToml,
            });
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }

    // Pass 2: walk up looking for .nika/
    let mut dir = start;
    loop {
        let nika_dir = dir.join(".nika");
        if nika_dir.exists() && nika_dir.is_dir() {
            return Ok(ProjectRoot {
                root: dir.to_path_buf(),
                source: ProjectRootSource::DotNika,
            });
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }

    // Nothing found
    Ok(ProjectRoot {
        root: start.to_path_buf(),
        source: ProjectRootSource::Fallback,
    })
}

/// Load `BootstrapConfig` from `nika.toml` at the given project root.
///
/// Returns `None` if the file doesn't exist or can't be parsed.
pub fn load_project_config(root: &Path) -> Option<nika_engine::runtime::BootstrapConfig> {
    let toml_path = root.join("nika.toml");
    let content = fs::read_to_string(toml_path).ok()?;
    toml::from_str(&content).ok()
}

/// Configuration management actions
#[derive(Subcommand)]
pub enum ConfigAction {
    /// List all configuration values
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Get a specific config value
    Get {
        /// Config key (dot-separated, e.g., editor.theme)
        key: String,
    },

    /// Set a config value
    Set {
        /// Config key (dot-separated, e.g., editor.theme)
        key: String,
        /// Value to set
        value: String,
    },

    /// Open config file in $EDITOR
    Edit,

    /// Show config file path
    Path,

    /// Reset config to defaults
    Reset {
        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

pub fn handle_config_command(action: ConfigAction, quiet: bool) -> Result<(), NikaError> {
    let current = std::env::current_dir()?;
    let project = find_project_root_from(&current)?;

    // Determine config path based on discovery
    let config_path = match project.source {
        ProjectRootSource::NikaToml => project.root.join("nika.toml"),
        _ => project.root.join(".nika").join("config.toml"),
    };

    match action {
        ConfigAction::Path => {
            println!("{}", config_path.display());
            Ok(())
        }

        ConfigAction::List { json } => {
            if !config_path.exists() {
                if json {
                    println!("{{}}");
                } else {
                    println!(
                        "{} No config file found at {}",
                        StatusIcon::Info,
                        config_path.display()
                    );
                    println!("  Run 'nika init' to create one.");
                }
                return Ok(());
            }

            let content = fs::read_to_string(&config_path)?;

            if json {
                // Parse TOML and convert to JSON
                let value: toml::Value =
                    toml::from_str(&content).map_err(|e| NikaError::ValidationError {
                        reason: format!("Invalid TOML: {e}"),
                    })?;
                let json = serde_json::to_string_pretty(&value).map_err(|e| {
                    NikaError::ValidationError {
                        reason: format!("JSON conversion failed: {e}"),
                    }
                })?;
                println!("{json}");
            } else {
                use colored::Colorize;
                use nika_engine::runtime::boot::BootstrapConfig;

                let source_label = match project.source {
                    ProjectRootSource::NikaToml => "nika.toml",
                    ProjectRootSource::DotNika => ".nika/config.toml (legacy)",
                    ProjectRootSource::Fallback => "defaults",
                };
                println!(
                    "{}  {}",
                    section_header("Nika Configuration"),
                    config_path.display().to_string().dimmed()
                );
                println!("{}", separator(50));

                // Parse into structured config for display
                match toml::from_str::<BootstrapConfig>(&content) {
                    Ok(config) => {
                        // [project]
                        if let Some(ref proj) = config.project {
                            println!();
                            println!("  {}", "[project]".bold());
                            println!("    name          {}", proj.name.cyan());
                            if let Some(ref desc) = proj.description {
                                println!("    description   {}", desc);
                            }
                        }

                        // [provider]
                        println!();
                        println!("  {}", "[provider]".bold());
                        println!("    default       {}", config.provider.default.cyan());
                        if let Some(ref m) = config.provider.model {
                            println!("    model         {}", m.cyan());
                        }
                        // Show API key status for LLM providers
                        {
                            use nika_engine::core::{ProviderCategory, KNOWN_PROVIDERS};
                            let mut keys_line = String::from("    keys          ");
                            for p in KNOWN_PROVIDERS
                                .iter()
                                .filter(|p| p.category == ProviderCategory::Llm)
                            {
                                let has_key = std::env::var(p.env_var)
                                    .map(|v| !v.is_empty())
                                    .unwrap_or(false);
                                if has_key {
                                    keys_line.push_str(&format!("{} {}  ", StatusIcon::Ok, p.id));
                                }
                            }
                            let configured: usize = KNOWN_PROVIDERS
                                .iter()
                                .filter(|p| p.category == ProviderCategory::Llm)
                                .filter(|p| {
                                    std::env::var(p.env_var)
                                        .map(|v| !v.is_empty())
                                        .unwrap_or(false)
                                })
                                .count();
                            if configured > 0 {
                                println!("{}", keys_line.trim_end());
                            } else {
                                println!("    keys          {} none configured", StatusIcon::Warn);
                            }
                        }

                        // [tools]
                        println!();
                        println!("  {}", "[tools]".bold());
                        println!("    permission    {}", config.tools.permission);
                        if let Some(ref wd) = config.tools.working_dir {
                            println!("    working_dir   {}", wd);
                        }

                        // [policy] (only if non-default)
                        if !config.policy.allow_exec
                            || !config.policy.allow_network
                            || config.policy.max_token_spend.is_some()
                        {
                            println!();
                            println!("  {}", "[policy]".bold());
                            println!(
                                "    allow_exec    {}",
                                if config.policy.allow_exec {
                                    format!("{}", StatusIcon::Ok)
                                } else {
                                    format!("{}", StatusIcon::Fail)
                                }
                            );
                            println!(
                                "    allow_network {}",
                                if config.policy.allow_network {
                                    format!("{}", StatusIcon::Ok)
                                } else {
                                    format!("{}", StatusIcon::Fail)
                                }
                            );
                            if let Some(max) = config.policy.max_token_spend {
                                println!("    max_tokens    {}", max);
                            }
                        }
                    }
                    Err(_) => {
                        // Fallback: raw TOML
                        println!();
                        println!("{content}");
                    }
                }
                println!();
                println!("  Source: {}", source_label.dimmed());
                println!();
            }
            Ok(())
        }

        ConfigAction::Get { key } => {
            if !config_path.exists() {
                return Err(NikaError::ValidationError {
                    reason: "No config file found. Run 'nika init' first.".to_string(),
                });
            }

            let content = fs::read_to_string(&config_path)?;
            let value: toml::Value =
                toml::from_str(&content).map_err(|e| NikaError::ValidationError {
                    reason: format!("Invalid TOML: {e}"),
                })?;

            // Navigate to the key (dot-separated path)
            let mut current = &value;
            for part in key.split('.') {
                current = current
                    .get(part)
                    .ok_or_else(|| NikaError::ValidationError {
                        reason: format!("Key '{key}' not found"),
                    })?;
            }

            // Print the value
            match current {
                toml::Value::String(s) => println!("{s}"),
                toml::Value::Integer(i) => println!("{i}"),
                toml::Value::Float(f) => println!("{f}"),
                toml::Value::Boolean(b) => println!("{b}"),
                _ => println!("{current}"),
            }
            Ok(())
        }

        ConfigAction::Set { key, value } => {
            if !config_path.exists() {
                return Err(NikaError::ValidationError {
                    reason: "No config file found. Run 'nika init' first.".to_string(),
                });
            }

            let content = fs::read_to_string(&config_path)?;
            let mut doc =
                content
                    .parse::<toml::Table>()
                    .map_err(|e| NikaError::ValidationError {
                        reason: format!("Invalid TOML: {e}"),
                    })?;

            // Navigate and set the value
            let parts: Vec<&str> = key.split('.').collect();
            if parts.is_empty() {
                return Err(NikaError::ValidationError {
                    reason: "Empty key".to_string(),
                });
            }

            // Build nested structure
            let mut current = &mut doc;
            for (i, part) in parts.iter().enumerate() {
                if i == parts.len() - 1 {
                    // Last part - set the value
                    let toml_value = parse_config_value(&value);
                    current.insert((*part).to_string(), toml_value);
                } else {
                    // Navigate or create table
                    if !current.contains_key(*part) {
                        current.insert((*part).to_string(), toml::Value::Table(toml::Table::new()));
                    }
                    current = current
                        .get_mut(*part)
                        .ok_or_else(|| NikaError::ValidationError {
                            reason: format!("Config key '{part}' not found"),
                        })?
                        .as_table_mut()
                        .ok_or_else(|| NikaError::ValidationError {
                            reason: format!("'{part}' is not a table"),
                        })?;
                }
            }

            // Write back
            let new_content =
                toml::to_string_pretty(&doc).map_err(|e| NikaError::ValidationError {
                    reason: format!("TOML serialization failed: {e}"),
                })?;
            fs::write(&config_path, new_content)?;

            if !quiet {
                println!("{} {} = {}", StatusIcon::Ok, key, value);
            }
            Ok(())
        }

        ConfigAction::Edit => {
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

            if !config_path.exists() {
                return Err(NikaError::ValidationError {
                    reason: format!(
                        "No config file found at {}. Run 'nika init' first.",
                        config_path.display()
                    ),
                });
            }

            let status = std::process::Command::new(&editor)
                .arg(&config_path)
                .status()
                .map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to launch editor '{editor}': {e}"),
                })?;

            if !status.success() {
                return Err(NikaError::ValidationError {
                    reason: format!("Editor '{}' exited with code {:?}", editor, status.code()),
                });
            }
            Ok(())
        }

        ConfigAction::Reset { force } => {
            if !force {
                println!(
                    "{} This will reset config to defaults. Use --force to confirm.",
                    StatusIcon::Warn
                );
                return Ok(());
            }

            if config_path.exists() {
                fs::remove_file(&config_path)?;
            }

            // Create default config
            let default_config = include_str!("../templates/config.toml");
            fs::write(&config_path, default_config)?;

            if !quiet {
                println!("{} Config reset to defaults", StatusIcon::Ok);
            }
            Ok(())
        }
    }
}

fn parse_config_value(value: &str) -> toml::Value {
    // Try boolean
    if value == "true" {
        return toml::Value::Boolean(true);
    }
    if value == "false" {
        return toml::Value::Boolean(false);
    }

    // Try integer
    if let Ok(i) = value.parse::<i64>() {
        return toml::Value::Integer(i);
    }

    // Try float
    if let Ok(f) = value.parse::<f64>() {
        return toml::Value::Float(f);
    }

    // Default to string
    toml::Value::String(value.to_string())
}

pub fn find_nika_dir() -> Result<PathBuf, NikaError> {
    let current = std::env::current_dir()?;

    let mut dir = current.as_path();
    loop {
        let nika_dir = dir.join(".nika");
        if nika_dir.exists() && nika_dir.is_dir() {
            return Ok(nika_dir);
        }

        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    // Default to current directory
    Ok(current.join(".nika"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ═══════════════════════════════════════════════════════════════════════
    // Phase 1: nika.toml project root discovery (TDD RED)
    // ═══════════════════════════════════════════════════════════════════════

    // Test 11: find_project_root_from() finds nika.toml in current dir
    #[test]
    fn find_project_root_nika_toml_in_current_dir() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("nika.toml"),
            "[project]\nname = \"test\"\n",
        )
        .unwrap();

        let result = find_project_root_from(temp.path()).unwrap();
        assert_eq!(result.root, temp.path());
        assert_eq!(result.source, ProjectRootSource::NikaToml);
    }

    // Test 12: find_project_root_from() finds nika.toml 3 levels up
    #[test]
    fn find_project_root_nika_toml_3_levels_up() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("nika.toml"),
            "[project]\nname = \"root\"\n",
        )
        .unwrap();
        let deep = temp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();

        let result = find_project_root_from(&deep).unwrap();
        assert_eq!(result.root, temp.path());
        assert_eq!(result.source, ProjectRootSource::NikaToml);
    }

    // Test 13: Fallback to .nika/ parent when no nika.toml
    #[test]
    fn find_project_root_fallback_dot_nika() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".nika")).unwrap();

        let result = find_project_root_from(temp.path()).unwrap();
        assert_eq!(result.root, temp.path());
        assert_eq!(result.source, ProjectRootSource::DotNika);
    }

    // Test 14: Returns start dir + Fallback source when nothing found
    #[test]
    fn find_project_root_returns_start_dir() {
        let temp = tempdir().unwrap();

        let result = find_project_root_from(temp.path()).unwrap();
        assert_eq!(result.root, temp.path());
        assert_eq!(result.source, ProjectRootSource::Fallback);
    }

    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn parse_bool_true() {
        assert_eq!(parse_config_value("true"), toml::Value::Boolean(true));
    }

    #[test]
    fn parse_bool_false() {
        assert_eq!(parse_config_value("false"), toml::Value::Boolean(false));
    }

    #[test]
    fn parse_integer() {
        assert_eq!(parse_config_value("42"), toml::Value::Integer(42));
    }

    #[test]
    fn parse_float() {
        assert_eq!(parse_config_value("1.5"), toml::Value::Float(1.5));
    }

    #[test]
    fn parse_string() {
        assert_eq!(
            parse_config_value("hello"),
            toml::Value::String("hello".into())
        );
    }

    #[test]
    fn parse_string_with_spaces() {
        assert_eq!(
            parse_config_value("hello world"),
            toml::Value::String("hello world".into())
        );
    }

    #[test]
    fn find_nika_dir_returns_path() {
        let result = find_nika_dir();
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with(".nika"));
    }

    // ───────────────────────────────────────────────────────────────────────
    // Phase 10: config list display tests (48-49)
    // ───────────────────────────────────────────────────────────────────────

    // Test 48: config list shows [project], [provider], [tools] sections
    #[test]
    fn config_list_shows_sections() {
        use nika_engine::runtime::boot::BootstrapConfig;

        let toml_content = r#"
[project]
name = "test-project"

[provider]
default = "anthropic"

[tools]
permission = "plan"
"#;
        let config: BootstrapConfig = toml::from_str(toml_content).unwrap();
        assert!(config.project.is_some(), "should parse [project]");
        assert_eq!(config.provider.default, "anthropic");
        assert_eq!(config.tools.permission, "plan");
    }

    // Test 49: config list can detect provider key presence
    #[test]
    fn config_list_shows_provider_status() {
        // Verify the config display can check for provider API keys
        use nika_engine::core::{ProviderCategory, KNOWN_PROVIDERS};

        let llm_providers: Vec<&str> = KNOWN_PROVIDERS
            .iter()
            .filter(|p| p.category == ProviderCategory::Llm)
            .map(|p| p.id)
            .collect();

        // Should have at least anthropic, openai, etc.
        assert!(
            llm_providers.contains(&"anthropic"),
            "KNOWN_PROVIDERS should include anthropic"
        );
        assert!(
            llm_providers.len() >= 5,
            "Should have at least 5 LLM providers, got {}",
            llm_providers.len()
        );
    }
}
