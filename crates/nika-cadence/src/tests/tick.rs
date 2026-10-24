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
use crate::tick::{TickDecision, expiry_passed, suspension_over, tick_decision};

fn at(text: &str) -> Zoned {
    text.parse::<Timestamp>()
        .expect("ts")
        .to_zoned(TimeZone::UTC)
}

fn registry_with(body: &str) -> ArmRegistry {
    let text = format!(
        "nika: proj\narm:\n  - workflow: workflows/doctor.nika\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.25\n{body}"
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
        TickDecision::Fire { slot, slots, .. } => {
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
        TickDecision::Fire { slot, slots, first } => {
            assert_eq!(slot.timestamp().to_string(), "2026-08-19T03:00:00Z");
            assert_eq!(slots, Some(2), "the 18th AND the 19th");
            assert_eq!(
                first.map(|first| first.timestamp().to_string()),
                Some("2026-08-18T03:00:00Z".to_owned()),
                "the silence starts on the 18th"
            );
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

    // An ACTIVE beat carrying `jusqu_au` is the registry law's to refuse (a bound belongs to a
    // suspension): parsed without that law, the firer still skips it as expired, never firing it.
    let expired = crate::parse_registry(
        "nika: proj\narm:\n  - workflow: workflows/doctor.nika\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.25\n    manqué: sauter\n    jusqu_au: \"2026-01-01\"\n",
    )
    .expect("parse");
    match tick_decision(&expired, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
        TickDecision::Skip { reason, .. } => assert_eq!(reason, "expired"),
        _ => panic!("un beat expiré saute"),
    }
}

/// A lawful paused beat on `cadence`, suspended until 12 October 2026.
fn paused_until_the_12th(cadence: &str) -> ArmRegistry {
    let text = format!(
        "nika: proj\narm:\n  - workflow: workflows/doctor.nika\n    cadence: \"{cadence}\"\n    plafond: 0.25\n    manqué: sauter\n    actif: false\n    raison: \"travaux\"\n    jusqu_au: \"2026-10-12\"\n"
    );
    let registry = crate::parse_registry(&text).expect("parse");
    assert!(
        crate::validate(&registry).next().is_none(),
        "the fixture must be lawful"
    );
    registry
}

fn over(cadence: &str, now: &Zoned) -> bool {
    let registry = paused_until_the_12th(cadence);
    suspension_over(registry.beats().next().expect("beat"), now)
}

/// The same instant, carried in Tokyo's zone (UTC+9 all year).
fn from_tokyo(text: &str) -> Zoned {
    at(text).with_time_zone(crate::next::bundled_tz("Asia/Tokyo").expect("zone"))
}

#[test]
fn east_of_utc_a_suspension_ends_at_the_zone_midnight() {
    // Auckland is UTC+13 on 12 October 2026: its 13th begins at 11:00Z on the 12th.
    let cadence = "TZ=Pacific/Auckland 0 9 * * *";
    assert!(!over(cadence, &at("2026-10-12T10:59:59Z")));
    assert!(over(cadence, &at("2026-10-12T11:00:00Z")));
}

#[test]
fn west_of_utc_a_suspension_outlives_the_utc_date() {
    // Los Angeles is UTC-7 on 12 October 2026: its 13th begins at 07:00Z on the 13th.
    let cadence = "TZ=America/Los_Angeles 0 9 * * *";
    assert!(!over(cadence, &at("2026-10-13T06:59:59Z")));
    assert!(over(cadence, &at("2026-10-13T07:00:00Z")));
}

#[test]
fn the_instant_zone_never_moves_the_cadence_date() {
    // Paris is UTC+2 on 12 October 2026: its 13th begins at 22:00Z, Tokyo's at 15:00Z.
    let cadence = "TZ=Europe/Paris 0 9 * * *";
    assert!(!over(cadence, &from_tokyo("2026-10-12T21:59:59Z")));
    assert!(over(cadence, &from_tokyo("2026-10-12T22:00:00Z")));
}

#[test]
fn a_webhook_beat_reads_the_utc_date() {
    assert!(!over("on-webhook", &from_tokyo("2026-10-12T23:59:59Z")));
    assert!(over("on-webhook", &from_tokyo("2026-10-13T00:00:00Z")));
}

#[test]
fn a_beat_without_a_date_is_never_over() {
    let registry = registry_with(BASE);
    let beat = registry.beats().next().expect("beat");
    assert!(!suspension_over(beat, &at("2099-01-01T00:00:00Z")));
}

#[test]
fn expiry_is_strictly_before_the_decision_date() {
    let registry = registry_with(
        "    manqué: sauter\n    actif: false\n    raison: \"travaux\"\n    jusqu_au: \"2026-08-19\"\n",
    );
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
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika\n",
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
            "arm fire workflows/doctor.nika · chevauchement: remplacer — ce tireur ne tient pas cette politique · aujourd'hui: sauter (le défaut) ou file",
        ),
        (
            "    manqué: sauter\n    chevauchement: sauter\n    après_saut: à-complétion\n",
            "arm fire workflows/doctor.nika · après_saut: à-complétion — ce tireur ne tient pas cette politique · aujourd'hui: prochain-créneau (le défaut)",
        ),
        (
            "    manqué: sauter\n    décalage: hash\n",
            "arm fire workflows/doctor.nika · décalage: — ce tireur ne tient pas cette politique · aujourd'hui le créneau tire à l'instant dit",
        ),
        (
            "    manqué: rattraper\n",
            "arm fire workflows/doctor.nika · manqué: rattraper — ce tireur ne tient pas cette politique · aujourd'hui: rattraper-une-fois ou sauter",
        ),
        (
            "    manqué: { rattraper: 3 }\n",
            "arm fire workflows/doctor.nika · manqué: { rattraper: N } — ce tireur ne tient pas cette politique · aujourd'hui: rattraper-une-fois ou sauter",
        ),
    ] {
        let registry = registry_with(extra);
        match tick_decision(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
            TickDecision::Refuse { line: got } => assert_eq!(got, line),
            other => panic!("D6 attendu, reçu {other:?}"),
        }
    }
}

/// The Spec's bounded catch-up `{ rattraper: N }` reads in both YAML forms as
/// the policy it names, and this firer refuses it by name as it refuses
/// `rattraper` (`policy_unsupported`: catch-up is the resident's, `nika
/// serve`), never as another policy. `{ rattraper: 1 }` IS `rattraper`.
#[test]
fn the_bounded_catch_up_is_refused_by_name() {
    let two = std::num::NonZeroU32::new(2).expect("two");
    for form in [
        "    manqué: { rattraper: 2 }\n",
        "    manqué:\n      rattraper: 2\n",
    ] {
        let registry = registry_with(form);
        let beat = registry.beats().next().expect("beat");
        assert_eq!(
            beat.manque,
            Some(crate::MissPolicy::RattraperDerniers(two)),
            "{form}"
        );
        assert_eq!(
            crate::tick::v0_unsupported(beat),
            Some((
                "manqué: { rattraper: N }",
                "ce tireur ne tient pas cette politique"
            ))
        );
        match tick_decision(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
            TickDecision::Refuse { line } => assert!(
                line.contains("manqué: { rattraper: N } — ce tireur ne tient pas cette politique"),
                "{line}"
            ),
            other => panic!("refused by name, got {other:?}"),
        }
    }
    let one = registry_with("    manqué: { rattraper: 1 }\n");
    assert_eq!(
        one.beats().next().expect("beat").manque,
        Some(crate::MissPolicy::Rattraper)
    );
}

/// N is a whole number from 1 under the one key `rattraper`: anything else
/// does not read, never as another policy.
#[test]
fn the_bounded_catch_up_reads_only_a_whole_count_from_one() {
    for form in [
        "    manqué: { rattraper: 0 }\n",
        "    manqué: { rattraper: -1 }\n",
        "    manqué: { rattraper: 2.5 }\n",
        "    manqué: { rattraper: 2, sauter: 1 }\n",
        "    manqué: { sauter: 2 }\n",
        "    manqué: {}\n",
    ] {
        let text = format!(
            "nika: proj\narm:\n  - workflow: workflows/doctor.nika\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.25\n{form}"
        );
        assert!(crate::parse_registry(&text).is_err(), "{form}");
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
    // The present limit, never a promised release (0.123.0 said « arrive
    // avec serve v0.2 », a version that does not keep these policies).
    assert_eq!(arrives, "ce tireur ne tient pas cette politique");
    assert!(!arrives.contains("v0."), "{arrives}");
    match tick_decision(&registry, 0, "doctor", &at("2026-08-19T03:02:00Z"), None) {
        TickDecision::Refuse { line } => {
            assert_eq!(
                line,
                "arm fire workflows/doctor.nika · chevauchement: remplacer — ce tireur ne tient pas cette politique · aujourd'hui: sauter (le défaut) ou file"
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
        "0a1b2c3d".to_owned(),
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

#[test]
fn schedule_decision_wire_is_closed_and_exact() {
    use crate::ScheduleDecision;

    assert_eq!(
        serde_json::to_string(&ScheduleDecision::Scheduled).expect("scheduled wire"),
        "\"scheduled\""
    );
    assert_eq!(
        serde_json::to_string(&ScheduleDecision::CatchUp).expect("catch-up wire"),
        "\"catch_up\""
    );
    assert!(
        serde_json::from_str::<ScheduleDecision>("\"late\"").is_err(),
        "unknown provenance fails closed"
    );
}

/// A suspension sleeps until its `jusqu_au`, saying so, then the OS-unit
/// firer wakes it as an active beat, its silence counted from the wake:
/// the slots it slept through are never a catch-up.
#[test]
fn a_suspension_sleeps_until_its_date_then_fires_without_replaying_it() {
    let registry = registry_with(concat!(
        "    manqué: rattraper-une-fois\n",
        "    actif: false\n",
        "    raison: \"holidays\"\n",
        "    jusqu_au: \"2026-08-31\"\n",
    ));
    let last = at("2026-07-01T03:00:00Z");
    match tick_decision(
        &registry,
        0,
        "doctor",
        &at("2026-08-31T03:02:00Z"),
        Some(&last),
    ) {
        TickDecision::Skip { reason, line, .. } => {
            assert_eq!(reason, "inactive");
            assert!(
                line.contains("holidays") && line.contains("jusqu_au 2026-08-31"),
                "{line}"
            );
        }
        other => panic!("on its own date the suspension holds: {other:?}"),
    }
    match tick_decision(
        &registry,
        0,
        "doctor",
        &at("2026-09-01T03:02:00Z"),
        Some(&last),
    ) {
        TickDecision::Fire { slot, slots, .. } => {
            assert_eq!(slot.timestamp().to_string(), "2026-09-01T03:00:00Z");
            assert_eq!(
                slots, None,
                "on time: the slots it slept through are not missed"
            );
        }
        other => panic!("the day after its date, it wakes: {other:?}"),
    }
}

/// The bound reads the cadence's own zone: « until 31 October » in
/// Europe/Paris ends at midnight Paris, whatever zone the instant rides.
#[test]
fn a_suspension_ends_at_midnight_in_the_cadence_zone() {
    let registry = crate::parse_registry(concat!(
        "nika: proj\narm:\n  - workflow: workflows/doctor.nika\n",
        "    cadence: \"TZ=Europe/Paris 30 0 * * *\"\n",
        "    plafond: 0.25\n    manqué: sauter\n",
        "    actif: false\n    raison: \"holidays\"\n    jusqu_au: \"2026-10-31\"\n",
    ))
    .expect("parse");
    // 22:40Z is 23:40 in Paris on 31 October: the suspension holds.
    assert!(matches!(
        tick_decision(&registry, 0, "doctor", &at("2026-10-31T22:40:00Z"), None),
        TickDecision::Skip { .. }
    ));
    // 23:32Z is 00:32 in Paris on 1 November: the 00:30 slot is on time.
    let woke = tick_decision(&registry, 0, "doctor", &at("2026-10-31T23:32:00Z"), None);
    assert!(matches!(woke, TickDecision::Fire { .. }), "{woke:?}");
}
