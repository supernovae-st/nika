// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The hand-counted 5-field cron — zero library, on purpose.
//!
//! Two scars own this file:
//!
//! - **#6** · a library that silently accepts SIX fields fires at the
//!   wrong time (the draft scheduler's, verbatim). So the field COUNT
//!   is validated here, by hand, before any field semantics.
//! - **the Vixie OR** · `dom` and `dow` restricted together match when
//!   EITHER does — the classic mis-fire trap. This grammar refuses the
//!   combination and asks for one side only.
//!
//! The weekday origin is NAMED: `0` and `7` are dimanche/Sunday, `1` is
//! lundi/Monday (`0 9 * * 1` reads as Monday on two hosts, Sunday on a
//! third — not here).

use crate::error::{CadenceError, CadenceErrorKind};

/// The five cron fields, sorted and deduplicated. An empty `dom`,
/// `months`, or `dow` means `*` (unrestricted).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CronSpec {
    minutes: Vec<u8>,
    hours: Vec<u8>,
    dom: Vec<u8>,
    months: Vec<u8>,
    dow: Vec<u8>,
}

impl CronSpec {
    /// Minutes (`0..=59`).
    #[must_use]
    pub fn minutes(&self) -> &[u8] {
        &self.minutes
    }

    /// Hours (`0..=23`).
    #[must_use]
    pub fn hours(&self) -> &[u8] {
        &self.hours
    }

    /// Days of month (`1..=31`) — empty means every day.
    #[must_use]
    pub fn dom(&self) -> &[u8] {
        &self.dom
    }

    /// Months (`1..=12`) — empty means every month.
    #[must_use]
    pub fn months(&self) -> &[u8] {
        &self.months
    }

    /// Weekdays (`0..=6`, Sunday `0`) — empty means every day.
    #[must_use]
    pub fn dow(&self) -> &[u8] {
        &self.dow
    }

    /// Build from already-validated fields (the readable form's path).
    pub(crate) fn weekly(hour: u8, minute: u8, weekday: u8) -> Self {
        Self {
            minutes: vec![minute],
            hours: vec![hour],
            dom: Vec::new(),
            months: Vec::new(),
            dow: vec![weekday],
        }
    }

    /// Does this civil date carry a slot? (Field matching only — the
    /// day/month/weekday predicates, never the clock.)
    pub(crate) fn covers(&self, date: jiff::civil::Date) -> bool {
        let Ok(month) = u8::try_from(date.month()) else {
            return false;
        };
        let Ok(day) = u8::try_from(date.day()) else {
            return false;
        };
        if !self.months.is_empty() && !self.months.contains(&month) {
            return false;
        }
        let dom_ok = self.dom.is_empty() || self.dom.contains(&day);
        // jiff offsets Sunday at 1 — this grammar NAMES Sunday at 0.
        let Ok(dow) = u8::try_from((date.weekday().to_sunday_one_offset() + 6) % 7) else {
            return false;
        };
        let dow_ok = self.dow.is_empty() || self.dow.contains(&dow);
        // Both-restricted is a parse-time refusal — at most one side is
        // ever restricted here.
        dom_ok && dow_ok
    }
}

const MONTH_NAMES: [(&str, u8); 12] = [
    ("jan", 1),
    ("feb", 2),
    ("mar", 3),
    ("apr", 4),
    ("may", 5),
    ("jun", 6),
    ("jul", 7),
    ("aug", 8),
    ("sep", 9),
    ("oct", 10),
    ("nov", 11),
    ("dec", 12),
];

const DOW_NAMES: [(&str, u8); 7] = [
    ("sun", 0),
    ("mon", 1),
    ("tue", 2),
    ("wed", 3),
    ("thu", 4),
    ("fri", 5),
    ("sat", 6),
];

/// Parse the five tokens — the count was already validated by the
/// caller (scar #6), this checks each field's shape and ranges.
pub(crate) fn parse_cron_fields(tokens: &[&str]) -> Result<CronSpec, CadenceError> {
    let minutes = parse_field(tokens[0], 0, 59, &[], "minute")?;
    let hours = parse_field(tokens[1], 0, 23, &[], "heure")?;
    let dom_raw = parse_field(tokens[2], 1, 31, &[], "jour-du-mois")?;
    let months = parse_field(tokens[3], 1, 12, &MONTH_NAMES, "mois")?;
    let mut dow = parse_field(tokens[4], 0, 7, &DOW_NAMES, "jour-de-semaine")?;
    for d in &mut dow {
        if *d == 7 {
            *d = 0;
        }
    }
    dow.sort_unstable();
    dow.dedup();
    let dom_any = tokens[2] == "*";
    let dow_any = tokens[4] == "*";
    if !dom_any && !dow_any {
        return Err(CadenceError::file(
            CadenceErrorKind::DomDowOr,
            "jour-du-mois ET jour-de-semaine restreints ensemble — le OR de Vixie entre les deux est le piège classique du tir raté",
            "garde un seul côté · `* * * * 1` (chaque lundi) ou `* * 1 * *` (chaque 1er), jamais les deux",
        ));
    }
    Ok(CronSpec {
        minutes,
        hours,
        dom: if dom_any { Vec::new() } else { dom_raw },
        months: if tokens[3] == "*" { Vec::new() } else { months },
        dow: if dow_any { Vec::new() } else { dow },
    })
}

/// One field: `*` · `*/n` · `a` · `a-b` · `a-b/n` · `a/n` · comma-joined.
fn parse_field(
    text: &str,
    lo: u8,
    hi: u8,
    names: &[(&str, u8)],
    label: &str,
) -> Result<Vec<u8>, CadenceError> {
    let mut out = Vec::new();
    for part in text.split(',') {
        let (range_text, step) = match part.split_once('/') {
            Some((base, stride)) => {
                let step: u8 = stride.parse().ok().filter(|n| *n >= 1).ok_or_else(|| {
                    CadenceError::file(
                        CadenceErrorKind::FieldSyntax,
                        format!("champ {label} `{part}` · un pas vaut au moins 1"),
                        format!("`{label}` · ex. `*/15`"),
                    )
                })?;
                (base, step)
            }
            None => (part, 1),
        };
        let has_step = part.contains('/');
        if range_text == "*" {
            push_range(&mut out, lo, hi, step);
        } else if let Some((a, b)) = range_text.split_once('-') {
            let from = field_value(a, lo, hi, names, label)?;
            let to = field_value(b, lo, hi, names, label)?;
            if from > to {
                return Err(CadenceError::file(
                    CadenceErrorKind::FieldRange,
                    format!("champ {label} `{part}` · borne basse au-dessus de la haute"),
                    format!("`{label}` entre {lo} et {hi}"),
                ));
            }
            push_range(&mut out, from, to, step);
        } else if has_step {
            let from = field_value(range_text, lo, hi, names, label)?;
            push_range(&mut out, from, hi, step);
        } else {
            out.push(field_value(range_text, lo, hi, names, label)?);
        }
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err(CadenceError::file(
            CadenceErrorKind::FieldSyntax,
            format!("champ {label} `{text}` · vide"),
            format!("`{label}` entre {lo} et {hi}"),
        ));
    }
    Ok(out)
}

fn push_range(out: &mut Vec<u8>, from: u8, to: u8, step: u8) {
    let mut v = from;
    loop {
        out.push(v);
        let Some(next) = v.checked_add(step) else {
            break;
        };
        if next > to {
            break;
        }
        v = next;
    }
}

fn field_value(
    text: &str,
    lo: u8,
    hi: u8,
    names: &[(&str, u8)],
    label: &str,
) -> Result<u8, CadenceError> {
    let lower = text.to_ascii_lowercase();
    if let Some((_, v)) = names.iter().find(|(name, _)| *name == lower) {
        return Ok(*v);
    }
    text.parse::<u8>()
        .ok()
        .filter(|v| *v >= lo && *v <= hi)
        .ok_or_else(|| {
            CadenceError::file(
                CadenceErrorKind::FieldRange,
                format!("champ {label} `{text}` · hors bornes ou illisible"),
                format!("`{label}` entre {lo} et {hi}"),
            )
        })
}
