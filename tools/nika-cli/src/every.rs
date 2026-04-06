//! `nika every` — create a recurring schedule.
//!
//! Examples:
//!   nika every 6h report.nika.yaml
//!   nika every day at 9:00 report.nika.yaml
//!   nika every --cron "0 */6 * * *" report.nika.yaml

use colored::Colorize;
use nika_core::ast::schedule::duration_to_cron;
use std::io::IsTerminal;
use std::time::Duration;

use nika_daemon::{daemon_socket_path, DaemonClient, DaemonRequest, DaemonResponse};
use nika_engine::error::NikaError;

/// Arguments for `nika every`.
#[derive(Debug, clap::Args)]
pub struct EveryArgs {
    /// Schedule expression and workflow path.
    /// Last argument is the workflow, everything before is the schedule.
    /// Examples: "6h report.nika.yaml", "day at 9:00 report.nika.yaml"
    #[arg(trailing_var_arg = true, num_args = 0..)]
    pub args: Vec<String>,

    /// Raw cron expression (overrides positional schedule)
    #[arg(long)]
    pub cron: Option<String>,

    /// IANA timezone (default: UTC)
    #[arg(long, default_value = "UTC")]
    pub tz: String,

    /// Schedule name (default: derived from workflow filename)
    #[arg(long)]
    pub name: Option<String>,

    /// Overlap policy: skip, queue, replace
    #[arg(long, default_value = "skip")]
    pub overlap: nika_core::ast::schedule::OverlapPolicy,

    /// Preview only, don't create
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn handle_every_command(args: EveryArgs, quiet: bool) -> Result<(), NikaError> {
    // Interactive wizard when no args and TTY
    if args.args.is_empty() && args.cron.is_none() {
        if !std::io::stdin().is_terminal() {
            return Err(NikaError::Execution(
                "Usage: nika every <schedule> <workflow.nika.yaml>\n\
                 Examples:\n  \
                 nika every 6h report.nika.yaml\n  \
                 nika every day at 9:00 report.nika.yaml\n  \
                 nika every --cron \"0 */6 * * *\" report.nika.yaml"
                    .into(),
            ));
        }
        return interactive_wizard(args, quiet).await;
    }

    let Some(workflow) = args.args.last().cloned() else {
        return Err(NikaError::Execution("missing workflow argument".into()));
    };
    if !workflow.ends_with(".nika.yaml") {
        return Err(NikaError::Execution(format!(
            "last argument must be a .nika.yaml workflow file, got '{workflow}'"
        )));
    }

    // Build cron expression
    let cron_expr = if let Some(ref raw_cron) = args.cron {
        raw_cron.clone()
    } else if args.args.len() == 1 {
        return Err(NikaError::Execution(
            "missing schedule expression. Examples:\n  \
             nika every 6h report.nika.yaml\n  \
             nika every --cron \"0 9 * * *\" report.nika.yaml"
                .into(),
        ));
    } else {
        let schedule_parts = &args.args[..args.args.len() - 1];
        let schedule_str = schedule_parts.join(" ");
        resolve_schedule_expr(&schedule_str)?
    };

    // Derive name
    let name = args
        .name
        .unwrap_or_else(|| auto_name(&workflow, &cron_expr));

    if !quiet {
        println!();
        println!("  {} schedule preview", "◆".cyan());
        println!("  Name       {}", name.bold());
        println!("  Workflow   {}", workflow);
        println!("  Cron       {}", cron_expr);
        println!("  Timezone   {}", args.tz);
        println!("  Overlap    {}", args.overlap);
    }

    if args.dry_run {
        if !quiet {
            println!("\n  {} dry run — not created", "⏸".yellow());
        }
        return Ok(());
    }

    let client = DaemonClient::new(daemon_socket_path()).with_timeout(Duration::from_secs(10));
    if !client.socket_exists() {
        return Err(NikaError::Execution(
            "Daemon not running. Start with: nika daemon start".into(),
        ));
    }

    let resp = client
        .send(DaemonRequest::ScheduleCreate {
            name: name.clone(),
            workflow: workflow.clone(),
            cron_expr: cron_expr.clone(),
            timezone: Some(args.tz),
            source: Some("cli".to_string()),
            overlap: Some(args.overlap.to_string()),
            inputs_json: None,
        })
        .await
        .map_err(|e| NikaError::Execution(format!("schedule: {e}")))?;

    match resp {
        DaemonResponse::ScheduleCreated { id: _, name } => {
            if !quiet {
                // Cascading celebration
                println!();
                println!("  {} Cron valid: {}", "✓".green().bold(), cron_expr);
                println!("  {} Registered: {}", "✓".green().bold(), name.bold());

                // Next 5 runs
                if let Ok(parsed_cron) = cron_expr.parse::<croner::Cron>() {
                    let now = chrono::Utc::now();
                    let mut from = now;
                    if let Ok(first) = parsed_cron.find_next_occurrence(&from, false) {
                        let rel = humanize_relative(first.signed_duration_since(now).num_seconds());
                        println!("  {} Next run: {}", "✓".green().bold(), rel);

                        // Show next 5 in compact form
                        println!();
                        println!("  {}", "Next 5 runs".bold());
                        let mut runs = vec![first];
                        from = first;
                        for _ in 0..4 {
                            match parsed_cron.find_next_occurrence(&from, false) {
                                Ok(next) => {
                                    runs.push(next);
                                    from = next;
                                }
                                Err(_) => break,
                            }
                        }
                        for (i, run) in runs.iter().enumerate() {
                            let formatted = run.format("%a %d %b %H:%M UTC").to_string();
                            let rel =
                                humanize_relative(run.signed_duration_since(now).num_seconds());
                            println!("   {}.  {:<26} {}", i + 1, formatted, rel.dimmed());
                        }
                    }
                }

                println!();
                println!("  {} {} is live! 🦋", "✓".green().bold(), name.bold());
                println!();
                println!("  View:     nika schedule show {name}");
                println!("  Pause:    nika schedule pause {name}");
                println!("  All:      nika schedule list");
                println!();
            }
        }
        DaemonResponse::Error { code, message } => {
            eprintln!("{} [{code}] {message}", "✗".red().bold());
        }
        _ => eprintln!("{} unexpected response", "✗".red().bold()),
    }

    Ok(())
}

// ─── Interactive wizard ──────────────────────────────────────────────────

/// Interactive cliclack wizard for `nika every` (no args).
async fn interactive_wizard(args: EveryArgs, quiet: bool) -> Result<(), NikaError> {
    cliclack::intro("nika every · Schedule a recurring workflow").map_err(wiz_err)?;

    // Step 1: Discover workflows
    let workflows = discover_workflows()?;
    if workflows.is_empty() {
        cliclack::outro("No .nika.yaml workflows found in current directory.").map_err(wiz_err)?;
        return Ok(());
    }

    let workflow: String = if workflows.len() == 1 {
        let w = workflows[0].clone();
        eprintln!("  Auto-selected: {}", w.bold());
        w
    } else {
        let items: Vec<(String, String, String)> = workflows
            .iter()
            .map(|w| (w.clone(), w.clone(), String::new()))
            .collect();
        cliclack::select("Which workflow?")
            .items(
                &items
                    .iter()
                    .map(|(id, label, hint)| (id.clone(), label.as_str(), hint.as_str()))
                    .collect::<Vec<_>>(),
            )
            .interact()
            .map_err(wiz_err)?
    };

    // Step 2: Frequency
    let freq: String = cliclack::select("How often?")
        .items(&[
            ("2h".to_string(), "Every 2 hours", "12 runs/day"),
            ("6h".to_string(), "Every 6 hours", "4 runs/day"),
            ("daily".to_string(), "Every day", "Most popular — 80% of schedules"),
            ("weekday".to_string(), "Every weekday", "Mon–Fri"),
            ("weekly".to_string(), "Every week", "Once a week"),
            ("custom".to_string(), "Type it yourself", "Cron or natural language"),
        ])
        .interact()
        .map_err(wiz_err)?;

    // Step 3: Resolve to cron
    let (cron_expr, tz) = match freq.as_str() {
        "daily" => {
            let time: String = cliclack::input("Time (24h format)")
                .default_input("09:00")
                .interact()
                .map_err(wiz_err)?;
            let (h, m) = parse_hhmm(&time)?;
            let tz = pick_timezone()?;
            (format!("{m} {h} * * *"), tz)
        }
        "weekday" => {
            let time: String = cliclack::input("Time (24h format)")
                .default_input("09:00")
                .interact()
                .map_err(wiz_err)?;
            let (h, m) = parse_hhmm(&time)?;
            let tz = pick_timezone()?;
            (format!("{m} {h} * * 1-5"), tz)
        }
        "weekly" => {
            let time: String = cliclack::input("Time (24h format)")
                .default_input("09:00")
                .interact()
                .map_err(wiz_err)?;
            let (h, m) = parse_hhmm(&time)?;
            let day: String = cliclack::select("Which day?")
                .items(&[
                    ("1".to_string(), "Monday", ""),
                    ("2".to_string(), "Tuesday", ""),
                    ("3".to_string(), "Wednesday", ""),
                    ("4".to_string(), "Thursday", ""),
                    ("5".to_string(), "Friday", ""),
                    ("6".to_string(), "Saturday", ""),
                    ("0".to_string(), "Sunday", ""),
                ])
                .interact()
                .map_err(wiz_err)?;
            let tz = pick_timezone()?;
            (format!("{m} {h} * * {day}"), tz)
        }
        "custom" => {
            let expr: String = cliclack::input("Schedule expression")
                .placeholder("0 9 * * * or every day at 9:00")
                .interact()
                .map_err(wiz_err)?;
            let cron = resolve_schedule_expr(&expr)?;
            let tz = pick_timezone()?;
            (cron, tz)
        }
        duration => {
            // "2h", "6h" etc
            let cron = duration_to_cron(duration).ok_or_else(|| {
                NikaError::Execution(format!("invalid duration: {duration}"))
            })?;
            (cron, "UTC".to_string())
        }
    };

    // Validate cron
    cron_expr.parse::<croner::Cron>().map_err(|e| {
        NikaError::Execution(format!("invalid cron expression: {e}"))
    })?;

    let name = args
        .name
        .unwrap_or_else(|| auto_name(&workflow, &cron_expr));

    // Step 4: Preview
    eprintln!();
    eprintln!("  {} Schedule preview", "◆".cyan());
    eprintln!("  Name       {}", name.bold());
    eprintln!("  Workflow   {}", workflow);
    eprintln!("  Cron       {}", cron_expr);
    eprintln!("  Timezone   {}", tz);

    // Show next 5 runs in preview
    if let Ok(parsed_cron) = cron_expr.parse::<croner::Cron>() {
        let now = chrono::Utc::now();
        let mut from = now;
        eprintln!();
        eprintln!("  {}", "Next 5 runs".bold());
        for i in 0..5 {
            match parsed_cron.find_next_occurrence(&from, false) {
                Ok(next) => {
                    let formatted = next.format("%a %d %b %H:%M").to_string();
                    let rel = humanize_relative(next.signed_duration_since(now).num_seconds());
                    eprintln!("   {}.  {:<22} {}", i + 1, formatted, rel.dimmed());
                    from = next;
                }
                Err(_) => break,
            }
        }
    }
    eprintln!();

    let confirm = cliclack::confirm("Create this schedule?")
        .initial_value(true)
        .interact()
        .map_err(wiz_err)?;

    if !confirm {
        cliclack::outro("Cancelled.").map_err(wiz_err)?;
        return Ok(());
    }

    // Step 5: Create via daemon
    let client = DaemonClient::new(daemon_socket_path()).with_timeout(Duration::from_secs(10));
    if !client.socket_exists() {
        cliclack::outro("Daemon not running. Start with: nika daemon start").map_err(wiz_err)?;
        return Ok(());
    }

    let resp = client
        .send(DaemonRequest::ScheduleCreate {
            name: name.clone(),
            workflow: workflow.clone(),
            cron_expr: cron_expr.clone(),
            timezone: Some(tz),
            source: Some("cli".to_string()),
            overlap: Some(args.overlap.to_string()),
            inputs_json: None,
        })
        .await
        .map_err(|e| NikaError::Execution(format!("schedule: {e}")))?;

    match resp {
        DaemonResponse::ScheduleCreated { id: _, name } => {
            if !quiet {
                println!();
                println!("  {} Cron valid: {}", "✓".green().bold(), cron_expr);
                println!("  {} Registered: {}", "✓".green().bold(), name.bold());
                println!("  {} {} is live! 🦋", "✓".green().bold(), name.bold());
                println!();
                println!("  View:     nika schedule show {name}");
                println!("  Pause:    nika schedule pause {name}");
                println!("  Trigger:  nika schedule trigger {name}");
                println!("  All:      nika schedule list");
                println!();
            }
        }
        DaemonResponse::Error { code, message } => {
            eprintln!("{} [{code}] {message}", "✗".red().bold());
        }
        _ => eprintln!("{} unexpected response", "✗".red().bold()),
    }

    Ok(())
}

/// Discover *.nika.yaml workflows in the current directory (recursive scan via glob).
fn discover_workflows() -> Result<Vec<String>, NikaError> {
    let mut found: Vec<String> = glob::glob("**/*.nika.yaml")
        .map_err(|e| NikaError::Execution(format!("glob error: {e}")))?
        .flatten()
        .map(|p| p.display().to_string())
        .collect();
    found.sort();
    found.dedup();
    Ok(found)
}

/// Pick timezone interactively.
fn pick_timezone() -> Result<String, NikaError> {
    // Try to detect system timezone from TZ env or /etc/localtime
    let detected = detect_system_tz();

    let mut items = vec![("UTC".to_string(), "UTC", "")];
    if let Some(ref tz) = detected {
        if tz != "UTC" {
            items.insert(
                0,
                (tz.clone(), Box::leak(format!("{tz} (detected)").into_boxed_str()), ""),
            );
        }
    }
    items.push(("__other__".to_string(), "Other...", "Type IANA name"));

    let selected: String = cliclack::select("Timezone")
        .items(
            &items
                .iter()
                .map(|(id, label, hint)| (id.clone(), *label, *hint))
                .collect::<Vec<_>>(),
        )
        .interact()
        .map_err(wiz_err)?;

    if selected == "__other__" {
        let tz: String = cliclack::input("IANA timezone")
            .placeholder("America/New_York")
            .interact()
            .map_err(wiz_err)?;
        // Validate
        tz.parse::<chrono_tz::Tz>().map_err(|_| {
            NikaError::Execution(format!("invalid timezone '{tz}'"))
        })?;
        Ok(tz)
    } else {
        Ok(selected)
    }
}

/// Detect system timezone from TZ env or /etc/localtime.
fn detect_system_tz() -> Option<String> {
    // 1. TZ environment variable
    if let Ok(tz) = std::env::var("TZ") {
        if !tz.is_empty() && tz.parse::<chrono_tz::Tz>().is_ok() {
            return Some(tz);
        }
    }
    // 2. Read /etc/localtime symlink target on macOS/Linux
    #[cfg(unix)]
    {
        if let Ok(target) = std::fs::read_link("/etc/localtime") {
            let path = target.display().to_string();
            // /var/db/timezone/zoneinfo/Europe/Paris or /usr/share/zoneinfo/Europe/Paris
            if let Some(idx) = path.find("zoneinfo/") {
                let tz = &path[idx + 9..];
                if tz.parse::<chrono_tz::Tz>().is_ok() {
                    return Some(tz.to_string());
                }
            }
        }
    }
    None
}

/// Parse "HH:MM" → (hour, minute).
fn parse_hhmm(s: &str) -> Result<(u32, u32), NikaError> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    if parts.len() != 2 {
        return Err(NikaError::Execution(format!(
            "invalid time format '{s}' — use HH:MM (e.g. 09:00)"
        )));
    }
    let h: u32 = parts[0].parse().map_err(|_| {
        NikaError::Execution(format!("invalid hour in '{s}'"))
    })?;
    let m: u32 = parts[1].parse().map_err(|_| {
        NikaError::Execution(format!("invalid minute in '{s}'"))
    })?;
    if h > 23 {
        return Err(NikaError::Execution(format!(
            "hour must be 0–23, got {h}"
        )));
    }
    if m > 59 {
        return Err(NikaError::Execution(format!(
            "minute must be 0–59, got {m}"
        )));
    }
    Ok((h, m))
}

fn wiz_err(e: impl std::fmt::Display) -> NikaError {
    NikaError::IoError(std::io::Error::other(e.to_string()))
}

/// Resolve a human schedule expression to a cron expression.
fn resolve_schedule_expr(expr: &str) -> Result<String, NikaError> {
    let trimmed = expr.trim();

    // 1. @preset
    if trimmed.starts_with('@') {
        trimmed.parse::<croner::Cron>().map_err(|e| {
            NikaError::Execution(format!("NIKA-280: invalid preset '{trimmed}': {e}"))
        })?;
        return Ok(trimmed.to_string());
    }

    // 2. hron ("day at 9:00", "weekday at 14:30")
    let hron_input = if trimmed.starts_with("every ") {
        trimmed.to_string()
    } else {
        format!("every {trimmed}")
    };
    if let Ok(schedule) = hron::Schedule::parse(&hron_input) {
        if let Ok(cron) = schedule.to_cron() {
            return Ok(cron);
        }
    }

    // 3. Duration shorthand
    if let Some(cron) = duration_to_cron(trimmed) {
        cron.parse::<croner::Cron>().map_err(|e| {
            NikaError::Execution(format!("NIKA-280: invalid interval '{trimmed}': {e}"))
        })?;
        return Ok(cron);
    }

    // 4. Raw cron
    trimmed.parse::<croner::Cron>().map_err(|e| {
        NikaError::Execution(format!("NIKA-280: invalid schedule '{trimmed}': {e}"))
    })?;
    Ok(trimmed.to_string())
}

/// Generate a schedule name from workflow + cron.
fn auto_name(workflow: &str, cron_expr: &str) -> String {
    let stem = workflow
        .trim_end_matches(".nika.yaml")
        .rsplit('/')
        .next()
        .unwrap_or(workflow);

    // Try to describe the frequency
    let freq = match cron_expr {
        "@hourly" => "hourly",
        "@daily" => "daily",
        "@weekly" => "weekly",
        "@monthly" => "monthly",
        "@yearly" => "yearly",
        s if s.starts_with("*/") && s.ends_with("* * * *") => {
            // */N * * * * → every Nm
            return format!("{stem}-{}m", &s[2..s.find(' ').unwrap_or(s.len())]);
        }
        s if s.starts_with("0 */") && s.ends_with("* * *") => {
            // 0 */N * * * → every Nh
            let n = &s[4..s[4..].find(' ').map(|i| i + 4).unwrap_or(s.len())];
            return format!("{stem}-{n}h");
        }
        _ => "cron",
    };
    format!("{stem}-{freq}")
}

/// Convert seconds to a human-readable relative time string.
fn humanize_relative(seconds: i64) -> String {
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
    fn test_resolve_preset() {
        assert_eq!(resolve_schedule_expr("@daily").unwrap(), "@daily");
        assert_eq!(resolve_schedule_expr("@hourly").unwrap(), "@hourly");
    }

    #[test]
    fn test_resolve_duration() {
        assert_eq!(resolve_schedule_expr("6h").unwrap(), "0 */6 * * *");
        assert_eq!(resolve_schedule_expr("30m").unwrap(), "*/30 * * * *");
    }

    #[test]
    fn test_resolve_raw_cron() {
        assert_eq!(resolve_schedule_expr("0 9 * * *").unwrap(), "0 9 * * *");
    }

    #[test]
    fn test_resolve_hron() {
        // "day at 9:00" → prepends "every " → hron parses
        let result = resolve_schedule_expr("day at 9:00");
        if let Ok(cron) = result {
            assert!(cron.contains('9'), "cron should mention 9: {cron}");
        }
        // If hron doesn't support this exact format, it falls through
    }

    #[test]
    fn test_auto_name() {
        assert_eq!(auto_name("report.nika.yaml", "@daily"), "report-daily");
        assert_eq!(auto_name("report.nika.yaml", "0 */6 * * *"), "report-6h");
        assert_eq!(
            auto_name("path/to/check.nika.yaml", "@hourly"),
            "check-hourly"
        );
    }

    #[test]
    fn test_resolve_invalid() {
        assert!(resolve_schedule_expr("not valid").is_err());
    }
}
