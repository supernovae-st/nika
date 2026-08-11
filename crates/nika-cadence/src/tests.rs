// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Two families, never confused (§2sexies):
//!
//! - **the calculator (here, L0)** — ZERO clock: every test rides
//!   literal `Zoned` instants (trap ① · there is no clock to drive).
//!   The DST tests traverse the 2026 Paris transitions and pin N1.
//! - **the dispatcher (later, L4)** — that is where `VirtualClock`
//!   will live, never here.

use jiff::Timestamp;
use jiff::Zoned;
use jiff::tz::TimeZone;
use proptest::prelude::*;

use crate::phrase::next_slots;
use crate::{
    AfterSkip, ArmRegistry, Cadence, CadenceErrorKind, Locus, MissPolicy, Overlap, parse_registry,
    validate,
};

/// A literal instant (UTC) — the calculator's only input.
fn at(ts: &str) -> Zoned {
    ts.parse::<Timestamp>()
        .expect("literal timestamp")
        .to_zoned(TimeZone::UTC)
}

/// The instant a candidate landed on, as a UTC string.
fn utc(z: &Zoned) -> String {
    z.timestamp().to_string()
}

/// Parse + law pass, returning the refusal classes.
fn kinds(yaml: &str) -> Vec<CadenceErrorKind> {
    let reg = parse_registry(yaml).expect("le registre de test se parse");
    validate(&reg).map(|f| f.kind()).collect()
}

const HEALTHY: &str = "
nika: v1
ceiling: 0.50
arm:
  - workflow: dx/workflows/geo-probe.nika.yaml
    cadence: TZ=Europe/Paris 0 9 * * 1
    plafond: 0.35
    manqué: sauter
";

// ── parse · the two forms, the zone inside ─────────────────────────

#[test]
fn parse_cron_with_zone_inside() {
    let cadence = Cadence::parse("TZ=Europe/Paris 0 9 * * 1").expect("cadence");
    let Cadence::Cron { tz, spec } = &cadence else {
        panic!("une cadence cron, pas un webhook");
    };
    assert_eq!(tz, "Europe/Paris");
    assert_eq!(spec.minutes(), &[0]);
    assert_eq!(spec.hours(), &[9]);
    assert_eq!(spec.dom(), &[]);
    assert_eq!(spec.months(), &[]);
    assert_eq!(spec.dow(), &[1]);
}

#[test]
fn parse_readable_form_equals_cron_form() {
    let readable = Cadence::parse("TZ=Europe/Paris lundi 9h07").expect("lisible");
    let cron = Cadence::parse("TZ=Europe/Paris 7 9 * * 1").expect("cron");
    assert_eq!(readable, cron, "les deux formes nomment le même créneau");
}

#[test]
fn parse_webhook_needs_no_zone() {
    assert_eq!(
        Cadence::parse("on-webhook").expect("webhook"),
        Cadence::Webhook
    );
    assert_eq!(
        Cadence::parse("  ON-WEBHOOK  ").expect("webhook, casse libre"),
        Cadence::Webhook
    );
}

#[test]
fn refuse_a_zoneless_cadence() {
    let err = Cadence::parse("0 9 * * 1").expect_err("le fuseau vit DANS l expression");
    assert_eq!(err.kind(), CadenceErrorKind::TzMissing);
    assert_eq!(err.kind().spec_code(), "cadence.tz-missing");
}

#[test]
fn refuse_six_fields_before_any_field_semantics() {
    // Scar #6 · the count dies FIRST — the 6th field is also out of
    // range, and the refusal must still be the count.
    let err = Cadence::parse("TZ=UTC 0 9 * * 1 99").expect_err("6 champs");
    assert_eq!(err.kind(), CadenceErrorKind::FieldCount);
    let err = Cadence::parse("TZ=UTC 0 9 * *").expect_err("4 champs");
    assert_eq!(err.kind(), CadenceErrorKind::FieldCount);
}

#[test]
fn refuse_a_field_out_of_range() {
    let err = Cadence::parse("TZ=UTC 61 9 * * 1").expect_err("minute 61");
    assert_eq!(err.kind(), CadenceErrorKind::FieldRange);
    let err = Cadence::parse("TZ=UTC 0 25 * * 1").expect_err("heure 25");
    assert_eq!(err.kind(), CadenceErrorKind::FieldRange);
}

#[test]
fn refuse_dom_and_dow_restricted_together() {
    // The Vixie OR trap — one side only, never both.
    let err = Cadence::parse("TZ=UTC 0 9 1 * 1").expect_err("le OR de Vixie");
    assert_eq!(err.kind(), CadenceErrorKind::DomDowOr);
    Cadence::parse("TZ=UTC 0 9 1 * *").expect("dom seul, accepté");
    Cadence::parse("TZ=UTC 0 9 * * 1").expect("dow seul, accepté");
}

#[test]
fn refuse_a_zone_unknown_to_the_embedded_tzdb() {
    let err = Cadence::parse("TZ=Mars/Olympus 0 9 * * 1").expect_err("fuseau inconnu");
    assert_eq!(err.kind(), CadenceErrorKind::TzUnknown);
}

#[test]
fn weekday_origin_is_named_and_seven_is_sunday() {
    let seven = Cadence::parse("TZ=UTC 0 9 * * 7").expect("7 = dimanche");
    let zero = Cadence::parse("TZ=UTC 0 9 * * 0").expect("0 = dimanche");
    let named = Cadence::parse("TZ=UTC 0 9 * * sun").expect("sun = dimanche");
    assert_eq!(seven, zero);
    assert_eq!(zero, named);
    let Cadence::Cron { spec, .. } = &seven else {
        panic!("cron");
    };
    assert_eq!(spec.dow(), &[0], "dimanche est NOMMÉ 0");
}

#[test]
fn month_and_weekday_names_parse() {
    let cadence = Cadence::parse("TZ=UTC 0 9 * jan mon").expect("noms");
    let Cadence::Cron { spec, .. } = &cadence else {
        panic!("cron");
    };
    assert_eq!(spec.months(), &[1]);
    assert_eq!(spec.dow(), &[1]);
}

// ── next_after · the pure calculator, literal instants ─────────────

#[test]
fn next_weekly_slot_from_the_previous_sunday() {
    let cadence = Cadence::parse("TZ=Europe/Paris lundi 9h07").expect("cadence");
    let next = cadence
        .next_after(&at("2026-08-09T12:00:00Z"))
        .expect("un lundi suit toujours un dimanche");
    assert_eq!(utc(&next), "2026-08-10T07:07:00Z");
}

#[test]
fn next_slot_is_strictly_after() {
    let cadence = Cadence::parse("TZ=Europe/Paris lundi 9h07").expect("cadence");
    let next = cadence
        .next_after(&at("2026-08-10T07:07:00Z"))
        .expect("la semaine suivante");
    assert_eq!(
        utc(&next),
        "2026-08-17T07:07:00Z",
        "jamais deux tirs au même instant"
    );
}

#[test]
fn next_slot_crosses_a_month() {
    let cadence = Cadence::parse("TZ=Europe/Paris 0 9 1 * *").expect("cadence");
    let next = cadence
        .next_after(&at("2026-08-15T00:00:00Z"))
        .expect("le 1er du mois suivant");
    assert_eq!(utc(&next), "2026-09-01T07:00:00Z");
}

#[test]
fn dst_gap_fires_at_the_first_valid_instant() {
    // N1 · 2026-03-29, Paris springs 02:00 → 03:00. A 02:30 slot does
    // not exist: it fires at 03:00 CEST — NEVER a silent skip, and
    // never jiff s 03:30 roll-forward.
    let cadence = Cadence::parse("TZ=Europe/Paris 30 2 * * *").expect("cadence");
    let next = cadence
        .next_after(&at("2026-03-28T12:00:00Z"))
        .expect("le créneau absent tire quand même");
    assert_eq!(
        utc(&next),
        "2026-03-29T01:00:00Z",
        "03:00 CEST, le premier instant valide"
    );
}

#[test]
fn dst_gap_at_the_boundary_also_fires_at_first_valid() {
    // N1, the doc s own example · 02:00 absent ⇒ 03:00.
    let cadence = Cadence::parse("TZ=Europe/Paris 0 2 * * *").expect("cadence");
    let next = cadence
        .next_after(&at("2026-03-28T12:00:00Z"))
        .expect("le créneau absent tire quand même");
    assert_eq!(utc(&next), "2026-03-29T01:00:00Z");
}

#[test]
fn dst_fold_fires_once_at_the_first_occurrence() {
    // N1 · 2026-10-25, Paris falls 03:00 → 02:00: 02:30 exists TWICE.
    // The beat fires at the first occurrence (CEST), and the following
    // slot is the next DAY — the doubled hour fires exactly once.
    let cadence = Cadence::parse("TZ=Europe/Paris 30 2 * * *").expect("cadence");
    let first = cadence
        .next_after(&at("2026-10-24T12:00:00Z"))
        .expect("la première occurrence");
    assert_eq!(
        utc(&first),
        "2026-10-25T00:30:00Z",
        "02:30 CEST, la première"
    );
    let after = cadence
        .next_after(&first)
        .expect("le lendemain, pas la seconde occurrence");
    assert_eq!(utc(&after), "2026-10-26T01:30:00Z", "UNE fois, jamais deux");
}

#[test]
fn a_slept_machine_simply_gets_the_next_slot() {
    // The laptop slept 3 days (N2 · no resume, no catch-up in the
    // calculator): it answers the next slot, nothing more.
    let cadence = Cadence::parse("TZ=Europe/Paris 0 6 * * *").expect("cadence");
    let next = cadence
        .next_after(&at("2026-08-13T10:00:00Z"))
        .expect("demain 6h");
    assert_eq!(utc(&next), "2026-08-14T04:00:00Z");
}

#[test]
fn a_29_february_lands_inside_the_horizon() {
    let cadence = Cadence::parse("TZ=Europe/Paris 0 9 29 2 *").expect("cadence");
    let next = cadence
        .next_after(&at("2026-08-11T00:00:00Z"))
        .expect("2028 est bissextile · 567 jours, sous l horizon de 1500");
    assert_eq!(utc(&next), "2028-02-29T08:00:00Z");
}

#[test]
fn next_slot_crosses_a_year() {
    let cadence = Cadence::parse("TZ=Europe/Paris 0 0 1 1 *").expect("cadence");
    let next = cadence
        .next_after(&at("2026-12-15T00:00:00Z"))
        .expect("le jour de l an");
    assert_eq!(
        utc(&next),
        "2026-12-31T23:00:00Z",
        "minuit CET, le 1er janvier"
    );
}

#[test]
fn a_webhook_has_no_calendar() {
    let cadence = Cadence::parse("on-webhook").expect("webhook");
    assert_eq!(cadence.next_after(&at("2026-08-11T00:00:00Z")), None);
}

// ── describe · display normalizes to the readable form ─────────────

#[test]
fn display_normalizes_to_the_readable_form() {
    let cron = Cadence::parse("TZ=Europe/Paris 7 9 * * 1").expect("cron");
    assert_eq!(cron.describe(), "lundi 9h07 · Europe/Paris");
    let readable = Cadence::parse("TZ=Europe/Paris lundi 9h07").expect("lisible");
    assert_eq!(readable.describe(), "lundi 9h07 · Europe/Paris");
}

#[test]
fn display_keeps_cron_when_not_a_single_weekly_slot() {
    let cadence = Cadence::parse("TZ=UTC */15 9-17 * * 1-5").expect("cadence");
    assert_eq!(
        cadence.describe(),
        "TZ=UTC 0,15,30,45 9,10,11,12,13,14,15,16,17 * * 1,2,3,4,5"
    );
}

#[test]
fn display_a_webhook() {
    assert_eq!(Cadence::Webhook.describe(), "on-webhook");
}

#[test]
fn the_four_next_dates_the_display_shows() {
    let cadence = Cadence::parse("TZ=Europe/Paris lundi 9h07").expect("cadence");
    let slots: Vec<String> = next_slots(&cadence, &at("2026-08-09T12:00:00Z"), 4)
        .map(|z| utc(&z))
        .collect();
    assert_eq!(
        slots,
        [
            "2026-08-10T07:07:00Z",
            "2026-08-17T07:07:00Z",
            "2026-08-24T07:07:00Z",
            "2026-08-31T07:07:00Z",
        ]
    );
}

#[test]
fn a_webhook_yields_no_dates() {
    assert_eq!(
        next_slots(&Cadence::Webhook, &at("2026-08-11T00:00:00Z"), 4).count(),
        0
    );
}

// ── validate · the laws, every refusal named ───────────────────────

#[test]
fn a_healthy_registry_validates_clean() {
    let reg = parse_registry(HEALTHY).expect("parse");
    assert_eq!(validate(&reg).count(), 0, "aucun refus sur un fichier sain");
    assert_eq!(reg.nika, ArmRegistry::SCHEMA);
    assert_eq!(reg.beat_count(), 1);
    let beat = reg.beats().next().expect("un beat");
    assert_eq!(beat.locus(), Locus::Local, "défaut sûr · loi ⑥");
    assert_eq!(beat.overlap(), Overlap::Sauter, "défaut sûr · loi ⑥");
    assert_eq!(
        beat.after_skip(),
        AfterSkip::ProchainCreneau,
        "défaut sûr · loi ⑥"
    );
    assert!(beat.is_active());
    assert_eq!(beat.manque, Some(MissPolicy::Sauter));
}

#[test]
fn manque_and_plafond_are_required_without_default() {
    let faults = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
",
    );
    assert!(
        faults.contains(&CadenceErrorKind::PlafondMissing),
        "la loi qui paie"
    );
    assert!(
        faults.contains(&CadenceErrorKind::ManqueMissing),
        "la loi du raté"
    );
    assert_eq!(faults.len(), 2);
}

#[test]
fn a_suspension_tells_its_reason_and_its_expiry() {
    let faults = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    actif: false
",
    );
    assert!(faults.contains(&CadenceErrorKind::SuspensionSansRaison));
    assert!(faults.contains(&CadenceErrorKind::SuspensionSansEcheance));
    assert_eq!(faults.len(), 2);

    let clean = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    actif: false
    raison: migration du fournisseur
    jusqu_au: 2026-09-01
",
    );
    assert_eq!(
        clean.len(),
        0,
        "une suspension racontée et bornée est saine"
    );

    let bad_date = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    actif: false
    raison: migration
    jusqu_au: le 1er septembre
",
    );
    assert!(bad_date.contains(&CadenceErrorKind::EcheanceSyntaxe));
}

#[test]
fn apres_saut_only_rides_a_sauter_overlap() {
    let faults = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    chevauchement: file
    après_saut: à-complétion
",
    );
    assert!(faults.contains(&CadenceErrorKind::ApresSautMisplaced));

    let clean = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    chevauchement: sauter
    après_saut: à-complétion
",
    );
    assert_eq!(clean.len(), 0);
}

#[test]
fn round_two_keys_are_refused_by_name() {
    for key in ["signature: \"ed25519:abc\"", "budget: 10.0"] {
        let yaml = format!(
            "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    {key}
"
        );
        let faults = kinds(&yaml);
        assert!(
            faults.contains(&CadenceErrorKind::DeferredKey),
            "{key} · refusé nommément, jamais ignoré"
        );
    }
    for key in ["traces: {}", "registry: {}"] {
        let yaml = format!("nika: v1\n{key}\n");
        let faults = kinds(&yaml);
        assert!(
            faults.contains(&CadenceErrorKind::DeferredKey),
            "{key} · refusé nommément, jamais ignoré"
        );
    }
}

#[test]
fn a_workflow_armed_twice_is_a_refusal() {
    let faults = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
",
    );
    assert_eq!(faults, [CadenceErrorKind::DuplicateWorkflow]);
}

#[test]
fn the_envelope_tag_is_frozen() {
    let faults = kinds("nika: v2\n");
    assert!(faults.contains(&CadenceErrorKind::TagFrozen));
}

#[test]
fn an_unknown_key_dies_at_parse() {
    let err = parse_registry("nika: v1\nfoo: bar\n").expect_err("clé inconnue");
    assert_eq!(err.kind(), CadenceErrorKind::Grammar);
    let err = parse_registry(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    surprise: true
",
    )
    .expect_err("clé inconnue dans un beat");
    assert_eq!(err.kind(), CadenceErrorKind::Grammar);
}

#[test]
fn tolerance_is_the_m_over_k_firm_form() {
    let faults = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    tolérance: 4/3
",
    );
    assert!(
        faults.contains(&CadenceErrorKind::ToleranceSyntaxe),
        "m ≤ k"
    );

    let clean = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    tolérance: 3/4
",
    );
    assert_eq!(clean.len(), 0);
}

#[test]
fn only_hash_jitter_exists() {
    let faults = kinds(
        "
nika: v1
arm:
  - workflow: dx/workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
    décalage: aleatoire
",
    );
    assert!(faults.contains(&CadenceErrorKind::DecalageInconnu));
}

#[test]
fn a_ceiling_is_a_positive_finite_number() {
    for ceiling in ["0", "-2"] {
        let faults = kinds(&format!("nika: v1\nceiling: {ceiling}\n"));
        assert!(faults.contains(&CadenceErrorKind::CeilingNonPositive));
    }
}

#[test]
fn the_workflow_path_names_a_nika_yaml() {
    let faults = kinds(
        "
nika: v1
arm:
  - workflow: README.md
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
",
    );
    assert!(faults.contains(&CadenceErrorKind::WorkflowPath));
}

#[test]
fn every_refusal_teaches_its_fix() {
    // The error contract: a refusal is never bare — it carries the law,
    // the remedy, and a slug of this grammar s own namespace.
    let reg = parse_registry(
        "
nika: v2
arm:
  - workflow: README.md
    cadence: on-webhook
    actif: false
",
    )
    .expect("parse");
    let faults: Vec<_> = validate(&reg).collect();
    assert!(faults.len() >= 4, "plusieurs lois brisées");
    for fault in &faults {
        assert!(!fault.detail().is_empty());
        assert!(
            !fault.remedy().is_empty(),
            "chaque refus enseigne sa réparation"
        );
        assert!(fault.kind().spec_code().starts_with("cadence."));
    }
}

// ── trap ③ · the EMBEDDED tzdb, never the host ─────────────────────

#[test]
fn the_embedded_tzdb_carries_real_dst_rules() {
    // Behavioral proof: these offsets come from the tzdb baked at build
    // time — a host without /usr/share/zoneinfo computes the same.
    assert!(
        jiff_tzdb::get("Europe/Paris").is_some(),
        "la base embarquée répond"
    );
    let noon = Cadence::parse("TZ=Europe/Paris 0 12 * * *").expect("cadence");
    let winter = noon
        .next_after(&at("2026-01-14T00:00:00Z"))
        .expect("midi en hiver");
    assert_eq!(utc(&winter), "2026-01-14T11:00:00Z", "CET = UTC+1");
    let summer = noon
        .next_after(&at("2026-07-14T00:00:00Z"))
        .expect("midi en été");
    assert_eq!(utc(&summer), "2026-07-14T10:00:00Z", "CEST = UTC+2");
}

#[test]
fn no_zone_ever_resolves_from_the_host() {
    // Static proof: no shipped line may call the host-preferring
    // resolvers. The needles are built at runtime so this very file
    // does not trip its own guard.
    let tz_get = concat!("TimeZone", "::get");
    let in_tz = concat!(".in", "_tz(");
    let dir = env!("CARGO_MANIFEST_DIR");
    for file in [
        "cron.rs",
        "error.rs",
        "lib.rs",
        "next.rs",
        "parse.rs",
        "phrase.rs",
        "registry.rs",
        "tests.rs",
    ] {
        let text = std::fs::read_to_string(format!("{dir}/src/{file}")).expect("la source se lit");
        for (n, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            assert!(
                !line.contains(tz_get) && !line.contains(in_tz),
                "{file}:{} · un fuseau se résout depuis la base EMBARQUÉE (jiff_tzdb::get), jamais depuis l hôte",
                n + 1
            );
        }
    }
}

// ── proptest · the calculator never panics, never lies ─────────────

proptest! {
    #[test]
    fn parse_never_panics(s in any::<String>()) {
        let _ = Cadence::parse(&s);
    }

    #[test]
    fn the_law_pass_never_panics(s in any::<String>()) {
        if let Ok(reg) = parse_registry(&s) {
            let _ = validate(&reg).count();
        }
    }

    #[test]
    fn a_daily_slot_is_strictly_later_and_within_a_day(secs in 0i64..=4_102_444_800) {
        let cadence = Cadence::parse("TZ=UTC 30 4 * * *").expect("cadence");
        let from_ts = Timestamp::from_second(secs).expect("instant");
        let from = from_ts.to_zoned(TimeZone::UTC);
        let next = cadence
            .next_after(&from)
            .expect("un beat quotidien a toujours un prochain créneau");
        prop_assert!(next.timestamp() > from_ts, "strictement après");
        prop_assert!(next.timestamp().as_second() - secs <= 86_400, "au plus un jour");
    }
}
