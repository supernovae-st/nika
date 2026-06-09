// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error recovery — the task-level `on_error:` block.
//!
//! Per spec `05-errors.md` §`on_error` · the three fields are **mutually
//! exclusive** ·
//!
//! | Field | Effect | Downstream sees |
//! |---|---|---|
//! | `recover: <value>` | a `${{ }}` ref OR a literal | `status: success` |
//! | `skip: true` | skip on error | `status: skipped` |
//! | `fail_workflow: true` | fail the workflow (default behavior) | n/a |
//!
//! Two-or-zero fields = a parse error — exactly one must be present.

use crate::source::Spanned;

/// A task's `on_error:` recovery policy — exactly one of the three modes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum OnError {
    /// `recover: <value>` — use a recovery output (a `${{ }}` ref like
    /// `${{ tasks.cached.output }}` OR any literal). Downstream sees
    /// `status: success` with the recover value as output.
    Recover(Spanned<serde_json::Value>),
    /// `skip: true` — downstream sees `status: skipped`.
    Skip,
    /// `fail_workflow: true` — explicit form of the default behavior.
    FailWorkflow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Span;

    #[test]
    fn recover_carries_value() {
        let v = OnError::Recover(Spanned::new(serde_json::json!(0), Span::default()));
        assert!(matches!(v, OnError::Recover(ref s) if s.value == 0));
    }

    #[test]
    fn unit_variants_exist() {
        assert!(matches!(OnError::Skip, OnError::Skip));
        assert!(matches!(OnError::FailWorkflow, OnError::FailWorkflow));
    }
}
