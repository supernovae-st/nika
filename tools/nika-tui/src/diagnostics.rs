// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TUI Diagnostics Bridge
//!
//! Bridges the Two-Phase IR analyzer with the TUI editor, providing
//! real-time error highlighting and inline diagnostics display.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │  Editor Buffer                                                          │
//! │       │                                                                 │
//! │       │ on_change()                                                     │
//! │       ▼                                                                 │
//! │  ┌─────────────────────────────────────────────────────────┐           │
//! │  │  DiagnosticsEngine                                       │           │
//! │  │  ├── analyze_text() → Parse + Analyze                    │           │
//! │  │  ├── get_diagnostics() → Vec<TuiDiagnostic>              │           │
//! │  │  └── get_line_diagnostics(line) → For gutter display     │           │
//! │  └─────────────────────────────────────────────────────────┘           │
//! │       │                                                                 │
//! │       ▼                                                                 │
//! │  Styled Line Rendering (error highlights, gutter icons)                 │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::style::{Color, Modifier, Style};

use nika_engine::ast::analyzer::{analyze, AnalyzeError};
use nika_engine::ast::raw::{self, ParseError};
use nika_engine::source::FileId;

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// Critical error that prevents workflow execution
    Error,
    /// Non-blocking warning
    Warning,
    /// Informational hint
    Hint,
}

impl DiagnosticSeverity {
    /// Get the gutter icon for this severity
    pub fn gutter_icon(&self) -> &'static str {
        match self {
            DiagnosticSeverity::Error => "●",
            DiagnosticSeverity::Warning => "▲",
            DiagnosticSeverity::Hint => "◆",
        }
    }

    /// Get the style for this severity (Solarized-compatible)
    pub fn style(&self) -> Style {
        match self {
            DiagnosticSeverity::Error => Style::default()
                .fg(Color::Rgb(220, 50, 47)) // Solarized red
                .add_modifier(Modifier::BOLD),
            DiagnosticSeverity::Warning => Style::default()
                .fg(Color::Rgb(203, 75, 22)) // Solarized orange
                .add_modifier(Modifier::BOLD),
            DiagnosticSeverity::Hint => Style::default()
                .fg(Color::Rgb(38, 139, 210)) // Solarized blue
                .add_modifier(Modifier::ITALIC),
        }
    }

    /// Get the underline style for inline highlighting
    pub fn underline_style(&self) -> Style {
        match self {
            DiagnosticSeverity::Error => Style::default()
                .fg(Color::Rgb(220, 50, 47))
                .add_modifier(Modifier::UNDERLINED),
            DiagnosticSeverity::Warning => Style::default()
                .fg(Color::Rgb(203, 75, 22))
                .add_modifier(Modifier::UNDERLINED),
            DiagnosticSeverity::Hint => Style::default()
                .fg(Color::Rgb(38, 139, 210))
                .add_modifier(Modifier::UNDERLINED),
        }
    }
}

/// A diagnostic message for the TUI editor
#[derive(Debug, Clone)]
pub struct TuiDiagnostic {
    /// Error code (e.g., "NIKA-140")
    pub code: String,
    /// Human-readable message
    pub message: String,
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// Start line (0-indexed)
    pub start_line: usize,
    /// Start column (0-indexed)
    pub start_col: usize,
    /// End line (0-indexed)
    pub end_line: usize,
    /// End column (0-indexed)
    pub end_col: usize,
}

impl TuiDiagnostic {
    /// Create from an AnalyzeError
    pub fn from_analyze_error(error: &AnalyzeError, source: &str) -> Self {
        let (start_line, start_col) = offset_to_line_col(error.span.start.into(), source);
        let (end_line, end_col) = offset_to_line_col(error.span.end.into(), source);

        Self {
            code: error.kind.code().to_string(),
            message: error.message.clone(),
            severity: DiagnosticSeverity::Error, // All analyze errors are errors
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    /// Create from a ParseError
    pub fn from_parse_error(error: &ParseError, source: &str) -> Self {
        let (start_line, start_col) = offset_to_line_col(error.span.start.into(), source);
        let (end_line, end_col) = offset_to_line_col(error.span.end.into(), source);

        Self {
            code: error.kind.code().to_string(),
            message: error.message.clone(),
            severity: DiagnosticSeverity::Error,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    /// Check if this diagnostic affects a given line
    pub fn affects_line(&self, line: usize) -> bool {
        line >= self.start_line && line <= self.end_line
    }

    /// Get the column range for a specific line
    pub fn column_range_for_line(&self, line: usize) -> Option<(usize, usize)> {
        if !self.affects_line(line) {
            return None;
        }

        let start = if line == self.start_line {
            self.start_col
        } else {
            0
        };

        let end = if line == self.end_line {
            self.end_col
        } else {
            usize::MAX // Full line
        };

        Some((start, end))
    }

    /// Format for display in a message area
    pub fn display_message(&self) -> String {
        format!("[{}] {}", self.code, self.message)
    }

    /// Format with location for status bar
    pub fn status_message(&self) -> String {
        format!(
            "[{}] line {}:{} - {}",
            self.code,
            self.start_line + 1,
            self.start_col + 1,
            self.message
        )
    }
}

/// Convert byte offset to (line, column) - both 0-indexed
fn offset_to_line_col(offset: usize, source: &str) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;

    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    (line, col)
}

/// Diagnostics engine for the TUI editor
///
/// Caches analysis results and provides efficient line-based lookups.
#[derive(Debug, Default)]
pub struct DiagnosticsEngine {
    /// Current diagnostics
    diagnostics: Vec<TuiDiagnostic>,
    /// Hash of last analyzed text (for caching)
    last_text_hash: u64,
}

impl DiagnosticsEngine {
    /// Create a new diagnostics engine
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            last_text_hash: 0,
        }
    }

    /// Analyze text and update diagnostics
    ///
    /// Returns true if diagnostics changed.
    pub fn analyze(&mut self, source: &str) -> bool {
        // Quick hash check to avoid re-analyzing identical content
        let hash = xxhash_rust::xxh3::xxh3_64(source.as_bytes());
        if hash == self.last_text_hash {
            return false;
        }
        self.last_text_hash = hash;

        self.diagnostics.clear();

        let file_id = FileId(0);

        // Phase 1: Parse to Raw AST
        match raw::parse(source, file_id) {
            Ok(raw_workflow) => {
                // Phase 2: Analyze
                let result = analyze(raw_workflow);

                // Convert analysis errors to TUI diagnostics
                for error in &result.errors {
                    self.diagnostics
                        .push(TuiDiagnostic::from_analyze_error(error, source));
                }
            }
            Err(parse_error) => {
                // Parse failed
                self.diagnostics
                    .push(TuiDiagnostic::from_parse_error(&parse_error, source));
            }
        }

        true
    }

    /// Get all diagnostics
    pub fn diagnostics(&self) -> &[TuiDiagnostic] {
        &self.diagnostics
    }

    /// Get diagnostics affecting a specific line
    pub fn diagnostics_for_line(&self, line: usize) -> Vec<&TuiDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.affects_line(line))
            .collect()
    }

    /// Check if a line has any diagnostics
    pub fn has_diagnostics_on_line(&self, line: usize) -> bool {
        self.diagnostics.iter().any(|d| d.affects_line(line))
    }

    /// Get the most severe diagnostic on a line (for gutter icon)
    pub fn most_severe_on_line(&self, line: usize) -> Option<&TuiDiagnostic> {
        self.diagnostics_for_line(line)
            .into_iter()
            .min_by_key(|d| match d.severity {
                DiagnosticSeverity::Error => 0,
                DiagnosticSeverity::Warning => 1,
                DiagnosticSeverity::Hint => 2,
            })
    }

    /// Get error count
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count()
    }

    /// Get warning count
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
            .count()
    }

    /// Clear all diagnostics
    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.last_text_hash = 0;
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error)
    }

    /// Get the first error (for status bar display)
    pub fn first_error(&self) -> Option<&TuiDiagnostic> {
        self.diagnostics
            .iter()
            .find(|d| d.severity == DiagnosticSeverity::Error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_to_line_col() {
        let source = "line1\nline2\nline3";
        assert_eq!(offset_to_line_col(0, source), (0, 0));
        assert_eq!(offset_to_line_col(3, source), (0, 3));
        assert_eq!(offset_to_line_col(6, source), (1, 0)); // Start of line2
        assert_eq!(offset_to_line_col(8, source), (1, 2)); // "ne" of line2
        assert_eq!(offset_to_line_col(12, source), (2, 0)); // Start of line3
    }

    #[test]
    fn test_diagnostic_affects_line() {
        let diag = TuiDiagnostic {
            code: "NIKA-140".to_string(),
            message: "Test error".to_string(),
            severity: DiagnosticSeverity::Error,
            start_line: 2,
            start_col: 5,
            end_line: 4,
            end_col: 10,
        };

        assert!(!diag.affects_line(0));
        assert!(!diag.affects_line(1));
        assert!(diag.affects_line(2));
        assert!(diag.affects_line(3));
        assert!(diag.affects_line(4));
        assert!(!diag.affects_line(5));
    }

    #[test]
    fn test_diagnostic_column_range() {
        let diag = TuiDiagnostic {
            code: "NIKA-140".to_string(),
            message: "Test error".to_string(),
            severity: DiagnosticSeverity::Error,
            start_line: 2,
            start_col: 5,
            end_line: 4,
            end_col: 10,
        };

        // Line before range
        assert_eq!(diag.column_range_for_line(1), None);

        // Start line
        assert_eq!(diag.column_range_for_line(2), Some((5, usize::MAX)));

        // Middle line
        assert_eq!(diag.column_range_for_line(3), Some((0, usize::MAX)));

        // End line
        assert_eq!(diag.column_range_for_line(4), Some((0, 10)));

        // Line after range
        assert_eq!(diag.column_range_for_line(5), None);
    }

    #[test]
    fn test_single_line_diagnostic_column_range() {
        let diag = TuiDiagnostic {
            code: "NIKA-140".to_string(),
            message: "Test error".to_string(),
            severity: DiagnosticSeverity::Error,
            start_line: 3,
            start_col: 5,
            end_line: 3,
            end_col: 15,
        };

        assert_eq!(diag.column_range_for_line(3), Some((5, 15)));
    }

    #[test]
    fn test_diagnostics_engine_empty() {
        let engine = DiagnosticsEngine::new();
        assert!(engine.diagnostics().is_empty());
        assert!(!engine.has_errors());
        assert_eq!(engine.error_count(), 0);
    }

    #[test]
    fn test_diagnostics_engine_valid_workflow() {
        let mut engine = DiagnosticsEngine::new();
        let yaml = r#"schema: nika/workflow@0.12
workflow: test
model: test-model

tasks:
  - id: step1
    infer: "Hello"
"#;

        engine.analyze(yaml);
        assert!(
            !engine.has_errors(),
            "Should have no errors: {:?}",
            engine.diagnostics()
        );
    }

    #[test]
    fn test_diagnostics_engine_duplicate_task() {
        let mut engine = DiagnosticsEngine::new();
        let yaml = r#"schema: nika/workflow@0.12
workflow: test

tasks:
  - id: step1
    infer: "Hello"
  - id: step1
    exec: "echo duplicate"
"#;

        engine.analyze(yaml);
        assert!(engine.has_errors());
        // Duplicate task IDs are now caught at parse time (NIKA-162)
        assert!(engine.diagnostics().iter().any(|d| d.code == "NIKA-162"));
    }

    #[test]
    fn test_diagnostics_engine_parse_error() {
        let mut engine = DiagnosticsEngine::new();
        let yaml = "schema: nika/workflow@0.12\ntasks: [unclosed";

        engine.analyze(yaml);
        assert!(engine.has_errors());
        assert!(engine.diagnostics().iter().any(|d| d.code == "NIKA-160"));
    }

    #[test]
    fn test_severity_styles() {
        let error_style = DiagnosticSeverity::Error.style();
        let warning_style = DiagnosticSeverity::Warning.style();
        let hint_style = DiagnosticSeverity::Hint.style();

        // Just verify they return valid styles
        assert!(error_style.fg.is_some());
        assert!(warning_style.fg.is_some());
        assert!(hint_style.fg.is_some());
    }

    #[test]
    fn test_gutter_icons() {
        assert_eq!(DiagnosticSeverity::Error.gutter_icon(), "●");
        assert_eq!(DiagnosticSeverity::Warning.gutter_icon(), "▲");
        assert_eq!(DiagnosticSeverity::Hint.gutter_icon(), "◆");
    }

    #[test]
    fn test_diagnostics_engine_caching() {
        let mut engine = DiagnosticsEngine::new();
        let yaml = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: step1
    infer: "Hello"
"#;

        // First analysis
        let changed1 = engine.analyze(yaml);
        assert!(changed1);

        // Same content - should not change
        let changed2 = engine.analyze(yaml);
        assert!(!changed2);

        // Different content
        let changed3 = engine.analyze("schema: nika/workflow@0.12\n");
        assert!(changed3);
    }

    #[test]
    fn test_most_severe_on_line() {
        // This would require multiple diagnostics on the same line
        // which is harder to construct, so we test basic functionality
        let mut engine = DiagnosticsEngine::new();
        let yaml = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: step1
    infer: "Hello"
"#;
        engine.analyze(yaml);

        // No errors, so no most severe
        assert!(engine.most_severe_on_line(0).is_none());
    }
}
