// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::unreachable
)]

//! Pins for the named-beat tick classifier (`tick_decision`).
//!
//! `due()` already drops inactive and cloud beats. OS units fire a
//! NAMED beat and must journal skip reasons; that total function lives
//! here so emit and the L4 firer cannot drift.

use jiff::{Timestamp, Zoned, tz::TimeZone};

use crate::registry::ArmRegistry;
use crate::tick::{TickDecision, expiry_passed, tick_decision};

fn at(text: &str) -> Zoned {
    text.parse::<Timestamp>()
        .expect("ts")
        .to_zoned(TimeZone::UTC)
}

fn registry_with(body: &str) -> ArmRegistry {
    let text = format!(
        "nika: v1\narm:\n  - workflow: workflows/doctor.nika.yaml\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.25\n{body}"
    );
    let registry = crate::parse_registry(&text).expect("parse");
    assert!(
        crate::validate(&registry).next().is_none(),
        "the fixture must be lawful"
    );
    registry
}

const BASE: &str = "    manqué: sauter\n";

fn decide_base(now: &Zoned, last: Option<&Zoned>) -> TickDecision {
    tick_decision(&registry_with(BASE), 0, "doctor", now, last)
}

#[test]
fn an_on_time_slot_without_state_fires() {
    match decide_base(&at("2026-08-19T03:02:00Z"), None) {
        TickDecision::Fire { slot, slots } => {
            assert_eq!(slot.timestamp().to_string(), "2026-08-19T03:00:00Z");
            assert_eq!(slots, None);
        }
        _ => panic!("on-time without state fires"),
    }
}

#[test]
fn a_first_contact_beyond_the_window_skips_without_a_record() {
    match decide_base(&at("2026-08-19T10:00:00Z"), None) {
        TickDecision::Skip {
            reason, journal, ..
        } => {
            assert_eq!(reason, "not-due");
            assert!(!journal, "N2 writes nothing");
        }
        _ => panic!("hors fenêtre sans état saute"),
    }
}

#[test]
fn an_already_decided_slot_skips_without_a_record() {
    let fired = at("2026-08-19T03:00:00Z");
    match decide_base(&at("2026-08-19T03:01:00Z"), Some(&fired)) {
        TickDecision::Skip {
            reason, journal, ..
        } => {
            assert_eq!(reason, "already");
            assert!(!journal);
        }
        _ => panic!("déjà décidé saute"),
    }
}

#[test]
fn a_different_prior_slot_is_not_mislabeled_already() {
    let older = at("2026-08-17T03:00:00Z");
    match decide_base(&at("2026-08-19T10:00:00Z"), Some(&older)) {
        TickDecision::Skip {
            reason, journal, ..
        } => {
            assert_eq!(reason, "missed:2");
            assert!(journal);
        }
        _ => panic!("the planner owns the two missed slots"),
    }

    let future = at("2026-08-20T03:00:00Z");
    match decide_base(&at("2026-08-19T10:00:00Z"), Some(&future)) {
        TickDecision::Skip {
            reason, journal, ..
        } => {
            assert_eq!(reason, "not-due");
            assert!(!journal);
        }
        _ => panic!("a non-matching state is never called already"),
    }
}

#[test]
fn a_missed_slot_under_sauter_is_journaled_and_consumed() {
    let fired = at("2026-08-18T03:00:00Z");
    match decide_base(&at("2026-08-19T10:00:00Z"), Some(&fired)) {
        TickDecision::Skip {
            reason,
            slot,
            journal,
            line,
        } => {
            assert_eq!(reason, "missed:1");
            assert!(journal);
            assert_eq!(
                slot.expect("the consumed slot").timestamp().to_string(),
                "2026-08-19T03:00:00Z"
            );
            assert!(line.starts_with("skipped doctor · missed:1"), "{line}");
        }
        _ => panic!("un créneau raté saute"),
    }
}

#[test]
fn rattraper_une_fois_fires_once_for_the_whole_silence() {
    let registry = registry_with("    manqué: rattraper-une-fois\n");
    let fired = at("2026-08-17T03:00:00Z");
    match tick_decision(
        &registry,
        0,
        "doctor",
        &at("2026-08-19T03:02:00Z"),
        Some(&fired),
    ) {
        TickDecision::Fire { slot, slots } => {
            assert_eq!(slot.timestamp().to_string(), "2026-08-19T03:00:00Z");
            assert_eq!(slots, Some(2), "the 18th AND the 19th");
        }
        _ => panic!("un seul tir pour tout le silence"),
    }
}

#[test]
fn inactive_cloud_and_expired_beats_skip_with_their_reason() {
    let registry = registry_with(concat!(
        "    manqué: sauter\n",
        "    actif: false\n",
        "    raison: \"pause estivale\"\n",
        "    jusqu_au: \"2099-12-31\"\n",
    ));
    match tick_decision(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
        TickDecision::Skip { reason, line, .. } => {
            assert_eq!(reason, "inactive");
            assert!(line.contains("pause estivale"), "{line}");
        }
        _ => panic!("un beat inactif saute"),
    }

    let registry = registry_with("    manqué: sauter\n    où: cloud\n");
    match tick_decision(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
        TickDecision::Skip { reason, .. } => assert_eq!(reason, "cloud"),
        _ => panic!("un beat cloud saute"),
    }

    let registry = registry_with("    manqué: sauter\n    jusqu_au: \"2026-01-01\"\n");
    match tick_decision(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
        TickDecision::Skip { reason, .. } => assert_eq!(reason, "expired"),
        _ => panic!("un beat expiré saute"),
    }
}

#[test]
fn expiry_is_strictly_before_the_decision_date() {
    let registry = registry_with("    manqué: sauter\n    jusqu_au: \"2026-08-19\"\n");
    let beat = registry.beats().next().expect("beat");
    assert_eq!(expiry_passed(beat, &at("2026-08-19T23:59:59Z")), None);
    assert_eq!(
        expiry_passed(beat, &at("2026-08-20T00:00:00Z")).as_deref(),
        Some("2026-08-19")
    );
}

#[test]
fn a_webhook_beat_skips_the_clock() {
    let text = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
        "    cadence: \"on-webhook\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    );
    let registry = crate::parse_registry(text).expect("parse");
    match tick_decision(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
        TickDecision::Skip { reason, .. } => assert_eq!(reason, "webhook"),
        _ => panic!("un webhook saute l'horloge"),
    }
}

#[test]
fn the_v0_unsupported_policies_refuse_with_teaching() {
    for (extra, line) in [
        (
            "    manqué: sauter\n    chevauchement: remplacer\n",
            "arm fire workflows/doctor.nika.yaml · chevauchement: remplacer — arrive avec serve v0.2 · aujourd'hui: sauter (le défaut) ou file",
        ),
        (
            "    manqué: sauter\n    chevauchement: sauter\n    après_saut: à-complétion\n",
            "arm fire workflows/doctor.nika.yaml · après_saut: à-complétion — arrive avec serve v0.2 · aujourd'hui: prochain-créneau (le défaut)",
        ),
        (
            "    manqué: sauter\n    décalage: hash\n",
            "arm fire workflows/doctor.nika.yaml · décalage: — arrive avec serve v0.2 · aujourd'hui le créneau tire à l'instant dit",
        ),
        (
            "    manqué: rattraper\n",
            "arm fire workflows/doctor.nika.yaml · manqué: rattraper — arrive avec serve v0.2 · aujourd'hui: rattraper-une-fois ou sauter",
        ),
    ] {
        let registry = registry_with(extra);
        match tick_decision(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
            TickDecision::Refuse { line: got } => assert_eq!(got, line),
            other => panic!("D6 attendu, reçu {other:?}"),
        }
    }
}

#[test]
fn emit_and_tick_share_the_same_v0_policy_set() {
    use std::path::PathBuf;

    use crate::emit::{self, EmitCtx, EmitRefusal, Mode, Target};
    use crate::tick::v0_unsupported;

    let registry = registry_with("    manqué: sauter\n    chevauchement: remplacer\n");
    let beat = registry.beats().next().expect("beat");
    let (what, arrives) = v0_unsupported(beat).expect("D6");
    assert_eq!(what, "chevauchement: remplacer");
    assert_eq!(arrives, "serve v0.2");
    match tick_decision(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
        TickDecision::Refuse { line } => {
            assert_eq!(
                line,
                "arm fire workflows/doctor.nika.yaml · chevauchement: remplacer — arrive avec serve v0.2 · aujourd'hui: sauter (le défaut) ou file"
            );
            assert!(line.contains(what), "{line}");
            assert!(line.contains(arrives), "{line}");
        }
        _ => panic!("the firer line is the same policy emit refuses"),
    }
    let ctx = EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/projet"),
        PathBuf::from("/projet/nika.yaml"),
        None,
        PathBuf::from("/projet/.nika/arm/logs"),
        "Europe/Paris".to_owned(),
    );
    match emit::render(&registry, &ctx, Target::Launchd, Mode::PerBeat) {
        Err(EmitRefusal::UnsupportedInV0 {
            what: got,
            arrives: got_arrives,
            ..
        }) => {
            assert_eq!(got, what);
            assert_eq!(got_arrives, arrives);
        }
        other => panic!("emit must refuse the same D6 pair, got {other:?}"),
    }
}
