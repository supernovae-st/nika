// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The renderer's visual vocabulary, borrowed before anything is invented.
//! It lives in the viewer member, [`nika_tui_view::visual`] (ADR-143), so the
//! renderer and every viewer paint with one palette; these paths keep the
//! renderer's callers unchanged.
//!
//! Colour, the task-state glyphs and the four verb identities belong to the
//! engine's one theme seam, [`nika_display::theme`]: the terminal's ANSI-16
//! slots, whose hues the user's own theme decides, a normal band for state and
//! verdict and a bright band for the verbs, and an ASCII column for every glyph.
//! This module adds only what that seam has no word for:
//!
//! - [`role`]: the Ratatui style of each semantic role, pinned to the slot the
//!   CLI frames paint, so a widget asks for a meaning and never for a hue;
//! - [`icon`]: the workspace objects a screen names (project, workflow,
//!   conversation, run…), each with a label that is always shown, a Unicode
//!   glyph only where the terminal cell is certain, and an ASCII twin;
//! - [`logomark`]: the Supernovae butterfly, the only brand mark drawn, sampled
//!   from the repository's own logomark and revealed once, never looped;
//! - [`state`]: a task or run state as the seam's own glyph and role, re-read
//!   as data for Ratatui and pinned to what the seam paints.
//!
//! A glyph decorates a label and never replaces it; a colour never carries a
//! meaning alone; nothing here reads the clock, the environment or a file.

pub mod icon;
pub mod logomark;
pub mod role;
pub mod state;
