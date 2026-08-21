// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
    )
)]

//! Lint passes — advisory diagnostics over a parsed workflow.
//!
//! Lints are **warnings · never errors** (spec `03-dag.md` §One obvious
//! way · « the discouraged forms are legal · just not canonical »). They
//! run on the raw AST after [`nika_schema::parse`] succeeds and are orthogonal
//! to [`nika_check::analyze`] (which emits spec ERRORS).
//!
//! v0.1 ships THREE rule sets · [`one_obvious_way`] — the control-flow
//! preference rules the spec marks « normative for linters » —
//! [`native_first()`] — the verb-choice preference rules (spec
//! `03-dag.md` §Native-first · exec with a probable native path) — and
//! [`arg_injection()`] — argument-injection advisories for the array command
//! form (spec `02-verbs.md` §exec Security · the differentiator).

mod arg_injection;
#[cfg(test)]
mod fixture_matrix_tests;
mod native_first;
mod preference_rules;

pub use arg_injection::arg_injection;
pub use native_first::native_first;
pub use preference_rules::{Lint, one_obvious_way};
