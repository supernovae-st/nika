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
/// Canonical icon chart:
/// - infer:  ⚡ Violet/Purple
/// - exec:   📟 Amber/Yellow
/// - fetch:  🛰️  Cyan
/// - invoke: 🔌 Emerald/Green
/// - agent:  🐔 Rose/Magenta
pub fn verb_icon(verb: &str) -> colored::ColoredString {
    match verb {
        "infer" => "⚡".purple(),
        "exec" => "📟".yellow(),
        "fetch" => "🛰️".cyan(),
        "invoke" => "🔌".green(),
        "agent" => "🐔".magenta(),
        _ => "●".white(),
    }
}

/// Return the canonical emoji for a verb (uncolored, for DAG boxes).
pub fn verb_emoji(verb: &str) -> &'static str {
    match verb {
        "infer" => "⚡",
        "exec" => "📟",
        "fetch" => "🛰️",
        "invoke" => "🔌",
        "agent" => "🐔",
        _ => "●",
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
            "{} {} {} {} {} {} {} {}",
            "\u{2713}".green().bold(), // ✓
            "Done!".green().bold(),
            elapsed_str.dimmed(),
            "·".dimmed(),
            format!("{} tokens", total_tokens).dimmed(),
            "·".dimmed(),
            format!("${}", crate::provider::cost::format_cost(total_cost)
                .trim_start_matches('$')).dimmed(),
            if total_cost > 0.0 { "" } else { "" }
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
// DAG VISUALIZATION v3 — Double-line borders, arrows, status badges
// ═══════════════════════════════════════════════════════════════════════════

/// Task info for DAG rendering.
pub struct DagTask {
    pub id: String,
    pub verb: String,
    pub status: DagTaskStatus,
    /// Optional metadata (duration, tokens, error)
    pub meta: Option<String>,
}

/// Task status for coloring.
#[derive(Clone, Copy, PartialEq)]
pub enum DagTaskStatus {
    Pending,
    Success,
    Failed,
    Skipped,
}

/// Render a DAG visualization with double-line borders, arrows, and status badges.
pub fn render_dag(
    tasks: &[DagTask],
    deps: &std::collections::HashMap<String, Vec<String>>,
) {
    if tasks.is_empty() {
        return;
    }

    let layers = compute_layers(tasks, deps);
    let edge_count: usize = deps.values().map(|v| v.len()).sum();

    // Header
    println!();
    println!(
        "  {} {} tasks {} {} layers {} {} edges",
        "DAG".cyan().bold(),
        tasks.len().to_string().white().bold(),
        "·".dimmed(),
        layers.len().to_string().white().bold(),
        "·".dimmed(),
        edge_count.to_string().white().bold(),
    );
    println!();

    // Render layers with edges
    for (i, layer) in layers.iter().enumerate() {
        if i > 0 {
            render_v3_edges(&layers[i - 1], layer, tasks, deps);
        }
        render_v3_boxes(layer, tasks);
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

/// Padding inside each box (each side).
const BOX_PAD: usize = 1;

// ── v3 Box Rendering ──────────────────────────────────────────────────

fn status_badge(status: DagTaskStatus) -> &'static str {
    match status {
        DagTaskStatus::Success => "✓",
        DagTaskStatus::Failed => "✗",
        DagTaskStatus::Skipped => "⊘",
        DagTaskStatus::Pending => " ",
    }
}

fn colorize(s: &str, status: DagTaskStatus) -> String {
    match status {
        DagTaskStatus::Success => s.green().to_string(),
        DagTaskStatus::Failed => s.red().bold().to_string(),
        DagTaskStatus::Skipped => s.yellow().dimmed().to_string(),
        DagTaskStatus::Pending => s.dimmed().to_string(),
    }
}

fn render_v3_boxes(layer: &[String], tasks: &[DagTask]) {
    // Build box data
    let boxes: Vec<(&DagTask, String, usize)> = layer
        .iter()
        .map(|id| {
            let task = tasks.iter().find(|t| t.id == *id).unwrap();
            let icon = match task.verb.as_str() {
                "infer" => "⚡",
                "exec" => "📟",
                "fetch" => "🛰️",
                "invoke" => "🔌",
                "agent" => "🐔",
                _ => "●",
            };
            let label = format!("{} {}", icon, task.id);
            let dw = display_width(&label);
            (task, label, dw)
        })
        .collect();

    // Top border: ╔═✓═══════════╗ (or ╔══════════════╗ for pending)
    let mut top = String::from("    ");
    for (i, (task, _, dw)) in boxes.iter().enumerate() {
        if i > 0 { top.push_str("  "); }
        let w = dw + BOX_PAD * 2;
        let border = if task.status == DagTaskStatus::Pending {
            format!("╔{}╗", "═".repeat(w))
        } else {
            let badge = status_badge(task.status);
            let fill_w = w.saturating_sub(2); // -2 for badge + surrounding ═
            format!("╔═{}═{}╗", badge, "═".repeat(fill_w.max(1)))
        };
        top.push_str(&colorize(&border, task.status));
    }
    println!("{}", top);

    // Content: ║  ⚡ task_name  ║
    let mut mid = String::from("    ");
    for (i, (task, label, dw)) in boxes.iter().enumerate() {
        if i > 0 { mid.push_str("  "); }
        let w = dw + BOX_PAD * 2;
        let pad_l = " ".repeat(BOX_PAD);
        let pad_r = " ".repeat(w.saturating_sub(dw + BOX_PAD));
        let content = format!("║{}{}{}║", pad_l, label, pad_r);
        mid.push_str(&colorize(&content, task.status));
    }
    println!("{}", mid);

    // Metadata line (if any task has meta)
    let has_meta = boxes.iter().any(|(t, _, _)| t.meta.is_some());
    if has_meta {
        let mut meta_line = String::from("    ");
        for (i, (task, _, dw)) in boxes.iter().enumerate() {
            if i > 0 { meta_line.push_str("  "); }
            let w = dw + BOX_PAD * 2;
            let meta_text = task.meta.as_deref().unwrap_or("");
            let meta_display = if meta_text.is_empty() {
                " ".repeat(w)
            } else {
                let mw = display_width(meta_text);
                let pad = w.saturating_sub(mw + BOX_PAD);
                format!("{}{}{}", " ".repeat(BOX_PAD), meta_text, " ".repeat(pad))
            };
            let content = format!("║{}║", meta_display);
            meta_line.push_str(&colorize(&content, task.status));
        }
        println!("{}", meta_line);
    }

    // Bottom border: ╚════════╤═════╝ (with ╤ at center for edge drop)
    let mut bottom = String::from("    ");
    for (i, (task, _, dw)) in boxes.iter().enumerate() {
        if i > 0 { bottom.push_str("  "); }
        let w = dw + BOX_PAD * 2;
        let border = format!("╚{}╝", "═".repeat(w));
        bottom.push_str(&colorize(&border, task.status));
    }
    println!("{}", bottom);
}

fn render_v3_edges(
    prev_layer: &[String],
    next_layer: &[String],
    tasks: &[DagTask],
    deps: &std::collections::HashMap<String, Vec<String>>,
) {
    let prev_centers = compute_box_centers(prev_layer, tasks);
    let next_centers = compute_box_centers(next_layer, tasks);

    let max_pos = prev_centers
        .iter()
        .chain(next_centers.iter())
        .map(|&(_, c, w)| c + w / 2 + 2)
        .max()
        .unwrap_or(40);

    // Collect edges
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (ni, next_id) in next_layer.iter().enumerate() {
        if let Some(task_deps) = deps.get(next_id) {
            for dep in task_deps {
                if let Some(pi) = prev_layer.iter().position(|p| p == dep) {
                    edges.push((prev_centers[pi].1, next_centers[ni].1));
                }
            }
        }
    }

    if edges.is_empty() {
        println!();
        return;
    }

    let width = max_pos + 4;

    // Nearly straight down? (within 3 columns = close enough to snap)
    let all_straight = edges.iter().all(|(f, t)| {
        let diff = if *f > *t { f - t } else { t - f };
        diff <= 3
    });
    if all_straight {
        // Use the average of source and target for pipe position
        let mut pipe_cols: Vec<usize> = Vec::new();
        for &(from, to) in &edges {
            let col = (from + to) / 2;
            if !pipe_cols.contains(&col) {
                pipe_cols.push(col);
            }
        }

        let mut line = vec![' '; width];
        for &col in &pipe_cols {
            if col < line.len() { line[col] = '│'; }
        }
        let s: String = line.iter().collect();
        println!("    {}", s.dimmed());

        let mut arrow_line = vec![' '; width];
        for &col in &pipe_cols {
            if col < arrow_line.len() { arrow_line[col] = '▼'; }
        }
        let s2: String = arrow_line.iter().collect();
        println!("    {}", s2.dimmed());
        return;
    }

    // Line 1: vertical drops with downward arrows
    let mut drop_line = vec![' '; width];
    for &(from, _) in &edges {
        if from < drop_line.len() { drop_line[from] = '│'; }
    }
    let s1: String = drop_line.iter().collect();
    println!("    {}", s1.dimmed());

    // Line 2: horizontal connections with arrows
    // Three-pass rendering to avoid character conflicts:
    // Pass 1: lay down horizontal lines ─
    // Pass 2: place arrows ▼ at targets
    // Pass 3: place corners └ ┘ at sources
    let mut conn_line = vec![' '; width];

    // Collect all edge segments
    struct EdgeSeg { src: usize, tgt: usize }
    let mut segs: Vec<EdgeSeg> = Vec::new();

    for (ni, next_id) in next_layer.iter().enumerate() {
        let target_col = next_centers[ni].1;
        if let Some(task_deps) = deps.get(next_id) {
            for dep in task_deps {
                if let Some(pi) = prev_layer.iter().position(|p| p == dep) {
                    segs.push(EdgeSeg { src: prev_centers[pi].1, tgt: target_col });
                }
            }
        }
    }

    // Pass 1: horizontal fills
    for seg in &segs {
        if seg.src != seg.tgt {
            let (lo, hi) = if seg.src < seg.tgt { (seg.src, seg.tgt) } else { (seg.tgt, seg.src) };
            for col in lo..=hi {
                if col < conn_line.len() && conn_line[col] == ' ' {
                    conn_line[col] = '─';
                }
            }
        }
    }

    // Pass 2: arrows at target columns
    let mut target_cols: Vec<usize> = segs.iter().map(|s| s.tgt).collect();
    target_cols.sort();
    target_cols.dedup();
    for &col in &target_cols {
        if col < conn_line.len() {
            conn_line[col] = '▼';
        }
    }

    // Pass 3: corners at source columns (only if src != tgt)
    for seg in &segs {
        if seg.src != seg.tgt && seg.src < conn_line.len() {
            // Don't overwrite arrows or already-placed corners
            let existing = conn_line[seg.src];
            if existing != '▼' && existing != '└' && existing != '┘' {
                conn_line[seg.src] = if seg.src < seg.tgt { '└' } else { '┘' };
            }
        }
    }

    let s2: String = conn_line.iter().collect();
    println!("    {}", s2.dimmed());
}

/// Calculate terminal display width of a string.
/// Emojis are 2 columns, ASCII is 1 column.
fn display_width(s: &str) -> usize {
    let mut w = 0;
    for ch in s.chars() {
        if ch.len_utf8() >= 3 {
            // Multi-byte char (emoji, CJK) → 2 columns
            w += 2;
        } else {
            w += 1;
        }
    }
    w
}

/// Compute center column position for each box in a layer.
fn compute_box_centers(layer: &[String], tasks: &[DagTask]) -> Vec<(usize, usize, usize)> {
    let indent = 4;
    let gap = 2;
    let mut positions = Vec::new();
    let mut col = indent;

    for (i, task_id) in layer.iter().enumerate() {
        let task = tasks.iter().find(|t| t.id == *task_id);
        let verb = task.map(|t| t.verb.as_str()).unwrap_or("exec");
        let icon = match verb {
            "infer" => "⚡",
            "exec" => "📟",
            "fetch" => "🛰️",
            "invoke" => "🔌",
            "agent" => "🐔",
            _ => "●",
        };
        let label = format!("{} {}", icon, task_id);
        let dw = display_width(&label) + BOX_PAD * 2 + 2; // +2 for ║ borders
        let center = col + dw / 2;
        positions.push((i, center, dw));
        col += dw + gap;
    }

    positions
}
