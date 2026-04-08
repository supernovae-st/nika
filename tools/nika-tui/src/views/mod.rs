// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TUI Views Module
//!
//! Three-view architecture for Nika TUI:
//!
//! **Views (Tab cycling through all 3):**
//! 1. **Studio View** - Unified editor with browser + YAML editor + DAG (default) \[s\]
//! 2. **Command View** - Fusion of Runner + Chat (execution + conversation) \[c\]
//! 3. **Control View** - Provider config, theme, preferences \[x\]
//!
//! # Navigation
//!
//! ```text
//!     [1/s]          [2/c]           [3/x]
//!  ┌─────────┐   ┌─────────┐    ┌─────────┐
//!  │ STUDIO  │◄─►│ COMMAND │◄──►│ CONTROL │
//!  │ Editor  │   │ Run+Chat│    │ Config  │
//!  └─────────┘   └─────────┘    └─────────┘
//!    DEFAULT
//! ```
//!
//! Navigation: \[Tab\] cycles all 3 views, \[Shift+Tab\] cycles backward.
//! Shortcuts: \[1-3\] jump directly, \[s/c/x\] letter shortcuts.

mod chat;
/// Chat-specific widgets, re-exported for public API access
pub use chat::widgets as chat_widgets;
pub mod command;
mod control;
mod monitor;
mod settings;
mod studio;
mod view_trait;
mod wizard;
// Main view exports
pub use chat::ChatView;
pub use settings::SettingsView;

// Internal re-exports (original struct names used internally)
// StudioView is now the 3-panel view (Browser + Editor + DAG)
pub use command::CommandView;
pub use control::ControlView;
pub use monitor::MonitorView;
pub use studio::YamlEditorPanel;
// StudioView = 3-panel layout, enums are internal
pub use studio::StudioView;

// Trait export
pub use view_trait::View;

// Wizard view
pub use wizard::WizardView;

// ═══════════════════════════════════════════════════════════════════════════════
// Panel Tab Enums
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
    /// Claude Code-like step-by-step view
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

/// Active view in the TUI - 3 views navigation
///
/// Consolidates to 3 views:
/// - Studio (1, default) \[s\] - Unified editor with browser + YAML editor + DAG preview
/// - Command (2) \[c\] - Fusion of Runner + Chat (execution monitoring + conversation)
/// - Control (3) \[x\] - Provider config, theme, preferences
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TuiView {
    /// Studio - Unified editor with browser + YAML editor + DAG preview [1/s]
    #[default]
    Studio,
    /// Command - Fusion of Runner + Chat (execution + conversation) [2/c]
    Command,
    /// Control - Provider config, theme, preferences [3/x]
    Control,
}

impl TuiView {
    /// Get all 3 views in order (Tab cycles through all)
    pub fn all() -> &'static [TuiView] {
        &[TuiView::Studio, TuiView::Command, TuiView::Control]
    }

    /// Check if this is the control view (non-workflow view)
    pub fn is_auxiliary(&self) -> bool {
        matches!(self, TuiView::Control)
    }

    /// Get all views including auxiliary
    pub fn all_including_auxiliary() -> &'static [TuiView] {
        Self::all()
    }

    /// Check if this is the studio view
    pub fn is_studio(&self) -> bool {
        matches!(self, TuiView::Studio)
    }

    /// Get next view (cycling through all 3 views)
    pub fn next(&self) -> Self {
        match self {
            TuiView::Studio => TuiView::Command,
            TuiView::Command => TuiView::Control,
            TuiView::Control => TuiView::Studio,
        }
    }

    /// Get previous view (cycling through all 3 views)
    pub fn prev(&self) -> Self {
        match self {
            TuiView::Studio => TuiView::Control,
            TuiView::Command => TuiView::Studio,
            TuiView::Control => TuiView::Command,
        }
    }

    /// Get view number (1-indexed for display)
    pub fn number(&self) -> u8 {
        match self {
            TuiView::Studio => 1,
            TuiView::Command => 2,
            TuiView::Control => 3,
        }
    }

    /// Get the title for the header bar
    pub fn title(&self) -> &'static str {
        match self {
            TuiView::Studio => "NIKA STUDIO",
            TuiView::Command => "NIKA COMMAND",
            TuiView::Control => "NIKA CONTROL",
        }
    }

    /// Get the icon for the view (terminal-friendly)
    pub fn icon(&self) -> &'static str {
        match self {
            TuiView::Studio => "📝",
            TuiView::Command => "▶",
            TuiView::Control => "⚙",
        }
    }

    /// Get the letter shortcut for the view
    pub fn shortcut(&self) -> char {
        match self {
            TuiView::Studio => 's',
            TuiView::Command => 'c',
            TuiView::Control => 'x',
        }
    }

    /// Toggle to the next view
    pub fn toggle(&self) -> Self {
        self.next()
    }
}

/// Model provider for LLM switching
pub use crate::command::{McpAction, ModelProvider};
/// Theme variant for direct theme selection
pub use crate::tokens::CosmicVariant;

/// Result of handling a key event in a view
#[derive(Debug, Clone, PartialEq)]
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
    /// Show an error message
    Error(String),
    /// Show a status message (success, info, warning, error) with auto-dismiss
    StatusMessage(crate::widgets::StatusMessage),
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
    /// Execute /mcp command - MCP server management
    ChatMcp(McpAction),
    /// Execute /clear command - clear chat history
    ChatClear,
    /// Open Control view
    OpenControl,
    /// Toggle theme
    ToggleTheme,
    /// Set specific theme by variant
    SetTheme(CosmicVariant),
    /// Verify all configured providers
    VerifyProviders,
    /// Refresh verification (invalidate cache + re-verify)
    RefreshVerification,
    /// Provider selector confirmed a change
    /// Signals app.rs to invalidate/recreate chat_agent with new provider
    ProviderSelectorConfirm { provider_id: String, model: String },
    /// Pull native model
    PullNativeModel(String),
    /// Delete native model
    DeleteNativeModel(String),
    /// Refresh native model list
    RefreshNativeModels,
    /// Validate workflow in Home view
    ValidateWorkflow(std::path::PathBuf),
    /// Launch setup wizard
    LaunchWizard,
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
    fn test_tui_view_all_three_views() {
        let views = TuiView::all();
        assert_eq!(views.len(), 3);
        assert_eq!(views[0], TuiView::Studio);
        assert_eq!(views[1], TuiView::Command);
        assert_eq!(views[2], TuiView::Control);
    }

    #[test]
    fn test_tui_view_is_auxiliary() {
        assert!(!TuiView::Studio.is_auxiliary());
        assert!(!TuiView::Command.is_auxiliary());
        assert!(TuiView::Control.is_auxiliary());
    }

    #[test]
    fn test_tui_view_is_studio() {
        assert!(TuiView::Studio.is_studio());
        assert!(!TuiView::Command.is_studio());
        assert!(!TuiView::Control.is_studio());
    }

    #[test]
    fn test_tui_view_next_cycles_all_three() {
        assert_eq!(TuiView::Studio.next(), TuiView::Command);
        assert_eq!(TuiView::Command.next(), TuiView::Control);
        assert_eq!(TuiView::Control.next(), TuiView::Studio);
    }

    #[test]
    fn test_tui_view_prev_cycles_all_three() {
        assert_eq!(TuiView::Studio.prev(), TuiView::Control);
        assert_eq!(TuiView::Command.prev(), TuiView::Studio);
        assert_eq!(TuiView::Control.prev(), TuiView::Command);
    }

    #[test]
    fn test_tui_view_number_all_three() {
        assert_eq!(TuiView::Studio.number(), 1);
        assert_eq!(TuiView::Command.number(), 2);
        assert_eq!(TuiView::Control.number(), 3);
    }

    #[test]
    fn test_tui_view_titles_all_three() {
        assert_eq!(TuiView::Studio.title(), "NIKA STUDIO");
        assert_eq!(TuiView::Command.title(), "NIKA COMMAND");
        assert_eq!(TuiView::Control.title(), "NIKA CONTROL");
    }

    #[test]
    fn test_tui_view_icons_all_three() {
        assert_eq!(TuiView::Studio.icon(), "📝");
        assert_eq!(TuiView::Command.icon(), "▶");
        assert_eq!(TuiView::Control.icon(), "⚙");
    }

    #[test]
    fn test_tui_view_shortcuts_all_three() {
        assert_eq!(TuiView::Studio.shortcut(), 's');
        assert_eq!(TuiView::Command.shortcut(), 'c');
        assert_eq!(TuiView::Control.shortcut(), 'x');
    }

    #[test]
    fn test_view_action_switch_to_all_three_views() {
        let actions = [
            ViewAction::SwitchView(TuiView::Studio),
            ViewAction::SwitchView(TuiView::Command),
            ViewAction::SwitchView(TuiView::Control),
        ];
        assert_eq!(actions.len(), 3);
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
    // Tab enum tests
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
    // ViewAction::SetTheme tests
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
