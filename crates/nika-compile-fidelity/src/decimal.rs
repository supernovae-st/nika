// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Exact decimal laws emitted by the compiler; pure jq text, never evaluated here.
//! The same laws guard source transport, ordering and arithmetic in the assembled workflow.

/// Exact order keys, rank cuts and JSON number transport guards.
pub const ORDER: &str = include_str!("decimal/order.jq");

/// Bounded exact sums, averages and stated roundings, after [`ORDER`].
/// A result is emitted only where a JSON number carries it exactly.
pub const ARITHMETIC: &str = include_str!("decimal/arithmetic.jq");
