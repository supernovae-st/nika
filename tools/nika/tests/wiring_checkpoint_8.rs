//! WIRING-8: Monitor View Integration
//!
//! Verifies: MonitorView implements View trait correctly
//! Run after: v0.11.0 (Six Views Architecture)
//!
//! Tests validate:
//! - MonitorView construction and defaults
//! - Panel focus cycling (4 panels: Progress, DAG, NovaNet, Agent)
//! - View trait implementation (status_line, tick)
//! - Key event handling

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika::tui::{MonitorView, NavPanelId as PanelId, TuiState, TuiView, View, ViewAction};

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: MonitorView construction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_8_monitor_view_new() {
    let view = MonitorView::new();
    assert_eq!(
        view.focus,
        PanelId::RunnerMission,
        "Default focus should be Progress"
    );
}

#[test]
fn wiring_8_monitor_view_default() {
    let view = MonitorView::default();
    assert_eq!(view.focus, PanelId::RunnerMission);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: Panel focus cycling
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_8_focus_next_cycles_four_panels() {
    let mut view = MonitorView::new();

    assert_eq!(view.focus, PanelId::RunnerMission);

    view.focus_next();
    assert_eq!(view.focus, PanelId::RunnerDag);

    view.focus_next();
    assert_eq!(view.focus, PanelId::RunnerNovanet);

    view.focus_next();
    assert_eq!(view.focus, PanelId::RunnerReasoning);

    view.focus_next();
    assert_eq!(
        view.focus,
        PanelId::RunnerMission,
        "Should cycle back to Progress"
    );
}

#[test]
fn wiring_8_focus_prev_cycles_four_panels() {
    let mut view = MonitorView::new();

    assert_eq!(view.focus, PanelId::RunnerMission);

    view.focus_prev();
    assert_eq!(
        view.focus,
        PanelId::RunnerReasoning,
        "Prev from Progress goes to Agent"
    );

    view.focus_prev();
    assert_eq!(view.focus, PanelId::RunnerNovanet);

    view.focus_prev();
    assert_eq!(view.focus, PanelId::RunnerDag);

    view.focus_prev();
    assert_eq!(view.focus, PanelId::RunnerMission);
}

#[test]
fn wiring_8_panel_id_has_four_variants() {
    // v0.12.1: Use panels_for_view instead of all() for 6-Views architecture
    let all = PanelId::panels_for_view(TuiView::Runner);
    assert_eq!(all.len(), 4, "MonitorView should have 4 panels");
    assert_eq!(all[0], PanelId::RunnerMission);
    assert_eq!(all[1], PanelId::RunnerDag);
    assert_eq!(all[2], PanelId::RunnerNovanet);
    assert_eq!(all[3], PanelId::RunnerReasoning);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: View trait implementation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_8_view_trait_status_line() {
    let view = MonitorView::new();
    let state = TuiState::new("test.nika.yaml");

    let status = view.status_line(&state);
    assert!(!status.is_empty(), "Status line should not be empty");
}

#[test]
fn wiring_8_view_trait_tick_no_panic() {
    let mut view = MonitorView::new();
    let mut state = TuiState::new("test.nika.yaml");

    // tick() should not panic - reaching here means success
    view.tick(&mut state);
}

#[test]
fn wiring_8_view_trait_polymorphism() {
    fn assert_view<V: View>(_v: &V) {}

    let view = MonitorView::new();
    assert_view(&view);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: Key event handling
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_8_handle_key_tab_focus_next() {
    let mut view = MonitorView::new();
    let mut state = TuiState::new("test.nika.yaml");

    let tab_key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
    let action = view.handle_key(tab_key, &mut state);

    assert!(
        matches!(action, ViewAction::None),
        "Tab should return None (internal focus change)"
    );
    assert_eq!(
        view.focus,
        PanelId::RunnerDag,
        "Tab should move to next panel"
    );
}

#[test]
fn wiring_8_handle_key_shift_tab_focus_prev() {
    let mut view = MonitorView::new();
    let mut state = TuiState::new("test.nika.yaml");

    // MonitorView uses KeyCode::Tab with SHIFT modifier for backwards cycling
    let shift_tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
    let action = view.handle_key(shift_tab, &mut state);

    assert!(
        matches!(action, ViewAction::None),
        "Shift+Tab should return None"
    );
    assert_eq!(
        view.focus,
        PanelId::RunnerReasoning,
        "Shift+Tab should move to previous panel"
    );
}

#[test]
fn wiring_8_handle_key_q_returns_home() {
    let mut view = MonitorView::new();
    let mut state = TuiState::new("test.nika.yaml");

    // MonitorView returns to Home on 'q', not Quit (app-level exit)
    let q_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    let action = view.handle_key(q_key, &mut state);

    assert!(
        matches!(action, ViewAction::SwitchView(TuiView::Studio)),
        "q should return to Studio view"
    );
}

#[test]
fn wiring_8_handle_key_number_jumps_to_panel() {
    let mut view = MonitorView::new();
    let mut state = TuiState::new("test.nika.yaml");

    // Press '2' to jump to DAG panel
    let key_2 = KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE);
    let _ = view.handle_key(key_2, &mut state);
    assert_eq!(view.focus, PanelId::RunnerDag);

    // Press '3' to jump to NovaNet panel
    let key_3 = KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE);
    let _ = view.handle_key(key_3, &mut state);
    assert_eq!(view.focus, PanelId::RunnerNovanet);

    // Press '4' to jump to Agent panel
    let key_4 = KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE);
    let _ = view.handle_key(key_4, &mut state);
    assert_eq!(view.focus, PanelId::RunnerReasoning);

    // Press '1' to jump to Progress panel
    let key_1 = KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE);
    let _ = view.handle_key(key_1, &mut state);
    assert_eq!(view.focus, PanelId::RunnerMission);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 5: Scroll state per panel
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_8_scroll_state_initial_zero() {
    let view = MonitorView::new();
    assert_eq!(
        view.scroll,
        [0, 0, 0, 0],
        "All scroll offsets should start at 0"
    );
}
