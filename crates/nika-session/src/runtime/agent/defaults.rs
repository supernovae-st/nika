// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A choice a proposal shows, settled with the default its candidate already carries, needs no
//! revision: the Session binds it as an answer to the proposal (`offered`, by the person's own
//! line) and the proposal shown keeps waiting unchanged, its identity, bytes and card those the
//! person saw: its next choice, or its save decision once none remain, comes with no author round
//! trip. Any other line, another option, a change or a proposal that still needs its author
//! ([`Conversation::keeps_default`]) keeps the author's turn.

use std::collections::BTreeMap;

use nika_session_agent::conversation::Conversation;

use super::{Driver, QueuedState, SessionRuntime, TurnOutcome, refusal, unix_ms};
use crate::change::ProjectChangeSet;
use crate::outcome::{QuestionId, RefusalClass};

/// What Nika tells the author beside the person's line it settled this way.
const KEPT: &str = "The candidate already carries this default, so Nika kept the proposal shown \
                    unchanged and showed the person its next choice, or its save decision once \
                    none remain: nothing was revised, saved or run.";

impl SessionRuntime {
    /// The proposal shown keeps waiting with the default `line` picked for its choice `id`
    /// bound, when nothing else needs the author; `None` keeps the author's turn. Only the
    /// Session's own loop is spared, which reads this tree as its transcript: a seat's agent
    /// keeps its own session, so every line still reaches it.
    pub(super) fn keep_default(&mut self, id: &QuestionId, line: &str) -> Option<TurnOutcome> {
        self.reasoner.agent_model()?;
        let set = self.pending.clone()?;
        let beside = (self.pending_trigger.as_ref()).and_then(|t| t.timezone.clone());
        let mut driver = self.agent.take()?;
        let kept = kept(&mut driver, &set, (id, line), beside.as_deref());
        self.agent = Some(driver);
        Some(match kept? {
            Ok((label, left)) => TurnOutcome::Facts(kept_words(&label, left, self.language)),
            Err(why) => refusal(
                RefusalClass::IntelligenceRefused,
                match self.language {
                    crate::Lang::Fr => format!(
                        "la conversation n'a pas pu continuer : {why} · ce qui a été dit est gardé"
                    ),
                    _ => format!("the conversation could not go on: {why} · what was said is kept"),
                },
            ),
        })
    }
}

/// The default `line` picks for the shown choice `id` of `set`, bound and recorded in the tree as
/// the person's reply to the proposal, beside what Nika read of it: the label of the offer kept
/// and how many choices remain open. `None` when the conversation does not keep it this way.
fn kept(
    driver: &mut Driver,
    set: &ProjectChangeSet,
    (id, line): (&QuestionId, &str),
    beside: Option<&str>,
) -> Option<Result<(String, usize), String>> {
    if driver.damaged.is_some() {
        return None;
    }
    let shown = driver.shown(Some(set))?.to_vec();
    if (driver.steering.records().iter()).any(|queued| queued.state == QueuedState::Waiting) {
        return None;
    }
    let parked = match driver.tree.as_ref()?.parked() {
        None => None,
        Some((call, "propose")) => Some(call.to_owned()),
        Some(_) => return None,
    };
    let bytes = (set.changes.iter())
        .find(|change| change.is_workflow())
        .map(|change| change.content().to_owned())?;
    let keeps = driver.toolbox.with(|c| {
        c.keeps_default(id, line, (&shown, beside))
            && c.candidate()
                .is_some_and(|candidate| candidate.source == bytes)
    });
    if !keeps {
        return None;
    }
    let tree = driver.tree.as_mut()?;
    let cite = tree.next_cite();
    let (said, label, left) = driver.toolbox.with(|c| {
        let label = default_label(c, id);
        let said = c.answered_one_by(id, &cite, line, &BTreeMap::new());
        (said, label, c.open().len())
    });
    let read: Vec<String> = said.iter().map(|line| format!("- {line}")).collect();
    let note = format!("Its reading of this answer:\n{}\n\n{KEPT}", read.join("\n"));
    let store = &mut driver.store;
    let appended = tree.append_user_read(line, (parked, None), Some(note), unix_ms(), |entry| {
        nika_session_agent::Store::append(&mut *store, entry)
    });
    Some(match appended {
        Ok((_, at)) if at == cite => Ok((label, left)),
        Ok((_, at)) => Err(format!("the answer was cited {cite} but recorded as {at}")),
        Err(error) => Err(error.to_string()),
    })
}

/// The label of the default the shown choice `id` recommends, else its key.
fn default_label(conversation: &Conversation, id: &QuestionId) -> String {
    let question = (conversation.open().iter()).find(|question| question.id == *id);
    let offer = question.and_then(|question| question.options.iter().find(|o| o.recommended));
    offer.map_or_else(String::new, |offer| {
        let label = offer.label.trim();
        if label.is_empty() {
            offer.key.clone()
        } else {
            label.to_owned()
        }
    })
}

/// What the person reads once the default is kept, in their language: the next choice
/// follows, or the save decision.
fn kept_words(label: &str, left: usize, lang: super::super::Lang) -> String {
    match (left == 0, lang) {
        (true, super::super::Lang::Fr) => format!(
            "défaut gardé · {label}\n(la proposition attend toujours · « oui » l'applique · « non » l'écarte)"
        ),
        (false, super::super::Lang::Fr) => format!(
            "défaut gardé · {label}\n(la proposition attend toujours · son choix suivant vient · « oui » garde chaque défaut)"
        ),
        (true, _) => format!(
            "kept the default · {label}\n(the proposal still waits · `yes` applies it · `no` discards it)"
        ),
        (false, _) => format!(
            "kept the default · {label}\n(the proposal still waits · its next choice follows · `yes` keeps every default)"
        ),
    }
}
