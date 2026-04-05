//! `nika schedule` subcommand handler.
//!
//! Manages cron schedules via the daemon:
//! - `nika schedule list` — dashboard
//! - `nika schedule show <name>` — detail card
//! - `nika schedule pause <name>` — pause
//! - `nika schedule resume <name>` — resume
//! - `nika schedule trigger <name>` — run now
//! - `nika schedule remove <name>` — delete

use clap::Subcommand;
use colored::Colorize;
use std::time::Duration;

use nika_daemon::{daemon_socket_path, DaemonClient, DaemonRequest, DaemonResponse};
use nika_engine::error::NikaError;

/// Schedule management actions.
#[derive(Subcommand)]
pub enum ScheduleAction {
    /// List all schedules (dashboard)
    #[command(alias = "ls")]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show schedule details
    Show {
        /// Schedule name
        name: String,
    },

    /// Pause a schedule
    Pause {
        /// Schedule name
        name: String,
    },

    /// Resume a paused schedule
    Resume {
        /// Schedule name
        name: String,
    },

    /// Trigger an immediate run
    Trigger {
        /// Schedule name
        name: String,
    },

    /// Remove a schedule
    #[command(alias = "rm")]
    Remove {
        /// Schedule name
        name: String,
    },
}

pub async fn handle_schedule_command(action: ScheduleAction, quiet: bool) -> Result<(), NikaError> {
    let client = DaemonClient::new(daemon_socket_path()).with_timeout(Duration::from_secs(10));

    if !client.socket_exists() {
        return Err(NikaError::Execution(
            "Daemon not running. Start with: nika daemon start".into(),
        ));
    }

    match action {
        ScheduleAction::List { json } => {
            let resp = client
                .send(DaemonRequest::ScheduleList {
                    enabled_only: false,
                })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::ScheduleListResult { schedules } => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&schedules)
                                .unwrap_or_else(|_| "[]".into())
                        );
                        return Ok(());
                    }

                    if schedules.is_empty() {
                        if !quiet {
                            println!("  No schedules yet.\n");
                            println!("  Get started:");
                            println!("    nika every 6h report.nika.yaml");
                            println!("\n  Or explore interactively:");
                            println!("    nika every");
                        }
                        return Ok(());
                    }

                    println!(
                        "\n  {:<20} {:<8} {:<20} {:<14} {:<6} {}",
                        "NAME".bold(),
                        "STATUS".bold(),
                        "CRON".bold(),
                        "TIMEZONE".bold(),
                        "RUNS".bold(),
                        "WORKFLOW".bold(),
                    );

                    for sched in &schedules {
                        let name = sched["name"].as_str().unwrap_or("-");
                        let paused = sched["paused"].as_bool().unwrap_or(false);
                        let cron = sched["cron_expr"].as_str().unwrap_or("-");
                        let tz = sched["timezone"].as_str().unwrap_or("UTC");
                        let workflow = sched["workflow"].as_str().unwrap_or("-");
                        let run_count = sched["run_count"].as_u64().unwrap_or(0);

                        let status = if paused {
                            "paused".yellow().to_string()
                        } else {
                            "active".green().to_string()
                        };
                        let icon = if paused { "⏸" } else { "●" };

                        println!(
                            "  {icon} {:<19} {:<8} {:<20} {:<14} {:<6} {}",
                            name, status, cron, tz, run_count, workflow,
                        );
                    }
                    println!("\n  {} schedule(s)\n", schedules.len());
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        ScheduleAction::Show { name } => {
            let resp = client
                .send(DaemonRequest::ScheduleGet { name })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::ScheduleDetail { schedule } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&schedule)
                            .unwrap_or_else(|e| format!("(serialize error: {e})"))
                    );
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        ScheduleAction::Pause { name } => {
            let resp = client
                .send(DaemonRequest::SchedulePause { name: name.clone() })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::Ok => {
                    if !quiet {
                        println!("{} schedule {} paused", "✓".green().bold(), name.bold());
                        println!("  Resume: nika schedule resume {}", name);
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        ScheduleAction::Resume { name } => {
            let resp = client
                .send(DaemonRequest::ScheduleResume { name: name.clone() })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::Ok => {
                    if !quiet {
                        println!("{} schedule {} resumed", "✓".green().bold(), name.bold());
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        ScheduleAction::Trigger { name } => {
            // For now, just show a message. Full implementation would submit a job.
            if !quiet {
                println!("{} triggering {} (manual run)", "⠋".cyan(), name.bold());
                println!("  TODO: submit job from schedule workflow");
            }
        }

        ScheduleAction::Remove { name } => {
            let resp = client
                .send(DaemonRequest::ScheduleDelete { name: name.clone() })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::Ok => {
                    if !quiet {
                        println!("{} schedule {} removed", "✓".green().bold(), name.bold());
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }
    }

    Ok(())
}

fn sched_err(e: nika_daemon::DaemonError) -> NikaError {
    NikaError::Execution(format!("schedule: {e}"))
}
