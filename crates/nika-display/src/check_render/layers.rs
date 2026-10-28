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
    /// The tasks whose own effects can reach the network (a dialed
    /// `net.http` host, an MCP tool, a paid media call), sorted. ACCESS
    /// READY judges model routes only: these are judged on the PERMITS
    /// row, and a mock model stubs none of them (#1605).
    pub network_tasks: Vec<String>,
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
            network_tasks: Vec::new(),
        }
    }

    /// Name the seat-served models (W3-F4).
    #[must_use]
    pub fn with_seat_served(mut self, models: Vec<String>) -> Self {
        self.seat_served = models;
        self
    }

    /// Name the tasks whose own effects can reach the network (see
    /// [`Self::network_tasks`]).
    #[must_use]
    pub fn with_network_tasks(mut self, tasks: Vec<String>) -> Self {
        self.network_tasks = tasks;
        self
    }

    /// Declare the ACCESS question MOOT — no `infer:`/`agent:` task, so no
    /// model route to judge (see [`Self::access_moot`]); a task's own
    /// effects can still reach the network ([`Self::network_tasks`]). The
    /// caller that holds the workflow is the only one that can know it.
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
        moot_access(&layers.network_tasks)
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

/// The MOOT access answer: no `infer:`/`agent:` task, so no model route to
/// judge. It never says the workflow stays off the network (#1605: it read
/// « nothing dials » over a `nika:fetch` that dialed): a task whose own
/// effects can reach it is named beside the row that judges it.
fn moot_access(network_tasks: &[String]) -> String {
    if network_tasks.is_empty() {
        return "n/a · no model route".to_owned();
    }
    format!(
        "n/a · no model route · {} can reach the network (PERMITS row)",
        named_tasks(network_tasks)
    )
}

/// `` `a` · `b` · `c` · +2 more `` — the first three task ids a line names,
/// in the order given, and how many more: the one-line verdict stays one
/// line on a file with many fetches (the JSON lists them all).
fn named_tasks(tasks: &[String]) -> String {
    let mut named: Vec<String> = (tasks.iter().take(3))
        .map(|task| format!("`{task}`"))
        .collect();
    if tasks.len() > 3 {
        named.push(format!("+{} more", tasks.len() - 3));
    }
    named.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The persona's ops5 shape at this layer: valid, capacity fits, no
    /// blocker, and no model route at all.
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
            "no model route, so the three answered layers decide"
        );
        let line = layers_line(&layers, Theme::new(false, false, false));
        assert!(
            line.contains("access ready n/a · no model route · capacity fit"),
            "the question has no subject, and the line says which: {line}"
        );
        assert!(line.contains("run ready ✔"), "{line}");
        assert!(
            !line.contains('○'),
            "no unjudged glyph on a question nobody asked: {line}"
        );
    }

    /// #1605 · a builtin-only file whose fetch dials: the moot line said
    /// « nothing dials » over it. It now says only that no model route
    /// exists, and names the tasks that can still reach the network beside
    /// the row that judges them.
    #[test]
    fn a_moot_access_names_what_can_still_reach_the_network() {
        let layers = builtin_only().with_network_tasks(vec!["grab".to_owned(), "post".to_owned()]);
        assert_eq!(layers.run_ready(), Some(true), "the premise is unchanged");
        let line = layers_line(&layers, Theme::new(false, false, false));
        assert!(
            line.contains(
                "access ready n/a · no model route · `grab` · `post` can reach the network \
                 (PERMITS row) · capacity fit ✔"
            ),
            "{line}"
        );
        assert!(!line.contains("nothing dials"), "{line}");
        // A file with many fetches keeps a one-line verdict: three names
        // and how many more (twenty fetch tasks made a 611-column line).
        let many: Vec<String> = (1..=20).map(|n| format!("fetch_{n:02}")).collect();
        let line = layers_line(
            &builtin_only().with_network_tasks(many),
            Theme::new(false, false, false),
        );
        assert!(
            line.contains(
                "access ready n/a · no model route · `fetch_01` · `fetch_02` · `fetch_03` · +17 \
                 more can reach the network (PERMITS row) · capacity fit ✔"
            ) && !line.contains("fetch_04"),
            "{line}"
        );
        // The network clause rides only a moot answer: a judged access
        // keeps its glyph, and its rung says the rest.
        let judged = VerdictLayers::new(true, Some(true), Vec::new(), true, Vec::new())
            .with_network_tasks(vec!["grab".to_owned()]);
        let line = layers_line(&judged, Theme::new(false, false, false));
        assert!(
            line.contains("access ready ✔ · capacity fit") && !line.contains("`grab`"),
            "{line}"
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
