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
                "infer" => "🧠",
                "exec" => "⚡",
                "fetch" => "🌐",
                "invoke" => "🔌",
                "agent" => "🤖",
                _ => "●",
            };
            let label = format!("{} {}", icon, task.id);
            let dw = display_width(&label);
            (task, label, dw)
        })
        .collect();

    // Top border: ╔═✓═══════════╗
    let mut top = String::from("    ");
    for (i, (task, _, dw)) in boxes.iter().enumerate() {
        if i > 0 { top.push_str("  "); }
        let badge = status_badge(task.status);
        let fill_w = dw + BOX_PAD * 2 - 1; // -1 for the badge char
        let border = format!("╔═{}═{}╗", badge, "═".repeat(fill_w.max(1)));
        top.push_str(&colorize(&border, task.status));
    }
    println!("{}", top);

    // Content: ║  🧠 task_name  ║
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

    // All straight down?
    let all_straight = edges.iter().all(|(f, t)| f == t);
    if all_straight {
        let mut line = vec![' '; width];
        for &(col, _) in &edges {
            if col < line.len() { line[col] = '│'; }
        }
        // Add arrow at each drop point
        let s: String = line.iter().collect();
        println!("    {}", s.dimmed());

        let mut arrow_line = vec![' '; width];
        for &(col, _) in &edges {
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
    let mut conn_line = vec![' '; width];

    for (ni, next_id) in next_layer.iter().enumerate() {
        let target_col = next_centers[ni].1;
        let mut sources: Vec<usize> = Vec::new();
        if let Some(task_deps) = deps.get(next_id) {
            for dep in task_deps {
                if let Some(pi) = prev_layer.iter().position(|p| p == dep) {
                    sources.push(prev_centers[pi].1);
                }
            }
        }
        if sources.is_empty() { continue; }

        for &src in &sources {
            let (lo, hi) = if src <= target_col { (src, target_col) } else { (target_col, src) };
            for col in lo..=hi {
                if col < conn_line.len() {
                    if conn_line[col] == ' ' {
                        conn_line[col] = '─';
                    } else if conn_line[col] == '│' {
                        conn_line[col] = '┼';
                    }
                }
            }
        }
        if target_col < conn_line.len() {
            conn_line[target_col] = '▼';
        }
        for &src in &sources {
            if src < conn_line.len() && src != target_col {
                conn_line[src] = if src < target_col { '└' } else { '┘' };
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

fn render_box_layer(layer: &[String], tasks: &[DagTask]) {
    // Build box content: (label_str, display_width, task_id, status)
    let boxes: Vec<(String, usize, String, DagTaskStatus)> = layer
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
            let label = format!("{} {}", icon, task_id);
            let dw = display_width(&label);
            (label, dw, task_id.clone(), status)
        })
        .collect();

    // Top border
    let mut top = String::from("    ");
    for (i, (_, dw, _, status)) in boxes.iter().enumerate() {
        if i > 0 {
            top.push_str("  ");
        }
        let w = dw + BOX_PAD * 2;
        let border = format!("┌{}┐", "─".repeat(w));
        top.push_str(&color_border(&border, *status));
    }
    println!("{}", top);

    // Content
    let mut mid = String::from("    ");
    for (i, (label, dw, _, status)) in boxes.iter().enumerate() {
        if i > 0 {
            mid.push_str("  ");
        }
        let w = dw + BOX_PAD * 2;
        let pad_left = " ".repeat(BOX_PAD);
        let pad_right_len = w.saturating_sub(dw + BOX_PAD);
        let pad_right = " ".repeat(pad_right_len);
        let content = format!("│{}{}{}│", pad_left, label, pad_right);
        mid.push_str(&color_border(&content, *status));
    }
    println!("{}", mid);

    // Bottom border
    let mut bottom = String::from("    ");
    for (i, (_, dw, _, status)) in boxes.iter().enumerate() {
        if i > 0 {
            bottom.push_str("  ");
        }
        let w = dw + BOX_PAD * 2;
        let border = format!("└{}┘", "─".repeat(w));
        bottom.push_str(&color_border(&border, *status));
    }
    println!("{}", bottom);
}

fn color_border(s: &str, status: DagTaskStatus) -> String {
    match status {
        DagTaskStatus::Success => s.green().to_string(),
        DagTaskStatus::Failed => s.red().bold().to_string(),
        DagTaskStatus::Skipped => s.yellow().dimmed().to_string(),
        DagTaskStatus::Pending => s.dimmed().to_string(),
    }
}

fn render_box_connectors(
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

    // Collect all edges as (from_col, to_col)
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

    // All straight down? Simple vertical pipes
    let all_straight = edges.iter().all(|(f, t)| f == t);
    if all_straight {
        let mut line = vec![' '; max_pos + 4];
        for &(col, _) in &edges {
            if col < line.len() {
                line[col] = '│';
            }
        }
        let s: String = line.iter().collect();
        println!("    {}", s.dimmed());
        return;
    }

    // Two-line rendering for complex edges:
    // Line 1: vertical drops from prev centers
    // Line 2: horizontal connections + vertical rises to next centers
    let width = max_pos + 4;

    // Line 1: drop down from each prev node that has outgoing edges
    let mut drop_line = vec![' '; width];
    let mut active_cols: Vec<usize> = Vec::new();
    for &(from, _) in &edges {
        if from < drop_line.len() && !active_cols.contains(&from) {
            drop_line[from] = '│';
            active_cols.push(from);
        }
    }
    let s1: String = drop_line.iter().collect();
    println!("    {}", s1.dimmed());

    // Line 2: horizontal spans connecting to next nodes
    let mut conn_line = vec![' '; width];

    // For each next node, find the leftmost and rightmost source
    for (ni, next_id) in next_layer.iter().enumerate() {
        let target_col = next_centers[ni].1;
        let mut sources: Vec<usize> = Vec::new();
        if let Some(task_deps) = deps.get(next_id) {
            for dep in task_deps {
                if let Some(pi) = prev_layer.iter().position(|p| p == dep) {
                    sources.push(prev_centers[pi].1);
                }
            }
        }
        if sources.is_empty() {
            continue;
        }

        // All sources merge at target_col
        for &src in &sources {
            let (lo, hi) = if src <= target_col {
                (src, target_col)
            } else {
                (target_col, src)
            };
            for col in lo..=hi {
                if col < conn_line.len() {
                    if conn_line[col] == ' ' {
                        conn_line[col] = '─';
                    } else if conn_line[col] == '│' {
                        conn_line[col] = '┼';
                    }
                }
            }
        }
        // Mark target with down arrow
        if target_col < conn_line.len() {
            conn_line[target_col] = '┬';
        }
        // Mark sources with corners
        for &src in &sources {
            if src < conn_line.len() && src != target_col {
                if src < target_col {
                    conn_line[src] = '└';
                } else {
                    conn_line[src] = '┘';
                }
            }
        }
    }

    let s2: String = conn_line.iter().collect();
    println!("    {}", s2.dimmed());
}

/// Compute center column position for each box in a layer.
/// Returns Vec of (task_id_index, center_col, box_width).
fn compute_box_centers(layer: &[String], tasks: &[DagTask]) -> Vec<(usize, usize, usize)> {
    let indent = 4; // base indent
    let gap = 2; // gap between boxes
    let mut positions = Vec::new();
    let mut col = indent;

    for (i, task_id) in layer.iter().enumerate() {
        let task = tasks.iter().find(|t| t.id == *task_id);
        let verb = task.map(|t| t.verb.as_str()).unwrap_or("exec");
        let icon = match verb {
            "infer" => "🧠",
            "exec" => "⚡",
            "fetch" => "🌐",
            "invoke" => "🔌",
            "agent" => "🤖",
            _ => "●",
        };
        let label = format!("{} {}", icon, task_id);
        let dw = display_width(&label) + BOX_PAD * 2 + 2; // +2 for │ borders
        let center = col + dw / 2;
        positions.push((i, center, dw));
        col += dw + gap;
    }

    positions
}

// Old non-boxed renderers removed — using boxed versions above.
