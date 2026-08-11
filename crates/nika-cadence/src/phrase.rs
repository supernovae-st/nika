// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The readable French form — and the display that normalizes TO it.
//!
//! Tranché 08-09 (§5, q1): `cadence:` accepts BOTH forms — cron is
//! universal and unreadable, the readable one is pleasant and
//! non-portable — "on ne choisit pas, on accepte", and display
//! normalizes to the readable side.
//!
//! Round 1 claims exactly ONE readable shape: `<jour-fr> <H>h<MM>`
//! (the only form the canon attests — `lundi 9h07` · `dimanche 18h07`).
//! Everything else speaks cron.

use crate::cron::CronSpec;
use crate::registry::Cadence;

/// The French weekday names the readable form accepts (lundi = 1 …
/// dimanche = 0 — the same origin the cron side NAMES).
const WEEKDAYS_FR: [(&str, u8); 7] = [
    ("lundi", 1),
    ("mardi", 2),
    ("mercredi", 3),
    ("jeudi", 4),
    ("vendredi", 5),
    ("samedi", 6),
    ("dimanche", 0),
];

/// The French name of a cron weekday (`0` = dimanche).
pub(crate) fn weekday_fr(dow: u8) -> Option<&'static str> {
    WEEKDAYS_FR
        .iter()
        .find(|(_, d)| *d == dow)
        .map(|(name, _)| *name)
}

/// Parse the readable form — `None` when the tokens are anything else
/// (the caller then speaks cron). Exactly two tokens: `<jour-fr>` and
/// `<H>h<MM>` with a two-digit minute.
pub(crate) fn parse_readable(tokens: &[&str]) -> Option<CronSpec> {
    if tokens.len() != 2 {
        return None;
    }
    let jour = WEEKDAYS_FR
        .iter()
        .find(|(name, _)| *name == tokens[0])
        .map(|(_, d)| *d)?;
    let (h, m) = tokens[1].split_once('h')?;
    let hour: u8 = h.parse().ok().filter(|v| *v <= 23)?;
    if m.len() != 2 {
        return None;
    }
    let minute: u8 = m.parse().ok().filter(|v| *v <= 59)?;
    Some(CronSpec::weekly(hour, minute, jour))
}

impl Cadence {
    /// The normalized display — the readable form wins when the spec is
    /// exactly one weekly slot; the zone is ALWAYS shown (the binding
    /// must be visible, never implicit).
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Webhook => "on-webhook".to_owned(),
            Self::Cron { tz, spec } => {
                if let Some(text) = weekly_readable(spec) {
                    format!("{text} · {tz}")
                } else {
                    format!(
                        "TZ={tz} {} {} {} {} {}",
                        join_field(spec.minutes()),
                        join_field(spec.hours()),
                        join_field(spec.dom()),
                        join_field(spec.months()),
                        join_field(spec.dow()),
                    )
                }
            }
        }
    }
}

/// `lundi 9h07` when the spec is exactly `m h * * dow` with one value
/// each — the shape the readable form renders.
fn weekly_readable(spec: &CronSpec) -> Option<String> {
    if spec.minutes().len() == 1
        && spec.hours().len() == 1
        && spec.dom().is_empty()
        && spec.months().is_empty()
        && spec.dow().len() == 1
    {
        let jour = weekday_fr(spec.dow()[0])?;
        Some(format!(
            "{jour} {}h{:02}",
            spec.hours()[0],
            spec.minutes()[0]
        ))
    } else {
        None
    }
}

fn join_field(values: &[u8]) -> String {
    if values.is_empty() {
        "*".to_owned()
    } else {
        values
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// The next `count` slots after `from` — "les 4 prochaines dates" the
/// display shows. An infinite-safe fold over [`Cadence::next_after`]
/// (each step is strictly later than the last), truncated at `count`;
/// a webhook or an unreachable schedule simply ends the walk early.
pub fn next_slots(
    cadence: &Cadence,
    from: &jiff::Zoned,
    count: usize,
) -> impl Iterator<Item = jiff::Zoned> {
    let first = cadence.next_after(from);
    std::iter::successors(first, move |prev| cadence.next_after(prev)).take(count)
}
