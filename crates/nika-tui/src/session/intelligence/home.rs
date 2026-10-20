// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The picker's home screen and its two role lists. Home is short: the recommendation,
//! applied with one `Enter` where a first choice is required (it is selected then), the
//! current choice kept (selected when the picker is reopened, so one `Enter` keeps it),
//! every other choice, then the three roles always shown with their values (Author,
//! Decision, Run, each row opening its change), and keeping the current choice as the
//! default. Every value is read from the Session's snapshot or this host's own record of what
//! the person asked; nothing here applies anything.

use nika_session::intelligence::{IntelligenceKind, UserIntelligencePreference};
use nika_session::work::{self, ModelFacts, ValueRole};

use super::describe::{self, Marks};
use super::{Shown, groups, keys, line, names};
use crate::lang::{Voice, fill};
use crate::model::{Asked, Offer, Shape};

/// The Session's word that keeps the current choice and closes the picker.
pub(super) const KEEP: &str = "cancel";

/// The saved decision word for the author deciding alone.
const ALONE: &str = nika_session::intelligence::AUTHOR_ALONE;

/// Whether `facts` name the model the operator saved as the default, over the same route.
fn saved(facts: &ModelFacts, default: Option<&UserIntelligencePreference>) -> bool {
    let Some(default) = default else {
        return false;
    };
    let route = match &default.kind {
        IntelligenceKind::Api { provider } => ("api", provider.as_str()),
        IntelligenceKind::Harness { seat, .. } => ("harness", seat.as_str()),
        IntelligenceKind::Local { provider } => ("local", provider.as_str()),
        _ => return false,
    };
    route == (facts.class.as_str(), facts.via.as_str())
        && default
            .model
            .as_deref()
            .is_none_or(|model| model == facts.model)
}

/// The model this conversation uses, in words: the option the Session marks as current, else
/// the selection's own model or route.
fn current_name(intelligence: &work::Intelligence) -> Option<String> {
    let marked = (intelligence.options.iter()).find(|offer| offer.recommended);
    if let Some(facts) = marked.and_then(describe::facts) {
        return Some(names::display(&facts.model));
    }
    let selected = intelligence
        .selected
        .as_ref()
        .filter(|selected| selected.ready)?;
    (selected.model.as_deref().map(names::display)).or_else(|| selected.via.clone())
}

/// Who authors, in words, the picker's own in `voice`.
fn author_name(intelligence: &work::Intelligence, voice: Voice) -> String {
    let author = &intelligence.author;
    match author.kind.as_str() {
        "provider" | "harness" => (author.model.as_deref().map(names::display))
            .or_else(|| author.seat.clone())
            .unwrap_or_else(|| voice.say("the provider's model").to_owned()),
        "unavailable" => voice.say("unavailable here").to_owned(),
        _ => voice.say("none yet").to_owned(),
    }
}

/// The decision seat the snapshot names, its refusal said, or the author alone, in `voice`.
fn decision_name(intelligence: &work::Intelligence, voice: Voice) -> (String, String) {
    match &intelligence.decision {
        Some(seat) => {
            let name = names::display(&seat.model);
            let note = (seat.refusal.as_deref()).map_or_else(
                || voice.say("change · typed decisions").to_owned(),
                |why| fill(voice.say("change · unavailable: {why}"), &[("{why}", why)]),
            );
            (name, note)
        }
        None => (
            voice.say("the author decides alone").to_owned(),
            voice
                .say("change · no decision seat is consulted")
                .to_owned(),
        ),
    }
}

/// The model of the next workflows' AI steps: the one the person asked, else the model the
/// workflow in preparation binds, else each workflow's own.
fn run_name(work: &work::Work, shown: &Shown<'_>) -> (String, String) {
    let voice = shown.voice;
    if let Some(model) = shown.run {
        return (
            names::display(model),
            voice
                .say("change · requested for the next workflows")
                .to_owned(),
        );
    }
    let bound = (work.bindings.iter()).find(|binding| binding.role == ValueRole::RunModel);
    match bound {
        Some(binding) => (
            names::display(&binding.value),
            voice.say("change · this workflow's model").to_owned(),
        ),
        None => (
            voice.say("set by each workflow").to_owned(),
            voice.say("change · the model of the AI steps").to_owned(),
        ),
    }
}

/// The home screen over `asked`.
pub(super) fn home(
    asked: &mut Asked,
    work: &work::Work,
    intelligence: &work::Intelligence,
    shown: &Shown<'_>,
) {
    let voice = shown.voice;
    let mut offers = Vec::new();
    let mut preselect = None;
    let recommended = (intelligence.recommended.as_ref())
        .and_then(|key| intelligence.options.iter().find(|offer| &offer.key == key));
    if let Some(offer) = recommended {
        let (name, note) = match describe::facts(offer) {
            Some(facts) => {
                let marks = Marks {
                    current: offer.recommended,
                    default: saved(facts, shown.saved),
                    ..Marks::default()
                };
                let mut note = describe::note(offer, facts, marks, true, voice);
                let named =
                    keys::words(shown.keys, &facts.via, voice).filter(|_| facts.class == "api");
                if let Some(named) = named {
                    note = format!("{note} · {named}");
                }
                (names::display(&facts.model), note)
            }
            None => (offer.label.clone(), String::new()),
        };
        preselect = Some(offers.len());
        let label = fill(
            voice.say("Recommended for you · {name}"),
            &[("{name}", &name)],
        );
        offers.push(Offer::new(&offer.key, label).noted(note));
    }
    let current = current_name(intelligence);
    if !asked.mandatory {
        let words = current
            .clone()
            .unwrap_or_else(|| voice.say("this conversation's").to_owned());
        // Reopened, the list opens on the choice in use: one `Enter` keeps it.
        preselect = Some(offers.len());
        let label = fill(voice.say("Keep current · {name}"), &[("{name}", &words)]);
        offers.push(Offer::new(KEEP, label).noted(voice.say("closes this list, nothing changed")));
    }
    offers.push(
        Offer::new(line::CONNECTIONS, voice.say("Other choice…"))
            .noted(voice.say("every connection and model")),
    );
    let author = author_name(intelligence, voice);
    offers.push(
        Offer::new(
            line::AUTHOR,
            fill(voice.say("Author: {name}"), &[("{name}", &author)]),
        )
        .noted(voice.say("change · writes and repairs your workflows")),
    );
    let (decision, note) = decision_name(intelligence, voice);
    let label = fill(voice.say("Decision: {name}"), &[("{name}", &decision)]);
    offers.push(Offer::new(line::DECISION, label).noted(note));
    let (run, note) = run_name(work, shown);
    let label = fill(voice.say("Run: {name}"), &[("{name}", &run)]);
    offers.push(Offer::new(line::RUN, label).noted(note));
    if let Some(current) = current.filter(|_| !asked.mandatory) {
        offers.push(default_offer(intelligence, shown, &current));
    }
    // The current API connection's key can be checked once, when picked.
    let api = (intelligence.selected.as_ref())
        .filter(|selected| selected.ready && selected.kind == "api")
        .and_then(|selected| selected.via.as_deref());
    if let Some((provider, row)) =
        api.and_then(|provider| Some((provider, groups::test_row(provider, shown)?)))
    {
        let name = names::provider(provider);
        let label = fill(voice.say("Test the key · {name}"), &[("{name}", &name)]);
        offers.push(Offer::new(&row.key, label).noted(row.note));
    }
    offers.extend(groups::found_models(intelligence, voice));
    asked.label = voice.say("Intelligence for this conversation").into();
    asked.shape = Shape::Choice(offers);
    asked.back = None;
    asked.preselect = preselect;
}

/// « Set as default »: the choice in use, `current`, becomes the one new conversations start
/// with, or the row says it already is (the same model and decision seat).
fn default_offer(intelligence: &work::Intelligence, shown: &Shown<'_>, current: &str) -> Offer {
    let voice = shown.voice;
    let decides = intelligence
        .decision
        .as_ref()
        .map_or(ALONE, |seat| seat.model.as_str());
    let model = intelligence
        .selected
        .as_ref()
        .and_then(|selected| selected.model.as_deref());
    let kept = shown.saved.is_some_and(|default| {
        default.model.as_deref() == model && default.decision.as_deref() == Some(decides)
    });
    let note = if kept {
        voice.say("already the default").to_owned()
    } else {
        let start = voice.say("new conversations start with {name}");
        fill(start, &[("{name}", current)])
    };
    Offer::new(line::DEFAULT, voice.say("Set as default")).noted(note)
}

/// Who makes the typed decisions: Jev, as the host named it (unavailable with its reason
/// when it cannot be consulted), or the author alone; `Esc` goes back home.
pub(super) fn decision(asked: &mut Asked, intelligence: &work::Intelligence, shown: &Shown<'_>) {
    let voice = shown.voice;
    let consulted = intelligence.decision.is_some();
    let jev = match shown.jev {
        Some((model, None)) => names::display(model),
        Some((_, Some(why))) => fill(voice.say("unavailable: {why}"), &[("{why}", why)]),
        None => voice
            .say("unavailable: no decision seat is configured here")
            .to_owned(),
    };
    let current = |on: bool, words: &str| {
        if on {
            format!("{} · {words}", voice.say("current"))
        } else {
            words.to_owned()
        }
    };
    let held = |offer: Offer, on: bool| if on { offer.in_use() } else { offer };
    let alone = voice.say("no decision seat is consulted");
    let offers = vec![
        held(
            Offer::new(line::JEV, voice.say("Jev (typed decisions)"))
                .noted(current(consulted, &jev)),
            consulted,
        ),
        held(
            Offer::new(line::ALONE, voice.say("Author decides alone"))
                .noted(current(!consulted, alone)),
            !consulted,
        ),
    ];
    asked.label = voice.say("Who makes the typed decisions").into();
    asked.shape = Shape::Choice(offers);
    asked.back = Some(line::HOME.to_owned());
}

/// The model for the next workflows' AI steps: each ready model once, whatever its route;
/// `Esc` goes back home.
pub(super) fn run(asked: &mut Asked, intelligence: &work::Intelligence, shown: &Shown<'_>) {
    let voice = shown.voice;
    let mut offers: Vec<Offer> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for offer in &intelligence.options {
        let Some(facts) = describe::facts(offer).filter(|facts| facts.configured) else {
            continue;
        };
        if seen.contains(&facts.model.as_str()) {
            continue;
        }
        seen.push(&facts.model);
        let marks = Marks {
            requested: shown.run == Some(facts.model.as_str()),
            ..Marks::default()
        };
        let requested = marks.requested;
        let row = Offer::new(
            format!("{}{}", line::RUN_MODEL, facts.model),
            names::display(&facts.model),
        )
        .noted(describe::note(offer, facts, marks, false, voice));
        offers.push(if requested { row.in_use() } else { row });
    }
    if offers.is_empty() {
        offers.push(
            Offer::new(line::HOME, voice.say("No ready model here")).noted(voice.say("back home")),
        );
    }
    asked.label = voice
        .say("Model for the AI steps of the next workflows")
        .into();
    asked.shape = Shape::Choice(offers);
    asked.back = Some(line::HOME.to_owned());
}
