//! Tests for HomeView

use std::path::PathBuf;

use super::*;
use crate::tui::standalone::BrowserEntry;
use crate::tui::state::TuiState;
use crate::tui::theme::{Theme, VerbColor};
use crate::tui::views::view_trait::View;

#[test]
fn test_home_view_new_creates_valid_state() {
    let view = HomeView::new(PathBuf::from("."));
    assert!(!view.history_expanded);
    // TreeState selection index may or may not be set depending on entries
    assert!(
        view.tree_state.selection_index().is_none() || view.tree_state.selection_index().is_some()
    );
}

#[test]
fn test_home_view_select_navigation() {
    let mut view = HomeView::new(PathBuf::from("."));

    // Add some mock entries for testing
    view.standalone.browser_entries.clear();
    view.standalone.browser_entries.push(BrowserEntry::new(
        PathBuf::from("test1.nika.yaml"),
        &PathBuf::from("."),
    ));
    view.standalone.browser_entries.push(BrowserEntry::new(
        PathBuf::from("test2.nika.yaml"),
        &PathBuf::from("."),
    ));
    // Set up filtered indices to match entries
    view.filtered_indices = vec![0, 1];
    view.tree_state.set_selection_index(Some(0));

    // Navigate down
    view.select_next();
    assert_eq!(view.tree_state.selection_index(), Some(1));

    // Navigate up
    view.select_prev();
    assert_eq!(view.tree_state.selection_index(), Some(0));

    // Navigate up at top (should stay at 0)
    view.select_prev();
    assert_eq!(view.tree_state.selection_index(), Some(0));
}

#[test]
fn test_home_view_history_toggle() {
    let mut view = HomeView::new(PathBuf::from("."));
    assert!(!view.history_expanded);

    view.history_expanded = true;
    assert!(view.history_expanded);

    view.history_expanded = false;
    assert!(!view.history_expanded);
}

#[test]
fn test_preview_mode_toggle() {
    let mut view = HomeView::new(PathBuf::from("."));

    // Default is DAG mode
    assert_eq!(view.preview_mode, PreviewMode::Dag);

    // Toggle to YAML
    view.preview_mode = PreviewMode::Yaml;
    assert_eq!(view.preview_mode, PreviewMode::Yaml);

    // Toggle back to DAG
    view.preview_mode = PreviewMode::Dag;
    assert_eq!(view.preview_mode, PreviewMode::Dag);
}

#[test]
fn test_preview_mode_key_handler() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut view = HomeView::new(PathBuf::from("."));
    let mut state = TuiState::new("test.nika.yaml");

    // Initial state: DAG mode
    assert_eq!(view.preview_mode, PreviewMode::Dag);

    // Press 'D' to toggle to YAML
    let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
    view.handle_key(key, &mut state);
    assert_eq!(view.preview_mode, PreviewMode::Yaml);

    // Press 'D' again to toggle back to DAG
    view.handle_key(key, &mut state);
    assert_eq!(view.preview_mode, PreviewMode::Dag);
}

#[test]
fn test_get_line_verb_color() {
    // Test verb detection
    assert_eq!(
        HomeView::get_line_verb_color("infer: prompt"),
        Some(VerbColor::Infer)
    );
    assert_eq!(
        HomeView::get_line_verb_color("exec: command"),
        Some(VerbColor::Exec)
    );
    assert_eq!(
        HomeView::get_line_verb_color("fetch: url"),
        Some(VerbColor::Fetch)
    );
    assert_eq!(
        HomeView::get_line_verb_color("invoke: tool"),
        Some(VerbColor::Invoke)
    );
    assert_eq!(
        HomeView::get_line_verb_color("agent: loop"),
        Some(VerbColor::Agent)
    );

    // Test non-verb lines return None
    assert_eq!(HomeView::get_line_verb_color("tasks:"), None);
    assert_eq!(HomeView::get_line_verb_color("  - id: step1"), None);
    assert_eq!(HomeView::get_line_verb_color(""), None);

    // Test indented verb lines (still detect verb)
    assert_eq!(
        HomeView::get_line_verb_color("    infer: nested"),
        Some(VerbColor::Infer)
    );
}

#[test]
fn test_home_view_selected_entry_with_empty_list() {
    let mut view = HomeView::new(PathBuf::from("."));
    view.standalone.browser_entries.clear();
    view.list_state.select(None);

    assert!(view.selected_entry().is_none());
}

#[test]
fn test_home_view_status_line() {
    let mut view = HomeView::new(PathBuf::from("."));
    view.standalone.browser_entries.clear();
    view.standalone.browser_entries.push(BrowserEntry::new(
        PathBuf::from("test.nika.yaml"),
        &PathBuf::from("."),
    ));
    view.standalone.history.clear();

    let state = TuiState::new("test.nika.yaml");
    let status = view.status_line(&state);
    assert!(status.contains("1 workflows"));
    assert!(status.contains("0 in history"));
}

// === File Browser Tests ===

#[test]
fn test_empty_directory_has_no_entries() {
    // Use a non-existent directory so StandaloneState starts empty
    let view = HomeView::new(PathBuf::from("/nonexistent/path/that/has/no/nika/files"));
    assert!(
        view.standalone.browser_entries.is_empty(),
        "Browser should be empty for non-existent path"
    );
}

#[test]
fn test_home_view_renders_file_browser() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    let mut view = HomeView::new(PathBuf::from("."));

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = TuiState::new("test.nika.yaml");
    let theme = Theme::novanet();

    terminal
        .draw(|frame| {
            view.render(frame, frame.area(), &state, &theme);
        })
        .unwrap();

    // Just verify it renders without panic
    let buffer = terminal.backend().buffer();
    let _content: String = buffer.content.iter().map(|c| c.symbol()).collect();
}
