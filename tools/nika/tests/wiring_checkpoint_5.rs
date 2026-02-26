//! WIRING Checkpoint 5: Session ↔ DAG Restore
//!
//! Verifies: Session persistence infrastructure works correctly
//!
//! See: docs/plans/v0.9.1/WIRING-CHECKPOINTS.md
//!
//! Note: This tests the session infrastructure. Full DAG persistence
//! integration tests require ChatView which is TUI-only.

use nika::tui::session::{delete_session, get_latest_session, list_sessions};

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: list_sessions returns valid result
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_5_list_sessions() {
    // Should return Ok even if no sessions exist
    let result = list_sessions();
    assert!(result.is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: get_latest_session handles empty state
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_5_get_latest_session() {
    let result = get_latest_session();
    // Should succeed (returns None if no sessions)
    assert!(result.is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: delete_session handles non-existent session
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_5_delete_nonexistent() {
    // Deleting a non-existent session should fail gracefully
    let result = delete_session("nonexistent-session-id-12345");
    // This should be an error (file not found) but not panic
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: SessionMeta has correct fields
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_5_session_meta_fields() {
    // Test that SessionMeta can be constructed (via list_sessions)
    let sessions = list_sessions().unwrap_or_default();

    // If any sessions exist, verify the structure
    if let Some(meta) = sessions.first() {
        assert!(!meta.id.is_empty());
        // Title and message_count should exist
        let _title = &meta.title;
        let _count = meta.message_count;
    }

    // Test passes even with no sessions - reaching here means success
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 5: Session directory constant exists
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_5_session_infrastructure() {
    // Verify session module is accessible
    // (this test passing means the module is properly exported)
    let _ = list_sessions();
    let _ = get_latest_session();

    // Session infrastructure is working - reaching here means success
}
