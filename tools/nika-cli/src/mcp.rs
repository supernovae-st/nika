//! MCP server management subcommand handler

use clap::Subcommand;

use nika_engine::ast::parse_workflow;
use nika_engine::error::NikaError;
use nika_engine::{McpClient, McpConfig};

/// MCP server management actions
///
/// Manage MCP servers at project (.mcp.json preferred, .nika/mcp.yaml fallback),
/// global (~/.nika/mcp.yaml), or workflow levels. 100 aliases available.
#[derive(Subcommand)]
pub enum McpAction {
    /// Add an MCP server to global or project config
    ///
    /// Supports aliases (neo4j, github, perplexity) or full npm packages.
    /// Use `nika mcp aliases` to see all 100 available aliases.
    Add {
        /// Server name (alias like 'neo4j' or package like '@neo4j/mcp-neo4j')
        name: String,

        /// Add to global config (~/.nika/mcp.yaml) instead of project
        #[arg(long)]
        global: bool,

        /// Add to project config (.nika/mcp.yaml) - default
        #[arg(long)]
        project: bool,

        /// Server description
        #[arg(short, long)]
        description: Option<String>,

        /// Custom command (default: npx)
        #[arg(long)]
        command: Option<String>,

        /// Custom arguments (overrides default `-y <package>`)
        #[arg(long)]
        args: Option<Vec<String>>,
    },

    /// Remove an MCP server from global or project config
    Remove {
        /// Server name to remove
        name: String,

        /// Remove from global config (~/.nika/mcp.yaml)
        #[arg(long)]
        global: bool,

        /// Remove from project config (.nika/mcp.yaml) - default
        #[arg(long)]
        project: bool,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// List MCP servers (merged from global, project, workflow)
    List {
        /// Workflow file with MCP definitions
        #[arg(short, long)]
        workflow: Option<String>,

        /// Show only global config (~/.nika/mcp.yaml)
        #[arg(long)]
        global: bool,

        /// Show only project config (.nika/mcp.yaml)
        #[arg(long)]
        project: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List available MCP server aliases (100 total)
    Aliases {
        /// Filter by category (anthropic, databases, search, developer, productivity, ai, image, communication, vectordb, analytics, ecommerce, cms, devops, social, maps)
        #[arg(short, long)]
        category: Option<String>,
    },

    /// Test connection to an MCP server
    Test {
        /// Workflow file with MCP definitions
        workflow: String,
        /// MCP server name (from workflow's mcp: section)
        server: String,
    },

    /// List tools available from an MCP server
    Tools {
        /// Workflow file with MCP definitions
        workflow: String,
        /// MCP server name
        server: String,
    },

    /// Start Nika as an MCP server (expose workflow tools to AI coding assistants)
    Serve,
}

/// Handle MCP server management commands
pub async fn handle_mcp_command(action: McpAction, _quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika_engine::core::{
        add_server_to_global, add_server_to_project, aliases_by_category, global_config_path,
        load_global_config, load_merged_config, load_project_config, project_config_path,
        remove_server_from_global, remove_server_from_project, resolve_name,
        server_from_npm_package, McpServer as CoreMcpServer,
    };

    match action {
        McpAction::Add {
            name,
            global,
            project,
            description,
            command,
            args,
        } => {
            // Determine scope (default to project if neither specified)
            let use_global = global && !project;

            // Resolve alias to package name (or use as-is if it's a package)
            let package = resolve_name(&name).ok_or_else(|| NikaError::ValidationError {
                reason: format!(
                    "Unknown alias '{name}'. Use 'nika mcp aliases' to see available aliases, \
                     or specify a full npm package like '@org/package'"
                ),
            })?;

            // Create server config
            let server = if let (Some(cmd), Some(args)) = (command, args) {
                // Custom command/args
                CoreMcpServer {
                    command: cmd,
                    args,
                    env: std::collections::HashMap::new(),
                    description,
                    enabled: true,
                    source: None,
                }
            } else {
                // Default npx -y <package>
                server_from_npm_package(&package, description.as_deref())
            };

            // Add to appropriate config
            let (added, scope, path) = if use_global {
                let added = add_server_to_global(&name, server).map_err(|e| {
                    NikaError::ValidationError {
                        reason: format!("Failed to add to global config: {e}"),
                    }
                })?;
                (added, "global", global_config_path())
            } else {
                let added = add_server_to_project(&name, server).map_err(|e| {
                    NikaError::ValidationError {
                        reason: format!("Failed to add to project config: {e}"),
                    }
                })?;
                (added, "project", project_config_path())
            };

            if added {
                println!(
                    "{} Added '{}' to {} config",
                    "✓".green(),
                    name.bold(),
                    scope
                );
                if let Some(ref p) = path {
                    let path_str = p.display().to_string();
                    println!("  {}", path_str.dimmed());
                }
                if name != package {
                    println!("  Alias '{}' → {}", name, package.dimmed());
                }
            } else {
                println!(
                    "{} Server '{}' already exists in {} config",
                    "ℹ".cyan(),
                    name,
                    scope
                );
            }
            Ok(())
        }

        McpAction::Remove {
            name,
            global,
            project,
            yes,
        } => {
            // Determine scope (default to project if neither specified)
            let use_global = global && !project;

            // Confirmation (unless --yes)
            if !yes {
                let scope = if use_global { "global" } else { "project" };
                print!("Remove '{}' from {} config? [y/N] ", name.bold(), scope);
                use std::io::Write;
                let _ = std::io::stdout().flush();

                let mut input = String::new();
                std::io::stdin()
                    .read_line(&mut input)
                    .map_err(|e| NikaError::ValidationError {
                        reason: format!("Failed to read from stdin: {e}"),
                    })?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("{} Cancelled", "ℹ".cyan());
                    return Ok(());
                }
            }

            // Remove from appropriate config
            let (removed, scope) = if use_global {
                let removed =
                    remove_server_from_global(&name).map_err(|e| NikaError::ValidationError {
                        reason: format!("Failed to remove from global config: {e}"),
                    })?;
                (removed, "global")
            } else {
                let removed =
                    remove_server_from_project(&name).map_err(|e| NikaError::ValidationError {
                        reason: format!("Failed to remove from project config: {e}"),
                    })?;
                (removed, "project")
            };

            if removed {
                println!(
                    "{} Removed '{}' from {} config",
                    "✓".green(),
                    name.bold(),
                    scope
                );
            } else {
                println!(
                    "{} Server '{}' not found in {} config",
                    "ℹ".cyan(),
                    name,
                    scope
                );
            }
            Ok(())
        }

        McpAction::List {
            workflow,
            global,
            project,
            json,
        } => {
            // If workflow provided, show workflow's MCP config
            if let Some(file) = workflow {
                let yaml = tokio::fs::read_to_string(&file).await?;
                let wf = parse_workflow(&yaml)?;

                match wf.mcp {
                    Some(ref mcp_servers) if !mcp_servers.is_empty() => {
                        if json {
                            println!("{}", serde_json::to_string_pretty(&mcp_servers)?);
                        } else {
                            println!("{}", "MCP Servers (workflow)".bold());
                            println!("{}", "─".repeat(60));

                            for (name, config) in mcp_servers {
                                println!(
                                    "  {:12} {} {}",
                                    name.cyan(),
                                    config.command.dimmed(),
                                    config.args.join(" ").dimmed()
                                );
                                if !config.env.is_empty() {
                                    for key in config.env.keys() {
                                        println!("               env: {}", key.yellow());
                                    }
                                }
                            }
                            println!();
                            println!(
                                "{}",
                                format!("Use 'nika mcp test {file} <server>' to test connection")
                                    .dimmed()
                            );
                        }
                    }
                    _ => {
                        println!("{} No MCP servers defined in {}", "ℹ".cyan(), file);
                    }
                }
                return Ok(());
            }

            // Show global and/or project configs
            let show_global = global || !project;
            let show_project = project || !global;

            if json {
                // JSON output
                let merged = load_merged_config().map_err(|e| NikaError::ValidationError {
                    reason: format!("Failed to load config: {e}"),
                })?;
                println!("{}", serde_json::to_string_pretty(&merged.servers)?);
                return Ok(());
            }

            // Text output
            let mut any_servers = false;

            if show_global {
                match load_global_config() {
                    Ok(Some(config)) if !config.servers.is_empty() => {
                        any_servers = true;
                        println!("{}", "Global MCP Servers".bold());
                        if let Some(p) = global_config_path() {
                            println!("{}", p.display().to_string().dimmed());
                        }
                        println!("{}", "─".repeat(60));

                        for (name, server) in &config.servers {
                            let status = if server.enabled {
                                "✓".green()
                            } else {
                                "○".dimmed()
                            };
                            println!(
                                "  {} {:12} {} {}",
                                status,
                                name.cyan(),
                                server.command.dimmed(),
                                server.args.join(" ").dimmed()
                            );
                        }
                        println!();
                    }
                    Ok(Some(_) | None) => {
                        if global && !project {
                            println!("{} No global MCP servers configured", "ℹ".cyan());
                            if let Some(p) = global_config_path() {
                                println!("  Config path: {}", p.display());
                            }
                        }
                    }
                    Err(e) => {
                        println!("{} Failed to load global config: {}", "✗".red(), e);
                    }
                }
            }

            if show_project {
                match load_project_config() {
                    Ok(Some(config)) if !config.servers.is_empty() => {
                        any_servers = true;
                        println!("{}", "Project MCP Servers".bold());
                        if let Some(p) = project_config_path() {
                            println!("{}", p.display().to_string().dimmed());
                        }
                        println!("{}", "─".repeat(60));

                        for (name, server) in &config.servers {
                            let status = if server.enabled {
                                "✓".green()
                            } else {
                                "○".dimmed()
                            };
                            println!(
                                "  {} {:12} {} {}",
                                status,
                                name.cyan(),
                                server.command.dimmed(),
                                server.args.join(" ").dimmed()
                            );
                        }
                        println!();
                    }
                    Ok(Some(_) | None) => {
                        if project && !global {
                            println!("{} No project MCP servers configured", "ℹ".cyan());
                            if let Some(p) = project_config_path() {
                                println!("  Config path: {}", p.display());
                            }
                        }
                    }
                    Err(e) => {
                        println!("{} Failed to load project config: {}", "✗".red(), e);
                    }
                }
            }

            if !any_servers && !global && !project {
                println!("{} No MCP servers configured", "ℹ".cyan());
                println!();
                println!("Add servers with:");
                println!("  nika mcp add neo4j          # Add to project config");
                println!("  nika mcp add neo4j --global # Add to global config");
                println!();
                println!("Or specify a workflow file:");
                println!("  nika mcp list --workflow my-flow.nika.yaml");
            }
            Ok(())
        }

        McpAction::Serve => {
            eprintln!("Starting Nika MCP server on stdio...");
            nika_mcp::server::run_server()
                .await
                .map_err(|e| NikaError::McpStartError {
                    name: "nika".to_string(),
                    reason: e.to_string(),
                })?;
            Ok(())
        }

        McpAction::Aliases { category } => {
            use nika_engine::core::{pricing_label, McpPricing, CATEGORIES, MCP_ALIASES};

            println!(
                "{}",
                format!("MCP Server Aliases ({} total)", MCP_ALIASES.len()).bold()
            );
            println!("{}", "─".repeat(60));

            // Helper: colorize pricing badge
            let fmt_pricing = |p: McpPricing| -> colored::ColoredString {
                match p {
                    McpPricing::Free => pricing_label(p).green(),
                    McpPricing::Freemium => pricing_label(p).yellow(),
                    McpPricing::Paid => pricing_label(p).red(),
                }
            };

            if let Some(cat) = category {
                let aliases = aliases_by_category(&cat);
                if aliases.is_empty() {
                    let known: Vec<&str> = CATEGORIES.iter().map(|(k, _)| *k).collect();
                    println!(
                        "{} Unknown category '{}'. Available: {}",
                        "✗".red(),
                        cat,
                        known.join(", ")
                    );
                } else {
                    println!();
                    println!("{}", cat.to_uppercase().bold());
                    for alias in aliases {
                        println!(
                            "  {:20} {:10} {}",
                            alias.name.cyan(),
                            fmt_pricing(alias.pricing),
                            format!("-> {}", alias.package).dimmed()
                        );
                    }
                }
            } else {
                // Show all 15 categories
                for (cat_key, cat_name) in CATEGORIES {
                    println!();
                    println!("{}", cat_name.bold());
                    for alias in aliases_by_category(cat_key) {
                        println!(
                            "  {:20} {:10} {}",
                            alias.name.cyan(),
                            fmt_pricing(alias.pricing),
                            format!("-> {}", alias.package).dimmed()
                        );
                    }
                }
            }

            println!();
            println!(
                "{}",
                "Use: nika mcp add <alias>  (e.g., nika mcp add neo4j)".dimmed()
            );
            Ok(())
        }

        McpAction::Test { workflow, server } => {
            println!("Testing connection to MCP server '{}'...", server.bold());

            // Load workflow
            let yaml = tokio::fs::read_to_string(&workflow).await?;
            let wf = parse_workflow(&yaml)?;

            // Get MCP config
            let mcp_servers = wf.mcp.as_ref().ok_or_else(|| NikaError::ValidationError {
                reason: format!("No mcp: section in {workflow}"),
            })?;

            let inline_config =
                mcp_servers
                    .get(&server)
                    .ok_or_else(|| NikaError::McpNotConnected {
                        name: server.clone(),
                    })?;

            // Build McpConfig
            let mut config = McpConfig::new(&server, &inline_config.command)
                .with_args(inline_config.args.iter().cloned());
            for (key, value) in &inline_config.env {
                config = config.with_env(key, value);
            }
            if let Some(ref cwd) = inline_config.cwd {
                config = config.with_cwd(cwd);
            }

            // Connect
            let client = McpClient::new(config)?;

            match client.connect().await {
                Ok(()) => {
                    println!("{} Connection successful!", "✓".green());

                    // List tools
                    match client.list_tools().await {
                        Ok(tools) => {
                            println!("  Found {} tools", tools.len());
                        }
                        Err(e) => {
                            println!("  {} Failed to list tools: {}", "⚠".yellow(), e);
                        }
                    }
                }
                Err(e) => {
                    println!("{} Connection failed: {}", "✗".red(), e);
                }
            }
            Ok(())
        }

        McpAction::Tools { workflow, server } => {
            // Load workflow
            let yaml = tokio::fs::read_to_string(&workflow).await?;
            let wf = parse_workflow(&yaml)?;

            // Get MCP config
            let mcp_servers = wf.mcp.as_ref().ok_or_else(|| NikaError::ValidationError {
                reason: format!("No mcp: section in {workflow}"),
            })?;

            let inline_config =
                mcp_servers
                    .get(&server)
                    .ok_or_else(|| NikaError::McpNotConnected {
                        name: server.clone(),
                    })?;

            // Build McpConfig
            let mut config = McpConfig::new(&server, &inline_config.command)
                .with_args(inline_config.args.iter().cloned());
            for (key, value) in &inline_config.env {
                config = config.with_env(key, value);
            }
            if let Some(ref cwd) = inline_config.cwd {
                config = config.with_cwd(cwd);
            }

            println!("Connecting to MCP server '{}'...", server.bold());

            // Connect
            let client = McpClient::new(config)?;
            client.connect().await?;

            // List tools
            let tools = client.list_tools().await?;

            println!();
            println!("{}", format!("Tools from '{server}'").bold());
            println!("{}", "─".repeat(60));

            if tools.is_empty() {
                println!("  {} No tools available", "ℹ".cyan());
            } else {
                for tool in &tools {
                    println!("  {} {}", "•".cyan(), tool.name.bold());
                    if let Some(ref desc) = tool.description {
                        // Truncate long descriptions
                        let desc_truncated: String = desc.chars().take(80).collect();
                        if desc.len() > 80 {
                            println!("    {}", format!("{desc_truncated}...").dimmed());
                        } else {
                            println!("    {}", desc_truncated.dimmed());
                        }
                    }
                }
            }

            println!();
            println!("{} tools available", tools.len());
            Ok(())
        }
    }
}
