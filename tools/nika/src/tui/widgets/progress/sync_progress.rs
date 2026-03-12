//! Sync Progress Widget
//!
//! Editor synchronization progress display.
//! Shows sync status for each configured editor (Claude Code, Cursor, etc.).

use std::path::PathBuf;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

use crate::sync::EditorKind;

// =============================================================================
// SOLARIZED COLOR CONSTANTS
// =============================================================================

/// Border color (blue - solarized)
const COLOR_BORDER: Color = Color::Rgb(38, 139, 210); // Blue

/// Pending color (yellow - solarized)
const COLOR_PENDING: Color = Color::Rgb(181, 137, 0); // Yellow

/// In progress color (cyan - solarized)
const COLOR_IN_PROGRESS: Color = Color::Rgb(42, 161, 152); // Cyan

/// Success color (green - solarized)
const COLOR_SUCCESS: Color = Color::Rgb(133, 153, 0); // Green

/// Skipped color (violet - solarized)
const COLOR_SKIPPED: Color = Color::Rgb(108, 113, 196); // Violet

/// Failed color (red - solarized)
const COLOR_FAILED: Color = Color::Rgb(220, 50, 47); // Red

/// Text color (base0 - solarized)
const COLOR_TEXT: Color = Color::Rgb(131, 148, 150); // Base0

/// Dim text color (base01 - solarized)
const COLOR_DIM: Color = Color::Rgb(88, 110, 117); // Base01

// =============================================================================
// SYNC STATUS
// =============================================================================

/// Synchronization status for an editor.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    /// Waiting to sync.
    Pending,
    /// Currently syncing.
    InProgress,
    /// Sync completed successfully.
    Success,
    /// Editor skipped (not installed or disabled).
    Skipped,
    /// Sync failed with error message.
    Failed(String),
}

impl SyncStatus {
    /// Get the display icon for this status.
    pub fn icon(&self) -> &'static str {
        match self {
            SyncStatus::Pending => "○",
            SyncStatus::InProgress => "◐",
            SyncStatus::Success => "✓",
            SyncStatus::Skipped => "⊘",
            SyncStatus::Failed(_) => "✗",
        }
    }

    /// Get the status color.
    pub fn color(&self) -> Color {
        match self {
            SyncStatus::Pending => COLOR_PENDING,
            SyncStatus::InProgress => COLOR_IN_PROGRESS,
            SyncStatus::Success => COLOR_SUCCESS,
            SyncStatus::Skipped => COLOR_SKIPPED,
            SyncStatus::Failed(_) => COLOR_FAILED,
        }
    }

    /// Get the status label.
    pub fn label(&self) -> &str {
        match self {
            SyncStatus::Pending => "Pending",
            SyncStatus::InProgress => "Syncing",
            SyncStatus::Success => "Synced",
            SyncStatus::Skipped => "Skipped",
            SyncStatus::Failed(_) => "Failed",
        }
    }

    /// Check if sync is in progress.
    pub fn is_active(&self) -> bool {
        matches!(self, SyncStatus::InProgress)
    }

    /// Check if sync is terminal (success, skipped, or failed).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            SyncStatus::Success | SyncStatus::Skipped | SyncStatus::Failed(_)
        )
    }
}

// =============================================================================
// EDITOR SYNC STATE
// =============================================================================

/// State of a single editor's synchronization.
#[derive(Debug, Clone)]
pub struct EditorSyncState {
    /// The editor kind.
    pub editor: EditorKind,
    /// Current sync status.
    pub status: SyncStatus,
    /// Path to the config file (when synced).
    pub config_path: Option<PathBuf>,
}

impl EditorSyncState {
    /// Create a new editor sync state.
    pub fn new(editor: EditorKind) -> Self {
        Self {
            editor,
            status: SyncStatus::Pending,
            config_path: None,
        }
    }

    /// Set the sync status.
    pub fn status(mut self, status: SyncStatus) -> Self {
        self.status = status;
        self
    }

    /// Set the config path.
    pub fn config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Get the display name for this editor.
    pub fn display_name(&self) -> &'static str {
        self.editor.display_name()
    }

    /// Get the editor icon.
    pub fn editor_icon(&self) -> &'static str {
        match self.editor {
            EditorKind::ClaudeCode => "🦋",
            EditorKind::Cursor => "⎔",
            EditorKind::Windsurf => "🌊",
            EditorKind::VsCode => "⊡",
        }
    }
}

// =============================================================================
// SYNC PROGRESS WIDGET
// =============================================================================

/// Editor sync progress widget.
///
/// Displays:
/// - List of editors with their sync status
/// - Current editor being synced (highlighted)
/// - Overall sync progress
pub struct SyncProgress {
    /// Editor sync states.
    pub editors: Vec<EditorSyncState>,
    /// Currently syncing editor.
    pub current_editor: Option<EditorKind>,
    /// Overall sync status.
    pub overall_status: SyncStatus,
}

impl SyncProgress {
    /// Create a new sync progress widget.
    pub fn new() -> Self {
        Self {
            editors: Vec::new(),
            current_editor: None,
            overall_status: SyncStatus::Pending,
        }
    }

    /// Add an editor sync state.
    pub fn add_editor(mut self, state: EditorSyncState) -> Self {
        self.editors.push(state);
        self
    }

    /// Set the editors list.
    pub fn editors(mut self, editors: Vec<EditorSyncState>) -> Self {
        self.editors = editors;
        self
    }

    /// Set the current editor being synced.
    pub fn current(mut self, editor: EditorKind) -> Self {
        self.current_editor = Some(editor);
        self
    }

    /// Set the overall sync status.
    pub fn overall_status(mut self, status: SyncStatus) -> Self {
        self.overall_status = status;
        self
    }

    /// Count completed syncs.
    pub fn completed_count(&self) -> usize {
        self.editors
            .iter()
            .filter(|e| e.status.is_terminal())
            .count()
    }

    /// Count successful syncs.
    pub fn success_count(&self) -> usize {
        self.editors
            .iter()
            .filter(|e| matches!(e.status, SyncStatus::Success))
            .count()
    }

    /// Count failed syncs.
    pub fn failed_count(&self) -> usize {
        self.editors
            .iter()
            .filter(|e| matches!(e.status, SyncStatus::Failed(_)))
            .count()
    }

    /// Calculate progress ratio (0.0 to 1.0).
    pub fn progress_ratio(&self) -> f64 {
        if self.editors.is_empty() {
            return 0.0;
        }
        self.completed_count() as f64 / self.editors.len() as f64
    }
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for SyncProgress {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 30 {
            return;
        }

        // Build block with title
        let title = format!(
            " {} Editor Sync ({}/{}) ",
            self.overall_status.icon(),
            self.success_count(),
            self.editors.len()
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(COLOR_BORDER))
            .title(title);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height == 0 || inner.width < 10 {
            return;
        }

        // Render each editor on its own line
        for (i, editor_state) in self.editors.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }

            let y = inner.y + i as u16;
            let is_current = self
                .current_editor
                .map(|e| e == editor_state.editor)
                .unwrap_or(false);

            // Build line: icon name ... status
            let editor_icon = editor_state.editor_icon();
            let editor_name = editor_state.display_name();
            let status_icon = editor_state.status.icon();
            let status_label = editor_state.status.label();

            // Editor icon and name
            let prefix = format!("{} {}", editor_icon, editor_name);
            let prefix_style = if is_current {
                Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_TEXT)
            };
            buf.set_string(inner.x, y, &prefix, prefix_style);

            // Status on the right
            let status_text = format!("{} {}", status_icon, status_label);
            let status_x = inner.right().saturating_sub(status_text.len() as u16);
            let status_style = Style::default().fg(editor_state.status.color());
            buf.set_string(status_x, y, &status_text, status_style);

            // Dots in between
            let dots_start = inner.x + prefix.len() as u16 + 1;
            let dots_end = status_x.saturating_sub(1);
            if dots_end > dots_start {
                let dots = ".".repeat((dots_end - dots_start) as usize);
                buf.set_string(dots_start, y, &dots, Style::default().fg(COLOR_DIM));
            }

            // Show config path if synced and we have space
            if let Some(ref path) = editor_state.config_path {
                if matches!(editor_state.status, SyncStatus::Success) {
                    // Check if we have an extra line
                    if ((i + 1) as u16) < inner.height {
                        let path_y = inner.y + (i + 1) as u16;
                        let path_str = format!("  └─ {}", path.display());
                        let truncated = if path_str.len() > inner.width as usize {
                            format!("{}...", &path_str[..inner.width as usize - 3])
                        } else {
                            path_str
                        };
                        buf.set_string(inner.x, path_y, &truncated, Style::default().fg(COLOR_DIM));
                    }
                }
            }

            // Show error message if failed
            if let SyncStatus::Failed(ref msg) = editor_state.status {
                if ((i + 1) as u16) < inner.height {
                    let error_y = inner.y + (i + 1) as u16;
                    let error_str = format!("  └─ {}", msg);
                    let truncated = if error_str.len() > inner.width as usize {
                        format!("{}...", &error_str[..inner.width as usize - 3])
                    } else {
                        error_str
                    };
                    buf.set_string(
                        inner.x,
                        error_y,
                        &truncated,
                        Style::default()
                            .fg(COLOR_FAILED)
                            .add_modifier(Modifier::ITALIC),
                    );
                }
            }
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_icon() {
        assert_eq!(SyncStatus::Pending.icon(), "○");
        assert_eq!(SyncStatus::InProgress.icon(), "◐");
        assert_eq!(SyncStatus::Success.icon(), "✓");
        assert_eq!(SyncStatus::Skipped.icon(), "⊘");
        assert_eq!(SyncStatus::Failed("err".to_string()).icon(), "✗");
    }

    #[test]
    fn test_sync_status_label() {
        assert_eq!(SyncStatus::Pending.label(), "Pending");
        assert_eq!(SyncStatus::InProgress.label(), "Syncing");
        assert_eq!(SyncStatus::Success.label(), "Synced");
        assert_eq!(SyncStatus::Skipped.label(), "Skipped");
        assert_eq!(SyncStatus::Failed("x".to_string()).label(), "Failed");
    }

    #[test]
    fn test_sync_status_is_active() {
        assert!(!SyncStatus::Pending.is_active());
        assert!(SyncStatus::InProgress.is_active());
        assert!(!SyncStatus::Success.is_active());
        assert!(!SyncStatus::Skipped.is_active());
        assert!(!SyncStatus::Failed("err".to_string()).is_active());
    }

    #[test]
    fn test_sync_status_is_terminal() {
        assert!(!SyncStatus::Pending.is_terminal());
        assert!(!SyncStatus::InProgress.is_terminal());
        assert!(SyncStatus::Success.is_terminal());
        assert!(SyncStatus::Skipped.is_terminal());
        assert!(SyncStatus::Failed("err".to_string()).is_terminal());
    }

    #[test]
    fn test_editor_sync_state_new() {
        let state = EditorSyncState::new(EditorKind::ClaudeCode);
        assert_eq!(state.editor, EditorKind::ClaudeCode);
        assert_eq!(state.status, SyncStatus::Pending);
        assert!(state.config_path.is_none());
    }

    #[test]
    fn test_editor_sync_state_builder() {
        let state = EditorSyncState::new(EditorKind::Cursor)
            .status(SyncStatus::Success)
            .config_path(PathBuf::from("/home/user/.cursor/mcp.json"));

        assert_eq!(state.editor, EditorKind::Cursor);
        assert_eq!(state.status, SyncStatus::Success);
        assert_eq!(
            state.config_path,
            Some(PathBuf::from("/home/user/.cursor/mcp.json"))
        );
    }

    #[test]
    fn test_editor_sync_state_display_name() {
        assert_eq!(
            EditorSyncState::new(EditorKind::ClaudeCode).display_name(),
            "Claude Code"
        );
        assert_eq!(
            EditorSyncState::new(EditorKind::Cursor).display_name(),
            "Cursor"
        );
        assert_eq!(
            EditorSyncState::new(EditorKind::Windsurf).display_name(),
            "Windsurf"
        );
        assert_eq!(
            EditorSyncState::new(EditorKind::VsCode).display_name(),
            "VS Code"
        );
    }

    #[test]
    fn test_editor_sync_state_icon() {
        assert_eq!(
            EditorSyncState::new(EditorKind::ClaudeCode).editor_icon(),
            "🦋"
        );
        assert_eq!(EditorSyncState::new(EditorKind::Cursor).editor_icon(), "⎔");
        assert_eq!(
            EditorSyncState::new(EditorKind::Windsurf).editor_icon(),
            "🌊"
        );
        assert_eq!(EditorSyncState::new(EditorKind::VsCode).editor_icon(), "⊡");
    }

    #[test]
    fn test_sync_progress_new() {
        let progress = SyncProgress::new();
        assert!(progress.editors.is_empty());
        assert!(progress.current_editor.is_none());
        assert_eq!(progress.overall_status, SyncStatus::Pending);
    }

    #[test]
    fn test_sync_progress_default() {
        let progress = SyncProgress::default();
        assert!(progress.editors.is_empty());
    }

    #[test]
    fn test_sync_progress_add_editor() {
        let progress = SyncProgress::new()
            .add_editor(EditorSyncState::new(EditorKind::ClaudeCode))
            .add_editor(EditorSyncState::new(EditorKind::Cursor));

        assert_eq!(progress.editors.len(), 2);
    }

    #[test]
    fn test_sync_progress_counts() {
        let progress = SyncProgress::new().editors(vec![
            EditorSyncState::new(EditorKind::ClaudeCode).status(SyncStatus::Success),
            EditorSyncState::new(EditorKind::Cursor).status(SyncStatus::Success),
            EditorSyncState::new(EditorKind::Windsurf).status(SyncStatus::Skipped),
            EditorSyncState::new(EditorKind::VsCode).status(SyncStatus::Failed("err".to_string())),
        ]);

        assert_eq!(progress.completed_count(), 4);
        assert_eq!(progress.success_count(), 2);
        assert_eq!(progress.failed_count(), 1);
    }

    #[test]
    fn test_sync_progress_ratio() {
        let progress = SyncProgress::new().editors(vec![
            EditorSyncState::new(EditorKind::ClaudeCode).status(SyncStatus::Success),
            EditorSyncState::new(EditorKind::Cursor).status(SyncStatus::InProgress),
        ]);

        assert!((progress.progress_ratio() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_sync_progress_ratio_empty() {
        let progress = SyncProgress::new();
        assert!((progress.progress_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_widget_renders_without_panic() {
        let progress = SyncProgress::new()
            .editors(vec![
                EditorSyncState::new(EditorKind::ClaudeCode).status(SyncStatus::Success),
                EditorSyncState::new(EditorKind::Cursor).status(SyncStatus::InProgress),
            ])
            .current(EditorKind::Cursor)
            .overall_status(SyncStatus::InProgress);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        progress.render(Rect::new(0, 0, 60, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Claude Code"));
        assert!(content.contains("Cursor"));
    }

    #[test]
    fn test_widget_renders_failed_state() {
        let progress = SyncProgress::new()
            .editors(vec![EditorSyncState::new(EditorKind::ClaudeCode)
                .status(SyncStatus::Failed("Permission denied".to_string()))]);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 10));
        progress.render(Rect::new(0, 0, 60, 10), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Claude Code"));
        assert!(content.contains("Failed"));
    }

    #[test]
    fn test_widget_handles_small_area() {
        let progress =
            SyncProgress::new().editors(vec![EditorSyncState::new(EditorKind::ClaudeCode)]);

        // Too small - should not panic
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        progress.render(Rect::new(0, 0, 10, 2), &mut buf);
    }

    #[test]
    fn test_status_color() {
        // Verify colors are different for different states
        assert_ne!(SyncStatus::Pending.color(), SyncStatus::Success.color());
        assert_ne!(
            SyncStatus::InProgress.color(),
            SyncStatus::Failed("err".to_string()).color()
        );
    }
}
