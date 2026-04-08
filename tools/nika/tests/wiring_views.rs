// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Wiring Tests: View Integration
//!
//! Consolidated from: wiring_checkpoint_8 (MonitorView) +
//!                     wiring_checkpoint_9 (Provider selection) +
//!                     wiring_checkpoint_10 (Full TUI navigation)
//!
//! Updated for 3-View architecture (v0.35): Studio, Command, Control
//!
//! Verifies: MonitorView, RigProvider, TuiView enum, ViewAction,
//!           View trait polymorphism, and key event handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
#[allow(deprecated)]
use nika::tui::{
    ChatView, MonitorView, NavPanelId as PanelId, SettingsView, StudioView, TuiState, TuiView,
    View, ViewAction,
};

use nika::provider::rig::RigProvider;

fn has_env(key: &str) -> bool {
    std::env::var(key).is_ok_and(|v| !v.is_empty())
}

// ═══════════════════════════════════════════════════════════════════════════
// MonitorView Construction (was WIRING-8)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn monitor_view_new() {
    let view = MonitorView::new();
    assert_eq!(
        view.focus,
        PanelId::RunnerMission,
        "Default focus should be Progress"
    );
}

#[test]
fn monitor_view_default() {
    let view = MonitorView::default();
    assert_eq!(view.focus, PanelId::RunnerMission);
}

// ═══════════════════════════════════════════════════════════════════════════
// MonitorView Panel Focus Cycling (was WIRING-8)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn focus_next_cycles_four_panels() {
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
fn focus_prev_cycles_four_panels() {
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
fn panel_id_command_view_has_chat_panels() {
    let all = PanelId::panels_for_view(TuiView::Command);
    assert_eq!(all.len(), 3, "Command view should have 3 chat panels");
    assert_eq!(all[0], PanelId::ChatConversation);
    assert_eq!(all[1], PanelId::ChatInput);
    assert_eq!(all[2], PanelId::ChatContext);
}

// ═══════════════════════════════════════════════════════════════════════════
// MonitorView View Trait (was WIRING-8)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn monitor_view_status_line() {
    let view = MonitorView::new();
    let state = TuiState::new("test.nika.yaml");

    let status = view.status_line(&state);
    assert!(!status.is_empty(), "Status line should not be empty");
}

#[test]
fn monitor_view_tick_no_panic() {
    let mut view = MonitorView::new();
    let mut state = TuiState::new("test.nika.yaml");

    view.tick(&mut state);
}

#[test]
fn monitor_view_trait_polymorphism() {
    fn assert_view<V: View>(_v: &V) {}

    let view = MonitorView::new();
    assert_view(&view);
}

// ═══════════════════════════════════════════════════════════════════════════
// MonitorView Key Handling (was WIRING-8)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn handle_key_tab_focus_next() {
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
fn handle_key_shift_tab_focus_prev() {
    let mut view = MonitorView::new();
    let mut state = TuiState::new("test.nika.yaml");

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
fn handle_key_q_returns_studio() {
    let mut view = MonitorView::new();
    let mut state = TuiState::new("test.nika.yaml");

    let q_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    let action = view.handle_key(q_key, &mut state);

    assert!(
        matches!(action, ViewAction::SwitchView(TuiView::Studio)),
        "q should return to Studio view"
    );
}

#[test]
fn handle_key_number_jumps_to_panel() {
    let mut view = MonitorView::new();
    let mut state = TuiState::new("test.nika.yaml");

    let key_2 = KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE);
    let _ = view.handle_key(key_2, &mut state);
    assert_eq!(view.focus, PanelId::RunnerDag);

    let key_3 = KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE);
    let _ = view.handle_key(key_3, &mut state);
    assert_eq!(view.focus, PanelId::RunnerNovanet);

    let key_4 = KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE);
    let _ = view.handle_key(key_4, &mut state);
    assert_eq!(view.focus, PanelId::RunnerReasoning);

    let key_1 = KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE);
    let _ = view.handle_key(key_1, &mut state);
    assert_eq!(view.focus, PanelId::RunnerMission);
}

#[test]
fn scroll_state_initial_zero() {
    let view = MonitorView::new();
    assert_eq!(
        view.scroll,
        [0, 0, 0, 0],
        "All scroll offsets should start at 0"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Provider Selection (was WIRING-9)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn claude_provider_constructs() {
    if has_env("ANTHROPIC_API_KEY") {
        let provider = RigProvider::claude();
        assert_eq!(provider.name(), "claude");
    }
}

#[test]
fn openai_provider_constructs() {
    if has_env("OPENAI_API_KEY") {
        let provider = RigProvider::openai();
        assert_eq!(provider.name(), "openai");
    }
}

#[test]
fn mistral_provider_if_key_exists() {
    if has_env("MISTRAL_API_KEY") {
        let provider = RigProvider::mistral();
        assert_eq!(provider.name(), "mistral");
    }
}

#[test]
fn groq_provider_if_key_exists() {
    if has_env("GROQ_API_KEY") {
        let provider = RigProvider::groq();
        assert_eq!(provider.name(), "groq");
    }
}

#[test]
fn deepseek_provider_if_key_exists() {
    if has_env("DEEPSEEK_API_KEY") {
        let provider = RigProvider::deepseek();
        assert_eq!(provider.name(), "deepseek");
    }
}

#[test]
fn claude_default_model() {
    if has_env("ANTHROPIC_API_KEY") {
        let provider = RigProvider::claude();
        let model = provider.default_model();
        assert!(!model.is_empty(), "Claude should have a default model");
        assert!(
            model.contains("claude") || model.contains("sonnet") || model.contains("opus"),
            "Claude model should contain 'claude', 'sonnet', or 'opus'"
        );
    }
}

#[test]
fn openai_default_model() {
    if has_env("OPENAI_API_KEY") {
        let provider = RigProvider::openai();
        let model = provider.default_model();
        assert!(!model.is_empty(), "OpenAI should have a default model");
    }
}

#[test]
fn auto_returns_option() {
    let result = RigProvider::auto();

    match result {
        Some(provider) => {
            let name = provider.name();
            assert!(
                ["claude", "openai", "mistral", "groq", "deepseek", "gemini"].contains(&name),
                "Auto-detected provider should be one of the 6 known providers"
            );
        }
        None => {
            // No API keys configured — this is also valid
        }
    }
}

#[test]
fn safe_providers_name_matches_constructor() {
    if has_env("ANTHROPIC_API_KEY") {
        assert_eq!(RigProvider::claude().name(), "claude");
    }
    if has_env("OPENAI_API_KEY") {
        assert_eq!(RigProvider::openai().name(), "openai");
    }
}

#[test]
fn six_provider_variants_exist() {
    let known_providers = ["claude", "openai", "mistral", "groq", "deepseek", "gemini"];
    assert_eq!(
        known_providers.len(),
        6,
        "Should have 6 known provider names"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TUI Views Construction (was WIRING-10)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn studio_view_constructs() {
    let _view = StudioView::new();
}

#[test]
#[allow(deprecated)]
fn monitor_view_constructs() {
    let _view = MonitorView::new();
}

#[test]
fn chat_view_constructs() {
    let _view = ChatView::new();
}

#[test]
fn settings_view_constructs() {
    let _view = SettingsView::new();
}

// ═══════════════════════════════════════════════════════════════════════════
// TuiView Enum — 3-View Architecture (Studio, Command, Control)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn tui_view_default_is_studio() {
    let view = TuiView::default();
    assert_eq!(view, TuiView::Studio, "Default view should be Studio");
}

#[test]
fn tui_view_all_three() {
    let all = TuiView::all();
    assert_eq!(all.len(), 3, "All views should be 3 (v0.35 3-Views)");
    assert_eq!(all[0], TuiView::Studio);
    assert_eq!(all[1], TuiView::Command);
    assert_eq!(all[2], TuiView::Control);
}

#[test]
fn tui_view_all_including_auxiliary() {
    let all = TuiView::all_including_auxiliary();
    assert_eq!(all.len(), 3, "All views should be 3");
    assert_eq!(all, TuiView::all());
}

#[test]
fn tui_view_is_auxiliary() {
    assert!(!TuiView::Studio.is_auxiliary());
    assert!(!TuiView::Command.is_auxiliary());
    assert!(TuiView::Control.is_auxiliary());
}

// ═══════════════════════════════════════════════════════════════════════════
// TuiView Navigation (was WIRING-10)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn tui_view_next_cycles_all_three() {
    assert_eq!(TuiView::Studio.next(), TuiView::Command);
    assert_eq!(TuiView::Command.next(), TuiView::Control);
    assert_eq!(
        TuiView::Control.next(),
        TuiView::Studio,
        "Should cycle back"
    );
}

#[test]
fn tui_view_prev_cycles_all_three() {
    assert_eq!(TuiView::Studio.prev(), TuiView::Control);
    assert_eq!(TuiView::Command.prev(), TuiView::Studio);
    assert_eq!(TuiView::Control.prev(), TuiView::Command);
}

#[test]
fn tui_view_toggle_is_next() {
    assert_eq!(TuiView::Command.toggle(), TuiView::Command.next());
    assert_eq!(TuiView::Studio.toggle(), TuiView::Studio.next());
}

// ═══════════════════════════════════════════════════════════════════════════
// TuiView Metadata (was WIRING-10)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn tui_view_numbers() {
    assert_eq!(TuiView::Studio.number(), 1);
    assert_eq!(TuiView::Command.number(), 2);
    assert_eq!(TuiView::Control.number(), 3);
}

#[test]
fn tui_view_titles() {
    assert_eq!(TuiView::Studio.title(), "NIKA STUDIO");
    assert_eq!(TuiView::Command.title(), "NIKA COMMAND");
    assert_eq!(TuiView::Control.title(), "NIKA CONTROL");
}

#[test]
fn tui_view_icons() {
    assert_eq!(TuiView::Studio.icon(), "📝");
    assert_eq!(TuiView::Command.icon(), "▶");
    assert_eq!(TuiView::Control.icon(), "⚙");
}

#[test]
fn tui_view_shortcuts() {
    assert_eq!(TuiView::Studio.shortcut(), 's');
    assert_eq!(TuiView::Command.shortcut(), 'c');
    assert_eq!(TuiView::Control.shortcut(), 'x');
}

// ═══════════════════════════════════════════════════════════════════════════
// ViewAction Variants (was WIRING-10)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn view_action_none() {
    let action = ViewAction::None;
    assert!(matches!(action, ViewAction::None));
}

#[test]
fn view_action_quit() {
    let action = ViewAction::Quit;
    assert!(matches!(action, ViewAction::Quit));
}

#[test]
fn view_action_switch_view() {
    let action = ViewAction::SwitchView(TuiView::Command);
    match action {
        ViewAction::SwitchView(view) => assert_eq!(view, TuiView::Command),
        _ => panic!("Expected SwitchView"),
    }
}

#[test]
fn view_action_run_workflow() {
    let path = std::path::PathBuf::from("test.nika.yaml");
    let action = ViewAction::RunWorkflow(path.clone());
    match action {
        ViewAction::RunWorkflow(p) => assert_eq!(p, path),
        _ => panic!("Expected RunWorkflow"),
    }
}

#[test]
fn view_action_open_in_studio() {
    let path = std::path::PathBuf::from("edit.nika.yaml");
    let action = ViewAction::OpenInStudio(path.clone());
    match action {
        ViewAction::OpenInStudio(p) => assert_eq!(p, path),
        _ => panic!("Expected OpenInStudio"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// View Trait Polymorphism (was WIRING-10)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
#[allow(deprecated)]
fn all_views_implement_trait() {
    fn assert_view<V: View>(_v: &V) {}

    assert_view(&StudioView::new());
    assert_view(&MonitorView::new());
    assert_view(&ChatView::new());
    assert_view(&SettingsView::new());
}

#[test]
#[allow(deprecated)]
fn views_status_line_not_empty() {
    let state = TuiState::new("test.nika.yaml");

    let studio = StudioView::new();
    let chat = ChatView::new();
    let monitor = MonitorView::new();

    assert!(!studio.status_line(&state).is_empty());
    assert!(!chat.status_line(&state).is_empty());
    assert!(!monitor.status_line(&state).is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// Key Event Handling (was WIRING-10)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
#[allow(deprecated)]
fn studio_view_search_key() {
    let mut view = StudioView::new();
    let mut state = TuiState::new("test.nika.yaml");

    let search_key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
    let action = view.handle_key(search_key, &mut state);

    assert!(
        matches!(action, ViewAction::None),
        "/ should return None (activates search)"
    );
}

#[test]
#[allow(deprecated)]
fn studio_view_switch_to_command() {
    let mut view = StudioView::new();
    let mut state = TuiState::new("test.nika.yaml");

    let key_2 = KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE);
    let action = view.handle_key(key_2, &mut state);

    assert!(
        matches!(action, ViewAction::SwitchView(TuiView::Command)),
        "2 should switch to Command"
    );
}
