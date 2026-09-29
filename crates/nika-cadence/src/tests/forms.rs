// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::unreachable
)]

//! The two forms the grammar gained after the 5-field cron: the last day
//! of a month (`L` in the day-of-month field) and the anchored interval
//! (`every N weeks from DATE HH:MM`). The law both keep is « never
//! approximated »: no 28 to 31 guess, no weekly beat firing twice as often.
//! The OS units may wake more often than the beat (launchd cannot say
//! either form, systemd cannot say the interval); the firer decides, and
//! the wake fold here pins that an extra wake never becomes a fire.

use std::path::PathBuf;

use jiff::Zoned;

use super::{at, utc};
use crate::emit::{self, EmitCtx, Mode, Target};
use crate::schedule::{ScheduleDraft, ScheduleWhenDraft};
use crate::schedule_plan::{ScheduleDecisionState, plan_schedule};
use crate::tick::{TickDecision, tick_decision};
use crate::{ArmRegistry, Cadence, CadenceErrorKind, MissPolicy, Shift, parse_registry, validate};

const EVERY: &str = "TZ=Europe/Paris every 2 weeks from 2026-10-05 09:00";

fn parse(text: &str) -> Cadence {
    Cadence::parse(text).unwrap_or_else(|error| panic!("{text} · {error}"))
}

fn next_utc(text: &str, from: &str) -> String {
    utc(&parse(text).next_after(&at(from)).expect("a slot").at)
}

fn prev_utc(text: &str, from: &str) -> Option<String> {
    parse(text).prev_before(&at(from)).map(|slot| utc(&slot.at))
}

fn one_beat(cadence: &str) -> ArmRegistry {
    let text = format!(
        "nika: proj\narm:\n  - workflow: workflows/w.nika\n    cadence: \"{cadence}\"\n    plafond: 0.25\n    manqué: sauter\n"
    );
    let reg = parse_registry(&text).expect("the registry parses");
    assert_eq!(validate(&reg).count(), 0, "{cadence} is lawful");
    reg
}

fn ctx(tz: &str) -> EmitCtx {
    EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/projet"),
        PathBuf::from("/projet/nika.yaml"),
        None,
        PathBuf::from("/projet/.nika/arm/logs"),
        tz.to_owned(),
    )
}

/// Every wake an OS unit would make, in order, each Fire fed back as the
/// last fired slot: the fires the firer would claim.
fn fires(reg: &ArmRegistry, wakes: &[&str]) -> Vec<String> {
    let mut last: Option<Zoned> = None;
    let mut out = Vec::new();
    for wake in wakes {
        if let TickDecision::Fire { slot, .. } =
            tick_decision(reg, 0, "w", &at(wake), last.as_ref())
        {
            out.push(utc(&slot));
            last = Some(slot);
        }
    }
    out
}

// ── the last day of a month ───────────────────────────────────────────

#[test]
fn month_end_lands_on_the_last_civil_day() {
    let text = "TZ=Europe/Paris 0 9 L * *";
    assert_eq!(
        next_utc(text, "2026-01-15T00:00:00Z"),
        "2026-01-31T08:00:00Z"
    );
    // 2026 is not leap, 2028 is, 2100 is not (a century is leap only by 400).
    assert_eq!(
        next_utc(text, "2026-02-01T00:00:00Z"),
        "2026-02-28T08:00:00Z"
    );
    assert_eq!(
        next_utc(text, "2028-02-01T00:00:00Z"),
        "2028-02-29T08:00:00Z"
    );
    assert_eq!(
        next_utc(text, "2100-02-01T00:00:00Z"),
        "2100-02-28T08:00:00Z"
    );
    // A 30-day month, in summer time.
    assert_eq!(
        next_utc(text, "2026-04-01T00:00:00Z"),
        "2026-04-30T07:00:00Z"
    );
}

#[test]
fn month_end_walks_one_slot_per_month() {
    let cadence = parse("TZ=Europe/Paris 0 9 L * *");
    let days: Vec<i8> = crate::next_slots(&cadence, &at("2026-01-01T00:00:00Z"), 12)
        .map(|slot| slot.civil.day())
        .collect();
    assert_eq!(days, [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]);
}

#[test]
fn month_end_keeps_the_month_field() {
    let text = "TZ=Europe/Paris 0 9 L 2,8 *";
    assert_eq!(
        next_utc(text, "2026-03-01T00:00:00Z"),
        "2026-08-31T07:00:00Z"
    );
    assert_eq!(
        next_utc(text, "2026-09-01T00:00:00Z"),
        "2027-02-28T08:00:00Z"
    );
}

#[test]
fn month_end_mirror_finds_the_last_month_end() {
    let text = "TZ=Europe/Paris 0 9 L * *";
    assert_eq!(
        prev_utc(text, "2026-03-15T00:00:00Z").as_deref(),
        Some("2026-02-28T08:00:00Z")
    );
    assert_eq!(
        prev_utc(text, "2026-03-31T07:00:00Z").as_deref(),
        Some("2026-03-31T07:00:00Z"),
        "at or before: the slot itself"
    );
}

#[test]
fn month_end_on_a_change_day_follows_n1() {
    // 2024-03-31 is the last day of March and the spring change in Paris:
    // 02:30 does not exist, the beat fires at the first valid instant.
    let cadence = parse("TZ=Europe/Paris 30 2 L * *");
    let slot = cadence
        .next_after(&at("2024-03-15T00:00:00Z"))
        .expect("a slot");
    assert_eq!(utc(&slot.at), "2024-03-31T01:00:00Z");
    assert_eq!(slot.shift, Shift::AdvancedFirstValid);
    // 2021-10-31 is the last day of October and the autumn change: 02:30
    // exists twice, the beat fires once, at the first occurrence.
    let slot = cadence
        .next_after(&at("2021-10-15T00:00:00Z"))
        .expect("a slot");
    assert_eq!(utc(&slot.at), "2021-10-31T00:30:00Z");
    assert_eq!(slot.shift, Shift::FoldedFirst);
}

#[test]
fn month_end_describes_itself_and_round_trips() {
    for text in [
        "TZ=Europe/Paris 0 9 L * *",
        "TZ=Europe/Paris 30 18 L 1,4,7,10 *",
    ] {
        let cadence = parse(text);
        assert_eq!(cadence.describe(), text);
        assert_eq!(parse(&cadence.describe()), cadence);
    }
    // `L` is not a spelling of 28-31: the two beats differ.
    assert_ne!(parse("TZ=UTC 0 9 L * *"), parse("TZ=UTC 0 9 28-31 * *"));
}

#[test]
fn month_end_refuses_every_other_spelling_by_name() {
    for text in [
        "TZ=UTC 0 9 1,L * *",
        "TZ=UTC 0 9 L-2 * *",
        "TZ=UTC 0 9 LW * *",
        "TZ=UTC 0 9 l * *",
        "TZ=UTC 0 9 L/2 * *",
    ] {
        let error = Cadence::parse(text).expect_err(text);
        assert_eq!(error.kind(), CadenceErrorKind::FieldSyntax, "{text}");
        let (a, b) = error.span().expect("the faulty token is painted");
        assert_eq!(
            &text[a..b],
            text.split_whitespace().nth(3).unwrap(),
            "{text}"
        );
        assert!(
            error.remedy().contains("0 9 L * *"),
            "{text} teaches the form · {}",
            error.remedy()
        );
    }
    // The last day beside a restricted weekday is the Vixie OR trap.
    let error = Cadence::parse("TZ=UTC 0 9 L * 1").expect_err("the OR trap");
    assert_eq!(error.kind(), CadenceErrorKind::DomDowOr);
}

// ── the anchored interval ─────────────────────────────────────────────

#[test]
fn every_n_weeks_starts_at_its_anchor_and_keeps_its_parity() {
    // Before the anchor: the anchor itself, in summer time.
    assert_eq!(
        next_utc(EVERY, "2026-09-01T00:00:00Z"),
        "2026-10-05T07:00:00Z"
    );
    // At the anchor: strictly after, two weeks on.
    assert_eq!(
        next_utc(EVERY, "2026-10-05T07:00:00Z"),
        "2026-10-19T07:00:00Z"
    );
    // The off week carries no slot.
    assert_eq!(
        next_utc(EVERY, "2026-10-12T00:00:00Z"),
        "2026-10-19T07:00:00Z"
    );
    // Across the autumn change the civil time holds: 09:00 CET is 08:00Z.
    assert_eq!(
        next_utc(EVERY, "2026-10-19T08:00:00Z"),
        "2026-11-02T08:00:00Z"
    );
}

#[test]
fn every_n_weeks_walks_whole_periods_in_civil_time() {
    let cadence = parse(EVERY);
    let slots: Vec<_> = crate::next_slots(&cadence, &at("2026-09-01T00:00:00Z"), 30).collect();
    assert_eq!(slots.len(), 30);
    for pair in slots.windows(2) {
        let days = pair[0].civil.date().until(pair[1].civil.date()).unwrap();
        assert_eq!(days.get_days(), 14, "{pair:?}");
    }
    assert!(slots.iter().all(|slot| {
        slot.civil.time() == jiff::civil::time(9, 0, 0, 0)
            && slot.civil.weekday() == jiff::civil::Weekday::Monday
            && slot.shift == Shift::Exact
    }));
}

#[test]
fn every_week_and_every_fifty_two_weeks() {
    let weekly = "TZ=UTC every 1 week from 2026-01-05 09:00";
    assert_eq!(
        next_utc(weekly, "2026-01-06T00:00:00Z"),
        "2026-01-12T09:00:00Z"
    );
    let yearly = "TZ=UTC every 52 weeks from 2026-01-05 09:00";
    assert_eq!(
        next_utc(yearly, "2026-01-06T00:00:00Z"),
        "2027-01-04T09:00:00Z"
    );
}

#[test]
fn every_n_weeks_mirror_never_reaches_before_the_anchor() {
    assert_eq!(
        prev_utc(EVERY, "2026-10-25T00:00:00Z").as_deref(),
        Some("2026-10-19T07:00:00Z")
    );
    assert_eq!(
        prev_utc(EVERY, "2026-10-05T07:00:00Z").as_deref(),
        Some("2026-10-05T07:00:00Z"),
        "at or before: the anchor itself"
    );
    assert_eq!(prev_utc(EVERY, "2026-10-05T06:59:59Z"), None);
    // Ten years on the walk is arithmetic, never a day loop: an on-week
    // Monday at 09:00, whole periods after the anchor, within one period.
    let far = at("2036-10-06T00:00:00Z");
    let slot = parse(EVERY).prev_before(&far).expect("a slot");
    let anchor = jiff::civil::date(2026, 10, 5);
    let days = anchor.until(slot.civil.date()).unwrap().get_days();
    assert_eq!(days % 14, 0, "{slot:?}");
    assert!(
        slot.at <= far
            && far.timestamp().as_second() - slot.at.timestamp().as_second() < 14 * 86_400
    );
}

#[test]
fn every_n_weeks_anchor_in_a_gap_follows_n1() {
    let cadence = parse("TZ=Europe/Paris every 1 week from 2026-03-29 02:30");
    let first = cadence
        .next_after(&at("2026-03-01T00:00:00Z"))
        .expect("the anchor");
    assert_eq!(utc(&first.at), "2026-03-29T01:00:00Z");
    assert_eq!(first.shift, Shift::AdvancedFirstValid);
    let second = cadence.next_after(&first.at).expect("a week on");
    assert_eq!(utc(&second.at), "2026-04-05T00:30:00Z");
    assert_eq!(second.shift, Shift::Exact);
}

#[test]
fn every_n_weeks_describes_itself_and_round_trips() {
    for (text, shown) in [
        (EVERY, EVERY),
        (
            "TZ=UTC every 1 weeks from 2026-01-05 9:05",
            "TZ=UTC every 1 week from 2026-01-05 09:05",
        ),
        (
            "TZ=UTC every 3 week from 2026-01-05 23:59",
            "TZ=UTC every 3 weeks from 2026-01-05 23:59",
        ),
    ] {
        let cadence = parse(text);
        assert_eq!(cadence.describe(), shown);
        assert_eq!(parse(&cadence.describe()), cadence);
    }
}

#[test]
fn every_n_weeks_refuses_by_name() {
    for (text, kind) in [
        (
            "TZ=UTC every 0 weeks from 2026-10-05 09:00",
            CadenceErrorKind::FieldRange,
        ),
        (
            "TZ=UTC every 53 weeks from 2026-10-05 09:00",
            CadenceErrorKind::FieldRange,
        ),
        ("TZ=UTC every 2 weeks", CadenceErrorKind::PhraseSyntax),
        (
            "TZ=UTC every 2 weeks from 2026-10-05",
            CadenceErrorKind::PhraseSyntax,
        ),
        ("TZ=UTC every other week", CadenceErrorKind::PhraseSyntax),
        (
            "TZ=UTC every two weeks from 2026-10-05 09:00",
            CadenceErrorKind::PhraseSyntax,
        ),
        (
            "TZ=UTC every 2 weeks from 2026-02-30 09:00",
            CadenceErrorKind::PhraseSyntax,
        ),
        (
            "TZ=UTC every 2 weeks from 2026-10-05 24:00",
            CadenceErrorKind::PhraseSyntax,
        ),
        (
            "TZ=UTC every 2 weeks from 2026-10-05 9h00",
            CadenceErrorKind::PhraseSyntax,
        ),
        (
            "TZ=UTC every 2 days from 2026-10-05 09:00",
            CadenceErrorKind::PhraseSyntax,
        ),
        (
            "TZ=UTC every 2 months from 2026-10-05 09:00",
            CadenceErrorKind::PhraseSyntax,
        ),
        (
            "TZ=UTC Every 2 weeks from 2026-10-05 09:00",
            CadenceErrorKind::PhraseSyntax,
        ),
        (
            "TZ=UTC every 2 weeks since 2026-10-05 09:00",
            CadenceErrorKind::PhraseSyntax,
        ),
        (
            "TZ=UTC every 2 weeks from 2026-10-05 09:00 x",
            CadenceErrorKind::PhraseSyntax,
        ),
        (
            "every 2 weeks from 2026-10-05 09:00",
            CadenceErrorKind::TzMissing,
        ),
    ] {
        let error = Cadence::parse(text).expect_err(text);
        assert_eq!(error.kind(), kind, "{text} · {error}");
        assert!(error.span().is_some(), "{text} paints its byte");
    }
}

// ── the OS units and the firer ────────────────────────────────────────

#[test]
fn systemd_says_the_month_end_exactly() {
    for (cadence, calendar) in [
        (
            "TZ=Europe/Paris 0 9 L * *",
            "OnCalendar=*-*~01 09:00:00 Europe/Paris",
        ),
        (
            "TZ=Europe/Paris 0 9 L 2,8 *",
            "OnCalendar=*-02,08~01 09:00:00 Europe/Paris",
        ),
    ] {
        let units = emit::render(
            &one_beat(cadence),
            &ctx("Europe/Paris"),
            Target::SystemdUser,
            Mode::PerBeat,
        )
        .expect("rendered");
        assert!(units[0].body.contains(calendar), "{}", units[0].body);
    }
}

#[test]
fn launchd_wakes_on_the_month_end_superset() {
    let units = emit::render(
        &one_beat("TZ=Europe/Paris 0 9 L * *"),
        &ctx("Europe/Paris"),
        Target::Launchd,
        Mode::PerBeat,
    )
    .expect("rendered");
    let body = &units[0].body;
    assert_eq!(body.matches("<key>Day</key>").count(), 4, "{body}");
    for day in 28..=31 {
        assert!(
            body.contains(&format!("<integer>{day}</integer>")),
            "{body}"
        );
    }
}

#[test]
fn every_n_weeks_emits_its_weekday_on_both_targets() {
    let reg = one_beat(EVERY);
    let units = emit::render(
        &reg,
        &ctx("Europe/Paris"),
        Target::SystemdUser,
        Mode::PerBeat,
    )
    .expect("rendered");
    assert!(
        units[0]
            .body
            .contains("OnCalendar=Mon *-*-* 09:00:00 Europe/Paris"),
        "{}",
        units[0].body
    );
    let units =
        emit::render(&reg, &ctx("Europe/Paris"), Target::Launchd, Mode::PerBeat).expect("rendered");
    let body = &units[0].body;
    assert_eq!(
        body.matches("<dict>").count(),
        2,
        "the root and one interval: {body}"
    );
    for (key, value) in [("Hour", 9), ("Minute", 0), ("Weekday", 1)] {
        assert!(
            body.contains(&format!(
                "<key>{key}</key>\n\t\t\t<integer>{value}</integer>"
            )),
            "{key} {value}: {body}"
        );
    }
}

#[test]
fn four_month_end_wakes_give_one_fire() {
    let reg = one_beat("TZ=UTC 0 9 L * *");
    let wakes = [
        "2026-01-28T09:00:00Z",
        "2026-01-29T09:00:00Z",
        "2026-01-30T09:00:00Z",
        "2026-01-31T09:00:00Z",
        "2026-02-28T09:00:00Z",
        "2026-03-28T09:00:00Z",
        "2026-03-29T09:00:00Z",
        "2026-03-30T09:00:00Z",
        "2026-03-31T09:00:00Z",
    ];
    assert_eq!(
        fires(&reg, &wakes),
        [
            "2026-01-31T09:00:00Z",
            "2026-02-28T09:00:00Z",
            "2026-03-31T09:00:00Z"
        ]
    );
}

#[test]
fn off_week_wakes_never_fire_across_a_change() {
    let reg = one_beat(EVERY);
    let wakes = [
        "2026-09-28T07:00:00Z",
        "2026-10-05T07:00:00Z",
        "2026-10-12T07:00:00Z",
        "2026-10-19T07:00:00Z",
        "2026-10-26T08:00:00Z",
        "2026-11-02T08:00:00Z",
    ];
    assert_eq!(
        fires(&reg, &wakes),
        [
            "2026-10-05T07:00:00Z",
            "2026-10-19T07:00:00Z",
            "2026-11-02T08:00:00Z"
        ]
    );
}

#[test]
fn a_schedule_keeps_both_forms_canonical_and_plans_them() {
    for (text, canonical, first) in [
        (
            "TZ=Europe/Paris 0 9 L * *",
            "TZ=Europe/Paris 0 9 L * *",
            "2026-10-31T08:00:00Z",
        ),
        (
            "TZ=Europe/Paris every 2 week from 2026-10-05 9:00",
            EVERY,
            "2026-10-05T07:00:00Z",
        ),
    ] {
        let draft = ScheduleDraft::new(
            "s",
            "w.nika",
            ScheduleWhenDraft::Cadence {
                expression: text.to_owned(),
            },
            0.25,
            MissPolicy::Sauter,
        );
        let definition = draft
            .validate()
            .unwrap_or_else(|f| panic!("{text} · {f:?}"));
        assert_eq!(definition.when().cadence_expression(), Some(canonical));
        let plan = plan_schedule(
            &definition,
            &at("2026-10-01T00:00:00Z"),
            &ScheduleDecisionState::empty(),
            3,
        )
        .unwrap_or_else(|e| panic!("{text} · {e:?}"));
        let next = plan.next_slots().next().expect("a projected slot");
        assert_eq!(next.scheduled_for().to_string(), first, "{text}");
    }
}
