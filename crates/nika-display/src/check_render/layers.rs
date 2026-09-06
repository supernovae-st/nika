// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The four layered verdicts of `nika check` (One Door · wave 2) — the
//! type the verb computes once, the ACCESS rung and the layers line the
//! render projects. Split from `check_render.rs` at the 1,500-line file
//! wall; the public path stays `check_render::VerdictLayers`.

use std::fmt::Write as _;

use super::mark;
use crate::theme::{Role, Theme};

/// The four layered verdicts of one audit (One Door · wave 2 · the
/// pack's product law: `check` answers four different questions and
/// never collapses them into one checkmark). VALID is the definition ·
/// ACCESS READY is this machine now (`None` = no static model, nothing
/// to judge · presence only, never a dial) · CAPACITY FIT is the seat
/// against the declaration · RUN READY folds the three with any known
/// blocker. The renderer prints them; the caller computed them ONCE
/// beside the exit code (P0-11).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct VerdictLayers {
    /// The definition is semantically legal (the ladder's `clean` half).
    pub valid: bool,
    /// Every static lane has a ready path on this machine · `None` when
    /// no static model exists to judge.
    pub access_ready: Option<bool>,
    /// The ACCESS rung's line — the lanes as resolved, one clause each.
    pub access_lines: Vec<String>,
    /// The seats can satisfy the declarations (thinking + capacity laws).
    pub capacity_fit: bool,
    /// The blockers RUN READY names (empty when ready).
    pub blockers: Vec<String>,
    /// The models whose admitted lane is a subscription seat — the COST
    /// rung's dollar figure is their API counterfactual, never their bill
    /// (W3-F4).
    pub seat_served: Vec<String>,
}

impl VerdictLayers {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        valid: bool,
        access_ready: Option<bool>,
        access_lines: Vec<String>,
        capacity_fit: bool,
        blockers: Vec<String>,
    ) -> Self {
        Self {
            valid,
            access_ready,
            access_lines,
            capacity_fit,
            blockers,
            seat_served: Vec::new(),
        }
    }

    /// Name the seat-served models (W3-F4).
    #[must_use]
    pub fn with_seat_served(mut self, models: Vec<String>) -> Self {
        self.seat_served = models;
        self
    }

    /// RUN READY: valid · every judged lane ready · capacity fit · no
    /// blocker. `None` when access is unjudged (the run may still refuse
    /// at admission on a run-time model).
    #[must_use]
    pub fn run_ready(&self) -> Option<bool> {
        match self.access_ready {
            Some(access) => {
                Some(self.valid && access && self.capacity_fit && self.blockers.is_empty())
            }
            None if !self.valid || !self.capacity_fit || !self.blockers.is_empty() => Some(false),
            None => None,
        }
    }
}

impl Default for VerdictLayers {
    fn default() -> Self {
        Self::new(true, None, Vec::new(), true, Vec::new())
    }
}

/// The ACCESS rung (wave 2): one line per static lane as THIS machine
/// resolves it — the path that serves, its class, its billing lane —
/// or the refusal with its witnesses. Presence is judged, never
/// validity: `check` does not dial (the pack's « check before spend »).
pub(super) fn access_rung(out: &mut String, layers: &VerdictLayers, t: Theme) {
    match layers.access_ready {
        None => {}
        Some(ready) => {
            let label = t.paint(Role::Strong, "ACCESS");
            let mut lines = layers.access_lines.iter();
            let first = lines.next().cloned().unwrap_or_default();
            let _ = writeln!(
                out,
                " {} {}   {}",
                mark(t, ready),
                label,
                t.paint(Role::Dim, &first)
            );
            for line in lines {
                let _ = writeln!(out, "            {}", t.paint(Role::Dim, line));
            }
        }
    }
}

/// The layers line (wave 2): the four verdicts side by side, so a
/// reader who saw a green audit also sees which question it answered.
pub(super) fn layers_line(layers: &VerdictLayers, t: Theme) -> String {
    let yes = if t.ascii { "ok" } else { "✔" };
    let no = if t.ascii { "X" } else { "✖" };
    let none = if t.ascii { "-" } else { "○" };
    let tick = |v: Option<bool>| match v {
        Some(true) => yes,
        Some(false) => no,
        None => none,
    };
    let run = layers.run_ready();
    let role = match run {
        Some(true) => Role::Good,
        Some(false) => Role::Warn,
        None => Role::Dim,
    };
    let mut line = format!(
        "layers · valid {} · access ready {} · capacity fit {} · run ready {}",
        tick(Some(layers.valid)),
        tick(layers.access_ready),
        tick(Some(layers.capacity_fit)),
        tick(run),
    );
    if let Some(first) = layers.blockers.first() {
        line.push_str(" · ");
        line.push_str(first);
    }
    t.paint(role, &line)
}
