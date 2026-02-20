//! TUI Views Module
//!
//! Two-view architecture for Nika TUI:
//!
//! 1. **Browser View** - Workflow selection and preview (default)
//! 2. **Monitor View** - Real-time execution monitoring
//!
//! # State Machine
//!
//! ```text
//!                     ┌─────────────┐
//!                     │   BROWSER   │ ◄─── Default view
//!                     │   (View 1)  │
//!                     └──────┬──────┘
//!                            │ [Enter] or [▶]
//!                            ▼
//!                     ┌─────────────┐
//!                     │   MONITOR   │
//!                     │   (View 2)  │
//!                     └──────┬──────┘
//!                            │ [Esc] or workflow complete
//!                            ▼
//!                     ┌─────────────┐
//!                     │   BROWSER   │ ◄─── Back to browser
//!                     └─────────────┘
//! ```

mod browser;
mod monitor;

pub use browser::BrowserView;
pub use monitor::{DagTab, MissionTab, MonitorView, NovanetTab, ReasoningTab};

/// Active view in the TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiView {
    /// Workflow browser - select and preview workflows before execution
    #[default]
    Browser,
    /// Execution monitor - real-time 4-panel execution display
    Monitor,
}

impl TuiView {
    /// Get the title for the header bar
    pub fn title(&self) -> &'static str {
        match self {
            TuiView::Browser => "NIKA WORKFLOW STUDIO",
            TuiView::Monitor => "NIKA EXECUTION",
        }
    }

    /// Get the emoji for the view
    pub fn icon(&self) -> &'static str {
        match self {
            TuiView::Browser => "⚡",
            TuiView::Monitor => "🚀",
        }
    }

    /// Toggle to the other view
    pub fn toggle(&self) -> Self {
        match self {
            TuiView::Browser => TuiView::Monitor,
            TuiView::Monitor => TuiView::Browser,
        }
    }
}

/// Result of handling a key event in a view
#[derive(Debug, Clone)]
pub enum ViewAction {
    /// No action needed
    None,
    /// Quit the TUI
    Quit,
    /// Switch to a different view
    SwitchView(TuiView),
    /// Run a workflow at the given path
    RunWorkflow(std::path::PathBuf),
    /// Show an error message
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_view_default() {
        let view = TuiView::default();
        assert_eq!(view, TuiView::Browser);
    }

    #[test]
    fn test_tui_view_toggle() {
        let browser = TuiView::Browser;
        assert_eq!(browser.toggle(), TuiView::Monitor);

        let monitor = TuiView::Monitor;
        assert_eq!(monitor.toggle(), TuiView::Browser);
    }

    #[test]
    fn test_tui_view_titles() {
        assert_eq!(TuiView::Browser.title(), "NIKA WORKFLOW STUDIO");
        assert_eq!(TuiView::Monitor.title(), "NIKA EXECUTION");
    }

    #[test]
    fn test_tui_view_icons() {
        assert_eq!(TuiView::Browser.icon(), "⚡");
        assert_eq!(TuiView::Monitor.icon(), "🚀");
    }
}
