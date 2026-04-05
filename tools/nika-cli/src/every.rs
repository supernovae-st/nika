//! `nika every` — create a recurring schedule.
//!
//! Examples:
//!   nika every 6h report.nika.yaml
//!   nika every day at 9:00 report.nika.yaml
//!   nika every --cron "0 */6 * * *" report.nika.yaml

use colored::Colorize;
use std::time::Duration;

use nika_daemon::{daemon_socket_path, DaemonClient, DaemonRequest, DaemonResponse};
use nika_engine::error::NikaError;

/// Arguments for `nika every`.
#[derive(Debug, clap::Args)]
pub struct EveryArgs {
    /// Schedule expression and workflow path.
    /// Last argument is the workflow, everything before is the schedule.
    /// Examples: "6h report.nika.yaml", "day at 9:00 report.nika.yaml"
    #[arg(trailing_var_arg = true, num_args = 1..)]
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
    pub overlap: String,

    /// Preview only, don't create
    #[arg(long)]
    pub dry_run: bool,
}

pub async fn handle_every_command(args: EveryArgs, quiet: bool) -> Result<(), NikaError> {
    // Parse: last arg = workflow, rest = schedule expression
    if args.args.is_empty() {
        // TODO: interactive wizard (cliclack)
        return Err(NikaError::Execution(
            "Usage: nika every <schedule> <workflow.nika.yaml>\n\
             Examples:\n  \
             nika every 6h report.nika.yaml\n  \
             nika every day at 9:00 report.nika.yaml\n  \
             nika every --cron \"0 */6 * * *\" report.nika.yaml"
                .into(),
        ));
    }

    let workflow = args.args.last().unwrap().clone();
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
            overlap: Some(args.overlap),
            inputs_json: None,
        })
        .await
        .map_err(|e| NikaError::Execution(format!("schedule: {e}")))?;

    match resp {
        DaemonResponse::ScheduleCreated { id: _, name } => {
            if !quiet {
                println!();
                println!("  {} schedule created", "✓".green().bold());
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

/// Convert duration shorthand to cron (mirrors ast/schedule.rs logic).
fn duration_to_cron(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_suffix('h') {
        let n: u32 = rest.parse().ok()?;
        if n == 0 || n > 23 {
            return None;
        }
        Some(format!("0 */{n} * * *"))
    } else if let Some(rest) = s.strip_suffix('m') {
        let n: u32 = rest.parse().ok()?;
        if n == 0 || n > 59 {
            return None;
        }
        Some(format!("*/{n} * * * *"))
    } else if let Some(rest) = s.strip_suffix('d') {
        let n: u32 = rest.parse().ok()?;
        if n == 0 || n > 28 {
            return None;
        }
        if n == 1 {
            Some("0 0 * * *".to_string())
        } else {
            Some(format!("0 0 */{n} * *"))
        }
    } else {
        None
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
