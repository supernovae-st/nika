// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Declared-vs-used drift (the `NIKA-DRIFT-001` advisory family) — the
//! scan COMPUTE descended to `nika_dap::drift` 2026-07-21 (the 15k
//! wall: compute descends, render stays). The check verb keeps the
//! terminal HINT block + the `--json` rows; this shim keeps the
//! `super::drift::…` paths compiling unchanged.

pub(super) use nika_dap::drift::{DRIFT_CODE, scan};
