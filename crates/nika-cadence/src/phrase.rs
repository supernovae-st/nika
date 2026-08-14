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
//!
//! The display rides the bitset's ONE encoding: a full field prints
//! `*` (never its expansion — the pre-bitset `describe("* * * * *")`
//! rendered 244 characters; now 5).

use crate::cron::{CronSpec, Field};
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
    ///
    /// **Never re-parse this output** (tranché 2026-08-12): the readable
    /// display is for EYES — `lundi 9h07 · Europe/Paris` is refused at
    /// parse (`TzMissing`, the `· tz` suffix is not grammar). The cron
    /// branch alone round-trips (`TZ=… m h dom mo dow`). The `Shift`
    /// enum stays `#[non_exhaustive]` so a declared « absorbed slot »
    /// can join when a consumer truly needs it (decision deferred).
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
                        show_field(*spec.minutes()),
                        show_field(*spec.hours()),
                        show_field(*spec.dom()),
                        show_field(*spec.months()),
                        show_field(*spec.dow()),
                    )
                }
            }
        }
    }
}

/// `lundi 9h07` when the spec is exactly `m h * * dow` with one value
/// each — the shape the readable form renders.
fn weekly_readable(spec: &CronSpec) -> Option<String> {
    if spec.dom().is_full() && spec.months().is_full() {
        let minute = spec.minutes().single()?;
        let hour = spec.hours().single()?;
        let dow = spec.dow().single()?;
        let jour = weekday_fr(dow)?;
        Some(format!("{jour} {hour}h{minute:02}"))
    } else {
        None
    }
}

/// A field as the cron text speaks it: `*` when full (the bitset's one
/// encoding — never the expansion), else the values comma-joined.
fn show_field<const LO: u8, const HI: u8>(field: Field<LO, HI>) -> String {
    if field.is_full() {
        "*".to_owned()
    } else {
        field
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}
