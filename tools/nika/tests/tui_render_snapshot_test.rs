// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TUI Render Snapshot Tests
//!
//! Visual rendering tests for widgets actively used in TUI views.
//! Uses ratatui TestBackend and insta snapshots.
//!
//! Run with: `cargo test --test tui_render_snapshot_test --features tui`

#![cfg(feature = "tui")]

use insta::assert_snapshot;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use nika::tui::widgets::{Timeline, TimelineEntry};
use nika::tui::TaskStatus;

// =============================================================================
// HELPERS
// =============================================================================

/// Render a widget to a string for snapshot testing
fn render_widget_to_string<W: Widget>(widget: W, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    widget.render(area, &mut buffer);
    buffer_to_string(&buffer)
}

/// Convert buffer to string representation
fn buffer_to_string(buffer: &Buffer) -> String {
    let mut result = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            result.push_str(cell.symbol());
        }
        result.push('\n');
    }
    result
}

// =============================================================================
// TIMELINE WIDGET TESTS (used in home.rs)
// =============================================================================

#[test]
fn test_timeline_empty() {
    let entries: Vec<TimelineEntry> = vec![];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_single_pending() {
    let entries = vec![TimelineEntry::new("step1", TaskStatus::Pending)];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_mixed_statuses() {
    let entries = vec![
        TimelineEntry::new("init", TaskStatus::Success).with_duration(100),
        TimelineEntry::new("build", TaskStatus::Success).with_duration(500),
        TimelineEntry::new("test", TaskStatus::Running).current(),
        TimelineEntry::new("deploy", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(1500).with_frame(0);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_all_completed() {
    let entries = vec![
        TimelineEntry::new("fetch", TaskStatus::Success).with_duration(200),
        TimelineEntry::new("parse", TaskStatus::Success).with_duration(150),
        TimelineEntry::new("infer", TaskStatus::Success).with_duration(800),
        TimelineEntry::new("save", TaskStatus::Success).with_duration(50),
    ];
    let timeline = Timeline::new(&entries).elapsed(1200);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_with_failure() {
    let entries = vec![
        TimelineEntry::new("setup", TaskStatus::Success),
        TimelineEntry::new("build", TaskStatus::Failed),
        TimelineEntry::new("test", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(500);
    let output = render_widget_to_string(timeline, 60, 4);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_with_breakpoint() {
    let entries = vec![
        TimelineEntry::new("step1", TaskStatus::Success),
        TimelineEntry::new("step2", TaskStatus::Paused).with_breakpoint(true),
        TimelineEntry::new("step3", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(300);
    let output = render_widget_to_string(timeline, 60, 5);
    assert_snapshot!(output);
}

#[test]
fn test_timeline_many_tasks() {
    let entries = (1..=10)
        .map(|i| {
            let status = if i <= 7 {
                TaskStatus::Success
            } else if i == 8 {
                TaskStatus::Running
            } else {
                TaskStatus::Pending
            };
            TimelineEntry::new(format!("t{}", i), status)
        })
        .collect::<Vec<_>>();
    let timeline = Timeline::new(&entries).elapsed(5000);
    let output = render_widget_to_string(timeline, 80, 4);
    assert_snapshot!(output);
}

// =============================================================================
// EDGE CASES
// =============================================================================

#[test]
fn test_minimum_timeline() {
    let entries = vec![TimelineEntry::new("x", TaskStatus::Running)];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 10, 3);
    assert_snapshot!("minimum_timeline", output);
}

#[test]
fn test_long_task_names_truncation() {
    let entries = vec![TimelineEntry::new(
        "very_long_task_name_that_should_be_truncated",
        TaskStatus::Running,
    )];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 30, 4);
    assert_snapshot!(output);
}

// =============================================================================
// TERMINAL SIZE RESPONSIVENESS
// =============================================================================

#[test]
fn test_timeline_80x24() {
    let entries = vec![
        TimelineEntry::new("t1", TaskStatus::Success),
        TimelineEntry::new("t2", TaskStatus::Running),
        TimelineEntry::new("t3", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 80, 4);
    assert_snapshot!("timeline_80x24", output);
}

#[test]
fn test_wide_terminal_120() {
    let entries = vec![
        TimelineEntry::new("init", TaskStatus::Success),
        TimelineEntry::new("fetch", TaskStatus::Success),
        TimelineEntry::new("parse", TaskStatus::Success),
        TimelineEntry::new("validate", TaskStatus::Running),
        TimelineEntry::new("transform", TaskStatus::Pending),
        TimelineEntry::new("save", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries).elapsed(10000);
    let output = render_widget_to_string(timeline, 120, 4);
    assert_snapshot!(output);
}

#[test]
fn test_narrow_terminal_40() {
    let entries = vec![
        TimelineEntry::new("a", TaskStatus::Success),
        TimelineEntry::new("b", TaskStatus::Running),
        TimelineEntry::new("c", TaskStatus::Pending),
    ];
    let timeline = Timeline::new(&entries);
    let output = render_widget_to_string(timeline, 40, 4);
    assert_snapshot!(output);
}
