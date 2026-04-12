// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Binding resolver events (core-level intermediate representation).
//!
//! `BindingEvent` is emitted by `binding/resolve.rs::from_with_spec_traced`
//! during binding resolution. It mirrors the 4 `EventKind::Binding*` variants
//! in `nika-event` byte-for-byte, but lives here because nika-core cannot
//! depend on nika-event (the dependency would go the wrong way).
//!
//! # Wiring
//!
//! Engine code converts via `From<BindingEvent> for EventKind` defined in
//! `nika-engine::binding::event_bridge`. Callers of `from_with_spec_traced`
//! map `Vec<BindingEvent>` → `Vec<EventKind>` at the engine boundary.
//!
//! # Why 4 variants, not all binding-related EventKinds?
//!
//! Only 4 `EventKind::Binding*` variants are emitted from within the
//! resolver. Other binding events (e.g. `BindingResolved` at task level)
//! live in nika-engine's runtime layer — outside the resolver's scope.

use std::sync::Arc;

use serde_json::Value;

/// Events emitted by the binding resolver.
///
/// The shape mirrors the corresponding `nika_event::EventKind` variants
/// exactly so the engine's `From<BindingEvent> for EventKind` impl can be
/// a one-to-one field move.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BindingEvent {
    /// Binding transform chain applied (e.g., `upper | trim | sort`).
    TransformApplied {
        task_id: Arc<str>,
        alias: String,
        transform_chain: String,
    },

    /// Binding default value applied (via `??` operator or explicit `default:`).
    DefaultApplied {
        task_id: Arc<str>,
        alias: String,
        path: String,
        default_value: Value,
    },

    /// `$env.VAR` binding resolved (successful or not).
    EnvResolved {
        task_id: Arc<str>,
        var_name: String,
        found: bool,
    },

    /// `$vault.service.field` binding resolved (successful or not).
    VaultResolved {
        task_id: Arc<str>,
        service: String,
        field: String,
        found: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BindingEvent>();
    }
}
