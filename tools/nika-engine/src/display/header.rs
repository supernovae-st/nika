//! Header renderer — workflow info box + static DAG.

use colored::Colorize;

use crate::display::colors::stripped_len;
use crate::display::icons;

/// Print the new rounded-corner header box.
///
/// ```text
/// ╭───────────────────────────────────────────────────────────╮
/// │                                                           │
/// │  N I K A                                        v0.38.0   │
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
    let tasks = format!("{} tasks · {} layers", task_count, layer_count);
    let pad = inner.saturating_sub(stripped_len(&info) + tasks.len() + 4);
    println!("│  {}{}{} │", info, " ".repeat(pad), tasks.dimmed());

    // Generation ID
    let gen = format!("gen:{}", &generation_id[..generation_id.len().min(8)]);
    println!(
        "│  {}{}│",
        gen.dimmed(),
        " ".repeat(inner.saturating_sub(gen.len() + 2))
    );

    println!("│{}│", " ".repeat(inner));
    println!("╰{}╯", border.dimmed());
    println!();
}
