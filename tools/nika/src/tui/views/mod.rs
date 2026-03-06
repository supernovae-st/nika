//! TUI Views Module
//!
//! Five-view architecture for Nika TUI (v0.21):
//!
//! **Views (Tab cycling through all 5):**
//! 1. **Studio View** - Unified editor with browser + YAML editor + DAG (default) [s]
//! 2. **Runner View** - Real-time execution monitoring [r]
//! 3. **Chat Playground View** - AI agent conversation interface [c]
//! 4. **Scheduler View** - Cron/queue management [d]
//! 5. **Settings View** - Provider config, theme, preferences [,]
//!
//! # Navigation
//!
//! ```text
//!     [1/s]          [2/r]           [3/c]          [4/d]          [5/,]
//!  ┌─────────┐   ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐
//!  │ STUDIO  │◄─►│ RUNNER  │◄──►│  CHAT   │◄──►│SCHEDULER│◄──►│SETTINGS │
//!  │ Editor  │   │ Execute │    │Playground│    │  Cron   │    │ Config  │
//!  └─────────┘   └─────────┘    └─────────┘    └─────────┘    └─────────┘
//!    DEFAULT
//! ```
//!
//! Navigation: [Tab] cycles all 5 views, [Shift+Tab] cycles backward.
//! Shortcuts: [1-5] jump directly, [s/r/c/d/,] letter shortcuts.

mod chat;
mod help;
mod home;
mod monitor;
mod scheduler;
mod settings;
mod split;
mod studio;
mod trait_view;
mod workspace;

// Main view exports (v0.20 names)
#[allow(unused_imports)]
pub use chat::{ChatMode, ChatView, MessageRole};
// Browse = Home (renamed in v0.20 from Explorer)
#[allow(unused_imports)]
pub use home::HomeView as BrowseView;
// Editor = YamlEditorPanel (renamed in v0.21)
#[allow(unused_imports)]
pub use studio::{EditorMode, YamlEditorPanel as EditorView};
// Runner = Monitor (renamed in v0.12)
#[allow(unused_imports)]
pub use monitor::MonitorView as RunnerView;
// Scheduler = NEW in v0.12
#[allow(unused_imports)]
pub use scheduler::SchedulerView;
// Settings stays the same
pub use settings::SettingsView;
// Split = NEW in v0.13 (Editor + Runner side-by-side)
#[allow(unused_imports)]
pub use split::{SplitFocus, SplitRatio, SplitView};
// Workspace = NEW in v0.20 (Browser + Editor + DAG unified)
#[allow(unused_imports)]
pub use workspace::{WorkspaceFocus, WorkspaceRatio, WorkspaceView};

// Internal re-exports (original struct names used internally)
// v0.20: Removed ExplorerView alias (unused deprecated alias)
// v0.21: StudioView → YamlEditorPanel (single-panel YAML editor)
pub use home::HomeView;
pub use monitor::MonitorView;
pub use studio::YamlEditorPanel;
// Backwards compatibility: StudioView was the original name
#[allow(unused_imports)]
pub use studio::YamlEditorPanel as StudioView;
// Help view still exists but is no longer a main TuiView (merged into Settings)
#[allow(unused_imports)]
pub use help::HelpView;

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

/// Active view in the TUI - 5 views navigation (v0.21)
///
/// v0.21 consolidates to 5 views:
/// - Studio (1, default) [s] - Unified editor with browser + YAML editor + DAG preview
/// - Runner (2) [r] - Real-time execution monitoring
/// - Chat (3) [c] - Conversational playground
/// - Scheduler (4) [d] - Cron/queue management
/// - Settings (5) [,] - Provider config, theme, preferences
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiView {
    /// Studio - Unified editor with browser + YAML editor + DAG preview [1/s]
    #[default]
    Studio,
    /// Runner - Real-time execution monitoring [2/r]
    Runner,
    /// Chat Playground - Command Nika conversationally [3/c]
    Chat,
    /// Scheduler - Cron/queue management [4/d]
    Scheduler,
    /// Settings - Provider config, theme, preferences [5/,]
    Settings,
}

impl TuiView {
    /// Get all 5 views in order (Tab cycles through all)
    pub fn all() -> &'static [TuiView] {
        &[
            TuiView::Studio,
            TuiView::Runner,
            TuiView::Chat,
            TuiView::Scheduler,
            TuiView::Settings,
        ]
    }

    /// Check if this is the settings view (non-workflow view)
    pub fn is_auxiliary(&self) -> bool {
        matches!(self, TuiView::Settings)
    }

    /// Check if this is the studio view
    pub fn is_studio(&self) -> bool {
        matches!(self, TuiView::Studio)
    }

    /// Get next view (cycling through all 5 views)
    pub fn next(&self) -> Self {
        match self {
            TuiView::Studio => TuiView::Runner,
            TuiView::Runner => TuiView::Chat,
            TuiView::Chat => TuiView::Scheduler,
            TuiView::Scheduler => TuiView::Settings,
            TuiView::Settings => TuiView::Studio,
        }
    }

    /// Get previous view (cycling through all 5 views)
    pub fn prev(&self) -> Self {
        match self {
            TuiView::Studio => TuiView::Settings,
            TuiView::Runner => TuiView::Studio,
            TuiView::Chat => TuiView::Runner,
            TuiView::Scheduler => TuiView::Chat,
            TuiView::Settings => TuiView::Scheduler,
        }
    }

    /// Get view number (1-indexed for display)
    pub fn number(&self) -> u8 {
        match self {
            TuiView::Studio => 1,
            TuiView::Runner => 2,
            TuiView::Chat => 3,
            TuiView::Scheduler => 4,
            TuiView::Settings => 5,
        }
    }

    /// Get the title for the header bar
    pub fn title(&self) -> &'static str {
        match self {
            TuiView::Studio => "NIKA STUDIO",
            TuiView::Runner => "NIKA RUNNER",
            TuiView::Chat => "NIKA CHAT PLAYGROUND",
            TuiView::Scheduler => "NIKA SCHEDULER",
            TuiView::Settings => "NIKA SETTINGS",
        }
    }

    /// Get the icon for the view (terminal-friendly)
    pub fn icon(&self) -> &'static str {
        match self {
            TuiView::Studio => "✏",
            TuiView::Runner => "▶",
            TuiView::Chat => "💬",
            TuiView::Scheduler => "📅",
            TuiView::Settings => "⚙",
        }
    }

    /// Get the letter shortcut for the view
    pub fn shortcut(&self) -> char {
        match self {
            TuiView::Studio => 's',
            TuiView::Runner => 'r',
            TuiView::Chat => 'c',
            TuiView::Scheduler => 'd',
            TuiView::Settings => ',',
        }
    }

    /// Toggle to the next view (for backwards compatibility)
    pub fn toggle(&self) -> Self {
        self.next()
    }
}

/// Model provider for LLM switching
pub use crate::tui::command::{McpAction, ModelProvider};
/// Theme variant for direct theme selection (v0.12.0)
pub use crate::tui::tokens::CosmicVariant;

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
    /// Set specific theme by variant (v0.12.0 - fix for `[1][2][3]` selection)
    SetTheme(CosmicVariant),
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
    /// Validate workflow in Home view (v0.11.0)
    ValidateWorkflow(std::path::PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_view_default() {
        let view = TuiView::default();
        assert_eq!(view, TuiView::Studio);
    }

    #[test]
    fn test_tui_view_all_five_views() {
        let views = TuiView::all();
        assert_eq!(views.len(), 5);
        assert_eq!(views[0], TuiView::Studio);
        assert_eq!(views[1], TuiView::Runner);
        assert_eq!(views[2], TuiView::Chat);
        assert_eq!(views[3], TuiView::Scheduler);
        assert_eq!(views[4], TuiView::Settings);
    }

    #[test]
    fn test_tui_view_all_has_all_five_views() {
        let views = TuiView::all();
        // 5-view architecture v0.21.0
        assert_eq!(views.len(), 5);
        assert_eq!(views[0], TuiView::Studio);
        assert_eq!(views[1], TuiView::Runner);
        assert_eq!(views[2], TuiView::Chat);
        assert_eq!(views[3], TuiView::Scheduler);
        assert_eq!(views[4], TuiView::Settings);
    }

    #[test]
    fn test_tui_view_is_auxiliary() {
        // 5-view architecture: only Settings is auxiliary
        assert!(!TuiView::Studio.is_auxiliary());
        assert!(!TuiView::Runner.is_auxiliary());
        assert!(!TuiView::Chat.is_auxiliary());
        assert!(!TuiView::Scheduler.is_auxiliary());
        assert!(TuiView::Settings.is_auxiliary());
    }

    #[test]
    fn test_tui_view_is_studio() {
        // 5-view architecture: only Studio is studio
        assert!(TuiView::Studio.is_studio());
        assert!(!TuiView::Runner.is_studio());
        assert!(!TuiView::Chat.is_studio());
        assert!(!TuiView::Scheduler.is_studio());
        assert!(!TuiView::Settings.is_studio());
    }

    #[test]
    fn test_tui_view_next_cycles_all_five() {
        assert_eq!(TuiView::Studio.next(), TuiView::Runner);
        assert_eq!(TuiView::Runner.next(), TuiView::Chat);
        assert_eq!(TuiView::Chat.next(), TuiView::Scheduler);
        assert_eq!(TuiView::Scheduler.next(), TuiView::Settings);
        assert_eq!(TuiView::Settings.next(), TuiView::Studio);
    }

    #[test]
    fn test_tui_view_prev_cycles_all_five() {
        assert_eq!(TuiView::Studio.prev(), TuiView::Settings);
        assert_eq!(TuiView::Runner.prev(), TuiView::Studio);
        assert_eq!(TuiView::Chat.prev(), TuiView::Runner);
        assert_eq!(TuiView::Scheduler.prev(), TuiView::Chat);
        assert_eq!(TuiView::Settings.prev(), TuiView::Scheduler);
    }

    #[test]
    fn test_tui_view_number_all_five() {
        assert_eq!(TuiView::Studio.number(), 1);
        assert_eq!(TuiView::Runner.number(), 2);
        assert_eq!(TuiView::Chat.number(), 3);
        assert_eq!(TuiView::Scheduler.number(), 4);
        assert_eq!(TuiView::Settings.number(), 5);
    }

    #[test]
    fn test_tui_view_titles_all_five() {
        assert_eq!(TuiView::Studio.title(), "NIKA STUDIO");
        assert_eq!(TuiView::Runner.title(), "NIKA RUNNER");
        assert_eq!(TuiView::Chat.title(), "NIKA CHAT PLAYGROUND");
        assert_eq!(TuiView::Scheduler.title(), "NIKA SCHEDULER");
        assert_eq!(TuiView::Settings.title(), "NIKA SETTINGS");
    }

    #[test]
    fn test_tui_view_icons_all_five() {
        assert_eq!(TuiView::Studio.icon(), "✏");
        assert_eq!(TuiView::Runner.icon(), "▶");
        assert_eq!(TuiView::Chat.icon(), "💬");
        assert_eq!(TuiView::Scheduler.icon(), "📅");
        assert_eq!(TuiView::Settings.icon(), "⚙");
    }

    #[test]
    fn test_tui_view_shortcuts_all_five() {
        assert_eq!(TuiView::Studio.shortcut(), 's');
        assert_eq!(TuiView::Runner.shortcut(), 'r');
        assert_eq!(TuiView::Chat.shortcut(), 'c');
        assert_eq!(TuiView::Scheduler.shortcut(), 'd');
        assert_eq!(TuiView::Settings.shortcut(), ',');
    }

    #[test]
    fn test_view_action_switch_to_all_five_views() {
        let actions = [
            ViewAction::SwitchView(TuiView::Studio),
            ViewAction::SwitchView(TuiView::Runner),
            ViewAction::SwitchView(TuiView::Chat),
            ViewAction::SwitchView(TuiView::Scheduler),
            ViewAction::SwitchView(TuiView::Settings),
        ];
        assert_eq!(actions.len(), 5);
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

    // ════════════════════════════════════════════════════════════════════════
    // ViewAction::SetTheme tests (v0.12.0)
    // ════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_view_action_set_theme_all_variants() {
        let actions = [
            ViewAction::SetTheme(CosmicVariant::CosmicDark),
            ViewAction::SetTheme(CosmicVariant::CosmicLight),
            ViewAction::SetTheme(CosmicVariant::CosmicViolet),
        ];
        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn test_view_action_set_theme_variant_check() {
        let action = ViewAction::SetTheme(CosmicVariant::CosmicLight);
        match action {
            ViewAction::SetTheme(variant) => {
                assert_eq!(variant, CosmicVariant::CosmicLight);
            }
            _ => panic!("Expected SetTheme"),
        }
    }
}
