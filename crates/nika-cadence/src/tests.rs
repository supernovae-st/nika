// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Two families, never confused (§2sexies):
//!
//! - **the calculator (here, L0)** — ZERO clock: every test rides
//!   literal `Zoned` instants (trap ① · there is no clock to drive).
//!   The DST tests traverse the 2026 Paris transitions and pin N1 —
//!   and since the slot-merge correction, they also pin that a
//!   displaced or merged slot is DECLARED ([`Slot::shift`]).
//! - **the dispatcher (later, L4)** — that is where `VirtualClock`
//!   will live, never here.

use jiff::Timestamp;
use jiff::Zoned;
use jiff::tz::TimeZone;
use proptest::prelude::*;

use crate::next::next_slots;
use crate::{
    AfterSkip, ArmRegistry, Cadence, CadenceErrorKind, Locus, MissPolicy, Overlap, Shift,
    parse_registry, validate,
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

/// The next slot's instant as a UTC string.
fn next_at(cadence: &Cadence, from: &str) -> String {
    utc(&cadence
        .next_after(&at(from))
        .expect("un créneau attendu")
        .at)
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
  - workflow: workflows/geo-probe.nika.yaml
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
    assert_eq!(spec.minutes().single(), Some(0));
    assert_eq!(spec.hours().single(), Some(9));
    assert!(spec.dom().is_full(), "une seule encodage · * = plein");
    assert!(spec.months().is_full());
    assert_eq!(
        spec.dow().iter().collect::<Vec<_>>(),
        vec![1],
        "lundi, seul"
    );
}

#[test]
fn parse_readable_form_equals_cron_form() {
    let readable = Cadence::parse("TZ=Europe/Paris lundi 9h07").expect("lisible");
    let cron = Cadence::parse("TZ=Europe/Paris 7 9 * * 1").expect("cron");
    assert_eq!(readable, cron, "les deux écritures nomment le même créneau");
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
    assert_eq!(
        err.span(),
        Some((0, 9)),
        "le refus peint l expression entière"
    );
}

#[test]
fn refuse_six_fields_before_any_field_semantics() {
    // Scar #6 · the count is the TYPE — the sixth field never reaches a
    // field parser, and the refusal paints the whole expression.
    let err = Cadence::parse("TZ=UTC 0 9 * * 1 99").expect_err("6 champs");
    assert_eq!(err.kind(), CadenceErrorKind::FieldCount);
    assert_eq!(err.span(), Some((7, 19)));
    let err = Cadence::parse("TZ=UTC 0 9 * *").expect_err("4 champs");
    assert_eq!(err.kind(), CadenceErrorKind::FieldCount);
}

#[test]
fn refuse_a_field_out_of_range() {
    let err = Cadence::parse("TZ=UTC 61 9 * * 1").expect_err("minute 61");
    assert_eq!(err.kind(), CadenceErrorKind::FieldRange);
    assert_eq!(err.span(), Some((7, 9)), "le token fautif, peint");
    let err = Cadence::parse("TZ=UTC 0 25 * * 1").expect_err("heure 25");
    assert_eq!(err.kind(), CadenceErrorKind::FieldRange);
    assert_eq!(err.span(), Some((9, 11)));
}

#[test]
fn refuse_dom_and_dow_restricted_together() {
    // The Vixie OR trap — judged on the SETS, not the text: a 1-31 dom
    // is every day, so only the dow side restricts and it is ACCEPTED.
    let err = Cadence::parse("TZ=UTC 0 9 1 * 1").expect_err("le OR de Vixie");
    assert_eq!(err.kind(), CadenceErrorKind::DomDowOr);
    assert_eq!(err.span(), Some((11, 16)), "la paire fautive, peinte");
    Cadence::parse("TZ=UTC 0 9 1 * *").expect("dom seul, accepté");
    Cadence::parse("TZ=UTC 0 9 * * 1").expect("dow seul, accepté");
    Cadence::parse("TZ=UTC 0 9 1 * 1-7").expect("1-7 = tous les jours · dom seul restreint");
}

#[test]
fn refuse_a_day_and_month_pair_no_calendar_can_satisfy() {
    // Gate 11, 2026-08-13. `31 4` is a banal typo: five well-formed fields,
    // zero faults from validate, and then covers() is true on no day, so
    // next_after walks its whole horizon and returns None. The beat parses
    // green and never fires, in silence. This grammar's contract is that
    // every refusal is named and teaches its fix; a silent None is the one
    // outcome it forbids.
    let err = Cadence::parse("TZ=UTC 0 9 31 4 *").expect_err("le 31 avril n'existe pas");
    assert_eq!(err.kind(), CadenceErrorKind::DateImpossible);
    assert!(
        err.remedy().contains("30"),
        "le refus enseigne la borne du mois, pas seulement qu'il refuse"
    );

    // The three months that stop at 30, each refused on 31.
    for token in [
        "TZ=UTC 0 9 31 6 *",
        "TZ=UTC 0 9 31 9 *",
        "TZ=UTC 0 9 31 11 *",
    ] {
        assert_eq!(
            Cadence::parse(token).expect_err(token).kind(),
            CadenceErrorKind::DateImpossible
        );
    }
    // February stops at 29, so 30 and 31 are impossible there.
    assert_eq!(
        Cadence::parse("TZ=UTC 0 9 30 2 *")
            .expect_err("30 février")
            .kind(),
        CadenceErrorKind::DateImpossible
    );

    // What must still pass. 29 February is REAL: a leap year makes it fire,
    // just rarely. Refusing it here would be the mirror fault.
    Cadence::parse("TZ=UTC 0 9 29 2 *").expect("le 29 février existe, tous les 4 ans");
    Cadence::parse("TZ=UTC 0 9 31 1 *").expect("le 31 janvier existe");
    Cadence::parse("TZ=UTC 0 9 30 4 *").expect("le 30 avril existe");
    // One satisfiable month in the set is enough for the whole set.
    Cadence::parse("TZ=UTC 0 9 31 4,5 *").expect("mai a un 31, la paire est satisfiable");
    // An unrestricted side can never make the pair impossible.
    Cadence::parse("TZ=UTC 0 9 31 * *").expect("dom seul restreint · janvier a un 31");
    Cadence::parse("TZ=UTC 0 9 * 4 *").expect("mois seul restreint");
}

#[test]
fn the_horizon_reaches_the_next_29_february_across_a_skipped_century() {
    // Gate 11, 2026-08-13. The horizon's own comment read "a 29 February is
    // never further than 4 years". It is false: a century year is leap only
    // when divisible by 400, so 2100 is not. From 2096-03-01 the next 29
    // February is 2104-02-29, about 2920 days out, and a 1500-day horizon
    // returned None. Same silent death as the (dom, month) pair, one layer
    // down: the beat parses green and never fires.
    let feb29 = Cadence::parse("TZ=UTC 0 9 29 2 *").expect("le 29 février");
    assert_eq!(
        next_at(&feb29, "2096-03-01T00:00:00Z"),
        "2104-02-29T09:00:00Z",
        "2100 n'est pas bissextile · le prochain 29 février est en 2104"
    );
    // The ordinary case still lands, and lands FIRST.
    assert_eq!(
        next_at(&feb29, "2028-01-01T00:00:00Z"),
        "2028-02-29T09:00:00Z"
    );
}

#[test]
fn the_month_bound_fails_closed_outside_the_calendar() {
    // Named by a Rust review, 2026-08-13. The default arm used to answer 31,
    // the MOST permissive bound, to the one input nobody vetted. An
    // out-of-range month would then have made the satisfiability guard PASS,
    // which is the silent None the guard exists to kill. Unreachable today
    // and still the wrong direction: a guard fails CLOSED.
    use crate::cron::longest_day_of;
    assert_eq!(longest_day_of(1), 31, "janvier");
    assert_eq!(
        longest_day_of(2),
        29,
        "février · le 29 existe, tous les 4 ans"
    );
    assert_eq!(longest_day_of(4), 30, "avril");
    assert_eq!(longest_day_of(12), 31, "décembre");
    // The two edges of the calendar, and what lies outside it.
    assert_eq!(longest_day_of(0), 0, "aucun mois · ne contribue RIEN");
    assert_eq!(longest_day_of(13), 0, "aucun mois · ne contribue RIEN");
    assert_eq!(longest_day_of(u8::MAX), 0);
    // The property that makes 0 the safe answer: no day a Field<1,31> can
    // hold is <= 0, so an impossible month admits nothing.
    for d in 1..=31u8 {
        assert!(
            d > longest_day_of(0),
            "aucun jour ne tient dans un non-mois"
        );
    }
}

#[test]
fn every_named_alias_resolves_inside_its_field() {
    // The named path in `field_value` returned its value WITHOUT the bound
    // check the numeric path applies. Harmless while every table sits inside
    // its field's range, and a mine otherwise: `Field::set` shifts by
    // `v - LO`, so an alias below LO underflows a u8. Found 2026-08-13 by an
    // adversarial review of the DateImpossible guard — not its target, the
    // mine beside it.
    //
    // The bound now lives in `field_value`; this test guards the TABLES. An
    // out-of-range alias added later falls through to the numeric path,
    // fails to parse, and reddens HERE rather than shifting by a wrapped
    // byte at runtime.
    for (name, want) in [
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
    ] {
        let cadence = Cadence::parse(&format!("TZ=UTC 0 9 * {name} *"))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let Cadence::Cron { spec, .. } = &cadence else {
            panic!("une cadence cron");
        };
        assert_eq!(
            spec.months().single(),
            Some(want),
            "le mois `{name}` doit valoir {want} et rester dans 1..=12"
        );
    }

    for (name, want) in [
        ("sun", 0),
        ("mon", 1),
        ("tue", 2),
        ("wed", 3),
        ("thu", 4),
        ("fri", 5),
        ("sat", 6),
    ] {
        let cadence = Cadence::parse(&format!("TZ=UTC 0 9 * * {name}"))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let Cadence::Cron { spec, .. } = &cadence else {
            panic!("une cadence cron");
        };
        assert_eq!(
            spec.dow().single(),
            Some(want),
            "le jour `{name}` doit valoir {want} et rester dans l origine NOMMÉE"
        );
    }
}

#[test]
fn every_refusal_class_carries_its_own_distinct_slug() {
    // Found while adding `DateImpossible` on 2026-08-13. The compiler
    // guarantees the COUNT (a match on this enum must cover every variant),
    // and it guarantees nothing about the VALUES: two variants returning the
    // same `cadence.*` slug compiles, passes every test, and silently merges
    // two refusal classes at the wire. A consumer switching on the slug would
    // then handle one case believing it handled two.
    //
    // The match below is the forcing function: adding a variant makes THIS
    // FILE fail to compile, which is the reminder to list it. The array is
    // the hand-kept half and the match is the compiler-kept half; neither
    // alone would hold.
    use CadenceErrorKind as K;
    let all = [
        K::Grammar,
        K::TagFrozen,
        K::CeilingNonPositive,
        K::DeferredKey,
        K::TzMissing,
        K::TzUnknown,
        K::FieldCount,
        K::FieldSyntax,
        K::FieldRange,
        K::DomDowOr,
        K::DateImpossible,
        K::PhraseSyntax,
        K::WorkflowPath,
        K::DuplicateWorkflow,
        K::PlafondMissing,
        K::PlafondNonPositive,
        K::ManqueMissing,
        K::ApresSautMisplaced,
        K::SuspensionSansRaison,
        K::SuspensionSansEcheance,
        K::EcheanceSyntaxe,
        K::ToleranceSyntaxe,
        K::DecalageInconnu,
    ];
    for k in all {
        #[allow(clippy::match_same_arms)]
        match k {
            K::Grammar
            | K::TagFrozen
            | K::CeilingNonPositive
            | K::DeferredKey
            | K::TzMissing
            | K::TzUnknown
            | K::FieldCount
            | K::FieldSyntax
            | K::FieldRange
            | K::DomDowOr
            | K::DateImpossible
            | K::PhraseSyntax
            | K::WorkflowPath
            | K::DuplicateWorkflow
            | K::PlafondMissing
            | K::PlafondNonPositive
            | K::ManqueMissing
            | K::ApresSautMisplaced
            | K::SuspensionSansRaison
            | K::SuspensionSansEcheance
            | K::EcheanceSyntaxe
            | K::ToleranceSyntaxe
            | K::DecalageInconnu => {}
        }
    }

    let mut slugs: Vec<&str> = all.iter().map(|k| k.spec_code()).collect();
    assert!(
        slugs.iter().all(|s| s.starts_with("cadence.")),
        "chaque slug porte le plan de la grammaire, jamais une plage NIKA-*"
    );
    let n = slugs.len();
    slugs.sort_unstable();
    slugs.dedup();
    assert_eq!(
        slugs.len(),
        n,
        "deux classes de refus partagent un slug · elles fusionneraient au fil"
    );
}

#[test]
fn the_ceiling_guard_refuses_zero_negative_infinite_and_nan() {
    // Gate 5, 2026-08-13. Three mutants survived on this guard
    // (`!(p > 0.0 && p.is_finite())`): flipping && to ||, > to >=, and the
    // whole guard to false all passed unnoticed. `plafond:` is REQUIRED with
    // no default precisely because it decides who pays; its guard was the
    // last one that should have been untested.
    //
    // Each value below kills a specific mutant:
    //   0.0   → kills `> to >=` (0 >= 0 would accept) AND `&& to ||`
    //           (`p > 0.0` false but is_finite true, so || would accept)
    //   -1.0  → kills the guard-to-false mutant
    //   .inf  → kills the is_finite half
    //   .nan  → NaN fails every comparison; the guard must still catch it
    for value in ["0", "0.0", "-1.0", ".inf", "-.inf", ".nan"] {
        let yaml = format!(
            "
nika: v1
ceiling: 0.50
arm:
  - workflow: workflows/geo-probe.nika.yaml
    cadence: TZ=Europe/Paris 0 9 * * 1
    plafond: {value}
    manqué: sauter
"
        );
        assert!(
            kinds(&yaml).contains(&CadenceErrorKind::PlafondNonPositive),
            "plafond: {value} doit être refusé · un plafond se borne au réel positif"
        );
    }
    // And the healthy one still passes, so the guard is not simply always-on.
    assert!(
        !kinds(HEALTHY).contains(&CadenceErrorKind::PlafondNonPositive),
        "0.35 est un plafond licite"
    );
}

#[test]
fn a_range_bound_is_judged_strictly() {
    // Gate 5, 2026-08-13. `if from > to` carried two survivors (> to ==,
    // > to >=). A single-value range must be ACCEPTED and a reversed one
    // REFUSED; either mutant breaks exactly one of those two.
    Cadence::parse("TZ=UTC 0 9 * * 5-5").expect("5-5 est un intervalle d un seul jour");
    Cadence::parse("TZ=UTC 5-5 9 * * *").expect("5-5 en minutes aussi");
    let err = Cadence::parse("TZ=UTC 0 9 * * 5-1").expect_err("un intervalle inversé");
    assert_eq!(err.kind(), CadenceErrorKind::FieldRange);
}

#[test]
fn an_empty_field_is_distinguishable_from_a_full_one() {
    // Gate 5, 2026-08-13. `Field::is_empty` had three survivors because it
    // has ZERO caller in src: the zero-consumer law at method granularity.
    // Clippy's len_without_is_empty forbids deleting it next to `len`, so it
    // gets the direct test it never had. Parsing is the only public way to
    // reach a Field, and the parser never yields an empty one, which is the
    // property worth pinning.
    let spec = Cadence::parse("TZ=UTC 0 9 * * 1").expect("cadence");
    let Cadence::Cron { spec, .. } = &spec else {
        panic!("une cadence cron");
    };
    assert!(
        !spec.dow().is_empty(),
        "un champ parsé porte au moins un bit"
    );
    assert!(!spec.minutes().is_empty());
    assert!(spec.dom().is_full(), "et * est plein, jamais vide");
    assert!(
        !spec.dom().is_empty(),
        "plein et vide ne sont pas la même chose"
    );
    assert_eq!(spec.dow().len(), 1, "len et is_empty s accordent");
}

#[test]
fn refuse_a_zone_unknown_to_the_embedded_tzdb() {
    let err = Cadence::parse("TZ=Mars/Olympus 0 9 * * 1").expect_err("fuseau inconnu");
    assert_eq!(err.kind(), CadenceErrorKind::TzUnknown);
    assert_eq!(err.span(), Some((3, 15)), "le nom du fuseau, peint");
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
    assert_eq!(spec.dow().single(), Some(0), "dimanche est NOMMÉ 0");
}

#[test]
fn month_and_weekday_names_parse() {
    let cadence = Cadence::parse("TZ=UTC 0 9 * jan mon").expect("noms");
    let Cadence::Cron { spec, .. } = &cadence else {
        panic!("cron");
    };
    assert_eq!(spec.months().single(), Some(1));
    assert_eq!(spec.dow().single(), Some(1));
}

// ── next_after · the pure calculator, literal instants ─────────────

#[test]
fn next_weekly_slot_from_the_previous_sunday() {
    let cadence = Cadence::parse("TZ=Europe/Paris lundi 9h07").expect("cadence");
    let slot = cadence
        .next_after(&at("2026-08-09T12:00:00Z"))
        .expect("un lundi suit toujours un dimanche");
    assert_eq!(utc(&slot.at), "2026-08-10T07:07:00Z");
    assert_eq!(
        slot.shift,
        Shift::Exact,
        "un lundi ordinaire, rien à déclarer"
    );
}

#[test]
fn next_slot_is_strictly_after() {
    let cadence = Cadence::parse("TZ=Europe/Paris lundi 9h07").expect("cadence");
    assert_eq!(
        next_at(&cadence, "2026-08-10T07:07:00Z"),
        "2026-08-17T07:07:00Z"
    );
}

#[test]
fn next_slot_crosses_a_month() {
    let cadence = Cadence::parse("TZ=Europe/Paris 0 9 1 * *").expect("cadence");
    assert_eq!(
        next_at(&cadence, "2026-08-15T00:00:00Z"),
        "2026-09-01T07:00:00Z"
    );
}

#[test]
fn dst_gap_fires_at_the_first_valid_instant() {
    // N1 · 2026-03-29, Paris springs 02:00 → 03:00. A 02:30 slot does
    // not exist: it fires at 03:00 CEST — NEVER a silent skip, never
    // jiff s 03:30 roll-forward — and the slot SAYS it moved.
    let cadence = Cadence::parse("TZ=Europe/Paris 30 2 * * *").expect("cadence");
    let slot = cadence
        .next_after(&at("2026-03-28T12:00:00Z"))
        .expect("le créneau absent tire quand même");
    assert_eq!(
        utc(&slot.at),
        "2026-03-29T01:00:00Z",
        "03:00 CEST, le premier instant licite"
    );
    assert_eq!(
        slot.shift,
        Shift::AdvancedFirstValid,
        "le déplacement est DÉCLARÉ"
    );
    assert_eq!(
        slot.civil.to_string(),
        "2026-03-29T02:30:00",
        "le civil demandé demeure lisible"
    );
}

#[test]
fn dst_gap_at_the_boundary_also_fires_at_first_valid() {
    // N1, the doc s own example · 02:00 absent ⇒ 03:00.
    let cadence = Cadence::parse("TZ=Europe/Paris 0 2 * * *").expect("cadence");
    let slot = cadence
        .next_after(&at("2026-03-28T12:00:00Z"))
        .expect("le créneau absent tire quand même");
    assert_eq!(utc(&slot.at), "2026-03-29T01:00:00Z");
    assert_eq!(slot.shift, Shift::AdvancedFirstValid);
}

#[test]
fn dst_fold_fires_once_at_the_first_occurrence() {
    // N1 · 2026-10-25, Paris falls 03:00 → 02:00: 02:30 exists TWICE.
    // The beat fires at the first occurrence (CEST), declares the fold,
    // and the following slot is the next DAY — the doubled hour fires
    // exactly once.
    let cadence = Cadence::parse("TZ=Europe/Paris 30 2 * * *").expect("cadence");
    let first = cadence
        .next_after(&at("2026-10-24T12:00:00Z"))
        .expect("la première occurrence");
    assert_eq!(
        utc(&first.at),
        "2026-10-25T00:30:00Z",
        "02:30 CEST, la première"
    );
    assert_eq!(first.shift, Shift::FoldedFirst, "le repli est DÉCLARÉ");
    let after = cadence
        .next_after(&first.at)
        .expect("le lendemain, pas la 2e occurrence");
    assert_eq!(
        utc(&after.at),
        "2026-10-26T01:30:00Z",
        "UNE fois, jamais deux"
    );
    assert_eq!(after.shift, Shift::Exact);
}

#[test]
fn the_gap_merge_is_visible_in_the_types() {
    // The correction s whole point (§2unvicies ②) · `0,30 2,3 * * *` on
    // gap day: four civil slots, TWO fires — and the fire that absorbed
    // 02:00/02:30 says so, with the civil it answered for.
    let cadence = Cadence::parse("TZ=Europe/Paris 0,30 2,3 * * *").expect("cadence");
    let slots: Vec<_> = next_slots(&cadence, &at("2026-03-28T12:00:00Z"), 2).collect();
    assert_eq!(slots.len(), 2);
    assert_eq!(utc(&slots[0].at), "2026-03-29T01:00:00Z");
    assert_eq!(slots[0].civil.to_string(), "2026-03-29T02:00:00");
    assert_eq!(slots[0].shift, Shift::AdvancedFirstValid);
    assert_eq!(utc(&slots[1].at), "2026-03-29T01:30:00Z", "03:30 CEST");
    assert_eq!(slots[1].shift, Shift::Exact);
    // « le 29 mars ce beat tire 2 fois au lieu de 4 » se LIT ici.
}

#[test]
fn a_slept_machine_simply_gets_the_next_slot() {
    // The laptop slept 3 days (N2 · no resume, no catch-up in the
    // calculator): it answers the next slot, nothing more.
    let cadence = Cadence::parse("TZ=Europe/Paris 0 6 * * *").expect("cadence");
    assert_eq!(
        next_at(&cadence, "2026-08-13T10:00:00Z"),
        "2026-08-14T04:00:00Z"
    );
}

#[test]
fn a_29_february_lands_inside_the_horizon() {
    let cadence = Cadence::parse("TZ=Europe/Paris 0 9 29 2 *").expect("cadence");
    assert_eq!(
        next_at(&cadence, "2026-08-11T00:00:00Z"),
        "2028-02-29T08:00:00Z"
    );
}

#[test]
fn next_slot_crosses_a_year() {
    let cadence = Cadence::parse("TZ=Europe/Paris 0 0 1 1 *").expect("cadence");
    assert_eq!(
        next_at(&cadence, "2026-12-15T00:00:00Z"),
        "2026-12-31T23:00:00Z"
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
fn display_a_full_spec_stays_short() {
    // The two-encodings regression (§2unvicies ①) · the pre-bitset
    // describe rendered every minute of "* * * * *" — 244 characters.
    let cadence = Cadence::parse("TZ=UTC * * * * *").expect("cadence");
    assert_eq!(cadence.describe(), "TZ=UTC * * * * *");
}

#[test]
fn display_a_webhook() {
    assert_eq!(Cadence::Webhook.describe(), "on-webhook");
}

#[test]
fn the_four_next_dates_the_display_shows() {
    let cadence = Cadence::parse("TZ=Europe/Paris lundi 9h07").expect("cadence");
    let slots: Vec<String> = next_slots(&cadence, &at("2026-08-09T12:00:00Z"), 4)
        .map(|s| utc(&s.at))
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
    assert_eq!(
        validate(&reg).count(),
        0,
        "aucun refus pour un fichier sain"
    );
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
  - workflow: workflows/a.nika.yaml
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
fn the_span_survives_the_law_pass() {
    // The painter's data never dies in transit (F1) — a refusal born on
    // authored text keeps its byte span THROUGH validate().
    let reg = parse_registry(
        "
nika: v1
arm:
  - workflow: workflows/a.nika.yaml
    cadence: TZ=UTC 61 9 * * 1
    plafond: 0.35
    manqué: sauter
",
    )
    .expect("parse");
    let faults: Vec<_> = validate(&reg).collect();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].kind(), CadenceErrorKind::FieldRange);
    assert_eq!(
        faults[0].span(),
        Some((7, 9)),
        "le span survit à validate — sinon il ne peint jamais"
    );
    assert_eq!(faults[0].on(), Some("workflows/a.nika.yaml"));
}

#[test]
fn an_attempted_readable_form_earns_its_own_refusal() {
    // PhraseSyntax is wired: a French sentence with a malformed time is
    // never answered with a cron sermon.
    let err = Cadence::parse("TZ=Europe/Paris lundi 9h7").expect_err("minute à 1 chiffre");
    assert_eq!(err.kind(), CadenceErrorKind::PhraseSyntax);
    assert_eq!(err.span(), Some((22, 25)), "le token horaire fautif, peint");
    let err = Cadence::parse("TZ=UTC Lundi 9h07").expect_err("la casse trahit la tentative");
    assert_eq!(err.kind(), CadenceErrorKind::PhraseSyntax);
    assert_eq!(err.span(), Some((13, 17)));
}

#[test]
fn a_zone_without_fields_says_the_fields_are_missing() {
    let err = Cadence::parse("TZ=Europe/Paris").expect_err("le fuseau seul");
    assert_eq!(err.kind(), CadenceErrorKind::FieldCount);
    assert!(
        err.detail().contains("les 5 champs qui manquent"),
        "le diagnostic ne confond pas fuseau absent et champs absents"
    );
}

#[test]
fn spans_paint_the_raw_input_even_through_trimming() {
    // A quoted YAML scalar can carry leading spaces — the span rebases.
    let err = Cadence::parse("   TZ=UTC 61 9 * * 1").expect_err("minute 61");
    assert_eq!(err.kind(), CadenceErrorKind::FieldRange);
    assert_eq!(
        err.span(),
        Some((10, 12)),
        "la peinture vise l entrée BRUTE"
    );
}

#[test]
fn the_workflow_path_is_relative_to_the_registry() {
    for path in [
        "/etc/cron.d/a.nika.yaml",
        "../hors-registre/a.nika.yaml",
        "beats/.nika.yaml",
        "sub/.nika.yaml",
    ] {
        let yaml = format!(
            "
nika: v1
arm:
  - workflow: {path}
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
"
        );
        let faults = kinds(&yaml);
        assert!(
            faults.contains(&CadenceErrorKind::WorkflowPath),
            "{path} · ni absolu, ni .., ni basename vide — la loi couvre sa phrase"
        );
    }
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

/// ⭐ Un remède MONTRÉ doit être lui-même LÉGAL.
///
/// Le test au-dessus vérifie que le remède est NON VIDE, jamais qu il est
/// JUSTE — le remède était donc décoratif. Il a longtemps montré un chemin
/// pris dans l arbre privé du monorepo · illisible pour qui n est pas nous,
/// et refusé par le garde-fou de confidentialité (`block-private-paths`)
/// dès qu une ligne le réintroduit.
///
/// La confidentialité reste le travail de ce garde-fou · il voit très bien
/// les lignes AJOUTÉES, et une régression en serait une. Ce test couvre ce
/// que lui ne peut pas voir · que le chemin affiché comme LA réparation
/// passe la règle qu il prétend enseigner.
#[test]
fn the_workflow_remedy_is_itself_a_legal_path() {
    let reg = parse_registry(
        "
nika: v1
arm:
  - workflow: /absolu/interdit.nika.yaml
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
",
    )
    .expect("le registre de test se parse");
    let remedy = validate(&reg)
        .find(|f| f.kind() == CadenceErrorKind::WorkflowPath)
        .map(|f| f.remedy().to_owned())
        .expect("la loi du chemin refuse un absolu");

    let montre = remedy
        .trim_start_matches("workflow: ")
        .split(' ')
        .next()
        .expect("le remède montre un chemin");
    let yaml = format!(
        "
nika: v1
arm:
  - workflow: {montre}
    cadence: on-webhook
    plafond: 0.35
    manqué: sauter
"
    );
    assert!(
        !kinds(&yaml).contains(&CadenceErrorKind::WorkflowPath),
        "`{montre}` est montré comme LA réparation · il doit passer la règle qu il enseigne"
    );
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
    assert_eq!(
        next_at(&noon, "2026-01-14T00:00:00Z"),
        "2026-01-14T11:00:00Z",
        "CET = UTC+1"
    );
    assert_eq!(
        next_at(&noon, "2026-07-14T00:00:00Z"),
        "2026-07-14T10:00:00Z",
        "CEST = UTC+2"
    );
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
        prop_assert!(next.at.timestamp() > from_ts, "strictement après");
        prop_assert!(next.at.timestamp().as_second() - secs <= 86_400, "au plus un jour");
    }
}
