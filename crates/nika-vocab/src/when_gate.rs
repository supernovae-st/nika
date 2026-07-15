// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `when:` gate — a `${{ … }}` CEL boolean OR a YAML boolean literal.
//!
//! Per spec `03-dag.md` §when shape rules · `when:` accepts exactly two
//! forms ·
//!
//! | Form | Meaning |
//! |---|---|
//! | `when: ${{ … }}` | a CEL boolean expression · evaluated over terminal deps |
//! | `when: true` / `when: false` | the YAML **boolean literal** · the always/never pattern (`true` replaces the default success-gate · the task runs even when an upstream failed) |
//!
//! A bare string (`when: "tasks.a.status == 'success'"` · no `${{ }}`)
//! is never evaluated and is rejected by the analyzer. A *quoted*
//! `"true"` is a bare string, not a boolean literal — marked-yaml keeps
//! the distinction (`as_bool` coerces plain-style scalars only), so the
//! parser sees exactly what the spec means.

/// A task's `when:` value — boolean literal or expression source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhenGate {
    /// `when: true` / `when: false` — the YAML boolean literal.
    /// `true` is the **always-pattern** (runs whatever happened
    /// upstream — the record/notify shape) · `false` never runs.
    Literal(bool),
    /// `when: ${{ … }}` — the raw string body · the analyzer enforces
    /// the single-island boolean shape (spec 03 · `NIKA-VAR-005` class).
    Expr(String),
}

impl WhenGate {
    /// The expression source, when this gate is the expression form.
    #[must_use]
    pub fn as_expr(&self) -> Option<&str> {
        match self {
            Self::Expr(s) => Some(s),
            Self::Literal(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_carries_bool() {
        assert_eq!(WhenGate::Literal(true), WhenGate::Literal(true));
        assert_ne!(WhenGate::Literal(true), WhenGate::Literal(false));
    }

    #[test]
    fn as_expr_splits_the_forms() {
        assert_eq!(
            WhenGate::Expr("${{ x > 0 }}".into()).as_expr(),
            Some("${{ x > 0 }}")
        );
        assert_eq!(WhenGate::Literal(true).as_expr(), None);
    }
}
