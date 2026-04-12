// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Convert `nika_core::binding::BindingEvent` into `nika_event::EventKind`.
//!
//! nika-core cannot depend on nika-event (dep graph points the other way),
//! so the binding resolver emits the core-level `BindingEvent` enum which
//! the engine maps to telemetry `EventKind` at the boundary.
//!
//! The orphan rule forbids `impl From<BindingEvent> for EventKind` in
//! engine (neither type is local here), so this module provides a free
//! function that callers use explicitly. Invariant #25's wildcard arm
//! guards against new `BindingEvent` variants added upstream.

use nika_core::binding::BindingEvent;

use crate::event::EventKind;

/// Convert a single `BindingEvent` into an `EventKind`.
pub fn binding_event_to_event_kind(ev: BindingEvent) -> EventKind {
    match ev {
        BindingEvent::TransformApplied {
            task_id,
            alias,
            transform_chain,
        } => EventKind::BindingTransformApplied {
            task_id,
            alias,
            transform_chain,
        },
        BindingEvent::DefaultApplied {
            task_id,
            alias,
            path,
            default_value,
        } => EventKind::BindingDefaultApplied {
            task_id,
            alias,
            path,
            default_value,
        },
        BindingEvent::EnvResolved {
            task_id,
            var_name,
            found,
        } => EventKind::BindingEnvResolved {
            task_id,
            var_name,
            found,
        },
        BindingEvent::VaultResolved {
            task_id,
            service,
            field,
            found,
        } => EventKind::BindingVaultResolved {
            task_id,
            service,
            field,
            found,
        },
        // Invariant #25: a new BindingEvent variant added upstream is
        // still observable as a generic Log entry so it stays
        // triageable. Removing this arm requires wiring the new
        // variant to a dedicated EventKind.
        other => EventKind::Log {
            level: "warn".into(),
            message: format!("unmapped BindingEvent variant: {other:?}"),
            task_id: None,
        },
    }
}

/// Convert a vector of `BindingEvent` into `EventKind` via
/// `binding_event_to_event_kind`.
pub fn binding_events_to_event_kinds(events: Vec<BindingEvent>) -> Vec<EventKind> {
    events.into_iter().map(binding_event_to_event_kind).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn tid() -> Arc<str> {
        Arc::from("t1")
    }

    #[test]
    fn transform_applied_roundtrip() {
        let be = BindingEvent::TransformApplied {
            task_id: tid(),
            alias: "x".into(),
            transform_chain: "upper | trim".into(),
        };
        let ek = binding_event_to_event_kind(be);
        assert!(matches!(
            ek,
            EventKind::BindingTransformApplied {
                ref alias,
                ref transform_chain,
                ..
            } if alias == "x" && transform_chain == "upper | trim"
        ));
    }

    #[test]
    fn default_applied_preserves_value() {
        let be = BindingEvent::DefaultApplied {
            task_id: tid(),
            alias: "a".into(),
            path: "task.missing".into(),
            default_value: json!("fallback"),
        };
        let ek = binding_event_to_event_kind(be);
        match ek {
            EventKind::BindingDefaultApplied {
                alias,
                path,
                default_value,
                ..
            } => {
                assert_eq!(alias, "a");
                assert_eq!(path, "task.missing");
                assert_eq!(default_value, json!("fallback"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn env_resolved_maps_found_flag() {
        let be = BindingEvent::EnvResolved {
            task_id: tid(),
            var_name: "API_KEY".into(),
            found: true,
        };
        let ek = binding_event_to_event_kind(be);
        assert!(matches!(
            ek,
            EventKind::BindingEnvResolved { found: true, ref var_name, .. }
            if var_name == "API_KEY"
        ));
    }

    #[test]
    fn vault_resolved_preserves_fields() {
        let be = BindingEvent::VaultResolved {
            task_id: tid(),
            service: "stripe".into(),
            field: "api_key".into(),
            found: false,
        };
        let ek = binding_event_to_event_kind(be);
        match ek {
            EventKind::BindingVaultResolved {
                service,
                field,
                found,
                ..
            } => {
                assert_eq!(service, "stripe");
                assert_eq!(field, "api_key");
                assert!(!found);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn bulk_conversion() {
        let evs = vec![
            BindingEvent::EnvResolved {
                task_id: tid(),
                var_name: "A".into(),
                found: true,
            },
            BindingEvent::VaultResolved {
                task_id: tid(),
                service: "s".into(),
                field: "f".into(),
                found: true,
            },
        ];
        let out = binding_events_to_event_kinds(evs);
        assert_eq!(out.len(), 2);
    }
}
