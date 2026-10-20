// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The keys of what is on screen now, read over the frame with `?` in the
//! full-words reader's frame ([`super::diagnostic`]): the surface's own keys
//! as its hint row names them, the keys it reads that the row has no room
//! for, the keys while Nika works, then the shell's keys and the view keys
//! the palette offers now, from the same catalog ([`super::catalog`]).
//! Nothing here sends, answers or grants; `Esc`, `Enter` or `?` closes it.

use crate::composer::Composer;
use crate::composer::answer::choice;
use crate::composer::chooser::{Act, Entry};
use crate::model::{Shape, UiState, Waiting};

/// Keys a conversation at the free prompt reads that its hint row leaves out.
const CONVERSATION: &[&str] = &[
    "Enter · send what you wrote",
    "Alt+Enter · a new line",
    "↑↓ · earlier lines, at the edges of the box",
    "/ · the commands, as you type one",
    "Tab · the commands, in an empty box",
    "Home End · the first and the latest messages (Ctrl+Home over words typed)",
];

/// The intelligence list's own keys.
const SETTINGS: &[&str] = &[
    "↑↓ PgUp PgDn Home End · choose a row",
    "Enter · apply the selected row",
    "type · filter the rows; a sentence no row matches is sent as a message",
    "Esc · clear the filter, then go back or keep the current choice",
    "1, 2 or 3 with a name, 4, /intelligence … · the expert route, typed",
];

/// A question's own keys.
const QUESTION: &[&str] = &[
    "↑↓ PgUp PgDn Home End · choose an answer",
    "1-9 · select that answer, while nothing is typed",
    "Enter · answer with your own words, else the selection",
    "Esc · clear your words, then the selection; it never answers",
];

/// A multiple choice's keys besides a question's.
const MULTIPLE: &[&str] = &[
    "Space · check or uncheck the selected answer",
    "Ctrl+A · check every answer, or none when all are",
    "Enter · validate every checked answer",
];

/// A decision's keys besides its hint row.
const DECISION: &[&str] = &[
    "↑↓ · choose an act; Enter does it, once",
    "Esc · never answers a decision",
    "F2 · the decision's whole words",
];

/// A decision list's digits, where its acts are listed.
const ACT_DIGITS: &str = "1-9 · select that act, while nothing is typed; Enter does it";

/// The keys that hold whatever is on screen, before the view keys the
/// palette offers (`?` among them).
const ALWAYS: &[&str] = &[
    "Ctrl+C · stop or interrupt; twice leaves",
    "Ctrl+O · every command and view key",
];

/// The words of the keys overlay for `state` and `composer`: the surface on
/// screen and its keys, then the keys that always hold and the view keys
/// `offered` now. `can_stop`: the turn under way holds a Stop.
pub(super) fn words(
    state: &UiState,
    composer: &Composer,
    offered: &[Entry],
    can_stop: bool,
) -> String {
    let voice = state.voice();
    let (name, own) = surface(state, composer, can_stop);
    let mut lines = vec![voice.say(name).to_owned()];
    lines.extend(own.iter().map(|line| format!("  {}", voice.say(line))));
    lines.push(String::new());
    lines.push(voice.say("Always").to_owned());
    lines.extend(ALWAYS.iter().map(|line| format!("  {}", voice.say(line))));
    let views = (offered.iter()).filter(|entry| matches!(entry.act, Act::Press(_)));
    lines.extend(views.map(|entry| format!("  {} · {}", entry.name, voice.say(&entry.effect))));
    lines.join("\n")
}

/// The surface on screen, named, and the keys it reads: its hint row's own
/// words first, as the row says them, then each key it reads in words.
fn surface(state: &UiState, composer: &Composer, can_stop: bool) -> (&'static str, Vec<String>) {
    let lines = |hint: &str, more: &[&[&str]]| {
        let mut lines = vec![hint.to_owned()];
        lines.extend(
            more.iter()
                .flat_map(|part| part.iter())
                .map(|line| (*line).to_owned()),
        );
        lines
    };
    if state.busy.is_some() {
        let stop = if can_stop {
            "Ctrl+C · stop the preparation; again leaves"
        } else {
            "Ctrl+C · a Run is never stopped here; twice leaves"
        };
        let working = [
            stop,
            "type · your words wait in the box",
            "Enter · words as a correction to the preparation",
            "PgUp PgDn · scroll, on a full screen",
        ];
        return ("While Nika works", working.map(str::to_owned).to_vec());
    }
    let waiting = &state.waiting;
    let hint = waiting.hint();
    match waiting {
        Waiting::Free => ("Conversation", lines(hint, &[CONVERSATION])),
        Waiting::Selection { .. } => ("Intelligence list", lines(hint, &[SETTINGS])),
        Waiting::QuestionDocument { asked, .. } | Waiting::ProposalChoice { asked, .. }
            if choice(waiting).is_some() =>
        {
            let multiple = matches!(asked.shape, Shape::Multiple(_));
            let mut own = lines(hint, &[QUESTION]);
            if multiple {
                own.extend(MULTIPLE.iter().map(|line| (*line).to_owned()));
            }
            let offers = choice(waiting).map_or(0, |(_, _, offers)| offers.len());
            if composer.offer_rows(waiting).contains(&(offers + 1)) {
                own.push("Let Nika choose · leave the choice to Nika, in words".to_owned());
            }
            ("Question", own)
        }
        Waiting::Action { .. } => {
            let mut own = lines(hint, &[DECISION]);
            own.insert(2, ACT_DIGITS.to_owned());
            ("Decision", own)
        }
        Waiting::Knowledge { .. } | Waiting::Proposal | Waiting::Gate => {
            ("Decision", lines(hint, &[DECISION]))
        }
        _ => ("Question", lines(hint, &[])),
    }
}

#[cfg(test)]
mod tests;
