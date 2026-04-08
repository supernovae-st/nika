// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
use chrono::{Timelike, Utc};

/// Schedule management actions.
#[derive(Subcommand)]
pub enum ScheduleAction {
    /// List all schedules (dashboard)
    #[command(alias = "ls")]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Show 24h timeline view
        #[arg(long)]
        timeline: bool,
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
        ScheduleAction::List { json, timeline } => {
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

                    if timeline {
                        render_timeline(&schedules);
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
                .send(DaemonRequest::ScheduleGet { name: name.clone() })
                .await
                .map_err(sched_err)?;

            match resp {
                DaemonResponse::ScheduleDetail { schedule } => {
                    let wf = schedule["workflow"].as_str().unwrap_or("");
                    let dots = fetch_history_dots(&client, wf, 10).await;
                    render_schedule_card(&schedule, &dots);
                }
                DaemonResponse::Error { code, message } => {
                    handle_schedule_error(&client, &code, &message, &name).await;
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
                        println!("  Resume anytime: nika schedule resume {name}");
                    }
                }
                DaemonResponse::Error { code, message } => {
                    handle_schedule_error(&client, &code, &message, &name).await;
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
                        println!(
                            "{} schedule {} back in action!",
                            "✓".green().bold(),
                            name.bold()
                        );
                        println!("  View: nika schedule show {name}");
                    }
                }
                DaemonResponse::Error { code, message } => {
                    handle_schedule_error(&client, &code, &message, &name).await;
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
                    handle_schedule_error(&client, &code, &message, &name).await;
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
                        println!("  Historical runs preserved in traces.");
                    }
                }
                DaemonResponse::Error { code, message } => {
                    handle_schedule_error(&client, &code, &message, &name).await;
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

/// Handle schedule errors with did-you-mean suggestions.
async fn handle_schedule_error(
    client: &nika_daemon::DaemonClient,
    code: &str,
    message: &str,
    name: &str,
) {
    eprintln!("{} [{code}] {message}", "✗".red().bold());

    // Did-you-mean for not-found errors
    if code == "NIKA-283" && message.contains("not found") {
        if let Ok(DaemonResponse::ScheduleListResult { schedules }) = client
            .send(DaemonRequest::ScheduleList {
                enabled_only: false,
            })
            .await
        {
            let names: Vec<&str> = schedules
                .iter()
                .filter_map(|s| s["name"].as_str())
                .collect();
            if let Some(suggestion) = find_closest(name, &names) {
                eprintln!();
                eprintln!("  Did you mean {}?", suggestion.bold());
                eprintln!("    nika schedule show {suggestion}");
            } else if names.is_empty() {
                eprintln!();
                eprintln!("  No schedules exist yet. Create one:");
                eprintln!("    nika every 6h workflow.nika.yaml");
            }
        }
    }
}

/// Find the closest match using Levenshtein distance (simple impl).
fn find_closest<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .map(|c| (*c, levenshtein(input, c)))
        .filter(|(_, d)| *d <= 3 && *d > 0) // max 3 edits, exclude exact match
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

/// Simple Levenshtein distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in dp.iter_mut().enumerate().take(a.len() + 1) {
        row[0] = i;
    }
    for (j, val) in dp[0].iter_mut().enumerate().take(b.len() + 1) {
        *val = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
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

// ─── 24h Timeline ──────────────────────────────────────────────────────

/// Compute which hour slots (0-23) a cron expression fires in during one day.
///
/// Uses a fixed reference date (2026-01-05, a Monday) to get deterministic
/// results for tests. For real display we use today's date via `render_timeline`.
#[cfg(test)]
fn compute_hour_slots(cron_str: &str) -> [bool; 24] {
    compute_hour_slots_for_date(
        cron_str,
        chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
    )
}

/// Compute hour slots for a specific date.
fn compute_hour_slots_for_date(cron_str: &str, date: chrono::NaiveDate) -> [bool; 24] {
    let mut slots = [false; 24];
    let Ok(cron) = cron_str.parse::<croner::Cron>() else {
        return slots;
    };

    // Start 1 second before midnight so find_next_occurrence (exclusive) can find 00:00
    let before_day = date
        .pred_opt()
        .unwrap_or(date)
        .and_hms_opt(23, 59, 59)
        .unwrap();
    let day_end = date.and_hms_opt(23, 59, 59).unwrap();
    let from_utc =
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(before_day, chrono::Utc);
    let to_utc = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(day_end, chrono::Utc);

    let mut cursor = from_utc;
    loop {
        match cron.find_next_occurrence(&cursor, false) {
            Ok(next) if next <= to_utc => {
                let hour = next.hour() as usize;
                if hour < 24 {
                    slots[hour] = true;
                }
                cursor = next;
            }
            _ => break,
        }
    }
    slots
}

/// Render a 24h ASCII timeline of when schedules fire.
fn render_timeline(schedules: &[serde_json::Value]) {
    let today = chrono::Utc::now().date_naive();

    // Collect active schedules with their hour slots
    let mut entries: Vec<(String, [bool; 24])> = Vec::new();

    for sched in schedules {
        let name = sched["name"].as_str().unwrap_or("-");
        let cron_str = sched["cron_expr"].as_str().unwrap_or("-");
        let paused = sched["paused"].as_bool().unwrap_or(false);
        if paused {
            continue;
        }

        let slots = compute_hour_slots_for_date(cron_str, today);

        // Truncate long names for display alignment
        let display_name = if name.len() > 8 {
            format!("{}...", &name[..7])
        } else {
            name.to_string()
        };
        entries.push((display_name, slots));
    }

    if entries.is_empty() {
        println!("  No active schedules for timeline.");
        return;
    }

    // Compute max name width for alignment
    let name_w = entries
        .iter()
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(6)
        .max(6);

    // Build the hours header: " 0  1  2  ... 23"
    let hours_header: String = (0..19)
        .map(|h| format!("{:>2}", h))
        .collect::<Vec<_>>()
        .join(" ");

    // Box width: name column + 3 chars per hour (19 hours shown) + padding
    // We show hours 0-18 on the header to match the design spec
    let content_w = name_w + 2 + 19 * 3;
    let box_w = content_w + 2; // +2 for "  " prefix inside box

    println!();
    let title = " 24h Overview ";
    let dash_after = box_w.saturating_sub(title.len() + 1);
    println!(
        "  {}{}{}{}",
        "\u{256d}\u{2500}".bold(),
        title.bold(),
        "\u{2500}".repeat(dash_after).bold(),
        "\u{256e}".bold()
    );

    // Hours header
    println!("  \u{2502}  {:>name_w$}  {} \u{2502}", "", hours_header);

    // Tick line
    let ticks: String = (0..19)
        .map(|_| "\u{253c}\u{2500}\u{2500}")
        .collect::<String>()
        + "\u{253c}";
    println!("  \u{2502}  {:>name_w$}  {}\u{2502}", "", ticks);

    // Blank line
    println!("  \u{2502}{}\u{2502}", " ".repeat(box_w));

    // Per-schedule rows
    let mut hour_counts = [0u32; 24];
    for (name, slots) in &entries {
        let dots: String = (0..19)
            .map(|h| {
                if slots[h] {
                    hour_counts[h] += 1;
                    format!(" {} ", "\u{25c6}".cyan())
                } else {
                    "   ".to_string()
                }
            })
            .collect();
        // Pad to box width
        let row = format!("  {:>name_w$}{}", name.bold(), dots);
        let visible_len = name_w + 2 + 19 * 3;
        let pad = box_w.saturating_sub(visible_len);
        println!("  \u{2502}{}{}\u{2502}", row, " ".repeat(pad));
    }

    // Also count hours 19-23 for the summary
    for (_, slots) in &entries {
        for h in 19..24 {
            if slots[h] {
                hour_counts[h] += 1;
            }
        }
    }

    // Blank line
    println!("  \u{2502}{}\u{2502}", " ".repeat(box_w));

    // Overlap warnings
    let overlaps: Vec<(usize, u32)> = hour_counts
        .iter()
        .enumerate()
        .filter(|(_, &c)| c > 1)
        .map(|(h, &c)| (h, c))
        .collect();

    if !overlaps.is_empty() {
        for (h, count) in &overlaps {
            let msg = format!(
                "  {} {:02}:00: {} workflows overlap \u{2014} consider staggering",
                "\u{26a0}".yellow(),
                h,
                count
            );
            let pad = box_w.saturating_sub(msg.len() - 2); // color codes don't count
                                                           // Use a rough estimate; just pad generously
            println!("  \u{2502}{}{}\u{2502}", msg, " ".repeat(pad.min(40)));
        }
        println!("  \u{2502}{}\u{2502}", " ".repeat(box_w));
    }

    // Separator
    println!("  \u{251c}{}\u{2524}", "\u{2500}".repeat(box_w));

    // Summary line
    let total_runs: u32 = hour_counts.iter().sum();
    let peak = hour_counts
        .iter()
        .enumerate()
        .max_by_key(|(_, &c)| c)
        .map(|(h, c)| format!("peak: {:02}:00 ({} runs)", h, c))
        .unwrap_or_default();
    let summary = format!(
        "  {} schedules \u{00b7} {} runs/day \u{00b7} {}",
        entries.len(),
        total_runs,
        peak
    );
    let pad = box_w.saturating_sub(summary.len());
    println!("  \u{2502}{}{}\u{2502}", summary, " ".repeat(pad));

    // Bottom border
    println!("  \u{2570}{}\u{256f}", "\u{2500}".repeat(box_w));
    println!();
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

    // ─── Timeline tests ────────────────────────────────────────────────

    #[test]
    fn test_compute_hour_slots_daily_at_9() {
        // "0 9 * * *" fires at hour 9 every day
        let slots = compute_hour_slots("0 9 * * *");
        assert!(slots[9], "should fire at hour 9");
        assert!(!slots[8], "should NOT fire at hour 8");
        assert!(!slots[10], "should NOT fire at hour 10");
        let count: usize = slots.iter().filter(|&&s| s).count();
        assert_eq!(count, 1, "should fire exactly once per day");
    }

    #[test]
    fn test_compute_hour_slots_every_2h() {
        // "0 */2 * * *" fires every 2 hours: 0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22
        let slots = compute_hour_slots("0 */2 * * *");
        for h in (0..24).step_by(2) {
            assert!(slots[h], "should fire at hour {h}");
        }
        for h in (1..24).step_by(2) {
            assert!(!slots[h], "should NOT fire at hour {h}");
        }
        let count: usize = slots.iter().filter(|&&s| s).count();
        assert_eq!(count, 12, "should fire 12 times per day");
    }

    #[test]
    fn test_compute_hour_slots_hourly() {
        // "0 * * * *" fires every hour
        let slots = compute_hour_slots("0 * * * *");
        for (h, &slot) in slots.iter().enumerate() {
            assert!(slot, "should fire at hour {h}");
        }
    }

    #[test]
    fn test_compute_hour_slots_invalid_cron() {
        let slots = compute_hour_slots("not-a-cron");
        let count: usize = slots.iter().filter(|&&s| s).count();
        assert_eq!(count, 0, "invalid cron should produce no slots");
    }

    #[test]
    fn test_compute_hour_slots_weekday_only() {
        // "0 9 * * 1-5" fires at 9 on weekdays only
        // 2026-01-05 is a Monday, so it should fire
        let monday = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let slots = compute_hour_slots_for_date("0 9 * * 1-5", monday);
        assert!(slots[9], "should fire at 9 on Monday");

        // 2026-01-04 is a Sunday, should NOT fire
        let sunday = chrono::NaiveDate::from_ymd_opt(2026, 1, 4).unwrap();
        let slots = compute_hour_slots_for_date("0 9 * * 1-5", sunday);
        let count: usize = slots.iter().filter(|&&s| s).count();
        assert_eq!(count, 0, "should not fire on Sunday");
    }

    #[test]
    fn test_compute_hour_slots_multiple_fires_per_hour() {
        // "*/15 9 * * *" fires at 9:00, 9:15, 9:30, 9:45 — but we only track hour 9
        let slots = compute_hour_slots("*/15 9 * * *");
        assert!(slots[9], "should fire at hour 9");
        let count: usize = slots.iter().filter(|&&s| s).count();
        assert_eq!(count, 1, "should only mark hour 9 (not multiple times)");
    }

    #[test]
    fn test_render_timeline_no_panic_with_sample_data() {
        let schedules = vec![
            serde_json::json!({
                "name": "daily-report",
                "cron_expr": "0 9 * * *",
                "paused": false,
                "workflow": "report.nika.yaml"
            }),
            serde_json::json!({
                "name": "hourly-check",
                "cron_expr": "0 * * * *",
                "paused": false,
                "workflow": "check.nika.yaml"
            }),
        ];
        // Should not panic
        render_timeline(&schedules);
    }

    #[test]
    fn test_render_timeline_skips_paused() {
        let schedules = vec![
            serde_json::json!({
                "name": "active-one",
                "cron_expr": "0 9 * * *",
                "paused": false,
                "workflow": "active.nika.yaml"
            }),
            serde_json::json!({
                "name": "paused-one",
                "cron_expr": "0 * * * *",
                "paused": true,
                "workflow": "paused.nika.yaml"
            }),
        ];
        // Should not panic, paused schedule excluded from view
        render_timeline(&schedules);
    }

    #[test]
    fn test_render_timeline_empty() {
        render_timeline(&[]);
    }

    #[test]
    fn test_render_timeline_all_paused() {
        let schedules = vec![serde_json::json!({
            "name": "paused",
            "cron_expr": "0 9 * * *",
            "paused": true,
            "workflow": "w.nika.yaml"
        })];
        render_timeline(&schedules);
    }

    #[test]
    fn test_render_timeline_long_name_truncated() {
        let schedules = vec![serde_json::json!({
            "name": "very-long-schedule-name-here",
            "cron_expr": "0 12 * * *",
            "paused": false,
            "workflow": "w.nika.yaml"
        })];
        // Should not panic; name should be truncated in display
        render_timeline(&schedules);
    }

    // ─── Did-you-mean tests ────────────────────────────────────────────

    #[test]
    fn test_levenshtein_exact() {
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn test_levenshtein_one_edit() {
        assert_eq!(levenshtein("abc", "adc"), 1); // substitution
        assert_eq!(levenshtein("abc", "abcd"), 1); // insertion
        assert_eq!(levenshtein("abc", "ab"), 1); // deletion
    }

    #[test]
    fn test_levenshtein_empty() {
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", ""), 3);
    }

    #[test]
    fn test_find_closest_typo() {
        let candidates = vec!["daily-report", "hourly-check", "weekly-sync"];
        assert_eq!(
            find_closest("daily-repost", &candidates),
            Some("daily-report")
        );
    }

    #[test]
    fn test_find_closest_no_match() {
        let candidates = vec!["daily-report"];
        assert_eq!(find_closest("completely-different", &candidates), None);
    }

    #[test]
    fn test_find_closest_empty() {
        let candidates: Vec<&str> = vec![];
        assert_eq!(find_closest("anything", &candidates), None);
    }
}
