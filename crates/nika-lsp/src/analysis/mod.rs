// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pure analysis brain — no I/O, no server state.
//!
//! Every function here is `(text[, offset]) -> value` over a `.nika.yaml`
//! source, computed against the L0 `nika-schema` parse + check ladder.
//! Because they are pure, the whole feature surface (diagnostics, hover,
//! completion, definition, symbols, position mapping) is unit- and
//! property-testable WITHOUT a running language-server connection. The
//! [`server`](crate::run_stdio) loop is the thin transport shell that
//! drives these.

pub mod code_action;
pub mod completion;
pub mod definition;
pub mod diagnostics;
pub mod document;
pub mod graph;
pub mod hover;
pub mod members;
pub mod position;
pub mod scope;
pub mod semantic_document;
pub mod symbols;
pub mod vocab;
