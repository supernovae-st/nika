// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Type definitions for the Studio view.
//!
//! Contains enums, structs, and their impls that define the editor mode,
//! panel focus, layout ratios, validation results, and LSP state types.

use ratatui::layout::Constraint;

/// Editor mode (vim-like)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorMode {
    #[default]
    Normal,
    Insert,
}

// ═══════════════════════════════════════════════════════════════════════════════
// LSP Completion + Hover State (Phase 4 — in-process LSP)
// ═══════════════════════════════════════════════════════════════════════════════

/// State for the inline completion popup (triggered by typing or Ctrl+Space).
#[derive(Default)]
pub struct CompletionState {
    /// Whether the popup is currently visible.
    pub visible: bool,
    /// Filtered completion items to display.
    pub items: Vec<CompletionEntry>,
    /// Currently selected index in the popup.
    pub selected: usize,
    /// Column where the trigger happened (for replace-on-accept).
    pub trigger_col: usize,
}

/// A single completion entry (mapped from ls_types::CompletionItem).
pub struct CompletionEntry {
    /// Display label (e.g. "infer:", "provider:").
    pub label: String,
    /// Kind hint (e.g. "Keyword", "Property").
    pub kind: String,
    /// Optional detail text shown inline.
    pub detail: Option<String>,
    /// Text to insert when accepted (may differ from label).
    pub insert_text: String,
}

/// State for the hover tooltip (triggered by K in Normal mode).
#[derive(Default)]
pub struct HoverState {
    /// Whether the tooltip is currently visible.
    pub visible: bool,
    /// Markdown content to display.
    pub content: String,
}

/// State for the code action popup (triggered by Ctrl+.).
#[derive(Default)]
pub struct CodeActionState {
    /// Whether the popup is currently visible.
    pub visible: bool,
    /// Available code actions: (title, edit).
    /// Each entry is (display title, Option<TextEdit>).
    pub actions: Vec<CodeActionDisplay>,
    /// Currently selected index in the popup.
    pub selected: usize,
}

/// A single code action entry for display in the popup.
pub struct CodeActionDisplay {
    /// Human-readable title.
    pub title: String,
    /// Text edit to apply (byte offsets + replacement text).
    pub edit: Option<(u32, u32, String)>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// StudioView: 3-Panel Layout
// Browser (20%) | Editor (50%) | DAG Structure (30%)
// ═══════════════════════════════════════════════════════════════════════════════

/// Panel focus in 3-panel Studio layout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StudioFocus {
    /// Left panel: File browser (TreeWidget)
    #[default]
    Browser,
    /// Center panel: YAML editor
    Editor,
    /// Right panel: DAG structure (read-only)
    Dag,
}

impl StudioFocus {
    /// Cycle to next panel (Browser -> Editor -> Dag -> Browser)
    pub fn next(&self) -> Self {
        match self {
            StudioFocus::Browser => StudioFocus::Editor,
            StudioFocus::Editor => StudioFocus::Dag,
            StudioFocus::Dag => StudioFocus::Browser,
        }
    }

    /// Cycle to previous panel (Browser <- Editor <- Dag <- Browser)
    pub fn prev(&self) -> Self {
        match self {
            StudioFocus::Browser => StudioFocus::Dag,
            StudioFocus::Editor => StudioFocus::Browser,
            StudioFocus::Dag => StudioFocus::Editor,
        }
    }

    /// Display name for status bar
    pub fn title(&self) -> &'static str {
        match self {
            StudioFocus::Browser => "Browser",
            StudioFocus::Editor => "Editor",
            StudioFocus::Dag => "DAG",
        }
    }
}

/// Layout ratios for 3-panel Studio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StudioRatio {
    /// Balanced: 20% / 50% / 30%
    #[default]
    Balanced,
    /// Editor focus: 15% / 65% / 20%
    EditorFocus,
    /// Browser focus: 35% / 45% / 20%
    BrowserFocus,
    /// DAG focus: 15% / 35% / 50%
    DagFocus,
}

impl StudioRatio {
    /// Get constraints for Layout::split()
    pub fn constraints(&self) -> [Constraint; 3] {
        match self {
            StudioRatio::Balanced => [
                Constraint::Percentage(20),
                Constraint::Percentage(50),
                Constraint::Percentage(30),
            ],
            StudioRatio::EditorFocus => [
                Constraint::Percentage(15),
                Constraint::Percentage(65),
                Constraint::Percentage(20),
            ],
            StudioRatio::BrowserFocus => [
                Constraint::Percentage(35),
                Constraint::Percentage(45),
                Constraint::Percentage(20),
            ],
            StudioRatio::DagFocus => [
                Constraint::Percentage(15),
                Constraint::Percentage(35),
                Constraint::Percentage(50),
            ],
        }
    }

    /// Cycle to next ratio (Balanced -> EditorFocus -> BrowserFocus -> DagFocus -> Balanced)
    pub fn next(&self) -> Self {
        match self {
            StudioRatio::Balanced => StudioRatio::EditorFocus,
            StudioRatio::EditorFocus => StudioRatio::BrowserFocus,
            StudioRatio::BrowserFocus => StudioRatio::DagFocus,
            StudioRatio::DagFocus => StudioRatio::Balanced,
        }
    }

    /// Display name for status bar
    pub fn title(&self) -> &'static str {
        match self {
            StudioRatio::Balanced => "Balanced",
            StudioRatio::EditorFocus => "Editor+",
            StudioRatio::BrowserFocus => "Browser+",
            StudioRatio::DagFocus => "DAG+",
        }
    }
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub yaml_valid: bool,
    pub schema_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self {
            yaml_valid: true,
            schema_valid: true,
            warnings: vec![],
            errors: vec![],
        }
    }
}
