//! Jobs daemon subcommand handler

use clap::Subcommand;
use std::path::PathBuf;

use nika::error::NikaError;

use super::config::find_nika_dir;

/// Jobs Daemon management actions
#[derive(Subcommand)]
pub enum JobsAction {
    /// Start the Jobs Daemon (daemonizes by default)
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,

        /// Path to jobs configuration file
        #[arg(short, long, default_value = ".nika/jobs.toml")]
        config: PathBuf,
    },

    /// Stop the running Jobs Daemon
    Stop {
        /// Force kill (SIGKILL instead of SIGTERM)
        #[arg(short, long)]
        force: bool,
    },

    /// Show status of the Jobs Daemon
    Status {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// List all configured jobs
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Path to jobs configuration file
        #[arg(short, long, default_value = ".nika/jobs.toml")]
        config: PathBuf,
    },

    /// Manually trigger a job
    Trigger {
        /// Job name to trigger
        job_name: String,

        /// Path to jobs configuration file
        #[arg(short, long, default_value = ".nika/jobs.toml")]
        config: PathBuf,
    },

    /// Pause a job (skip scheduled runs)
    Pause {
        /// Job name to pause
        job_name: String,
    },

    /// Resume a paused job
    Resume {
        /// Job name to resume
        job_name: String,
    },

    /// Show execution history for jobs
    History {
        /// Job name (all jobs if not specified)
        job_name: Option<String>,

        /// Limit number of entries
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Reload daemon configuration
    Reload,

    // === Background Job Commands ===
    // These proxy to `nika-daemon jobs` for background workflow execution
    /// Submit a workflow for background execution
    ///
    /// Returns a job ID that can be used to track status and output.
    Submit {
        /// Path to workflow file
        workflow: PathBuf,

        /// Additional workflow arguments
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// Optional job name (defaults to workflow filename)
        #[arg(short, long)]
        name: Option<String>,

        /// Job priority (-10 to 10, higher = more priority)
        #[arg(short, long, default_value = "0")]
        priority: i32,
    },

    /// Cancel a running background job
    Cancel {
        /// Job ID to cancel
        id: String,
    },

    /// Show output from a background job
    Output {
        /// Job ID
        id: String,

        /// Follow output in real-time (like tail -f)
        #[arg(short, long)]
        follow: bool,
    },

    /// Clear completed/failed background jobs
    Clear {
        /// Clear all jobs including running ones (use with caution)
        #[arg(long)]
        all: bool,
    },
}

pub async fn handle_jobs_command(action: JobsAction, quiet: bool) -> Result<(), NikaError> {
    use colored::Colorize;
    use nika::jobs::{JobsConfig, JobsDaemon, StateStore};

    match action {
        JobsAction::Start { foreground, config } => {
            // Check if config file exists
            if !config.exists() {
                return Err(NikaError::ConfigError {
                    reason: format!("Jobs config file not found: {}", config.display()),
                });
            }

            // Load configuration
            let jobs_config =
                JobsConfig::from_file(&config).map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to load jobs config: {}", e),
                })?;

            if !quiet {
                println!(
                    "{} Starting Jobs Daemon with {} jobs from {}",
                    "🚀".bold(),
                    jobs_config.definitions.len().to_string().cyan(),
                    config.display()
                );
            }

            // Create and start daemon
            let mut daemon = JobsDaemon::new(jobs_config).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to create daemon: {}", e),
            })?;

            if foreground {
                // Run in foreground (blocking)
                if !quiet {
                    println!(
                        "{} Running in foreground mode (Ctrl+C to stop)",
                        "ℹ️".bold()
                    );
                }
                daemon.start().await.map_err(|e| NikaError::RuntimeError {
                    reason: format!("Daemon error: {}", e),
                })?;
            } else {
                // Daemonize
                daemon.start().await.map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to start daemon: {}", e),
                })?;

                if !quiet {
                    println!("{} Jobs Daemon started successfully", "✅".green());
                }
            }
        }

        JobsAction::Stop { force } => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            if !pid_file.exists() {
                return Err(NikaError::RuntimeError {
                    reason: "No running daemon found (jobs.pid not found)".to_string(),
                });
            }

            if !quiet {
                println!(
                    "{} Stopping Jobs Daemon{}...",
                    "🛑".bold(),
                    if force { " (force)" } else { "" }
                );
            }

            JobsDaemon::stop_by_pid_file(&pid_file).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to stop daemon: {}", e),
            })?;

            if !quiet {
                println!("{} Jobs Daemon stopped", "✅".green());
            }
        }

        JobsAction::Status { json } => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            let status = JobsDaemon::get_status_from_pid_file(&pid_file);
            let is_running = matches!(
                status,
                nika::jobs::DaemonStatus::Running | nika::jobs::DaemonStatus::Starting
            );

            if json {
                let output = serde_json::json!({
                    "running": is_running,
                    "status": status.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!("{}", "Jobs Daemon Status".bold().cyan());
                match status {
                    nika::jobs::DaemonStatus::Running => {
                        println!("  {} {}", "Status:".dimmed(), "Running".green().bold());
                    }
                    nika::jobs::DaemonStatus::Starting => {
                        println!("  {} {}", "Status:".dimmed(), "Starting".yellow().bold());
                    }
                    nika::jobs::DaemonStatus::ShuttingDown => {
                        println!(
                            "  {} {}",
                            "Status:".dimmed(),
                            "Shutting Down".yellow().bold()
                        );
                    }
                    nika::jobs::DaemonStatus::Stopped => {
                        println!("  {} {}", "Status:".dimmed(), "Stopped".red().bold());
                    }
                }
            }
        }

        JobsAction::List { json, config } => {
            // Check if config file exists
            if !config.exists() {
                return Err(NikaError::ConfigError {
                    reason: format!("Jobs config file not found: {}", config.display()),
                });
            }

            // Load configuration
            let jobs_config =
                JobsConfig::from_file(&config).map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to load jobs config: {}", e),
                })?;

            if json {
                let output: Vec<serde_json::Value> = jobs_config
                    .definitions
                    .iter()
                    .map(|j| {
                        serde_json::json!({
                            "name": j.name,
                            "workflow": j.workflow.display().to_string(),
                            "schedule": format!("{:?}", j.trigger),
                            "enabled": j.enabled,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!("{}", "Configured Jobs".bold().cyan());
                println!();
                for job in &jobs_config.definitions {
                    let status = if job.enabled {
                        "●".green()
                    } else {
                        "○".dimmed()
                    };
                    println!(
                        "  {} {} {}",
                        status,
                        job.name.bold(),
                        format!("({})", job.workflow.display()).dimmed()
                    );
                    println!("    {} {:?}", "Schedule:".dimmed(), job.trigger);
                }
                println!();
                println!(
                    "{} {} jobs configured",
                    "Total:".dimmed(),
                    jobs_config.definitions.len()
                );
            }
        }

        JobsAction::Trigger { job_name, config } => {
            // Load config and create daemon to trigger job
            let jobs_config =
                JobsConfig::from_file(&config).map_err(|e| NikaError::ConfigError {
                    reason: format!("Failed to load jobs config: {}", e),
                })?;

            // Verify job exists
            if !jobs_config.definitions.iter().any(|j| j.name == job_name) {
                return Err(NikaError::ValidationError {
                    reason: format!("Job '{}' not found in config", job_name),
                });
            }

            if !quiet {
                println!("{} Triggering job '{}'...", "⚡".bold(), job_name.cyan());
            }

            // Create daemon and trigger
            let daemon = JobsDaemon::new(jobs_config).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to create daemon: {}", e),
            })?;

            daemon
                .trigger_job(&job_name)
                .await
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to trigger job: {}", e),
                })?;

            if !quiet {
                println!("{} Job '{}' triggered successfully", "✅".green(), job_name);
            }
        }

        JobsAction::Pause { job_name } => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            if !pid_file.exists() {
                return Err(NikaError::RuntimeError {
                    reason: "No running daemon found".to_string(),
                });
            }

            // Send pause command to daemon via IPC
            // For now, we'll use a simple approach via the daemon
            let daemon = JobsDaemon::from_config_file(&find_nika_dir()?.join("jobs.toml"))
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to connect to daemon: {}", e),
                })?;

            daemon
                .pause_job(&job_name)
                .await
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to pause job: {}", e),
                })?;

            if !quiet {
                println!("{} Job '{}' paused", "⏸️".bold(), job_name.yellow());
            }
        }

        JobsAction::Resume { job_name } => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            if !pid_file.exists() {
                return Err(NikaError::RuntimeError {
                    reason: "No running daemon found".to_string(),
                });
            }

            let daemon = JobsDaemon::from_config_file(&find_nika_dir()?.join("jobs.toml"))
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to connect to daemon: {}", e),
                })?;

            daemon
                .resume_job(&job_name)
                .await
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to resume job: {}", e),
                })?;

            if !quiet {
                println!("{} Job '{}' resumed", "▶️".bold(), job_name.green());
            }
        }

        JobsAction::History {
            job_name,
            limit,
            json,
        } => {
            let state_dir = find_nika_dir()?.join("jobs.db");
            let store = StateStore::new(&state_dir).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to open state store: {}", e),
            })?;

            let executions = store
                .list_executions(job_name.as_deref(), limit)
                .map_err(|e| NikaError::RuntimeError {
                    reason: format!("Failed to query history: {}", e),
                })?;

            if json {
                let output: Vec<serde_json::Value> = executions
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "id": e.id,
                            "job_name": e.job_name,
                            "status": format!("{:?}", e.status),
                            "trigger": e.trigger,
                            "started_at": e.started_at.to_rfc3339(),
                            "ended_at": e.ended_at.map(|t| t.to_rfc3339()),
                            "duration_ms": e.duration_ms,
                            "attempt": e.attempt,
                            "error": e.error,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let title = match &job_name {
                    Some(name) => format!("Execution History for '{}'", name),
                    None => "Execution History (All Jobs)".to_string(),
                };
                println!("{}", title.bold().cyan());
                println!();

                if executions.is_empty() {
                    println!("  {}", "No executions found".dimmed());
                } else {
                    for exec in &executions {
                        let status_icon = match exec.status {
                            nika::jobs::JobExecutionStatus::Completed => "✅",
                            nika::jobs::JobExecutionStatus::Failed => "❌",
                            nika::jobs::JobExecutionStatus::Running => "🔄",
                            nika::jobs::JobExecutionStatus::Queued => "⏳",
                            nika::jobs::JobExecutionStatus::Cancelled => "🚫",
                        };

                        let duration = exec
                            .duration_ms
                            .map(|d| format!("{}ms", d))
                            .unwrap_or_else(|| "-".to_string());

                        println!(
                            "  {} {} {} {}",
                            status_icon,
                            exec.job_name.bold(),
                            exec.started_at
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string()
                                .dimmed(),
                            format!("({})", duration).dimmed()
                        );

                        if let Some(ref err) = exec.error {
                            println!("    {} {}", "Error:".red(), err);
                        }
                    }
                }

                println!();
                println!(
                    "{} {} executions shown",
                    "Total:".dimmed(),
                    executions.len()
                );
            }
        }

        JobsAction::Reload => {
            let pid_file = find_nika_dir()?.join("jobs.pid");

            if !quiet {
                println!("{} Reloading daemon configuration...", "🔄".bold());
            }

            JobsDaemon::reload_by_signal(&pid_file).map_err(|e| NikaError::RuntimeError {
                reason: format!("Failed to reload daemon: {}", e),
            })?;

            if !quiet {
                println!("{} Configuration reload signal sent", "✅".green());
            }
        }

        // === Background Job Commands ===
        // These proxy to `nika-daemon jobs` for background workflow execution
        JobsAction::Submit {
            workflow,
            args,
            name,
            priority,
        } => {
            // Proxy to: nika-daemon jobs submit <workflow> [args...] [--name NAME] [--priority N]
            let mut cmd_args = vec!["jobs", "submit"];
            let workflow_str = workflow.to_string_lossy();
            cmd_args.push(&workflow_str);

            // Add workflow arguments
            let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            cmd_args.extend(args_refs.iter());

            let name_flag;
            if let Some(n) = &name {
                cmd_args.push("--name");
                name_flag = n.clone();
                cmd_args.push(&name_flag);
            }

            let priority_str = priority.to_string();
            cmd_args.push("--priority");
            cmd_args.push(&priority_str);

            if !quiet {
                println!(
                    "{} Submitting workflow for background execution...",
                    "📤".bold()
                );
            }

            let status = std::process::Command::new("nika-daemon")
                .args(&cmd_args)
                .status()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        eprintln!();
                        eprintln!("nika-daemon not found. Install from:");
                        eprintln!("  cargo install nika-daemon");
                    }
                    NikaError::InvalidConfig {
                        message: format!("Failed to run nika-daemon jobs submit: {}", e),
                    }
                })?;

            if !status.success() {
                return Err(NikaError::RuntimeError {
                    reason: "nika-daemon jobs submit failed".to_string(),
                });
            }
        }

        JobsAction::Cancel { id } => {
            // Proxy to: nika-daemon jobs cancel <id>
            if !quiet {
                println!("{} Cancelling job {}...", "🛑".bold(), id);
            }

            let status = std::process::Command::new("nika-daemon")
                .args(["jobs", "cancel", &id])
                .status()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        eprintln!();
                        eprintln!("nika-daemon not found. Install from:");
                        eprintln!("  cargo install nika-daemon");
                    }
                    NikaError::InvalidConfig {
                        message: format!("Failed to run nika-daemon jobs cancel: {}", e),
                    }
                })?;

            if !status.success() {
                return Err(NikaError::RuntimeError {
                    reason: format!("Failed to cancel job {}", id),
                });
            }
        }

        JobsAction::Output { id, follow } => {
            // Proxy to: nika-daemon jobs output <id> [--follow]
            let mut cmd_args = vec!["jobs", "output", &id];
            if follow {
                cmd_args.push("--follow");
            }

            // For output, we want to stream stdout/stderr directly
            let status = std::process::Command::new("nika-daemon")
                .args(&cmd_args)
                .status()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        eprintln!();
                        eprintln!("nika-daemon not found. Install from:");
                        eprintln!("  cargo install nika-daemon");
                    }
                    NikaError::InvalidConfig {
                        message: format!("Failed to run nika-daemon jobs output: {}", e),
                    }
                })?;

            if !status.success() {
                return Err(NikaError::RuntimeError {
                    reason: format!("Failed to get output for job {}", id),
                });
            }
        }

        JobsAction::Clear { all } => {
            // Proxy to: nika-daemon jobs clear [--all]
            let mut cmd_args = vec!["jobs", "clear"];
            if all {
                cmd_args.push("--all");
            }

            if !quiet {
                if all {
                    println!("{} Clearing all jobs...", "🧹".bold());
                } else {
                    println!("{} Clearing completed/failed jobs...", "🧹".bold());
                }
            }

            let status = std::process::Command::new("nika-daemon")
                .args(&cmd_args)
                .status()
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        eprintln!();
                        eprintln!("nika-daemon not found. Install from:");
                        eprintln!("  cargo install nika-daemon");
                    }
                    NikaError::InvalidConfig {
                        message: format!("Failed to run nika-daemon jobs clear: {}", e),
                    }
                })?;

            if !status.success() {
                return Err(NikaError::RuntimeError {
                    reason: "Failed to clear jobs".to_string(),
                });
            }

            if !quiet {
                println!("{} Jobs cleared successfully", "✅".green());
            }
        }
    }

    Ok(())
}
