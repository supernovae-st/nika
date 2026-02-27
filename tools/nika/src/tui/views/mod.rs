//! TUI Views Module
//!
//! Seven-view architecture for Nika TUI (v0.13):
//!
//! **Views (Tab cycling through all 7):**
//! 1. **Explorer View** - File browser + DAG preview (default) [e]
//! 2. **Chat View** - AI agent conversation interface [c]
//! 3. **Editor View** - YAML editor with validation [d]
//! 4. **Runner View** - Real-time execution monitoring [r]
//! 5. **Scheduler View** - Cron/queue management [s]
//! 6. **Settings View** - Provider config, theme, preferences [,]
//! 7. **Split View** - Side-by-side Editor + Runner [F9]
//!
//! # Navigation
//!
//! ```text
//!     [1/e]          [2/c]           [3/d]          [4/r]          [5/s]          [6/,]        [F9]
//!  ┌─────────┐   ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐   ┌─────────┐
//!  │EXPLORER │◄─►│  CHAT   │◄──►│ EDITOR  │◄──►│ RUNNER  │◄──►│SCHEDULER│◄──►│SETTINGS │   │  SPLIT  │
//!  │ Browser │   │  Agent  │    │  YAML   │    │ Execute │    │  Cron   │    │ Config  │   │ Ed+Run  │
//!  └─────────┘   └─────────┘    └─────────┘    └─────────┘    └─────────┘    └─────────┘   └─────────┘
//!    DEFAULT                                                                                 (toggle)
//! ```
//!
//! Navigation: [Tab] cycles main 6 views, [Shift+Tab] cycles backward.
//! Shortcuts: [1-6] jump directly, [e/c/d/r/s/,] letter shortcuts, [F9] split toggle.

mod chat;
mod help;
mod home;
mod monitor;
mod scheduler;
mod settings;
mod split;
mod studio;
mod trait_view;

// Main view exports (v0.12 names)
#[allow(unused_imports)]
pub use chat::{ChatMode, ChatView, MessageRole};
// Explorer = Home (renamed in v0.12)
#[allow(unused_imports)]
pub use home::HomeView as ExplorerView;
// Editor = Studio (renamed in v0.12)
#[allow(unused_imports)]
pub use studio::{EditorMode, StudioView as EditorView};
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

// Legacy aliases for backwards compatibility (deprecated in v0.12)
#[deprecated(since = "0.12.0", note = "Use ExplorerView instead")]
pub use home::HomeView;
#[deprecated(since = "0.12.0", note = "Use RunnerView instead")]
pub use monitor::MonitorView;
#[deprecated(since = "0.12.0", note = "Use EditorView instead")]
pub use studio::StudioView;
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

/// Active view in the TUI - 6 views navigation (v0.12)
///
/// v0.12 renames and reorders views per the 6-Views Design spec:
/// - Home → Explorer (1, default)
/// - Chat stays Chat (2)
/// - Studio → Editor (3)
/// - Monitor → Runner (4)
/// - NEW: Scheduler (5)
/// - Settings stays (6)
/// - Help merged into Settings (no longer a TuiView)
/// - NEW: Split (v0.13) - Editor + Runner side-by-side (F9)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiView {
    /// Explorer - file browser + DAG preview (default) [1/e]
    #[default]
    Explorer,
    /// Chat agent - command Nika conversationally [2/c]
    Chat,
    /// Editor - edit YAML with validation [3/d]
    Editor,
    /// Runner - real-time execution monitoring [4/r]
    Runner,
    /// Scheduler - cron/queue management [5/s]
    Scheduler,
    /// Settings - provider config, theme, preferences [6/,]
    Settings,
    /// Split - side-by-side Editor + Runner (v0.13) (F9 key)
    Split,
}

impl TuiView {
    /// Get all 6 main views in order (Tab cycles through these)
    /// Note: Split view is excluded - accessible via F9 toggle
    pub fn all() -> &'static [TuiView] {
        &[
            TuiView::Explorer,
            TuiView::Chat,
            TuiView::Editor,
            TuiView::Runner,
            TuiView::Scheduler,
            TuiView::Settings,
        ]
    }

    /// Get all 7 views including Split (v0.13)
    pub fn all_including_split() -> &'static [TuiView] {
        &[
            TuiView::Explorer,
            TuiView::Chat,
            TuiView::Editor,
            TuiView::Runner,
            TuiView::Scheduler,
            TuiView::Settings,
            TuiView::Split,
        ]
    }

    /// Alias for all() - main 6 views (v0.12: no auxiliary distinction)
    pub fn all_including_auxiliary() -> &'static [TuiView] {
        Self::all()
    }

    /// Check if this is an auxiliary view (v0.12: only Settings is auxiliary)
    pub fn is_auxiliary(&self) -> bool {
        matches!(self, TuiView::Settings)
    }

    /// Check if this is the split view (v0.13)
    pub fn is_split(&self) -> bool {
        matches!(self, TuiView::Split)
    }

    /// Get next view (cycling through main 6 views, Split returns to Editor)
    pub fn next(&self) -> Self {
        match self {
            TuiView::Explorer => TuiView::Chat,
            TuiView::Chat => TuiView::Editor,
            TuiView::Editor => TuiView::Runner,
            TuiView::Runner => TuiView::Scheduler,
            TuiView::Scheduler => TuiView::Settings,
            TuiView::Settings => TuiView::Explorer,
            TuiView::Split => TuiView::Editor, // Split exits to Editor
        }
    }

    /// Get previous view (cycling through main 6 views, Split returns to Editor)
    pub fn prev(&self) -> Self {
        match self {
            TuiView::Explorer => TuiView::Settings,
            TuiView::Chat => TuiView::Explorer,
            TuiView::Editor => TuiView::Chat,
            TuiView::Runner => TuiView::Editor,
            TuiView::Scheduler => TuiView::Runner,
            TuiView::Settings => TuiView::Scheduler,
            TuiView::Split => TuiView::Editor, // Split exits to Editor
        }
    }

    /// Get view number (1-indexed for display, Split is 7)
    pub fn number(&self) -> u8 {
        match self {
            TuiView::Explorer => 1,
            TuiView::Chat => 2,
            TuiView::Editor => 3,
            TuiView::Runner => 4,
            TuiView::Scheduler => 5,
            TuiView::Settings => 6,
            TuiView::Split => 7,
        }
    }

    /// Get the title for the header bar (v0.12 names)
    pub fn title(&self) -> &'static str {
        match self {
            TuiView::Explorer => "NIKA EXPLORER",
            TuiView::Chat => "NIKA CHAT",
            TuiView::Editor => "NIKA EDITOR",
            TuiView::Runner => "NIKA RUNNER",
            TuiView::Scheduler => "NIKA SCHEDULER",
            TuiView::Settings => "NIKA SETTINGS",
            TuiView::Split => "NIKA SPLIT",
        }
    }

    /// Get the icon for the view (terminal-friendly)
    pub fn icon(&self) -> &'static str {
        match self {
            TuiView::Explorer => "📁",
            TuiView::Chat => "💬",
            TuiView::Editor => "✏",
            TuiView::Runner => "▶",
            TuiView::Scheduler => "📅",
            TuiView::Settings => "⚙",
            TuiView::Split => "⊞",
        }
    }

    /// Get the letter shortcut for the view (v0.12)
    /// Note: Split uses F9, not a letter shortcut
    pub fn shortcut(&self) -> char {
        match self {
            TuiView::Explorer => 'e',
            TuiView::Chat => 'c',
            TuiView::Editor => 'd',
            TuiView::Runner => 'r',
            TuiView::Scheduler => 's',
            TuiView::Settings => ',',
            TuiView::Split => '/', // Placeholder - Split uses F9
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
        assert_eq!(view, TuiView::Explorer);
    }

    #[test]
    fn test_tui_view_all_six_views() {
        let views = TuiView::all();
        assert_eq!(views.len(), 6);
        assert_eq!(views[0], TuiView::Explorer);
        assert_eq!(views[1], TuiView::Chat);
        assert_eq!(views[2], TuiView::Editor);
        assert_eq!(views[3], TuiView::Runner);
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
        assert!(!TuiView::Explorer.is_auxiliary());
        assert!(!TuiView::Chat.is_auxiliary());
        assert!(!TuiView::Editor.is_auxiliary());
        assert!(!TuiView::Runner.is_auxiliary());
        assert!(!TuiView::Scheduler.is_auxiliary());
        assert!(TuiView::Settings.is_auxiliary());
        assert!(!TuiView::Split.is_auxiliary());
    }

    #[test]
    fn test_tui_view_is_split() {
        assert!(!TuiView::Explorer.is_split());
        assert!(!TuiView::Chat.is_split());
        assert!(!TuiView::Editor.is_split());
        assert!(!TuiView::Runner.is_split());
        assert!(!TuiView::Scheduler.is_split());
        assert!(!TuiView::Settings.is_split());
        assert!(TuiView::Split.is_split());
    }

    #[test]
    fn test_tui_view_next_cycles_all_six() {
        assert_eq!(TuiView::Explorer.next(), TuiView::Chat);
        assert_eq!(TuiView::Chat.next(), TuiView::Editor);
        assert_eq!(TuiView::Editor.next(), TuiView::Runner);
        assert_eq!(TuiView::Runner.next(), TuiView::Scheduler);
        assert_eq!(TuiView::Scheduler.next(), TuiView::Settings);
        assert_eq!(TuiView::Settings.next(), TuiView::Explorer);
        // Split exits to Editor (not in cycle)
        assert_eq!(TuiView::Split.next(), TuiView::Editor);
    }

    #[test]
    fn test_tui_view_prev_cycles_all_six() {
        assert_eq!(TuiView::Explorer.prev(), TuiView::Settings);
        assert_eq!(TuiView::Chat.prev(), TuiView::Explorer);
        assert_eq!(TuiView::Editor.prev(), TuiView::Chat);
        assert_eq!(TuiView::Runner.prev(), TuiView::Editor);
        assert_eq!(TuiView::Scheduler.prev(), TuiView::Runner);
        assert_eq!(TuiView::Settings.prev(), TuiView::Scheduler);
        // Split exits to Editor (not in cycle)
        assert_eq!(TuiView::Split.prev(), TuiView::Editor);
    }

    #[test]
    fn test_tui_view_number_all_seven() {
        assert_eq!(TuiView::Explorer.number(), 1);
        assert_eq!(TuiView::Chat.number(), 2);
        assert_eq!(TuiView::Editor.number(), 3);
        assert_eq!(TuiView::Runner.number(), 4);
        assert_eq!(TuiView::Scheduler.number(), 5);
        assert_eq!(TuiView::Settings.number(), 6);
        assert_eq!(TuiView::Split.number(), 7);
    }

    #[test]
    fn test_tui_view_titles_all_seven() {
        assert_eq!(TuiView::Explorer.title(), "NIKA EXPLORER");
        assert_eq!(TuiView::Chat.title(), "NIKA CHAT");
        assert_eq!(TuiView::Editor.title(), "NIKA EDITOR");
        assert_eq!(TuiView::Runner.title(), "NIKA RUNNER");
        assert_eq!(TuiView::Scheduler.title(), "NIKA SCHEDULER");
        assert_eq!(TuiView::Settings.title(), "NIKA SETTINGS");
        assert_eq!(TuiView::Split.title(), "NIKA SPLIT");
    }

    #[test]
    fn test_tui_view_icons_all_seven() {
        assert_eq!(TuiView::Explorer.icon(), "📁");
        assert_eq!(TuiView::Chat.icon(), "💬");
        assert_eq!(TuiView::Editor.icon(), "✏");
        assert_eq!(TuiView::Runner.icon(), "▶");
        assert_eq!(TuiView::Scheduler.icon(), "📅");
        assert_eq!(TuiView::Settings.icon(), "⚙");
        assert_eq!(TuiView::Split.icon(), "⊞");
    }

    #[test]
    fn test_tui_view_shortcuts_all_seven() {
        assert_eq!(TuiView::Explorer.shortcut(), 'e');
        assert_eq!(TuiView::Chat.shortcut(), 'c');
        assert_eq!(TuiView::Editor.shortcut(), 'd');
        assert_eq!(TuiView::Runner.shortcut(), 'r');
        assert_eq!(TuiView::Scheduler.shortcut(), 's');
        assert_eq!(TuiView::Settings.shortcut(), ',');
        assert_eq!(TuiView::Split.shortcut(), '/'); // Placeholder - Split uses F9
    }

    #[test]
    fn test_tui_view_all_including_split() {
        let views = TuiView::all_including_split();
        assert_eq!(views.len(), 7);
        assert_eq!(views[6], TuiView::Split);
    }

    #[test]
    fn test_view_action_switch_to_all_seven_views() {
        let actions = [
            ViewAction::SwitchView(TuiView::Explorer),
            ViewAction::SwitchView(TuiView::Chat),
            ViewAction::SwitchView(TuiView::Editor),
            ViewAction::SwitchView(TuiView::Runner),
            ViewAction::SwitchView(TuiView::Scheduler),
            ViewAction::SwitchView(TuiView::Settings),
            ViewAction::SwitchView(TuiView::Split),
        ];
        assert_eq!(actions.len(), 7);
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
