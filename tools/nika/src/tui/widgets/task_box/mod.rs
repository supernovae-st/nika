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
//! use nika::tui::widgets::task_box::{TaskBox, InferBox, BoxState, VerbColor};
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

// Individual box widgets
mod agent;
mod exec;
mod fetch;
mod infer;
mod invoke;

// Re-exports
pub use colors::{exit, http, status, VerbColor};
pub use key_action::KeyAction;
pub use render_mode::RenderMode;
pub use state::{BoxState, BRAILLE_SPINNER};

pub use agent::AgentBox;
pub use exec::ExecBox;
pub use fetch::FetchBox;
pub use infer::InferBox;
pub use invoke::InvokeBox;

use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

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
}

impl Widget for TaskBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self {
            Self::Infer(b) => b.render(area, buf),
            Self::Exec(b) => b.render(area, buf),
            Self::Fetch(b) => b.render(area, buf),
            Self::Invoke(b) => b.render(area, buf),
            Self::Agent(b) => b.render(area, buf),
        }
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
        // Clone to consume - needed for Widget trait
        self.task_box.clone().render(area, buf);
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
}
