//! TUI Views Module
//!
//! Seven-view architecture for Nika TUI (v0.20):
//!
//! **Views (Tab cycling through all 7):**
//! 1. **Browse View** - File browser + DAG preview (default) [b]
//! 2. **Editor View** - YAML editor with validation [e]
//! 3. **Runner View** - Real-time execution monitoring [r]
//! 4. **Chat Playground View** - AI agent conversation interface [c]
//! 5. **Scheduler View** - Cron/queue management [s]
//! 6. **Settings View** - Provider config, theme, preferences [,]
//! 7. **Split View** - Side-by-side Editor + Runner [F9]
//!
//! # Navigation
//!
//! ```text
//!     [1/b]          [2/e]           [3/r]          [4/c]          [5/s]          [6/,]        [F9]
//!  ┌─────────┐   ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐   ┌─────────┐
//!  │ BROWSE  │◄─►│ EDITOR  │◄──►│ RUNNER  │◄──►│  CHAT   │◄──►│SCHEDULER│◄──►│SETTINGS │   │  SPLIT  │
//!  │ Browser │   │  YAML   │    │ Execute │    │Playground│    │  Cron   │    │ Config  │   │ Ed+Run  │
//!  └─────────┘   └─────────┘    └─────────┘    └─────────┘    └─────────┘    └─────────┘   └─────────┘
//!    DEFAULT                                                                                 (toggle)
//! ```
//!
//! Navigation: [Tab] cycles main 6 views, [Shift+Tab] cycles backward.
//! Shortcuts: [1-6] jump directly, [b/e/r/c/s/,] letter shortcuts, [F9] split toggle.

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
// Workspace = 3-panel unified view (Browser + Editor + DAG) [v0.20]
#[allow(unused_imports)]
pub use workspace::{WorkspaceFocus, WorkspaceRatio, WorkspaceView};

// Internal re-exports (original struct names used internally)
// v0.20: Removed ExplorerView alias (unused deprecated alias)
// v0.21: StudioView → YamlEditorPanel (single-panel YAML editor)
pub use home::HomeView;
pub use monitor::MonitorView;
pub use studio::YamlEditorPanel;
// Backwards compatibility: StudioView was the original name for the YAML editor
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

/// Active view in the TUI - 6 views navigation (v0.20)
///
/// v0.20 renames and reorders views:
/// - Explorer → Browse (1, default) `[b]`
/// - Studio → Editor (2) `[e]`
/// - Monitor → Runner (3) `[r]`
/// - Chat → Chat Playground (4) `[c]`
/// - Scheduler (5) `[s]`
/// - Settings (6) `[,]`
/// v0.21: 5-View Architecture (consolidated from 8)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiView {
    /// Studio - 3-panel workspace (Browser | Editor | DAG) [1/s] (default)
    #[default]
    Studio,
    /// Runner - real-time execution monitoring [2/r]
    Runner,
    /// Chat Playground - command Nika conversationally [3/c]
    Chat,
    /// Scheduler - cron/queue management [4/d]
    Scheduler,
    /// Settings - provider config, theme, preferences [5/,]
    Settings,
}

impl TuiView {
    /// Get all 5 views in order (Tab cycles through these)
    /// v0.21: Consolidated from 8 → 5 views
    pub fn all() -> &'static [TuiView] {
        &[
            TuiView::Studio,
            TuiView::Runner,
            TuiView::Chat,
            TuiView::Scheduler,
            TuiView::Settings,
        ]
    }

    /// Alias for all() - all 5 views
    pub fn all_including_auxiliary() -> &'static [TuiView] {
        Self::all()
    }

    /// Check if this is an auxiliary view (only Settings is auxiliary)
    pub fn is_auxiliary(&self) -> bool {
        matches!(self, TuiView::Settings)
    }

    /// Check if this is the Studio view (default, 3-panel workspace)
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
    /// v0.21: 1-5 instead of 1-8
    pub fn number(&self) -> u8 {
        match self {
            TuiView::Studio => 1,
            TuiView::Runner => 2,
            TuiView::Chat => 3,
            TuiView::Scheduler => 4,
            TuiView::Settings => 5,
        }
    }

    /// Get the title for the header bar (v0.21 names)
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
            TuiView::Studio => "🗂",
            TuiView::Runner => "▶",
            TuiView::Chat => "💬",
            TuiView::Scheduler => "📅",
            TuiView::Settings => "⚙",
        }
    }

    /// Get the letter shortcut for the view (v0.21)
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
        assert_eq!(view, TuiView::Browse);
    }

    #[test]
    fn test_tui_view_all_six_views() {
        let views = TuiView::all();
        assert_eq!(views.len(), 6);
        assert_eq!(views[0], TuiView::Browse);
        assert_eq!(views[1], TuiView::Editor);
        assert_eq!(views[2], TuiView::Runner);
        assert_eq!(views[3], TuiView::Chat);
        assert_eq!(views[4], TuiView::Scheduler);
        assert_eq!(views[5], TuiView::Settings);
    }

    #[test]
    fn test_tui_view_all_including_auxiliary_same_as_all() {
        let views = TuiView::all_including_auxiliary();
        assert_eq!(views.len(), 6);
        assert_eq!(views, TuiView::all());
    }

    #[test]
    fn test_tui_view_is_auxiliary() {
        assert!(!TuiView::Browse.is_auxiliary());
        assert!(!TuiView::Editor.is_auxiliary());
        assert!(!TuiView::Runner.is_auxiliary());
        assert!(!TuiView::Chat.is_auxiliary());
        assert!(!TuiView::Scheduler.is_auxiliary());
        assert!(TuiView::Settings.is_auxiliary());
        assert!(!TuiView::Split.is_auxiliary());
    }

    #[test]
    fn test_tui_view_is_split() {
        assert!(!TuiView::Browse.is_split());
        assert!(!TuiView::Editor.is_split());
        assert!(!TuiView::Runner.is_split());
        assert!(!TuiView::Chat.is_split());
        assert!(!TuiView::Scheduler.is_split());
        assert!(!TuiView::Settings.is_split());
        assert!(TuiView::Split.is_split());
    }

    #[test]
    fn test_tui_view_next_cycles_all_six() {
        assert_eq!(TuiView::Browse.next(), TuiView::Editor);
        assert_eq!(TuiView::Editor.next(), TuiView::Runner);
        assert_eq!(TuiView::Runner.next(), TuiView::Chat);
        assert_eq!(TuiView::Chat.next(), TuiView::Scheduler);
        assert_eq!(TuiView::Scheduler.next(), TuiView::Settings);
        assert_eq!(TuiView::Settings.next(), TuiView::Browse);
        // Split exits to Editor (not in cycle)
        assert_eq!(TuiView::Split.next(), TuiView::Editor);
    }

    #[test]
    fn test_tui_view_prev_cycles_all_six() {
        assert_eq!(TuiView::Browse.prev(), TuiView::Settings);
        assert_eq!(TuiView::Editor.prev(), TuiView::Browse);
        assert_eq!(TuiView::Runner.prev(), TuiView::Editor);
        assert_eq!(TuiView::Chat.prev(), TuiView::Runner);
        assert_eq!(TuiView::Scheduler.prev(), TuiView::Chat);
        assert_eq!(TuiView::Settings.prev(), TuiView::Scheduler);
        // Split exits to Editor (not in cycle)
        assert_eq!(TuiView::Split.prev(), TuiView::Editor);
    }

    #[test]
    fn test_tui_view_number_all_seven() {
        assert_eq!(TuiView::Browse.number(), 1);
        assert_eq!(TuiView::Editor.number(), 2);
        assert_eq!(TuiView::Runner.number(), 3);
        assert_eq!(TuiView::Chat.number(), 4);
        assert_eq!(TuiView::Scheduler.number(), 5);
        assert_eq!(TuiView::Settings.number(), 6);
        assert_eq!(TuiView::Split.number(), 7);
    }

    #[test]
    fn test_tui_view_titles_all_seven() {
        assert_eq!(TuiView::Browse.title(), "NIKA BROWSE");
        assert_eq!(TuiView::Editor.title(), "NIKA EDITOR");
        assert_eq!(TuiView::Runner.title(), "NIKA RUNNER");
        assert_eq!(TuiView::Chat.title(), "NIKA CHAT PLAYGROUND");
        assert_eq!(TuiView::Scheduler.title(), "NIKA SCHEDULER");
        assert_eq!(TuiView::Settings.title(), "NIKA SETTINGS");
        assert_eq!(TuiView::Split.title(), "NIKA SPLIT");
    }

    #[test]
    fn test_tui_view_icons_all_seven() {
        assert_eq!(TuiView::Browse.icon(), "📁");
        assert_eq!(TuiView::Editor.icon(), "✏");
        assert_eq!(TuiView::Runner.icon(), "▶");
        assert_eq!(TuiView::Chat.icon(), "💬");
        assert_eq!(TuiView::Scheduler.icon(), "📅");
        assert_eq!(TuiView::Settings.icon(), "⚙");
        assert_eq!(TuiView::Split.icon(), "⊞");
    }

    #[test]
    fn test_tui_view_shortcuts_all_seven() {
        assert_eq!(TuiView::Browse.shortcut(), 'b');
        assert_eq!(TuiView::Editor.shortcut(), 'e');
        assert_eq!(TuiView::Runner.shortcut(), 'r');
        assert_eq!(TuiView::Chat.shortcut(), 'c');
        assert_eq!(TuiView::Scheduler.shortcut(), 's');
        assert_eq!(TuiView::Settings.shortcut(), ',');
        assert_eq!(TuiView::Split.shortcut(), '/'); // Placeholder - Split uses F9
    }

    #[test]
    fn test_tui_view_all_including_split() {
        let views = TuiView::all_including_split();
        assert_eq!(views.len(), 8);
        assert_eq!(views[6], TuiView::Split);
        assert_eq!(views[7], TuiView::Workspace);
    }

    #[test]
    fn test_view_action_switch_to_all_eight_views() {
        let actions = [
            ViewAction::SwitchView(TuiView::Browse),
            ViewAction::SwitchView(TuiView::Editor),
            ViewAction::SwitchView(TuiView::Runner),
            ViewAction::SwitchView(TuiView::Chat),
            ViewAction::SwitchView(TuiView::Scheduler),
            ViewAction::SwitchView(TuiView::Settings),
            ViewAction::SwitchView(TuiView::Split),
            ViewAction::SwitchView(TuiView::Workspace),
        ];
        assert_eq!(actions.len(), 8);
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
