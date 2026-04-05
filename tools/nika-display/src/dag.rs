//! Static DAG renderer -- printed once, no ANSI cursor movement.
//!
//! Shows task names with verb icons connected by arrows.
//! Groups tasks by topological layer (parallel tasks on same line).

use colored::Colorize;

use crate::icons;

/// A task in the static DAG, positioned by topological layer.
pub struct StaticDagTask {
    pub id: String,
    pub verb: String,
    pub layer: usize,
}

/// A directed edge between two tasks.
pub struct StaticDagEdge {
    pub from: String,
    pub to: String,
}

/// Print a compact static DAG to stdout.
///
/// Tasks are grouped by topological layer (parallel tasks on the same line).
/// Arrows between layers are rendered as dimmed vertical connectors.
///
/// ```text
///   DAG  6 tasks . 3 layers . 5 edges
///
///    ✧ research       ⎈ scrape       ☄ fetch_api
///         │               │               │
///         ▾               ▾               ▾
///    ❋ analyze        ✧ summarize
///         │               │
///         ▾               ▾
///    ⊛ publish
/// ```
pub fn print_static_dag(tasks: &[StaticDagTask], edges: &[StaticDagEdge]) {
    if tasks.is_empty() {
        return;
    }

    // Compute summary
    let max_layer = tasks.iter().map(|t| t.layer).max().unwrap_or(0);
    let num_layers = max_layer + 1;

    let tc = tasks.len();
    let ec = edges.len();
    let summary = format!(
        "DAG  {} {} \u{00B7} {} {} \u{00B7} {} {}",
        tc,
        if tc == 1 { "task" } else { "tasks" },
        num_layers,
        if num_layers == 1 { "layer" } else { "layers" },
        ec,
        if ec == 1 { "edge" } else { "edges" },
    );
    println!("  {}", summary.cyan().bold());
    println!();

    // Render one line per layer with task names, then vertical connectors
    for layer_idx in 0..=max_layer {
        let layer_tasks: Vec<&StaticDagTask> =
            tasks.iter().filter(|t| t.layer == layer_idx).collect();

        // Build the task line: "icon name" separated by generous spacing
        let line: Vec<String> = layer_tasks
            .iter()
            .map(|t| format!("{} {}", icons::verb_plain(&t.verb), t.id))
            .collect();

        println!("   {}", line.join("       "));

        // Print vertical connectors to next layer (if not last layer)
        if layer_idx < max_layer {
            // Find which tasks in this layer have outgoing edges
            let has_outgoing: Vec<bool> = layer_tasks
                .iter()
                .map(|t| edges.iter().any(|e| e.from == t.id))
                .collect();

            if has_outgoing.iter().any(|&b| b) {
                // Build connector lines matching task positions
                let mut pipe_line = String::from("   ");
                let mut arrow_line = String::from("   ");

                for (i, task) in layer_tasks.iter().enumerate() {
                    // The icon is 1 char, then space, then task id
                    // Center the connector under the task name
                    let task_label_len = task.id.len() + 2; // icon + space + id
                    let center = task_label_len / 2;

                    if has_outgoing[i] {
                        pipe_line.push_str(&" ".repeat(center));
                        pipe_line.push_str(&format!("{}", "\u{2502}".dimmed())); // │
                        pipe_line.push_str(&" ".repeat(task_label_len.saturating_sub(center + 1)));
                    } else {
                        pipe_line.push_str(&" ".repeat(task_label_len));
                    }

                    if has_outgoing[i] {
                        arrow_line.push_str(&" ".repeat(center));
                        arrow_line.push_str(&format!("{}", "\u{25BE}".dimmed())); // ▾
                        arrow_line.push_str(&" ".repeat(task_label_len.saturating_sub(center + 1)));
                    } else {
                        arrow_line.push_str(&" ".repeat(task_label_len));
                    }

                    // Add spacing between tasks (matching the join separator)
                    if i < layer_tasks.len() - 1 {
                        pipe_line.push_str("       ");
                        arrow_line.push_str("       ");
                    }
                }

                println!("{}", pipe_line);
                println!("{}", arrow_line);
            }
        }
    }
    println!();
}
