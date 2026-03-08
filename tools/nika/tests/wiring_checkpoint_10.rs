//! WIRING-10: Full TUI Navigation
//!
//! Verifies: View enum, navigation, and key bindings work together
//! Run after: v0.22.0 (4-Views Architecture Update)
//!
//! Tests validate:
//! - All 4 views construct properly (Studio, Runner, Chat, Settings)
//! - TuiView enum navigation (next/prev, cycling through all 4)
//! - ViewAction variants for navigation
//! - View trait polymorphism

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
#[allow(deprecated)]
use nika::tui::{
    ChatView, HelpView, MonitorView, SettingsView, StudioView, TuiState, TuiView, View, ViewAction,
};

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: All views construct (v0.22 4-Views)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_studio_view_constructs() {
    let _view = StudioView::new();
    // StudioView constructed successfully (default view in v0.21)
}

#[test]
#[allow(deprecated)]
fn wiring_10_runner_view_constructs() {
    let _view = MonitorView::new();
    // MonitorView (RunnerView) constructed successfully
}

#[test]
fn wiring_10_chat_view_constructs() {
    let _view = ChatView::new();
    // ChatView constructed successfully
}

#[test]
fn wiring_10_settings_view_constructs() {
    let _view = SettingsView::new();
    // SettingsView constructed successfully
}

#[test]
#[allow(deprecated)]
fn wiring_10_help_view_constructs() {
    // Help view still exists for backwards compat but is merged into Settings
    let _view = HelpView::new();
    // HelpView constructed successfully (merged into Settings)
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: TuiView enum variants (v0.22 4-Views)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_tui_view_default_is_studio() {
    let view = TuiView::default();
    assert_eq!(view, TuiView::Studio, "Default view should be Studio");
}

#[test]
fn wiring_10_tui_view_all_four() {
    let all = TuiView::all();
    assert_eq!(all.len(), 4, "All views should be 4 (v0.22 4-Views)");
    assert_eq!(all[0], TuiView::Studio);
    assert_eq!(all[1], TuiView::Runner);
    assert_eq!(all[2], TuiView::Chat);
    assert_eq!(all[3], TuiView::Settings);
}

#[test]
fn wiring_10_tui_view_all_including_auxiliary() {
    let all = TuiView::all_including_auxiliary();
    assert_eq!(all.len(), 4, "All views should be 4");
    // v0.22: all_including_auxiliary() is same as all()
    assert_eq!(all, TuiView::all());
}

#[test]
fn wiring_10_tui_view_is_auxiliary() {
    // v0.22: Only Settings is auxiliary
    assert!(!TuiView::Studio.is_auxiliary());
    assert!(!TuiView::Runner.is_auxiliary());
    assert!(!TuiView::Chat.is_auxiliary());
    assert!(TuiView::Settings.is_auxiliary());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: TuiView navigation (cycles through all 4)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_tui_view_next_cycles_all_four() {
    // v0.22 order: Studio, Runner, Chat, Settings (4-view architecture)
    assert_eq!(TuiView::Studio.next(), TuiView::Runner);
    assert_eq!(TuiView::Runner.next(), TuiView::Chat);
    assert_eq!(TuiView::Chat.next(), TuiView::Settings);
    assert_eq!(
        TuiView::Settings.next(),
        TuiView::Studio,
        "Should cycle back"
    );
}

#[test]
fn wiring_10_tui_view_prev_cycles_all_four() {
    // v0.22 order: Studio, Runner, Chat, Settings (4-view architecture)
    assert_eq!(TuiView::Studio.prev(), TuiView::Settings);
    assert_eq!(TuiView::Runner.prev(), TuiView::Studio);
    assert_eq!(TuiView::Chat.prev(), TuiView::Runner);
    assert_eq!(TuiView::Settings.prev(), TuiView::Chat);
}

#[test]
fn wiring_10_tui_view_toggle_is_next() {
    assert_eq!(TuiView::Chat.toggle(), TuiView::Chat.next());
    assert_eq!(TuiView::Studio.toggle(), TuiView::Studio.next());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: TuiView metadata (v0.22 values - 4-view architecture)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_tui_view_numbers() {
    // v0.22 order: Studio(1), Runner(2), Chat(3), Settings(4)
    assert_eq!(TuiView::Studio.number(), 1);
    assert_eq!(TuiView::Runner.number(), 2);
    assert_eq!(TuiView::Chat.number(), 3);
    assert_eq!(TuiView::Settings.number(), 4);
}

#[test]
fn wiring_10_tui_view_titles() {
    // v0.22: Studio is default view
    assert_eq!(TuiView::Studio.title(), "NIKA STUDIO");
    assert_eq!(TuiView::Runner.title(), "NIKA RUNNER");
    assert_eq!(TuiView::Chat.title(), "NIKA CHAT");
    assert_eq!(TuiView::Settings.title(), "NIKA SETTINGS");
}

#[test]
fn wiring_10_tui_view_icons() {
    // v0.22: Each view has a unique icon (order: Studio, Runner, Chat, Settings)
    assert_eq!(TuiView::Studio.icon(), "📝");
    assert_eq!(TuiView::Runner.icon(), "▶");
    assert_eq!(TuiView::Chat.icon(), "💬");
    assert_eq!(TuiView::Settings.icon(), "⚙");
}

#[test]
fn wiring_10_tui_view_shortcuts() {
    // v0.22: Letter shortcuts for each view (s/1, r/2, c/3, ,/4)
    assert_eq!(TuiView::Studio.shortcut(), 's');
    assert_eq!(TuiView::Runner.shortcut(), 'r');
    assert_eq!(TuiView::Chat.shortcut(), 'c');
    assert_eq!(TuiView::Settings.shortcut(), ',');
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 5: ViewAction variants
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_view_action_none() {
    let action = ViewAction::None;
    assert!(matches!(action, ViewAction::None));
}

#[test]
fn wiring_10_view_action_quit() {
    let action = ViewAction::Quit;
    assert!(matches!(action, ViewAction::Quit));
}

#[test]
fn wiring_10_view_action_switch_view() {
    let action = ViewAction::SwitchView(TuiView::Chat);
    match action {
        ViewAction::SwitchView(view) => assert_eq!(view, TuiView::Chat),
        _ => panic!("Expected SwitchView"),
    }
}

#[test]
fn wiring_10_view_action_run_workflow() {
    let path = std::path::PathBuf::from("test.nika.yaml");
    let action = ViewAction::RunWorkflow(path.clone());
    match action {
        ViewAction::RunWorkflow(p) => assert_eq!(p, path),
        _ => panic!("Expected RunWorkflow"),
    }
}

#[test]
fn wiring_10_view_action_open_in_studio() {
    let path = std::path::PathBuf::from("edit.nika.yaml");
    let action = ViewAction::OpenInStudio(path.clone());
    match action {
        ViewAction::OpenInStudio(p) => assert_eq!(p, path),
        _ => panic!("Expected OpenInStudio"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 6: View trait polymorphism
// ═══════════════════════════════════════════════════════════════════════════

#[test]
#[allow(deprecated)]
fn wiring_10_views_implement_trait() {
    fn assert_view<V: View>(_v: &V) {}

    // v0.22 4-Views: Studio, Runner, Chat, Settings
    assert_view(&StudioView::new());
    assert_view(&MonitorView::new()); // RunnerView (deprecated alias)
    assert_view(&ChatView::new());
    assert_view(&SettingsView::new());
    assert_view(&HelpView::new()); // Deprecated, merged into Settings
}

#[test]
#[allow(deprecated)]
fn wiring_10_views_status_line_not_empty() {
    let state = TuiState::new("test.nika.yaml");

    // v0.22 4-Views: Studio, Runner (MonitorView), Chat, Settings
    let studio = StudioView::new();
    let chat = ChatView::new();
    let monitor = MonitorView::new();

    assert!(!studio.status_line(&state).is_empty());
    assert!(!chat.status_line(&state).is_empty());
    assert!(!monitor.status_line(&state).is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 7: Key event handling basics
// ═══════════════════════════════════════════════════════════════════════════

// NOTE: Quit handling (q, Ctrl+C) is at App level, not View level (v0.8.1+)
// Views return ViewAction::None for unhandled keys, App intercepts quit keys

#[test]
#[allow(deprecated)]
fn wiring_10_studio_view_search_key() {
    let mut view = StudioView::new();
    let mut state = TuiState::new("test.nika.yaml");

    // '/' activates search mode (v0.21: Studio is default view)
    let search_key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
    let action = view.handle_key(search_key, &mut state);

    assert!(
        matches!(action, ViewAction::None),
        "/ should return None (activates search)"
    );
}

#[test]
#[allow(deprecated)]
fn wiring_10_studio_view_switch_to_runner() {
    let mut view = StudioView::new();
    let mut state = TuiState::new("test.nika.yaml");

    // '2' switches to Runner view (v0.21 5-Views: Studio=1, Runner=2, Chat=3, Scheduler=4, Settings=5)
    let key_2 = KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE);
    let action = view.handle_key(key_2, &mut state);

    assert!(
        matches!(action, ViewAction::SwitchView(TuiView::Runner)),
        "2 should switch to Runner"
    );
}
