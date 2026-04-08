// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Package management subcommand handler

use clap::Subcommand;
use std::fs;
use std::path::{Path, PathBuf};

use nika_engine::error::NikaError;
use nika_engine::serde_yaml;

/// Package management actions
///
/// Manage SuperNovae packages (workflows, skills, schemas) stored in ~/.nika/packages/
#[derive(Subcommand)]
pub enum PkgAction {
    /// List installed packages
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show information about a package
    Info {
        /// Package name (e.g., @nika/seo-audit, @workflows/code-review)
        package: String,
    },

    /// Add a package to the project
    ///
    /// Downloads and installs the package and its dependencies.
    /// Updates nika.yaml and nika.lock in the project directory.
    Add {
        /// Package name (e.g., @nika/seo-audit, @workflows/code-review)
        package: String,

        /// Package type (workflow, agent, skill, prompt, job, schema)
        #[arg(short, long)]
        r#type: Option<String>,

        /// Version constraint (e.g., ^0.1, 1.0.0)
        #[arg(long, visible_alias = "ver")]
        version: Option<String>,

        /// Add as dev dependency
        #[arg(long)]
        dev: bool,
    },

    /// Remove a package from the project
    Remove {
        /// Package name to remove
        package: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Install packages from nika.yaml
    Install {
        /// Use exact versions from nika.lock
        #[arg(long)]
        frozen: bool,
    },

    /// Update packages to latest compatible versions
    Update {
        /// Package to update (updates all if not specified)
        package: Option<String>,
    },

    /// List outdated packages
    Outdated,

    /// Search packages in the registry
    Search {
        /// Search query
        query: String,

        /// Package type filter (workflow, agent, skill, prompt, job, schema)
        #[arg(short, long)]
        r#type: Option<String>,

        /// Maximum results to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
}

///
/// Manages packages (workflows, skills, schemas) stored in ~/.nika/packages/
pub async fn handle_pkg_command(action: PkgAction, _quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika_engine::registry::{list_installed, load_manifest, load_registry};

    match action {
        PkgAction::List { json } => {
            let packages = list_installed()?;

            if json {
                let registry = load_registry()?;
                println!("{}", serde_json::to_string_pretty(&registry)?);
            } else if packages.is_empty() {
                println!("{} No packages installed", "ℹ".cyan());
                println!();
                println!("Install packages with:");
                println!("  nika pkg add @nika/seo-audit");
                println!("  nika pkg install  # Install from nika.yaml");
            } else {
                println!("{}", "Installed Packages".bold());
                println!("{}", "─".repeat(60));

                for (name, version) in &packages {
                    println!("  {}@{}", name.cyan(), version.green());
                }

                println!();
                println!("{} package(s) installed", packages.len());
            }
            Ok(())
        }

        PkgAction::Info { package } => {
            // Try to find the installed version
            let registry = load_registry()?;

            if let Some(installed) = registry.get(&package) {
                println!("{}", format!("Package: {package}").bold());
                println!("{}", "─".repeat(60));
                println!("  Version:   {}", installed.version.green());
                println!("  Path:      {}", installed.manifest_path.dimmed());
                println!("  Installed: {}", installed.installed_at.dimmed());

                // Try to load manifest for more details
                if let Ok(manifest) = load_manifest(&package, &installed.version) {
                    if let Some(ref desc) = manifest.description {
                        println!("  Description: {desc}");
                    }
                    if !manifest.skills.is_empty() {
                        println!();
                        println!("  Skills:");
                        for (name, skill) in &manifest.skills {
                            println!("    • {} ({})", name.cyan(), skill.path.dimmed());
                        }
                    }
                }
            } else {
                println!("{} Package '{}' not installed", "ℹ".cyan(), package);
                println!();
                println!("To install: nika pkg add {package}");
            }
            Ok(())
        }

        PkgAction::Add {
            package,
            r#type,
            version,
            dev: _dev,
        } => {
            use nika_engine::registry::{
                ensure_nika_home, is_version_installed, package_dir, save_registry,
                InstalledPackage, RegistryClient,
            };

            println!("{} Adding package: {}", "📦".cyan(), package.green());

            // Infer type from scope if not provided
            let pkg_type = r#type
                .as_deref()
                .or_else(|| infer_package_type(&package))
                .unwrap_or("workflow");

            println!("  Type: {}", pkg_type.dimmed());

            // Ensure ~/.nika/ exists
            ensure_nika_home()?;

            // Create registry client
            let client = RegistryClient::new().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to create registry client: {e}"),
            })?;

            // Fetch package info from registry
            println!("  {} Fetching package info...", "→".dimmed());
            let pkg_info =
                client
                    .get_package(&package)
                    .await
                    .map_err(|_e| NikaError::PackageNotFound {
                        name: package.clone(),
                        version: "latest".to_string(),
                    })?;

            // Determine version to install
            let target_version = version.as_deref().unwrap_or(&pkg_info.latest_version);
            println!("  {} Version: {}", "→".dimmed(), target_version.green());

            // Check if already installed
            if is_version_installed(&package, target_version)? {
                println!(
                    "{} {}@{} is already installed",
                    "✓".green(),
                    package.cyan(),
                    target_version.green()
                );
                return Ok(());
            }

            // Get target directory
            let target_dir = package_dir(&package, target_version)?;
            println!(
                "  {} Installing to: {}",
                "→".dimmed(),
                target_dir.display().to_string().dimmed()
            );

            // Download and extract package
            println!("  {} Downloading...", "→".dimmed());
            client
                .download_and_extract(&package, target_version, &target_dir)
                .await
                .map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to download package: {e}"),
                })?;

            // Update registry index
            let mut registry = load_registry()?;
            let manifest_path = format!("packages/{package}/{target_version}/manifest.yaml");
            registry.insert(
                package.clone(),
                InstalledPackage::now(target_version.to_string(), manifest_path),
            );
            save_registry(&registry)?;

            println!();
            println!(
                "{} Successfully installed {}@{}",
                "✓".green(),
                package.cyan(),
                target_version.green()
            );

            // Show installed skills if any
            if let Ok(manifest) = load_manifest(&package, target_version) {
                if !manifest.skills.is_empty() {
                    println!();
                    println!("  {} Skills:", "📚".cyan());
                    for (name, skill) in &manifest.skills {
                        println!("    • {} ({})", name.cyan(), skill.path.dimmed());
                    }
                }
            }

            Ok(())
        }

        PkgAction::Remove { package, yes: _ } => {
            use nika_engine::registry::{package_dir, save_registry};

            println!("{} Removing package: {}", "🗑".red(), package);

            // Check if installed
            let mut registry = load_registry()?;
            let installed = match registry.get(&package) {
                Some(pkg) => pkg.clone(),
                None => {
                    println!("{} Package '{}' is not installed", "ℹ".cyan(), package);
                    return Ok(());
                }
            };

            // Get package directory
            let pkg_dir = package_dir(&package, &installed.version)?;

            // Remove package directory
            if pkg_dir.exists() {
                println!(
                    "  {} Removing {}",
                    "→".dimmed(),
                    pkg_dir.display().to_string().dimmed()
                );
                std::fs::remove_dir_all(&pkg_dir).map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to remove package directory: {e}"),
                })?;

                // Clean up empty parent directories
                if let Some(parent) = pkg_dir.parent() {
                    if parent
                        .read_dir()
                        .map(|mut d| d.next().is_none())
                        .unwrap_or(false)
                    {
                        let _ = std::fs::remove_dir(parent);
                    }
                }
            }

            // Update registry
            registry.remove(&package);
            save_registry(&registry)?;

            println!();
            println!(
                "{} Successfully removed {}@{}",
                "✓".green(),
                package.cyan(),
                installed.version.green()
            );
            Ok(())
        }

        PkgAction::Install { frozen } => {
            use nika_engine::registry::{
                ensure_nika_home, is_version_installed, package_dir, save_registry,
                InstalledPackage, Lockfile, RegistryClient,
            };

            println!(
                "{} Installing packages from project manifest{}",
                "📦".cyan(),
                if frozen { " (frozen)" } else { "" }
            );

            // Ensure ~/.nika/ exists
            ensure_nika_home()?;

            // Find project manifest
            let manifest_path = if Path::new("nika.yaml").exists() {
                PathBuf::from("nika.yaml")
            } else {
                println!("{} No project manifest found (nika.yaml)", "⚠".yellow());
                println!();
                println!("Create one with:");
                println!("  nika init");
                return Ok(());
            };

            println!("  {} Reading {}", "→".dimmed(), manifest_path.display());

            // Read manifest
            let content =
                fs::read_to_string(&manifest_path).map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to read {}: {}", manifest_path.display(), e),
                })?;

            // Parse manifest to get dependencies
            #[derive(serde::Deserialize)]
            struct ProjectManifest {
                #[serde(default)]
                dependencies: std::collections::HashMap<String, String>,
            }

            let manifest: ProjectManifest =
                serde_yaml::from_str(&content).map_err(|e| NikaError::ParseError {
                    details: format!("Failed to parse manifest: {e}"),
                })?;

            if manifest.dependencies.is_empty() {
                println!("{} No dependencies to install", "ℹ".cyan());
                return Ok(());
            }

            // Load lockfile for frozen installs
            let lockfile = if frozen {
                println!("  {} Reading nika.lock", "→".dimmed());
                Lockfile::load(None).unwrap_or_else(|_| {
                    println!(
                        "{} No nika.lock found, will use latest versions",
                        "⚠".yellow()
                    );
                    Lockfile::new()
                })
            } else {
                Lockfile::new()
            };

            // Create registry client
            let client = RegistryClient::new().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to create registry client: {e}"),
            })?;
            let mut registry = load_registry()?;
            let mut installed_count = 0;
            let mut skipped_count = 0;

            println!();
            println!(
                "{} Installing {} dependencies...",
                "📦".cyan(),
                manifest.dependencies.len()
            );

            for (name, version_spec) in &manifest.dependencies {
                // Determine version to install
                let target_version = if frozen {
                    // Use locked version if available
                    lockfile
                        .find_version(name)
                        .map_or_else(|| version_spec.clone(), |v| v.to_string())
                } else {
                    // Use version spec or fetch latest
                    if version_spec == "*" || version_spec == "latest" {
                        match client.get_package(name).await {
                            Ok(info) => info.latest_version.clone(),
                            Err(_) => {
                                println!("  {} {} - not found", "✗".red(), name.cyan());
                                continue;
                            }
                        }
                    } else {
                        version_spec.trim_start_matches('^').to_string()
                    }
                };

                // Check if already installed
                if is_version_installed(name, &target_version)? {
                    println!(
                        "  {} {}@{} (already installed)",
                        "✓".green(),
                        name.cyan(),
                        target_version.dimmed()
                    );
                    skipped_count += 1;
                    continue;
                }

                // Install package
                let target_dir = package_dir(name, &target_version)?;
                match client
                    .download_and_extract(name, &target_version, &target_dir)
                    .await
                {
                    Ok(_) => {
                        // Update registry
                        let manifest_path =
                            format!("packages/{name}/{target_version}/manifest.yaml");
                        registry.insert(
                            name.clone(),
                            InstalledPackage::now(target_version.clone(), manifest_path),
                        );
                        println!(
                            "  {} {}@{}",
                            "✓".green(),
                            name.cyan(),
                            target_version.green()
                        );
                        installed_count += 1;
                    }
                    Err(e) => {
                        println!(
                            "  {} {} - {}",
                            "✗".red(),
                            name.cyan(),
                            e.to_string().dimmed()
                        );
                    }
                }
            }

            // Save registry
            save_registry(&registry)?;

            println!();
            if installed_count > 0 || skipped_count > 0 {
                println!(
                    "{} {} package(s) installed, {} already up to date",
                    "✓".green(),
                    installed_count,
                    skipped_count
                );
            }

            Ok(())
        }

        PkgAction::Update { package } => {
            if let Some(ref pkg) = package {
                println!("{} Updating package: {}", "🔄".cyan(), pkg.green());
            } else {
                println!("{} Updating all packages", "🔄".cyan());
            }

            println!();
            println!("{} Package update is not yet implemented", "⚠".yellow());
            Ok(())
        }

        PkgAction::Outdated => {
            println!("{} Checking for outdated packages...", "📋".cyan());

            println!();
            println!(
                "{} Outdated package detection is not yet implemented",
                "⚠".yellow()
            );
            Ok(())
        }

        PkgAction::Search {
            query,
            r#type,
            limit,
        } => {
            use nika_engine::registry::RegistryClient;

            // Build search query - append type filter if provided
            let search_query = if let Some(ref t) = r#type {
                format!("{query} type:{t}")
            } else {
                query.clone()
            };

            println!(
                "{} Searching registry for '{}'...",
                "🔍".cyan(),
                query.green()
            );

            if let Some(ref t) = r#type {
                println!("  Type filter: {}", t.dimmed());
            }

            // Create registry client
            let client = RegistryClient::new().map_err(|e| NikaError::ConfigError {
                reason: format!("Failed to create registry client: {e}"),
            })?;

            // Search registry (page=1, per_page=limit)
            let response = client.search(&search_query, 1, limit).await.map_err(|e| {
                NikaError::ValidationError {
                    reason: format!("Search failed: {e}"),
                }
            })?;

            println!();
            if response.results.is_empty() {
                println!("{} No packages found matching '{}'", "ℹ".cyan(), query);
            } else {
                println!(
                    "{} Found {} package(s):",
                    "📦".cyan(),
                    response.results.len()
                );
                println!("{}", "─".repeat(60));

                for result in &response.results {
                    println!(
                        "  {} {}",
                        result.name.cyan(),
                        format!("v{}", result.version).green()
                    );
                    if let Some(ref desc) = result.description {
                        println!("    {}", desc.dimmed());
                    }
                    if let Some(ref keywords) = result.keywords {
                        if !keywords.is_empty() {
                            println!("    Keywords: {}", keywords.join(", ").dimmed());
                        }
                    }
                }

                println!("{}", "─".repeat(60));
                println!();
                println!("Install with: nika pkg add <package-name>");
            }
            Ok(())
        }
    }
}

fn infer_package_type(package: &str) -> Option<&'static str> {
    if package.starts_with("@workflows/") || package.starts_with("@nika/") {
        Some("workflow")
    } else if package.starts_with("@agents/") {
        Some("agent")
    } else if package.starts_with("@skills/") {
        Some("skill")
    } else if package.starts_with("@prompts/") {
        Some("prompt")
    } else if package.starts_with("@jobs/") {
        Some("job")
    } else if package.starts_with("@schemas/") || package.starts_with("@novanet/") {
        Some("schema")
    } else {
        None
    }
}
