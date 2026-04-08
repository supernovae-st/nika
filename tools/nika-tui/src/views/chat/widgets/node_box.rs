// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat Node Box Widget
//!
//! Renders individual nodes in the Chat DAG visualization.
//! Supports User, Assistant, ToolCall, System, and Error node types.
//!
//! Chat-as-DAG architecture

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Widget},
};

use crate::tokens::compat;

// ═══════════════════════════════════════════════════════════════════════════
// CHAT NODE KIND
// ═══════════════════════════════════════════════════════════════════════════

/// Kind of node in the Chat DAG
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatNodeKind {
    /// User message
    User,
    /// Assistant response
    Assistant,
    /// Tool call (MCP invoke)
    ToolCall,
    /// System message
    System,
    /// Error state
    Error,
}

impl ChatNodeKind {
    /// Get icon for this node kind
    pub fn icon(&self) -> &'static str {
        match self {
            ChatNodeKind::User => "👤",
            ChatNodeKind::Assistant => "🤖",
            ChatNodeKind::ToolCall => "🔌",
            ChatNodeKind::System => "⚙️",
            ChatNodeKind::Error => "❌",
        }
    }

    /// All node kinds
    pub fn all() -> &'static [ChatNodeKind] {
        &[
            ChatNodeKind::User,
            ChatNodeKind::Assistant,
            ChatNodeKind::ToolCall,
            ChatNodeKind::System,
            ChatNodeKind::Error,
        ]
    }

    /// Default color for this node kind
    pub fn color(&self) -> Color {
        match self {
            ChatNodeKind::User => compat::CYAN_500,
            ChatNodeKind::Assistant => compat::GREEN_500,
            ChatNodeKind::ToolCall => compat::PINK_500,
            ChatNodeKind::System => compat::AMBER_500,
            ChatNodeKind::Error => compat::RED_500,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT NODE STATE
// ═══════════════════════════════════════════════════════════════════════════

/// State of a node in execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatNodeState {
    /// Idle - not yet started
    #[default]
    Idle,
    /// Running - currently executing
    Running,
    /// Complete - finished successfully
    Complete,
    /// Failed - execution failed
    Failed,
}

impl ChatNodeState {
    /// Check if node is currently running
    pub fn is_running(&self) -> bool {
        matches!(self, ChatNodeState::Running)
    }

    /// Get border color for this state
    pub fn border_color(&self) -> Color {
        match self {
            ChatNodeState::Idle => compat::SLATE_500,
            ChatNodeState::Running => compat::AMBER_500,
            ChatNodeState::Complete => compat::GREEN_500,
            ChatNodeState::Failed => compat::RED_500,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT NODE BOX
// ═══════════════════════════════════════════════════════════════════════════

/// A box widget representing a node in the Chat DAG
#[derive(Debug, Clone)]
pub struct ChatNodeBox {
    /// Node kind (User, Assistant, etc.)
    kind: ChatNodeKind,
    /// Display label (truncated message preview)
    label: String,
    /// Stable node index for @N reference
    index: u32,
    /// Current execution state
    state: ChatNodeState,
    /// Whether this node is selected/focused
    selected: bool,
    /// Animation tick for pulse effect
    animation_tick: u8,
}

impl ChatNodeBox {
    /// Create a new chat node box
    pub fn new(kind: ChatNodeKind) -> Self {
        Self {
            kind,
            label: String::new(),
            index: 0,
            state: ChatNodeState::default(),
            selected: false,
            animation_tick: 0,
        }
    }

    /// Set the display label
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    /// Set the stable node index (@N reference)
    pub fn with_index(mut self, index: u32) -> Self {
        self.index = index;
        self
    }

    /// Set the execution state
    pub fn with_state(mut self, state: ChatNodeState) -> Self {
        self.state = state;
        self
    }

    /// Set whether this node is selected
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Advance animation state
    pub fn tick(&mut self) {
        if self.state.is_running() {
            self.animation_tick = self.animation_tick.wrapping_add(1);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WIDGET IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════════

impl Widget for ChatNodeBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Skip if area too small
        if area.height < 3 || area.width < 10 {
            return;
        }

        // Calculate dimensions
        let width = area.width.min(30);
        let _height = 3.min(area.height);

        // Border style based on state
        let border_color = if self.selected {
            self.kind.color()
        } else {
            self.state.border_color()
        };

        let border_style = Style::default().fg(border_color);
        let border_style = if self.selected || self.state.is_running() {
            border_style.add_modifier(Modifier::BOLD)
        } else {
            border_style
        };

        // Draw border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Draw content: "@N 🔌 Label"
        let content = format!(
            "@{} {} {}",
            self.index,
            self.kind.icon(),
            truncate_label(&self.label, (width.saturating_sub(8)) as usize)
        );

        let content_style = Style::default().fg(self.kind.color());
        buf.set_string(inner.x, inner.y, &content, content_style);
    }
}

/// Truncate label to fit within max length, adding ellipsis
fn truncate_label(label: &str, max_len: usize) -> String {
    if label.chars().count() <= max_len {
        label.to_string()
    } else if max_len <= 1 {
        "…".to_string()
    } else {
        let truncated: String = label.chars().take(max_len.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- ChatNodeKind tests ---

    #[test]
    fn test_chat_node_kind_icons() {
        assert_eq!(ChatNodeKind::User.icon(), "👤");
        assert_eq!(ChatNodeKind::Assistant.icon(), "🤖");
        assert_eq!(ChatNodeKind::ToolCall.icon(), "🔌");
        assert_eq!(ChatNodeKind::System.icon(), "⚙️");
        assert_eq!(ChatNodeKind::Error.icon(), "❌");
    }

    #[test]
    fn test_chat_node_kind_all_variants() {
        let kinds = ChatNodeKind::all();
        assert_eq!(kinds.len(), 5);
    }

    #[test]
    fn test_chat_node_kind_colors() {
        assert_eq!(ChatNodeKind::User.color(), compat::CYAN_500);
        assert_eq!(ChatNodeKind::Assistant.color(), compat::GREEN_500);
        assert_eq!(ChatNodeKind::ToolCall.color(), compat::PINK_500);
        assert_eq!(ChatNodeKind::System.color(), compat::AMBER_500);
        assert_eq!(ChatNodeKind::Error.color(), compat::RED_500);
    }

    // --- ChatNodeState tests ---

    #[test]
    fn test_chat_node_state_is_running() {
        assert!(!ChatNodeState::Idle.is_running());
        assert!(ChatNodeState::Running.is_running());
        assert!(!ChatNodeState::Complete.is_running());
        assert!(!ChatNodeState::Failed.is_running());
    }

    #[test]
    fn test_chat_node_state_border_color() {
        assert_eq!(ChatNodeState::Idle.border_color(), compat::SLATE_500);
        assert_eq!(ChatNodeState::Running.border_color(), compat::AMBER_500);
        assert_eq!(ChatNodeState::Complete.border_color(), compat::GREEN_500);
        assert_eq!(ChatNodeState::Failed.border_color(), compat::RED_500);
    }

    #[test]
    fn test_chat_node_state_default() {
        let state = ChatNodeState::default();
        assert_eq!(state, ChatNodeState::Idle);
    }

    // --- ChatNodeBox creation tests ---

    #[test]
    fn test_chat_node_box_creation() {
        let node = ChatNodeBox::new(ChatNodeKind::User)
            .with_label("Hello world")
            .with_index(1);

        // Verify via render: renders without panic and contains @1
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 3));
        node.render(buf.area, &mut buf);
        let content = buffer_to_string(&buf);
        assert!(content.contains("@1"));
    }

    #[test]
    fn test_chat_node_box_default_state() {
        // Verify default construction works
        let _node = ChatNodeBox::new(ChatNodeKind::User);
    }

    #[test]
    fn test_chat_node_box_builder_pattern() {
        let node = ChatNodeBox::new(ChatNodeKind::Assistant)
            .with_label("I'll help you...")
            .with_index(2)
            .with_state(ChatNodeState::Running)
            .with_selected(true);

        // Verify via render
        let mut buf = Buffer::empty(Rect::new(0, 0, 25, 3));
        node.render(buf.area, &mut buf);
        let content = buffer_to_string(&buf);
        assert!(content.contains("@2"));
    }

    // --- Animation tests ---

    #[test]
    fn test_chat_node_box_tick() {
        let mut node = ChatNodeBox::new(ChatNodeKind::User).with_state(ChatNodeState::Running);

        let initial = node.animation_tick;
        node.tick();
        assert_eq!(node.animation_tick, initial.wrapping_add(1));
    }

    #[test]
    fn test_chat_node_box_tick_only_when_running() {
        let mut node = ChatNodeBox::new(ChatNodeKind::User).with_state(ChatNodeState::Idle);

        let initial = node.animation_tick;
        node.tick();
        // Should not change when not running
        assert_eq!(node.animation_tick, initial);
    }

    // --- Truncation tests ---

    #[test]
    fn test_truncate_label_short() {
        let result = truncate_label("Hello", 10);
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_truncate_label_long() {
        let result = truncate_label("Hello, World!", 8);
        assert_eq!(result, "Hello, …");
    }

    #[test]
    fn test_truncate_label_empty() {
        let result = truncate_label("", 10);
        assert_eq!(result, "");
    }

    #[test]
    fn test_truncate_label_exact_fit() {
        let result = truncate_label("Hello", 5);
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_truncate_label_unicode() {
        // Unicode characters should be handled correctly
        let result = truncate_label("你好世界", 3);
        assert_eq!(result, "你好…");
    }

    // --- Render tests ---

    #[test]
    fn test_chat_node_box_render_basic() {
        let node = ChatNodeBox::new(ChatNodeKind::User)
            .with_label("Hello")
            .with_index(1);

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 3));
        node.render(buf.area, &mut buf);

        // Should contain icon and index
        let content = buffer_to_string(&buf);
        assert!(content.contains("@1"));
    }

    #[test]
    fn test_chat_node_box_render_selected() {
        let node = ChatNodeBox::new(ChatNodeKind::User)
            .with_label("Hello")
            .with_index(1)
            .with_selected(true);

        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 3));
        node.render(buf.area, &mut buf);

        // Should render without panic
        let content = buffer_to_string(&buf);
        assert!(!content.is_empty());
    }

    #[test]
    fn test_chat_node_box_render_running() {
        let node = ChatNodeBox::new(ChatNodeKind::ToolCall)
            .with_label("search...")
            .with_index(3)
            .with_state(ChatNodeState::Running);

        let mut buf = Buffer::empty(Rect::new(0, 0, 25, 3));
        node.render(buf.area, &mut buf);

        // Should render without panic
        let content = buffer_to_string(&buf);
        assert!(content.contains("@3"));
    }

    #[test]
    fn test_chat_node_box_render_too_small() {
        let node = ChatNodeBox::new(ChatNodeKind::User)
            .with_label("Hello")
            .with_index(1);

        // Area too small - should skip rendering
        let mut buf = Buffer::empty(Rect::new(0, 0, 5, 2));
        node.render(Rect::new(0, 0, 5, 2), &mut buf);

        // Buffer should remain empty (default cells)
        let cell = buf.cell((0, 0)).unwrap();
        assert_eq!(cell.symbol(), " ");
    }

    // --- Integration test ---

    #[test]
    fn test_chat_node_box_exported() {
        // This test verifies the type is accessible
        let _ = ChatNodeBox::new(ChatNodeKind::User);
    }

    /// Helper to convert buffer to string for assertions
    fn buffer_to_string(buf: &Buffer) -> String {
        buf.content.iter().map(|c| c.symbol()).collect()
    }
}
