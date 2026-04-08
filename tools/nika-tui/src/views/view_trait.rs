// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! View trait for polymorphic TUI views
//!
//! Each view (Chat, Home, Studio, Monitor) implements this trait
//! for consistent rendering and input handling.
//!
//! Added lifecycle hooks (`on_enter`, `on_leave`, `tick`) for
//! animation support and view transition handling.

use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use super::ViewAction;
use crate::state::TuiState;
use crate::theme::Theme;

/// Trait for TUI views
///
/// Each view (Chat, Home, Studio, Monitor) implements this trait
/// for consistent rendering and input handling.
///
/// ## Lifecycle Hooks
///
/// Views can override lifecycle hooks for animation and state management:
/// - `on_enter`: Called when view becomes active (focus gained)
/// - `on_leave`: Called when view becomes inactive (focus lost)
/// - `tick`: Called each frame for animations (60fps target)
///
/// All hooks have default no-op implementations.
pub trait View {
    /// Render the view to the frame
    /// Changed to &mut self to support scroll state updates during render
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme);

    /// Handle a key event, returning an action
    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction;

    /// Get the view's status line text (for footer)
    fn status_line(&self, state: &TuiState) -> String;

    // ─────────────────────────────────────────────────────────────────────────
    // Lifecycle Hooks - Default implementations provided
    // Reserved for future view lifecycle management (not yet wired)
    // ─────────────────────────────────────────────────────────────────────────

    /// Called when this view becomes active (gains focus)
    ///
    /// Use this to:
    /// - Start animations
    /// - Load/refresh data
    /// - Initialize view-specific state
    ///
    /// Default: no-op
    fn on_enter(&mut self, _state: &mut TuiState) {}

    /// Called when this view becomes inactive (loses focus)
    ///
    /// Use this to:
    /// - Pause animations
    /// - Save draft state
    /// - Clean up temporary resources
    ///
    /// Default: no-op
    fn on_leave(&mut self, _state: &mut TuiState) {}

    /// Called each frame for animation updates (target: 60fps)
    ///
    /// Use this to:
    /// - Update animation frames (spinners, pulses, effects)
    /// - Progress time-based state
    /// - Update streaming content display
    ///
    /// Performance: Must complete in <1ms to maintain 60fps.
    ///
    /// Default: no-op
    fn tick(&mut self, _state: &mut TuiState) {}

    /// Whether this view needs background ticking even when inactive
    ///
    /// Override to return true for views that need updates while
    /// inactive (e.g., streaming content, background operations).
    ///
    /// Default: false (only tick when active)
    fn needs_background_tick(&self) -> bool {
        false
    }
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
        fn render(&mut self, _frame: &mut Frame, _area: Rect, _state: &TuiState, _theme: &Theme) {
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

    // ─────────────────────────────────────────────────────────────────────────
    // Lifecycle Hook Tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_lifecycle_hooks_have_default_implementations() {
        // MockView doesn't override lifecycle hooks, so defaults are used
        let mut view = MockView::new();
        let mut state = TuiState::new("test.nika.yaml");

        // These should compile and not panic (default no-ops)
        view.on_enter(&mut state);
        view.on_leave(&mut state);
        view.tick(&mut state);
    }

    // Mock view that tracks lifecycle hook calls
    struct MockViewWithHooks {
        on_enter_count: usize,
        on_leave_count: usize,
        tick_count: usize,
    }

    impl MockViewWithHooks {
        fn new() -> Self {
            Self {
                on_enter_count: 0,
                on_leave_count: 0,
                tick_count: 0,
            }
        }
    }

    impl View for MockViewWithHooks {
        fn render(&mut self, _frame: &mut Frame, _area: Rect, _state: &TuiState, _theme: &Theme) {}
        fn handle_key(&mut self, _key: KeyEvent, _state: &mut TuiState) -> ViewAction {
            ViewAction::None
        }
        fn status_line(&self, _state: &TuiState) -> String {
            "MockWithHooks".to_string()
        }

        fn on_enter(&mut self, _state: &mut TuiState) {
            self.on_enter_count += 1;
        }

        fn on_leave(&mut self, _state: &mut TuiState) {
            self.on_leave_count += 1;
        }

        fn tick(&mut self, _state: &mut TuiState) {
            self.tick_count += 1;
        }
    }

    #[test]
    fn test_on_enter_hook_is_called() {
        let mut view = MockViewWithHooks::new();
        let mut state = TuiState::new("test.nika.yaml");

        assert_eq!(view.on_enter_count, 0);
        view.on_enter(&mut state);
        assert_eq!(view.on_enter_count, 1);
        view.on_enter(&mut state);
        assert_eq!(view.on_enter_count, 2);
    }

    #[test]
    fn test_on_leave_hook_is_called() {
        let mut view = MockViewWithHooks::new();
        let mut state = TuiState::new("test.nika.yaml");

        assert_eq!(view.on_leave_count, 0);
        view.on_leave(&mut state);
        assert_eq!(view.on_leave_count, 1);
    }

    #[test]
    fn test_tick_hook_is_called_multiple_times() {
        let mut view = MockViewWithHooks::new();
        let mut state = TuiState::new("test.nika.yaml");

        assert_eq!(view.tick_count, 0);
        for _ in 0..60 {
            view.tick(&mut state);
        }
        assert_eq!(view.tick_count, 60, "60 ticks for 1 second at 60fps");
    }

    #[test]
    fn test_lifecycle_sequence_enter_tick_leave() {
        let mut view = MockViewWithHooks::new();
        let mut state = TuiState::new("test.nika.yaml");

        // Typical lifecycle: enter → multiple ticks → leave
        view.on_enter(&mut state);
        assert_eq!(view.on_enter_count, 1);

        for _ in 0..10 {
            view.tick(&mut state);
        }
        assert_eq!(view.tick_count, 10);

        view.on_leave(&mut state);
        assert_eq!(view.on_leave_count, 1);

        // After leave, ticks can still happen (view still exists)
        view.tick(&mut state);
        assert_eq!(view.tick_count, 11);
    }

    #[test]
    fn test_needs_background_tick_default_is_false() {
        let view = MockView::new();
        // Default implementation should return false
        assert!(!view.needs_background_tick());
    }

    #[test]
    fn test_needs_background_tick_on_hooks_view_is_false() {
        let view = MockViewWithHooks::new();
        // MockViewWithHooks also uses default (false)
        assert!(!view.needs_background_tick());
    }
}
