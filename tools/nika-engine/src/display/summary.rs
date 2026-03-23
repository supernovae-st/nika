//! Summary display helpers — workflow completion and doctor diagnostics.

use colored::Colorize;

// Unicode box characters for lightweight framing.
const TOP_LEFT: &str = "\u{250c}"; // ┌
const TOP_RIGHT: &str = "\u{2510}"; // ┐
const BOTTOM_LEFT: &str = "\u{2514}"; // └
const BOTTOM_RIGHT: &str = "\u{2518}"; // ┘
const HORIZONTAL: &str = "\u{2500}"; // ─
const VERTICAL: &str = "\u{2502}"; // │

/// Print the workflow completion summary.
///
/// ```text
/// ──────────────────────────────────────────────────
/// ✓ Done! (1.7s | 42 tokens | $0.0003)
/// ```
pub fn print_done_summary(
    elapsed_str: &str,
    total_tokens: u64,
    total_cost: f64,
    trace_path: Option<&str>,
) {
    println!();
    println!("{}", HORIZONTAL.repeat(50).dimmed());

    // Main done line with telemetry
    if total_tokens > 0 {
        println!(
            "{} {} {} {} {} {} {}",
            "\u{2713}".green().bold(), // ✓
            "Done!".green().bold(),
            elapsed_str.dimmed(),
            "·".dimmed(),
            format!("{} tokens", total_tokens).dimmed(),
            "·".dimmed(),
            format!(
                "${}",
                crate::provider::cost::format_cost(total_cost).trim_start_matches('$')
            )
            .dimmed()
        );
    } else {
        println!(
            "{} {} {}",
            "\u{2713}".green().bold(), // ✓
            "Done!".green().bold(),
            elapsed_str.dimmed(),
        );
    }

    // Trace link
    if let Some(path) = trace_path {
        println!("  {} {}", "trace:".dimmed(), path.dimmed());
    }

    println!();
}

/// Print the doctor header with a nice box.
///
/// ```text
/// ┌─ Nika Doctor ──────────────────────────────────┐
/// │ v0.12.0 | Checking system health...            │
/// └────────────────────────────────────────────────┘
/// ```
pub fn print_doctor_header(version: &str) {
    let title = "Nika Doctor";
    let inner = format!(" v{} | Checking system health... ", version);

    let title_segment = format!("{} {} ", HORIZONTAL, title);
    let min_width = inner.len() + 2;
    let fill_len = if title_segment.len() < min_width {
        min_width - title_segment.len()
    } else {
        1
    };
    let top_fill = HORIZONTAL.repeat(fill_len);
    let top_line = format!(
        "{}{}{}{}",
        TOP_LEFT.dimmed(),
        title_segment.bold(),
        top_fill.dimmed(),
        TOP_RIGHT.dimmed()
    );

    let total_width = title_segment.len() + fill_len;
    let pad = if inner.len() < total_width {
        " ".repeat(total_width - inner.len())
    } else {
        String::new()
    };
    let content_line = format!(
        "{}{}{}{}",
        VERTICAL.dimmed(),
        inner.dimmed(),
        pad,
        VERTICAL.dimmed()
    );

    let bottom_fill = HORIZONTAL.repeat(total_width);
    let bottom_line = format!(
        "{}{}{}",
        BOTTOM_LEFT.dimmed(),
        bottom_fill.dimmed(),
        BOTTOM_RIGHT.dimmed()
    );

    println!();
    println!("{}", top_line);
    println!("{}", content_line);
    println!("{}", bottom_line);
    println!();
}

/// Print the doctor summary with colored counts, separator, and optional next steps.
pub fn print_doctor_summary(pass_count: usize, warn_count: usize, fail_count: usize) {
    println!();
    println!("{}", HORIZONTAL.repeat(50).dimmed());

    let status_icon = if fail_count > 0 {
        "\u{2717}".red().bold() // ✗
    } else if warn_count > 0 {
        "\u{26a0}".yellow().bold() // ⚠
    } else {
        "\u{2713}".green().bold() // ✓
    };

    let status_word = if fail_count > 0 {
        "Issues found".red().bold()
    } else if warn_count > 0 {
        "Mostly healthy".yellow().bold()
    } else {
        "All good!".green().bold()
    };

    println!(
        "{} {} \u{2014} {} passed, {} warnings, {} failed", // — (em dash)
        status_icon,
        status_word,
        pass_count.to_string().green(),
        warn_count.to_string().yellow(),
        fail_count.to_string().red()
    );

    if warn_count > 0 || fail_count > 0 {
        println!();
        println!("  {}", "Recommended order:".bold());
        println!(
            "    {} {} initialize project",
            "1.".bold(),
            "nika init".cyan()
        );
        println!(
            "    {} {} verify everything works",
            "2.".bold(),
            "nika doctor".cyan()
        );
    }

    println!();
}
