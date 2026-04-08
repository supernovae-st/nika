// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat Task Queue Widget
//!
//! Displays pending/running/completed tasks in a scrollable list.
//! Part of the Chat DAG visualization system.
//!
//! Chat-as-DAG architecture

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};
use std::time::{Duration, Instant};

use crate::tokens::compat;

// ═══════════════════════════════════════════════════════════════════════════
// CHAT TASK STATE
// ═══════════════════════════════════════════════════════════════════════════

/// State of a task in the queue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatTaskState {
    #[default]
    Pending,
    Running,
    Complete,
    Failed,
}

impl ChatTaskState {
    /// Get icon for this state
    pub fn icon(&self) -> &'static str {
        match self {
            ChatTaskState::Pending => "◦",
            ChatTaskState::Running => "⠹",
            ChatTaskState::Complete => "✓",
            ChatTaskState::Failed => "✗",
        }
    }

    /// Get color for this state
    pub fn color(&self) -> Color {
        match self {
            ChatTaskState::Pending => compat::SLATE_500,
            ChatTaskState::Running => compat::AMBER_500,
            ChatTaskState::Complete => compat::GREEN_500,
            ChatTaskState::Failed => compat::RED_500,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT TASK VERB
// ═══════════════════════════════════════════════════════════════════════════

/// Verb type for the task (matches Nika's 5 verbs)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatTaskVerb {
    Infer,
    Exec,
    Fetch,
    Invoke,
    Agent,
}

impl ChatTaskVerb {
    /// Get icon for this verb
    pub fn icon(&self) -> &'static str {
        match self {
            ChatTaskVerb::Infer => "⚡",
            ChatTaskVerb::Exec => "📟",
            ChatTaskVerb::Fetch => "🛰️",
            ChatTaskVerb::Invoke => "🔌",
            ChatTaskVerb::Agent => "🐔",
        }
    }

    /// Get name for this verb
    pub fn name(&self) -> &'static str {
        match self {
            ChatTaskVerb::Infer => "infer",
            ChatTaskVerb::Exec => "exec",
            ChatTaskVerb::Fetch => "fetch",
            ChatTaskVerb::Invoke => "invoke",
            ChatTaskVerb::Agent => "agent",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT TASK QUEUE ITEM
// ═══════════════════════════════════════════════════════════════════════════

/// A single item in the task queue
#[derive(Debug, Clone)]
pub struct ChatTaskQueueItem {
    id: String,
    verb: ChatTaskVerb,
    state: ChatTaskState,
    elapsed: Option<Duration>,
    started_at: Option<Instant>, // For live elapsed time on running tasks
    progress: f32,               // 0.0 - 1.0
}

impl ChatTaskQueueItem {
    /// Create a new task queue item
    pub fn new(id: &str, verb: ChatTaskVerb) -> Self {
        Self {
            id: id.to_string(),
            verb,
            state: ChatTaskState::default(),
            elapsed: None,
            started_at: None,
            progress: 0.0,
        }
    }

    /// Set the task state
    pub fn with_state(mut self, state: ChatTaskState) -> Self {
        self.state = state;
        self
    }

    /// Set the progress (0.0 - 1.0)
    pub fn with_progress(mut self, progress: f32) -> Self {
        self.progress = progress.clamp(0.0, 1.0);
        self
    }

    /// Set the elapsed time
    pub fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = Some(elapsed);
        self
    }

    /// Set the start time (for live elapsed calculation)
    pub fn with_started_at(mut self, started_at: Instant) -> Self {
        self.started_at = Some(started_at);
        self
    }

    // Getters
    /// Get the task ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the verb type
    pub fn verb(&self) -> ChatTaskVerb {
        self.verb
    }

    /// Get the task state
    pub fn state(&self) -> ChatTaskState {
        self.state
    }

    /// Get the progress
    pub fn progress(&self) -> f32 {
        self.progress
    }

    /// Get the elapsed time
    pub fn elapsed(&self) -> Option<Duration> {
        self.elapsed
    }

    /// Get the start time
    pub fn started_at(&self) -> Option<Instant> {
        self.started_at
    }

    // Mutable setters for ChatView integration
    /// Set the task state (mutable)
    pub fn set_state(&mut self, state: ChatTaskState) {
        self.state = state;
    }

    /// Set the elapsed time (mutable)
    pub fn set_elapsed(&mut self, elapsed: Duration) {
        self.elapsed = Some(elapsed);
    }

    /// Set the progress (mutable)
    pub fn set_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    /// Set the start time (mutable)
    pub fn set_started_at(&mut self, started_at: Instant) {
        self.started_at = Some(started_at);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT TASK QUEUE
// ═══════════════════════════════════════════════════════════════════════════

/// A queue of tasks with state tracking and scrolling
#[derive(Debug, Clone, Default)]
pub struct ChatTaskQueue {
    items: Vec<ChatTaskQueueItem>,
    scroll_offset: usize,
    title: String,
}

impl ChatTaskQueue {
    /// Create a new empty task queue
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            scroll_offset: 0,
            title: "TASKS".to_string(),
        }
    }

    /// Set the title
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    /// Add an item to the queue
    pub fn add(&mut self, item: ChatTaskQueueItem) {
        self.items.push(item);
    }

    /// Get all items
    pub fn items(&self) -> &[ChatTaskQueueItem] {
        &self.items
    }

    /// Get the number of items
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Update the state of a task by ID
    pub fn update_state(&mut self, id: &str, state: ChatTaskState) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.state = state;
        }
    }

    /// Update the progress of a task by ID
    pub fn update_progress(&mut self, id: &str, progress: f32) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.progress = progress.clamp(0.0, 1.0);
        }
    }

    /// Update the elapsed time of a task by ID
    pub fn update_elapsed(&mut self, id: &str, elapsed: Duration) {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.elapsed = Some(elapsed);
        }
    }

    /// Get count of completed tasks
    pub fn completed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.state == ChatTaskState::Complete)
            .count()
    }

    /// Get count of running tasks
    pub fn running_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.state == ChatTaskState::Running)
            .count()
    }

    /// Scroll up
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll down
    pub fn scroll_down(&mut self, visible_height: usize) {
        let max_offset = self.items.len().saturating_sub(visible_height);
        self.scroll_offset = (self.scroll_offset + 1).min(max_offset);
    }

    /// Get current scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.items.clear();
        self.scroll_offset = 0;
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WIDGET IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════════

impl Widget for ChatTaskQueue {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Draw border with title and count
        let header = format!(
            " {} {}/{} ",
            self.title,
            self.completed_count(),
            self.items.len()
        );

        let block = Block::default().borders(Borders::ALL).title(header);

        let inner = block.inner(area);
        block.render(area, buf);

        if self.items.is_empty() {
            buf.set_string(
                inner.x + 1,
                inner.y,
                "No tasks",
                Style::default().fg(compat::SLATE_600),
            );
            return;
        }

        // Render visible items
        let visible_count = inner.height as usize;
        for (i, item) in self
            .items
            .iter()
            .skip(self.scroll_offset)
            .take(visible_count)
            .enumerate()
        {
            let y = inner.y + i as u16;
            let style = Style::default().fg(item.state.color());

            // Format elapsed time - use live elapsed for running tasks
            let elapsed_str = if item.state == ChatTaskState::Running {
                // Live elapsed from start time
                item.started_at
                    .map(|start| format!("{:.1}s", start.elapsed().as_secs_f32()))
                    .unwrap_or_else(|| "0.0s".to_string())
            } else {
                // Static elapsed for completed/failed tasks
                item.elapsed
                    .map(|d| format!("{:.1}s", d.as_secs_f32()))
                    .unwrap_or_else(|| "—".to_string())
            };

            // Render progress bar
            let progress_bar = render_progress_bar(item.progress, 10);
            let progress_pct = (item.progress * 100.0) as u32;

            // Format: "✓ task_name    ⚡ infer   2.1s   ██████████ 100%"
            let line = format!(
                " {} {:<16} {} {:<6} {:>6}  {}  {:>3}%",
                item.state.icon(),
                truncate_str(&item.id, 16),
                item.verb.icon(),
                item.verb.name(),
                elapsed_str,
                progress_bar,
                progress_pct,
            );

            // Truncate line to fit width
            let max_width = inner.width as usize;
            let display_line = truncate_str(&line, max_width);

            buf.set_string(inner.x, y, &display_line, style);
        }

        // Show scroll indicator if needed
        if self.items.len() > visible_count {
            let scroll_indicator = if self.scroll_offset > 0 { "▲" } else { " " };
            buf.set_string(
                inner.x + inner.width.saturating_sub(1),
                inner.y,
                scroll_indicator,
                Style::default().fg(compat::SLATE_600),
            );

            let can_scroll_down = self.scroll_offset + visible_count < self.items.len();
            let scroll_down = if can_scroll_down { "▼" } else { " " };
            buf.set_string(
                inner.x + inner.width.saturating_sub(1),
                inner.y + inner.height.saturating_sub(1),
                scroll_down,
                Style::default().fg(compat::SLATE_600),
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

/// Truncate string to max length
fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else if max <= 1 {
        "…".to_string()
    } else {
        let truncated: String = s.chars().take(max - 1).collect();
        format!("{}…", truncated)
    }
}

/// Render a progress bar
fn render_progress_bar(progress: f32, width: usize) -> String {
    let filled = (progress * width as f32) as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- ChatTaskState tests ---

    #[test]
    fn test_chat_task_state_icons() {
        assert_eq!(ChatTaskState::Pending.icon(), "◦");
        assert_eq!(ChatTaskState::Running.icon(), "⠹");
        assert_eq!(ChatTaskState::Complete.icon(), "✓");
        assert_eq!(ChatTaskState::Failed.icon(), "✗");
    }

    #[test]
    fn test_chat_task_state_colors() {
        assert_eq!(ChatTaskState::Pending.color(), compat::SLATE_500);
        assert_eq!(ChatTaskState::Running.color(), compat::AMBER_500);
        assert_eq!(ChatTaskState::Complete.color(), compat::GREEN_500);
        assert_eq!(ChatTaskState::Failed.color(), compat::RED_500);
    }

    #[test]
    fn test_chat_task_state_default() {
        let state = ChatTaskState::default();
        assert_eq!(state, ChatTaskState::Pending);
    }

    // --- ChatTaskVerb tests ---

    #[test]
    fn test_chat_task_verb_icons() {
        assert_eq!(ChatTaskVerb::Infer.icon(), "⚡");
        assert_eq!(ChatTaskVerb::Exec.icon(), "📟");
        assert_eq!(ChatTaskVerb::Fetch.icon(), "🛰️");
        assert_eq!(ChatTaskVerb::Invoke.icon(), "🔌");
        assert_eq!(ChatTaskVerb::Agent.icon(), "🐔");
    }

    #[test]
    fn test_chat_task_verb_names() {
        assert_eq!(ChatTaskVerb::Infer.name(), "infer");
        assert_eq!(ChatTaskVerb::Exec.name(), "exec");
        assert_eq!(ChatTaskVerb::Fetch.name(), "fetch");
        assert_eq!(ChatTaskVerb::Invoke.name(), "invoke");
        assert_eq!(ChatTaskVerb::Agent.name(), "agent");
    }

    // --- ChatTaskQueueItem tests ---

    #[test]
    fn test_chat_task_queue_item_creation() {
        let item = ChatTaskQueueItem::new("fetch_entity", ChatTaskVerb::Invoke);

        assert_eq!(item.id(), "fetch_entity");
        assert_eq!(item.verb(), ChatTaskVerb::Invoke);
        assert_eq!(item.state(), ChatTaskState::Pending);
    }

    #[test]
    fn test_chat_task_queue_item_with_progress() {
        let item = ChatTaskQueueItem::new("task1", ChatTaskVerb::Infer).with_progress(0.75);

        assert_eq!(item.progress(), 0.75);
    }

    #[test]
    fn test_chat_task_queue_item_progress_clamped() {
        let item = ChatTaskQueueItem::new("task1", ChatTaskVerb::Infer).with_progress(1.5);

        assert_eq!(item.progress(), 1.0);
    }

    #[test]
    fn test_chat_task_queue_item_with_elapsed() {
        let item = ChatTaskQueueItem::new("task1", ChatTaskVerb::Exec)
            .with_elapsed(Duration::from_secs(5));

        assert_eq!(item.elapsed(), Some(Duration::from_secs(5)));
    }

    // --- ChatTaskQueue tests ---

    #[test]
    fn test_chat_task_queue_creation() {
        let queue = ChatTaskQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_chat_task_queue_add_items() {
        let mut queue = ChatTaskQueue::new();
        queue.add(ChatTaskQueueItem::new("task1", ChatTaskVerb::Infer));
        queue.add(ChatTaskQueueItem::new("task2", ChatTaskVerb::Exec));

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.items()[0].id(), "task1");
        assert_eq!(queue.items()[1].id(), "task2");
    }

    #[test]
    fn test_chat_task_queue_update_state() {
        let mut queue = ChatTaskQueue::new();
        queue.add(ChatTaskQueueItem::new("task1", ChatTaskVerb::Infer));

        queue.update_state("task1", ChatTaskState::Running);

        assert_eq!(queue.items()[0].state(), ChatTaskState::Running);
    }

    #[test]
    fn test_chat_task_queue_update_progress() {
        let mut queue = ChatTaskQueue::new();
        queue.add(ChatTaskQueueItem::new("task1", ChatTaskVerb::Infer));

        queue.update_progress("task1", 0.5);

        assert_eq!(queue.items()[0].progress(), 0.5);
    }

    #[test]
    fn test_chat_task_queue_completed_count() {
        let mut queue = ChatTaskQueue::new();
        queue.add(
            ChatTaskQueueItem::new("task1", ChatTaskVerb::Infer)
                .with_state(ChatTaskState::Complete),
        );
        queue.add(
            ChatTaskQueueItem::new("task2", ChatTaskVerb::Exec).with_state(ChatTaskState::Running),
        );
        queue.add(
            ChatTaskQueueItem::new("task3", ChatTaskVerb::Fetch)
                .with_state(ChatTaskState::Complete),
        );

        assert_eq!(queue.completed_count(), 2);
        assert_eq!(queue.running_count(), 1);
    }

    // --- Scroll tests ---

    #[test]
    fn test_chat_task_queue_scroll_up() {
        let mut queue = ChatTaskQueue::new();
        queue.scroll_offset = 5;

        queue.scroll_up();
        assert_eq!(queue.scroll_offset(), 4);

        // At 0, scroll up should stay at 0
        queue.scroll_offset = 0;
        queue.scroll_up();
        assert_eq!(queue.scroll_offset(), 0);
    }

    #[test]
    fn test_chat_task_queue_scroll_down() {
        let mut queue = ChatTaskQueue::new();
        for i in 0..10 {
            queue.add(ChatTaskQueueItem::new(
                &format!("task{}", i),
                ChatTaskVerb::Infer,
            ));
        }

        queue.scroll_down(5); // visible height = 5
        assert_eq!(queue.scroll_offset(), 1);

        // Should not scroll past max
        queue.scroll_offset = 5;
        queue.scroll_down(5);
        assert_eq!(queue.scroll_offset(), 5); // max offset for 10 items with 5 visible
    }

    // --- Render tests ---

    #[test]
    fn test_chat_task_queue_render_empty() {
        let queue = ChatTaskQueue::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        queue.render(buf.area, &mut buf);

        let content = buffer_to_string(&buf);
        assert!(content.contains("TASKS"));
        assert!(content.contains("No tasks"));
    }

    #[test]
    fn test_chat_task_queue_render_with_items() {
        let mut queue = ChatTaskQueue::new();
        queue.add(
            ChatTaskQueueItem::new("fetch", ChatTaskVerb::Invoke)
                .with_state(ChatTaskState::Complete),
        );
        queue.add(
            ChatTaskQueueItem::new("gen", ChatTaskVerb::Infer).with_state(ChatTaskState::Running),
        );

        let mut buf = Buffer::empty(Rect::new(0, 0, 70, 10));
        queue.render(buf.area, &mut buf);

        let content = buffer_to_string(&buf);
        assert!(content.contains("✓"), "Should show complete icon");
        assert!(content.contains("fetch"), "Should show task id");
        assert!(content.contains("1/2"), "Should show completed/total");
    }

    #[test]
    fn test_chat_task_queue_render_with_progress() {
        let mut queue = ChatTaskQueue::new();
        queue.add(
            ChatTaskQueueItem::new("generate", ChatTaskVerb::Infer)
                .with_state(ChatTaskState::Running)
                .with_progress(0.6)
                .with_elapsed(Duration::from_millis(1500)),
        );

        let mut buf = Buffer::empty(Rect::new(0, 0, 70, 10));
        queue.render(buf.area, &mut buf);

        let content = buffer_to_string(&buf);
        assert!(content.contains("60%"), "Should show progress percentage");
        assert!(content.contains("█"), "Should show filled progress bar");
    }

    // --- Helper tests ---

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        assert_eq!(truncate_str("hello world", 8), "hello w…");
    }

    #[test]
    fn test_progress_bar() {
        assert_eq!(render_progress_bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(render_progress_bar(0.5, 10), "█████░░░░░");
        assert_eq!(render_progress_bar(1.0, 10), "██████████");
    }

    // --- Integration test ---

    #[test]
    fn test_chat_task_queue_exported() {
        let _ = ChatTaskQueue::new();
    }

    /// Helper to convert buffer to string for assertions
    fn buffer_to_string(buf: &Buffer) -> String {
        buf.content.iter().map(|c| c.symbol()).collect()
    }
}
