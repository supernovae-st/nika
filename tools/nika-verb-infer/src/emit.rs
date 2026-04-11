// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Single-source `ProviderResponded` event emission helper (W16-A0).
//!
//! Invariant #24 (S14.5) enforcement: every code path that emits
//! `EventKind::ProviderResponded` goes through this one free function so
//! that field additions/renames are a single-site change and the golden
//! oracle has exactly one regression target.
//!
//! Before W16-A0 landed, `nika-engine/src/runtime/executor/infer.rs`
//! emitted `ProviderResponded` from 7 direct sites (lines 621, 1156,
//! 1330, 1388, 1527, 1592, 1898) and `nika_verb_infer::run` emitted from
//! an 8th site — 8 places to edit when the variant gained a field, and
//! only the 8th was covered by the S14-δ golden oracle. S14.5 invariant
//! #24 explicitly named infer.rs as the violator; W16-A0 is the fix.
//!
//! ## Why primitive params (not `&InferResponse`)
//!
//! The 6 non-mock engine sites build the event from an internal
//! `StreamResult` (streaming paths) or from pre-estimated tokens
//! (vision, L0b tool-injection). None of them has an `InferResponse` in
//! scope, and synthesizing one just for this helper would add a
//! redundant shape. The verb crate's `run` does have an `InferResponse`
//! but extracts the 8 fields inline at the call site — symmetric with
//! the engine. Primitives keep the helper's contract obvious: one
//! parameter per event field, no pre-processing surprises.
//!
//! ## Pure pass-through
//!
//! All pre-processing stays at the call site:
//!
//! - `StopReason` → `FinishReason` mapping (see
//!   [`crate::stop_reason_to_finish_reason`]). W16-B3 (landed
//!   same-session as the helper) gave that mapping a second
//!   parameter `finish_reason_raw: Option<&str>` so `ContentFilter`
//!   and `Unknown` preserve the provider's verbatim string.
//!   Consumption happens IN the mapping fn, NOT in this helper —
//!   the helper takes an already-mapped `FinishReason` as the 8th
//!   positional argument. Do not add `finish_reason_raw` here
//!   without also breaking the 1:1 param-to-variant-field contract.
//! - `cost_usd` sanitization (`if cost.is_finite() { cost } else { 0.0 }`
//!   — engine pattern at the streaming sites).
//! - Token estimation (engine vision/L0b paths).
//!
//! ## Synchronous
//!
//! Invariants #1 / #16 (no `!Send` guards across `.await`) — all 8
//! current call sites have zero `.await` between response
//! materialization and the emit, confirmed by the rust-async-expert
//! Phase 1 audit. The helper is a plain `fn`, not `async`.

use std::sync::Arc;

use nika_event::types::FinishReason;
use nika_event::{EventKind, EventLog};

/// Emit a `ProviderResponded` event with the 8 canonical fields.
///
/// This is the single source of truth for `EventKind::ProviderResponded`
/// emission across the engine bridge (`runtime/executor/infer.rs`) and
/// the verb crate (`nika_verb_infer::run`). A new field on the variant
/// is added here first — `cargo check` then fails at every call site
/// (the destructure requires all fields), which is the intended
/// forcing function.
///
/// Parameters map 1:1 to the [`EventKind::ProviderResponded`] variant's
/// fields. `task_id` is taken by reference and cloned internally so
/// callers don't have to `Arc::clone` at each call site.
///
/// # Example
///
/// ```ignore
/// use std::sync::Arc;
/// use nika_event::types::FinishReason;
/// use nika_event::EventLog;
/// use nika_verb_infer::emit_provider_responded;
///
/// let log = EventLog::new();
/// let task_id: Arc<str> = Arc::from("task-42");
/// emit_provider_responded(
///     &log,
///     &task_id,
///     Some("req-abc".to_string()),
///     42,    // input_tokens
///     17,    // output_tokens
///     8,     // cache_read_tokens
///     Some(123),  // ttft_ms
///     FinishReason::EndTurn,
///     0.0042,  // cost_usd
/// );
/// ```
#[allow(clippy::too_many_arguments)]
pub fn emit_provider_responded(
    event_log: &EventLog,
    task_id: &Arc<str>,
    request_id: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    ttft_ms: Option<u64>,
    finish_reason: FinishReason,
    cost_usd: f64,
) {
    event_log.emit(EventKind::ProviderResponded {
        task_id: Arc::clone(task_id),
        request_id,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        ttft_ms,
        finish_reason,
        cost_usd,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The helper emits exactly one event with all 8 fields preserved
    /// verbatim — the golden oracle contract for invariant #24.
    #[test]
    fn emit_helper_forwards_all_eight_fields() {
        let log = EventLog::new();
        let task_id: Arc<str> = Arc::from("test-task-42");

        emit_provider_responded(
            &log,
            &task_id,
            Some("req_golden_01".to_string()),
            42,
            17,
            8,
            Some(123),
            FinishReason::EndTurn,
            0.004_2_f64,
        );

        let events = log.events();
        let responded: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::ProviderResponded { .. }))
            .collect();
        assert_eq!(
            responded.len(),
            1,
            "helper must emit exactly one ProviderResponded"
        );

        let EventKind::ProviderResponded {
            task_id: tid,
            request_id,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            ttft_ms,
            finish_reason,
            cost_usd,
        } = &responded[0].kind
        else {
            panic!("expected ProviderResponded, got {:?}", responded[0].kind);
        };

        assert_eq!(tid.as_ref(), "test-task-42");
        assert_eq!(request_id.as_deref(), Some("req_golden_01"));
        assert_eq!(*input_tokens, 42);
        assert_eq!(*output_tokens, 17);
        assert_eq!(*cache_read_tokens, 8);
        assert_eq!(*ttft_ms, Some(123));
        assert_eq!(finish_reason, &FinishReason::EndTurn);
        // Exact equality — `cost_usd` is pure pass-through, no
        // arithmetic, same bit pattern both sides (S14.5 precedent).
        assert_eq!(*cost_usd, 0.004_2_f64);
    }

    /// `request_id = None` + `ttft_ms = None` path (synthetic vision /
    /// L0b non-streaming shape). The engine sites 1527/1592/1898 all
    /// pass `None` for these two fields.
    #[test]
    fn emit_helper_preserves_none_request_id_and_ttft() {
        let log = EventLog::new();
        let task_id: Arc<str> = Arc::from("vision-task");

        emit_provider_responded(
            &log,
            &task_id,
            None,
            1000,
            500,
            0,
            None,
            FinishReason::Stop,
            0.012,
        );

        let events = log.events();
        let EventKind::ProviderResponded {
            request_id,
            ttft_ms,
            cache_read_tokens,
            ..
        } = &events[0].kind
        else {
            panic!("expected ProviderResponded, got {:?}", events[0].kind);
        };

        assert!(request_id.is_none());
        assert!(ttft_ms.is_none());
        assert_eq!(*cache_read_tokens, 0);
    }

    /// `FinishReason::Mock` override path — the engine's site 621 (mock
    /// provider fast-path) passes `Mock` directly without going through
    /// `StopReason`. Verify the helper doesn't mangle non-standard
    /// finish reasons.
    #[test]
    fn emit_helper_accepts_mock_finish_reason() {
        let log = EventLog::new();
        let task_id: Arc<str> = Arc::from("mock-task");

        emit_provider_responded(
            &log,
            &task_id,
            Some("mock-request".to_string()),
            10,
            20,
            0,
            Some(0),
            FinishReason::Mock,
            0.0,
        );

        let events = log.events();
        let EventKind::ProviderResponded { finish_reason, .. } = &events[0].kind else {
            panic!("expected ProviderResponded, got {:?}", events[0].kind);
        };
        assert_eq!(finish_reason, &FinishReason::Mock);
    }
}
