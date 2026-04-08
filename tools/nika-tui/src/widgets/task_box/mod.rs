// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task Box Widgets
//!
//! Structured visual containers for the 5 semantic verbs.
//! Each verb has a distinct visual treatment with color-coded borders,
//! icons, and verb-specific information panels.
//!
//! ## Verb Boxes
//!
//! | Verb | Icon | Color | Widget |
//! |------|------|-------|--------|
//! | `infer:` | ⚡ | Violet | [`InferBox`] |
//! | `exec:` | 📟 | Amber | [`ExecBox`] |
//! | `fetch:` | 🛰️ | Cyan | [`FetchBox`] |
//! | `invoke:` | 🔌 | Emerald | [`InvokeBox`] |
//! | `agent:` | 🐔 | Rose | [`AgentBox`] |
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika_tui::widgets::task_box::{TaskBox, InferBox, BoxState, VerbColor};
//!
//! let infer_box = InferBox::new("claude-sonnet-4-6", "Generate a market analysis")
//!     .with_state(BoxState::running());
//!
//! let task = TaskBox::Infer(infer_box);
//! ```

mod colors;
mod key_action;
mod render_mode;
mod state;
mod token_velocity;

// Individual box widgets
mod agent;
mod exec;
mod fetch;
mod infer;
mod invoke;
mod invoke_render;

// Re-exports
pub use colors::{exit, http, status, VerbColor};
pub use key_action::KeyAction;
pub use render_mode::RenderMode;
pub use state::{BoxState, BRAILLE_SPINNER};
pub use token_velocity::TokenVelocity;

pub use agent::AgentBox;
pub use exec::ExecBox;
pub use fetch::FetchBox;
pub use infer::InferBox;
pub use invoke::{BuiltinHint, InvokeBox};

use crossterm::event::KeyCode;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{ListItem, Widget},
};

/// Context passed from ChatView for streaming-aware rendering
#[derive(Debug, Clone)]
pub struct StreamingContext<'a> {
    /// Current render mode (Compact/Expanded/Full)
    pub render_mode: RenderMode,
    /// Whether the view is currently streaming a response
    pub is_streaming: bool,
    /// Partial response text during streaming
    pub partial_response: &'a str,
    /// Current frame counter for animations (pulse, cursor blink)
    pub frame: u64,
}

/// Union type for all task boxes
#[derive(Debug, Clone)]
pub enum TaskBox {
    /// LLM text generation
    Infer(InferBox),
    /// Shell command execution
    Exec(ExecBox),
    /// HTTP request
    Fetch(FetchBox),
    /// MCP tool call
    Invoke(InvokeBox),
    /// Multi-turn agentic loop (container)
    Agent(AgentBox),
}

impl TaskBox {
    /// Get the verb color for this task box
    pub fn verb_color(&self) -> VerbColor {
        match self {
            Self::Infer(_) => VerbColor::Infer,
            Self::Exec(_) => VerbColor::Exec,
            Self::Fetch(_) => VerbColor::Fetch,
            Self::Invoke(_) => VerbColor::Invoke,
            Self::Agent(_) => VerbColor::Agent,
        }
    }

    /// Get the current state
    pub fn state(&self) -> &BoxState {
        match self {
            Self::Infer(b) => &b.state,
            Self::Exec(b) => &b.state,
            Self::Fetch(b) => &b.state,
            Self::Invoke(b) => &b.state,
            Self::Agent(b) => &b.state,
        }
    }

    /// Get mutable state reference
    pub fn state_mut(&mut self) -> &mut BoxState {
        match self {
            Self::Infer(b) => &mut b.state,
            Self::Exec(b) => &mut b.state,
            Self::Fetch(b) => &mut b.state,
            Self::Invoke(b) => &mut b.state,
            Self::Agent(b) => &mut b.state,
        }
    }

    /// Advance animation frame
    pub fn tick(&mut self) {
        self.state_mut().tick();
        // Also tick children for AgentBox
        if let Self::Agent(agent) = self {
            for child in &mut agent.children {
                child.tick();
            }
        }
    }

    /// Check if this box is a terminal state
    pub fn is_terminal(&self) -> bool {
        self.state().is_terminal()
    }

    /// Check if this box is running
    pub fn is_running(&self) -> bool {
        self.state().is_running()
    }

    /// Calculate required height for rendering
    pub fn required_height(&self) -> u16 {
        match self {
            Self::Infer(b) => b.required_height(),
            Self::Exec(b) => b.required_height(),
            Self::Fetch(b) => b.required_height(),
            Self::Invoke(b) => b.required_height(),
            Self::Agent(b) => b.required_height(),
        }
    }

    /// Get the task ID if available
    pub fn task_id(&self) -> Option<&str> {
        match self {
            Self::Agent(b) => Some(&b.task_id),
            _ => None,
        }
    }

    /// Handle keyboard input, return action if recognized
    pub fn handle_key(&mut self, key: KeyCode) -> Option<KeyAction> {
        let action = KeyAction::from_key(key)?;

        match action {
            KeyAction::Toggle => {
                match self {
                    TaskBox::Infer(b) => b.expanded_response = !b.expanded_response,
                    TaskBox::Exec(b) => b.expanded_stdout = !b.expanded_stdout,
                    TaskBox::Fetch(b) => b.expanded_response = !b.expanded_response,
                    TaskBox::Invoke(b) => b.toggle_result(),
                    TaskBox::Agent(b) => b.toggle_response(),
                }
                Some(action)
            }
            KeyAction::Mode => {
                self.cycle_render_mode();
                Some(action)
            }
            KeyAction::Children => {
                if let TaskBox::Agent(b) = self {
                    b.toggle_children();
                    Some(action)
                } else {
                    None
                }
            }
            KeyAction::Kill => {
                if matches!(self, TaskBox::Exec(_)) {
                    Some(action) // Caller handles actual kill
                } else {
                    None
                }
            }
            KeyAction::Retry => {
                if matches!(self, TaskBox::Fetch(_)) {
                    Some(action) // Caller handles actual retry
                } else {
                    None
                }
            }
            // Copy, Json, etc. return action for caller to handle
            _ => Some(action),
        }
    }

    /// Get available actions for this box type
    pub fn available_actions(&self) -> &'static [KeyAction] {
        match self {
            TaskBox::Infer(_) => KeyAction::actions_for_verb("infer"),
            TaskBox::Exec(_) => KeyAction::actions_for_verb("exec"),
            TaskBox::Fetch(_) => KeyAction::actions_for_verb("fetch"),
            TaskBox::Invoke(_) => KeyAction::actions_for_verb("invoke"),
            TaskBox::Agent(_) => KeyAction::actions_for_verb("agent"),
        }
    }

    /// Get current render mode
    pub fn render_mode(&self) -> RenderMode {
        match self {
            TaskBox::Infer(b) => b.render_mode,
            TaskBox::Exec(b) => b.render_mode,
            TaskBox::Fetch(b) => b.render_mode,
            TaskBox::Invoke(b) => b.render_mode,
            TaskBox::Agent(b) => b.render_mode,
        }
    }

    /// Cycle to next render mode
    pub fn cycle_render_mode(&mut self) {
        match self {
            TaskBox::Infer(b) => b.render_mode = b.render_mode.cycle(),
            TaskBox::Exec(b) => b.render_mode = b.render_mode.cycle(),
            TaskBox::Fetch(b) => b.render_mode = b.render_mode.cycle(),
            TaskBox::Invoke(b) => b.render_mode = b.render_mode.cycle(),
            TaskBox::Agent(b) => b.render_mode = b.render_mode.cycle(),
        }
    }

    /// Convert this TaskBox into ListItem lines for scrollable rendering.
    ///
    /// This method delegates to each variant's implementation, allowing
    /// the ChatView to use a ratatui List widget instead of inline rendering.
    /// The StreamingContext provides animation frame, streaming state, and
    /// partial response text for streaming-aware rendering.
    pub fn to_list_items(&self, ctx: &StreamingContext) -> Vec<ListItem<'static>> {
        match self {
            Self::Infer(b) => b.to_list_items(ctx),
            Self::Exec(b) => b.to_list_items(ctx),
            Self::Fetch(b) => b.to_list_items(ctx),
            Self::Invoke(b) => b.to_list_items(ctx),
            Self::Agent(b) => b.to_list_items(ctx),
        }
    }

    /// Calculate pulse intensity for running state animation.
    ///
    /// Returns a value between 0.0 and 1.0 based on the current frame,
    /// creating a smooth sine wave animation for the pulse effect.
    pub fn pulse_intensity(&self, frame: u64) -> f32 {
        if self.state().is_running() {
            ((frame as f32 / 30.0).sin() + 1.0) / 2.0
        } else {
            0.0
        }
    }

    /// Set pulse intensity on the inner box struct.
    ///
    /// This propagates the computed pulse value into each variant's
    /// `pulse_intensity` field so that `to_list_items()` renders
    /// animated borders for running tasks.
    pub fn set_pulse_intensity(&mut self, intensity: f32) {
        let clamped = intensity.clamp(0.0, 1.0);
        match self {
            TaskBox::Infer(b) => b.pulse_intensity = clamped,
            TaskBox::Exec(b) => b.pulse_intensity = clamped,
            TaskBox::Fetch(b) => b.pulse_intensity = clamped,
            TaskBox::Invoke(b) => b.pulse_intensity = clamped,
            TaskBox::Agent(b) => b.pulse_intensity = clamped,
        }
    }
}

// PERF: Zero-copy render via `impl Widget for &TaskBox` (ratatui 0.30).
// Eliminates 600+ deep clones/sec for visible tasks at 60fps.
impl Widget for &TaskBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self {
            TaskBox::Infer(b) => b.render(area, buf),
            TaskBox::Exec(b) => b.render(area, buf),
            TaskBox::Fetch(b) => b.render(area, buf),
            TaskBox::Invoke(b) => b.render(area, buf),
            TaskBox::Agent(b) => b.render(area, buf),
        }
    }
}

impl Widget for TaskBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        (&self).render(area, buf);
    }
}

/// Widget for TaskBox reference (non-consuming)
pub struct TaskBoxWidget<'a> {
    task_box: &'a TaskBox,
}

impl<'a> TaskBoxWidget<'a> {
    pub fn new(task_box: &'a TaskBox) -> Self {
        Self { task_box }
    }
}

impl Widget for TaskBoxWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.task_box.render(area, buf); // zero-copy: calls Widget for &TaskBox
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_box_verb_color() {
        let infer = TaskBox::Infer(InferBox::new("model", "prompt"));
        assert_eq!(infer.verb_color(), VerbColor::Infer);

        let exec = TaskBox::Exec(ExecBox::new("ls -la"));
        assert_eq!(exec.verb_color(), VerbColor::Exec);

        let fetch = TaskBox::Fetch(FetchBox::new("GET", "https://example.com"));
        assert_eq!(fetch.verb_color(), VerbColor::Fetch);

        let invoke = TaskBox::Invoke(InvokeBox::new("tool", "server"));
        assert_eq!(invoke.verb_color(), VerbColor::Invoke);

        let agent = TaskBox::Agent(AgentBox::new("task-1", "prompt"));
        assert_eq!(agent.verb_color(), VerbColor::Agent);
    }

    #[test]
    fn test_task_box_state() {
        let mut infer = TaskBox::Infer(InferBox::new("model", "prompt"));
        assert!(matches!(infer.state(), BoxState::Queued));

        *infer.state_mut() = BoxState::running();
        assert!(infer.state().is_running());
    }

    #[test]
    fn test_task_box_tick() {
        let mut infer =
            TaskBox::Infer(InferBox::new("model", "prompt").with_state(BoxState::running()));

        // Get initial frame
        let initial_icon = infer.state().icon();
        infer.tick();
        let next_icon = infer.state().icon();

        // Spinner should change
        assert_ne!(initial_icon, next_icon);
    }

    #[test]
    fn test_task_box_terminal_states() {
        let queued = TaskBox::Infer(InferBox::new("model", "prompt"));
        assert!(!queued.is_terminal());
        assert!(!queued.is_running());

        let running =
            TaskBox::Infer(InferBox::new("model", "prompt").with_state(BoxState::running()));
        assert!(!running.is_terminal());
        assert!(running.is_running());

        let success =
            TaskBox::Infer(InferBox::new("model", "prompt").with_state(BoxState::success(100)));
        assert!(success.is_terminal());
        assert!(!success.is_running());
    }

    #[test]
    fn test_task_box_task_id() {
        let infer = TaskBox::Infer(InferBox::new("model", "prompt"));
        assert!(infer.task_id().is_none());

        let agent = TaskBox::Agent(AgentBox::new("my-task", "prompt"));
        assert_eq!(agent.task_id(), Some("my-task"));
    }

    // === handle_key and render_mode tests ===

    #[test]
    fn test_taskbox_handle_key_toggle() {
        let mut box_ = TaskBox::Infer(InferBox::new("model", "prompt"));

        // Initially not expanded
        if let TaskBox::Infer(ref inner) = box_ {
            assert!(!inner.expanded_response);
        }

        // Toggle with Enter
        let action = box_.handle_key(KeyCode::Enter);
        assert_eq!(action, Some(KeyAction::Toggle));

        // Verify state changed
        if let TaskBox::Infer(ref inner) = box_ {
            assert!(inner.expanded_response);
        }
    }

    #[test]
    fn test_taskbox_handle_key_mode() {
        let mut box_ = TaskBox::Exec(ExecBox::new("echo hello"));

        // Get initial mode
        let initial_mode = box_.render_mode();
        assert_eq!(initial_mode, RenderMode::Expanded);

        // Cycle mode with 'm'
        let action = box_.handle_key(KeyCode::Char('m'));
        assert_eq!(action, Some(KeyAction::Mode));

        // Verify mode changed
        assert_eq!(box_.render_mode(), RenderMode::Full);
    }

    #[test]
    fn test_taskbox_available_actions() {
        let infer = TaskBox::Infer(InferBox::new("model", "prompt"));
        let actions = infer.available_actions();
        assert!(actions.contains(&KeyAction::Copy));
        assert!(actions.contains(&KeyAction::Json));
        assert!(!actions.contains(&KeyAction::Kill)); // Exec-only

        let exec = TaskBox::Exec(ExecBox::new("echo hello"));
        let actions = exec.available_actions();
        assert!(actions.contains(&KeyAction::Kill));
        assert!(actions.contains(&KeyAction::Rerun));
    }

    #[test]
    fn test_taskbox_render_mode_default() {
        let infer = TaskBox::Infer(InferBox::new("model", "prompt"));
        assert_eq!(infer.render_mode(), RenderMode::Expanded);

        let exec = TaskBox::Exec(ExecBox::new("ls"));
        assert_eq!(exec.render_mode(), RenderMode::Expanded);

        let fetch = TaskBox::Fetch(FetchBox::new("GET", "https://example.com"));
        assert_eq!(fetch.render_mode(), RenderMode::Expanded);
    }

    #[test]
    fn test_taskbox_cycle_render_mode() {
        let mut box_ = TaskBox::Invoke(InvokeBox::new("tool", "server"));

        assert_eq!(box_.render_mode(), RenderMode::Expanded);

        box_.cycle_render_mode();
        assert_eq!(box_.render_mode(), RenderMode::Full);

        box_.cycle_render_mode();
        assert_eq!(box_.render_mode(), RenderMode::Compact);

        box_.cycle_render_mode();
        assert_eq!(box_.render_mode(), RenderMode::Expanded);
    }
}
