//! Event Handling - Keyboard input processing

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

use super::state::{AppState, PanelFocus, WorkflowStatus};

/// Actions that can be triggered by user input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Pause,
    Resume,
    Restart,
    NextPanel,
    PrevPanel,
    ScrollUp,
    ScrollDown,
    Select,
    Help,
    None,
}

/// Handle keyboard events
pub fn handle_key_event(key: KeyEvent, state: &mut AppState) -> Action {
    // Global keybindings (work in any state)
    match (key.modifiers, key.code) {
        // Quit: q or Ctrl+C
        (KeyModifiers::NONE, KeyCode::Char('q')) => return Action::Quit,
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Action::Quit,

        // Help: F1 or ?
        (KeyModifiers::NONE, KeyCode::F(1)) => return Action::Help,
        (KeyModifiers::SHIFT, KeyCode::Char('?')) => return Action::Help,

        // Panel navigation: Tab / Shift+Tab
        (KeyModifiers::NONE, KeyCode::Tab) => {
            state.focus = state.focus.next();
            return Action::NextPanel;
        }
        (KeyModifiers::SHIFT, KeyCode::BackTab) => {
            state.focus = state.focus.prev();
            return Action::PrevPanel;
        }

        _ => {}
    }

    // Workflow control keybindings
    match (key.modifiers, key.code) {
        // Pause/Resume: p or Space
        (KeyModifiers::NONE, KeyCode::Char('p')) | (KeyModifiers::NONE, KeyCode::Char(' ')) => {
            match state.status {
                WorkflowStatus::Running => {
                    state.status = WorkflowStatus::Paused;
                    return Action::Pause;
                }
                WorkflowStatus::Paused => {
                    state.status = WorkflowStatus::Running;
                    return Action::Resume;
                }
                _ => {}
            }
        }

        // Restart: r
        (KeyModifiers::NONE, KeyCode::Char('r')) => {
            if state.status == WorkflowStatus::Completed || state.status == WorkflowStatus::Failed {
                return Action::Restart;
            }
        }

        _ => {}
    }

    // Panel-specific keybindings
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            scroll_panel(state, -1);
            return Action::ScrollUp;
        }
        KeyCode::Down | KeyCode::Char('j') => {
            scroll_panel(state, 1);
            return Action::ScrollDown;
        }
        KeyCode::Enter => {
            return Action::Select;
        }
        _ => {}
    }

    Action::None
}

/// Scroll the currently focused panel
fn scroll_panel(state: &mut AppState, delta: i32) {
    let panel = match state.focus {
        PanelFocus::Activity => super::state::Panel::Activity,
        PanelFocus::Subagents => super::state::Panel::Subagents,
        PanelFocus::Connections => super::state::Panel::Connections,
        PanelFocus::Skills => super::state::Panel::Skills,
        _ => return, // Non-scrollable panels
    };

    let current = state.scroll_positions.get(&panel).copied().unwrap_or(0);
    let new = if delta < 0 {
        current.saturating_sub((-delta) as usize)
    } else {
        current.saturating_add(delta as usize)
    };
    state.scroll_positions.insert(panel, new);
}

/// Poll for keyboard events with timeout
pub fn poll_event(timeout: Duration) -> std::io::Result<Option<KeyEvent>> {
    if event::poll(timeout)? {
        if let Event::Key(key) = event::read()? {
            return Ok(Some(key));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quit_action() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(handle_key_event(key, &mut state), Action::Quit);
    }

    #[test]
    fn test_ctrl_c_quit() {
        let mut state = AppState::default();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(handle_key_event(key, &mut state), Action::Quit);
    }

    #[test]
    fn test_tab_cycles_focus() {
        let mut state = AppState::default();
        assert_eq!(state.focus, PanelFocus::Dag);

        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        handle_key_event(key, &mut state);
        assert_eq!(state.focus, PanelFocus::Session);
    }

    #[test]
    fn test_pause_resume() {
        let mut state = AppState::default();
        state.status = WorkflowStatus::Running;

        let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
        let action = handle_key_event(key, &mut state);
        assert_eq!(action, Action::Pause);
        assert_eq!(state.status, WorkflowStatus::Paused);

        let action = handle_key_event(key, &mut state);
        assert_eq!(action, Action::Resume);
        assert_eq!(state.status, WorkflowStatus::Running);
    }
}
