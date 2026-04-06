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

use crate::every::format_run_estimate;

// Re-export for schedule card display
use chrono::Utc;

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
                        "\n  {:<20} {:<8} {:<20} {:<14} {:<6} {:<16} {}",
                        "NAME".bold(),
                        "STATUS".bold(),
                        "CRON".bold(),
                        "TIMEZONE".bold(),
                        "RUNS".bold(),
                        "FREQ".bold(),
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

                        let freq = crate::every::runs_per_day(cron)
                            .map(|rpd| {
                                if rpd >= 1.0 {
                                    format!("{:.0}/day", rpd)
                                } else {
                                    format!("{:.1}/week", rpd * 7.0)
                                }
                            })
                            .unwrap_or_else(|| "-".into());

                        // History dots from recent runs
                        let dots = fetch_history_dots(&client, workflow, 10).await;
                        let dots_str = if dots.is_empty() {
                            String::new()
                        } else {
                            let summary = history_summary(&dots);
                            format!("  {} {}", render_history_dots(&dots), summary.dimmed())
                        };

                        println!(
                            "  {icon} {:<19} {:<8} {:<20} {:<14} {:<6} {:<16} {}",
                            name, status, cron, tz, run_count, freq, workflow,
                        );
                        if run_count > 0 && !dots_str.is_empty() {
                            println!("   {dots_str}");
                        }
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
                    let wf = schedule["workflow"].as_str().unwrap_or("");
                    let dots = fetch_history_dots(&client, wf, 10).await;
                    render_schedule_card(&schedule, &dots);
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
            // First, get the schedule to find its workflow
            let resp = client
                .send(DaemonRequest::ScheduleGet { name: name.clone() })
                .await
                .map_err(sched_err)?;

            let workflow = match resp {
                DaemonResponse::ScheduleDetail { ref schedule } => {
                    schedule["workflow"].as_str().unwrap_or("").to_string()
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                    return Ok(());
                }
                _ => {
                    eprintln!("{} unexpected response", "✗".red().bold());
                    return Ok(());
                }
            };

            if workflow.is_empty() {
                eprintln!(
                    "{} schedule '{}' has no workflow configured",
                    "✗".red().bold(),
                    name
                );
                return Ok(());
            }

            // Submit a job for this workflow
            let resp = client
                .send(DaemonRequest::JobSubmit {
                    workflow: workflow.clone(),
                    name: Some(format!("{name}-manual")),
                    args: None,
                    cron: None,
                    max_retries: None,
                })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::JobCreated { id } => {
                    if !quiet {
                        println!(
                            "{} triggered {} → job {}",
                            "✓".green().bold(),
                            name.bold(),
                            id.dimmed()
                        );
                        println!("  Workflow: {workflow}");
                        println!("  Track:   nika trace show {id}");
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
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

/// Fetch recent run statuses for a workflow via GetWorkflowHistory.
async fn fetch_history_dots(
    client: &nika_daemon::DaemonClient,
    workflow: &str,
    limit: usize,
) -> Vec<String> {
    let resp = client
        .send(DaemonRequest::GetWorkflowHistory {
            workflow: workflow.to_string(),
        })
        .await;
    match resp {
        Ok(DaemonResponse::WorkflowHistoryResult { runs }) => {
            let start = runs.len().saturating_sub(limit);
            runs[start..].iter().map(|r| r.state.clone()).collect()
        }
        _ => vec![],
    }
}

/// Render job states as colored dot characters.
pub fn render_history_dots(states: &[String]) -> String {
    states
        .iter()
        .map(|s| match s.as_str() {
            "completed" => "✓".green().to_string(),
            "failed" => "✗".red().to_string(),
            "cancelled" => "─".dimmed().to_string(),
            "running" => "◆".cyan().to_string(),
            "pending" => "○".dimmed().to_string(),
            _ => "?".dimmed().to_string(),
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Summarize pass/fail from states.
pub fn history_summary(states: &[String]) -> String {
    let pass = states.iter().filter(|s| *s == "completed").count();
    let total = states.len();
    format!("{pass}/{total}")
}

/// Render a detailed schedule card with box-drawing characters.
fn render_schedule_card(sched: &serde_json::Value, history: &[String]) {
    let name = sched["name"].as_str().unwrap_or("-");
    let workflow = sched["workflow"].as_str().unwrap_or("-");
    let cron = sched["cron_expr"].as_str().unwrap_or("-");
    let tz = sched["timezone"].as_str().unwrap_or("UTC");
    let paused = sched["paused"].as_bool().unwrap_or(false);
    let source = sched["source"].as_str().unwrap_or("cli");
    let overlap = sched["overlap"].as_str().unwrap_or("skip");
    let run_count = sched["run_count"].as_u64().unwrap_or(0);
    let last_run = sched["last_run_at"].as_str().unwrap_or("-");

    let status = if paused {
        "⏸ Paused".yellow().to_string()
    } else {
        "● Active".green().to_string()
    };

    // Try to describe cron in human-readable form
    let human = hron::Schedule::explain_cron(cron).unwrap_or_else(|_| cron.to_string());

    println!();
    println!(
        "  ╭─ {} ─────────────────────────────────────────────╮",
        name.bold()
    );
    println!("  │                                                       │");
    println!("  │  Workflow   {:<42} │", workflow);
    println!("  │  Schedule   {:<42} │", human);
    println!("  │  Cron       {:<42} │", cron);
    println!("  │  Timezone   {:<42} │", tz);
    println!("  │  Status     {:<42} │", status);
    println!("  │  Source     {:<42} │", source);
    println!("  │  Overlap    {:<42} │", overlap);
    println!("  │  Runs       {:<42} │", run_count);
    println!("  │  Last run   {:<42} │", last_run);
    if let Some(estimate) = format_run_estimate(cron) {
        println!("  │  Frequency  {:<42} │", estimate);
    }
    if !history.is_empty() {
        let dots_line = format!(
            "{}  {}",
            render_history_dots(history),
            history_summary(history)
        );
        println!("  │  History    {:<42} │", dots_line);
    }
    println!("  │                                                       │");

    // Next 5 runs
    if let Ok(parsed_cron) = cron.parse::<croner::Cron>() {
        let now = Utc::now();
        let mut next_runs: Vec<chrono::DateTime<Utc>> = Vec::new();
        let mut from = now;
        for _ in 0..5 {
            match parsed_cron.find_next_occurrence(&from, false) {
                Ok(next) => {
                    next_runs.push(next);
                    from = next;
                }
                Err(_) => break,
            }
        }
        if !next_runs.is_empty() {
            println!("  ├─────────────────────────────────────────────────────┤");
            println!("  │  {:<53} │", "Next 5 runs".bold());
            for (i, run) in next_runs.iter().enumerate() {
                let formatted = run.format("%a %d %b %H:%M").to_string();
                let relative = humanize_duration((*run - now).num_seconds());
                println!("  │   {}.  {:<22} {:<24} │", i + 1, formatted, relative);
            }
            println!("  │                                                       │");
        }
    }

    println!("  ╰─────────────────────────────────────────────────────╯");
    println!();
    println!("  Actions:");
    println!("    nika schedule trigger {name}     Run now");
    if paused {
        println!("    nika schedule resume {name}      Resume");
    } else {
        println!("    nika schedule pause {name}       Pause");
    }
    println!("    nika schedule remove {name}      Delete");
    println!();
}

/// Convert seconds to a human-readable relative time string.
fn humanize_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("in {seconds}s")
    } else if seconds < 3600 {
        format!("in {}m", seconds / 60)
    } else if seconds < 86400 {
        let h = seconds / 3600;
        let m = (seconds % 3600) / 60;
        if m == 0 {
            format!("in {h}h")
        } else {
            format!("in {h}h {m}m")
        }
    } else {
        let d = seconds / 86400;
        let h = (seconds % 86400) / 3600;
        if h == 0 {
            format!("in {d}d")
        } else {
            format!("in {d}d {h}h")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_humanize_duration() {
        assert_eq!(humanize_duration(30), "in 30s");
        assert_eq!(humanize_duration(90), "in 1m");
        assert_eq!(humanize_duration(3600), "in 1h");
        assert_eq!(humanize_duration(5400), "in 1h 30m");
        assert_eq!(humanize_duration(90000), "in 1d 1h");
    }

    #[test]
    fn test_render_history_dots_mixed() {
        let states: Vec<String> = vec![
            "completed".into(),
            "completed".into(),
            "failed".into(),
            "completed".into(),
        ];
        let dots = render_history_dots(&states);
        assert!(dots.contains('✓'), "should contain ✓");
        assert!(dots.contains('✗'), "should contain ✗");
    }

    #[test]
    fn test_render_history_dots_empty() {
        let dots = render_history_dots(&[]);
        assert!(dots.is_empty());
    }

    #[test]
    fn test_history_summary() {
        let states: Vec<String> = vec![
            "completed".into(),
            "failed".into(),
            "completed".into(),
            "completed".into(),
        ];
        assert_eq!(history_summary(&states), "3/4");
    }

    #[test]
    fn test_history_summary_all_pass() {
        let states: Vec<String> = vec!["completed".into(), "completed".into()];
        assert_eq!(history_summary(&states), "2/2");
    }
}
