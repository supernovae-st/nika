//! TUI Views Module
//!
//! Six-view architecture for Nika TUI (v0.11):
//!
//! **Main Views (Tab cycling):**
//! 1. **Chat View** - AI agent conversation interface
//! 2. **Home View** - Workflow browser (default)
//! 3. **Studio View** - YAML editor with validation
//! 4. **Monitor View** - Real-time execution monitoring
//!
//! **Auxiliary Views (accessed via shortcuts):**
//! 5. **Settings View** - Provider config, theme, preferences
//! 6. **Help View** - Keyboard shortcuts reference
//!
//! # Navigation
//!
//! ```text
//!     [1/a]          [2/h]           [3/s]          [4/m]
//!  ┌─────────┐   ┌─────────┐    ┌─────────┐    ┌─────────┐
//!  │  CHAT   │◄─►│  HOME   │◄──►│ STUDIO  │◄──►│ MONITOR │
//!  │  Agent  │   │ Browser │    │  Editor │    │ Execute │
//!  └─────────┘   └─────────┘    └─────────┘    └─────────┘
//!                     ▲              ▲              ▲
//!                     │              │              │
//!                     ▼              ▼              ▼
//!              ┌───────────┐  ┌───────────┐
//!              │ SETTINGS  │  │   HELP    │
//!              │  [Ctrl+,] │  │    [?]    │
//!              └───────────┘  └───────────┘
//! ```
//!
//! Navigation: [Tab] cycles main 4 views, [Shift+Tab] cycles backward.
//! Shortcuts: [1-4] jump to main views, [?] opens Help, [Ctrl+,] opens Settings.

mod chat;
mod help;
mod home;
mod monitor;
mod settings;
mod studio;
mod trait_view;

// Main view exports
#[allow(unused_imports)]
pub use chat::{ChatMode, ChatView, MessageRole};
pub use home::HomeView;
pub use monitor::MonitorView;
pub use studio::{EditorMode, StudioView};

// Auxiliary view exports (v0.11)
pub use help::HelpView;
pub use settings::SettingsView;

// Trait export
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
    /// Claude Code-like step-by-step view (v0.8)
    Steps,
}

impl ReasoningTab {
    pub fn next(&self) -> Self {
        match self {
            ReasoningTab::Turns => ReasoningTab::Thinking,
            ReasoningTab::Thinking => ReasoningTab::Steps,
            ReasoningTab::Steps => ReasoningTab::Turns,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            ReasoningTab::Turns => "Turns",
            ReasoningTab::Thinking => "Thinking",
            ReasoningTab::Steps => "Steps",
        }
    }
}

/// Active view in the TUI - 6 views navigation (v0.11)
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
    /// Settings - provider config, theme, preferences (v0.11.1)
    Settings,
    /// Help - keyboard shortcuts reference (v0.11.2)
    Help,
}

impl TuiView {
    /// Get all views in order (main 4 views for Tab cycling)
    pub fn all() -> &'static [TuiView] {
        &[
            TuiView::Chat,
            TuiView::Home,
            TuiView::Studio,
            TuiView::Monitor,
        ]
    }

    /// Get all 6 views including auxiliary (v0.11)
    pub fn all_including_auxiliary() -> &'static [TuiView] {
        &[
            TuiView::Chat,
            TuiView::Home,
            TuiView::Studio,
            TuiView::Monitor,
            TuiView::Settings,
            TuiView::Help,
        ]
    }

    /// Check if this is an auxiliary view (Settings/Help)
    pub fn is_auxiliary(&self) -> bool {
        matches!(self, TuiView::Settings | TuiView::Help)
    }

    /// Get next view (cycling through main 4 views only)
    pub fn next(&self) -> Self {
        match self {
            TuiView::Chat => TuiView::Home,
            TuiView::Home => TuiView::Studio,
            TuiView::Studio => TuiView::Monitor,
            TuiView::Monitor => TuiView::Chat,
            // Auxiliary views return to Home on next
            TuiView::Settings | TuiView::Help => TuiView::Home,
        }
    }

    /// Get previous view (cycling through main 4 views only)
    pub fn prev(&self) -> Self {
        match self {
            TuiView::Chat => TuiView::Monitor,
            TuiView::Home => TuiView::Chat,
            TuiView::Studio => TuiView::Home,
            TuiView::Monitor => TuiView::Studio,
            // Auxiliary views return to Home on prev
            TuiView::Settings | TuiView::Help => TuiView::Home,
        }
    }

    /// Get view number (1-indexed for display, 0 for auxiliary)
    pub fn number(&self) -> u8 {
        match self {
            TuiView::Chat => 1,
            TuiView::Home => 2,
            TuiView::Studio => 3,
            TuiView::Monitor => 4,
            TuiView::Settings => 5,
            TuiView::Help => 6,
        }
    }

    /// Get the title for the header bar
    pub fn title(&self) -> &'static str {
        match self {
            TuiView::Chat => "NIKA AGENT",
            TuiView::Home => "NIKA HOME",
            TuiView::Studio => "NIKA STUDIO",
            TuiView::Monitor => "NIKA MONITOR",
            TuiView::Settings => "NIKA SETTINGS",
            TuiView::Help => "NIKA HELP",
        }
    }

    /// Get the icon for the view (terminal-friendly)
    pub fn icon(&self) -> &'static str {
        match self {
            TuiView::Chat => "◆",
            TuiView::Home => "◆",
            TuiView::Studio => "◆",
            TuiView::Monitor => "◆",
            TuiView::Settings => "⚙",
            TuiView::Help => "?",
        }
    }

    /// Toggle to the next view (for backwards compatibility)
    pub fn toggle(&self) -> Self {
        self.next()
    }
}

/// Model provider for LLM switching
pub use crate::tui::command::{McpAction, ModelProvider};

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
    /// Execute /agent command - multi-turn agent (goal, max_turns, extended_thinking, mcp_servers)
    ChatAgent(String, Option<u32>, bool, Vec<String>),
    /// Execute /model command - switch LLM provider
    ChatModelSwitch(ModelProvider),
    /// Execute /mcp command - MCP server management (v0.5.2)
    ChatMcp(McpAction),
    /// Execute /clear command - clear chat history
    ChatClear,
    /// Open Settings view
    OpenSettings,
    /// Toggle theme (v0.8.1)
    ToggleTheme,
    /// Verify all configured providers (v0.8.2)
    VerifyProviders,
    /// Refresh verification (invalidate cache + re-verify) (v0.8.2)
    RefreshVerification,
    /// Provider selector confirmed a change (v0.8.3 - BUG #2 fix)
    /// Signals app.rs to invalidate/recreate chat_agent with new provider
    ProviderSelectorConfirm { provider_id: String, model: String },
    /// Pull Ollama model (v0.12.3)
    PullOllamaModel(String),
    /// Delete Ollama model (v0.12.3)
    DeleteOllamaModel(String),
    /// Refresh Ollama model list (v0.12.3)
    RefreshOllamaModels,
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
    fn test_tui_view_all_main_four_variants() {
        let views = TuiView::all();
        assert_eq!(views.len(), 4);
        assert_eq!(views[0], TuiView::Chat);
        assert_eq!(views[1], TuiView::Home);
        assert_eq!(views[2], TuiView::Studio);
        assert_eq!(views[3], TuiView::Monitor);
    }

    #[test]
    fn test_tui_view_all_including_auxiliary() {
        let views = TuiView::all_including_auxiliary();
        assert_eq!(views.len(), 6);
        assert_eq!(views[4], TuiView::Settings);
        assert_eq!(views[5], TuiView::Help);
    }

    #[test]
    fn test_tui_view_is_auxiliary() {
        assert!(!TuiView::Chat.is_auxiliary());
        assert!(!TuiView::Home.is_auxiliary());
        assert!(!TuiView::Studio.is_auxiliary());
        assert!(!TuiView::Monitor.is_auxiliary());
        assert!(TuiView::Settings.is_auxiliary());
        assert!(TuiView::Help.is_auxiliary());
    }

    #[test]
    fn test_tui_view_next_cycles_main_views() {
        assert_eq!(TuiView::Chat.next(), TuiView::Home);
        assert_eq!(TuiView::Home.next(), TuiView::Studio);
        assert_eq!(TuiView::Studio.next(), TuiView::Monitor);
        assert_eq!(TuiView::Monitor.next(), TuiView::Chat);
    }

    #[test]
    fn test_tui_view_auxiliary_next_returns_home() {
        assert_eq!(TuiView::Settings.next(), TuiView::Home);
        assert_eq!(TuiView::Help.next(), TuiView::Home);
    }

    #[test]
    fn test_tui_view_prev_cycles_main_views() {
        assert_eq!(TuiView::Chat.prev(), TuiView::Monitor);
        assert_eq!(TuiView::Home.prev(), TuiView::Chat);
        assert_eq!(TuiView::Studio.prev(), TuiView::Home);
        assert_eq!(TuiView::Monitor.prev(), TuiView::Studio);
    }

    #[test]
    fn test_tui_view_auxiliary_prev_returns_home() {
        assert_eq!(TuiView::Settings.prev(), TuiView::Home);
        assert_eq!(TuiView::Help.prev(), TuiView::Home);
    }

    #[test]
    fn test_tui_view_number_all_six() {
        assert_eq!(TuiView::Chat.number(), 1);
        assert_eq!(TuiView::Home.number(), 2);
        assert_eq!(TuiView::Studio.number(), 3);
        assert_eq!(TuiView::Monitor.number(), 4);
        assert_eq!(TuiView::Settings.number(), 5);
        assert_eq!(TuiView::Help.number(), 6);
    }

    #[test]
    fn test_tui_view_titles_all_six() {
        assert_eq!(TuiView::Chat.title(), "NIKA AGENT");
        assert_eq!(TuiView::Home.title(), "NIKA HOME");
        assert_eq!(TuiView::Studio.title(), "NIKA STUDIO");
        assert_eq!(TuiView::Monitor.title(), "NIKA MONITOR");
        assert_eq!(TuiView::Settings.title(), "NIKA SETTINGS");
        assert_eq!(TuiView::Help.title(), "NIKA HELP");
    }

    #[test]
    fn test_tui_view_icons_all_six() {
        assert_eq!(TuiView::Chat.icon(), "◆");
        assert_eq!(TuiView::Home.icon(), "◆");
        assert_eq!(TuiView::Studio.icon(), "◆");
        assert_eq!(TuiView::Monitor.icon(), "◆");
        assert_eq!(TuiView::Settings.icon(), "⚙");
        assert_eq!(TuiView::Help.icon(), "?");
    }

    #[test]
    fn test_view_action_switch_to_all_six_views() {
        let actions = [
            ViewAction::SwitchView(TuiView::Chat),
            ViewAction::SwitchView(TuiView::Home),
            ViewAction::SwitchView(TuiView::Studio),
            ViewAction::SwitchView(TuiView::Monitor),
            ViewAction::SwitchView(TuiView::Settings),
            ViewAction::SwitchView(TuiView::Help),
        ];
        assert_eq!(actions.len(), 6);
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
        assert_eq!(tab.next().next(), ReasoningTab::Steps);
        assert_eq!(tab.next().next().next(), ReasoningTab::Turns);
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
        assert_eq!(ReasoningTab::Steps.title(), "Steps");
    }

    #[test]
    fn test_tab_defaults() {
        assert_eq!(MissionTab::default(), MissionTab::Progress);
        assert_eq!(DagTab::default(), DagTab::Graph);
        assert_eq!(NovanetTab::default(), NovanetTab::Summary);
        assert_eq!(ReasoningTab::default(), ReasoningTab::Turns);
    }
}
