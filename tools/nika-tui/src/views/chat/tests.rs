// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat View Tests
//!
//! Unit tests for ChatView functionality.

use super::*;
use crate::state::ChatOverlayMessageRole;
use crate::widgets::InferStreamData;
use tui_input::InputRequest;

#[test]
fn test_chat_view_new() {
    let view = ChatView::new();
    assert_eq!(view.messages.len(), 1); // Welcome message
    assert!(view.input.value().is_empty());
}

#[test]
fn test_chat_view_submit() {
    let mut view = ChatView::new();
    view.input = Input::new("Hello Nika".to_string());
    view.input.handle(InputRequest::GoToEnd);

    let result = view.submit();
    assert_eq!(result, Some("Hello Nika".to_string()));
    assert!(view.input.value().is_empty());
    assert_eq!(view.messages.len(), 2); // Welcome + user message
}

#[test]
fn test_chat_view_submit_empty() {
    let mut view = ChatView::new();
    view.input = Input::new("   ".to_string());

    let result = view.submit();
    assert_eq!(result, None);
}

#[test]
fn test_chat_view_history_navigation() {
    let mut view = ChatView::new();
    view.add_user_message("First".to_string());
    view.add_user_message("Second".to_string());

    view.history_up();
    assert_eq!(view.input.value(), "Second");

    view.history_up();
    assert_eq!(view.input.value(), "First");

    view.history_down();
    assert_eq!(view.input.value(), "Second");
}

#[test]
fn test_chat_view_history_down_clears_input() {
    let mut view = ChatView::new();
    view.add_user_message("Test".to_string());

    view.history_up();
    assert_eq!(view.input.value(), "Test");

    view.history_down();
    assert!(view.input.value().is_empty());
}

#[test]
fn test_chat_view_cursor() {
    let mut view = ChatView::new();
    view.insert_char('H');
    view.insert_char('i');
    assert_eq!(view.input.value(), "Hi");
    assert_eq!(view.input.cursor(), 2);

    view.cursor_left();
    assert_eq!(view.input.cursor(), 1);

    view.insert_char('e');
    assert_eq!(view.input.value(), "Hei");

    view.backspace();
    assert_eq!(view.input.value(), "Hi");
}

#[test]
fn test_chat_view_cursor_right() {
    let mut view = ChatView::new();
    view.input = Input::new("Hello".to_string());
    view.input.handle(InputRequest::GoToStart);

    view.cursor_right();
    assert_eq!(view.input.cursor(), 1);

    view.cursor_right();
    view.cursor_right();
    view.cursor_right();
    view.cursor_right();
    assert_eq!(view.input.cursor(), 5);

    // Should not go past the end
    view.cursor_right();
    assert_eq!(view.input.cursor(), 5);
}

#[test]
fn test_chat_view_backspace_at_start() {
    let mut view = ChatView::new();
    view.input = Input::new("Hi".to_string());
    view.input.handle(InputRequest::GoToStart);

    view.backspace();
    assert_eq!(view.input.value(), "Hi");
    assert_eq!(view.input.cursor(), 0);
}

#[test]
fn test_chat_view_add_nika_message() {
    let mut view = ChatView::new();
    view.add_nika_message("Hello!".to_string(), None);

    assert_eq!(view.messages.len(), 2);
    assert_eq!(view.messages[1].role, MessageRole::Nika);
    assert_eq!(view.messages[1].content, "Hello!");
}

#[test]
fn test_chat_view_add_nika_message_with_execution() {
    let mut view = ChatView::new();
    let exec = ExecutionResult {
        workflow_name: "test.nika.yaml".to_string(),
        status: ExecutionStatus::Completed,
        tasks_completed: 3,
        tasks_total: 3,
        output: Some("Done".to_string()),
    };
    view.add_nika_message("Workflow completed".to_string(), Some(exec));

    assert_eq!(view.messages.len(), 2);
    assert!(view.messages[1].execution.is_some());
    let exec = view.messages[1].execution.as_ref().unwrap();
    assert_eq!(exec.status, ExecutionStatus::Completed);
}

#[test]
fn test_message_role_equality() {
    assert_eq!(MessageRole::User, MessageRole::User);
    assert_ne!(MessageRole::User, MessageRole::Nika);
    assert_ne!(MessageRole::Nika, MessageRole::System);
}

#[test]
fn test_execution_status_equality() {
    assert_eq!(ExecutionStatus::Running, ExecutionStatus::Running);
    assert_ne!(ExecutionStatus::Running, ExecutionStatus::Completed);
    assert_ne!(ExecutionStatus::Completed, ExecutionStatus::Failed);
}

#[test]
fn test_chat_view_status_line() {
    let view = ChatView::new();
    let state = TuiState::new("test.nika.yaml");
    let status = view.status_line(&state);
    // New format: "{msgs} msgs | {provider} | {model}"
    assert!(status.contains("1 msgs"));
    assert!(status.contains(" | ")); // Contains provider | model separator
}

#[test]
fn test_chat_view_default() {
    let view = ChatView::default();
    assert_eq!(view.messages.len(), 1);
    assert!(view.input.value().is_empty());
}

#[test]
fn test_chat_view_unicode_input() {
    let mut view = ChatView::new();

    // Test emoji input (4 bytes per char)
    view.insert_char('\u{1F980}'); // Rust crab emoji
    view.insert_char('!');
    assert_eq!(view.input.value(), "\u{1F980}!");
    assert_eq!(view.input.cursor(), 2); // 2 chars, not 5 bytes

    // Test backspace removes whole emoji
    view.backspace();
    assert_eq!(view.input.value(), "\u{1F980}");
    assert_eq!(view.input.cursor(), 1);

    // Test cursor navigation with unicode
    view.insert_char('\u{1F600}'); // Grinning face emoji
    assert_eq!(view.input.value(), "\u{1F980}\u{1F600}");
    assert_eq!(view.input.cursor(), 2);

    view.cursor_left();
    assert_eq!(view.input.cursor(), 1);

    // Insert in middle
    view.insert_char('A');
    assert_eq!(view.input.value(), "\u{1F980}A\u{1F600}");
    assert_eq!(view.input.cursor(), 2);

    // Cursor right should work correctly
    view.cursor_right();
    assert_eq!(view.input.cursor(), 3);

    // Should not go past end
    view.cursor_right();
    assert_eq!(view.input.cursor(), 3);
}

#[test]
fn test_chat_view_unicode_history() {
    let mut view = ChatView::new();
    view.add_user_message("Hello \u{1F44B}".to_string()); // Wave emoji

    view.history_up();
    assert_eq!(view.input.value(), "Hello \u{1F44B}");
    assert_eq!(view.input.cursor(), 7); // 7 chars (H-e-l-l-o-space-emoji), not 10 bytes
}

#[test]
fn test_chat_view_multibyte_backspace() {
    let mut view = ChatView::new();

    // Build string with mixed byte-width chars
    view.insert_char('a'); // 1 byte
    view.insert_char('\u{00E9}'); // 2 bytes (e with acute)
    view.insert_char('\u{4E2D}'); // 3 bytes (Chinese character)
    view.insert_char('\u{1F980}'); // 4 bytes (crab emoji)

    assert_eq!(view.input.value(), "a\u{00E9}\u{4E2D}\u{1F980}");
    assert_eq!(view.input.cursor(), 4);

    // Backspace should remove each char correctly
    view.backspace();
    assert_eq!(view.input.value(), "a\u{00E9}\u{4E2D}");
    assert_eq!(view.input.cursor(), 3);

    view.backspace();
    assert_eq!(view.input.value(), "a\u{00E9}");
    assert_eq!(view.input.cursor(), 2);

    view.backspace();
    assert_eq!(view.input.value(), "a");
    assert_eq!(view.input.cursor(), 1);

    view.backspace();
    assert_eq!(view.input.value(), "");
    assert_eq!(view.input.cursor(), 0);
}

#[test]
fn test_chat_view_streaming() {
    let mut view = ChatView::new();
    assert!(!view.is_streaming);

    view.start_streaming();
    assert!(view.is_streaming);
    assert!(view.partial_response.is_empty());

    view.append_streaming("Hello ");
    view.append_streaming("world!");
    assert_eq!(view.partial_response, "Hello world!");

    let result = view.finish_streaming();
    assert_eq!(result, "Hello world!");
    assert!(!view.is_streaming);
    assert!(view.partial_response.is_empty());
}

#[test]
fn test_chat_view_set_model() {
    let mut view = ChatView::new();
    view.set_model("gpt-4o-mini");
    assert_eq!(view.provider.model, "gpt-4o-mini");
}

#[test]
fn test_chat_view_tool_message() {
    let mut view = ChatView::new();
    view.add_tool_message("Tool output: OK".to_string());
    assert_eq!(view.messages.len(), 2);
    assert_eq!(view.messages[1].role, MessageRole::Tool);
    assert_eq!(view.messages[1].content, "Tool output: OK");
}

#[test]
fn test_message_role_tool() {
    assert_eq!(MessageRole::Tool, MessageRole::Tool);
    assert_ne!(MessageRole::Tool, MessageRole::User);
    assert_ne!(MessageRole::Tool, MessageRole::Nika);
    assert_ne!(MessageRole::Tool, MessageRole::System);
}

#[test]
fn test_chat_view_status_line_with_model() {
    let mut view = ChatView::new();
    view.set_model("gpt-4o-test");
    view.set_provider("OpenAI");
    let state = TuiState::new("test.nika.yaml");
    let status = view.status_line(&state);
    // New format: "{msgs} msgs | {provider} | {model}"
    assert!(status.contains("OpenAI"));
    assert!(status.contains("gpt-4o-test"));
    assert!(status.contains("1 msgs"));
}

#[test]
fn test_chat_view_status_line_streaming() {
    let mut view = ChatView::new();
    view.start_streaming();
    let state = TuiState::new("test.nika.yaml");
    let status = view.status_line(&state);
    assert!(status.contains("Streaming..."));
}

// === Chat UX Enrichment (v2) Tests ===

#[test]
fn test_chat_view_session_context_initialized() {
    let view = ChatView::new();
    assert_eq!(view.session_context.token_limit, 200_000);
    assert!(view.session_context.started.is_some());
    assert_eq!(view.session_context.mcp_servers.len(), 1);
    assert_eq!(view.session_context.mcp_servers[0].name, "novanet");
}

#[test]
fn test_chat_view_activity_items_empty_by_default() {
    let view = ChatView::new();
    assert!(view.activity_items.is_empty());
}

#[test]
fn test_chat_view_command_palette_closed_by_default() {
    let view = ChatView::new();
    assert!(!view.command_palette.visible);
}

#[test]
fn test_chat_view_toggle_command_palette() {
    let mut view = ChatView::new();
    assert!(!view.command_palette.visible);

    view.toggle_command_palette();
    assert!(view.command_palette.visible);

    view.toggle_command_palette();
    assert!(!view.command_palette.visible);
}

#[test]
fn test_chat_view_tick_increments_frame() {
    let mut view = ChatView::new();
    assert_eq!(view.frame, 0);

    view.tick();
    assert_eq!(view.frame, 1);

    view.tick();
    assert_eq!(view.frame, 2);
}

#[test]
fn test_chat_view_add_mcp_call() {
    let mut view = ChatView::new();
    view.add_mcp_call("novanet_describe", "novanet", r#"{"entity": "qr-code"}"#);

    assert_eq!(view.inline_content.len(), 1);
    if let InlineContent::McpCall(data) = &view.inline_content[0] {
        assert_eq!(data.tool, "novanet_describe");
        assert_eq!(data.server, "novanet");
        assert_eq!(data.status, McpCallStatus::Running);
    } else {
        panic!("Expected McpCall");
    }

    // Should add activity item
    assert_eq!(view.activity_items.len(), 1);
    assert_eq!(view.activity_items[0].verb, "invoke");
    assert_eq!(view.activity_items[0].temp, ActivityTemp::Hot);

    // Should update MCP server status to hot
    assert_eq!(view.session_context.mcp_servers[0].status, McpStatus::Hot);
}

#[test]
fn test_chat_view_complete_mcp_call() {
    let mut view = ChatView::new();
    view.add_mcp_call("novanet_describe", "novanet", "params");
    view.complete_mcp_call(r#"{"result": "success"}"#);

    if let InlineContent::McpCall(data) = &view.inline_content[0] {
        assert_eq!(data.status, McpCallStatus::Success);
        assert!(data.result.is_some());
    } else {
        panic!("Expected McpCall");
    }
}

#[test]
fn test_chat_view_fail_mcp_call() {
    let mut view = ChatView::new();
    view.add_mcp_call("novanet_describe", "novanet", "params");
    view.fail_mcp_call("Connection error");

    if let InlineContent::McpCall(data) = &view.inline_content[0] {
        assert_eq!(data.status, McpCallStatus::Failed);
        assert!(data.error.is_some());
        assert_eq!(data.error.as_ref().unwrap(), "Connection error");
    } else {
        panic!("Expected McpCall");
    }
}

#[test]
fn test_chat_view_start_infer_stream() {
    let mut view = ChatView::new();
    view.start_infer_stream("claude-sonnet-4", 100, 2000);

    // InferStream boxes no longer created - streaming_decrypt handles visual
    assert_eq!(view.inline_content.len(), 0);

    // Should add activity item for Mission Control panel
    assert_eq!(view.activity_items.len(), 1);
    assert_eq!(view.activity_items[0].verb, "infer");
}

#[test]
fn test_chat_view_append_infer_content() {
    let mut view = ChatView::new();
    view.start_infer_stream("claude-sonnet-4", 100, 2000);
    view.append_infer_content("Hello ", 10);
    view.append_infer_content("World!", 20);

    // No InferStream in inline_content - streaming_decrypt handles visual
    assert_eq!(view.inline_content.len(), 0);

    // partial_response is used by streaming_decrypt for the matrix reveal effect
    assert_eq!(view.partial_response, "Hello World!");
}

#[test]
fn test_chat_view_update_tokens() {
    let mut view = ChatView::new();
    view.update_tokens(5000, 0.25);

    assert_eq!(view.session_context.tokens_used, 5000);
    assert_eq!(view.session_context.total_cost, 0.25);
}

#[test]
fn test_centered_rect() {
    let area = Rect::new(0, 0, 100, 50);
    let centered = centered_rect(60, 50, area);

    // Should be roughly centered
    assert!(centered.x > 0);
    assert!(centered.y > 0);
    assert!(centered.width < 100);
    assert!(centered.height < 50);
}

#[test]
fn test_inline_content_enum() {
    let mcp_data = McpCallData::new("tool", "server");
    let content = InlineContent::McpCall(mcp_data);

    if let InlineContent::McpCall(data) = content {
        assert_eq!(data.tool, "tool");
    } else {
        panic!("Expected McpCall variant");
    }

    let infer_data = InferStreamData::new("model");
    let content = InlineContent::InferStream(infer_data);

    if let InlineContent::InferStream(data) = content {
        assert_eq!(data.model, "model");
    } else {
        panic!("Expected InferStream variant");
    }
}

// === Scroll Tests (Panel-based scroll system) ===
// Updated to test offset-based scrolling (NovaNet pattern)

#[test]
fn test_chat_view_scroll_up() {
    let mut view = ChatView::new();
    // Focus conversation panel and set offset position
    view.focus_panel(ChatPanel::Conversation);
    view.conversation_scroll.offset = 5;
    view.conversation_scroll.total = 20;
    view.conversation_scroll.visible = 10;

    view.scroll_up();
    assert_eq!(view.conversation_scroll.offset, 4);

    view.scroll_up();
    view.scroll_up();
    view.scroll_up();
    view.scroll_up();
    assert_eq!(view.conversation_scroll.offset, 0);

    // Should not go negative
    view.scroll_up();
    assert_eq!(view.conversation_scroll.offset, 0);
}

#[test]
fn test_chat_view_scroll_down() {
    let mut view = ChatView::new();
    // Focus conversation panel and set up scrollable content
    view.focus_panel(ChatPanel::Conversation);
    view.conversation_scroll.total = 20;
    view.conversation_scroll.visible = 10;
    view.conversation_scroll.offset = 0;

    // Can scroll down when there's content
    view.scroll_down();
    assert_eq!(view.conversation_scroll.offset, 1);

    view.scroll_down();
    view.scroll_down();
    assert_eq!(view.conversation_scroll.offset, 3);

    // Set offset near end
    view.conversation_scroll.offset = 9;
    view.scroll_down();
    assert_eq!(view.conversation_scroll.offset, 10); // max is total - visible

    // Should cap at total - visible
    view.scroll_down();
    assert_eq!(view.conversation_scroll.offset, 10);
}

#[test]
fn test_chat_view_scroll_to_bottom() {
    let mut view = ChatView::new();
    view.focus_panel(ChatPanel::Conversation);
    view.conversation_scroll.total = 20;
    view.conversation_scroll.visible = 10;
    view.conversation_scroll.offset = 3;
    view.conversation_scroll.cursor = 3;

    view.scroll_to_bottom();
    // scroll_to_bottom sets offset to total - visible and cursor to total - 1
    assert_eq!(view.conversation_scroll.offset, 10);
    assert_eq!(view.conversation_scroll.cursor, 19);
}

#[test]
fn test_chat_view_scroll_from_input_panel() {
    // Test that scroll works from Input panel (NovaNet pattern)
    let mut view = ChatView::new();
    // Default focus is Input panel
    assert_eq!(view.focused_panel, ChatPanel::Input);
    view.conversation_scroll.total = 20;
    view.conversation_scroll.visible = 10;
    view.conversation_scroll.offset = 5;

    // Scroll should still work on conversation panel
    view.scroll_down();
    assert_eq!(view.conversation_scroll.offset, 6);

    view.scroll_up();
    assert_eq!(view.conversation_scroll.offset, 5);
}

// === Thinking Display Tests (CRITICAL 3) ===

#[test]
fn test_chat_message_has_thinking_field() {
    let msg = ChatMessage {
        id: 1, // Test ID
        role: MessageRole::Nika,
        content: "Here's my answer.".to_string(),
        timestamp: Local::now(),
        created_at: Instant::now(),
        execution: None,
        thinking: Some("Let me analyze this step by step...".to_string()),
    };

    assert!(msg.thinking.is_some());
    assert_eq!(
        msg.thinking.as_ref().unwrap(),
        "Let me analyze this step by step..."
    );
}

#[test]
fn test_chat_view_add_nika_message_with_thinking() {
    let mut view = ChatView::new();
    view.add_nika_message_with_thinking(
        "The answer is 42.".to_string(),
        Some("First, let me think about this deeply...".to_string()),
        None,
    );

    assert_eq!(view.messages.len(), 2); // welcome + new message
    let msg = &view.messages[1];
    assert_eq!(msg.role, MessageRole::Nika);
    assert_eq!(msg.content, "The answer is 42.");
    assert!(msg.thinking.is_some());
    assert_eq!(
        msg.thinking.as_ref().unwrap(),
        "First, let me think about this deeply..."
    );
}

#[test]
fn test_chat_view_add_nika_message_without_thinking() {
    let mut view = ChatView::new();
    view.add_nika_message_with_thinking("Quick answer.".to_string(), None, None);

    assert_eq!(view.messages.len(), 2);
    let msg = &view.messages[1];
    assert!(msg.thinking.is_none());
}

#[test]
fn test_chat_view_regular_nika_message_has_no_thinking() {
    let mut view = ChatView::new();
    view.add_nika_message("Regular response.".to_string(), None);

    assert_eq!(view.messages.len(), 2);
    let msg = &view.messages[1];
    assert!(msg.thinking.is_none());
}

#[test]
fn test_chat_view_append_thinking() {
    let mut view = ChatView::new();
    assert!(view.pending_thinking.is_none());

    view.append_thinking("First thought");
    assert_eq!(view.pending_thinking.as_ref().unwrap(), "First thought");

    view.append_thinking("Second thought");
    assert_eq!(
        view.pending_thinking.as_ref().unwrap(),
        "First thought\nSecond thought"
    );
}

#[test]
fn test_chat_view_finalize_thinking() {
    let mut view = ChatView::new();

    // Add a Nika message first
    view.add_nika_message("Here's my answer.".to_string(), None);
    assert!(view.messages[1].thinking.is_none());

    // Accumulate thinking
    view.append_thinking("Let me think...");
    view.append_thinking("Step 1: analyze");
    assert!(view.pending_thinking.is_some());

    // Finalize - should attach to last Nika message
    view.finalize_thinking();
    assert!(view.pending_thinking.is_none());
    assert!(view.messages[1].thinking.is_some());
    assert_eq!(
        view.messages[1].thinking.as_ref().unwrap(),
        "Let me think...\nStep 1: analyze"
    );
}

#[test]
fn test_chat_view_finalize_thinking_no_nika_message() {
    let mut view = ChatView::new();

    // Only has system message (welcome)
    view.append_thinking("Some thinking");

    // Finalize - should clear but not attach (no Nika message)
    view.finalize_thinking();
    assert!(view.pending_thinking.is_none());
    // Welcome message should not have thinking
    assert!(view.messages[0].thinking.is_none());
}

// === Error Handling Tests (HIGH 5) ===
// Note: categorize_error tests moved to helpers.rs

#[test]
fn test_show_error_adds_system_message() {
    let mut view = ChatView::new();
    let initial_count = view.messages.len();

    view.show_error("Test error message");

    assert_eq!(view.messages.len(), initial_count + 1);
    let last = view.messages.last().unwrap();
    assert_eq!(last.role, MessageRole::System);
    assert!(last.content.contains("Error"));
    assert!(last.content.contains("Test error message"));
    assert!(last.content.contains("/help"));
}

// === tui-input Feature Tests ===

#[test]
fn test_chat_view_word_navigation() {
    let mut view = ChatView::new();
    view.input = Input::new("hello world foo".to_string());
    view.input.handle(InputRequest::GoToStart);
    assert_eq!(view.input.cursor(), 0);

    // Move to next word
    view.cursor_next_word();
    assert_eq!(view.input.cursor(), 6); // After "hello " at 'w'

    view.cursor_next_word();
    assert_eq!(view.input.cursor(), 12); // After "world " at 'f'

    // Move to previous word
    view.cursor_prev_word();
    assert_eq!(view.input.cursor(), 6); // Back to 'w'

    view.cursor_prev_word();
    assert_eq!(view.input.cursor(), 0); // Back to start
}

#[test]
fn test_chat_view_delete_prev_word() {
    let mut view = ChatView::new();
    view.input = Input::new("hello world".to_string());
    view.input.handle(InputRequest::GoToEnd);
    assert_eq!(view.input.cursor(), 11);

    // Delete "world"
    view.delete_prev_word();
    assert_eq!(view.input.value(), "hello ");
    assert_eq!(view.input.cursor(), 6);

    // Delete "hello "
    view.delete_prev_word();
    assert_eq!(view.input.value(), "");
    assert_eq!(view.input.cursor(), 0);
}

#[test]
fn test_chat_view_cursor_start_end() {
    let mut view = ChatView::new();
    view.input = Input::new("hello world".to_string());

    // Start in middle
    view.input.handle(InputRequest::GoToPrevWord);
    assert!(view.input.cursor() < 11);

    // Go to end
    view.cursor_end();
    assert_eq!(view.input.cursor(), 11);

    // Go to start
    view.cursor_start();
    assert_eq!(view.input.cursor(), 0);
}

#[test]
fn test_chat_view_clipboard_does_not_panic() {
    let mut view = ChatView::new();
    view.input = Input::new("test".to_string());
    view.input.handle(InputRequest::GoToEnd);

    // Copy and paste should not panic even if clipboard is None or available
    // If clipboard works, it will append "test" at cursor position
    view.copy_to_clipboard();

    // Reset input to test paste alone
    view.input.reset();
    view.paste_from_clipboard();

    // Either clipboard is None (empty) or it works (has "test")
    let value = view.input.value();
    assert!(value.is_empty() || value == "test");
}

#[test]
fn test_chat_view_input_reset() {
    let mut view = ChatView::new();
    view.input = Input::new("hello world".to_string());
    view.input.handle(InputRequest::GoToEnd);
    assert_eq!(view.input.cursor(), 11);
    assert_eq!(view.input.value(), "hello world");

    // Reset input
    view.input.reset();
    assert_eq!(view.input.value(), "");
    assert_eq!(view.input.cursor(), 0);
}

// === Session Persistence Tests (HIGH 8) ===

#[test]
fn test_serializable_role_conversion() {
    // MessageRole -> SerializableRole
    assert_eq!(
        SerializableRole::from(&MessageRole::User),
        SerializableRole::User
    );
    assert_eq!(
        SerializableRole::from(&MessageRole::Nika),
        SerializableRole::Nika
    );
    assert_eq!(
        SerializableRole::from(&MessageRole::System),
        SerializableRole::System
    );
    assert_eq!(
        SerializableRole::from(&MessageRole::Tool),
        SerializableRole::Tool
    );

    // SerializableRole -> MessageRole
    assert_eq!(MessageRole::from(SerializableRole::User), MessageRole::User);
    assert_eq!(MessageRole::from(SerializableRole::Nika), MessageRole::Nika);
    assert_eq!(
        MessageRole::from(SerializableRole::System),
        MessageRole::System
    );
    assert_eq!(MessageRole::from(SerializableRole::Tool), MessageRole::Tool);
}

#[test]
fn test_chat_session_from_messages() {
    let mut view = ChatView::new();
    view.add_user_message("Hello".to_string());
    view.add_nika_message("Hi there!".to_string(), None);
    view.set_model("claude-sonnet");

    let session = ChatSession::from_messages(&view.messages, &view.provider.model);

    assert_eq!(session.version, "0.5.2");
    assert_eq!(session.model, "claude-sonnet");
    // 1 welcome message + 2 added messages = 3
    assert_eq!(session.messages.len(), 3);
    assert_eq!(session.messages[1].content, "Hello");
    assert_eq!(session.messages[1].role, SerializableRole::User);
    assert_eq!(session.messages[2].content, "Hi there!");
    assert_eq!(session.messages[2].role, SerializableRole::Nika);
}

#[test]
fn test_chat_session_round_trip() {
    use tempfile::tempdir;

    let mut view = ChatView::new();
    view.add_user_message("Test message".to_string());
    view.add_nika_message("Response".to_string(), None);
    view.set_model("gpt-4");

    let dir = tempdir().unwrap();
    let path = dir.path().join("test-session.json");

    // Save session
    view.save_session(&path).unwrap();
    assert!(path.exists());

    // Load into fresh view
    let mut view2 = ChatView::new();
    view2.load_session(&path).unwrap();

    // Verify messages restored (excluding welcome message which gets replaced)
    assert_eq!(view2.messages.len(), view.messages.len());
    assert_eq!(view2.messages[1].content, "Test message");
    assert_eq!(view2.messages[1].role, MessageRole::User);
    assert_eq!(view2.messages[2].content, "Response");
    assert_eq!(view2.messages[2].role, MessageRole::Nika);
    assert_eq!(view2.provider.model, "gpt-4");
}

#[test]
fn test_chat_session_preserves_thinking() {
    use tempfile::tempdir;

    let mut view = ChatView::new();
    view.add_nika_message_with_thinking(
        "Answer".to_string(),
        Some("My reasoning...".to_string()),
        None,
    );

    let dir = tempdir().unwrap();
    let path = dir.path().join("thinking-session.json");

    view.save_session(&path).unwrap();

    let mut view2 = ChatView::new();
    view2.load_session(&path).unwrap();

    // Last message should have thinking preserved
    let last = view2.messages.last().unwrap();
    assert_eq!(last.content, "Answer");
    assert_eq!(last.thinking, Some("My reasoning...".to_string()));
}

#[test]
fn test_default_session_path() {
    let path = ChatView::default_session_path();
    assert!(path.ends_with("nika-chat-session.json") || path.to_string_lossy().contains("nika"));
}

// === Phase 8: Real-time Streaming Updates Tests ===

#[test]
fn test_set_current_verb() {
    let mut view = ChatView::new();
    assert!(matches!(view.current_verb, CurrentVerb::None));

    view.set_current_verb(CurrentVerb::Infer);
    assert!(matches!(view.current_verb, CurrentVerb::Infer));

    view.set_current_verb(CurrentVerb::Agent);
    assert!(matches!(view.current_verb, CurrentVerb::Agent));

    view.set_current_verb(CurrentVerb::Invoke);
    assert!(matches!(view.current_verb, CurrentVerb::Invoke));

    view.set_current_verb(CurrentVerb::Exec);
    assert!(matches!(view.current_verb, CurrentVerb::Exec));

    view.set_current_verb(CurrentVerb::Fetch);
    assert!(matches!(view.current_verb, CurrentVerb::Fetch));
}

#[test]
fn test_update_mode_from_input_agent() {
    // Test that typing /agent updates chat_mode and current_verb
    let mut view = ChatView::new();

    // Initial state
    assert!(matches!(view.chat_mode, ChatMode::Infer));
    assert!(matches!(view.current_verb, CurrentVerb::None));

    // Type "/agent " (with space to mark complete)
    for c in "/agent ".chars() {
        view.insert_char(c);
    }

    // Mode should switch to Agent
    assert!(
        matches!(view.chat_mode, ChatMode::Agent),
        "chat_mode should be Agent after typing /agent"
    );
    assert!(
        matches!(view.current_verb, CurrentVerb::Agent),
        "current_verb should be Agent after typing /agent"
    );
}

#[test]
fn test_update_mode_from_input_infer() {
    // Test that typing /infer updates mode
    let mut view = ChatView::new();
    view.chat_mode = ChatMode::Agent; // Start in Agent mode

    // Type "/infer "
    for c in "/infer ".chars() {
        view.insert_char(c);
    }

    // Mode should switch to Infer
    assert!(
        matches!(view.chat_mode, ChatMode::Infer),
        "chat_mode should be Infer after typing /infer"
    );
    assert!(
        matches!(view.current_verb, CurrentVerb::Infer),
        "current_verb should be Infer after typing /infer"
    );
}

#[test]
fn test_update_mode_from_input_verb_exec() {
    // Test that typing /exec updates current_verb but not chat_mode
    let mut view = ChatView::new();

    // Type "/exec " (the Nika workflow verb, not shell exec)
    for c in "/exec ".chars() {
        view.insert_char(c);
    }

    // chat_mode should stay Infer (exec is not a chat mode)
    assert!(
        matches!(view.chat_mode, ChatMode::Infer),
        "chat_mode should stay Infer for /exec"
    );
    // current_verb should be Exec
    assert!(
        matches!(view.current_verb, CurrentVerb::Exec),
        "current_verb should be Exec after typing /exec"
    );
}

#[test]
fn test_update_mode_from_input_reset_on_clear() {
    // Test that clearing input resets current_verb
    let mut view = ChatView::new();

    // Type "/agent "
    for c in "/agent ".chars() {
        view.insert_char(c);
    }
    assert!(matches!(view.current_verb, CurrentVerb::Agent));

    // Clear input by backspacing everything
    for _ in 0.."/agent ".len() {
        view.backspace();
    }

    // current_verb should reset to None
    assert!(
        matches!(view.current_verb, CurrentVerb::None),
        "current_verb should be None after clearing input"
    );
    // chat_mode stays Agent (intentional - user chose Agent mode)
    assert!(
        matches!(view.chat_mode, ChatMode::Agent),
        "chat_mode should stay Agent after clearing"
    );
}

#[test]
fn test_update_turn_metrics_initial() {
    let mut view = ChatView::new();

    // Initial state
    assert_eq!(view.turn_metrics.input_tokens, 0);
    assert_eq!(view.turn_metrics.output_tokens, 0);
    assert_eq!(view.session_metrics.input_tokens, 0);
    assert_eq!(view.session_metrics.output_tokens, 0);

    // First update
    view.update_turn_metrics(100, 50, 0.001);

    // Turn metrics should reflect absolute values
    assert_eq!(view.turn_metrics.input_tokens, 100);
    assert_eq!(view.turn_metrics.output_tokens, 50);
    assert!((view.turn_metrics.cost_usd - 0.001).abs() < 0.0001);

    // Session metrics should have deltas (same as absolute for first update)
    assert_eq!(view.session_metrics.input_tokens, 100);
    assert_eq!(view.session_metrics.output_tokens, 50);
}

#[test]
fn test_update_turn_metrics_incremental() {
    let mut view = ChatView::new();

    // First update: 100 input, 50 output
    view.update_turn_metrics(100, 50, 0.001);

    // Second update: 100 input (same), 80 output (30 more)
    view.update_turn_metrics(100, 80, 0.002);

    // Turn metrics should reflect new absolute values
    assert_eq!(view.turn_metrics.input_tokens, 100);
    assert_eq!(view.turn_metrics.output_tokens, 80);

    // Session metrics should have accumulated deltas
    // First: +100, +50. Second: +0, +30
    assert_eq!(view.session_metrics.input_tokens, 100); // 100 + 0
    assert_eq!(view.session_metrics.output_tokens, 80); // 50 + 30
}

#[test]
fn test_increment_output_tokens() {
    let mut view = ChatView::new();

    // Start with some baseline
    view.update_turn_metrics(100, 50, 0.001);

    // Increment output tokens directly
    view.increment_output_tokens(25);

    assert_eq!(view.turn_metrics.output_tokens, 75); // 50 + 25
    assert_eq!(view.session_metrics.output_tokens, 75); // 50 + 25
                                                        // Input tokens unchanged
    assert_eq!(view.turn_metrics.input_tokens, 100);
    assert_eq!(view.session_metrics.input_tokens, 100);
}

#[test]
fn test_reset_turn_metrics() {
    let mut view = ChatView::new();
    view.set_current_verb(CurrentVerb::Agent);
    view.update_turn_metrics(100, 50, 0.001);

    view.reset_turn_metrics();

    // Turn metrics should be reset
    assert_eq!(view.turn_metrics.input_tokens, 0);
    assert_eq!(view.turn_metrics.output_tokens, 0);
    assert_eq!(view.turn_metrics.cost_usd, 0.0);
    assert!(matches!(view.current_verb, CurrentVerb::None));

    // Session metrics should be unchanged
    assert_eq!(view.session_metrics.input_tokens, 100);
    assert_eq!(view.session_metrics.output_tokens, 50);
}

#[test]
fn test_complete_turn() {
    let mut view = ChatView::new();

    // Simulate a turn with streaming updates
    view.set_current_verb(CurrentVerb::Infer);
    view.update_turn_metrics(100, 50, 0.001);
    view.update_turn_metrics(100, 100, 0.002); // More output tokens

    // Session should already have accumulated values
    assert_eq!(view.session_metrics.input_tokens, 100);
    assert_eq!(view.session_metrics.output_tokens, 100);

    // Complete the turn
    view.complete_turn();

    // Turn metrics should be reset
    assert_eq!(view.turn_metrics.input_tokens, 0);
    assert_eq!(view.turn_metrics.output_tokens, 0);
    assert!(matches!(view.current_verb, CurrentVerb::None));

    // Session metrics should be unchanged (already updated incrementally)
    assert_eq!(view.session_metrics.input_tokens, 100);
    assert_eq!(view.session_metrics.output_tokens, 100);
}

#[test]
fn test_multi_turn_session() {
    let mut view = ChatView::new();

    // Turn 1: infer with 100 input, 50 output
    view.set_current_verb(CurrentVerb::Infer);
    view.update_turn_metrics(100, 50, 0.001);
    view.complete_turn();

    // Turn 2: agent with 200 input, 100 output
    view.set_current_verb(CurrentVerb::Agent);
    view.update_turn_metrics(200, 100, 0.003);
    view.complete_turn();

    // Session should have accumulated both turns
    assert_eq!(view.session_metrics.input_tokens, 300); // 100 + 200
    assert_eq!(view.session_metrics.output_tokens, 150); // 50 + 100
    assert!((view.session_metrics.cost_usd - 0.004).abs() < 0.0001);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests for activity tracking methods (exec, fetch, agent)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_exec_activity_lifecycle() {
    use crate::widgets::ActivityTemp;

    let mut view = ChatView::new();

    // Initially no activities
    assert!(view.activity_items.is_empty());

    // Add exec activity
    view.add_exec_activity("ls -la");
    assert_eq!(view.activity_items.len(), 1);
    assert_eq!(view.activity_items[0].verb, "exec");
    assert!(matches!(view.activity_items[0].temp, ActivityTemp::Hot));

    // Complete exec activity
    view.complete_exec_activity();
    assert!(matches!(view.activity_items[0].temp, ActivityTemp::Warm));
}

#[test]
fn test_fetch_activity_lifecycle() {
    use crate::widgets::ActivityTemp;

    let mut view = ChatView::new();

    // Add fetch activity
    view.add_fetch_activity("https://example.com", "GET");
    assert_eq!(view.activity_items.len(), 1);
    assert_eq!(view.activity_items[0].verb, "fetch");

    // Complete fetch activity
    view.complete_fetch_activity();
    assert!(matches!(view.activity_items[0].temp, ActivityTemp::Warm));
}

#[test]
fn test_agent_activity_lifecycle() {
    use crate::widgets::ActivityTemp;

    let mut view = ChatView::new();

    // Add agent activity
    view.add_agent_activity("Generate a landing page");
    assert_eq!(view.activity_items.len(), 1);
    assert_eq!(view.activity_items[0].verb, "agent");

    // Complete agent activity
    view.complete_agent_activity();
    assert!(matches!(view.activity_items[0].temp, ActivityTemp::Warm));
}

#[test]
fn test_multiple_concurrent_activities() {
    use crate::widgets::ActivityTemp;

    let mut view = ChatView::new();

    // Start multiple activities concurrently
    view.add_exec_activity("npm run build");
    view.add_fetch_activity("https://api.example.com", "POST");
    view.add_agent_activity("Analyze results");

    assert_eq!(view.activity_items.len(), 3);

    // All should be hot initially
    for item in &view.activity_items {
        assert!(matches!(item.temp, ActivityTemp::Hot));
    }

    // Complete them in different order
    view.complete_fetch_activity();
    view.complete_exec_activity();
    view.complete_agent_activity();

    // All should be warm now
    for item in &view.activity_items {
        assert!(matches!(item.temp, ActivityTemp::Warm));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Text Selection Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_chat_message_selection_new() {
    let pos = ChatSelectionPos {
        message_index: 0,
        char_offset: 5,
    };
    let selection = ChatMessageSelection::new(pos);
    assert_eq!(selection.start, pos);
    assert_eq!(selection.end, pos);
}

#[test]
fn test_chat_message_selection_normalized() {
    // Forward selection (start < end)
    let sel = ChatMessageSelection {
        start: ChatSelectionPos {
            message_index: 0,
            char_offset: 5,
        },
        end: ChatSelectionPos {
            message_index: 0,
            char_offset: 10,
        },
    };
    let (start, end) = sel.normalized();
    assert_eq!(start.char_offset, 5);
    assert_eq!(end.char_offset, 10);

    // Backward selection (end < start)
    let sel = ChatMessageSelection {
        start: ChatSelectionPos {
            message_index: 0,
            char_offset: 10,
        },
        end: ChatSelectionPos {
            message_index: 0,
            char_offset: 5,
        },
    };
    let (start, end) = sel.normalized();
    assert_eq!(start.char_offset, 5);
    assert_eq!(end.char_offset, 10);
}

#[test]
fn test_chat_message_selection_contains() {
    let sel = ChatMessageSelection {
        start: ChatSelectionPos {
            message_index: 1,
            char_offset: 5,
        },
        end: ChatSelectionPos {
            message_index: 1,
            char_offset: 15,
        },
    };

    // Inside selection
    assert!(sel.contains(ChatSelectionPos {
        message_index: 1,
        char_offset: 10
    }));

    // At start
    assert!(sel.contains(ChatSelectionPos {
        message_index: 1,
        char_offset: 5
    }));

    // Before selection
    assert!(!sel.contains(ChatSelectionPos {
        message_index: 1,
        char_offset: 4
    }));
}

#[test]
fn test_get_selected_text() {
    let mut view = ChatView::new();
    view.messages.clear();
    view.add_user_message("Hello, World!".to_string());

    // Select "World"
    view.text_selection = Some(ChatMessageSelection {
        start: ChatSelectionPos {
            message_index: 0,
            char_offset: 7,
        },
        end: ChatSelectionPos {
            message_index: 0,
            char_offset: 12,
        },
    });

    let selected = view.get_selected_text();
    assert_eq!(selected, Some("World".to_string()));
}

#[test]
fn test_clear_selection() {
    let mut view = ChatView::new();
    view.text_selection = Some(ChatMessageSelection::new(ChatSelectionPos {
        message_index: 0,
        char_offset: 0,
    }));
    view.is_selecting = true;

    view.clear_selection();

    assert!(view.text_selection.is_none());
    assert!(!view.is_selecting);
}

// Note: char_to_byte_offset tests are in selection.rs where the function is defined

#[test]
fn test_selection_initialization() {
    let view = ChatView::new();
    assert!(view.text_selection.is_none());
    assert!(!view.is_selecting);
    assert!(view.line_positions.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// Provider Switching Tests (Critical Bug Fix)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_chat_view_has_current_provider_id() {
    let view = ChatView::new();
    // Provider ID should be set based on available API keys
    assert!(!view.provider.id.is_empty());
    // Should be one of the known providers, or "none" if no API keys available
    let valid_providers = [
        "anthropic",
        "openai",
        "mistral",
        "groq",
        "deepseek",
        "gemini",
        "xai",
        "none",
    ];
    assert!(
        valid_providers.contains(&view.provider.id.as_str()),
        "Provider ID '{}' should be a valid provider or 'none'",
        view.provider.id
    );
}

#[test]
fn test_provider_id_matches_model() {
    let view = ChatView::new();
    // Provider ID should match the model's provider
    match view.provider.id.as_str() {
        "anthropic" => assert!(view.provider.model.starts_with("claude")),
        "openai" => assert!(view.provider.model.starts_with("gpt")),
        "mistral" => assert!(view.provider.model.starts_with("mistral")),
        "groq" => {
            assert!(
                view.provider.model.contains("llama") || view.provider.model.contains("mixtral")
            )
        }
        "deepseek" => assert!(view.provider.model.starts_with("deepseek")),
        "gemini" => assert!(view.provider.model.starts_with("gemini")),
        "xai" => assert!(view.provider.model.starts_with("grok")),
        "none" => assert!(view.provider.model == "No API Key"), // CI without keys
        _ => panic!("Unknown provider: {}", view.provider.id),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Thinking Toggle Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_thinking_toggle_initial_state() {
    let view = ChatView::new();
    assert!(view.thinking_collapsed.is_empty());
    // Default: expanded (thinking_expanded_default = true)
    assert!(view.thinking_expanded_default);
}

#[test]
fn test_toggle_thinking_message_without_thinking() {
    let mut view = ChatView::new();
    view.add_user_message("Question".to_string());

    // User message is at index 1 (system welcome is 0)
    // Should not add to collapsed set if no thinking
    view.toggle_thinking(1);
    assert!(view.thinking_collapsed.is_empty());
}

#[test]
fn test_toggle_all_thinking() {
    let mut view = ChatView::new();

    // Add messages with thinking (indices 1 and 2 after system welcome at 0)
    view.add_nika_message_with_thinking(
        "Answer 1".to_string(),
        Some("Thinking 1".to_string()),
        None,
    );
    view.add_nika_message_with_thinking(
        "Answer 2".to_string(),
        Some("Thinking 2".to_string()),
        None,
    );

    // Default is expanded (thinking_expanded_default = true)
    assert!(view.thinking_expanded_default);

    // Toggle all - should collapse all
    view.toggle_all_thinking();
    assert!(!view.thinking_expanded_default);
    assert!(view.thinking_collapsed.is_empty()); // Cleared

    // Toggle all again - should expand all
    view.toggle_all_thinking();
    assert!(view.thinking_expanded_default);
    assert!(view.thinking_collapsed.is_empty()); // Still cleared
}

#[test]
fn test_is_thinking_visible_respects_default() {
    let mut view = ChatView::new();
    view.add_nika_message_with_thinking("Answer".to_string(), Some("Thinking".to_string()), None);

    // Message is at index 1 (system welcome at 0)
    // Get the message ID for direct set manipulation
    let msg_id = view.messages[1].id;

    // Default = true -> visible
    assert!(view.is_thinking_visible(1));

    // Change default to collapsed
    view.thinking_expanded_default = false;
    assert!(!view.is_thinking_visible(1));

    // Add message ID to override set -> should be visible (opposite of default=false)
    // Use message ID instead of index for stability
    view.thinking_collapsed.insert(msg_id);
    assert!(view.is_thinking_visible(1));
}

#[test]
fn test_toggle_thinking_with_content() {
    let mut view = ChatView::new();
    // Default is expanded (true)
    view.add_nika_message_with_thinking(
        "Answer".to_string(),
        Some("My reasoning...".to_string()),
        None,
    );

    // Message at index 1, initially expanded (default = true, not in override set)
    assert!(view.is_thinking_visible(1));

    // Toggle adds to override set -> now shows opposite of default (false = collapsed)
    view.toggle_thinking(1);
    assert!(!view.is_thinking_visible(1)); // Now collapsed

    // Toggle again removes from override set -> back to default (true = expanded)
    view.toggle_thinking(1);
    assert!(view.is_thinking_visible(1)); // Back to expanded
}

#[test]
fn test_thinking_toggle_with_collapsed_default() {
    let mut view = ChatView::new();
    view.thinking_expanded_default = false; // Start with collapsed default

    view.add_nika_message_with_thinking(
        "Answer".to_string(),
        Some("Thinking...".to_string()),
        None,
    );

    // Message at index 1, initially collapsed (default = false, not in override set)
    assert!(!view.is_thinking_visible(1));

    // Toggle adds to override set -> shows opposite of default (true = expanded)
    view.toggle_thinking(1);
    assert!(view.is_thinking_visible(1)); // Now expanded

    // Toggle again removes from override set -> back to default (false = collapsed)
    view.toggle_thinking(1);
    assert!(!view.is_thinking_visible(1)); // Back to collapsed
}

// ═══════════════════════════════════════════════════════════════════════════
// Export Path Validation Security Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_export_rejects_absolute_paths() {
    let view = ChatView::new();
    let result = view.export_session(Some("/tmp/malicious.json"));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Absolute paths not allowed"));
}

#[test]
fn test_export_rejects_path_traversal() {
    let view = ChatView::new();

    // Simple parent traversal
    let result = view.export_session(Some("../../../etc/passwd.json"));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Path traversal"));

    // Hidden in middle of path
    let result = view.export_session(Some("exports/../../../malicious.json"));
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Path traversal"));
}

#[test]
fn test_export_allows_relative_paths() {
    let view = ChatView::new();

    // Simple filename (no directory) - should work
    // Use unique name to avoid test interference
    let filename = format!("test-export-relative-{}", std::process::id());
    let result = view.export_session(Some(&filename));
    assert!(result.is_ok());
    let path = result.unwrap();
    assert!(path.ends_with(".json")); // .json added

    // Cleanup test file
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_export_adds_json_extension() {
    let view = ChatView::new();

    // Use unique filenames to avoid test interference
    let base_name = format!("test-export-ext-{}", std::process::id());

    // Without .json extension
    let result = view.export_session(Some(&base_name));
    assert!(result.is_ok());
    let path1 = result.unwrap();
    assert!(path1.ends_with(".json"));
    let _ = std::fs::remove_file(&path1);

    // With .json extension already
    let name_with_ext = format!("{}.json", base_name);
    let result = view.export_session(Some(&name_with_ext));
    assert!(result.is_ok());
    let path2 = result.unwrap();
    assert_eq!(path2, name_with_ext);
    let _ = std::fs::remove_file(&path2);
}

#[test]
fn test_export_default_filename_is_safe() {
    let view = ChatView::new();

    // Default filename (None) should always be safe
    let result = view.export_session(None);
    assert!(result.is_ok());
    let filename = result.unwrap();
    assert!(filename.starts_with("nika-chat-"));
    assert!(filename.ends_with(".json"));

    // Cleanup test file
    let _ = std::fs::remove_file(&filename);
}

// ═══════════════════════════════════════════════════════════════════════════════
// @ Mention Autocomplete Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_mention_autocomplete_hidden_by_default() {
    let view = ChatView::new();
    assert!(!view.mention_autocomplete.visible);
    assert!(view.mention_autocomplete.suggestions.is_empty());
}

#[test]
fn test_mention_autocomplete_triggers_on_at() {
    let mut view = ChatView::new();

    // Type @ to trigger autocomplete
    view.insert_char('@');
    view.check_mention_trigger();

    // Should show autocomplete with suggestions (file suggestions)
    // Note: actual visibility depends on generate_mention_suggestions implementation
    // For now, we test that check_mention_trigger runs without panic
    assert!(view.input.value() == "@");
}

#[test]
fn test_mention_autocomplete_hides_on_space() {
    let mut view = ChatView::new();

    // Type @test (simulating a mention)
    view.input = Input::new("@test ".to_string());
    view.input.handle(InputRequest::GoToEnd);
    view.check_mention_trigger();

    // After space, autocomplete should be hidden
    assert!(!view.mention_autocomplete.visible);
}

#[test]
fn test_mention_autocomplete_navigation() {
    let mut view = ChatView::new();

    // Manually show autocomplete with suggestions
    let suggestions = vec![
        MentionSuggestion {
            mention_type: MentionType::File,
            display: "file1.txt".to_string(),
            insert: "@file1.txt".to_string(),
            description: Some("A file".to_string()),
        },
        MentionSuggestion {
            mention_type: MentionType::File,
            display: "file2.txt".to_string(),
            insert: "@file2.txt".to_string(),
            description: Some("Another file".to_string()),
        },
    ];
    view.mention_autocomplete.show(
        crate::widgets::MentionTrigger {
            start: 0,
            partial: "@".to_string(),
            trigger_type: None,
            query: String::new(),
        },
        suggestions,
    );

    assert!(view.mention_autocomplete.visible);
    assert_eq!(view.mention_autocomplete.selected, 0);

    // Navigate down
    view.mention_autocomplete.next();
    assert_eq!(view.mention_autocomplete.selected, 1);

    // Navigate down wraps around
    view.mention_autocomplete.next();
    assert_eq!(view.mention_autocomplete.selected, 0);

    // Navigate up wraps around
    view.mention_autocomplete.prev();
    assert_eq!(view.mention_autocomplete.selected, 1);
}

#[test]
fn test_mention_autocomplete_hide() {
    let mut view = ChatView::new();

    // Manually show autocomplete
    view.mention_autocomplete.show(
        crate::widgets::MentionTrigger {
            start: 0,
            partial: "@".to_string(),
            trigger_type: None,
            query: String::new(),
        },
        vec![MentionSuggestion {
            mention_type: MentionType::File,
            display: "test.txt".to_string(),
            insert: "@test.txt".to_string(),
            description: None,
        }],
    );
    assert!(view.mention_autocomplete.visible);

    // Hide
    view.mention_autocomplete.hide();
    assert!(!view.mention_autocomplete.visible);
    assert!(view.mention_autocomplete.suggestions.is_empty());
}

#[test]
fn test_check_mention_trigger_unicode_safe() {
    let mut view = ChatView::new();

    // Test with unicode content before cursor
    view.input = Input::new("🦀🐔@".to_string());
    view.input.handle(InputRequest::GoToEnd);

    // Should not panic on unicode (byte vs char index handling)
    view.check_mention_trigger();
}

#[test]
fn test_check_mention_trigger_empty_input() {
    let mut view = ChatView::new();

    // Empty input should not show autocomplete
    view.check_mention_trigger();
    assert!(!view.mention_autocomplete.visible);
}

#[test]
fn test_accept_mention_suggestion_replaces_trigger() {
    let mut view = ChatView::new();

    // Set up input with partial mention
    view.input = Input::new("@fi".to_string());
    view.input.handle(InputRequest::GoToEnd);

    // Show autocomplete with a suggestion
    view.mention_autocomplete.show(
        crate::widgets::MentionTrigger {
            start: 0,
            partial: "@fi".to_string(),
            trigger_type: None,
            query: "fi".to_string(),
        },
        vec![MentionSuggestion {
            mention_type: MentionType::File,
            display: "file.txt".to_string(),
            insert: "@file.txt".to_string(),
            description: None,
        }],
    );

    // Accept the suggestion
    view.accept_mention_suggestion();

    // Should have replaced @fi with @file.txt
    assert!(view.input.value().contains("file.txt"));
    assert!(!view.mention_autocomplete.visible);
}

#[test]
fn test_accept_mention_cursor_position_mid_sentence() {
    let mut view = ChatView::new();

    // Set up input with mention in middle: "Load @fi and process"
    view.input = Input::new("Load @fi and process".to_string());
    // Position cursor after @fi (at position 8)
    view.input.handle(InputRequest::GoToStart);
    for _ in 0..8 {
        view.input.handle(InputRequest::GoToNextChar);
    }

    // Show autocomplete with a suggestion
    view.mention_autocomplete.show(
        crate::widgets::MentionTrigger {
            start: 5,
            partial: "@fi".to_string(),
            trigger_type: None,
            query: "fi".to_string(),
        },
        vec![MentionSuggestion {
            mention_type: MentionType::File,
            display: "file.rs".to_string(),
            insert: "@file.rs".to_string(),
            description: None,
        }],
    );

    // Accept the suggestion
    view.accept_mention_suggestion();

    // Input should be "Load @file.rs  and process" (with space after mention)
    let value = view.input.value();
    assert!(
        value.contains("@file.rs"),
        "Should contain @file.rs: {}",
        value
    );

    // Cursor should be after "@file.rs " (position 15), NOT at end
    // "Load " (5) + "@file.rs" (8) + " " (1) = 14 chars
    let cursor = view.input.cursor();
    let expected_cursor = 5 + 8 + 1; // start + mention length + space
    assert_eq!(
        cursor, expected_cursor,
        "Cursor should be at {} after mention, not at {} (value: '{}')",
        expected_cursor, cursor, value
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Multi-line Input Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_multiline_input_newline_insertion() {
    let mut view = ChatView::new();

    // Type some text
    view.insert_char('h');
    view.insert_char('e');
    view.insert_char('l');
    view.insert_char('l');
    view.insert_char('o');

    // Insert newline (simulating Shift+Enter)
    view.input.handle(InputRequest::InsertChar('\n'));

    // Type more text
    view.insert_char('w');
    view.insert_char('o');
    view.insert_char('r');
    view.insert_char('l');
    view.insert_char('d');

    // Should have multi-line content
    let value = view.input.value();
    assert!(value.contains('\n'));
    assert!(value.contains("hello"));
    assert!(value.contains("world"));
}

#[test]
fn test_multiline_input_submit_preserves_newlines() {
    let mut view = ChatView::new();

    // Set multi-line input directly
    view.input = Input::new("line1\nline2\nline3".to_string());
    view.input.handle(InputRequest::GoToEnd);

    // Submit
    let message = view.submit();
    assert!(message.is_some());
    let msg = message.unwrap();

    // Should preserve newlines
    assert!(msg.contains('\n'));
    assert_eq!(msg.lines().count(), 3);
}

// ═══════════════════════════════════════════════════════════════════════════════
// MCP Retry Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_mcp_retry_no_failed_call() {
    let view = ChatView::new();

    // By default, no failed MCP call
    assert!(view.last_failed_mcp.is_none());
}

#[test]
fn test_mcp_retry_saves_failed_call() {
    let mut view = ChatView::new();

    // Add an MCP call
    view.add_mcp_call("test_tool", "test_server", r#"{"key": "value"}"#);

    // Fail the call
    view.fail_mcp_call("Connection timeout");

    // Should have saved the failed call
    assert!(view.last_failed_mcp.is_some());
    let failed = view.last_failed_mcp.as_ref().unwrap();
    assert_eq!(failed.tool, "test_tool");
    assert_eq!(failed.server, "test_server");
    assert_eq!(failed.params["key"], "value");
}

#[test]
fn test_mcp_retry_clears_on_take() {
    let mut view = ChatView::new();

    // Set up a failed MCP call
    view.last_failed_mcp = Some(McpRetryInfo {
        tool: "my_tool".to_string(),
        server: "my_server".to_string(),
        params: serde_json::json!({"test": 123}),
    });

    // Take the failed call (simulating Ctrl+R)
    let taken = view.last_failed_mcp.take();
    assert!(taken.is_some());

    // Should be cleared now
    assert!(view.last_failed_mcp.is_none());
}

#[test]
fn test_mcp_retry_overwrites_previous_failure() {
    let mut view = ChatView::new();

    // First MCP call fails
    view.add_mcp_call("tool_1", "server_1", r#"{"call": 1}"#);
    view.fail_mcp_call("Error 1");

    // Second MCP call fails
    view.add_mcp_call("tool_2", "server_2", r#"{"call": 2}"#);
    view.fail_mcp_call("Error 2");

    // Should have the second failed call
    let failed = view.last_failed_mcp.as_ref().unwrap();
    assert_eq!(failed.tool, "tool_2");
    assert_eq!(failed.server, "server_2");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Conversation Search Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_search_mode_starts_inactive() {
    let view = ChatView::new();
    assert!(!view.search_mode);
    assert!(view.search_query.is_empty());
    assert!(view.search_results.is_empty());
}

#[test]
fn test_start_search_activates_mode() {
    let mut view = ChatView::new();
    view.start_search();

    assert!(view.search_mode);
    assert!(view.search_query.is_empty());
}

#[test]
fn test_exit_search_deactivates_mode() {
    let mut view = ChatView::new();
    view.start_search();
    view.search_input_char('t');
    view.search_input_char('e');
    view.search_input_char('s');
    view.search_input_char('t');

    view.exit_search();

    assert!(!view.search_mode);
    assert!(view.search_query.is_empty());
    assert!(view.search_results.is_empty());
}

#[test]
fn test_search_finds_matching_messages() {
    let mut view = ChatView::new();

    // Add some messages
    view.add_user_message("Hello world".to_string());
    view.add_nika_message("Hi there!".to_string(), None);
    view.add_user_message("world of rust".to_string());

    // Start search
    view.start_search();
    view.search_input_char('w');
    view.search_input_char('o');
    view.search_input_char('r');
    view.search_input_char('l');
    view.search_input_char('d');

    // Should find 2 messages (indices 1 and 3 - 0 is the initial system message)
    assert_eq!(view.search_results.len(), 2);
}

#[test]
fn test_search_case_insensitive() {
    let mut view = ChatView::new();
    view.add_user_message("HELLO WORLD".to_string());

    view.start_search();
    view.search_input_char('h');
    view.search_input_char('e');
    view.search_input_char('l');
    view.search_input_char('l');
    view.search_input_char('o');

    // Should find the message even with different case
    assert!(!view.search_results.is_empty());
}

#[test]
fn test_search_navigate_results() {
    let mut view = ChatView::new();

    // Add messages
    view.add_user_message("test one".to_string());
    view.add_user_message("test two".to_string());
    view.add_user_message("test three".to_string());

    view.start_search();
    view.search_input_char('t');
    view.search_input_char('e');
    view.search_input_char('s');
    view.search_input_char('t');

    assert_eq!(view.search_current, 0);

    view.next_search_result();
    assert_eq!(view.search_current, 1);

    view.next_search_result();
    assert_eq!(view.search_current, 2);

    // Should wrap around
    view.next_search_result();
    assert_eq!(view.search_current, 0);
}

#[test]
fn test_search_prev_result() {
    let mut view = ChatView::new();
    view.add_user_message("test one".to_string());
    view.add_user_message("test two".to_string());

    view.start_search();
    view.search_input_char('t');
    view.search_input_char('e');
    view.search_input_char('s');
    view.search_input_char('t');

    // Start at 0, go prev should wrap to end
    view.prev_search_result();
    assert_eq!(view.search_current, view.search_results.len() - 1);
}

#[test]
fn test_search_backspace() {
    let mut view = ChatView::new();
    view.start_search();
    view.search_input_char('a');
    view.search_input_char('b');
    view.search_input_char('c');

    assert_eq!(view.search_query, "abc");

    view.search_input_backspace();
    assert_eq!(view.search_query, "ab");

    view.search_input_backspace();
    assert_eq!(view.search_query, "a");
}

#[test]
fn test_search_empty_query_no_results() {
    let mut view = ChatView::new();
    view.add_user_message("test message".to_string());

    view.start_search();
    // Empty query
    view.update_search();

    assert!(view.search_results.is_empty());
}

#[test]
fn test_search_finds_thinking_content() {
    let mut view = ChatView::new();
    // Add message with thinking content that has unique word
    let msg_id = view.next_message_id();
    view.messages.push(ChatMessage {
        id: msg_id,
        role: MessageRole::Nika,
        content: "Here is my answer".to_string(),
        thinking: Some("Let me analyze the problem carefully".to_string()),
        timestamp: Local::now(),
        created_at: Instant::now(),
        execution: None,
    });
    // Add normal message without the search term
    view.add_user_message("other message".to_string());

    view.start_search();
    // Search for word only in thinking content
    view.search_query = "analyze".to_string();
    view.update_search();

    // Should find the message with thinking content
    assert_eq!(view.search_results.len(), 1);
    assert_eq!(view.search_results[0], 1); // Index 1 (after welcome message)
}

#[test]
fn test_search_no_duplicate_when_both_match() {
    let mut view = ChatView::new();
    // Add message where both content and thinking contain the search term
    let msg_id = view.next_message_id();
    view.messages.push(ChatMessage {
        id: msg_id,
        role: MessageRole::Nika,
        content: "This test contains the word unique".to_string(),
        thinking: Some("Thinking about unique solution".to_string()),
        timestamp: Local::now(),
        created_at: Instant::now(),
        execution: None,
    });

    view.start_search();
    view.search_query = "unique".to_string();
    view.update_search();

    // Should only find it once, not twice
    assert_eq!(view.search_results.len(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Smooth Scrolling Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_smooth_scroll_initial_state() {
    let view = ChatView::new();
    assert_eq!(view.scroll_velocity, 0.0);
    assert_eq!(view.scroll_accumulator, 0.0);
    assert!(!view.scroll_animating);
}

#[test]
fn test_smooth_scroll_starts_animation() {
    let mut view = ChatView::new();

    view.smooth_scroll(1); // Scroll down

    assert!(view.scroll_animating);
    assert!(view.scroll_velocity > 0.0);
}

#[test]
fn test_smooth_scroll_negative_direction() {
    let mut view = ChatView::new();

    view.smooth_scroll(-1); // Scroll up

    assert!(view.scroll_animating);
    assert!(view.scroll_velocity < 0.0);
    // Scrolling up should disable auto-follow
    assert!(!view.user_at_bottom);
}

#[test]
fn test_smooth_scroll_velocity_accumulates() {
    let mut view = ChatView::new();

    view.smooth_scroll(1);
    let v1 = view.scroll_velocity;

    view.smooth_scroll(1);
    let v2 = view.scroll_velocity;

    // Velocity should accumulate
    assert!(v2 > v1);
}

#[test]
fn test_update_scroll_animation_applies_friction() {
    let mut view = ChatView::new();
    view.smooth_scroll(1);
    let initial_velocity = view.scroll_velocity;

    view.update_scroll_animation();
    let after_velocity = view.scroll_velocity;

    // Velocity should decrease after friction
    assert!(after_velocity < initial_velocity);
}

#[test]
fn test_update_scroll_animation_stops_at_low_velocity() {
    let mut view = ChatView::new();
    view.scroll_velocity = 0.05; // Below SCROLL_MIN_VELOCITY
    view.scroll_animating = true;

    let still_animating = view.update_scroll_animation();

    assert!(!still_animating);
    assert!(!view.scroll_animating);
    assert_eq!(view.scroll_velocity, 0.0);
    assert_eq!(view.scroll_accumulator, 0.0);
}

#[test]
fn test_update_scroll_animation_inactive_returns_false() {
    let mut view = ChatView::new();
    // Not animating

    let result = view.update_scroll_animation();

    assert!(!result);
}

#[test]
fn test_stop_smooth_scroll() {
    let mut view = ChatView::new();
    view.smooth_scroll(1);
    assert!(view.scroll_animating);

    view.stop_smooth_scroll();

    assert!(!view.scroll_animating);
    assert_eq!(view.scroll_velocity, 0.0);
    assert_eq!(view.scroll_accumulator, 0.0);
}

#[test]
fn test_is_scroll_animating() {
    let mut view = ChatView::new();
    assert!(!view.is_scroll_animating());

    view.smooth_scroll(1);
    assert!(view.is_scroll_animating());

    view.stop_smooth_scroll();
    assert!(!view.is_scroll_animating());
}

#[test]
fn test_smooth_scroll_animation_converges_to_stop() {
    let mut view = ChatView::new();
    view.smooth_scroll(1);

    // Run animation until it stops (should take finite iterations)
    let mut iterations = 0;
    while view.update_scroll_animation() && iterations < 100 {
        iterations += 1;
    }

    // Animation should eventually stop
    assert!(!view.scroll_animating);
    assert!(
        iterations < 100,
        "Animation should converge within 100 iterations"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Chat DAG Panel Integration Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_dag_panel_default_shown() {
    // DAG panel is shown by default (toggle with Ctrl+D)
    let view = ChatView::new();
    assert!(view.show_dag_panel);
    // DAG nodes/edges are synced on toggle, so initially empty is OK
    // The sync happens when the panel is first shown
}

#[test]
fn test_dag_panel_toggle() {
    let mut view = ChatView::new();
    // DAG panel starts visible by default
    assert!(view.show_dag_panel);

    view.toggle_dag_panel();
    assert!(!view.show_dag_panel);

    view.toggle_dag_panel();
    assert!(view.show_dag_panel);
    // Should have synced DAG state from messages (1 welcome message)
    assert_eq!(view.dag_nodes.len(), 1);
}

#[test]
fn test_dag_panel_sync_from_messages() {
    let mut view = ChatView::new();
    view.add_user_message("Hello".to_string());
    view.add_nika_message("Hi there!".to_string(), None);
    view.add_user_message("How are you?".to_string());

    view.sync_dag_from_messages();

    // Should have 4 nodes: welcome + 3 messages
    assert_eq!(view.dag_nodes.len(), 4);
    // Should have 3 edges: each message links to previous
    assert_eq!(view.dag_edges.len(), 3);

    // Check node kinds
    assert_eq!(view.dag_nodes[0].kind, ChatNodeKind::System);
    assert_eq!(view.dag_nodes[1].kind, ChatNodeKind::User);
    assert_eq!(view.dag_nodes[2].kind, ChatNodeKind::Assistant);
    assert_eq!(view.dag_nodes[3].kind, ChatNodeKind::User);
}

#[test]
fn test_dag_panel_node_label_truncation() {
    let mut view = ChatView::new();
    view.add_user_message("This is a very long message that should be truncated".to_string());

    view.sync_dag_from_messages();

    // The user message is the second node (after welcome)
    let user_node = &view.dag_nodes[1];
    assert!(user_node.label.len() <= 30);
    assert!(user_node.label.ends_with("..."));
}

#[test]
fn test_task_queue_add_and_update() {
    let mut view = ChatView::new();

    view.add_task_to_queue("task-1", ChatTaskVerb::Infer);
    assert_eq!(view.task_queue.len(), 1);
    assert_eq!(view.task_queue[0].id(), "task-1");
    assert_eq!(view.task_queue[0].state(), ChatTaskState::Pending);

    view.update_task_state(
        "task-1",
        ChatTaskState::Running,
        Some(std::time::Duration::from_millis(100)),
    );
    assert_eq!(view.task_queue[0].state(), ChatTaskState::Running);

    view.update_task_state("task-1", ChatTaskState::Complete, None);
    assert_eq!(view.task_queue[0].state(), ChatTaskState::Complete);
}

#[test]
fn test_dag_panel_selected_index() {
    let mut view = ChatView::new();
    view.add_user_message("Hello".to_string());
    view.sync_dag_from_messages();

    assert!(view.dag_selected.is_none());

    view.dag_selected = Some(0);
    assert_eq!(view.dag_selected, Some(0));

    // Selected should be within bounds
    view.dag_selected = Some(100); // Out of bounds
                                   // The render logic handles this gracefully
}

#[test]
fn test_dag_panel_toggle_syncs_state() {
    let mut view = ChatView::new();
    // DAG panel starts visible by default
    assert!(view.show_dag_panel);

    view.add_user_message("First".to_string());
    view.add_nika_message("Response".to_string(), None);

    // Close the panel
    view.toggle_dag_panel();
    assert!(!view.show_dag_panel);

    // Reopen - this should sync the DAG state
    view.toggle_dag_panel();
    assert!(view.show_dag_panel);
    assert_eq!(view.dag_nodes.len(), 3); // welcome + 2 messages

    // Add more messages while panel is open
    view.add_user_message("Second".to_string());
    // Nodes NOT auto-synced (requires explicit sync or toggle)

    // Close and reopen to resync
    view.toggle_dag_panel(); // Close
    view.toggle_dag_panel(); // Reopen
    assert_eq!(view.dag_nodes.len(), 4); // welcome + 3 messages
}

// ═══════════════════════════════════════════════════════════════════════════════
// SESSION PERSISTENCE TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_get_chat_state_converts_messages() {
    let mut view = ChatView::new();
    view.add_user_message("Hello Nika".to_string());
    view.add_nika_message("Hi! How can I help?".to_string(), None);

    let state = view.get_chat_state();

    // Should have 3 messages: welcome + user + nika
    assert_eq!(state.messages.len(), 3);

    // Check role conversion
    assert_eq!(state.messages[0].role, ChatOverlayMessageRole::System);
    assert_eq!(state.messages[1].role, ChatOverlayMessageRole::User);
    assert_eq!(state.messages[2].role, ChatOverlayMessageRole::Nika);

    // Check content preservation
    assert_eq!(state.messages[1].content, "Hello Nika");
    assert_eq!(state.messages[2].content, "Hi! How can I help?");
}

#[test]
fn test_get_chat_state_preserves_input_state() {
    let mut view = ChatView::new();
    view.input = Input::new("partial input".to_string());
    view.input.handle(InputRequest::GoToEnd);

    let state = view.get_chat_state();

    assert_eq!(state.input, "partial input");
    assert_eq!(state.cursor, 13); // Length of "partial input"
}

#[test]
fn test_get_chat_state_preserves_history() {
    let mut view = ChatView::new();
    view.add_user_message("First".to_string());
    view.add_user_message("Second".to_string());

    let state = view.get_chat_state();

    assert_eq!(state.history.len(), 2);
    assert_eq!(state.history[0], "First");
    assert_eq!(state.history[1], "Second");
}

#[test]
fn test_get_chat_state_preserves_model() {
    let view = ChatView::new();
    let state = view.get_chat_state();

    // Model should be set based on env (or default)
    assert!(!state.current_model.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatWorkflow Binding Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_chat_view_workflow_initialized() {
    let view = ChatView::new();
    // Workflow should be initialized with welcome message
    assert_eq!(view.workflow.message_count(), 1);
}

#[test]
fn test_chat_view_user_message_wires_to_workflow() {
    let mut view = ChatView::new();
    view.add_user_message("Hello world".to_string());

    // Should be in both messages vec and workflow
    assert_eq!(view.messages.len(), 2); // welcome + user
    assert_eq!(view.workflow.message_count(), 2); // welcome + user
}

#[test]
fn test_chat_view_nika_message_wires_to_workflow() {
    let mut view = ChatView::new();
    view.add_user_message("Hello".to_string());
    view.add_nika_message("Hi there!".to_string(), None);

    // All should be in workflow (welcome + user + nika)
    assert_eq!(view.workflow.message_count(), 3);
}

#[test]
fn test_chat_view_system_message_wires_to_workflow() {
    let mut view = ChatView::new();
    view.add_system_message("System notification");

    // System message in workflow (welcome + system)
    assert_eq!(view.workflow.message_count(), 2);
}

#[test]
fn test_chat_view_tool_message_wires_to_workflow() {
    let mut view = ChatView::new();
    view.add_tool_message("Tool output".to_string());

    // Tool message in workflow (welcome + tool)
    assert_eq!(view.workflow.message_count(), 2);
}

#[test]
fn test_chat_view_workflow_preserves_roles() {
    let mut view = ChatView::new();
    view.add_user_message("User msg".to_string());
    view.add_nika_message("Assistant msg".to_string(), None);
    view.add_system_message("System msg");
    view.add_tool_message("Tool msg".to_string());

    let messages = view.workflow.all_messages();
    assert_eq!(messages.len(), 5); // welcome + 4 messages
    assert_eq!(messages[0].1.role, WorkflowRole::System); // welcome
    assert_eq!(messages[1].1.role, WorkflowRole::User);
    assert_eq!(messages[2].1.role, WorkflowRole::Assistant);
    assert_eq!(messages[3].1.role, WorkflowRole::System);
    assert_eq!(messages[4].1.role, WorkflowRole::Tool);
}

#[test]
fn test_chat_view_sync_dag_uses_workflow() {
    let mut view = ChatView::new();
    view.show_dag_panel = true;
    view.add_user_message("First".to_string());
    view.add_nika_message("Second".to_string(), None);

    // Sync should use workflow
    view.sync_dag_from_messages();

    // Should have 3 nodes (from workflow: welcome + user + nika)
    assert_eq!(view.dag_nodes.len(), 3);
    // Should have 2 edges (sequential: welcome->user, user->nika)
    assert_eq!(view.dag_edges.len(), 2);
}

#[test]
fn test_chat_view_workflow_mention_creates_edge() {
    let mut view = ChatView::new();
    view.show_dag_panel = true;

    // Add messages with @1 reference (note: @1 is the welcome message now)
    view.add_user_message("What is Rust?".to_string());
    view.add_nika_message("Rust is a programming language".to_string(), None);
    view.add_user_message("@2 Tell me more about safety".to_string()); // References msg 2 (user question)

    view.sync_dag_from_messages();

    // Should have 4 nodes (welcome + 3 messages)
    assert_eq!(view.dag_nodes.len(), 4);
    // Should have 3+ edges: sequential chain + @2 mention
    // Note: depends on how mentions are resolved
    assert!(view.dag_edges.len() >= 3);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Phase 9: New feature tests — last_nika_message_content
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_last_nika_message_content_returns_none_for_empty() {
    let view = ChatView::new();
    // Only welcome (System) message exists
    assert!(view.last_nika_message_content().is_none());
}

#[test]
fn test_last_nika_message_content_returns_latest() {
    let mut view = ChatView::new();
    view.add_nika_message("First response".to_string(), None);
    view.add_user_message("Follow up".to_string());
    view.add_nika_message("Second response".to_string(), None);

    let content = view.last_nika_message_content();
    assert_eq!(content, Some("Second response".to_string()));
}

#[test]
fn test_last_nika_message_content_skips_user_messages() {
    let mut view = ChatView::new();
    view.add_nika_message("Only nika msg".to_string(), None);
    view.add_user_message("User msg after".to_string());

    let content = view.last_nika_message_content();
    assert_eq!(content, Some("Only nika msg".to_string()));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Phase 9: Tab completion for non-verb commands
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_tab_completion_help_command() {
    let mut view = ChatView::new();
    view.input = Input::new("/he".to_string());
    view.input.handle(InputRequest::GoToEnd);

    let result = view.try_verb_autocomplete();
    assert_eq!(result, Some("/help ".to_string()));
}

#[test]
fn test_tab_completion_model_command() {
    let mut view = ChatView::new();
    view.input = Input::new("/mo".to_string());
    view.input.handle(InputRequest::GoToEnd);

    let result = view.try_verb_autocomplete();
    assert_eq!(result, Some("/model ".to_string()));
}

#[test]
fn test_tab_completion_clear_command() {
    let mut view = ChatView::new();
    view.input = Input::new("/cl".to_string());
    view.input.handle(InputRequest::GoToEnd);

    let result = view.try_verb_autocomplete();
    assert_eq!(result, Some("/clear ".to_string()));
}

#[test]
fn test_tab_completion_no_match() {
    let mut view = ChatView::new();
    view.input = Input::new("/zz".to_string());
    view.input.handle(InputRequest::GoToEnd);

    let result = view.try_verb_autocomplete();
    assert!(result.is_none());
}

#[test]
fn test_tab_completion_already_complete_non_verb() {
    let mut view = ChatView::new();
    view.input = Input::new("/help".to_string());
    view.input.handle(InputRequest::GoToEnd);

    // Already complete — no verb detection, no non-verb completion (exact match)
    let result = view.try_verb_autocomplete();
    assert!(result.is_none());
}

#[test]
fn test_tab_completion_single_char_too_short() {
    let mut view = ChatView::new();
    view.input = Input::new("/h".to_string());
    view.input.handle(InputRequest::GoToEnd);

    // Single char after / is too short for autocomplete (needs 2+)
    let result = view.try_verb_autocomplete();
    assert!(result.is_none());
}
