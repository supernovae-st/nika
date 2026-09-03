// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The stamper/sink seams' HOME is `nika-event` (descended at the 15k
//! wall · P3 B7 — the stamper IS event machinery). This shim keeps the
//! crate's `stamp::` paths and the public re-export byte-stable.

pub use nika_event::stamp::*;
