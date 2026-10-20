// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The steps of a run that settled, right below its story, when they tell what the Session's
//! summary of the run (« Terminé · … », its time, steps, cost, ceiling, produced files and next
//! gestures) does not: a step failed, or a model was called. Each step reads in the theme's own
//! state glyph (`✔` done, `✖` failed and why, `○` never run, `⊘` a gate the person answered
//! « no »…, a failure in the failure colour, never by colour alone), with its time, and its model
//! and cost where it called one, so the person sees which step cost what. No head line, no totals,
//! no files: the summary is the Session's, once (live gauntlet 5: two summaries of one run on the
//! same screen, one saying « ✔ Exécuté » after the person's « non »). A run with no model and no
//! failure has no card; the run object (F6) and its story (F2) keep the whole detail. Every line is
//! a fact the run's own frames declared; an unknown time, model or cost is left out. A long run
//! lists its first steps and every failure, then says how many it did not list.

use std::time::Duration;

use nika_display::run_story::RunState;
use nika_display::state::{TaskRow, TaskState};

use super::pulse::{file_name, run_money, took_words};
use crate::lang::Voice;
use crate::visual::state;
use crate::workspace::live::LiveRun;

/// The most steps a card lists, every failure aside.
const STEPS: usize = 12;

/// The steps of `leg` once it settled, in `voice`, when they say what the Session's summary
/// does not: a step failed, or a model was called. `None` before its settlement, while its
/// gate still asks, and for a run with no model and no failure.
pub(super) fn card(leg: &LiveRun, voice: Voice) -> Option<String> {
    let settled = leg
        .settlement()
        .filter(|settled| settled.state != RunState::Paused)?;
    // The steps in the order the run executed them, never the frames' alphabetical one.
    let rows = leg.rows_in_order();
    let failed = rows.iter().any(|row| row.state == TaskState::Failed);
    let spend = &settled.spend;
    let called = rows.iter().any(|row| row.model.is_some())
        || spend.priced_calls.saturating_add(spend.unpriced_calls) > 0;
    (failed || called).then(|| steps(leg, &rows, voice).join("\n"))
}

/// One line per step of `leg`, the first [`STEPS`] and every failure, then how many
/// were not listed.
fn steps(leg: &LiveRun, rows: &[&TaskRow], voice: Voice) -> Vec<String> {
    let mut listed = Vec::new();
    let mut omitted = 0_usize;
    for (at, row) in rows.iter().enumerate() {
        if at < STEPS || row.state == TaskState::Failed {
            listed.push(step(row, leg.reads_as(&row.id, row.state), voice));
        } else {
            omitted += 1;
        }
    }
    if omitted > 0 {
        listed.push(format!(
            "… {}",
            voice.count(omitted, "more step not listed")
        ));
    }
    listed
}

/// A step: its glyph and id, its wall time, its model and its cost when
/// known, why it failed, that it never ran, or that the person answered its
/// gate no (`reads`, the state it reads in: the run's stop, never done).
fn step(row: &TaskRow, reads: TaskState, voice: Voice) -> String {
    let mut facts = vec![format!("{} {}", state::cell(reads, voice.ascii).0, row.id)];
    facts.extend(
        row.wall_ms()
            .map(|ms| took_words(Duration::from_millis(ms))),
    );
    facts.extend(
        row.model
            .as_deref()
            .map(|model| file_name(model).to_owned()),
    );
    facts.extend(row.cost_usd.filter(|usd| *usd > 0.0).map(run_money));
    match row.state {
        TaskState::Failed => {
            let why = row.detail.lines().next().unwrap_or_default().trim();
            facts.push(if why.is_empty() {
                voice.say("failed").to_owned()
            } else {
                voice.say("failed: {why}").replace("{why}", why)
            });
        }
        TaskState::Skipped => facts.push(voice.say("skipped").to_owned()),
        TaskState::Cancelled | TaskState::Pending => facts.push(voice.say("not run").to_owned()),
        _ if reads != row.state => facts.push(voice.say("answered no").to_owned()),
        _ => {}
    }
    facts.join(" · ")
}

#[cfg(test)]
mod tests;
