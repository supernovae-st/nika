//! WIRING-10: Full TUI Navigation
//!
//! Verifies: View enum, navigation, and key bindings work together
//! Run after: v0.20.0 (6-Views Architecture Update)
//!
//! Tests validate:
//! - All 6 views construct properly (Browse, Editor, Runner, Chat, Scheduler, Settings)
//! - TuiView enum navigation (next/prev, cycling through all 6)
//! - ViewAction variants for navigation
//! - View trait polymorphism

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
#[allow(deprecated)]
use nika::tui::{
    ChatView, HelpView, HomeView, MonitorView, SettingsView, StudioView, TuiState, TuiView, View,
    ViewAction,
};
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: All views construct
// ═══════════════════════════════════════════════════════════════════════════

#[test]
#[allow(deprecated)]
fn wiring_10_home_view_constructs() {
    let _view = HomeView::new(PathBuf::from("."));
    // HomeView (ExplorerView) constructed successfully
}

#[test]
fn wiring_10_chat_view_constructs() {
    let _view = ChatView::new();
    // ChatView constructed successfully
}

#[test]
#[allow(deprecated)]
fn wiring_10_studio_view_constructs() {
    let _view = StudioView::new();
    // StudioView (EditorView) constructed successfully
}

#[test]
#[allow(deprecated)]
fn wiring_10_monitor_view_constructs() {
    let _view = MonitorView::new();
    // MonitorView (RunnerView) constructed successfully
}

#[test]
fn wiring_10_settings_view_constructs() {
    let _view = SettingsView::new();
    // SettingsView constructed successfully
}

#[test]
#[allow(deprecated)]
fn wiring_10_help_view_constructs() {
    // Help view still exists for backwards compat but is merged into Settings in v0.12
    let _view = HelpView::new();
    // HelpView constructed successfully (merged into Settings)
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: TuiView enum variants (v0.12 6-Views)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_tui_view_default_is_browse() {
    let view = TuiView::default();
    assert_eq!(view, TuiView::Browse, "Default view should be Browse");
}

#[test]
fn wiring_10_tui_view_all_six() {
    let all = TuiView::all();
    assert_eq!(all.len(), 6, "All views should be 6 (v0.20 6-Views)");
    assert_eq!(all[0], TuiView::Browse);
    assert_eq!(all[1], TuiView::Editor);
    assert_eq!(all[2], TuiView::Runner);
    assert_eq!(all[3], TuiView::Chat);
    assert_eq!(all[4], TuiView::Scheduler);
    assert_eq!(all[5], TuiView::Settings);
}

#[test]
fn wiring_10_tui_view_all_including_auxiliary() {
    let all = TuiView::all_including_auxiliary();
    assert_eq!(all.len(), 6, "All views should be 6");
    // v0.12: all_including_auxiliary() is same as all()
    assert_eq!(all, TuiView::all());
}

#[test]
fn wiring_10_tui_view_is_auxiliary() {
    // v0.12: Only Settings is auxiliary
    assert!(!TuiView::Browse.is_auxiliary());
    assert!(!TuiView::Chat.is_auxiliary());
    assert!(!TuiView::Editor.is_auxiliary());
    assert!(!TuiView::Runner.is_auxiliary());
    assert!(!TuiView::Scheduler.is_auxiliary());
    assert!(TuiView::Settings.is_auxiliary());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: TuiView navigation (cycles through all 6)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_tui_view_next_cycles_all_six() {
    // v0.20 order: Browse, Editor, Runner, Chat, Scheduler, Settings
    assert_eq!(TuiView::Browse.next(), TuiView::Editor);
    assert_eq!(TuiView::Editor.next(), TuiView::Runner);
    assert_eq!(TuiView::Runner.next(), TuiView::Chat);
    assert_eq!(TuiView::Chat.next(), TuiView::Scheduler);
    assert_eq!(TuiView::Scheduler.next(), TuiView::Settings);
    assert_eq!(
        TuiView::Settings.next(),
        TuiView::Browse,
        "Should cycle back"
    );
}

#[test]
fn wiring_10_tui_view_prev_cycles_all_six() {
    // v0.20 order: Browse, Editor, Runner, Chat, Scheduler, Settings
    assert_eq!(TuiView::Browse.prev(), TuiView::Settings);
    assert_eq!(TuiView::Editor.prev(), TuiView::Browse);
    assert_eq!(TuiView::Runner.prev(), TuiView::Editor);
    assert_eq!(TuiView::Chat.prev(), TuiView::Runner);
    assert_eq!(TuiView::Scheduler.prev(), TuiView::Chat);
    assert_eq!(TuiView::Settings.prev(), TuiView::Scheduler);
}

#[test]
fn wiring_10_tui_view_toggle_is_next() {
    assert_eq!(TuiView::Chat.toggle(), TuiView::Chat.next());
    assert_eq!(TuiView::Browse.toggle(), TuiView::Browse.next());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: TuiView metadata (v0.12 values)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_10_tui_view_numbers() {
    // v0.20 order: Browse(1), Editor(2), Runner(3), Chat(4), Scheduler(5), Settings(6)
    assert_eq!(TuiView::Browse.number(), 1);
    assert_eq!(TuiView::Editor.number(), 2);
    assert_eq!(TuiView::Runner.number(), 3);
    assert_eq!(TuiView::Chat.number(), 4);
    assert_eq!(TuiView::Scheduler.number(), 5);
    assert_eq!(TuiView::Settings.number(), 6);
}

#[test]
fn wiring_10_tui_view_titles() {
    // v0.20: Browse title changed from EXPLORER to BROWSE
    assert_eq!(TuiView::Browse.title(), "NIKA BROWSE");
    assert_eq!(TuiView::Editor.title(), "NIKA EDITOR");
    assert_eq!(TuiView::Runner.title(), "NIKA RUNNER");
    assert_eq!(TuiView::Chat.title(), "NIKA CHAT PLAYGROUND");
    assert_eq!(TuiView::Scheduler.title(), "NIKA SCHEDULER");
    assert_eq!(TuiView::Settings.title(), "NIKA SETTINGS");
}

#[test]
fn wiring_10_tui_view_icons() {
    // v0.20: Each view has a unique icon (order: Browse, Editor, Runner, Chat, Scheduler, Settings)
    assert_eq!(TuiView::Browse.icon(), "📁");
    assert_eq!(TuiView::Editor.icon(), "✏");
    assert_eq!(TuiView::Runner.icon(), "▶");
    assert_eq!(TuiView::Chat.icon(), "💬");
    assert_eq!(TuiView::Scheduler.icon(), "📅");
    assert_eq!(TuiView::Settings.icon(), "⚙");
}

#[test]
fn wiring_10_tui_view_shortcuts() {
    // v0.20: Letter shortcuts for each view (b/1, e/2, r/3, c/4, s/5, ,/6)
    assert_eq!(TuiView::Browse.shortcut(), 'b');
    assert_eq!(TuiView::Editor.shortcut(), 'e');
    assert_eq!(TuiView::Runner.shortcut(), 'r');
    assert_eq!(TuiView::Chat.shortcut(), 'c');
    assert_eq!(TuiView::Scheduler.shortcut(), 's');
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

    assert_view(&HomeView::new(PathBuf::from(".")));
    assert_view(&ChatView::new());
    assert_view(&StudioView::new());
    assert_view(&MonitorView::new());
    assert_view(&SettingsView::new());
    assert_view(&HelpView::new());
}

#[test]
#[allow(deprecated)]
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
#[allow(deprecated)]
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
#[allow(deprecated)]
fn wiring_10_home_view_switch_to_runner() {
    let mut view = HomeView::new(PathBuf::from("."));
    let mut state = TuiState::new("test.nika.yaml");

    // '3' switches to Runner view (v0.20)
    let key_3 = KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE);
    let action = view.handle_key(key_3, &mut state);

    assert!(
        matches!(action, ViewAction::SwitchView(TuiView::Runner)),
        "3 should switch to Runner"
    );
}
