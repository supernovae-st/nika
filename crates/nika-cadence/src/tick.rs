// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Named-beat tick classifier — the remaining L0 hole `due()` does not fill.
//!
//! `due()` already drops inactive and cloud beats. OS units fire a NAMED
//! beat (`nika arm fire <label>` · `nika serve`) and must journal the
//! skip *reason*. That total function is pure: registry + index +
//! injected instant + last decided slot. The L4 firer locks, claims,
//! and runs; this module never opens a path.
//!
//! The D6 policy set (`v0_unsupported`) is the ONE list. Emit and the
//! firer both read it — they must never disagree.

use jiff::Zoned;

use crate::due::{DueKind, due};
use crate::registry::{AfterSkip, ArmRegistry, Beat, Cadence, Locus, MissPolicy, Overlap};

/// Durable provenance of a cadence fire decision.
///
/// The wire spelling is deliberately closed: an on-time slot is
/// `scheduled`, while one fire answering for missed slots is `catch_up`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ScheduleDecision {
    /// The selected slot was on time.
    Scheduled,
    /// The fire answers for one or more missed slots.
    CatchUp,
}

/// The named-beat verdict. Distinct from [`crate::firing::Decision`]
/// (the lifecycle machine: `Become` / `JournalClaim` / `Fire` / `Ignore`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TickDecision {
    /// A policy said no. `slot` Some ⇒ the decision consumes the slot
    /// (last.json moves); `journal` false ⇒ nothing changed, nothing is
    /// written (a duplicate tick is not a decision).
    Skip {
        /// The full one-line report (D8).
        line: String,
        /// The machine token (`missed:2` · `overlap` · `cloud` · …).
        reason: String,
        /// The slot the decision covers.
        slot: Option<Zoned>,
        /// Journal the decision (history · maybe last.json).
        journal: bool,
    },
    /// v0 refuses with teaching, naming the version it arrives with.
    Refuse {
        /// The teaching line (exit 2).
        line: String,
    },
    /// Fire the slot — `slots` Some(n) is `rattraper-une-fois`: ONE run
    /// answers for the whole silence.
    Fire {
        /// The slot to fire.
        slot: Zoned,
        /// The silence's count for `rattraper-une-fois`.
        slots: Option<u32>,
        /// The silence's first slot for `rattraper-une-fois` (with `slots`):
        /// the run is told the silence from it to `slot` (spec 04 §`run`).
        first: Option<Zoned>,
    },
}

/// One D6 policy. Emit and the firer both project from this enum —
/// adding a fifth variant is a compile error at both edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V0Policy {
    Remplacer,
    ACompletion,
    Rattraper,
    RattraperDerniers,
    Decalage,
}

impl V0Policy {
    fn of(beat: &Beat) -> Option<Self> {
        if beat.chevauchement == Some(Overlap::Remplacer) {
            return Some(Self::Remplacer);
        }
        if beat.apres_saut == Some(AfterSkip::ACompletion) {
            return Some(Self::ACompletion);
        }
        if beat.manque == Some(MissPolicy::Rattraper) {
            return Some(Self::Rattraper);
        }
        if matches!(beat.manque, Some(MissPolicy::RattraperDerniers(_))) {
            return Some(Self::RattraperDerniers);
        }
        if beat.decalage.is_some() {
            return Some(Self::Decalage);
        }
        None
    }

    const fn what(self) -> &'static str {
        match self {
            Self::Remplacer => "chevauchement: remplacer",
            Self::ACompletion => "après_saut: à-complétion",
            Self::Rattraper => "manqué: rattraper",
            Self::RattraperDerniers => "manqué: { rattraper: N }",
            Self::Decalage => "décalage:",
        }
    }

    const ARRIVES: &'static str = "serve v0.2";

    const fn today(self) -> &'static str {
        match self {
            Self::Remplacer => "aujourd'hui: sauter (le défaut) ou file",
            Self::ACompletion => "aujourd'hui: prochain-créneau (le défaut)",
            Self::Rattraper | Self::RattraperDerniers => {
                "aujourd'hui: rattraper-une-fois ou sauter"
            }
            Self::Decalage => "aujourd'hui le créneau tire à l'instant dit",
        }
    }
}

/// The D6 set — v0 refuses what it cannot keep, naming the version the
/// support arrives with. Emit and [`tick_decision`] share this list.
#[must_use]
pub fn v0_unsupported(beat: &Beat) -> Option<(&'static str, &'static str)> {
    let policy = V0Policy::of(beat)?;
    Some((policy.what(), V0Policy::ARRIVES))
}

/// The shared pause-bound law: an ISO civil date strictly before the
/// decision instant's own date ⇒ the bounded suspension is over. The
/// arm-fire edge and the serve planner judge on the same instant's civil
/// date (a bare `--now` rides UTC); [`suspension_over`] reads the same law
/// in the cadence's own zone.
#[must_use]
pub(crate) fn date_expired(raw: &str, now: &Zoned) -> bool {
    let Ok(date) = raw.parse::<jiff::civil::Date>() else {
        return false;
    };
    date < now.date()
}

/// `jusqu_au` strictly before the decision instant's own date ⇒ the
/// suspension is over. v0 judges on the instant's civil date through the
/// crate-private `date_expired` helper.
#[must_use]
pub fn expiry_passed(beat: &Beat, now: &Zoned) -> Option<String> {
    let raw = beat.jusqu_au.as_deref()?;
    date_expired(raw, now).then(|| raw.to_owned())
}

/// The bounded suspension is over: the beat's `jusqu_au` civil date is
/// strictly before the decision instant's date in the cadence's own `TZ=`
/// zone, whatever zone the instant arrives in. An `on-webhook` beat has no
/// calendar and reads the UTC date. `false` without a `jusqu_au`, or when
/// the date, the cadence or its zone does not resolve: validation refuses
/// each of them before a beat is armed.
#[must_use]
pub fn suspension_over(beat: &Beat, now: &Zoned) -> bool {
    let Some(Ok(date)) = beat
        .jusqu_au
        .as_deref()
        .map(str::parse::<jiff::civil::Date>)
    else {
        return false;
    };
    let zone = match Cadence::parse(&beat.cadence) {
        Ok(cadence) => match cadence.tz() {
            Some(name) => crate::next::bundled_tz(name),
            None => Some(jiff::tz::TimeZone::UTC),
        },
        Err(_) => None,
    };
    zone.is_some_and(|zone| date < now.with_time_zone(zone).date())
}

/// The zone a cadence expression reads its dates in, as `suspension_over`
/// reads it: its own `TZ=`, UTC for an on-webhook beat, `None` when the
/// cadence or its zone does not resolve.
fn cadence_zone(expression: &str) -> Option<jiff::tz::TimeZone> {
    match Cadence::parse(expression).ok()?.tz() {
        Some(name) => crate::next::bundled_tz(name),
        None => Some(jiff::tz::TimeZone::UTC),
    }
}

/// The instant a suspension bounded by `until` ends: the start of the day
/// after it, in the zone `expression` reads its dates in (UTC without one).
pub(crate) fn wake_of(until: &str, expression: Option<&str>) -> Option<Zoned> {
    let until = until.parse::<jiff::civil::Date>().ok()?;
    let zone = match expression {
        Some(expression) => cadence_zone(expression)?,
        None => jiff::tz::TimeZone::UTC,
    };
    until.tomorrow().ok()?.to_zoned(zone).ok()
}

/// The suspension law for a resident definition's `pauseUntil`, the same as
/// `suspension_over` reads a beat's `jusqu_au`: the civil date strictly
/// before now's date in the cadence's own zone (UTC without a cadence).
#[must_use]
pub(crate) fn suspension_date_over(until: &str, expression: Option<&str>, now: &Zoned) -> bool {
    let Ok(date) = until.parse::<jiff::civil::Date>() else {
        return false;
    };
    let zone = match expression {
        Some(expression) => cadence_zone(expression),
        None => Some(jiff::tz::TimeZone::UTC),
    };
    zone.is_some_and(|zone| date < now.with_time_zone(zone).date())
}

/// The instant a suspended beat wakes (the start of the day after its
/// `jusqu_au`, in its cadence's zone); `None` for an active beat or a
/// suspension without a bound.
#[must_use]
pub(crate) fn wake_instant(beat: &Beat) -> Option<Zoned> {
    if beat.is_active() {
        return None;
    }
    wake_of(beat.jusqu_au.as_deref()?, Some(&beat.cadence))
}

/// Suspended and not yet woken: the beat sleeps.
#[must_use]
pub(crate) fn still_suspended(beat: &Beat, now: &Zoned) -> bool {
    !beat.is_active() && !suspension_over(beat, now)
}

/// The named-beat decision, pure. Order: the file's own truth
/// (inactive · cloud · expired), then the v0 refusals (they teach even
/// when the beat would be due), then the clock's verdict.
#[must_use]
pub fn tick_decision(
    registry: &ArmRegistry,
    index: usize,
    label: &str,
    now: &Zoned,
    last: Option<&Zoned>,
) -> TickDecision {
    let Some(beat) = registry.beats().nth(index) else {
        return TickDecision::Refuse {
            line: "arm fire · engine fault: the label resolved past the registry".to_owned(),
        };
    };
    // A suspension sleeps until its `jusqu_au`, then the beat fires as an
    // active one under the same laws.
    if still_suspended(beat, now) {
        let why = beat.raison.as_deref().unwrap_or("sans raison");
        let until = beat
            .jusqu_au
            .as_deref()
            .map_or(String::new(), |date| format!(" · jusqu_au {date}"));
        return TickDecision::Skip {
            line: format!("skipped {label} · inactive — {why}{until}"),
            reason: "inactive".to_owned(),
            slot: None,
            journal: true,
        };
    }
    if beat.locus() == Locus::Cloud {
        return TickDecision::Skip {
            line: format!(
                "skipped {label} · cloud — le cloud exécute, le calendrier demeure au registre"
            ),
            reason: "cloud".to_owned(),
            slot: None,
            journal: true,
        };
    }
    if beat.is_active()
        && let Some(expired) = expiry_passed(beat, now)
    {
        return TickDecision::Skip {
            line: format!("skipped {label} · expired · jusqu_au {expired}"),
            reason: "expired".to_owned(),
            slot: None,
            journal: true,
        };
    }
    if let Some(line) = v0_refusal_line(beat) {
        return TickDecision::Refuse { line };
    }
    let cadence = match Cadence::parse(&beat.cadence) {
        Ok(cadence) => cadence,
        Err(error) => {
            return TickDecision::Refuse {
                line: format!("arm fire · engine fault: a validated cadence refuses — {error}"),
            };
        }
    };
    if matches!(cadence, Cadence::Webhook) {
        return TickDecision::Skip {
            line: format!(
                "skipped {label} · webhook — le beat tire à l'événement, jamais à l'horloge"
            ),
            reason: "webhook".to_owned(),
            slot: None,
            journal: true,
        };
    }
    decide_by_clock(registry, index, label, now, last, &cadence, beat)
}

fn v0_refusal_line(beat: &Beat) -> Option<String> {
    let policy = V0Policy::of(beat)?;
    Some(format!(
        "arm fire {} · {} — arrive avec {} · {}",
        beat.workflow.as_str(),
        policy.what(),
        V0Policy::ARRIVES,
        policy.today(),
    ))
}

/// The clock half: the planner's silence over `(last, now]`, the
/// on-time window, the miss policy.
fn decide_by_clock(
    registry: &ArmRegistry,
    index: usize,
    label: &str,
    now: &Zoned,
    last: Option<&Zoned>,
    cadence: &Cadence,
    beat: &Beat,
) -> TickDecision {
    let last_owned = last.cloned();
    let last_of = move |i: usize| {
        if i == index { last_owned.clone() } else { None }
    };
    let dues = match due(registry, now, &last_of) {
        Ok(dues) => dues,
        Err(error) => {
            return TickDecision::Refuse {
                line: format!("arm fire · engine fault: a validated registry refuses — {error}"),
            };
        }
    };
    match dues.into_iter().find(|item| item.index == index) {
        Some(due_item) => match (due_item.kind, beat.manque) {
            (DueKind::OnTime, _) => TickDecision::Fire {
                slot: due_item.slot.at,
                slots: None,
                first: None,
            },
            (DueKind::Missed { slots }, Some(MissPolicy::Sauter)) => TickDecision::Skip {
                line: format!(
                    "skipped {label} · missed:{slots} · slot {}",
                    due_item.slot.at.timestamp()
                ),
                reason: format!("missed:{slots}"),
                slot: Some(due_item.slot.at),
                journal: true,
            },
            (DueKind::Missed { slots }, Some(MissPolicy::RattraperUneFois)) => TickDecision::Fire {
                slot: due_item.slot.at,
                slots: Some(slots),
                first: due_item.first.map(|first| first.at),
            },
            _ => TickDecision::Refuse {
                line: "arm fire · engine fault: manqué: policy escaped validation".to_owned(),
            },
        },
        None => match (last, cadence.prev_before(now)) {
            (Some(fired), Some(prev)) if prev.at == *fired => TickDecision::Skip {
                line: format!("skipped {label} · already · slot {}", prev.at.timestamp()),
                reason: "already".to_owned(),
                slot: None,
                journal: false,
            },
            _ => TickDecision::Skip {
                line: format!(
                    "skipped {label} · not-due — hors fenêtre, et N2 n'invente pas d'arriéré"
                ),
                reason: "not-due".to_owned(),
                slot: None,
                journal: false,
            },
        },
    }
}
