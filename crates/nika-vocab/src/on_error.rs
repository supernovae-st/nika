// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error recovery — the task-level `on_error:` block.
//!
//! Per spec `05-errors.md` §`on_error` · exactly ONE action + an
//! optional code filter ·
//!
//! | Field | Effect | Downstream sees |
//! |---|---|---|
//! | `recover: <value>` | a `${{ }}` ref OR a literal | `status: success` |
//! | `skip: true` | skip on error · **the original error stays readable** at `tasks.X.error` | `status: skipped` |
//! | `on_codes: [NIKA-…]` | **filter** · the action applies ONLY when the final error's code is listed · an unlisted code falls through to the default fail — the catch-side mirror of `retry.on_codes` |
//!
//! Two-or-zero actions = a parse error — exactly one must be present.
//! `on_codes` combines with any one action; alone it is a parse error
//! (a filter with nothing to filter).

use nika_source::Spanned;

/// The `on_error:` action — exactly one recovery behavior is chosen.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum OnErrorAction {
    /// `recover: <value>` — use a recovery output (a `${{ }}` ref like
    /// `${{ tasks.cached.output }}` OR any literal). Downstream sees
    /// `status: success` with the recover value as output · the value
    /// substitutes the raw output BEFORE `output:` binding extraction
    /// (spec 05 §recovery × bindings).
    Recover(Spanned<serde_json::Value>),
    /// `skip: true` — downstream sees `status: skipped` · the original
    /// typed error stays readable at `tasks.X.error` (spec 05 · the one
    /// state where both coexist · enables per-code routing).
    Skip,
}

/// A task's `on_error:` recovery policy — one action + optional filter.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct OnError {
    /// The recovery action (exactly one · parser-enforced).
    pub action: OnErrorAction,
    /// `on_codes:` — when non-empty, the action applies ONLY to the
    /// listed canonical codes (`NIKA-<NS>[-<SUB>]-<NNN>` · validated
    /// against [`is_valid_error_code`](crate::is_valid_error_code))
    /// · an unlisted code falls through to the default fail.
    pub on_codes: Vec<Spanned<String>>,
}

impl OnError {
    /// A policy with the given action and no code filter (applies to
    /// every error · the pre-filter spec shape).
    #[must_use]
    pub fn new(action: OnErrorAction) -> Self {
        Self {
            action,
            on_codes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_source::Span;

    #[test]
    fn recover_carries_value() {
        let v = OnError::new(OnErrorAction::Recover(Spanned::new(
            serde_json::json!(0),
            Span::default(),
        )));
        assert!(matches!(v.action, OnErrorAction::Recover(ref s) if s.value == 0));
        assert!(v.on_codes.is_empty(), "new() = no filter · applies to all");
    }

    #[test]
    fn unit_variants_exist() {
        assert!(matches!(
            OnError::new(OnErrorAction::Skip).action,
            OnErrorAction::Skip
        ));
    }
}
