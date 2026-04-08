// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Debug script to visualize DAG rendering

use nika::tui::widgets::{DagAscii, NodeBoxData, NodeBoxMode};
use nika::tui::{TaskStatus, VerbColor};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
use rustc_hash::FxHashMap;

fn main() {
    // Create test nodes
    let nodes = vec![
        NodeBoxData::new("task1", VerbColor::Infer)
            .with_status(TaskStatus::Pending)
            .with_estimate("~1s"),
        NodeBoxData::new("task2", VerbColor::Exec)
            .with_status(TaskStatus::Pending)
            .with_estimate("~2s"),
    ];

    // Dependencies: task2 depends on task1
    let mut deps = FxHashMap::default();
    deps.insert("task2".to_string(), vec!["task1".to_string()]);

    let widget = DagAscii::new(&nodes)
        .with_dependencies(deps)
        .mode(NodeBoxMode::Minimal);

    // Create a buffer and render
    let area = Rect::new(0, 0, 60, 15);
    let mut buffer = Buffer::empty(area);
    widget.render(area, &mut buffer);

    // Print the buffer content
    println!(
        "DAG ASCII Render Output ({} x {}):",
        area.width, area.height
    );
    println!("{}", "=".repeat(area.width as usize));

    for y in 0..area.height {
        let mut line = String::new();
        for x in 0..area.width {
            let cell = buffer.cell((x, y)).unwrap();
            line.push_str(cell.symbol());
        }
        println!("|{}|", line);
    }
    println!("{}", "=".repeat(area.width as usize));
}
