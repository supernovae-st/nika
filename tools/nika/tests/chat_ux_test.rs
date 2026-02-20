//! Chat UX Feature Tests
//!
//! Tests all Chat UX v2 widgets and features:
//! - SessionContextBar
//! - ActivityStack (hot/warm/cold)
//! - CommandPalette (keyboard navigation)
//! - InferStreamBox (streaming display)
//! - McpCallBox (inline MCP calls)

#![cfg(feature = "tui")]

use nika::tui::widgets::{
    default_commands, ActivityItem, ActivityStack, ActivityTemp, CommandPaletteState, InferStatus,
    InferStreamBox, InferStreamData, McpCallBox, McpCallData, McpCallStatus, McpServerInfo,
    McpStatus, PaletteCommand, SessionContext, SessionContextBar,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use std::time::Duration;

// ============================================================================
// SESSION CONTEXT TESTS
// ============================================================================

#[test]
fn test_session_context_new() {
    let ctx = SessionContext::new();
    assert_eq!(ctx.tokens_used, 0);
    assert_eq!(ctx.total_cost, 0.0);
    assert_eq!(ctx.token_limit, 200_000);
}

#[test]
fn test_session_context_add_tokens() {
    let mut ctx = SessionContext::new();
    ctx.add_tokens(1000, 500);
    assert_eq!(ctx.tokens_used, 1500); // input + output combined
    assert!(ctx.total_cost > 0.0);
}

#[test]
fn test_session_context_usage_percent() {
    let mut ctx = SessionContext::new();
    ctx.tokens_used = 50_000;
    ctx.token_limit = 200_000;
    assert_eq!(ctx.usage_percent(), 25.0);
}

#[test]
fn test_session_context_bar_compact_rendering() {
    let ctx = SessionContext::new();
    let bar = SessionContextBar::new(&ctx).compact();

    let area = Rect::new(0, 0, 80, 3);
    let mut buffer = Buffer::empty(area);
    bar.render(area, &mut buffer);

    // Buffer should have content
    let content = buffer_to_string(&buffer);
    assert!(!content.trim().is_empty());
}

#[test]
fn test_mcp_server_info_new() {
    let info = McpServerInfo::new("novanet");
    assert_eq!(info.name, "novanet");
    assert_eq!(info.status, McpStatus::Cold);
    assert_eq!(info.call_count, 0);
}

#[test]
fn test_mcp_server_info_record_call() {
    let mut info = McpServerInfo::new("novanet");
    info.record_call();
    assert_eq!(info.status, McpStatus::Hot);
    assert_eq!(info.call_count, 1);
    assert!(info.last_call.is_some());
}

// ============================================================================
// MCP STATUS TESTS
// ============================================================================

#[test]
fn test_mcp_status_variants() {
    assert_eq!(format!("{:?}", McpStatus::Hot), "Hot");
    assert_eq!(format!("{:?}", McpStatus::Warm), "Warm");
    assert_eq!(format!("{:?}", McpStatus::Cold), "Cold");
    assert_eq!(format!("{:?}", McpStatus::Error), "Error");
}

#[test]
fn test_mcp_status_default() {
    let status = McpStatus::default();
    assert_eq!(status, McpStatus::Cold);
}

#[test]
fn test_mcp_status_indicators() {
    let (ind, _) = McpStatus::Hot.indicator();
    assert!(ind.contains("🟢"));

    let (ind, _) = McpStatus::Cold.indicator();
    assert!(ind.contains("⚪"));
}

#[test]
fn test_mcp_status_labels() {
    assert_eq!(McpStatus::Hot.label(), "hot");
    assert_eq!(McpStatus::Warm.label(), "warm");
    assert_eq!(McpStatus::Cold.label(), "cold");
    assert_eq!(McpStatus::Error.label(), "error");
}

// ============================================================================
// ACTIVITY STACK TESTS
// ============================================================================

#[test]
fn test_activity_item_hot() {
    let item = ActivityItem::hot("task-1", "infer");
    assert_eq!(item.id, "task-1");
    assert_eq!(item.verb, "infer");
    assert_eq!(item.temp, ActivityTemp::Hot);
    assert!(item.started.is_some());
}

#[test]
fn test_activity_item_warm() {
    let item = ActivityItem::warm("task-2", "exec", Duration::from_secs(5));
    assert_eq!(item.temp, ActivityTemp::Warm);
    assert_eq!(item.duration, Some(Duration::from_secs(5)));
}

#[test]
fn test_activity_item_queued() {
    let item = ActivityItem::queued("task-3", "fetch", "task-1");
    assert_eq!(item.temp, ActivityTemp::Queued);
    assert_eq!(item.waiting_on, Some("task-1".to_string()));
}

#[test]
fn test_activity_item_with_tokens() {
    let item = ActivityItem::hot("task-1", "infer").with_tokens(100, 50);
    assert_eq!(item.tokens, Some((100, 50)));
}

#[test]
fn test_activity_item_with_detail() {
    let item = ActivityItem::warm("task-1", "exec", Duration::from_secs(1))
        .with_detail("Build successful");
    assert_eq!(item.detail, Some("Build successful".to_string()));
}

#[test]
fn test_activity_item_elapsed() {
    let item = ActivityItem::hot("task-1", "infer");
    let elapsed = item.elapsed();
    assert!(elapsed.is_some());
    assert!(elapsed.unwrap() < Duration::from_secs(1));
}

#[test]
fn test_activity_temp_headers() {
    let (hot_header, _) = ActivityTemp::Hot.header();
    assert!(hot_header.contains("HOT"));

    let (warm_header, _) = ActivityTemp::Warm.header();
    assert!(warm_header.contains("WARM"));

    let (queued_header, _) = ActivityTemp::Queued.header();
    assert!(queued_header.contains("QUEUED"));
}

#[test]
fn test_activity_stack_rendering() {
    let items = vec![
        ActivityItem::hot("task-1", "infer"),
        ActivityItem::warm("task-2", "exec", Duration::from_secs(3)),
        ActivityItem::queued("task-3", "fetch", "task-1"),
    ];

    let stack = ActivityStack::new(&items).frame(0);

    let area = Rect::new(0, 0, 60, 15);
    let mut buffer = Buffer::empty(area);
    stack.render(area, &mut buffer);

    let content = buffer_to_string(&buffer);
    assert!(content.contains("ACTIVITY"));
}

#[test]
fn test_activity_stack_empty() {
    let items: Vec<ActivityItem> = vec![];
    let stack = ActivityStack::new(&items);

    let area = Rect::new(0, 0, 60, 10);
    let mut buffer = Buffer::empty(area);
    stack.render(area, &mut buffer);

    let content = buffer_to_string(&buffer);
    assert!(!content.trim().is_empty());
}

// ============================================================================
// COMMAND PALETTE TESTS
// ============================================================================

#[test]
fn test_command_palette_state_new() {
    let state = CommandPaletteState::new();
    assert!(!state.visible);
    assert!(state.query.is_empty());
    assert_eq!(state.selected, 0);
}

#[test]
fn test_command_palette_toggle() {
    let mut state = CommandPaletteState::new();
    assert!(!state.visible);

    state.toggle();
    assert!(state.visible);

    state.toggle();
    assert!(!state.visible);
}

#[test]
fn test_command_palette_input_char() {
    let mut state = CommandPaletteState::new();
    state.visible = true;

    state.input_char('t');
    state.input_char('e');
    state.input_char('s');
    state.input_char('t');

    assert_eq!(state.query, "test");
}

#[test]
fn test_command_palette_backspace() {
    let mut state = CommandPaletteState::new();
    state.visible = true;
    state.query = "test".to_string();

    state.backspace();
    assert_eq!(state.query, "tes");

    state.backspace();
    state.backspace();
    state.backspace();
    assert_eq!(state.query, "");

    // Backspace on empty should not panic
    state.backspace();
    assert_eq!(state.query, "");
}

#[test]
fn test_command_palette_select_next() {
    let mut state = CommandPaletteState::new();
    state.visible = true;
    state.commands = default_commands();

    assert_eq!(state.selected, 0);
    state.select_next();
    assert_eq!(state.selected, 1);
}

#[test]
fn test_command_palette_select_prev() {
    let mut state = CommandPaletteState::new();
    state.visible = true;
    state.commands = default_commands();
    state.selected = 2;

    state.select_prev();
    assert_eq!(state.selected, 1);

    state.select_prev();
    assert_eq!(state.selected, 0);

    // Wraps around to last item (vim-style navigation)
    state.select_prev();
    assert_eq!(state.selected, state.filtered.len() - 1);
}

#[test]
fn test_command_palette_execute_selected() {
    let mut state = CommandPaletteState::new();
    state.visible = true;
    state.commands = default_commands();

    let cmd = state.execute_selected();
    assert!(cmd.is_some());
    assert!(!state.visible); // Should close after execution
}

#[test]
fn test_command_palette_filtering() {
    let state = CommandPaletteState::new();

    // Should have default commands filtered
    assert!(!state.filtered.is_empty());
    // Filtered should contain valid indices
    for &idx in &state.filtered {
        assert!(idx < state.commands.len());
    }
}

#[test]
fn test_palette_command_creation() {
    // Use builder pattern
    let cmd = PaletteCommand::new("test", "Test Command", "A test command").with_shortcut("⌘T");

    assert_eq!(cmd.id, "test");
    assert_eq!(cmd.shortcut, Some("⌘T".to_string()));
    assert_eq!(cmd.label, "Test Command");
}

#[test]
fn test_default_commands() {
    let commands = default_commands();
    assert!(!commands.is_empty());
    assert!(commands.len() >= 3);
}

// ============================================================================
// INFER STREAM BOX TESTS
// ============================================================================

#[test]
fn test_infer_stream_data_new() {
    let data = InferStreamData::new("claude-sonnet-4-20250514");
    assert_eq!(data.model, "claude-sonnet-4-20250514");
    assert!(data.content.is_empty());
    assert_eq!(data.status, InferStatus::Running);
}

#[test]
fn test_infer_stream_data_append() {
    let mut data = InferStreamData::new("claude-sonnet-4-20250514");
    data.append_content("Hello ");
    data.append_content("World!");
    assert_eq!(data.content, "Hello World!");
}

#[test]
fn test_infer_stream_data_status_transitions() {
    let mut data = InferStreamData::new("claude-sonnet-4-20250514");
    assert_eq!(data.status, InferStatus::Running);

    data.complete();
    assert_eq!(data.status, InferStatus::Complete);
}

#[test]
fn test_infer_stream_data_with_tokens() {
    let data = InferStreamData::new("claude-sonnet-4-20250514").with_tokens(100, 50);
    assert_eq!(data.tokens_in, 100);
    assert_eq!(data.tokens_out, 50);
}

#[test]
fn test_infer_stream_progress() {
    let data = InferStreamData::new("model")
        .with_tokens(0, 500)
        .with_max_tokens(2000);
    assert_eq!(data.progress_percent(), 25.0);
}

#[test]
fn test_infer_stream_box_rendering() {
    let data = InferStreamData::new("claude-sonnet-4-20250514")
        .with_content("This is a streaming response...");

    let stream_box = InferStreamBox::new(&data);

    let area = Rect::new(0, 0, 60, 10);
    let mut buffer = Buffer::empty(area);
    stream_box.render(area, &mut buffer);

    let content = buffer_to_string(&buffer);
    assert!(!content.trim().is_empty());
}

#[test]
fn test_infer_status_display() {
    assert_eq!(format!("{:?}", InferStatus::Running), "Running");
    assert_eq!(format!("{:?}", InferStatus::Complete), "Complete");
    assert_eq!(format!("{:?}", InferStatus::Failed), "Failed");
}

// ============================================================================
// MCP CALL BOX TESTS
// ============================================================================

#[test]
fn test_mcp_call_data_new() {
    let data = McpCallData::new("novanet_describe", "novanet");
    assert_eq!(data.tool, "novanet_describe");
    assert_eq!(data.server, "novanet");
    assert_eq!(data.status, McpCallStatus::Running);
}

#[test]
fn test_mcp_call_data_with_params() {
    let data =
        McpCallData::new("novanet_describe", "novanet").with_params(r#"{"entity": "qr-code"}"#);
    assert!(!data.params.is_empty());
}

#[test]
fn test_mcp_call_data_with_result() {
    let data =
        McpCallData::new("novanet_describe", "novanet").with_result(r#"{"name": "QR Code"}"#);
    assert!(data.result.is_some());
    assert_eq!(data.status, McpCallStatus::Success);
}

#[test]
fn test_mcp_call_status_transitions() {
    let data = McpCallData::new("tool", "server");
    assert_eq!(data.status, McpCallStatus::Running);

    let data = McpCallData::new("tool", "server").with_result("ok");
    assert_eq!(data.status, McpCallStatus::Success);

    let data = McpCallData::new("tool", "server").with_error("failed");
    assert_eq!(data.status, McpCallStatus::Failed);
}

#[test]
fn test_mcp_call_data_error_state() {
    let data = McpCallData::new("novanet_describe", "novanet").with_error("Connection refused");
    assert_eq!(data.status, McpCallStatus::Failed);
    assert_eq!(data.error, Some("Connection refused".to_string()));
}

#[test]
fn test_mcp_call_box_rendering() {
    let data =
        McpCallData::new("novanet_describe", "novanet").with_params(r#"{"entity": "qr-code"}"#);

    let call_box = McpCallBox::new(&data);

    let area = Rect::new(0, 0, 60, 6);
    let mut buffer = Buffer::empty(area);
    call_box.render(area, &mut buffer);

    let content = buffer_to_string(&buffer);
    assert!(!content.trim().is_empty());
}

#[test]
fn test_mcp_call_box_with_duration() {
    let data = McpCallData::new("novanet_describe", "novanet")
        .with_result(r#"{"name": "QR Code"}"#)
        .with_duration(Duration::from_millis(150));

    assert_eq!(data.duration, Duration::from_millis(150));
}

// ============================================================================
// INTEGRATION TESTS - Widget Combinations
// ============================================================================

#[test]
fn test_chat_ux_full_layout() {
    let ctx = SessionContext::new();
    let activities = vec![
        ActivityItem::hot("task-1", "infer"),
        ActivityItem::queued("task-2", "exec", "task-1"),
    ];
    let mut palette_state = CommandPaletteState::new();
    palette_state.commands = default_commands();

    let area = Rect::new(0, 0, 100, 40);
    let mut buffer = Buffer::empty(area);

    // Session bar at top
    let bar_area = Rect::new(0, 0, 100, 3);
    SessionContextBar::new(&ctx)
        .compact()
        .render(bar_area, &mut buffer);

    // Activity stack in sidebar
    let stack_area = Rect::new(0, 3, 40, 15);
    ActivityStack::new(&activities).render(stack_area, &mut buffer);

    // Verify buffer has content
    let content = buffer_to_string(&buffer);
    assert!(!content.chars().all(|c| c == ' ' || c == '\n'));
}

#[test]
fn test_activity_transitions() {
    let mut activities: Vec<ActivityItem> = vec![];

    // Start task
    activities.push(ActivityItem::hot("task-1", "infer"));
    assert_eq!(activities.len(), 1);
    assert_eq!(activities[0].temp, ActivityTemp::Hot);

    // Complete task, add to warm
    activities[0] = ActivityItem::warm("task-1", "infer", Duration::from_secs(2));

    // Start next task
    activities.push(ActivityItem::hot("task-2", "exec"));

    assert_eq!(activities[0].temp, ActivityTemp::Warm);
    assert_eq!(activities[1].temp, ActivityTemp::Hot);
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn buffer_to_string(buffer: &Buffer) -> String {
    let mut result = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = buffer.cell((x, y)).unwrap();
            result.push_str(cell.symbol());
        }
        result.push('\n');
    }
    result
}
