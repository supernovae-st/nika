// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG visualization v3 — double-line borders, arrows, status badges.

use colored::Colorize;

/// Task info for DAG rendering.
pub struct DagTask {
    pub id: String,
    pub verb: String,
    pub status: DagTaskStatus,
    /// Optional metadata (duration, tokens, error) — line 1
    pub meta: Option<String>,
    /// Additional property tags for check mode (structured, mcp, guardrails, etc.)
    pub tags: Vec<String>,
}

/// Task status for coloring.
#[derive(Clone, Copy, PartialEq)]
pub enum DagTaskStatus {
    Pending,
    Success,
    Failed,
    Skipped,
}

/// Padding inside each box (each side).
const BOX_PAD: usize = 1;

/// Render a DAG visualization with double-line borders, arrows, and status badges.
pub fn render_dag(tasks: &[DagTask], deps: &std::collections::HashMap<String, Vec<String>>) {
    if tasks.is_empty() {
        return;
    }

    let layers = compute_layers(tasks, deps);
    let edge_count: usize = deps.values().map(|v| v.len()).sum();

    // Header
    println!();
    let task_word = if tasks.len() == 1 { "task" } else { "tasks" };
    let layer_word = if layers.len() == 1 { "layer" } else { "layers" };
    let edge_word = if edge_count == 1 { "edge" } else { "edges" };
    println!(
        "  {} {} {} {} {} {} {} {}",
        "DAG".cyan().bold(),
        tasks.len().to_string().white().bold(),
        task_word,
        "·".dimmed(),
        layers.len().to_string().white().bold(),
        layer_word,
        "·".dimmed(),
        format!("{} {}", edge_count, edge_word).white().bold(),
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
        .filter_map(|id| {
            let task = tasks.iter().find(|t| t.id == *id)?;
            let icon = crate::icons::verb_plain(&task.verb);
            let label = format!("{} {}", icon, task.id);
            let dw = display_width(&label);
            Some((task, label, dw))
        })
        .collect();

    // Top border: ╔═✓═══════════╗ (or ╔══════════════╗ for pending)
    let mut top = String::from("    ");
    for (i, (task, _, dw)) in boxes.iter().enumerate() {
        if i > 0 {
            top.push_str("  ");
        }
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
        if i > 0 {
            mid.push_str("  ");
        }
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
            if i > 0 {
                meta_line.push_str("  ");
            }
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

    // Tag lines (if any task has tags)
    let max_tag_count = boxes
        .iter()
        .map(|(t, _, _)| t.tags.len())
        .max()
        .unwrap_or(0);
    for tag_idx in 0..max_tag_count {
        let mut tag_line = String::from("    ");
        for (i, (task, _, dw)) in boxes.iter().enumerate() {
            if i > 0 {
                tag_line.push_str("  ");
            }
            let w = dw + BOX_PAD * 2;
            let tag_display = if let Some(tag) = task.tags.get(tag_idx) {
                let tw = display_width(tag);
                let pad = w.saturating_sub(tw + BOX_PAD);
                format!("{}{}{}", " ".repeat(BOX_PAD), tag, " ".repeat(pad))
            } else {
                " ".repeat(w)
            };
            let content = format!("║{}║", tag_display);
            tag_line.push_str(&colorize(&content, task.status));
        }
        println!("{}", tag_line);
    }

    // Bottom border: ╚════════╤═════╝ (with ╤ at center for edge drop)
    let mut bottom = String::from("    ");
    for (i, (task, _, dw)) in boxes.iter().enumerate() {
        if i > 0 {
            bottom.push_str("  ");
        }
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
            if col < line.len() {
                line[col] = '│';
            }
        }
        let s: String = line.iter().collect();
        println!("    {}", s.dimmed());

        let mut arrow_line = vec![' '; width];
        for &col in &pipe_cols {
            if col < arrow_line.len() {
                arrow_line[col] = '▼';
            }
        }
        let s2: String = arrow_line.iter().collect();
        println!("    {}", s2.dimmed());
        return;
    }

    // Line 1: vertical drops with downward arrows
    let mut drop_line = vec![' '; width];
    for &(from, _) in &edges {
        if from < drop_line.len() {
            drop_line[from] = '│';
        }
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
    struct EdgeSeg {
        src: usize,
        tgt: usize,
    }
    let mut segs: Vec<EdgeSeg> = Vec::new();

    for (ni, next_id) in next_layer.iter().enumerate() {
        let target_col = next_centers[ni].1;
        if let Some(task_deps) = deps.get(next_id) {
            for dep in task_deps {
                if let Some(pi) = prev_layer.iter().position(|p| p == dep) {
                    segs.push(EdgeSeg {
                        src: prev_centers[pi].1,
                        tgt: target_col,
                    });
                }
            }
        }
    }

    // Pass 1: horizontal fills
    for seg in &segs {
        if seg.src != seg.tgt {
            let (lo, hi) = if seg.src < seg.tgt {
                (seg.src, seg.tgt)
            } else {
                (seg.tgt, seg.src)
            };
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
    use unicode_width::UnicodeWidthStr;
    UnicodeWidthStr::width(s)
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
        let icon = crate::icons::verb_plain(verb);
        let label = format!("{} {}", icon, task_id);
        let dw = display_width(&label) + BOX_PAD * 2 + 2; // +2 for ║ borders
        let center = col + dw / 2;
        positions.push((i, center, dw));
        col += dw + gap;
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dag_task_has_tags_field() {
        let task = DagTask {
            id: "test".to_string(),
            verb: "infer".to_string(),
            status: DagTaskStatus::Pending,
            meta: None,
            tags: vec!["structured".to_string(), "mcp:novanet".to_string()],
        };
        assert_eq!(task.tags.len(), 2);
        assert_eq!(task.tags[0], "structured");
        assert_eq!(task.tags[1], "mcp:novanet");
    }

    #[test]
    fn dag_task_empty_tags() {
        let task = DagTask {
            id: "test".to_string(),
            verb: "exec".to_string(),
            status: DagTaskStatus::Success,
            meta: Some("0.3s".to_string()),
            tags: Vec::new(),
        };
        assert!(task.tags.is_empty());
    }

    #[test]
    fn verb_plain_icons_used_in_boxes() {
        // Verify verb_plain returns single-width characters (not multi-byte emoji)
        let icon = crate::icons::verb_plain("infer");
        assert_eq!(icon, "\u{2727}"); // checkmark-like
        let icon = crate::icons::verb_plain("exec");
        assert_eq!(icon, "\u{2388}"); // helm
        let icon = crate::icons::verb_plain("fetch");
        assert_eq!(icon, "\u{2604}"); // comet
        let icon = crate::icons::verb_plain("invoke");
        assert_eq!(icon, "\u{229B}"); // circled asterisk
        let icon = crate::icons::verb_plain("agent");
        assert_eq!(icon, "\u{274B}"); // heavy teardrop
    }

    #[test]
    fn display_width_uses_unicode_width() {
        // ASCII: 1 column per char
        assert_eq!(display_width("hello"), 5);
        // Cosmic icons are single-width
        assert_eq!(display_width("\u{2727}"), 1);
        assert_eq!(display_width("\u{2388}"), 1);
    }
}
