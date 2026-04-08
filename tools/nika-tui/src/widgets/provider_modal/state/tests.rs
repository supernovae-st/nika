// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tests for provider modal state
#![allow(clippy::field_reassign_with_default)]

use super::*;

#[test]
fn test_tab_default_is_cloud() {
    assert_eq!(ProviderModalTab::default(), ProviderModalTab::Cloud);
}

#[test]
fn test_tab_next_cycles() {
    assert_eq!(ProviderModalTab::Cloud.next(), ProviderModalTab::Native);
    assert_eq!(ProviderModalTab::Native.next(), ProviderModalTab::Keys);
    assert_eq!(ProviderModalTab::Keys.next(), ProviderModalTab::Config);
    assert_eq!(ProviderModalTab::Config.next(), ProviderModalTab::Cloud);
}

#[test]
fn test_tab_prev_cycles() {
    assert_eq!(ProviderModalTab::Cloud.prev(), ProviderModalTab::Config);
    assert_eq!(ProviderModalTab::Config.prev(), ProviderModalTab::Keys);
}

#[test]
fn test_tab_from_key() {
    assert_eq!(
        ProviderModalTab::from_key('1'),
        Some(ProviderModalTab::Cloud)
    );
    assert_eq!(
        ProviderModalTab::from_key('2'),
        Some(ProviderModalTab::Native)
    );
    assert_eq!(
        ProviderModalTab::from_key('3'),
        Some(ProviderModalTab::Keys)
    );
    assert_eq!(
        ProviderModalTab::from_key('4'),
        Some(ProviderModalTab::Config)
    );
    assert_eq!(ProviderModalTab::from_key('x'), None);
}

#[test]
fn test_tab_label() {
    assert_eq!(ProviderModalTab::Cloud.label(), "☁️  CLOUD");
    assert_eq!(ProviderModalTab::Native.label(), "🦙 NATIVE");
}

#[test]
fn test_connection_status_display() {
    let connected = ConnectionStatus::Connected { latency_ms: 182 };
    assert_eq!(connected.display_text(), "● 182ms");

    let checking = ConnectionStatus::Checking;
    assert_eq!(checking.display_text(), "⠹ Checking...");

    let failed = ConnectionStatus::Failed {
        error: "Timeout".into(),
    };
    assert_eq!(failed.display_text(), "✗ Timeout");

    let not_configured = ConnectionStatus::NotConfigured;
    assert_eq!(not_configured.display_text(), "○ Not configured");
}

#[test]
fn test_connection_status_is_available() {
    assert!(ConnectionStatus::Connected { latency_ms: 100 }.is_available());
    assert!(!ConnectionStatus::Checking.is_available());
    assert!(!ConnectionStatus::Failed { error: "".into() }.is_available());
    assert!(!ConnectionStatus::NotConfigured.is_available());
}

#[test]
fn test_api_key_masking() {
    let key = "sk-ant-api03-abc123xyz789def456ghi";
    let masked = ApiKeyState::mask_key(key);
    assert_eq!(masked, "sk-ant...i");
}

#[test]
fn test_api_key_masking_short_key() {
    let key = "short";
    let masked = ApiKeyState::mask_key(key);
    assert_eq!(masked, "****");
}

#[test]
fn test_api_key_state_display() {
    let not_configured = ApiKeyState::NotConfigured;
    assert_eq!(not_configured.status_icon(), "⚠");

    let configured = ApiKeyState::Configured {
        masked: "sk-...xyz".into(),
    };
    assert_eq!(configured.status_icon(), "✓");

    let invalid = ApiKeyState::Invalid {
        masked: "sk-...xyz".into(),
        error: "Bad".into(),
    };
    assert_eq!(invalid.status_icon(), "✗");
}

#[test]
fn test_download_progress_percentage() {
    let state = DownloadState::Downloading {
        model: "llama3.2".into(),
        progress: 0.45,
        downloaded: 2_100_000_000,
        total: 4_700_000_000,
    };
    assert_eq!(state.percentage(), 45);
}

#[test]
fn test_download_state_is_active() {
    assert!(!DownloadState::Idle.is_active());
    assert!(DownloadState::Downloading {
        model: "".into(),
        progress: 0.0,
        downloaded: 0,
        total: 0
    }
    .is_active());
    assert!(!DownloadState::Complete { model: "".into() }.is_active());
    assert!(!DownloadState::Failed {
        model: "".into(),
        error: "".into()
    }
    .is_active());
}

#[test]
fn test_download_format_bytes() {
    assert_eq!(DownloadState::format_bytes(4_700_000_000), "4.7 GB");
    assert_eq!(DownloadState::format_bytes(500_000_000), "500.0 MB");
    assert_eq!(DownloadState::format_bytes(1_500), "1.5 KB");
    assert_eq!(DownloadState::format_bytes(500), "500 B");
}

#[test]
fn test_modal_state_default() {
    let state = ProviderModalState::default();
    assert!(!state.visible);
    assert_eq!(state.active_tab, ProviderModalTab::Cloud);
    assert_eq!(state.selected_idx, 0);
}

#[test]
fn test_modal_toggle_visibility() {
    let mut state = ProviderModalState::default();
    assert!(!state.visible);

    state.toggle();
    assert!(state.visible);

    state.toggle();
    assert!(!state.visible);
}

#[test]
fn test_modal_open_close() {
    let mut state = ProviderModalState::default();
    state.open();
    assert!(state.visible);
    state.close();
    assert!(!state.visible);
}

#[test]
fn test_modal_tab_switch() {
    let mut state = ProviderModalState::default();
    state.selected_idx = 3;
    state.switch_tab(ProviderModalTab::Native);

    assert_eq!(state.active_tab, ProviderModalTab::Native);
    assert_eq!(state.selected_idx, 0); // Reset on tab switch
}

#[test]
fn test_modal_navigate_list_mode_wrapping() {
    // Use Keys tab for list navigation test (not grid)
    let mut state = ProviderModalState::default();
    state.switch_tab(ProviderModalTab::Keys); // Keys tab uses list navigation
    state.item_count = 5;

    assert_eq!(state.selected_idx, 0);

    state.navigate_down();
    assert_eq!(state.selected_idx, 1);

    state.navigate_down();
    state.navigate_down();
    state.navigate_down();
    assert_eq!(state.selected_idx, 4);

    // Wraps to first
    state.navigate_down();
    assert_eq!(state.selected_idx, 0);

    // Wraps to last when at first and going up
    state.navigate_up();
    assert_eq!(state.selected_idx, 4);

    state.navigate_up();
    assert_eq!(state.selected_idx, 3);
}

#[test]
fn test_modal_navigate_grid_mode_wrapping() {
    // Cloud tab (default) uses 3-column grid navigation with wrapping (7 providers)
    let mut state = ProviderModalState::default();
    // Default is Cloud tab with item_count = 7

    // Navigation uses 3-column grid layout:
    //   0 1 2  (row 0: Claude, OpenAI, Mistral)
    //   3 4 5  (row 1: Groq, DeepSeek, Gemini)
    //   6      (row 2: xAI)

    assert_eq!(state.selected_idx, 0);
    assert_eq!(state.active_tab, ProviderModalTab::Cloud);

    // Navigate right: 0 -> 1 -> 2 -> wraps to 0
    state.navigate_right();
    assert_eq!(state.selected_idx, 1);
    state.navigate_right();
    assert_eq!(state.selected_idx, 2);
    state.navigate_right();
    assert_eq!(state.selected_idx, 0); // Wraps to start of row

    // Navigate down: 0 -> 3 -> 6 -> wraps to 0
    state.navigate_down();
    assert_eq!(state.selected_idx, 3);
    state.navigate_down();
    assert_eq!(state.selected_idx, 6);
    state.navigate_down();
    assert_eq!(state.selected_idx, 0); // Wraps: col 0, row 0

    // Navigate to position 2, then down wraps to same column
    state.selected_idx = 2;
    state.navigate_down();
    assert_eq!(state.selected_idx, 5);
    state.navigate_down();
    assert_eq!(state.selected_idx, 2); // Wraps: 5+3=8 >= 7, col=5%3=2

    // Navigate left wrapping: 3 -> wraps to 5
    state.selected_idx = 3;
    state.navigate_left();
    assert_eq!(state.selected_idx, 5); // Wraps to end of row

    // Navigate up wrapping: 0 -> wraps to 6 (xAI, last item in col 0)
    state.selected_idx = 0;
    state.navigate_up();
    assert_eq!(state.selected_idx, 6); // col 0 last is xAI (row 2)

    // Navigate up wrapping: 1 -> wraps to 4 (partial row 2 has no col 1, so row 1)
    state.selected_idx = 1;
    state.navigate_up();
    assert_eq!(state.selected_idx, 4); // col 1 last is DeepSeek (row 1)

    // Navigate up wrapping: 2 -> wraps to 5 (partial row 2 has no col 2, so row 1)
    state.selected_idx = 2;
    state.navigate_up();
    assert_eq!(state.selected_idx, 5); // col 2 last is Gemini (row 1)
}

// SEC-004: Debug redacts key_input_buffer
#[test]
fn test_modal_state_debug_redacts_key_buffer() {
    let mut state = ProviderModalState::default();
    state.key_input_buffer = "sk-ant-secret-key-12345".to_string();

    let debug_output = format!("{:?}", state);

    // API key should NOT appear in debug output
    assert!(!debug_output.contains("sk-ant"));
    assert!(!debug_output.contains("secret"));
    // Should show [REDACTED] instead
    assert!(debug_output.contains("[REDACTED]"));
}

#[test]
fn test_modal_close_clears_input() {
    let mut state = ProviderModalState::default();
    state.open();
    state.key_input_mode = true;
    state.key_input_buffer = "sk-ant-test".to_string();

    state.close();

    assert!(!state.visible);
    assert!(!state.key_input_mode);
    assert!(state.key_input_buffer.is_empty());
}

#[test]
fn test_set_provider_status_by_index() {
    let mut state = ProviderModalState::default();
    state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 100 });
    state.set_provider_status(
        1,
        ConnectionStatus::Failed {
            error: "No key".into(),
        },
    );

    assert_eq!(state.provider_statuses.len(), 2);
    assert!(matches!(
        state.provider_statuses[0],
        ConnectionStatus::Connected { latency_ms: 100 }
    ));
}

#[test]
fn test_set_provider_status_by_name() {
    let mut state = ProviderModalState::default();
    state.set_provider_status_by_name("anthropic", ConnectionStatus::Connected { latency_ms: 150 });
    state.set_provider_status_by_name("openai", ConnectionStatus::Checking);
    state.set_provider_status_by_name(
        "claude",
        ConnectionStatus::Failed {
            error: "Updated".into(),
        },
    ); // Same as anthropic (index 0)

    assert!(matches!(
        state.provider_statuses[0],
        ConnectionStatus::Failed { .. }
    ));
    assert!(matches!(
        state.provider_statuses[1],
        ConnectionStatus::Checking
    ));
}

#[test]
fn test_set_provider_status_by_name_all_providers() {
    let mut state = ProviderModalState::default();
    state.set_provider_status_by_name("anthropic", ConnectionStatus::Connected { latency_ms: 1 });
    state.set_provider_status_by_name("openai", ConnectionStatus::Connected { latency_ms: 2 });
    state.set_provider_status_by_name("mistral", ConnectionStatus::Connected { latency_ms: 3 });
    state.set_provider_status_by_name("groq", ConnectionStatus::Connected { latency_ms: 4 });
    state.set_provider_status_by_name("deepseek", ConnectionStatus::Connected { latency_ms: 5 });
    state.set_provider_status_by_name("gemini", ConnectionStatus::Connected { latency_ms: 6 });
    state.set_provider_status_by_name("xai", ConnectionStatus::Connected { latency_ms: 7 });
    // Native is not a cloud provider, handled separately

    assert_eq!(state.provider_statuses.len(), 7);
}

#[test]
fn test_get_provider_statuses_returns_6() {
    let state = ProviderModalState::default();
    let statuses = state.get_provider_statuses();
    assert_eq!(statuses.len(), 7); // 7 cloud providers
                                   // All should be Unknown by default
    assert!(statuses
        .iter()
        .all(|s| matches!(s, ConnectionStatus::Unknown)));
}

#[test]
fn test_get_provider_statuses_with_partial_data() {
    let mut state = ProviderModalState::default();
    state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 100 });
    state.set_provider_status(2, ConnectionStatus::Checking);

    let statuses = state.get_provider_statuses();
    assert_eq!(statuses.len(), 7); // 7 cloud providers
    assert!(matches!(statuses[0], ConnectionStatus::Connected { .. }));
    assert!(matches!(statuses[1], ConnectionStatus::Unknown));
    assert!(matches!(statuses[2], ConnectionStatus::Checking));
}

#[test]
fn test_set_native_models() {
    let mut state = ProviderModalState::default();
    let models = vec![NativeModelInfo {
        name: "llama3.2".to_string(),
        size: 4_700_000_000,
        digest: "sha256:abc".to_string(),
        modified_at: "2026-02-24".to_string(),
        details: NativeModelDetails {
            parameter_size: "8B".to_string(),
            quantization_level: "Q4_0".to_string(),
            family: Some("llama".to_string()),
        },
    }];

    state.set_native_models(models);
    assert_eq!(state.native_models.len(), 1);
    assert_eq!(state.native_models[0].name, "llama3.2");
}

#[test]
fn test_modal_state_default_has_empty_statuses() {
    let state = ProviderModalState::default();
    assert!(state.provider_statuses.is_empty());
    assert!(state.native_models.is_empty());
}

#[test]
fn test_process_loader_event_provider_status() {
    use super::super::loader::LoaderEvent;

    let mut state = ProviderModalState::default();
    state.process_loader_event(LoaderEvent::ProviderStatus {
        provider: "anthropic",
        status: ConnectionStatus::Connected { latency_ms: 120 },
    });

    let statuses = state.get_provider_statuses();
    assert!(matches!(
        statuses[0],
        ConnectionStatus::Connected { latency_ms: 120 }
    ));
}

#[test]
fn test_process_loader_event_native_available() {
    use super::super::loader::LoaderEvent;

    let mut state = ProviderModalState::default();
    assert!(!state.native_available);

    state.process_loader_event(LoaderEvent::NativeAvailable(true));
    assert!(state.native_available);

    state.process_loader_event(LoaderEvent::NativeAvailable(false));
    assert!(!state.native_available);
}

#[test]
fn test_process_loader_event_native_models() {
    use super::super::loader::LoaderEvent;

    let mut state = ProviderModalState::default();
    let models = vec![NativeModelInfo {
        name: "llama3.2".to_string(),
        size: 4_700_000_000,
        digest: "sha256:abc".to_string(),
        modified_at: "2026-02-24".to_string(),
        details: NativeModelDetails {
            parameter_size: "8B".to_string(),
            quantization_level: "Q4_0".to_string(),
            family: Some("llama".to_string()),
        },
    }];

    state.process_loader_event(LoaderEvent::NativeModels(models));
    assert_eq!(state.native_models.len(), 1);
}

#[test]
fn test_process_loader_event_providers_complete() {
    use super::super::loader::LoaderEvent;

    let mut state = ProviderModalState::default();
    // Should not panic
    state.process_loader_event(LoaderEvent::ProvidersComplete);
}

#[test]
fn test_process_loader_event_error() {
    use super::super::loader::LoaderEvent;

    let mut state = ProviderModalState::default();
    // Should not panic, just log
    state.process_loader_event(LoaderEvent::Error {
        source: "native".to_string(),
        message: "Connection refused".to_string(),
    });
}

#[test]
fn test_active_model_and_tab_label() {
    let mut state = ProviderModalState::default();
    assert!(state.active_model.is_none());
    assert_eq!(state.cloud_tab_label(), "☁️  CLOUD");

    state.set_active_model("claude-sonnet-4-6");
    assert_eq!(state.active_model, Some("claude-sonnet-4-6".to_string()));
    assert_eq!(state.cloud_tab_label(), "☁️  CLOUD [claude-sonnet-4-6]");
}

#[test]
fn test_active_model_long_name_truncated() {
    let mut state = ProviderModalState::default();
    state.set_active_model("claude-3-5-sonnet-latest-version-2025");
    // Should truncate to 17 chars + "..." (threshold is >20)
    let label = state.cloud_tab_label();
    assert!(label.contains("..."));
    assert!(label.len() < 50);
}

#[test]
fn test_animation_frame_cycles() {
    let mut state = ProviderModalState::default();
    assert_eq!(state.animation_frame, 0);

    state.tick_animation();
    assert_eq!(state.animation_frame, 1);

    // Should cycle through indicators
    for _ in 0..100 {
        state.tick_animation();
    }
    // Should not panic and indicator should be valid
    let indicator = state.active_indicator();
    assert!(!indicator.is_empty());
}

#[test]
fn test_active_indicator_returns_valid_chars() {
    let mut state = ProviderModalState::default();
    let valid_chars = ["★", "✦", "●", "◆", "✧", "◉", "✴", "❋"];

    for _ in 0..32 {
        let indicator = state.active_indicator();
        assert!(valid_chars.contains(&indicator));
        state.tick_animation();
    }
}

#[test]
fn test_native_tab_label_empty() {
    let state = ProviderModalState::default();
    assert_eq!(state.native_tab_label(), "🦙 NATIVE");
}

#[test]
fn test_native_tab_label_with_models() {
    let mut state = ProviderModalState::default();
    let details = NativeModelDetails {
        parameter_size: "7B".to_string(),
        quantization_level: "Q4_0".to_string(),
        family: Some("llama".to_string()),
    };
    state.native_models = vec![
        NativeModelInfo {
            name: "llama3.2".to_string(),
            size: 4_200_000_000,
            digest: "sha256:abc123".to_string(),
            modified_at: "2024-01-01".to_string(),
            details: details.clone(),
        },
        NativeModelInfo {
            name: "codellama".to_string(),
            size: 3_800_000_000,
            digest: "sha256:def456".to_string(),
            modified_at: "2024-01-01".to_string(),
            details,
        },
    ];
    assert_eq!(state.native_tab_label(), "🦙 NATIVE (2)");
}

#[test]
fn test_keys_tab_label_empty() {
    let state = ProviderModalState::default();
    assert_eq!(state.keys_tab_label(), "🔐 KEYS");
}

#[test]
fn test_keys_tab_label_with_verified() {
    let mut state = ProviderModalState::default();
    state.provider_statuses = vec![
        ConnectionStatus::Connected { latency_ms: 100 },
        ConnectionStatus::Connected { latency_ms: 150 },
        ConnectionStatus::NotConfigured,
    ];
    // 2 verified out of 7 cloud providers
    assert!(state.keys_tab_label().contains("2/7"));
}

// Latency history tests
#[test]
fn test_latency_history_default_empty() {
    let state = ProviderModalState::default();
    // latency_history is pub(super), check via get_latency_history
    for i in 0..6 {
        assert!(state.get_latency_history(i).is_empty());
    }
}

#[test]
fn test_push_latency_adds_sample() {
    let mut state = ProviderModalState::default();
    state.push_latency(0, 100);
    state.push_latency(0, 150);
    state.push_latency(0, 120);

    let history = state.get_latency_history(0);
    assert_eq!(history, &[100, 150, 120]);
}

#[test]
fn test_push_latency_respects_max() {
    let mut state = ProviderModalState::default();
    // Push more than max (10)
    for i in 0..15 {
        state.push_latency(0, i as u64 * 10);
    }

    let history = state.get_latency_history(0);
    assert_eq!(history.len(), 10);
    // Should have kept the last 10 values (50, 60, ..., 140)
    assert_eq!(history[0], 50);
    assert_eq!(history[9], 140);
}

#[test]
fn test_push_latency_by_name() {
    let mut state = ProviderModalState::default();
    state.push_latency_by_name("anthropic", 100);
    state.push_latency_by_name("claude", 120); // Same as anthropic
    state.push_latency_by_name("openai", 200);
    state.push_latency_by_name("mistral", 80);

    assert_eq!(state.get_latency_history(0), &[100, 120]); // claude/anthropic
    assert_eq!(state.get_latency_history(1), &[200]); // openai
    assert_eq!(state.get_latency_history(2), &[80]); // mistral
}

#[test]
fn test_push_latency_invalid_index_ignored() {
    let mut state = ProviderModalState::default();
    state.push_latency(10, 100); // Invalid
    state.push_latency_by_name("unknown", 100); // Invalid

    // No history added
    for i in 0..6 {
        assert!(state.get_latency_history(i).is_empty());
    }
}

#[test]
fn test_get_latency_history_invalid_index() {
    let state = ProviderModalState::default();
    let empty: &[u64] = &[];
    assert_eq!(state.get_latency_history(10), empty); // Invalid
}

#[test]
fn test_set_provider_status_pushes_latency() {
    let mut state = ProviderModalState::default();
    state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 150 });
    state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 180 });

    let history = state.get_latency_history(0);
    assert_eq!(history, &[150, 180]);
}

#[test]
fn test_set_provider_status_by_name_pushes_latency() {
    let mut state = ProviderModalState::default();
    state.set_provider_status_by_name("openai", ConnectionStatus::Connected { latency_ms: 200 });

    let history = state.get_latency_history(1);
    assert_eq!(history, &[200]);
}

#[test]
fn test_latency_history_not_pushed_for_non_connected() {
    let mut state = ProviderModalState::default();
    state.set_provider_status(0, ConnectionStatus::Checking);
    state.set_provider_status(
        0,
        ConnectionStatus::Failed {
            error: "timeout".into(),
        },
    );
    state.set_provider_status(0, ConnectionStatus::NotConfigured);

    let history = state.get_latency_history(0);
    assert!(history.is_empty());
}

#[test]
fn test_get_session_stats_empty() {
    let state = ProviderModalState::default();
    let stats = state.get_session_stats();

    assert_eq!(stats.connected_providers, 0);
    assert_eq!(stats.total_providers, 7); // 7 cloud providers
    assert_eq!(stats.tokens_used, 0);
    assert!(stats.avg_latency_ms.is_none());
}

#[test]
fn test_get_session_stats_with_connections() {
    let mut state = ProviderModalState::default();
    state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 100 });
    state.set_provider_status(1, ConnectionStatus::Connected { latency_ms: 200 });
    state.set_provider_status(2, ConnectionStatus::NotConfigured);

    let stats = state.get_session_stats();

    assert_eq!(stats.connected_providers, 2);
    assert_eq!(stats.total_providers, 7); // 7 cloud providers
                                          // Average of 100 and 200 is 150
    assert_eq!(stats.avg_latency_ms, Some(150));
}

// Verification state tests
#[test]
fn test_verification_state_default() {
    let state = ProviderModalState::default();
    assert!(!state.verification_active);
    assert_eq!(state.verification_state.entries.len(), 7);
}

#[test]
fn test_start_verification() {
    let mut state = ProviderModalState::default();
    state.start_verification();

    assert!(state.verification_active);
    // All entries should be reset to Checking
    for entry in &state.verification_state.entries {
        assert_eq!(
            entry.status,
            super::super::components::ConnectionCheckStatus::Checking
        );
        assert_eq!(entry.progress, 0.0);
    }
}

#[test]
fn test_sync_verification_status() {
    use super::super::components::ConnectionCheckStatus;

    let mut state = ProviderModalState::default();
    state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 100 });
    state.set_provider_status(
        1,
        ConnectionStatus::Failed {
            error: "Timeout".into(),
        },
    );
    state.set_provider_status(2, ConnectionStatus::NotConfigured);

    // Verification status should be synced
    assert_eq!(
        state.verification_state.entries[0].status,
        ConnectionCheckStatus::Connected
    );
    assert_eq!(
        state.verification_state.entries[1].status,
        ConnectionCheckStatus::Failed
    );
    assert_eq!(
        state.verification_state.entries[2].status,
        ConnectionCheckStatus::NotConfigured
    );
}

#[test]
fn test_tick_animation_advances_verification() {
    let mut state = ProviderModalState::default();
    state.start_verification();

    let initial_frame = state.verification_state.frame;
    state.tick_animation();

    assert_eq!(state.verification_state.frame, initial_frame + 1);
}

#[test]
fn test_verification_auto_deactivates_when_complete() {
    use super::super::components::ConnectionCheckStatus;

    let mut state = ProviderModalState::default();
    state.start_verification();
    assert!(state.verification_active);

    // Set all cloud providers to connected
    for i in 0..super::providers::CLOUD_PROVIDER_COUNT {
        state
            .verification_state
            .set_status(i, ConnectionCheckStatus::Connected);
    }

    // Tick until complete
    for _ in 0..30 {
        state.tick_animation();
    }

    // Should auto-deactivate when all complete
    assert!(!state.verification_active);
}

#[test]
fn test_sync_all_verification_statuses() {
    use super::super::components::ConnectionCheckStatus;

    let mut state = ProviderModalState::default();

    // Set provider statuses directly (bypassing set_provider_status)
    state.provider_statuses = vec![
        ConnectionStatus::Connected { latency_ms: 100 },
        ConnectionStatus::Checking,
        ConnectionStatus::Failed {
            error: "err".into(),
        },
        ConnectionStatus::NotConfigured,
        ConnectionStatus::Unknown,
        ConnectionStatus::Connected { latency_ms: 200 },
    ];

    state.sync_all_verification_statuses();

    assert_eq!(
        state.verification_state.entries[0].status,
        ConnectionCheckStatus::Connected
    );
    assert_eq!(
        state.verification_state.entries[1].status,
        ConnectionCheckStatus::Checking
    );
    assert_eq!(
        state.verification_state.entries[2].status,
        ConnectionCheckStatus::Failed
    );
    assert_eq!(
        state.verification_state.entries[3].status,
        ConnectionCheckStatus::NotConfigured
    );
    assert_eq!(
        state.verification_state.entries[4].status,
        ConnectionCheckStatus::Checking
    );
    assert_eq!(
        state.verification_state.entries[5].status,
        ConnectionCheckStatus::Connected
    );
}

#[test]
fn test_sync_all_verification_statuses_covers_xai() {
    use super::super::components::ConnectionCheckStatus;

    let mut state = ProviderModalState::default();

    // Set all 7 providers to Connected
    state.provider_statuses = vec![ConnectionStatus::Connected { latency_ms: 10 }; 7];

    state.sync_all_verification_statuses();

    // Index 6 (xAI) must be synced — was previously skipped by 0..6
    assert_eq!(
        state.verification_state.entries[6].status,
        ConnectionCheckStatus::Connected,
        "xAI (index 6) must be synced by sync_all_verification_statuses"
    );
}

// ═══ TESTS: Session Token Tracking ═══

#[test]
fn test_set_session_tokens_updates_stats() {
    let mut state = ProviderModalState::default();

    // Initially tokens should be 0
    assert_eq!(state.get_session_stats().tokens_used, 0);

    // Set session tokens
    state.set_session_tokens(12345);

    // Stats should now reflect the tokens
    let stats = state.get_session_stats();
    assert_eq!(
        stats.tokens_used, 12345,
        "Session stats should reflect set tokens"
    );
}

#[test]
fn test_session_tokens_update_multiple_times() {
    let mut state = ProviderModalState::default();

    state.set_session_tokens(100);
    assert_eq!(state.get_session_stats().tokens_used, 100);

    state.set_session_tokens(500);
    assert_eq!(state.get_session_stats().tokens_used, 500);

    state.set_session_tokens(0);
    assert_eq!(state.get_session_stats().tokens_used, 0);
}

// ═══ TESTS: MCP Connection Status ═══

#[test]
fn test_set_mcp_connections_updates_stats() {
    let mut state = ProviderModalState::default();

    // Initially MCP connections should be 0
    assert_eq!(state.get_session_stats().mcp_connections, 0);

    // Set MCP connection count
    state.set_mcp_connections(3);

    // Stats should now reflect the connections
    let stats = state.get_session_stats();
    assert_eq!(
        stats.mcp_connections, 3,
        "Session stats should reflect MCP connections"
    );
}

#[test]
fn test_mcp_connections_update_multiple_times() {
    let mut state = ProviderModalState::default();

    state.set_mcp_connections(2);
    assert_eq!(state.get_session_stats().mcp_connections, 2);

    state.set_mcp_connections(5);
    assert_eq!(state.get_session_stats().mcp_connections, 5);

    state.set_mcp_connections(0);
    assert_eq!(state.get_session_stats().mcp_connections, 0);
}

// ═══ TESTS: Task-16 bug fixes ═══

#[test]
fn test_switch_tab_clears_key_input_mode() {
    let mut state = ProviderModalState::default();
    state.switch_tab(ProviderModalTab::Keys);
    state.key_input_mode = true;
    state.key_input_buffer = "sk-ant-partial".to_string();

    // Switching tab while in input mode must clear input state
    state.switch_tab(ProviderModalTab::Cloud);

    assert!(
        !state.key_input_mode,
        "key_input_mode must be cleared on tab switch"
    );
    assert!(
        state.key_input_buffer.is_empty(),
        "key_input_buffer must be zeroized on tab switch"
    );
    assert_eq!(state.active_tab, ProviderModalTab::Cloud);
    assert_eq!(state.selected_idx, 0);
}

#[test]
fn test_switch_tab_no_input_mode_noop() {
    let mut state = ProviderModalState::default();
    // key_input_mode is false — switch_tab must not panic or break anything
    state.switch_tab(ProviderModalTab::Native);
    assert_eq!(state.active_tab, ProviderModalTab::Native);
    assert!(!state.key_input_mode);
}

#[test]
fn test_set_native_models_clamps_selected_idx_when_native_tab() {
    let mut state = ProviderModalState::default();
    state.switch_tab(ProviderModalTab::Native);
    state.selected_idx = 5; // Would be OOB after the shrink

    let details = NativeModelDetails {
        parameter_size: "7B".to_string(),
        quantization_level: "Q4_0".to_string(),
        family: None,
    };
    let two_models = vec![
        NativeModelInfo {
            name: "a".to_string(),
            size: 0,
            digest: "d".to_string(),
            modified_at: "2024".to_string(),
            details: details.clone(),
        },
        NativeModelInfo {
            name: "b".to_string(),
            size: 0,
            digest: "d".to_string(),
            modified_at: "2024".to_string(),
            details,
        },
    ];

    state.set_native_models(two_models);

    // selected_idx must be clamped to the new max (1) and item_count must match
    assert_eq!(
        state.selected_idx, 1,
        "selected_idx clamped to last valid index"
    );
    assert_eq!(state.item_count, 2, "item_count updated to model count");
}

#[test]
fn test_set_native_models_no_clamp_when_not_native_tab() {
    let mut state = ProviderModalState::default();
    // Active tab is Cloud (default) — set_native_models must NOT change selected_idx
    state.selected_idx = 3;

    state.set_native_models(vec![]); // Would clamp to 0 if tab were Native

    assert_eq!(
        state.selected_idx, 3,
        "selected_idx unchanged when not on Native tab"
    );
}

#[test]
fn test_navigate_up_does_not_panic_with_single_item() {
    let mut state = ProviderModalState::default();
    state.active_tab = ProviderModalTab::Cloud;
    state.item_count = 1;
    state.selected_idx = 0;

    // Must not panic (was usize underflow: (0 - 1) * 3)
    state.navigate_up();
    assert_eq!(state.selected_idx, 0);
}
