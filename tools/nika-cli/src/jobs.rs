// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika job` subcommand handler.
//!
//! Manages workflow jobs via the daemon:
//! - `nika job submit <workflow>` — submit a new job
//! - `nika job list` — list all jobs
//! - `nika job status <id>` — show job details
//! - `nika job cancel <id>` — cancel a running job
//! - `nika job retry <id>` — retry a failed job
//! - `nika job history <id>` — show job history

use clap::Subcommand;
use colored::Colorize;
use std::time::Duration;

use nika_daemon::{daemon_socket_path, DaemonClient, DaemonRequest, DaemonResponse};
use nika_engine::error::NikaError;

/// Job management actions.
#[derive(Subcommand)]
pub enum JobAction {
    /// Submit a workflow as a background job
    Submit {
        /// Path to .nika.yaml workflow file
        workflow: String,

        /// Job name (optional, for display)
        #[arg(long)]
        name: Option<String>,

        /// Cron schedule (e.g., "0 * * * *" for hourly)
        #[arg(long)]
        cron: Option<String>,

        /// Maximum retries on failure
        #[arg(long, default_value = "0")]
        max_retries: u32,
    },

    /// List all jobs
    List {
        /// Filter by state: pending, running, completed, failed, cancelled
        #[arg(long)]
        state: Option<String>,
    },

    /// Show job details
    Status {
        /// Job ID
        id: String,
    },

    /// Cancel a running job
    Cancel {
        /// Job ID
        id: String,
    },

    /// Retry a failed job
    Retry {
        /// Job ID
        id: String,
    },

    /// Show job history events
    History {
        /// Job ID
        id: String,
    },
}

pub async fn handle_job_command(action: JobAction, quiet: bool) -> Result<(), NikaError> {
    let client = DaemonClient::new(daemon_socket_path()).with_timeout(Duration::from_secs(10));

    if !client.socket_exists() {
        return Err(NikaError::Execution(
            "Daemon not running. Start with: nika daemon start".into(),
        ));
    }

    match action {
        JobAction::Submit {
            workflow,
            name,
            cron,
            max_retries,
        } => {
            let resp = client
                .send(DaemonRequest::JobSubmit {
                    workflow: workflow.clone(),
                    name: name.clone(),
                    args: None,
                    cron: cron.clone(),
                    max_retries: Some(max_retries),
                })
                .await
                .map_err(job_err)?;

            match resp {
                DaemonResponse::JobCreated { id } => {
                    if !quiet {
                        println!("{} job submitted", "✓".green().bold());
                        println!("  id:       {}", id);
                        println!("  workflow: {}", workflow);
                        if let Some(name) = name {
                            println!("  name:     {}", name);
                        }
                        if let Some(cron) = cron {
                            println!("  cron:     {}", cron);
                        }
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        JobAction::List { state } => {
            let resp = client
                .send(DaemonRequest::JobList { state })
                .await
                .map_err(job_err)?;

            match resp {
                DaemonResponse::JobList { jobs } => {
                    if jobs.is_empty() {
                        println!("No jobs found");
                        return Ok(());
                    }

                    println!(
                        "{:<38} {:<12} {:<20} {}",
                        "ID".bold(),
                        "STATE".bold(),
                        "WORKFLOW".bold(),
                        "CREATED".bold()
                    );

                    for job in &jobs {
                        let id = job["id"].as_str().unwrap_or("-");
                        let state = job["state"].as_str().unwrap_or("-");
                        let workflow = job["workflow"].as_str().unwrap_or("-");
                        let created = job["created_at"].as_str().unwrap_or("-");

                        let state_colored = match state {
                            "running" => state.cyan().to_string(),
                            "completed" => state.green().to_string(),
                            "failed" => state.red().to_string(),
                            "cancelled" => state.yellow().to_string(),
                            _ => state.dimmed().to_string(),
                        };

                        // Truncate ID to first 8 chars for display
                        let short_id = if id.len() > 8 { &id[..8] } else { id };
                        let short_wf = if workflow.len() > 20 {
                            &workflow[workflow.len() - 20..]
                        } else {
                            workflow
                        };
                        let short_created = if created.len() > 19 {
                            &created[..19]
                        } else {
                            created
                        };

                        println!(
                            "{:<38} {:<12} {:<20} {}",
                            short_id.dimmed(),
                            state_colored,
                            short_wf,
                            short_created.dimmed()
                        );
                    }

                    println!("\n{} job(s)", jobs.len());
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        JobAction::Status { id } => {
            let resp = client
                .send(DaemonRequest::JobStatus { id })
                .await
                .map_err(job_err)?;

            match resp {
                DaemonResponse::JobDetail { job } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&job)
                            .unwrap_or_else(|e| format!("(serialization error: {e})"))
                    );
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        JobAction::Cancel { id } => {
            let resp = client
                .send(DaemonRequest::JobCancel { id: id.clone() })
                .await
                .map_err(job_err)?;

            match resp {
                DaemonResponse::Ok => {
                    if !quiet {
                        println!("{} job {} cancelled", "✓".green().bold(), id);
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        JobAction::Retry { id } => {
            let resp = client
                .send(DaemonRequest::JobRetry { id: id.clone() })
                .await
                .map_err(job_err)?;

            match resp {
                DaemonResponse::JobCreated { id: new_id } => {
                    if !quiet {
                        println!("{} job {} retried → {}", "✓".green().bold(), id, new_id);
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        JobAction::History { id } => {
            let resp = client
                .send(DaemonRequest::JobHistory { id })
                .await
                .map_err(job_err)?;

            match resp {
                DaemonResponse::JobHistoryList { events } => {
                    if events.is_empty() {
                        println!("No history events");
                        return Ok(());
                    }

                    for event in &events {
                        let ts = event["timestamp"].as_str().unwrap_or("-");
                        let evt = event["event"].as_str().unwrap_or("-");
                        let details = event["details"].as_str().unwrap_or("");

                        let short_ts = if ts.len() > 19 { &ts[..19] } else { ts };

                        let evt_colored = match evt {
                            "submitted" => evt.cyan().to_string(),
                            "started" => evt.blue().to_string(),
                            "completed" => evt.green().to_string(),
                            "failed" => evt.red().to_string(),
                            "cancelled" => evt.yellow().to_string(),
                            "retried" => evt.magenta().to_string(),
                            _ => evt.to_string(),
                        };

                        if details.is_empty() {
                            println!("  {} {}", short_ts.dimmed(), evt_colored);
                        } else {
                            println!(
                                "  {} {} — {}",
                                short_ts.dimmed(),
                                evt_colored,
                                details.dimmed()
                            );
                        }
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

fn job_err(e: nika_daemon::DaemonError) -> NikaError {
    NikaError::Execution(format!("job: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn job_list_no_daemon_returns_error() {
        // Isolate from real daemon by pointing NIKA_HOME to a tempdir
        let dir = tempfile::tempdir().unwrap();
        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        // When daemon is not running, job commands should return Err, not Ok(())
        let result = handle_job_command(JobAction::List { state: None }, true).await;
        assert!(
            result.is_err(),
            "job list should return Err when daemon is not running"
        );

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }
}
