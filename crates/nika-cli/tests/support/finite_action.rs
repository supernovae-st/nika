// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A finite Session action on a PTY, read from the raw stream: the `action ›` prompt the
//! renderer paints for a decision (a run's cost, a proposal's consent, a gate) and the keys
//! that choose one of its offers.
//!
//! The diff renderer repaints only the cells that change: from `nika › ` to `action › ` the
//! seventh cell (a blank) stays, so the prompt reaches the stream as `action`, a cursor move,
//! then `›`. [`PAINTED`] accepts control sequences and blanks between the two words, never
//! other text, so the title's « action required » never matches. A spending decision paints
//! its question under `nika › ` first and reveals `action › ` only after the typeahead it
//! discards: keys are sent once [`PAINTED`] matched, never before.
//!
//! Nothing is preselected: the first Down selects the first offer, and Enter sends the selected
//! offer once. Esc never answers an action: its `no` or `cancel` is an offer like any other,
//! chosen by its index. A bare Enter within 500 ms of the previous activation is taken as its
//! repeat; the cursor key before it ends that window.

/// Down, as a terminal sends it in either cursor-key mode the renderer reads.
pub(crate) const DOWN: &str = "\x1b[B";
/// Up, likewise.
pub(crate) const UP: &str = "\x1b[A";
/// Enter in raw mode.
pub(crate) const ENTER: &str = "\r";
/// Esc alone: sent by itself, so the renderer cannot read it as the start of a sequence.
pub(crate) const ESC: &str = "\x1b";
/// F2 (xterm): closes the reader an action's details offer opened.
pub(crate) const F2: &str = "\x1bOQ";

/// The painted `action ›` prompt, with the cursor moves and styles the diff renderer may put
/// between its two words.
pub(crate) const PAINTED: expectrl::Regex<&str> =
    expectrl::Regex(r"action(?:\x1b\[[0-9;?]*[ -/]*[@-~]| )*›");

/// The keys that select the offer painted at `index` (from 0, top to bottom) and send it.
pub(crate) fn choose(index: usize) -> String {
    format!("{}{ENTER}", DOWN.repeat(index + 1))
}
