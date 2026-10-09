// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Fitting words into cells, relocated to
//! [`nika_tui_view::workspace::text`] (ADR-143): this path keeps the
//! renderer's callers unchanged.

pub(crate) use nika_tui_view::workspace::text::{fit_head, marks, twins, wrap};
