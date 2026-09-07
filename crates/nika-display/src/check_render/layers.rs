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
    /// The ACCESS question is MOOT, not unanswered: this file holds no
    /// `infer:`/`agent:` task, so no seat will ever dial and there is
    /// nothing for a pin to fix. Distinct from `access_ready == None`
    /// with a task present, where the model arrives at run time and
    /// admission judges it (a persona wave · the operations sceptic: `run ready ○` on a file that
    /// then ran 3/3 green — the two shapes shared one glyph).
    ///
    /// Defaults FALSE, so a caller that never sets it keeps the
    /// unanswered reading: the honest direction is to withhold a ✔,
    /// never to invent one.
    pub access_moot: bool,
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
            access_moot: false,
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

    /// Declare the ACCESS question MOOT — no task dials (see
    /// [`Self::access_moot`]). The caller that holds the workflow is the
    /// only one that can know it.
    #[must_use]
    pub fn with_access_moot(mut self, moot: bool) -> Self {
        self.access_moot = moot;
        self
    }

    /// RUN READY: valid · every judged lane ready · capacity fit · no
    /// blocker.
    ///
    /// `None` — genuinely unknown — only when access is UNANSWERED: a
    /// task dials, but its model arrives at run time and admission
    /// judges it. When access is MOOT the three answered layers decide,
    /// because no seat will ever be asked for. A persona wave (the operations sceptic), verbatim:
    /// « `run ready ○` on a file that then ran `RUN_RC=0`, 3/3 tasks
    /// green. No blocker is named, no flag flips it, `--access mock`
    /// changes nothing. The readiness line is a verdict the run
    /// contradicts. » — that file had zero `infer:`/`agent:` tasks, so
    /// the access question had no subject, and `None` was read as doubt.
    #[must_use]
    pub fn run_ready(&self) -> Option<bool> {
        match self.access_ready {
            Some(access) => {
                Some(self.valid && access && self.capacity_fit && self.blockers.is_empty())
            }
            None if !self.valid || !self.capacity_fit || !self.blockers.is_empty() => Some(false),
            None if self.access_moot => Some(true),
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
    // MOOT reads « n/a », never the unjudged glyph: a file with no
    // `infer:`/`agent:` task is not waiting on an answer, and no flag
    // (`--access mock` included) can turn one into a ✔.
    let unjudged = layers.access_ready.is_none() && !layers.access_moot;
    let access = if layers.access_moot && layers.access_ready.is_none() {
        "n/a · nothing dials".to_owned()
    } else {
        tick(layers.access_ready).to_owned()
    };
    let mut line = format!(
        "layers · valid {} · access ready {access} · capacity fit {} · run ready {}",
        tick(Some(layers.valid)),
        tick(Some(layers.capacity_fit)),
        tick(run),
    );
    if let Some(first) = layers.blockers.first() {
        line.push_str(" · ");
        line.push_str(first);
    }
    // The ONE place `○` is defined — printed exactly where it appears,
    // for the reader who has it on screen. A persona wave (the operations sceptic): « `○` is never
    // defined. Not in the card, not in `--help` output I could reach. »
    if unjudged {
        line.push_str(" · ");
        line.push_str(none);
        line.push_str(
            " = not judged here · this file's model arrives at run time and admission judges it",
        );
    }
    t.paint(role, &line)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The persona's ops5 shape at this layer: valid, capacity fits, no
    /// blocker, and NOTHING dials.
    fn builtin_only() -> VerdictLayers {
        VerdictLayers::new(true, None, Vec::new(), true, Vec::new()).with_access_moot(true)
    }

    /// A file that DOES dial, whose model is only known at run time.
    fn run_time_model() -> VerdictLayers {
        VerdictLayers::new(true, None, Vec::new(), true, Vec::new())
    }

    /// a persona wave (the operations sceptic), verbatim: « `run ready ○` on a
    /// file that then ran `RUN_RC=0`, 3/3 tasks green. No blocker is
    /// named, no flag flips it, `--access mock` changes nothing. The
    /// readiness line is a verdict the run contradicts. »
    #[test]
    fn a_file_that_never_dials_is_run_ready_and_its_access_reads_n_a() {
        let layers = builtin_only();
        assert_eq!(
            layers.run_ready(),
            Some(true),
            "nothing dials, so the three answered layers decide"
        );
        let line = layers_line(&layers, Theme::new(false, false, false));
        assert!(
            line.contains("access ready n/a · nothing dials"),
            "the question has no subject, and the line says which: {line}"
        );
        assert!(line.contains("run ready ✔"), "{line}");
        assert!(
            !line.contains('○'),
            "no unjudged glyph on a question nobody asked: {line}"
        );
    }

    /// The other shape keeps `○` — and the line DEFINES it, once, where
    /// it appears. The operations sceptic: « `○` is never defined. Not in the card, not in
    /// `--help` output I could reach. »
    #[test]
    fn an_unanswered_access_keeps_the_circle_and_defines_it_in_place() {
        let layers = run_time_model();
        assert_eq!(layers.run_ready(), None, "genuinely unknown here");
        let line = layers_line(&layers, Theme::new(false, false, false));
        assert!(
            line.contains("access ready ○") && line.contains("run ready ○"),
            "{line}"
        );
        assert!(
            line.contains("○ = not judged here")
                && line.contains("arrives at run time")
                && line.contains("admission judges it"),
            "the glyph is defined where a reader has it on screen: {line}"
        );
        // ASCII terminals get the definition of THEIR glyph, not of one
        // they cannot render.
        let ascii = layers_line(&layers, Theme::new(false, true, false));
        assert!(
            ascii.contains("- = not judged here") && !ascii.contains('○'),
            "{ascii}"
        );
    }

    /// MOOT is a premise, never an override: a refusal anywhere still
    /// settles RUN READY false.
    #[test]
    fn moot_access_never_clears_a_real_blocker() {
        let invalid =
            VerdictLayers::new(false, None, Vec::new(), true, Vec::new()).with_access_moot(true);
        assert_eq!(
            invalid.run_ready(),
            Some(false),
            "an invalid file is never ready"
        );
        let blocked = VerdictLayers::new(
            true,
            None,
            Vec::new(),
            true,
            vec!["capacity: mock/echo · thinking unsupported".to_owned()],
        )
        .with_access_moot(true);
        assert_eq!(blocked.run_ready(), Some(false), "a named blocker holds");
        let unfit =
            VerdictLayers::new(true, None, Vec::new(), false, Vec::new()).with_access_moot(true);
        assert_eq!(unfit.run_ready(), Some(false), "capacity still decides");
    }

    /// The default is the CONSERVATIVE reading: a caller that never
    /// declares the premise withholds the ✔, it does not invent one.
    #[test]
    fn the_default_withholds_rather_than_invents() {
        assert!(!VerdictLayers::default().access_moot);
        assert_eq!(VerdictLayers::default().run_ready(), None);
    }
}
