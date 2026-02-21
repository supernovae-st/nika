//! TUI Views Module
//!
//! Four-view architecture for Nika TUI:
//!
//! 1. **Chat View** - AI agent conversation interface
//! 2. **Home View** - Workflow browser (default)
//! 3. **Studio View** - YAML editor with validation
//! 4. **Monitor View** - Real-time execution monitoring
//!
//! # Navigation
//!
//! ```text
//!     [1/a]          [2/h]           [3/s]          [4/m]
//!  ┌─────────┐   ┌─────────┐    ┌─────────┐    ┌─────────┐
//!  │  CHAT   │◄─►│  HOME   │◄──►│ STUDIO  │◄──►│ MONITOR │
//!  │  Agent  │   │ Browser │    │  Editor │    │ Execute │
//!  └─────────┘   └─────────┘    └─────────┘    └─────────┘
//!                     ▲
//!                     │ Default view
//! ```
//!
//! Navigation: [Tab] cycles forward, [Shift+Tab] cycles backward.
//! Shortcuts: [1-4] or [a/h/s/m] jump directly to view.

mod browser;
mod chat;
mod home;
mod studio;
mod trait_view;

pub use browser::BrowserView;
// ChatMode exported for future external use (mode indicator in status bar)
#[allow(unused_imports)]
pub use chat::{ChatMode, ChatView, MessageRole};
pub use home::HomeView;
pub use studio::{EditorMode, StudioView};
// Future export: ValidationResult
pub use trait_view::View;

// ═══════════════════════════════════════════════════════════════════════════════
// Panel Tab Enums (moved from monitor.rs during v0.5.2 cleanup)
// Used by TuiState for tracking active tabs in each panel
// ═══════════════════════════════════════════════════════════════════════════════

/// Tab state for Mission Control panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissionTab {
    #[default]
    Progress,
    TaskIO,
    Output,
}

impl MissionTab {
    pub fn next(&self) -> Self {
        match self {
            MissionTab::Progress => MissionTab::TaskIO,
            MissionTab::TaskIO => MissionTab::Output,
            MissionTab::Output => MissionTab::Progress,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            MissionTab::Progress => "Progress",
            MissionTab::TaskIO => "IO",
            MissionTab::Output => "Output",
        }
    }
}

/// Tab state for DAG panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DagTab {
    #[default]
    Graph,
    Yaml,
}

impl DagTab {
    pub fn next(&self) -> Self {
        match self {
            DagTab::Graph => DagTab::Yaml,
            DagTab::Yaml => DagTab::Graph,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            DagTab::Graph => "Graph",
            DagTab::Yaml => "YAML",
        }
    }
}

/// Tab state for NovaNet panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NovanetTab {
    #[default]
    Summary,
    FullJson,
}

impl NovanetTab {
    pub fn next(&self) -> Self {
        match self {
            NovanetTab::Summary => NovanetTab::FullJson,
            NovanetTab::FullJson => NovanetTab::Summary,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            NovanetTab::Summary => "Summary",
            NovanetTab::FullJson => "Full JSON",
        }
    }
}

/// Tab state for Reasoning panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningTab {
    #[default]
    Turns,
    Thinking,
}

impl ReasoningTab {
    pub fn next(&self) -> Self {
        match self {
            ReasoningTab::Turns => ReasoningTab::Thinking,
            ReasoningTab::Thinking => ReasoningTab::Turns,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            ReasoningTab::Turns => "Turns",
            ReasoningTab::Thinking => "Thinking",
        }
    }
}

/// Active view in the TUI - 4 views navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiView {
    /// Chat agent - command Nika conversationally
    Chat,
    /// Home browser - browse and select workflows (default)
    #[default]
    Home,
    /// Studio editor - edit YAML with validation
    Studio,
    /// Monitor execution - real-time 4-panel display
    Monitor,
}

impl TuiView {
    /// Get all views in order
    pub fn all() -> &'static [TuiView] {
        &[
            TuiView::Chat,
            TuiView::Home,
            TuiView::Studio,
            TuiView::Monitor,
        ]
    }

    /// Get next view (cycling)
    pub fn next(&self) -> Self {
        match self {
            TuiView::Chat => TuiView::Home,
            TuiView::Home => TuiView::Studio,
            TuiView::Studio => TuiView::Monitor,
            TuiView::Monitor => TuiView::Chat,
        }
    }

    /// Get previous view (cycling)
    pub fn prev(&self) -> Self {
        match self {
            TuiView::Chat => TuiView::Monitor,
            TuiView::Home => TuiView::Chat,
            TuiView::Studio => TuiView::Home,
            TuiView::Monitor => TuiView::Studio,
        }
    }

    /// Get view number (1-indexed for display)
    pub fn number(&self) -> u8 {
        match self {
            TuiView::Chat => 1,
            TuiView::Home => 2,
            TuiView::Studio => 3,
            TuiView::Monitor => 4,
        }
    }

    /// Get keyboard shortcut
    pub fn shortcut(&self) -> char {
        match self {
            TuiView::Chat => 'a',    // [a]gent
            TuiView::Home => 'h',    // [h]ome
            TuiView::Studio => 's',  // [s]tudio
            TuiView::Monitor => 'm', // [m]onitor
        }
    }

    /// Get the title for the header bar
    pub fn title(&self) -> &'static str {
        match self {
            TuiView::Chat => "NIKA AGENT",
            TuiView::Home => "NIKA HOME",
            TuiView::Studio => "NIKA STUDIO",
            TuiView::Monitor => "NIKA MONITOR",
        }
    }

    /// Get the icon for the view (terminal-friendly diamond)
    pub fn icon(&self) -> &'static str {
        match self {
            TuiView::Chat => "◆",
            TuiView::Home => "◆",
            TuiView::Studio => "◆",
            TuiView::Monitor => "◆",
        }
    }

    /// Toggle to the next view (for backwards compatibility)
    pub fn toggle(&self) -> Self {
        self.next()
    }
}

/// Model provider for LLM switching
pub use crate::tui::command::ModelProvider;

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
    /// Open a workflow in Studio for editing
    OpenInStudio(std::path::PathBuf),
    /// Send a message to the chat agent
    SendChatMessage(String),
    /// Toggle chat overlay
    ToggleChatOverlay,
    /// Show an error message
    Error(String),
    // ═══════════════════════════════════════════════════════════════════════
    // Chat Agent Command Actions (Task 5.1)
    // ═══════════════════════════════════════════════════════════════════════
    /// Execute /infer command - LLM inference with expanded prompt
    ChatInfer(String),
    /// Execute /exec command - shell command execution
    ChatExec(String),
    /// Execute /fetch command - HTTP request (url, method)
    ChatFetch(String, String),
    /// Execute /invoke command - MCP tool call (tool, server, params)
    ChatInvoke(String, Option<String>, serde_json::Value),
    /// Execute /agent command - multi-turn agent (goal, max_turns, extended_thinking)
    ChatAgent(String, Option<u32>, bool),
    /// Execute /model command - switch LLM provider
    ChatModelSwitch(ModelProvider),
    /// Execute /clear command - clear chat history
    ChatClear,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_view_default() {
        let view = TuiView::default();
        assert_eq!(view, TuiView::Home);
    }

    #[test]
    fn test_tui_view_all_four_variants() {
        let views = TuiView::all();
        assert_eq!(views.len(), 4);
        assert_eq!(views[0], TuiView::Chat);
        assert_eq!(views[1], TuiView::Home);
        assert_eq!(views[2], TuiView::Studio);
        assert_eq!(views[3], TuiView::Monitor);
    }

    #[test]
    fn test_tui_view_next_cycles() {
        assert_eq!(TuiView::Chat.next(), TuiView::Home);
        assert_eq!(TuiView::Home.next(), TuiView::Studio);
        assert_eq!(TuiView::Studio.next(), TuiView::Monitor);
        assert_eq!(TuiView::Monitor.next(), TuiView::Chat);
    }

    #[test]
    fn test_tui_view_prev_cycles() {
        assert_eq!(TuiView::Chat.prev(), TuiView::Monitor);
        assert_eq!(TuiView::Home.prev(), TuiView::Chat);
        assert_eq!(TuiView::Studio.prev(), TuiView::Home);
        assert_eq!(TuiView::Monitor.prev(), TuiView::Studio);
    }

    #[test]
    fn test_tui_view_number() {
        assert_eq!(TuiView::Chat.number(), 1);
        assert_eq!(TuiView::Home.number(), 2);
        assert_eq!(TuiView::Studio.number(), 3);
        assert_eq!(TuiView::Monitor.number(), 4);
    }

    #[test]
    fn test_tui_view_shortcut() {
        assert_eq!(TuiView::Chat.shortcut(), 'a');
        assert_eq!(TuiView::Home.shortcut(), 'h');
        assert_eq!(TuiView::Studio.shortcut(), 's');
        assert_eq!(TuiView::Monitor.shortcut(), 'm');
    }

    #[test]
    fn test_tui_view_titles() {
        assert_eq!(TuiView::Chat.title(), "NIKA AGENT");
        assert_eq!(TuiView::Home.title(), "NIKA HOME");
        assert_eq!(TuiView::Studio.title(), "NIKA STUDIO");
        assert_eq!(TuiView::Monitor.title(), "NIKA MONITOR");
    }

    #[test]
    fn test_tui_view_icons() {
        assert_eq!(TuiView::Chat.icon(), "◆");
        assert_eq!(TuiView::Home.icon(), "◆");
        assert_eq!(TuiView::Studio.icon(), "◆");
        assert_eq!(TuiView::Monitor.icon(), "◆");
    }

    #[test]
    fn test_view_action_switch_to_all_views() {
        let actions = [
            ViewAction::SwitchView(TuiView::Chat),
            ViewAction::SwitchView(TuiView::Home),
            ViewAction::SwitchView(TuiView::Studio),
            ViewAction::SwitchView(TuiView::Monitor),
        ];
        assert_eq!(actions.len(), 4);
    }

    #[test]
    fn test_view_action_open_in_studio() {
        let action = ViewAction::OpenInStudio(std::path::PathBuf::from("test.nika.yaml"));
        match action {
            ViewAction::OpenInStudio(path) => assert_eq!(path.to_str(), Some("test.nika.yaml")),
            _ => panic!("Expected OpenInStudio"),
        }
    }

    #[test]
    fn test_view_action_send_chat_message() {
        let action = ViewAction::SendChatMessage("Hello Nika".to_string());
        match action {
            ViewAction::SendChatMessage(msg) => assert_eq!(msg, "Hello Nika"),
            _ => panic!("Expected SendChatMessage"),
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // Tab enum tests (moved from monitor.rs during v0.5.2 cleanup)
    // ════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_mission_tab_cycles() {
        let tab = MissionTab::Progress;
        assert_eq!(tab.next(), MissionTab::TaskIO);
        assert_eq!(tab.next().next(), MissionTab::Output);
        assert_eq!(tab.next().next().next(), MissionTab::Progress);
    }

    #[test]
    fn test_dag_tab_cycles() {
        let tab = DagTab::Graph;
        assert_eq!(tab.next(), DagTab::Yaml);
        assert_eq!(tab.next().next(), DagTab::Graph);
    }

    #[test]
    fn test_novanet_tab_cycles() {
        let tab = NovanetTab::Summary;
        assert_eq!(tab.next(), NovanetTab::FullJson);
        assert_eq!(tab.next().next(), NovanetTab::Summary);
    }

    #[test]
    fn test_reasoning_tab_cycles() {
        let tab = ReasoningTab::Turns;
        assert_eq!(tab.next(), ReasoningTab::Thinking);
        assert_eq!(tab.next().next(), ReasoningTab::Turns);
    }

    #[test]
    fn test_tab_titles() {
        assert_eq!(MissionTab::Progress.title(), "Progress");
        assert_eq!(MissionTab::TaskIO.title(), "IO");
        assert_eq!(MissionTab::Output.title(), "Output");
        assert_eq!(DagTab::Graph.title(), "Graph");
        assert_eq!(DagTab::Yaml.title(), "YAML");
        assert_eq!(NovanetTab::Summary.title(), "Summary");
        assert_eq!(NovanetTab::FullJson.title(), "Full JSON");
        assert_eq!(ReasoningTab::Turns.title(), "Turns");
        assert_eq!(ReasoningTab::Thinking.title(), "Thinking");
    }

    #[test]
    fn test_tab_defaults() {
        assert_eq!(MissionTab::default(), MissionTab::Progress);
        assert_eq!(DagTab::default(), DagTab::Graph);
        assert_eq!(NovanetTab::default(), NovanetTab::Summary);
        assert_eq!(ReasoningTab::default(), ReasoningTab::Turns);
    }
}
