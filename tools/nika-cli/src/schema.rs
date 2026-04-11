// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema management subcommand handler

use std::sync::LazyLock;

use clap::Subcommand;
use colored::Colorize;

use nika_engine::error::NikaError;

/// Regex for extracting schema version from a workflow file.
static SCHEMA_VERSION_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"^schema:\s*["']?(nika/workflow@[0-9.]+)["']?"#).expect("valid regex")
});

/// Regex for replacing schema version in a workflow file.
static SCHEMA_REPLACE_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"schema:\s*["']?nika/workflow@[\d.]+["']?"#).expect("valid regex")
});

/// Schema management actions
#[derive(Subcommand)]
pub enum SchemaAction {
    /// Upgrade workflow to latest schema version
    Upgrade {
        /// Path to .nika.yaml file or directory
        path: String,

        /// Target schema version (default: latest)
        #[arg(short, long)]
        target: Option<String>,

        /// Process all files in directory
        #[arg(long)]
        all: bool,

        /// Create backup before upgrading (default: true)
        #[arg(long, default_value = "true")]
        backup: bool,
    },

    /// Show current schema version of a workflow (or list available versions)
    Version {
        /// Path to .nika.yaml file (optional - shows available versions if omitted)
        path: Option<String>,
    },

    /// Validate workflow against schema
    Validate {
        /// Path to .nika.yaml file
        path: String,

        /// Schema version to validate against
        #[arg(short, long)]
        schema: Option<String>,
    },
}

pub fn handle_schema_command(action: SchemaAction, quiet: bool) -> Result<(), NikaError> {
    match action {
        SchemaAction::Version { path } => {
            match path {
                Some(p) => {
                    // Show version for a specific file
                    let path = std::path::Path::new(&p);
                    if !path.exists() {
                        return Err(NikaError::WorkflowNotFound {
                            path: path.to_string_lossy().to_string(),
                        });
                    }

                    let content = std::fs::read_to_string(path)?;
                    let schema = extract_schema_version(&content);

                    if quiet {
                        println!("{schema}");
                    } else {
                        println!("{} {}", "Schema version:".cyan(), schema.green());

                        // Parse and show details if it's a valid nika schema
                        if let Some(version) = schema.strip_prefix("nika/workflow@") {
                            let latest = "0.12";
                            if version == latest {
                                println!("  {} You're on the latest schema version", "✓".green());
                            } else {
                                println!("  {} Latest version is @{}", "ℹ".yellow(), latest);
                                println!(
                                    "  {} Run 'nika schema upgrade {}' to upgrade",
                                    "→".cyan(),
                                    path.display()
                                );
                            }
                        }
                    }
                }
                None => {
                    // List available schema versions
                    if !quiet {
                        println!("{}", "Available Nika schema versions:".cyan().bold());
                        println!();
                    }

                    let versions = [
                        ("0.1", "Basic: infer, exec, fetch verbs"),
                        ("0.2", "+invoke, +agent verbs, +mcp config"),
                        ("0.3", "+for_each parallelism, rig-core integration"),
                        ("0.5", "+decompose, +lazy bindings, +spawn_agent"),
                        ("0.6", "+multi-provider support (6 providers)"),
                        ("0.7", "+full streaming for all providers"),
                        ("0.8", "+Studio DX (edit history, sessions, themes)"),
                        ("0.9", "+context: file loading, +include: DAG fusion"),
                        ("0.10", "+two-phase AST, +analyzer validation"),
                        ("0.11", "+native inference (provider: native, mistral.rs)"),
                        ("0.12", "+with: bindings, +include, +depends_on (current)"),
                    ];

                    for (version, desc) in &versions {
                        if quiet {
                            println!("nika/workflow@{version}");
                        } else {
                            let prefix = if *version == "0.12" {
                                "→".green()
                            } else {
                                " ".normal()
                            };
                            println!("  {} nika/workflow@{:<4}  {}", prefix, version.cyan(), desc);
                        }
                    }

                    if !quiet {
                        println!();
                        println!(
                            "  {} Use 'nika schema version <file>' to check a workflow",
                            "ℹ".yellow()
                        );
                    }
                }
            }
            Ok(())
        }

        SchemaAction::Validate { path, schema } => {
            let path = std::path::Path::new(&path);
            if !path.exists() {
                return Err(NikaError::WorkflowNotFound {
                    path: path.to_string_lossy().to_string(),
                });
            }

            // Helper function to validate a single file
            fn validate_single_file(
                file_path: &std::path::Path,
                target_schema: Option<&str>,
                quiet: bool,
            ) -> Result<(), NikaError> {
                let content = std::fs::read_to_string(file_path)?;

                // Get schema version to validate against
                let schema_version = target_schema.map(|s| s.to_string()).unwrap_or_else(|| {
                    let extracted = extract_schema_version(&content);
                    if extracted == "(not specified)" {
                        "nika/workflow@0.12".to_string()
                    } else {
                        extracted
                    }
                });

                if !quiet {
                    println!(
                        "{} Validating {} against {}",
                        "→".cyan(),
                        file_path.display(),
                        schema_version.green()
                    );
                }

                // Try to parse the workflow to validate it
                let result = nika_engine::ast::raw::parse(&content, nika_engine::source::FileId(0));
                match result {
                    Ok(raw_workflow) => {
                        // Run analyzer for semantic validation
                        let analyze_result = nika_engine::ast::analyzer::analyze(raw_workflow);
                        if analyze_result.is_ok() {
                            if !quiet {
                                println!("{} Workflow is valid", "✓".green());
                            }
                            Ok(())
                        } else {
                            let errors: Vec<String> = analyze_result
                                .errors
                                .iter()
                                .map(|e| format!("  • {e}"))
                                .collect();
                            if !quiet {
                                println!("{} Validation failed:", "✗".red());
                                for error in &errors {
                                    println!("{}", error.red());
                                }
                            }
                            Err(NikaError::ValidationError {
                                reason: format!(
                                    "{} validation error(s) found",
                                    analyze_result.errors.len()
                                ),
                            })
                        }
                    }
                    Err(e) => {
                        if !quiet {
                            println!("{} Parse error: {}", "✗".red(), e);
                        }
                        Err(NikaError::ValidationError {
                            reason: format!("Parse error: {e}"),
                        })
                    }
                }
            }

            // Check if path is a directory
            if path.is_dir() {
                let files = find_nika_yaml_files(path)?;

                if files.is_empty() {
                    if !quiet {
                        println!(
                            "{} No .nika.yaml files found in {}",
                            "⚠".yellow(),
                            path.display()
                        );
                    }
                    return Ok(());
                }

                if !quiet {
                    println!(
                        "{} Found {} workflow file(s) in {}",
                        "→".cyan(),
                        files.len(),
                        path.display()
                    );
                    println!();
                }

                let mut passed = 0;
                let mut failed = 0;

                for file in &files {
                    match validate_single_file(file, schema.as_deref(), quiet) {
                        Ok(()) => passed += 1,
                        Err(_) => failed += 1,
                    }
                    if !quiet {
                        println!();
                    }
                }

                if !quiet {
                    println!("────────────────────────────────────────");
                    println!(
                        "{} Summary: {} passed, {} failed",
                        "→".cyan(),
                        passed.to_string().green(),
                        if failed > 0 {
                            failed.to_string().red()
                        } else {
                            failed.to_string().green()
                        }
                    );
                }

                if failed > 0 {
                    Err(NikaError::ValidationError {
                        reason: format!("{failed} file(s) failed validation"),
                    })
                } else {
                    Ok(())
                }
            } else {
                // Single file validation
                validate_single_file(path, schema.as_deref(), quiet)
            }
        }

        SchemaAction::Upgrade {
            path,
            target,
            all,
            backup,
        } => {
            let target_version = target.as_deref().unwrap_or("0.12");

            if all {
                // Upgrade all .nika.yaml files in directory
                let dir = std::path::Path::new(&path);
                if !dir.is_dir() {
                    return Err(NikaError::ValidationError {
                        reason: format!("{path} is not a directory"),
                    });
                }

                // Recursively find all .nika.yaml files
                let files = find_nika_yaml_files(dir)?;

                if files.is_empty() {
                    if !quiet {
                        println!("{} No .nika.yaml files found in {}", "ℹ".yellow(), path);
                    }
                    return Ok(());
                }

                if !quiet {
                    println!(
                        "{} Upgrading {} workflow(s) to schema @{}",
                        "→".cyan(),
                        files.len(),
                        target_version
                    );
                }

                let mut upgraded = 0;
                let mut skipped = 0;
                let mut errors = 0;

                for file in files {
                    match upgrade_workflow_file(&file, target_version, backup) {
                        Ok(true) => upgraded += 1,
                        Ok(false) => skipped += 1,
                        Err(e) => {
                            if !quiet {
                                println!("  {} {}: {}", "✗".red(), file.display(), e);
                            }
                            errors += 1;
                        }
                    }
                }

                if !quiet {
                    println!();
                    println!(
                        "{} {} upgraded, {} skipped, {} errors",
                        "Summary:".cyan(),
                        upgraded.to_string().green(),
                        skipped,
                        errors
                    );
                }

                if errors > 0 {
                    Err(NikaError::ValidationError {
                        reason: format!("{errors} file(s) failed to upgrade"),
                    })
                } else {
                    Ok(())
                }
            } else {
                // Upgrade single file
                let file_path = std::path::Path::new(&path);
                if !file_path.exists() {
                    return Err(NikaError::WorkflowNotFound { path: path.clone() });
                }

                match upgrade_workflow_file(file_path, target_version, backup)? {
                    true => {
                        if !quiet {
                            println!(
                                "{} Upgraded {} to schema @{}",
                                "✓".green(),
                                path,
                                target_version
                            );
                        }
                        Ok(())
                    }
                    false => {
                        if !quiet {
                            println!(
                                "{} {} is already at schema @{} or newer",
                                "ℹ".yellow(),
                                path,
                                target_version
                            );
                        }
                        Ok(())
                    }
                }
            }
        }
    }
}

fn extract_schema_version(content: &str) -> String {
    for line in content.lines() {
        if let Some(captures) = SCHEMA_VERSION_RE.captures(line.trim()) {
            if let Some(m) = captures.get(1) {
                return m.as_str().to_string();
            }
        }
    }
    "(not specified)".to_string()
}

/// Find all .nika.yaml files recursively in a directory
fn find_nika_yaml_files(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, NikaError> {
    let mut files = Vec::new();

    fn visit_dir(
        dir: &std::path::Path,
        files: &mut Vec<std::path::PathBuf>,
    ) -> Result<(), NikaError> {
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    // Skip hidden directories and common ignore patterns
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !name.starts_with('.') && name != "node_modules" && name != "target" {
                        visit_dir(&path, files)?;
                    }
                } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".nika.yaml") || name.ends_with(".nika.yml") {
                        files.push(path);
                    }
                }
            }
        }
        Ok(())
    }

    visit_dir(dir, &mut files)?;
    files.sort();
    Ok(files)
}

/// Upgrade a single workflow file to target schema version
/// Returns Ok(true) if upgraded, Ok(false) if skipped (already at version)
fn upgrade_workflow_file(
    path: &std::path::Path,
    target_version: &str,
    backup: bool,
) -> Result<bool, NikaError> {
    let content = std::fs::read_to_string(path)?;

    // Get current schema version using regex helper
    let current_schema = extract_schema_version(&content);
    let current = if current_schema == "(not specified)" {
        "0.1".to_string()
    } else {
        // Extract version number from "nika/workflow@X.Y"
        current_schema
            .strip_prefix("nika/workflow@")
            .unwrap_or("0.1")
            .to_string()
    };

    // Parse versions for comparison
    let current_parts: Vec<u32> = current
        .split('.')
        .filter_map(|s: &str| s.parse::<u32>().ok())
        .collect();
    let target_parts: Vec<u32> = target_version
        .split('.')
        .filter_map(|s: &str| s.parse::<u32>().ok())
        .collect();

    // Compare versions (simple comparison)
    let current_num = current_parts.first().copied().unwrap_or(0) * 100
        + current_parts.get(1).copied().unwrap_or(0);
    let target_num = target_parts.first().copied().unwrap_or(0) * 100
        + target_parts.get(1).copied().unwrap_or(0);

    if current_num >= target_num {
        return Ok(false); // Already at or above target version
    }

    // Create backup if requested
    if backup {
        let backup_path = path.with_extension("nika.yaml.bak");
        std::fs::copy(path, &backup_path)?;
    }

    // Update schema version using regex replacement (preserves formatting)
    let new_schema_line = format!(r#"schema: "nika/workflow@{target_version}""#);
    let updated = SCHEMA_REPLACE_RE
        .replace(&content, new_schema_line.as_str())
        .to_string();

    // If no replacement was made, the file might not have a schema line - add one
    let updated = if updated == content {
        // Prepend schema line at the beginning
        format!("{new_schema_line}\n{content}")
    } else {
        updated
    };

    std::fs::write(path, updated)?;

    Ok(true)
}
