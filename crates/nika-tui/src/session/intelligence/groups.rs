// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Browse the complete owner-supplied catalogue by connection. Opening a group is local;
//! only one of its original offer keys can apply an intelligence through Session.

use crate::model::{Offer, Shape, Waiting};
use nika_session::work;

const PREFIX: &str = "/intelligence models ";

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
        "Choose a model below, or name one with /intelligence 2 provider/model. No selection has changed."
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

pub(super) fn requested(work: &work::Work, line: &str) -> Option<String> {
    let asked = line.strip_prefix(PREFIX)?;
    work.intelligence
        .as_ref()?
        .options
        .iter()
        .find_map(|offer| {
            let (key, _) = connection(&offer.key)?;
            (key == asked).then(|| key.to_owned())
        })
}

pub(in crate::session) fn project(waiting: &mut Waiting, work: &work::Work, opened: Option<&str>) {
    let Waiting::Selection { asked } = waiting else {
        return;
    };
    let Some(intelligence) = &work.intelligence else {
        return;
    };
    let options = &intelligence.options;
    let mut offers = Vec::new();
    if let Some(opened) = opened.filter(|opened| {
        options
            .iter()
            .any(|offer| connection(&offer.key).is_some_and(|(key, _)| key == *opened))
    }) {
        offers.extend(
            options
                .iter()
                .filter(|offer| connection(&offer.key).is_some_and(|(key, _)| key == opened))
                .map(|offer| Offer::new(&offer.key, &offer.label)),
        );
        let setup_only = setup_only(
            opened,
            offers
                .iter()
                .map(|offer| (offer.key.as_str(), offer.label.as_str())),
        );
        asked.label = format!(
            "{} · {}",
            if setup_only {
                "Configure API"
            } else {
                "Choose a model"
            },
            opened.split_once(' ').map_or(opened, |(_, name)| name)
        );
        offers.push(Offer::new("/intelligence", "All connections"));
    } else {
        asked.label = "Choose an AI connection".into();
        // The current model remains immediately accessible; no radio is preselected.
        offers.extend(
            options
                .iter()
                .filter(|offer| offer.recommended)
                .map(|offer| Offer::new(&offer.key, &offer.label)),
        );
        let mut seen = Vec::new();
        for offer in options {
            let Some((key, access)) = connection(&offer.key) else {
                offers.push(Offer::new(&offer.key, &offer.label));
                continue;
            };
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let provider = key.split_once(' ').map_or(key, |(_, name)| name);
            let group = options
                .iter()
                .filter(|offer| {
                    connection(&offer.key).is_some_and(|(candidate, _)| candidate == key)
                })
                .collect::<Vec<_>>();
            let count = group.iter().filter(|offer| offer.key.contains('/')).count();
            let models = match count {
                0 => String::new(),
                1 => " · 1 model".into(),
                _ => format!(" · {count} models"),
            };
            let needs_key = access == "API"
                && group
                    .iter()
                    .all(|offer| offer.label.split(" · ").any(|part| part == "key required"));
            let requirement = if needs_key { " · key required" } else { "" };
            offers.push(Offer::new(
                format!("{PREFIX}{key}"),
                format!("{provider} · {access}{models}{requirement}"),
            ));
        }
    }
    if !asked.mandatory {
        offers.push(Offer::new("cancel", "Keep the current selection"));
    }
    asked.shape = Shape::Choice(offers);
}
