//! WIRING-10: Full TUI Navigation
//!
//! Verifies: View enum, navigation, and key bindings work together
//! Run after: v0.12.0 (6-Views complete)
//!
//! Tests validate:
//! - All 6 views construct properly (Chat, Home, Studio, Monitor, Settings, Help)
//! - TuiView enum navigation (next/prev, cycling)
//! - ViewAction variants for navigation
//! - View trait polymorphism

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika::tui::{
    ChatView, HelpView, HomeView, MonitorView, SettingsView, StudioView, TuiState, TuiView, View,
    ViewAction,
};
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: All views construct
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_home_view_constructs() {
    let _view = HomeView::new(PathBuf::from("."));
    assert!(true, "HomeView should construct");
}

#[test]
fn wiring_10_chat_view_constructs() {
    let _view = ChatView::new();
    assert!(true, "ChatView should construct");
}

#[test]
fn wiring_10_studio_view_constructs() {
    let _view = StudioView::new();
    assert!(true, "StudioView should construct");
}

#[test]
fn wiring_10_monitor_view_constructs() {
    let _view = MonitorView::new();
    assert!(true, "MonitorView should construct");
}

#[test]
fn wiring_10_settings_view_constructs() {
    let _view = SettingsView::new();
    assert!(true, "SettingsView should construct");
}

#[test]
fn wiring_10_help_view_constructs() {
    let _view = HelpView::new();
    assert!(true, "HelpView should construct");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: TuiView enum variants
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_tui_view_default_is_home() {
    let view = TuiView::default();
    assert_eq!(view, TuiView::Home, "Default view should be Home");
}

#[test]
fn wiring_10_tui_view_all_main_four() {
    let all = TuiView::all();
    assert_eq!(all.len(), 4, "Main views should be 4");
    assert_eq!(all[0], TuiView::Chat);
    assert_eq!(all[1], TuiView::Home);
    assert_eq!(all[2], TuiView::Studio);
    assert_eq!(all[3], TuiView::Monitor);
}

#[test]
fn wiring_10_tui_view_all_including_auxiliary() {
    let all = TuiView::all_including_auxiliary();
    assert_eq!(all.len(), 6, "All views should be 6");
    assert_eq!(all[4], TuiView::Settings);
    assert_eq!(all[5], TuiView::Help);
}

#[test]
fn wiring_10_tui_view_is_auxiliary() {
    assert!(!TuiView::Chat.is_auxiliary());
    assert!(!TuiView::Home.is_auxiliary());
    assert!(!TuiView::Studio.is_auxiliary());
    assert!(!TuiView::Monitor.is_auxiliary());
    assert!(TuiView::Settings.is_auxiliary());
    assert!(TuiView::Help.is_auxiliary());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: TuiView navigation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_tui_view_next_cycles_main_four() {
    assert_eq!(TuiView::Chat.next(), TuiView::Home);
    assert_eq!(TuiView::Home.next(), TuiView::Studio);
    assert_eq!(TuiView::Studio.next(), TuiView::Monitor);
    assert_eq!(TuiView::Monitor.next(), TuiView::Chat, "Should cycle back");
}

#[test]
fn wiring_10_tui_view_prev_cycles_main_four() {
    assert_eq!(TuiView::Chat.prev(), TuiView::Monitor);
    assert_eq!(TuiView::Home.prev(), TuiView::Chat);
    assert_eq!(TuiView::Studio.prev(), TuiView::Home);
    assert_eq!(TuiView::Monitor.prev(), TuiView::Studio);
}

#[test]
fn wiring_10_auxiliary_views_return_to_home() {
    // Auxiliary views (Settings, Help) return to Home on next/prev
    assert_eq!(TuiView::Settings.next(), TuiView::Home);
    assert_eq!(TuiView::Settings.prev(), TuiView::Home);
    assert_eq!(TuiView::Help.next(), TuiView::Home);
    assert_eq!(TuiView::Help.prev(), TuiView::Home);
}

#[test]
fn wiring_10_tui_view_toggle_is_next() {
    assert_eq!(TuiView::Chat.toggle(), TuiView::Chat.next());
    assert_eq!(TuiView::Home.toggle(), TuiView::Home.next());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: TuiView metadata
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_tui_view_numbers() {
    assert_eq!(TuiView::Chat.number(), 1);
    assert_eq!(TuiView::Home.number(), 2);
    assert_eq!(TuiView::Studio.number(), 3);
    assert_eq!(TuiView::Monitor.number(), 4);
    assert_eq!(TuiView::Settings.number(), 5);
    assert_eq!(TuiView::Help.number(), 6);
}

#[test]
fn wiring_10_tui_view_titles() {
    assert_eq!(TuiView::Chat.title(), "NIKA AGENT");
    assert_eq!(TuiView::Home.title(), "NIKA HOME");
    assert_eq!(TuiView::Studio.title(), "NIKA STUDIO");
    assert_eq!(TuiView::Monitor.title(), "NIKA MONITOR");
    assert_eq!(TuiView::Settings.title(), "NIKA SETTINGS");
    assert_eq!(TuiView::Help.title(), "NIKA HELP");
}

#[test]
fn wiring_10_tui_view_icons() {
    // All main views use diamond
    assert_eq!(TuiView::Chat.icon(), "◆");
    assert_eq!(TuiView::Home.icon(), "◆");
    assert_eq!(TuiView::Studio.icon(), "◆");
    assert_eq!(TuiView::Monitor.icon(), "◆");
    // Auxiliary views have special icons
    assert_eq!(TuiView::Settings.icon(), "⚙");
    assert_eq!(TuiView::Help.icon(), "?");
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
fn wiring_10_views_implement_trait() {
    fn assert_view<V: View>(_v: &V) {}

    assert_view(&HomeView::new(PathBuf::from(".")));
    assert_view(&ChatView::new());
    assert_view(&StudioView::new());
    assert_view(&MonitorView::new());
    assert_view(&SettingsView::new());
    assert_view(&HelpView::new());
}

#[test]
fn wiring_10_views_status_line_not_empty() {
    let state = TuiState::new("test.nika.yaml");

    let home = HomeView::new(PathBuf::from("."));
    let chat = ChatView::new();
    let monitor = MonitorView::new();

    assert!(!home.status_line(&state).is_empty());
    assert!(!chat.status_line(&state).is_empty());
    assert!(!monitor.status_line(&state).is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 7: Key event handling basics
// ═══════════════════════════════════════════════════════════════════════════

// NOTE: Quit handling (q, Ctrl+C) is at App level, not View level (v0.8.1+)
// Views return ViewAction::None for unhandled keys, App intercepts quit keys

#[test]
fn wiring_10_home_view_search_key() {
    let mut view = HomeView::new(PathBuf::from("."));
    let mut state = TuiState::new("test.nika.yaml");

    // '/' activates search mode
    let search_key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
    let action = view.handle_key(search_key, &mut state);

    assert!(
        matches!(action, ViewAction::None),
        "/ should return None (activates search)"
    );
}

#[test]
fn wiring_10_home_view_switch_to_studio() {
    let mut view = HomeView::new(PathBuf::from("."));
    let mut state = TuiState::new("test.nika.yaml");

    // '3' switches to Studio view
    let key_3 = KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE);
    let action = view.handle_key(key_3, &mut state);

    assert!(
        matches!(action, ViewAction::SwitchView(TuiView::Studio)),
        "3 should switch to Studio"
    );
}
