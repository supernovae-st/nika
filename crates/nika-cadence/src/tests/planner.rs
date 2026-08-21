// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::unreachable
)]

//! The W1 surface, pinned: `prev_before` (the mirror — at-or-before,
//! the gap never returned, the fold's FIRST occurrence, the 366-day
//! bound) and `due` (the pure planner — the clock and the firing state
//! handed in, never read). Same law as the parent module: literal
//! `Zoned` instants, zero clock to drive (trap ①).

use jiff::Timestamp;
use jiff::tz::TimeZone;
use proptest::prelude::*;

use super::{at, utc, zoned};
use crate::due::{DueKind, MISSED_SLOTS_CAP, due, earliest_next};
use crate::{Cadence, CadenceErrorKind, Shift, parse_registry};

// ── prev_before · the mirror, literal instants ───────────────────────

#[test]
fn prev_before_is_the_mirror_of_next_after() {
    // At-or-before, contre strictly-after: les deux bornent l'intervalle
    // « (prev, next] ». À 10h00, le créneau qui vient de passer est 03h00.
    let cad = Cadence::parse("TZ=Europe/Paris 0 3 * * *").expect("cadence");
    let from = zoned("2026-08-18T10:00:00[Europe/Paris]");
    let prev = cad.prev_before(&from).expect("a slot exists before");
    assert_eq!(prev.civil.to_string(), "2026-08-18T03:00:00");
    let back = cad
        .next_after(
            &prev
                .at
                .checked_sub(jiff::Span::new().seconds(1))
                .expect("span"),
        )
        .expect("next");
    assert_eq!(back.at, prev.at, "next_after(prev − 1s) retombe dessus");
}

#[test]
fn prev_before_at_the_slot_returns_the_slot_itself() {
    // Inclusive: un créneau À l'instant demandé vient de passer — il est dû.
    let cad = Cadence::parse("TZ=Europe/Paris 0 3 * * *").expect("cadence");
    let from = zoned("2026-08-18T03:00:00[Europe/Paris]");
    let prev = cad.prev_before(&from).expect("le créneau même");
    assert_eq!(prev.civil.to_string(), "2026-08-18T03:00:00");
}

#[test]
fn prev_before_walks_the_dst_gap_backwards() {
    // N1 miroir · 2026-03-29, Paris saute 02:00 → 03:00: le 02:30 du 29
    // n'a JAMAIS existé — il a tiré à 03:00 (le premier instant licite
    // APRÈS), et ce tir-là appartient à next_after. Le miroir le saute
    // et rend le 28.
    let cad = Cadence::parse("TZ=Europe/Paris 30 2 * * *").expect("cadence");
    let from = zoned("2026-03-29T12:00:00[Europe/Paris]");
    let prev = cad.prev_before(&from).expect("slot");
    assert_eq!(prev.civil.to_string(), "2026-03-28T02:30:00");
    assert_eq!(prev.shift, Shift::Exact);
}

#[test]
fn prev_before_walks_the_dst_fold_backwards() {
    // N1 miroir · 2026-10-25, Paris recule 03:00 → 02:00: 02:30 existe
    // DEUX fois, le beat a tiré à la PREMIÈRE (00:30 UTC). Le miroir la
    // rend — jamais la 2e occurrence, jamais deux fois.
    let cad = Cadence::parse("TZ=Europe/Paris 30 2 * * *").expect("cadence");
    let from = zoned("2026-10-25T12:00:00[Europe/Paris]");
    let prev = cad.prev_before(&from).expect("slot");
    assert_eq!(prev.civil.to_string(), "2026-10-25T02:30:00");
    assert_eq!(
        utc(&prev.at),
        "2026-10-25T00:30:00Z",
        "la première occurrence"
    );
    assert_eq!(prev.shift, Shift::FoldedFirst, "le repli est DÉCLARÉ");
    // …et le créneau d'avant est la VEILLE, pas la 2e occurrence.
    let before = cad
        .prev_before(
            &prev
                .at
                .checked_sub(jiff::Span::new().seconds(1))
                .expect("span"),
        )
        .expect("la veille");
    assert_eq!(before.civil.to_string(), "2026-10-24T02:30:00");
}

#[test]
fn prev_before_beyond_366_days_is_none() {
    // La borne: 366 jours — le passé récent du planificateur, jamais une
    // archéologie. Le 29 février 2024 est hors de portée en août 2026.
    let cad = Cadence::parse("TZ=Europe/Paris 0 9 29 2 *").expect("cadence");
    assert_eq!(cad.prev_before(&at("2026-08-18T00:00:00Z")), None);
    // …mais dans la fenêtre, il répond.
    let prev = cad
        .prev_before(&at("2024-03-01T00:00:00Z"))
        .expect("le 29 février, cette année-là");
    assert_eq!(prev.civil.to_string(), "2024-02-29T09:00:00");
}

#[test]
fn prev_before_a_webhook_has_no_calendar() {
    assert_eq!(
        Cadence::Webhook.prev_before(&at("2026-08-11T00:00:00Z")),
        None
    );
}

#[test]
fn prev_before_crosses_a_month_and_a_year_backwards() {
    let cad = Cadence::parse("TZ=Europe/Paris 0 9 1 * *").expect("cadence");
    let prev = cad
        .prev_before(&at("2026-08-15T00:00:00Z"))
        .expect("le 1er du mois d'avant");
    assert_eq!(utc(&prev.at), "2026-08-01T07:00:00Z");
    let jan = Cadence::parse("TZ=UTC 0 0 1 1 *").expect("cadence");
    let prev = jan
        .prev_before(&at("2026-06-15T00:00:00Z"))
        .expect("le nouvel an");
    assert_eq!(utc(&prev.at), "2026-01-01T00:00:00Z");
}

// ── due · the pure planner, the clock handed in ─────────────────────

const DUE_DAILY: &str = "
nika: v1
ceiling: 0.50
arm:
  - workflow: workflows/nightly.nika.yaml
    cadence: TZ=Europe/Paris 0 3 * * *
    plafond: 0.10
    manqué: sauter
";

#[test]
fn due_an_on_time_slot_is_due_once() {
    // 03:02, jamais tiré · le créneau de 03:00 est dû, À L'HEURE.
    let reg = parse_registry(DUE_DAILY).expect("registre");
    let now = zoned("2026-08-18T03:02:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &now, &|_| None).expect("le plan").collect();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].index, 0);
    assert_eq!(plan[0].kind, DueKind::OnTime);
    assert_eq!(plan[0].slot.civil.to_string(), "2026-08-18T03:00:00");
    assert_eq!(plan[0].beat.workflow, "workflows/nightly.nika.yaml");
}

#[test]
fn due_a_missed_slot_says_how_many() {
    // 10:00, dernier tir hier 03:00 · le créneau du jour est dû, RATÉ,
    // et le silence compte UN créneau.
    let reg = parse_registry(DUE_DAILY).expect("registre");
    let now = zoned("2026-08-18T10:00:00[Europe/Paris]");
    let fired = zoned("2026-08-17T03:00:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &now, &|_| Some(fired.clone()))
        .expect("le plan")
        .collect();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].kind, DueKind::Missed { slots: 1 });
}

#[test]
fn due_an_idle_or_cloud_beat_is_never_due() {
    let reg = parse_registry(
        "
nika: v1
arm:
  - workflow: workflows/suspended.nika.yaml
    cadence: TZ=Europe/Paris 0 3 * * *
    plafond: 0.10
    manqué: sauter
    actif: false
    raison: pause estivale
    jusqu_au: 2026-09-01
  - workflow: workflows/cloud.nika.yaml
    cadence: TZ=Europe/Paris 0 3 * * *
    plafond: 0.10
    manqué: sauter
    où: cloud
",
    )
    .expect("registre");
    let now = zoned("2026-08-18T03:02:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &now, &|_| None).expect("le plan").collect();
    assert!(
        plan.is_empty(),
        "ni le suspendu ni le cloud ne sont dus ICI: {plan:?}"
    );
    assert_eq!(
        earliest_next(&reg, &now).expect("le prochain"),
        None,
        "rien d'armé ne tire ici"
    );
}

#[test]
fn earliest_next_is_the_soonest_of_the_armed_beats() {
    let reg = parse_registry(
        "
nika: v1
arm:
  - workflow: workflows/weekly.nika.yaml
    cadence: TZ=Europe/Paris lundi 9h07
    plafond: 0.25
    manqué: sauter
  - workflow: workflows/nightly.nika.yaml
    cadence: TZ=Europe/Paris 0 3 * * *
    plafond: 0.10
    manqué: rattraper
",
    )
    .expect("registre");
    // Mardi 10:00 Paris: l'hebdo vise lundi prochain, le quotidien demain.
    let now = zoned("2026-08-18T10:00:00[Europe/Paris]");
    let (index, slot) = earliest_next(&reg, &now)
        .expect("le plan")
        .expect("un prochain créneau");
    assert_eq!(index, 1, "le quotidien tire avant l'hebdo");
    assert_eq!(slot.civil.to_string(), "2026-08-19T03:00:00");
}

#[test]
fn earliest_next_a_tie_keeps_the_first_beat() {
    // Deux beats, même cadence, même prochain créneau — le PREMIER du
    // registre garde l'égalité (un dernier qui l'emporterait rendrait
    // l'ordre du fichier mensonger). Tue le mutant `< → <=` dans la
    // comparaison du meilleur.
    let reg = parse_registry(
        "
nika: v1
arm:
  - workflow: workflows/a.nika.yaml
    cadence: TZ=Europe/Paris 0 3 * * *
    plafond: 0.10
    manqué: sauter
  - workflow: workflows/b.nika.yaml
    cadence: TZ=Europe/Paris 0 3 * * *
    plafond: 0.20
    manqué: sauter
",
    )
    .expect("registre");
    let now = zoned("2026-08-18T10:00:00[Europe/Paris]");
    let (index, _) = earliest_next(&reg, &now)
        .expect("le plan")
        .expect("un prochain créneau");
    assert_eq!(index, 0, "l'égalité garde le premier du registre");
}

#[test]
fn due_an_already_fired_slot_is_not_due_again() {
    // La borne STRICTE: last_fired == le créneau → déjà tiré, jamais deux fois.
    let reg = parse_registry(DUE_DAILY).expect("registre");
    let now = zoned("2026-08-18T03:02:00[Europe/Paris]");
    let fired = zoned("2026-08-18T03:00:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &now, &|_| Some(fired.clone()))
        .expect("le plan")
        .collect();
    assert!(plan.is_empty(), "un créneau tiré ne se retire pas");
}

#[test]
fn due_never_fired_and_outside_the_window_invents_no_backlog() {
    // N2: pas d'état, pas de rattrapage inventé — seule la fenêtre compte.
    let reg = parse_registry(DUE_DAILY).expect("registre");
    let now = zoned("2026-08-18T10:00:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &now, &|_| None).expect("le plan").collect();
    assert!(plan.is_empty(), "jamais tiré + hors fenêtre = pas dû");
}

#[test]
fn the_on_time_window_is_five_minutes_to_the_second() {
    // La borne de la fenêtre, des deux côtés: 03:05:00 pile est ON TIME,
    // 03:05:01 est MISSED.
    let reg = parse_registry(DUE_DAILY).expect("registre");
    let fired = zoned("2026-08-17T03:00:00[Europe/Paris]");
    let state = |_: usize| Some(fired.clone());
    let edge = zoned("2026-08-18T03:05:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &edge, &state).expect("le plan").collect();
    assert_eq!(
        plan[0].kind,
        DueKind::OnTime,
        "à cinq minutes pile, à l'heure"
    );
    let late = zoned("2026-08-18T03:05:01[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &late, &state).expect("le plan").collect();
    assert_eq!(
        plan[0].kind,
        DueKind::Missed { slots: 1 },
        "03:05:01 — une respiration de trop, raté"
    );
}

#[test]
fn due_counts_every_slot_of_the_silence() {
    // Deux jours de silence pour un beat quotidien = DEUX créneaux dus.
    let reg = parse_registry(DUE_DAILY).expect("registre");
    let now = zoned("2026-08-18T10:00:00[Europe/Paris]");
    let fired = zoned("2026-08-16T03:00:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &now, &|_| Some(fired.clone()))
        .expect("le plan")
        .collect();
    assert_eq!(plan[0].kind, DueKind::Missed { slots: 2 }, "le 17 ET le 18");
}

#[test]
fn the_missed_count_saturates_at_the_cap() {
    // Au-delà de dix mille créneaux le compte dit le cap: un silence
    // plus long est une panne, pas un chiffre.
    let hourly = DUE_DAILY.replace("0 3 * * *", "0 * * * *");
    let reg = parse_registry(&hourly).expect("registre");
    let now = zoned("2026-08-18T03:02:00[Europe/Paris]");
    let fired = zoned("2025-06-26T00:00:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &now, &|_| Some(fired.clone()))
        .expect("le plan")
        .collect();
    #[allow(clippy::cast_possible_truncation)]
    let cap = MISSED_SLOTS_CAP as u32;
    assert_eq!(plan[0].kind, DueKind::Missed { slots: cap });
}

#[test]
fn due_the_gap_fire_is_due_like_any_other() {
    // Le créneau du gap a tiré à 03:00 CEST le 29 mars — un beat dont le
    // dernier tir est la veille le doit À L'HEURE à 03:02. La vue du
    // planificateur est le FEU (next_after porte l'avancé), pas le civil
    // qui existe — sinon ce tir manqué tomberait dans un trou.
    let gap = DUE_DAILY.replace("0 3 * * *", "30 2 * * *");
    let reg = parse_registry(&gap).expect("registre");
    let fired = zoned("2026-03-28T02:30:00[Europe/Paris]");
    let now = zoned("2026-03-29T03:02:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &now, &|_| Some(fired.clone()))
        .expect("le plan")
        .collect();
    assert_eq!(plan.len(), 1, "le tir avancé du gap est dû");
    assert_eq!(plan[0].kind, DueKind::OnTime);
    assert_eq!(
        plan[0].slot.shift,
        Shift::AdvancedFirstValid,
        "le créneau dû DÉCLARE son déplacement"
    );
}

#[test]
fn a_webhook_beat_is_never_due_nor_next() {
    let reg = parse_registry(
        "
nika: v1
arm:
  - workflow: workflows/hooked.nika.yaml
    cadence: on-webhook
    plafond: 0.10
    manqué: sauter
",
    )
    .expect("registre");
    let now = zoned("2026-08-18T10:00:00[Europe/Paris]");
    let plan: Vec<_> = due(&reg, &now, &|_| None).expect("le plan").collect();
    assert!(plan.is_empty(), "un webhook n'a pas de calendrier");
    assert_eq!(earliest_next(&reg, &now).expect("le prochain"), None);
}

#[test]
fn due_a_cadence_that_breaks_the_law_refuses_the_plan() {
    // Un registre qui a contourné validate (TZ absente) — le planificateur
    // REFUSE le plan entier, il ne saute pas le beat en silence.
    let reg = parse_registry(
        "
nika: v1
arm:
  - workflow: workflows/nightly.nika.yaml
    cadence: 0 3 * * *
    plafond: 0.10
    manqué: sauter
",
    )
    .expect("registre");
    let now = zoned("2026-08-18T10:00:00[Europe/Paris]");
    let err = due(&reg, &now, &|_| None)
        .map(Iterator::count)
        .expect_err("le refus");
    assert_eq!(err.kind(), CadenceErrorKind::TzMissing);
    let err = earliest_next(&reg, &now)
        .map(|slot| slot.map(|_| ()))
        .expect_err("le refus");
    assert_eq!(err.kind(), CadenceErrorKind::TzMissing);
}

// ── proptest · the inverse law, the gap's exception declared ────────

proptest! {
    #[test]
    fn prev_before_and_next_after_are_mutual_inverses(secs in 0i64..=4_102_444_800) {
        // La loi miroir, tout instant, toute cadence du corpus:
        // prev_before(next_after(t) + 1s) == next_after(t). Exception
        // DÉCLARÉE: le créneau AVANCÉ (gap DST) n'a jamais existé — il
        // n'appartient pas à prev_before; la loi devient « le miroir rend
        // le réel d'avant, dont next_after retombe dessus ».
        let corpus = [
            "TZ=UTC 30 4 * * *",
            "TZ=Europe/Paris 0 3 * * *",
            "TZ=Europe/Paris 30 2 * * *", // traverse le gap ET le fold 2026
            "TZ=Europe/Paris lundi 9h07",
            "TZ=America/New_York 15 9 * * 1-5",
        ];
        let from = Timestamp::from_second(secs)
            .expect("instant")
            .to_zoned(TimeZone::UTC);
        for expr in corpus {
            let cad = Cadence::parse(expr).expect("cadence");
            let next = cad.next_after(&from).expect("un prochain créneau");
            let just_after = next
                .at
                .checked_add(jiff::Span::new().seconds(1))
                .expect("+1s");
            let prev = cad.prev_before(&just_after).expect("le miroir répond");
            if next.shift == Shift::AdvancedFirstValid {
                prop_assert!(prev.at < next.at, "{} · l'avancé n'est jamais RENDU", expr);
                let round = cad.next_after(&prev.at).expect("next depuis le réel");
                prop_assert_eq!(
                    &round.at,
                    &next.at,
                    "{} · next_after du réel retombe dessus",
                    expr
                );
            } else {
                prop_assert_eq!(&prev.at, &next.at, "{} · le miroir exact", expr);
            }
        }
    }
}
