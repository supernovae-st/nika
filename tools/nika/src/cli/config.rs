//! Configuration management subcommand handler

use clap::Subcommand;
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

use nika::error::NikaError;

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
    // Find .nika directory
    let nika_dir = find_nika_dir()?;
    let config_path = nika_dir.join("config.toml");

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
                        "ℹ".cyan(),
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
                        reason: format!("Invalid TOML: {}", e),
                    })?;
                let json = serde_json::to_string_pretty(&value).map_err(|e| {
                    NikaError::ValidationError {
                        reason: format!("JSON conversion failed: {}", e),
                    }
                })?;
                println!("{}", json);
            } else {
                println!("{}", "Nika Configuration".bold());
                println!("{}", "─".repeat(40));
                println!();
                println!("{}", content);
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
                    reason: format!("Invalid TOML: {}", e),
                })?;

            // Navigate to the key (dot-separated path)
            let mut current = &value;
            for part in key.split('.') {
                current = current
                    .get(part)
                    .ok_or_else(|| NikaError::ValidationError {
                        reason: format!("Key '{}' not found", key),
                    })?;
            }

            // Print the value
            match current {
                toml::Value::String(s) => println!("{}", s),
                toml::Value::Integer(i) => println!("{}", i),
                toml::Value::Float(f) => println!("{}", f),
                toml::Value::Boolean(b) => println!("{}", b),
                _ => println!("{}", current),
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
                        reason: format!("Invalid TOML: {}", e),
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
                            reason: format!("Config key '{}' not found", part),
                        })?
                        .as_table_mut()
                        .ok_or_else(|| NikaError::ValidationError {
                            reason: format!("'{}' is not a table", part),
                        })?;
                }
            }

            // Write back
            let new_content =
                toml::to_string_pretty(&doc).map_err(|e| NikaError::ValidationError {
                    reason: format!("TOML serialization failed: {}", e),
                })?;
            fs::write(&config_path, new_content)?;

            if !quiet {
                println!("{} {} = {}", "✓".green(), key, value);
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
                    reason: format!("Failed to launch editor '{}': {}", editor, e),
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
                    "⚠".yellow()
                );
                return Ok(());
            }

            if config_path.exists() {
                fs::remove_file(&config_path)?;
            }

            // Create default config
            let default_config = include_str!("../../templates/config.toml");
            fs::write(&config_path, default_config)?;

            if !quiet {
                println!("{} Config reset to defaults", "✓".green());
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
