// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native Setup Wizard State
//!
//! TUI wizard for guided setup of Nika, providers, models, MCP servers, and editors.
//!
//! # M4 Native Setup Wizard
//!
//! This module provides the state management for a 6-step setup wizard:
//!
//! 1. **Welcome** - Introduction and overview
//! 2. **Providers** - API key configuration for cloud providers
//! 3. **Models** - Local model download (mistral.rs)
//! 4. **McpServers** - MCP server configuration
//! 5. **EditorSync** - Editor integration setup
//! 6. **Verification** - Final validation checks
//!
//! # Example
//!
//! ```rust
//! use nika_tui::wizard::{WizardState, WizardStep};
//!
//! let mut wizard = WizardState::new();
//! assert_eq!(wizard.current_step, WizardStep::Welcome);
//!
//! // Complete welcome step and advance
//! wizard.mark_complete(WizardStep::Welcome);
//! wizard.advance();
//! assert_eq!(wizard.current_step, WizardStep::Providers);
//! ```

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Total number of wizard steps (excluding Complete).
const WIZARD_STEP_COUNT: usize = 6;

/// Wizard step in the setup flow.
///
/// Steps are ordered and must be completed sequentially,
/// though users can go back to review previous steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WizardStep {
    /// Step 0: Introduction and overview.
    #[default]
    Welcome,
    /// Step 1: API key setup for cloud providers.
    Providers,
    /// Step 2: Local model download (mistral.rs).
    Models,
    /// Step 3: MCP server configuration.
    McpServers,
    /// Step 4: Editor sync setup (Claude Code, Cursor, etc.).
    EditorSync,
    /// Step 5: Final verification checks.
    Verification,
    /// Done: Wizard completed successfully.
    Complete,
}

impl WizardStep {
    /// Get the step number (0-5 for active steps, 6 for Complete).
    pub fn number(&self) -> usize {
        match self {
            Self::Welcome => 0,
            Self::Providers => 1,
            Self::Models => 2,
            Self::McpServers => 3,
            Self::EditorSync => 4,
            Self::Verification => 5,
            Self::Complete => 6,
        }
    }

    /// Get the step from a number.
    pub fn from_number(n: usize) -> Option<Self> {
        match n {
            0 => Some(Self::Welcome),
            1 => Some(Self::Providers),
            2 => Some(Self::Models),
            3 => Some(Self::McpServers),
            4 => Some(Self::EditorSync),
            5 => Some(Self::Verification),
            6 => Some(Self::Complete),
            _ => None,
        }
    }

    /// Get the display title for this step.
    pub fn title(&self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::Providers => "Cloud Providers",
            Self::Models => "Local Models",
            Self::McpServers => "MCP Servers",
            Self::EditorSync => "Editor Sync",
            Self::Verification => "Verification",
            Self::Complete => "Complete",
        }
    }

    /// Get a short description of what this step does.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Welcome => "Introduction to Nika and setup overview",
            Self::Providers => "Configure API keys for Claude, OpenAI, Mistral, etc.",
            Self::Models => "Download local models for offline inference",
            Self::McpServers => "Configure MCP servers (Neo4j, GitHub, etc.)",
            Self::EditorSync => "Sync configuration to your editors",
            Self::Verification => "Verify all components are working",
            Self::Complete => "Setup completed successfully",
        }
    }

    /// Get the next step (returns None if at Complete).
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Welcome => Some(Self::Providers),
            Self::Providers => Some(Self::Models),
            Self::Models => Some(Self::McpServers),
            Self::McpServers => Some(Self::EditorSync),
            Self::EditorSync => Some(Self::Verification),
            Self::Verification => Some(Self::Complete),
            Self::Complete => None,
        }
    }

    /// Get the previous step (returns None if at Welcome).
    pub fn prev(&self) -> Option<Self> {
        match self {
            Self::Welcome => None,
            Self::Providers => Some(Self::Welcome),
            Self::Models => Some(Self::Providers),
            Self::McpServers => Some(Self::Models),
            Self::EditorSync => Some(Self::McpServers),
            Self::Verification => Some(Self::EditorSync),
            Self::Complete => Some(Self::Verification),
        }
    }

    /// Check if this is the first step.
    pub fn is_first(&self) -> bool {
        matches!(self, Self::Welcome)
    }

    /// Check if this is the last active step (before Complete).
    pub fn is_last(&self) -> bool {
        matches!(self, Self::Verification)
    }

    /// Check if the wizard is complete.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }

    /// All active steps (excludes Complete).
    pub fn all_active() -> &'static [WizardStep] {
        &[
            Self::Welcome,
            Self::Providers,
            Self::Models,
            Self::McpServers,
            Self::EditorSync,
            Self::Verification,
        ]
    }
}

impl std::fmt::Display for WizardStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.title())
    }
}

/// Wizard configuration for persistence.
///
/// Saved to `~/.nika/wizard.json` after completion.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WizardConfig {
    /// Whether the wizard has been completed.
    pub completed: bool,
    /// When the wizard was completed (if completed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    /// Steps that were skipped by the user.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_steps: Vec<WizardStep>,
}

impl WizardConfig {
    /// Create a new config marking wizard as completed.
    pub fn completed() -> Self {
        Self {
            completed: true,
            completed_at: Some(Utc::now()),
            skipped_steps: Vec::new(),
        }
    }

    /// Create a completed config with skipped steps.
    pub fn completed_with_skipped(skipped: Vec<WizardStep>) -> Self {
        Self {
            completed: true,
            completed_at: Some(Utc::now()),
            skipped_steps: skipped,
        }
    }

    /// Check if a step was skipped.
    pub fn was_skipped(&self, step: WizardStep) -> bool {
        self.skipped_steps.contains(&step)
    }
}

/// Main wizard state for TUI rendering and interaction.
///
/// Tracks the current step, completed steps, and gathered configuration data.
#[derive(Debug, Clone)]
pub struct WizardState {
    /// Current active step.
    pub current_step: WizardStep,
    /// Steps that have been completed.
    pub completed_steps: HashSet<WizardStep>,
    /// Provider API key status (provider name -> has_key).
    pub provider_keys: HashMap<String, bool>,
    /// Selected model for native inference (if any).
    pub selected_model: Option<String>,
    /// Editors enabled for sync.
    pub enabled_editors: Vec<String>,
    /// MCP servers to configure.
    pub mcp_servers: Vec<String>,
    /// Error messages collected during setup.
    pub errors: Vec<String>,
    /// Steps skipped by the user.
    skipped_steps: HashSet<WizardStep>,
}

impl Default for WizardState {
    fn default() -> Self {
        Self::new()
    }
}

impl WizardState {
    /// Create a new wizard state starting at Welcome.
    pub fn new() -> Self {
        Self {
            current_step: WizardStep::Welcome,
            completed_steps: HashSet::new(),
            provider_keys: HashMap::new(),
            selected_model: None,
            enabled_editors: Vec::new(),
            mcp_servers: Vec::new(),
            errors: Vec::new(),
            skipped_steps: HashSet::new(),
        }
    }

    /// Advance to the next step if possible.
    ///
    /// Returns `true` if advanced, `false` if already at Complete.
    pub fn advance(&mut self) -> bool {
        if let Some(next) = self.current_step.next() {
            self.current_step = next;
            true
        } else {
            false
        }
    }

    /// Go back to the previous step if possible.
    ///
    /// Returns `true` if moved back, `false` if already at Welcome.
    pub fn go_back(&mut self) -> bool {
        if let Some(prev) = self.current_step.prev() {
            self.current_step = prev;
            true
        } else {
            false
        }
    }

    /// Check if a step has been completed.
    pub fn is_step_complete(&self, step: WizardStep) -> bool {
        self.completed_steps.contains(&step)
    }

    /// Mark a step as complete.
    pub fn mark_complete(&mut self, step: WizardStep) {
        self.completed_steps.insert(step);
    }

    /// Mark a step as incomplete (for re-doing).
    pub fn mark_incomplete(&mut self, step: WizardStep) {
        self.completed_steps.remove(&step);
    }

    /// Skip the current step and advance.
    ///
    /// Skipped steps are tracked but not marked complete.
    pub fn skip_current(&mut self) -> bool {
        self.skipped_steps.insert(self.current_step);
        self.advance()
    }

    /// Check if a step was skipped.
    pub fn is_step_skipped(&self, step: WizardStep) -> bool {
        self.skipped_steps.contains(&step)
    }

    /// Check if the current step allows advancing.
    ///
    /// Rules:
    /// - Welcome: Always can advance
    /// - Providers: At least one API key configured
    /// - Models: Optional (can skip or select model)
    /// - McpServers: Optional (can skip)
    /// - EditorSync: Optional (can skip)
    /// - Verification: All required steps complete
    /// - Complete: Cannot advance further
    pub fn can_advance(&self) -> bool {
        match self.current_step {
            WizardStep::Welcome => true,
            WizardStep::Providers => {
                // At least one provider key configured
                self.provider_keys.values().any(|&has_key| has_key)
            }
            WizardStep::Models => {
                // Optional - can advance without selecting a model
                true
            }
            WizardStep::McpServers => {
                // Optional - can advance without MCP servers
                true
            }
            WizardStep::EditorSync => {
                // Optional - can advance without enabling editors
                true
            }
            WizardStep::Verification => {
                // All prior non-skipped steps should be complete
                self.is_step_complete(WizardStep::Welcome)
                    && (self.is_step_complete(WizardStep::Providers)
                        || self.is_step_skipped(WizardStep::Providers))
            }
            WizardStep::Complete => false,
        }
    }

    /// Calculate progress percentage (0-100).
    ///
    /// Based on completed steps out of total active steps.
    pub fn progress_percentage(&self) -> u8 {
        if self.current_step.is_complete() {
            return 100;
        }

        let completed = self.completed_steps.len();
        let total = WIZARD_STEP_COUNT;

        ((completed as f64 / total as f64) * 100.0).round() as u8
    }

    /// Get the number of completed steps.
    pub fn completed_count(&self) -> usize {
        self.completed_steps.len()
    }

    /// Get the total number of active steps.
    pub fn total_steps(&self) -> usize {
        WIZARD_STEP_COUNT
    }

    /// Check if the wizard is fully complete.
    pub fn is_wizard_complete(&self) -> bool {
        self.current_step.is_complete()
    }

    /// Set provider key status.
    pub fn set_provider_key(&mut self, provider: impl Into<String>, has_key: bool) {
        self.provider_keys.insert(provider.into(), has_key);
    }

    /// Check if a provider has a configured key.
    pub fn has_provider_key(&self, provider: &str) -> bool {
        self.provider_keys.get(provider).copied().unwrap_or(false)
    }

    /// Get count of configured providers.
    pub fn configured_provider_count(&self) -> usize {
        self.provider_keys.values().filter(|&&v| v).count()
    }

    /// Set the selected model for native inference.
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.selected_model = Some(model.into());
    }

    /// Clear the selected model.
    pub fn clear_model(&mut self) {
        self.selected_model = None;
    }

    /// Add an MCP server to configure.
    pub fn add_mcp_server(&mut self, server: impl Into<String>) {
        let server = server.into();
        if !self.mcp_servers.contains(&server) {
            self.mcp_servers.push(server);
        }
    }

    /// Remove an MCP server.
    pub fn remove_mcp_server(&mut self, server: &str) {
        self.mcp_servers.retain(|s| s != server);
    }

    /// Enable an editor for sync.
    pub fn enable_editor(&mut self, editor: &str) {
        if !self.enabled_editors.iter().any(|e| e == editor) {
            self.enabled_editors.push(editor.to_string());
        }
    }

    /// Disable an editor for sync.
    pub fn disable_editor(&mut self, editor: &str) {
        self.enabled_editors.retain(|e| e != editor);
    }

    /// Check if an editor is enabled.
    pub fn is_editor_enabled(&self, editor: &str) -> bool {
        self.enabled_editors.iter().any(|e| e == editor)
    }

    /// Add an error message.
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
    }

    /// Clear all errors.
    pub fn clear_errors(&mut self) {
        self.errors.clear();
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Generate wizard config for persistence.
    pub fn to_config(&self) -> WizardConfig {
        if self.is_wizard_complete() {
            WizardConfig::completed_with_skipped(self.skipped_steps.iter().copied().collect())
        } else {
            WizardConfig::default()
        }
    }

    /// Reset the wizard to initial state.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // WizardStep Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_step_default_is_welcome() {
        assert_eq!(WizardStep::default(), WizardStep::Welcome);
    }

    #[test]
    fn test_step_numbers() {
        assert_eq!(WizardStep::Welcome.number(), 0);
        assert_eq!(WizardStep::Providers.number(), 1);
        assert_eq!(WizardStep::Models.number(), 2);
        assert_eq!(WizardStep::McpServers.number(), 3);
        assert_eq!(WizardStep::EditorSync.number(), 4);
        assert_eq!(WizardStep::Verification.number(), 5);
        assert_eq!(WizardStep::Complete.number(), 6);
    }

    #[test]
    fn test_step_from_number() {
        assert_eq!(WizardStep::from_number(0), Some(WizardStep::Welcome));
        assert_eq!(WizardStep::from_number(1), Some(WizardStep::Providers));
        assert_eq!(WizardStep::from_number(6), Some(WizardStep::Complete));
        assert_eq!(WizardStep::from_number(7), None);
    }

    #[test]
    fn test_step_next_chain() {
        let mut step = WizardStep::Welcome;

        step = step.next().unwrap();
        assert_eq!(step, WizardStep::Providers);

        step = step.next().unwrap();
        assert_eq!(step, WizardStep::Models);

        step = step.next().unwrap();
        assert_eq!(step, WizardStep::McpServers);

        step = step.next().unwrap();
        assert_eq!(step, WizardStep::EditorSync);

        step = step.next().unwrap();
        assert_eq!(step, WizardStep::Verification);

        step = step.next().unwrap();
        assert_eq!(step, WizardStep::Complete);

        assert!(step.next().is_none());
    }

    #[test]
    fn test_step_prev_chain() {
        let mut step = WizardStep::Complete;

        step = step.prev().unwrap();
        assert_eq!(step, WizardStep::Verification);

        step = step.prev().unwrap();
        assert_eq!(step, WizardStep::EditorSync);

        step = step.prev().unwrap();
        assert_eq!(step, WizardStep::McpServers);

        step = step.prev().unwrap();
        assert_eq!(step, WizardStep::Models);

        step = step.prev().unwrap();
        assert_eq!(step, WizardStep::Providers);

        step = step.prev().unwrap();
        assert_eq!(step, WizardStep::Welcome);

        assert!(step.prev().is_none());
    }

    #[test]
    fn test_step_is_first_last_complete() {
        assert!(WizardStep::Welcome.is_first());
        assert!(!WizardStep::Providers.is_first());

        assert!(WizardStep::Verification.is_last());
        assert!(!WizardStep::EditorSync.is_last());

        assert!(WizardStep::Complete.is_complete());
        assert!(!WizardStep::Verification.is_complete());
    }

    #[test]
    fn test_step_all_active() {
        let active = WizardStep::all_active();
        assert_eq!(active.len(), 6);
        assert!(!active.contains(&WizardStep::Complete));
    }

    #[test]
    fn test_step_titles() {
        assert_eq!(WizardStep::Welcome.title(), "Welcome");
        assert_eq!(WizardStep::Providers.title(), "Cloud Providers");
        assert_eq!(WizardStep::Models.title(), "Local Models");
        assert_eq!(WizardStep::McpServers.title(), "MCP Servers");
        assert_eq!(WizardStep::EditorSync.title(), "Editor Sync");
        assert_eq!(WizardStep::Verification.title(), "Verification");
        assert_eq!(WizardStep::Complete.title(), "Complete");
    }

    #[test]
    fn test_step_display() {
        assert_eq!(format!("{}", WizardStep::Welcome), "Welcome");
        assert_eq!(format!("{}", WizardStep::Complete), "Complete");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // WizardConfig Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_config_default() {
        let config = WizardConfig::default();
        assert!(!config.completed);
        assert!(config.completed_at.is_none());
        assert!(config.skipped_steps.is_empty());
    }

    #[test]
    fn test_config_completed() {
        let config = WizardConfig::completed();
        assert!(config.completed);
        assert!(config.completed_at.is_some());
        assert!(config.skipped_steps.is_empty());
    }

    #[test]
    fn test_config_completed_with_skipped() {
        let config = WizardConfig::completed_with_skipped(vec![WizardStep::Models]);
        assert!(config.completed);
        assert!(config.was_skipped(WizardStep::Models));
        assert!(!config.was_skipped(WizardStep::Providers));
    }

    #[test]
    fn test_config_serialization() {
        let config = WizardConfig::completed_with_skipped(vec![WizardStep::Models]);
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"completed\":true"));
        assert!(json.contains("\"models\""));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // WizardState Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_state_new() {
        let state = WizardState::new();
        assert_eq!(state.current_step, WizardStep::Welcome);
        assert!(state.completed_steps.is_empty());
        assert!(state.provider_keys.is_empty());
        assert!(state.selected_model.is_none());
        assert!(state.enabled_editors.is_empty());
        assert!(state.mcp_servers.is_empty());
        assert!(state.errors.is_empty());
    }

    #[test]
    fn test_state_default() {
        let state = WizardState::default();
        assert_eq!(state.current_step, WizardStep::Welcome);
    }

    #[test]
    fn test_state_advance() {
        let mut state = WizardState::new();

        assert!(state.advance());
        assert_eq!(state.current_step, WizardStep::Providers);

        assert!(state.advance());
        assert_eq!(state.current_step, WizardStep::Models);

        // Advance to Complete
        state.advance();
        state.advance();
        state.advance();
        state.advance();
        assert_eq!(state.current_step, WizardStep::Complete);

        // Cannot advance past Complete
        assert!(!state.advance());
        assert_eq!(state.current_step, WizardStep::Complete);
    }

    #[test]
    fn test_state_go_back() {
        let mut state = WizardState::new();
        state.current_step = WizardStep::Models;

        assert!(state.go_back());
        assert_eq!(state.current_step, WizardStep::Providers);

        assert!(state.go_back());
        assert_eq!(state.current_step, WizardStep::Welcome);

        // Cannot go back past Welcome
        assert!(!state.go_back());
        assert_eq!(state.current_step, WizardStep::Welcome);
    }

    #[test]
    fn test_state_mark_complete() {
        let mut state = WizardState::new();

        assert!(!state.is_step_complete(WizardStep::Welcome));

        state.mark_complete(WizardStep::Welcome);
        assert!(state.is_step_complete(WizardStep::Welcome));
        assert!(!state.is_step_complete(WizardStep::Providers));

        state.mark_incomplete(WizardStep::Welcome);
        assert!(!state.is_step_complete(WizardStep::Welcome));
    }

    #[test]
    fn test_state_skip_current() {
        let mut state = WizardState::new();
        state.current_step = WizardStep::Models;

        assert!(state.skip_current());
        assert_eq!(state.current_step, WizardStep::McpServers);
        assert!(state.is_step_skipped(WizardStep::Models));
        assert!(!state.is_step_complete(WizardStep::Models));
    }

    #[test]
    fn test_state_can_advance_welcome() {
        let state = WizardState::new();
        assert!(state.can_advance());
    }

    #[test]
    fn test_state_can_advance_providers() {
        let mut state = WizardState::new();
        state.current_step = WizardStep::Providers;

        // No keys configured - cannot advance
        assert!(!state.can_advance());

        // Configure a key - can advance
        state.set_provider_key("anthropic", true);
        assert!(state.can_advance());
    }

    #[test]
    fn test_state_can_advance_optional_steps() {
        let mut state = WizardState::new();

        // Models is optional
        state.current_step = WizardStep::Models;
        assert!(state.can_advance());

        // McpServers is optional
        state.current_step = WizardStep::McpServers;
        assert!(state.can_advance());

        // EditorSync is optional
        state.current_step = WizardStep::EditorSync;
        assert!(state.can_advance());
    }

    #[test]
    fn test_state_can_advance_verification() {
        let mut state = WizardState::new();
        state.current_step = WizardStep::Verification;

        // Welcome not complete - cannot advance
        assert!(!state.can_advance());

        state.mark_complete(WizardStep::Welcome);
        // Providers not complete and not skipped - cannot advance
        assert!(!state.can_advance());

        // Skip providers - can now advance
        state.skipped_steps.insert(WizardStep::Providers);
        assert!(state.can_advance());
    }

    #[test]
    fn test_state_can_advance_complete() {
        let mut state = WizardState::new();
        state.current_step = WizardStep::Complete;
        assert!(!state.can_advance());
    }

    #[test]
    fn test_state_progress_percentage() {
        let mut state = WizardState::new();

        // No steps complete
        assert_eq!(state.progress_percentage(), 0);

        // 1 of 6 complete
        state.mark_complete(WizardStep::Welcome);
        assert_eq!(state.progress_percentage(), 17);

        // 3 of 6 complete
        state.mark_complete(WizardStep::Providers);
        state.mark_complete(WizardStep::Models);
        assert_eq!(state.progress_percentage(), 50);

        // All complete
        state.mark_complete(WizardStep::McpServers);
        state.mark_complete(WizardStep::EditorSync);
        state.mark_complete(WizardStep::Verification);
        assert_eq!(state.progress_percentage(), 100);
    }

    #[test]
    fn test_state_progress_at_complete() {
        let mut state = WizardState::new();
        state.current_step = WizardStep::Complete;
        assert_eq!(state.progress_percentage(), 100);
    }

    #[test]
    fn test_state_completed_count() {
        let mut state = WizardState::new();

        assert_eq!(state.completed_count(), 0);

        state.mark_complete(WizardStep::Welcome);
        state.mark_complete(WizardStep::Providers);
        assert_eq!(state.completed_count(), 2);
    }

    #[test]
    fn test_state_total_steps() {
        let state = WizardState::new();
        assert_eq!(state.total_steps(), 6);
    }

    #[test]
    fn test_state_is_wizard_complete() {
        let mut state = WizardState::new();
        assert!(!state.is_wizard_complete());

        state.current_step = WizardStep::Complete;
        assert!(state.is_wizard_complete());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Provider Key Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_state_provider_keys() {
        let mut state = WizardState::new();

        assert!(!state.has_provider_key("anthropic"));
        assert_eq!(state.configured_provider_count(), 0);

        state.set_provider_key("anthropic", true);
        assert!(state.has_provider_key("anthropic"));
        assert_eq!(state.configured_provider_count(), 1);

        state.set_provider_key("openai", true);
        state.set_provider_key("mistral", false);
        assert_eq!(state.configured_provider_count(), 2);

        // Update existing
        state.set_provider_key("anthropic", false);
        assert!(!state.has_provider_key("anthropic"));
        assert_eq!(state.configured_provider_count(), 1);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Model Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_state_model() {
        let mut state = WizardState::new();

        assert!(state.selected_model.is_none());

        state.set_model("llama3.2:1b");
        assert_eq!(state.selected_model, Some("llama3.2:1b".to_string()));

        state.clear_model();
        assert!(state.selected_model.is_none());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MCP Server Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_state_mcp_servers() {
        let mut state = WizardState::new();

        assert!(state.mcp_servers.is_empty());

        state.add_mcp_server("neo4j");
        state.add_mcp_server("github");
        assert_eq!(state.mcp_servers.len(), 2);

        // Duplicate not added
        state.add_mcp_server("neo4j");
        assert_eq!(state.mcp_servers.len(), 2);

        state.remove_mcp_server("neo4j");
        assert_eq!(state.mcp_servers, vec!["github"]);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Editor Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_state_editors() {
        let mut state = WizardState::new();

        assert!(state.enabled_editors.is_empty());
        assert!(!state.is_editor_enabled("claude-code"));

        state.enable_editor("claude-code");
        assert!(state.is_editor_enabled("claude-code"));
        assert!(!state.is_editor_enabled("cursor"));

        // Duplicate not added
        state.enable_editor("claude-code");
        assert_eq!(state.enabled_editors.len(), 1);

        state.enable_editor("cursor");
        assert_eq!(state.enabled_editors.len(), 2);

        state.disable_editor("claude-code");
        assert!(!state.is_editor_enabled("claude-code"));
        assert_eq!(state.enabled_editors.len(), 1);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Error Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_state_errors() {
        let mut state = WizardState::new();

        assert!(!state.has_errors());

        state.add_error("Connection failed");
        state.add_error("Invalid key format");
        assert!(state.has_errors());
        assert_eq!(state.errors.len(), 2);

        state.clear_errors();
        assert!(!state.has_errors());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Config Generation Tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_state_to_config_incomplete() {
        let state = WizardState::new();
        let config = state.to_config();

        assert!(!config.completed);
    }

    #[test]
    fn test_state_to_config_complete() {
        let mut state = WizardState::new();
        state.current_step = WizardStep::Complete;
        state.skipped_steps.insert(WizardStep::Models);

        let config = state.to_config();

        assert!(config.completed);
        assert!(config.was_skipped(WizardStep::Models));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Reset Test
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_state_reset() {
        let mut state = WizardState::new();
        state.current_step = WizardStep::Models;
        state.mark_complete(WizardStep::Welcome);
        state.set_provider_key("anthropic", true);
        state.set_model("llama3.2");
        state.add_error("test error");

        state.reset();

        assert_eq!(state.current_step, WizardStep::Welcome);
        assert!(state.completed_steps.is_empty());
        assert!(state.provider_keys.is_empty());
        assert!(state.selected_model.is_none());
        assert!(state.errors.is_empty());
    }
}
