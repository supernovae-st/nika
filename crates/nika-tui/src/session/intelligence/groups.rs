// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Browse the complete owner-supplied catalogue by connection. Opening a group is local;
//! only one of its original offer keys can apply an intelligence through Session.

use super::describe::{self, Marks};
use super::{Shown, keys, line, names};
use crate::lang::{Voice, fill};
use crate::model::{Asked, Offer, Shape};
use nika_session::work;

/// The marks of an option: the Session marks the model this conversation
/// uses as its `recommended` option, and names the one it recommends. An
/// option naming no model is its connection's own (an agent account listing
/// no model of its own): it is in use while its connection is (`in_use`).
fn current(offer: &work::Offer, recommended: Option<&str>, in_use: Option<&str>) -> Marks {
    let whole = describe::facts(offer).is_none() && in_use == Some(offer.key.as_str());
    Marks {
        current: offer.recommended || whole,
        recommended: recommended == Some(offer.key.as_str()),
        ..Marks::default()
    }
}

/// The connection this conversation uses, by its key (`1 codex`, `2 anthropic`): the one of
/// the option the Session marks as the model in use, else the one its selection names while
/// that selection can serve here. An agent account reached with no model list of its own has
/// no option marked (live gauntlet r06 B14: « ( ) codex · Agent account » while codex
/// authored), so its selection names it.
fn in_use(intelligence: &work::Intelligence) -> Option<String> {
    let marked = (intelligence.options.iter()).find(|offer| offer.recommended);
    if let Some((key, _)) = marked.and_then(|offer| connection(&offer.key)) {
        return Some(key.to_owned());
    }
    let selected = (intelligence.selected.as_ref()).filter(|selected| selected.ready)?;
    let pick = match selected.kind.as_str() {
        "harness" => "1",
        "api" => "2",
        "local" => "3",
        _ => return None,
    };
    Some(format!("{pick} {}", selected.via.as_deref()?))
}

/// Of `listed` (every option of one or more connections), which rows say newest, by index:
/// each provider and access proposes the newest generation of the family its first listed
/// model is of, the first of equals, and a family is marked once in the list, on the newest
/// of those its connections propose (live gauntlet r07 B5: « newest `DeepSeek` » stood on three
/// connections' rows at once). None where the Session recommends a model of the connection
/// that proposes it (that one says so). A legacy model, a dated snapshot or one stating no
/// version is never the newest. The words say the family: `newest Grok`.
fn newest(listed: &[&work::Offer], recommended: Option<&str>) -> Vec<Option<String>> {
    let mut marked = vec![None; listed.len()];
    // What each provider proposes: the row, its family, its version, and whether the Session
    // recommends a model of that provider, which marks none.
    let mut proposed: Vec<(usize, String, (u32, u32), bool)> = Vec::new();
    let mut providers: Vec<(&str, &str)> = Vec::new();
    for offer in listed {
        if let Some(facts) = describe::facts(offer)
            && !providers.contains(&(facts.via.as_str(), facts.class.as_str()))
        {
            providers.push((facts.via.as_str(), facts.class.as_str()));
        }
    }
    for (via, class) in providers {
        let members: Vec<(usize, &work::ModelFacts)> = (listed.iter().enumerate())
            .filter(|(_, offer)| offer.key.contains('/'))
            .filter_map(|(at, offer)| Some((at, describe::facts(offer)?)))
            .filter(|(_, facts)| facts.via == via && facts.class == class)
            .collect();
        let recommends = (members.iter()).any(|(at, _)| {
            listed
                .get(*at)
                .is_some_and(|o| Some(o.key.as_str()) == recommended)
        });
        let Some(family) = (members.iter())
            .find_map(|(_, facts)| names::generation(&facts.model))
            .map(|generation| generation.family)
        else {
            continue;
        };
        let mut best: Option<(usize, (u32, u32))> = None;
        for (at, facts) in &members {
            let Some(generation) = names::generation(&facts.model) else {
                continue;
            };
            let Some(major) = generation
                .major
                .filter(|_| generation.family == family && !generation.dated && !facts.legacy)
            else {
                continue;
            };
            let version = (major, generation.minor);
            if best.is_none_or(|(_, top)| version > top) {
                best = Some((*at, version));
            }
        }
        if let Some((at, version)) = best {
            proposed.push((at, family, version, recommends));
        }
    }
    for (at, family, version, recommends) in &proposed {
        // The newest of the family among every proposal, the first listed of equals.
        let newest = (proposed.iter())
            .filter(|(_, other, ..)| other == family)
            .all(|(other_at, _, other, _)| other < version || (other == version && other_at >= at));
        if newest
            && !recommends
            && let Some(slot) = marked.get_mut(*at)
        {
            *slot = Some(family.clone());
        }
    }
    marked
}

/// `row` with the newest mark of the family `newest` names before its other facts, in `voice`.
fn noting_newest(mut row: Offer, newest: Option<&String>, voice: Voice) -> Offer {
    if let Some(family) = newest {
        let words = fill(voice.say("newest {family}"), &[("{family}", family)]);
        row.note = if row.note.is_empty() {
            words
        } else {
            format!("{words} · {}", row.note)
        };
    }
    row
}

fn setup_only<'a>(opened: &str, mut offers: impl Iterator<Item = (&'a str, &'a str)>) -> bool {
    opened.starts_with("2 ")
        && offers.all(|(key, label)| {
            !key.contains('/') && label.split(" · ").any(|part| part == "key required")
        })
}

pub(super) fn guidance(work: &work::Work, opened: &str) -> &'static str {
    let offers = work
        .intelligence
        .iter()
        .flat_map(|intelligence| intelligence.options.iter())
        .filter(|offer| connection(&offer.key).is_some_and(|(key, _)| key == opened))
        .map(|offer| (offer.key.as_str(), offer.label.as_str()));
    if setup_only(opened, offers) {
        "Configure the API key named below, then reopen Nika. No selection has changed."
    } else {
        "Choose a model below. No selection has changed."
    }
}

fn connection(key: &str) -> Option<(&str, &str)> {
    let (pick, _) = key.split_once(' ')?;
    let access = match pick {
        "1" => "Agent account",
        "2" => "API",
        "3" => "Local",
        _ => return None,
    };
    Some((
        key.split_once('/').map_or(key, |(prefix, _)| prefix),
        access,
    ))
}

/// The connection a picker line names after `prefix`, when this inventory offers it.
pub(super) fn requested_after(work: &work::Work, said: &str, prefix: &str) -> Option<String> {
    let asked = said.strip_prefix(prefix)?;
    work.intelligence
        .as_ref()?
        .options
        .iter()
        .find_map(|offer| {
            let (key, _) = connection(&offer.key)?;
            (key == asked).then(|| key.to_owned())
        })
}

pub(super) fn requested(work: &work::Work, said: &str) -> Option<String> {
    requested_after(work, said, line::MODELS)
}

/// The row that checks `provider`'s key once when picked, while a key is set and this host
/// can check it: the variable that supplies it, then the last verdict or what the check costs.
pub(super) fn test_row(provider: &str, shown: &Shown<'_>) -> Option<Offer> {
    let voice = shown.voice;
    let named = keys::words(shown.keys, provider, voice).filter(|_| shown.testable)?;
    // The verdict word is said in the picker's language already.
    let verdict = (shown.checked)
        .filter(|(checked, _)| *checked == provider)
        .map_or(voice.say("one free request, no model runs"), |(_, word)| {
            word
        });
    Some(
        Offer::new(
            format!("{}{provider}", line::TEST),
            voice.say("Test the key"),
        )
        .noted(format!("{named} · {verdict}")),
    )
}

/// Which of `listed` (one connection's options) fold behind « Older models »: a legacy model,
/// a dated snapshot, or a model whose family states a newer major among them. The model in use
/// and the recommended one never fold; a model stating no family or version stays.
fn folded(listed: &[&work::Offer], recommended: Option<&str>) -> Vec<bool> {
    let generations: Vec<Option<names::Generation>> = (listed.iter())
        .map(|offer| describe::facts(offer).and_then(|facts| names::generation(&facts.model)))
        .collect();
    let mut newest: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
    for generation in generations.iter().flatten() {
        if let Some(major) = generation.major {
            let top = newest.entry(generation.family.as_str()).or_insert(major);
            *top = (*top).max(major);
        }
    }
    (listed.iter().zip(&generations))
        .map(|(offer, generation)| {
            if offer.recommended || recommended == Some(offer.key.as_str()) {
                return false;
            }
            let legacy = describe::facts(offer).is_some_and(|facts| facts.legacy);
            let older = generation.as_ref().is_some_and(|generation| {
                generation.dated
                    || (generation.major)
                        .zip(newest.get(generation.family.as_str()))
                        .is_some_and(|(major, top)| major < *top)
            });
            legacy || older
        })
        .collect()
}

/// One connection's options, and which of them fold behind « Older models ».
fn grouped<'a>(intelligence: &'a work::Intelligence, opened: &str) -> Vec<(&'a work::Offer, bool)> {
    let listed: Vec<&work::Offer> = (intelligence.options.iter())
        .filter(|offer| connection(&offer.key).is_some_and(|(key, _)| key == opened))
        .collect();
    let folded = folded(&listed, intelligence.recommended.as_deref());
    listed.into_iter().zip(folded).collect()
}

/// One connection's current models, then one row opening its older ones when any fold, the
/// key check of an API connection, and the row back to every connection; a connection this
/// inventory no longer offers shows every connection instead.
pub(super) fn models(
    asked: &mut Asked,
    intelligence: &work::Intelligence,
    opened: &str,
    shown: &Shown<'_>,
) {
    let grouped = grouped(intelligence, opened);
    if grouped.is_empty() {
        connections(asked, intelligence, shown);
        return;
    }
    let voice = shown.voice;
    let recommended = intelligence.recommended.as_deref();
    let in_use = in_use(intelligence);
    let listed: Vec<&work::Offer> = grouped.iter().map(|(offer, _)| *offer).collect();
    let newest = newest(&listed, recommended);
    let mut offers: Vec<Offer> = (grouped.iter().zip(&newest))
        .filter(|((_, older), _)| !older)
        .map(|((offer, _), newest)| {
            let marks = current(offer, recommended, in_use.as_deref());
            let row = describe::row(offer, marks, false, voice);
            noting_newest(row, newest.as_ref(), voice)
        })
        .collect();
    let older = grouped.iter().filter(|(_, older)| *older).count();
    if older > 0 {
        let count = older.to_string();
        offers.push(
            Offer::new(
                format!("{}{opened}", line::OLDER),
                fill(voice.say("Older models ({older})…"), &[("{older}", &count)]),
            )
            .noted(voice.say("earlier generations, dated snapshots and legacy models"))
            .aside(),
        );
    }
    let setup_only = setup_only(
        opened,
        offers
            .iter()
            .map(|offer| (offer.key.as_str(), offer.label.as_str())),
    );
    asked.label = format!(
        "{} · {}",
        voice.say(if setup_only {
            "Configure API"
        } else {
            "Choose a model"
        }),
        opened.split_once(' ').map_or(opened, |(_, name)| name)
    );
    if let Some(provider) = opened.strip_prefix("2 ") {
        offers.extend(test_row(provider, shown).map(Offer::aside));
    }
    offers.push(Offer::new(line::CONNECTIONS, voice.say("All connections")).aside());
    asked.back = Some(line::CONNECTIONS.to_owned());
    asked.shape = Shape::Choice(offers);
}

/// Every model of every connection, listed only while typed words find it: a model can be
/// reached by its name from home or from every connection, and applied by its own key. Each
/// provider marks one: the model the Session recommends, else its newest.
pub(super) fn found_models(intelligence: &work::Intelligence, voice: Voice) -> Vec<Offer> {
    let recommended = intelligence.recommended.as_deref();
    let listed: Vec<&work::Offer> = (intelligence.options.iter())
        .filter(|offer| describe::facts(offer).is_some())
        .collect();
    let newest = newest(&listed, recommended);
    (listed.iter().zip(&newest))
        .map(|(offer, newest)| {
            let row = describe::row(offer, current(offer, recommended, None), true, voice);
            noting_newest(row, newest.as_ref(), voice).found_only()
        })
        .collect()
}

/// One connection's older models (earlier generations, dated snapshots, legacy), then the row
/// back to its current ones; `Esc` goes back there too.
pub(super) fn older(
    asked: &mut Asked,
    intelligence: &work::Intelligence,
    opened: &str,
    shown: &Shown<'_>,
) {
    let voice = shown.voice;
    let recommended = intelligence.recommended.as_deref();
    let in_use = in_use(intelligence);
    let mut offers: Vec<Offer> = (grouped(intelligence, opened).into_iter())
        .filter(|(_, older)| *older)
        .map(|(offer, _)| {
            let marks = current(offer, recommended, in_use.as_deref());
            describe::row(offer, marks, false, voice)
        })
        .collect();
    if offers.is_empty() {
        models(asked, intelligence, opened, shown);
        return;
    }
    let back = format!("{}{opened}", line::MODELS);
    let name = opened.split_once(' ').map_or(opened, |(_, name)| name);
    asked.label = fill(voice.say("Older models · {name}"), &[("{name}", name)]);
    offers.push(Offer::new(&back, voice.say("Current models")).aside());
    asked.back = Some(back);
    asked.shape = Shape::Choice(offers);
}

/// Within each access (agent accounts, APIs, local servers), the connections usable now first,
/// each access keeping the places its rows hold and every other order as it was (live gauntlet,
/// 16 October: the one API whose key was present stood 12th, under eleven « key required »).
fn ready_first(rows: &mut [(Offer, Option<&str>, bool)]) {
    let mut accesses: Vec<&str> = Vec::new();
    for access in rows.iter().filter_map(|(_, access, _)| *access) {
        if !accesses.contains(&access) {
            accesses.push(access);
        }
    }
    for access in accesses {
        let places: Vec<usize> = (rows.iter().enumerate())
            .filter(|(_, (_, of, _))| *of == Some(access))
            .map(|(at, _)| at)
            .collect();
        let mut held: Vec<(Offer, Option<&str>, bool)> =
            places.iter().map(|at| rows[*at].clone()).collect();
        // Stable: the usable rows first, each side in the order it had.
        held.sort_by_key(|(_, _, usable)| !usable);
        for (at, row) in places.into_iter().zip(held) {
            rows[at] = row;
        }
    }
}

/// Every connection, the model in use first and the connection in use marked as models are;
/// `Esc` goes back home.
pub(super) fn connections(asked: &mut Asked, intelligence: &work::Intelligence, shown: &Shown<'_>) {
    let options = &intelligence.options;
    let voice = shown.voice;
    let mut offers = Vec::new();
    asked.label = voice.say("Choose an AI connection").into();
    // The current model leads, so the list opens on it: `Enter` keeps it.
    let recommended = intelligence.recommended.as_deref();
    let in_use = in_use(intelligence);
    offers.extend(
        options
            .iter()
            .filter(|offer| offer.recommended)
            .map(|offer| describe::row(offer, current(offer, recommended, None), true, voice)),
    );
    let mut seen = Vec::new();
    // Each connection's row, the access it is reached by and whether it is usable now.
    let mut rows: Vec<(Offer, Option<&str>, bool)> = Vec::new();
    for offer in options {
        let Some((key, access)) = connection(&offer.key) else {
            rows.push((Offer::new(&offer.key, &offer.label), None, true));
            continue;
        };
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let provider = key.split_once(' ').map_or(key, |(_, name)| name);
        let group = options
            .iter()
            .filter(|offer| connection(&offer.key).is_some_and(|(candidate, _)| candidate == key))
            .collect::<Vec<_>>();
        // The models its own list shows, counted as that list's title counts them, and those
        // that fold behind its « Older models » row, said apart (live gauntlet, 16 October:
        // « 375 models » opened on « 1-50 of 280 »).
        let listed = grouped(intelligence, key);
        let modelled = |older: bool| {
            (listed.iter())
                .filter(|(offer, folded)| *folded == older && offer.key.contains('/'))
                .count()
        };
        let (count, older) = (modelled(false), modelled(true));
        let mut models = match count {
            0 => String::new(),
            _ => format!(" · {}", voice.count(count, "model")),
        };
        if older > 0 {
            models = format!("{models} · {}", voice.count(older, "older"));
        }
        let needs_key = access == "API"
            && group
                .iter()
                .all(|offer| offer.label.split(" · ").any(|part| part == "key required"));
        // A seat that writes no workflow says so before any choice, as the Session's rows do.
        let no_authoring = nika_session::authoring::NO_AUTHORING;
        let writes_nothing = access == "Agent account"
            && (group.iter())
                .any(|offer| offer.label.split(" · ").any(|part| part == no_authoring));
        let requirement = if needs_key {
            format!(" · {}", voice.say("key required"))
        } else if writes_nothing {
            format!(" · {}", voice.say(no_authoring))
        } else {
            String::new()
        };
        let named = (access == "API")
            .then(|| keys::words(shown.keys, provider, voice))
            .flatten()
            .unwrap_or_default();
        let class = match key.split_once(' ').map(|(pick, _)| pick) {
            Some("1") => "harness",
            Some("3") => "local",
            _ => "api",
        };
        // The connection in use reads as models do: « current » first, and its radio filled
        // unless a model of it leads the list, which holds the list's one radio.
        let used = in_use.as_deref() == Some(key);
        let note = match (used, named.is_empty()) {
            (true, true) => voice.say("current").to_owned(),
            (true, false) => format!("{} · {named}", voice.say("current")),
            (false, _) => named,
        };
        let row = Offer::new(
            format!("{}{key}", line::MODELS),
            format!("{provider} · {}{models}{requirement}", voice.say(access)),
        )
        .noted(note)
        .grouped(describe::group(provider, class));
        let row = if used { row.in_use() } else { row };
        // Usable now, as the Session reports it: an API whose key is present, an account or
        // a local server it detected that can write workflows. Nothing is probed here.
        rows.push((row, Some(access), !needs_key && !writes_nothing));
    }
    ready_first(&mut rows);
    offers.extend(rows.into_iter().map(|(row, ..)| row));
    offers.extend(found_models(intelligence, voice));
    asked.back = Some(line::HOME.to_owned());
    asked.shape = Shape::Choice(offers);
}
