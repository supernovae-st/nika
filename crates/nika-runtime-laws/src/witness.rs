// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The permit-decision witness's HOME is `nika-cap` (NEP-0007 law 2 ·
//! descended at the 15k wall, P3 B5 — the decision over a `Permits`
//! boundary is capability-boundary data, and the collector is pure
//! `std`). This shim keeps the crate's call sites reading
//! `crate::witness::{PermitWitness, PermitDecision}` exactly as before.

pub use nika_cap::{PermitDecision, PermitWitness};
