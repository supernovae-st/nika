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

// ═══════════════════════════════════════════════════════════════════════════
// DAG VISUALIZATION
// ═══════════════════════════════════════════════════════════════════════════

/// Task info for DAG rendering.
pub struct DagTask {
    pub id: String,
    pub verb: String,
    pub status: DagTaskStatus,
}

/// Task status for coloring.
#[derive(Clone, Copy, PartialEq)]
pub enum DagTaskStatus {
    Pending,
    Success,
    Failed,
}

/// Render a beautiful DAG visualization to the terminal.
///
/// Each task is rendered as a boxed node with verb icon.
/// Connections show fan-out and fan-in patterns.
///
/// ```text
///     ┌──────────────┐
///     │ ⚡ read_code  │
///     └──────┬───────┘
///        ┌───┴───┐
///   ┌────┴─────┐ ┌─────┴────┐
///   │ 🧠 review │ │ 🧠 check │
///   └────┬─────┘ └─────┬────┘
///        └───┬───┘
///     ┌──────┴───────┐
///     │ 🧠 report    │
///     └──────────────┘
/// ```
pub fn render_dag(
    tasks: &[DagTask],
    deps: &std::collections::HashMap<String, Vec<String>>,
) {
    if tasks.is_empty() {
        return;
    }

    // Compute depth layers
    let layers = compute_layers(tasks, deps);
    let max_depth = layers.len();

    // Render title
    println!(
        "\n  {} {} tasks, {} layers\n",
        "DAG".cyan().bold(),
        tasks.len().to_string().bold(),
        max_depth.to_string().bold()
    );

    // Render each layer with boxed nodes
    for (i, layer) in layers.iter().enumerate() {
        if i > 0 {
            render_box_connectors(&layers[i - 1], layer, tasks);
        }
        render_box_layer(layer, tasks);
    }

    println!();
}

fn compute_layers(
    tasks: &[DagTask],
    deps: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<Vec<String>> {
    use std::collections::HashMap;

    let mut depth: HashMap<&str, usize> = HashMap::new();
    for t in tasks {
        depth.insert(&t.id, 0);
    }

    // Iteratively compute max-depth
    let mut changed = true;
    let mut iterations = 0;
    while changed && iterations < 100 {
        changed = false;
        iterations += 1;
        for t in tasks {
            if let Some(task_deps) = deps.get(&t.id) {
                for dep in task_deps {
                    if let Some(&dep_depth) = depth.get(dep.as_str()) {
                        let new_depth = dep_depth + 1;
                        if new_depth > depth[t.id.as_str()] {
                            depth.insert(&t.id, new_depth);
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    let max_depth = depth.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<String>> = vec![Vec::new(); max_depth + 1];
    for t in tasks {
        layers[depth[t.id.as_str()]].push(t.id.clone());
    }
    layers
}

/// Width of each task box (not counting emoji width variance).
const BOX_PAD: usize = 2; // spaces inside box on each side

fn render_box_layer(layer: &[String], tasks: &[DagTask]) {
    // Calculate box width for each task
    let boxes: Vec<(String, String, DagTaskStatus)> = layer
        .iter()
        .map(|task_id| {
            let task = tasks.iter().find(|t| t.id == *task_id);
            let (verb, status) = task
                .map(|t| (t.verb.as_str(), t.status))
                .unwrap_or(("exec", DagTaskStatus::Pending));
            let icon = match verb {
                "infer" => "🧠",
                "exec" => "⚡",
                "fetch" => "🌐",
                "invoke" => "🔌",
                "agent" => "🤖",
                _ => "●",
            };
            (format!("{} {}", icon, task_id), task_id.clone(), status)
        })
        .collect();

    // Top border
    let mut top = String::from("    ");
    for (i, (label, _, status)) in boxes.iter().enumerate() {
        if i > 0 {
            top.push_str("  ");
        }
        let w = label.len() + BOX_PAD * 2 + 1; // +1 for emoji width
        let border = format!("┌{}┐", "─".repeat(w));
        top.push_str(&color_border(&border, *status));
    }
    println!("{}", top);

    // Content
    let mut mid = String::from("    ");
    for (i, (label, _, status)) in boxes.iter().enumerate() {
        if i > 0 {
            mid.push_str("  ");
        }
        let w = label.len() + BOX_PAD * 2 + 1;
        let pad_left = " ".repeat(BOX_PAD);
        let pad_right = " ".repeat(w - label.len() - BOX_PAD);
        let content = format!("│{}{}{}│", pad_left, label, pad_right);
        mid.push_str(&color_border(&content, *status));
    }
    println!("{}", mid);

    // Bottom border
    let mut bottom = String::from("    ");
    for (i, (label, _, status)) in boxes.iter().enumerate() {
        if i > 0 {
            bottom.push_str("  ");
        }
        let w = label.len() + BOX_PAD * 2 + 1;
        let border = format!("└{}┘", "─".repeat(w));
        bottom.push_str(&color_border(&border, *status));
    }
    println!("{}", bottom);
}

fn color_border(s: &str, status: DagTaskStatus) -> String {
    match status {
        DagTaskStatus::Success => s.green().to_string(),
        DagTaskStatus::Failed => s.red().bold().to_string(),
        DagTaskStatus::Pending => s.dimmed().to_string(),
    }
}

fn render_box_connectors(prev: &[String], next: &[String], _tasks: &[DagTask]) {
    if prev.len() == 1 && next.len() == 1 {
        println!("    {}     {}", " ".repeat(prev[0].len()), "│".dimmed());
    } else if prev.len() >= 1 && next.len() > prev.len() {
        // Fan-out: show spreading lines
        println!(
            "    {}     {}",
            " ".repeat(prev[0].len() / 2),
            "├────────┐".dimmed()
        );
    } else if prev.len() > 1 && next.len() <= 1 {
        // Fan-in: show converging lines
        println!(
            "    {}     {}",
            " ".repeat(4),
            "└────────┘".dimmed()
        );
    } else {
        // Parallel continuation
        let mut line = String::from("          ");
        for (i, _) in prev.iter().enumerate() {
            if i > 0 {
                line.push_str("          ");
            }
            line.push_str(&"│".dimmed().to_string());
        }
        println!("{}", line);
    }
}

// Old non-boxed renderers removed — using boxed versions above.
