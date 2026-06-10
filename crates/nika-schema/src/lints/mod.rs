// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Lint passes — advisory diagnostics over a parsed workflow.
//!
//! Lints are **warnings · never errors** (spec `03-dag.md` §One obvious
//! way · « the discouraged forms are legal · just not canonical »). They
//! run on the raw AST after [`crate::parse`] succeeds and are orthogonal
//! to [`crate::analyze`] (which emits spec ERRORS).
//!
//! v0.1 ships ONE rule set · [`one_obvious_way`] — the 7 control-flow
//! preference rules the spec marks « normative for linters ».

mod one_obvious_way;

pub use one_obvious_way::{Lint, one_obvious_way};
