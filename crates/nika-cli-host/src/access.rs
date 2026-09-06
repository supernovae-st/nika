// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The access-plan door of the CLI host. The ONE resolver lives in
//! [`nika_service_execution::access`] since wave 1b (the CLI door, the
//! answered gate leg, the resident's jobs and an ARM beat read one law);
//! this module re-exports it so every CLI surface keeps one path to it.

pub use nika_service_execution::access::{model_needs, resolve_plan, resolve_plan_over};
