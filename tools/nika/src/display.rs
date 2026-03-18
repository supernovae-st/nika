//! CLI display helpers for consistent, visually appealing output.
//!
//! Provides box-drawing, verb icons, and summary formatting used by
//! `main.rs` (workflow header), `runner.rs` (progress), and `doctor.rs`.

use colored::Colorize;

// Unicode box characters for lightweight framing.
const TOP_LEFT: &str = "\u{250c}";     // ┌
const TOP_RIGHT: &str = "\u{2510}";    // ┐
const BOTTOM_LEFT: &str = "\u{2514}";  // └
const BOTTOM_RIGHT: &str = "\u{2518}"; // ┘
const HORIZONTAL: &str = "\u{2500}";   // ─
const VERTICAL: &str = "\u{2502}";     // │

/// Return a colored icon for each Nika verb.
///
/// - infer  -> yellow brain
/// - exec   -> blue lightning
/// - fetch  -> green globe
/// - invoke -> cyan plug
/// - agent  -> magenta robot
pub fn verb_icon(verb: &str) -> colored::ColoredString {
    match verb {
        "infer" => "\u{1f9e0}".yellow(),  // 🧠
        "exec" => "\u{26a1}".blue(),       // ⚡
        "fetch" => "\u{1f310}".green(),    // 🌐
        "invoke" => "\u{1f50c}".cyan(),    // 🔌
        "agent" => "\u{1f916}".magenta(),  // 🤖
        _ => "\u{25cf}".white(),           // ●
    }
}

/// Print a header box around workflow metadata.
///
/// ```text
/// ┌─ my-workflow ──────────────────────────────────┐
/// │ Provider: openai | Model: gpt-4.1-mini | Tasks: 3 │
/// └────────────────────────────────────────────────┘
/// ```
pub fn print_workflow_header(
    name: Option<&str>,
    provider: &str,
    model: &str,
    task_count: usize,
) {
    let display_name = name.unwrap_or("workflow");
    let noun = if task_count == 1 { "task" } else { "tasks" };
    let inner = format!(
        " Provider: {} | Model: {} | {}: {} ",
        provider, model, noun, task_count
    );

    // Title line: ┌─ name ─────...─┐
    let title_segment = format!("{} {} ", HORIZONTAL, display_name);
    // Ensure box is at least as wide as the inner content + 2 for corners
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
        title_segment.dimmed(),
        top_fill.dimmed(),
        TOP_RIGHT.dimmed()
    );

    // Inner content line
    let total_width = title_segment.len() + fill_len;
    let pad = if inner.len() < total_width {
        " ".repeat(total_width - inner.len())
    } else {
        String::new()
    };
    let content_line = format!(
        "{}{}{}{}",
        VERTICAL.dimmed(),
        inner.cyan(),
        pad,
        VERTICAL.dimmed()
    );

    // Bottom line
    let bottom_fill = HORIZONTAL.repeat(total_width);
    let bottom_line = format!(
        "{}{}{}",
        BOTTOM_LEFT.dimmed(),
        bottom_fill.dimmed(),
        BOTTOM_RIGHT.dimmed()
    );

    println!("{}", top_line);
    println!("{}", content_line);
    println!("{}", bottom_line);
}

/// Print the workflow completion summary.
///
/// ```text
/// ──────────────────────────────────────────────────
/// ✓ Done! (1.7s | 42 tokens | $0.0003)
/// ```
pub fn print_done_summary(elapsed_str: &str, total_tokens: u64, total_cost: f64) {
    println!();
    println!("{}", HORIZONTAL.repeat(50).dimmed());
    if total_tokens > 0 {
        println!(
            "{} {} ({} | {} tokens | ${})",
            "\u{2713}".green().bold(), // ✓
            "Done!".green().bold(),
            elapsed_str.dimmed(),
            total_tokens.to_string().dimmed(),
            crate::provider::cost::format_cost(total_cost)
                .trim_start_matches('$')
                .dimmed()
        );
    } else {
        println!(
            "{} {} ({})",
            "\u{2713}".green().bold(), // ✓
            "Done!".green().bold(),
            elapsed_str.dimmed()
        );
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

/// Print the doctor summary with colored counts and a separator.
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
    println!();
}

/// Format elapsed time with color based on duration.
///
/// - < 1s: green (fast)
/// - 1-5s: yellow (moderate)
/// - > 5s: red (slow)
pub fn format_duration(secs: f32) -> colored::ColoredString {
    let text = if secs < 0.1 {
        format!("{:.0}ms", secs * 1000.0)
    } else if secs < 1.0 {
        format!("{:.1}s", secs)
    } else if secs < 60.0 {
        format!("{:.1}s", secs)
    } else {
        format!("{}m{:.0}s", (secs / 60.0) as u32, secs % 60.0)
    };

    if secs < 1.0 {
        text.green()
    } else if secs < 5.0 {
        text.yellow()
    } else {
        text.red()
    }
}

/// Print a workflow summary line with task counts by verb.
pub fn print_task_summary(
    total: usize,
    succeeded: usize,
    failed: usize,
    skipped: usize,
) {
    if failed > 0 {
        println!(
            "  {} {} succeeded, {} failed, {} skipped",
            "Tasks:".dimmed(),
            succeeded.to_string().green(),
            failed.to_string().red(),
            skipped.to_string().yellow()
        );
    } else if total > 1 {
        println!(
            "  {} {} succeeded",
            "Tasks:".dimmed(),
            succeeded.to_string().green()
        );
    }
}
