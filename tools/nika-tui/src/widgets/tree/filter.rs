// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tree filtering system for file type selection.
//!
//! Supports filtering by:
//! - File type (workflows, agents, skills)
//! - Hidden files toggle
//! - Fuzzy search (via external matcher)

use super::NodeKind;

/// Filter mode for tree display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreeFilter {
    /// Show all files (except hidden unless toggled)
    #[default]
    All,
    /// Show only .nika.yaml workflows
    WorkflowsOnly,
    /// Show only .son agents
    AgentsOnly,
    /// Show only .skill.md files
    SkillsOnly,
    /// Show only ecosystem files (all SuperNovae types)
    EcosystemOnly,
}

impl TreeFilter {
    /// Check if a node kind passes this filter
    ///
    /// Directories always pass to maintain tree structure.
    pub fn matches(&self, kind: &NodeKind) -> bool {
        // Directories always pass (needed for tree structure)
        if kind.is_directory() {
            return true;
        }

        match self {
            TreeFilter::All => true,
            TreeFilter::WorkflowsOnly => matches!(kind, NodeKind::NikaWorkflow),
            TreeFilter::AgentsOnly => matches!(kind, NodeKind::SonAgent),
            TreeFilter::SkillsOnly => matches!(kind, NodeKind::SkillFile),
            TreeFilter::EcosystemOnly => kind.is_ecosystem(),
        }
    }

    /// Cycle to next filter mode
    pub fn next(&self) -> Self {
        match self {
            TreeFilter::All => TreeFilter::WorkflowsOnly,
            TreeFilter::WorkflowsOnly => TreeFilter::AgentsOnly,
            TreeFilter::AgentsOnly => TreeFilter::SkillsOnly,
            TreeFilter::SkillsOnly => TreeFilter::EcosystemOnly,
            TreeFilter::EcosystemOnly => TreeFilter::All,
        }
    }

    /// Cycle to previous filter mode
    pub fn prev(&self) -> Self {
        match self {
            TreeFilter::All => TreeFilter::EcosystemOnly,
            TreeFilter::WorkflowsOnly => TreeFilter::All,
            TreeFilter::AgentsOnly => TreeFilter::WorkflowsOnly,
            TreeFilter::SkillsOnly => TreeFilter::AgentsOnly,
            TreeFilter::EcosystemOnly => TreeFilter::SkillsOnly,
        }
    }

    /// Display name for status line
    pub fn display_name(&self) -> &'static str {
        match self {
            TreeFilter::All => "All",
            TreeFilter::WorkflowsOnly => "Workflows",
            TreeFilter::AgentsOnly => "Agents",
            TreeFilter::SkillsOnly => "Skills",
            TreeFilter::EcosystemOnly => "Ecosystem",
        }
    }

    /// Short label for UI badge
    pub fn badge(&self) -> &'static str {
        match self {
            TreeFilter::All => "",
            TreeFilter::WorkflowsOnly => "W",
            TreeFilter::AgentsOnly => "A",
            TreeFilter::SkillsOnly => "S",
            TreeFilter::EcosystemOnly => "E",
        }
    }

    /// Icon for filter mode
    pub fn icon(&self) -> &'static str {
        match self {
            TreeFilter::All => "",
            TreeFilter::WorkflowsOnly => "W",
            TreeFilter::AgentsOnly => "A",
            TreeFilter::SkillsOnly => "S",
            TreeFilter::EcosystemOnly => "E",
        }
    }
}

/// Get filter from keyboard shortcut
///
/// Supported shortcuts:
/// - `W` → Workflows only
/// - `A` → Agents only
/// - `S` → Skills only (caution: may conflict with other shortcuts)
/// - `E` → Ecosystem only
/// - `0` → All (reset filter)
pub fn filter_from_key(key: char) -> Option<TreeFilter> {
    match key.to_ascii_uppercase() {
        'W' => Some(TreeFilter::WorkflowsOnly),
        'A' => Some(TreeFilter::AgentsOnly),
        // 'S' reserved for Settings in some views
        'E' => Some(TreeFilter::EcosystemOnly),
        '0' => Some(TreeFilter::All),
        _ => None,
    }
}

/// Filter configuration
#[derive(Debug, Clone, Copy)]
pub struct FilterConfig {
    /// Current filter mode
    pub filter: TreeFilter,
    /// Show hidden files
    pub show_hidden: bool,
    /// Show directories (always true in tree mode)
    pub show_directories: bool,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterConfig {
    /// Create a new filter config with defaults
    pub fn new() -> Self {
        Self {
            filter: TreeFilter::All,
            show_hidden: false,
            show_directories: true,
        }
    }

    /// Check if a node should be visible
    pub fn is_visible(&self, kind: &NodeKind, is_hidden: bool) -> bool {
        // Hidden check
        if is_hidden && !self.show_hidden {
            return false;
        }

        // Directory check
        if kind.is_directory() && !self.show_directories {
            return false;
        }

        // Filter check
        self.filter.matches(kind)
    }

    /// Toggle hidden files
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
    }

    /// Set filter mode
    pub fn set_filter(&mut self, filter: TreeFilter) {
        self.filter = filter;
    }

    /// Cycle to next filter
    pub fn next_filter(&mut self) {
        self.filter = self.filter.next();
    }

    /// Reset to defaults
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_all_matches_everything() {
        let filter = TreeFilter::All;
        assert!(filter.matches(&NodeKind::NikaWorkflow));
        assert!(filter.matches(&NodeKind::SonAgent));
        assert!(filter.matches(&NodeKind::Rust));
        assert!(filter.matches(&NodeKind::File));
        assert!(filter.matches(&NodeKind::Directory));
    }

    #[test]
    fn test_filter_workflows_only() {
        let filter = TreeFilter::WorkflowsOnly;
        assert!(filter.matches(&NodeKind::NikaWorkflow));
        assert!(!filter.matches(&NodeKind::SonAgent));
        assert!(!filter.matches(&NodeKind::Rust));
        // Directories always pass
        assert!(filter.matches(&NodeKind::Directory));
        assert!(filter.matches(&NodeKind::WorkflowsFolder));
    }

    #[test]
    fn test_filter_agents_only() {
        let filter = TreeFilter::AgentsOnly;
        assert!(filter.matches(&NodeKind::SonAgent));
        assert!(!filter.matches(&NodeKind::NikaWorkflow));
        assert!(!filter.matches(&NodeKind::SkillFile));
        // Directories always pass
        assert!(filter.matches(&NodeKind::Directory));
    }

    #[test]
    fn test_filter_skills_only() {
        let filter = TreeFilter::SkillsOnly;
        assert!(filter.matches(&NodeKind::SkillFile));
        assert!(!filter.matches(&NodeKind::NikaWorkflow));
        assert!(!filter.matches(&NodeKind::SonAgent));
        // Directories always pass
        assert!(filter.matches(&NodeKind::Directory));
    }

    #[test]
    fn test_filter_ecosystem_only() {
        let filter = TreeFilter::EcosystemOnly;
        assert!(filter.matches(&NodeKind::NikaWorkflow));
        assert!(filter.matches(&NodeKind::SonAgent));
        assert!(filter.matches(&NodeKind::SkillFile));
        assert!(filter.matches(&NodeKind::NikaFolder));
        assert!(filter.matches(&NodeKind::NovanetFolder));
        assert!(!filter.matches(&NodeKind::Rust));
        assert!(!filter.matches(&NodeKind::File));
    }

    #[test]
    fn test_filter_next_cycle() {
        let mut filter = TreeFilter::All;
        filter = filter.next();
        assert_eq!(filter, TreeFilter::WorkflowsOnly);
        filter = filter.next();
        assert_eq!(filter, TreeFilter::AgentsOnly);
        filter = filter.next();
        assert_eq!(filter, TreeFilter::SkillsOnly);
        filter = filter.next();
        assert_eq!(filter, TreeFilter::EcosystemOnly);
        filter = filter.next();
        assert_eq!(filter, TreeFilter::All);
    }

    #[test]
    fn test_filter_prev_cycle() {
        let mut filter = TreeFilter::All;
        filter = filter.prev();
        assert_eq!(filter, TreeFilter::EcosystemOnly);
        filter = filter.prev();
        assert_eq!(filter, TreeFilter::SkillsOnly);
    }

    #[test]
    fn test_filter_display_name() {
        assert_eq!(TreeFilter::All.display_name(), "All");
        assert_eq!(TreeFilter::WorkflowsOnly.display_name(), "Workflows");
        assert_eq!(TreeFilter::AgentsOnly.display_name(), "Agents");
        assert_eq!(TreeFilter::SkillsOnly.display_name(), "Skills");
        assert_eq!(TreeFilter::EcosystemOnly.display_name(), "Ecosystem");
    }

    #[test]
    fn test_filter_from_key() {
        assert_eq!(filter_from_key('W'), Some(TreeFilter::WorkflowsOnly));
        assert_eq!(filter_from_key('w'), Some(TreeFilter::WorkflowsOnly));
        assert_eq!(filter_from_key('A'), Some(TreeFilter::AgentsOnly));
        assert_eq!(filter_from_key('E'), Some(TreeFilter::EcosystemOnly));
        assert_eq!(filter_from_key('0'), Some(TreeFilter::All));
        assert_eq!(filter_from_key('x'), None);
    }

    #[test]
    fn test_filter_config_new() {
        let config = FilterConfig::new();
        assert_eq!(config.filter, TreeFilter::All);
        assert!(!config.show_hidden);
        assert!(config.show_directories);
    }

    #[test]
    fn test_filter_config_is_visible() {
        let config = FilterConfig::new();

        // Normal file passes
        assert!(config.is_visible(&NodeKind::NikaWorkflow, false));

        // Hidden file fails
        assert!(!config.is_visible(&NodeKind::NikaWorkflow, true));

        // With show_hidden enabled
        let mut config = FilterConfig::new();
        config.show_hidden = true;
        assert!(config.is_visible(&NodeKind::NikaWorkflow, true));
    }

    #[test]
    fn test_filter_config_with_filter() {
        let mut config = FilterConfig::new();
        config.filter = TreeFilter::WorkflowsOnly;

        assert!(config.is_visible(&NodeKind::NikaWorkflow, false));
        assert!(!config.is_visible(&NodeKind::Rust, false));
        // Directory still passes
        assert!(config.is_visible(&NodeKind::Directory, false));
    }

    #[test]
    fn test_filter_config_toggle_hidden() {
        let mut config = FilterConfig::new();
        assert!(!config.show_hidden);
        config.toggle_hidden();
        assert!(config.show_hidden);
        config.toggle_hidden();
        assert!(!config.show_hidden);
    }

    #[test]
    fn test_filter_config_next_filter() {
        let mut config = FilterConfig::new();
        assert_eq!(config.filter, TreeFilter::All);
        config.next_filter();
        assert_eq!(config.filter, TreeFilter::WorkflowsOnly);
    }

    #[test]
    fn test_filter_config_reset() {
        let mut config = FilterConfig::new();
        config.filter = TreeFilter::AgentsOnly;
        config.show_hidden = true;
        config.reset();
        assert_eq!(config.filter, TreeFilter::All);
        assert!(!config.show_hidden);
    }
}
