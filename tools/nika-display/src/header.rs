//! Header renderer — workflow info box + static DAG.

use rustc_hash::FxHashMap as HashMap;

use colored::Colorize;

use crate::colors::stripped_len;
use crate::icons;

/// Print the new rounded-corner header box.
///
/// ```text
/// ╭───────────────────────────────────────────────────────────╮
/// │                                                           │
/// │  N I K A                                        v0.40.2   │
/// │                                                           │
/// │  seo-pipeline                                             │
/// │  ⋈ claude / sonnet-4                   6 tasks · 3 layers │
/// │  gen:7f3a2b                                               │
/// │                                                           │
/// ╰───────────────────────────────────────────────────────────╯
/// ```
pub fn print_header(
    name: Option<&str>,
    provider: &str,
    model: &str,
    task_count: usize,
    layer_count: usize,
    version: &str,
    generation_id: &str,
) {
    let w = terminal_size::terminal_size()
        .map(|(tw, _)| tw.0 as usize)
        .unwrap_or(80)
        .min(72);

    let inner = w - 2; // account for │ on each side
    let border = "─".repeat(inner);

    println!("╭{}╮", border.dimmed());
    println!("│{}│", " ".repeat(inner));

    // Title line
    let title = "N I K A";
    let ver = format!("v{}", version);
    let pad = inner.saturating_sub(title.len() + ver.len() + 4);
    println!(
        "│  {}{}{}  │",
        title.bold().white(),
        " ".repeat(pad),
        ver.dimmed()
    );
    println!("│{}│", " ".repeat(inner));

    // Workflow name
    let display_name = name.unwrap_or("(unnamed)");
    println!(
        "│  {}{}│",
        display_name.bold(),
        " ".repeat(inner.saturating_sub(display_name.len() + 2))
    );

    // Provider + task count
    let info = format!("{} {} / {}", icons::provider(), provider, model);
    let task_word = if task_count == 1 { "task" } else { "tasks" };
    let layer_word = if layer_count == 1 { "layer" } else { "layers" };
    let tasks = format!(
        "{} {} · {} {}",
        task_count, task_word, layer_count, layer_word
    );
    let pad = inner.saturating_sub(stripped_len(&info) + tasks.len() + 4);
    println!("│  {}{}{} │", info, " ".repeat(pad), tasks.dimmed());

    // Generation ID
    let short_id: String = generation_id.chars().take(8).collect();
    let gen = format!("gen:{}", short_id);
    println!(
        "│  {}{}│",
        gen.dimmed(),
        " ".repeat(inner.saturating_sub(gen.len() + 2))
    );

    println!("│{}│", " ".repeat(inner));
    println!("╰{}╯", border.dimmed());
    println!();
}

/// Print a one-line DAG summary showing the execution flow.
///
/// ```text
/// DAG: fetch_data → [summarize, translate] → review  (4 tasks, 2 parallel)
/// ```
pub fn print_dag_summary(task_names: &[&str], depths: &HashMap<&str, usize>) {
    if task_names.is_empty() {
        return;
    }

    // Group tasks by layer
    let max_layer = depths.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<&str>> = vec![Vec::new(); max_layer + 1];
    for &name in task_names {
        if let Some(&layer) = depths.get(name) {
            layers[layer].push(name);
        }
    }

    // Count parallel tasks
    let parallel_count = layers
        .iter()
        .filter(|l| l.len() > 1)
        .flat_map(|l| l.iter())
        .count();

    // Build flow string
    let parts: Vec<String> = layers
        .iter()
        .filter(|l| !l.is_empty())
        .map(|layer| {
            if layer.len() == 1 {
                layer[0].to_string()
            } else {
                format!("[{}]", layer.join(", "))
            }
        })
        .collect();

    let flow = parts.join(" \u{2192} "); // →

    let stats = if parallel_count > 0 {
        format!("({} tasks, {} parallel)", task_names.len(), parallel_count)
    } else {
        format!("({} tasks)", task_names.len())
    };

    println!("  {} {}  {}", "DAG:".dimmed(), flow, stats.dimmed());
    println!();
}
