// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session's own protocol lines, answered before any cost review, route or reading (V9
//! P1 · BUG-U6, S02 and S05b of the 2026-09-27 black-box audit). A command-shaped line — `/`,
//! then a name, never a path — belongs to a closed namespace: a command the waiting state
//! serves answers there; any other is refused here, with the known names it nearly spells, and
//! is never read as work, an answer, a consent or a gate's answer. The read-only asks — the
//! read-only commands, « why? » beside what waits, « what happened? » — are answered from the
//! machine's own state. None of them calls a model, stages a cost review, binds an answer,
//! revises a proposal or discards what waits: the goal, the answers, the question, the
//! candidate's bytes and the consent's identity stay exactly as they were.

use super::{SLASH_COMMANDS, SessionRuntime, TurnOutcome, aside};
use crate::authoring::{is_cancel, is_what_happened, is_why};
use crate::outcome::{Refusal, RefusalClass};

/// Commands served beyond the help card's list, then those a turn serves after its read-only lines.
const ALSO_KNOWN: &[&str] = &["/exit", "/restore", "/cancel", "/last"];
const TURN_SERVED: &[&str] = &["/quit", "/exit", "/intelligence"];

/// What every refused command says last.
const UNCHANGED: &str = "`/help` lists the commands · nothing was sent, nothing changed";

/// Beside a question a turn owns (an authoring round's, a run input's, an activation's).
const QUESTION_WAITS: &str =
    "the question still waits · reply on the next line · `cancel` drops it";

/// The command a line reads as: `/`, then a first word naming no path (`/tmp/a.csv`,
/// `/notes.md`, `/Users/me` are paths), its trailing punctuation aside. `None` otherwise.
pub(super) fn command_word(line: &str) -> Option<&str> {
    let first = line.split_whitespace().next()?;
    let word = first.trim_end_matches(['?', '!', '.', ',', ';', ':']);
    (!word.strip_prefix('/')?.contains(['/', '.', '\\'])).then_some(word)
}

/// The refusal of a command-shaped line the waiting state did not serve: a known command typed
/// with anything after it or where it does not apply, else an unknown name with the known ones
/// it begins or nearly spells (case aside). `None` for any other line.
pub(super) fn unserved_command(line: &str) -> Option<String> {
    let line = line.trim();
    let word = command_word(line)?;
    let known = || SLASH_COMMANDS.iter().chain(ALSO_KNOWN).copied();
    let said = if word == "/" {
        "`/` needs a command name after it".to_owned()
    } else if known().any(|k| k == word) && line == word {
        format!("`{word}` does not apply here")
    } else if known().any(|k| k == word) {
        format!("type `{word}` alone — it takes nothing after it")
    } else {
        let typed = word[1..].to_lowercase();
        let near: Vec<String> = known()
            .filter(|k| k[1..].starts_with(&typed) || edits(&typed, &k[1..]) <= 2)
            .map(|k| format!("`{k}`"))
            .take(3)
            .collect();
        if near.is_empty() {
            format!("unknown command `{word}`")
        } else {
            format!(
                "unknown command `{word}` — did you mean {}?",
                near.join(" or ")
            )
        }
    };
    Some(format!("{said} · {UNCHANGED}"))
}

/// The edit distance between two short names (insertions, deletions, substitutions).
fn edits(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == *cb {
                diagonal
            } else {
                1 + diagonal.min(above).min(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

impl SessionRuntime {
    /// A read-only line at a prompt of its own (the proposal, a gate, the first screen), after
    /// the commands that prompt serves: « what happened? » is said, a command refused, beside
    /// what waits — which keeps waiting (`waits` names it). `None` for any other line.
    pub(super) fn beside(&self, line: &str, waits: &str) -> Option<TurnOutcome> {
        if is_what_happened(line) {
            let said = self
                .last_recovery
                .clone()
                .unwrap_or_else(|| crate::facts::last_run(&self.snapshot.root));
            return Some(TurnOutcome::Aside(format!("{said}\n  {waits}")));
        }
        let text = unserved_command(line)?;
        Some(TurnOutcome::Refusal(Refusal::new(
            RefusalClass::WrongState,
            format!("{text}\n  {waits}"),
        )))
    }

    /// A turn's read-only lines, answered before a proposal a host left waiting is discarded
    /// and before anything else reads them: the read-only commands, « what happened? » (the
    /// last card from memory, else the last run from its trace), and — when no question owns
    /// the line — a command the turn does not serve. `None` for any other line.
    pub(super) fn read_only_turn(&mut self, input: &str) -> Option<TurnOutcome> {
        let owned =
            self.authoring.is_some() || self.run_inputs.is_some() || self.activation.is_some();
        Some(match input {
            "/help" => TurnOutcome::Help(self.help_card()),
            "/status" => TurnOutcome::Facts(self.status()),
            "/why" => self.explain_pending(),
            "/meaning" => self.meaning_unrecorded(),
            "/proof" => self.proof_unrecorded(),
            "/details" => TurnOutcome::Facts(self.details()),
            _ if is_what_happened(input) => match self.last_recovery() {
                Some(card) => card,
                None if owned => self.beside(input, QUESTION_WAITS)?,
                None => TurnOutcome::Facts(crate::facts::last_run(&self.snapshot.root)),
            },
            _ if owned || TURN_SERVED.contains(&input) => return None,
            _ => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                unserved_command(input)?,
            )),
        })
    }

    /// An open authoring question's own protocol, before any cost review, route or reading:
    /// « why? » explains it, a cancel word drops the round (`drop` answers a clause's
    /// disposition instead), a command-shaped line is refused. Nothing binds; `None` otherwise.
    pub(super) fn question_protocol(&mut self, line: &str) -> Option<TurnOutcome> {
        let round = self.authoring.as_ref()?;
        if is_why(line) {
            return Some(self.explain_pending());
        }
        let disposes = line.trim().eq_ignore_ascii_case("drop")
            && round.current().is_some_and(|q| q.key.starts_with("gap."));
        if is_cancel(line) && !disposes {
            let asked = self.question_id_of(round);
            self.questions.close(asked);
            self.authoring = None;
            self.intent.unresolved.clear();
            self.remember(line, "(authoring discarded)");
            return Some(TurnOutcome::Facts(
                "authoring discarded · nothing was written · describe the work again when ready"
                    .to_owned(),
            ));
        }
        let text = unserved_command(line)?;
        Some(TurnOutcome::Refusal(Refusal::new(
            RefusalClass::WrongState,
            format!("{text}\n  {QUESTION_WAITS}"),
        )))
    }

    /// `/why` — the aside for whatever waits: an authoring question, a
    /// declared input, a gate, a proposal; a fact when nothing waits.
    pub(super) fn explain_pending(&self) -> TurnOutcome {
        if let Some(round) = &self.authoring
            && let Some(question) = round.current()
        {
            return TurnOutcome::Aside(aside::explain_question(question, round));
        }
        if let Some(inputs) = &self.run_inputs
            && let Some(name) = inputs.first_needed()
        {
            return TurnOutcome::Aside(aside::explain_input(
                inputs.workflow(),
                name,
                inputs.remaining(),
            ));
        }
        if let Some(gate) = &self.pending_gate {
            return TurnOutcome::Aside(aside::explain_gate(gate, &self.snapshot.root));
        }
        if let Some(set) = &self.pending {
            return TurnOutcome::Aside(format!(
                "{}\n(the proposal still waits · `yes` applies it · `no` discards it)",
                set.effects_fact()
            ));
        }
        TurnOutcome::Facts(
            "nothing waits for you right now · describe work, ask a fact, or `run …` an accepted workflow"
                .to_owned(),
        )
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
