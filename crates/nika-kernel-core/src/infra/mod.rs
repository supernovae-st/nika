// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Infrastructure traits — billing, events, metrics, tracing, ID generation, secrets.
//!
//! Future: stays in kernel (or `nika-kernel-infra` if we split).

pub mod audit;
pub mod billing;
pub mod event_sink;
pub mod id_gen;
pub mod metrics;
pub mod secret;
pub mod trace;
