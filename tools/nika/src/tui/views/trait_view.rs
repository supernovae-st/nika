//! View trait for polymorphic TUI views
//!
//! Each view (Chat, Home, Studio, Monitor) implements this trait
//! for consistent rendering and input handling.

use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use super::ViewAction;
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;

/// Trait for TUI views
///
/// Each view (Chat, Home, Studio, Monitor) implements this trait
/// for consistent rendering and input handling.
pub trait View {
    /// Render the view to the frame
    fn render(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme);

    /// Handle a key event, returning an action
    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction;

    /// Get the view's status line text (for footer)
    fn status_line(&self, state: &TuiState) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock view for testing the View trait
    struct MockView;

    impl MockView {
        fn new() -> Self {
            Self
        }
    }

    impl View for MockView {
        fn render(&self, _frame: &mut Frame, _area: Rect, _state: &TuiState, _theme: &Theme) {
            // No-op for tests
        }

        fn handle_key(&mut self, _key: KeyEvent, _state: &mut TuiState) -> ViewAction {
            ViewAction::None
        }

        fn status_line(&self, _state: &TuiState) -> String {
            "[Test] Mock view".to_string()
        }
    }

    #[test]
    fn test_mock_view_status_line() {
        let view = MockView::new();
        let state = TuiState::new("test.nika.yaml");
        assert_eq!(view.status_line(&state), "[Test] Mock view");
    }

    #[test]
    fn test_mock_view_handle_key_returns_none() {
        let mut view = MockView::new();
        let mut state = TuiState::new("test.nika.yaml");
        let key = KeyEvent::from(crossterm::event::KeyCode::Char('x'));
        let action = view.handle_key(key, &mut state);
        match action {
            ViewAction::None => {}
            _ => panic!("Expected ViewAction::None"),
        }
    }

    #[test]
    fn test_mock_view_implements_view_trait() {
        // Verify that MockView can be used as a trait object
        let view = MockView::new();
        let _: &dyn View = &view;
    }
}
