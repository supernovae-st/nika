// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source compatibility of `nika_compile_cognition::{decide, rehearse}` after the two modules
//! moved to the size-cap member below the seats' doors, `nika_compile_seats` (2026-10-07 ·
//! ADR-146). This file is an external consumer: it compiles against the cognition paths, and
//! they name the very same items — the same types (a value of one path IS a value of the
//! other), the same traits (a seat or a room written against one is one of the other), the same
//! functions (their signatures carry the member's types) and the same constants — never copies.
//! The one observable difference is a type's name at run time, which now names the member.
#![allow(clippy::expect_used)]

use std::any::type_name;
use std::time::Duration;

use nika_compile_cognition::decide::{
    ChoiceAnswer, ChoiceFuture, ChoiceOption, ChoiceQuestion, DecisionSeat,
};
use nika_compile_cognition::rehearse::{
    Attempt, EffectCounts, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse,
};
use nika_compile_seats as member;
use serde_json::{Value, json};

/// Each function of the cognition path, typed with the member's types: a cognition-side copy of
/// these items could not be assigned here.
const KEYS: fn(&member::decide::ChoiceQuestion) -> Vec<String> = ChoiceQuestion::keys;
const RECORD: fn(
    &member::decide::ChoiceQuestion,
    Result<&member::decide::ChoiceAnswer, &member::decide::DecisionError>,
) -> Value = nika_compile_cognition::decide::record;
const NONE_EFFECTS: fn() -> member::rehearse::EffectCounts = EffectCounts::none;

/// A seat written against the member's trait.
struct First;

impl member::decide::DecisionSeat for First {
    fn name(&self) -> &'static str {
        "fixture/first"
    }
    fn choose<'a>(&'a self, question: &'a member::decide::ChoiceQuestion) -> ChoiceFuture<'a> {
        let choice = question.options[0].key.clone();
        Box::pin(async move { Ok(ChoiceAnswer::new(choice, "fixture/first")) })
    }
}

/// A room written against the cognition path's trait, which never runs anything.
struct Refusing;

impl Rehearse for Refusing {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        let outcome = Rehearsal::NotRun {
            reason: "fixture".to_owned(),
        };
        let report =
            RehearsalReport::new(outcome, Attempt::NeverAttempted, NONE_EFFECTS(), candidate);
        Box::pin(async move { report })
    }
    fn bound(&self) -> Duration {
        Duration::from_secs(1)
    }
}

#[test]
fn the_cognition_paths_name_the_members_items() {
    // One type under two paths: values move between them without conversion.
    let question: member::decide::ChoiceQuestion = ChoiceQuestion::new(
        "q",
        "pick one",
        json!({"clause": "c"}),
        vec![ChoiceOption::new("a", "the a")],
    );
    assert_eq!(
        KEYS(&question),
        ["a", nika_compile_cognition::decide::NONE_OPTION]
    );
    assert_eq!(
        nika_compile_cognition::decide::NONE_OPTION,
        member::decide::NONE_OPTION
    );
    // A seat of the member's trait is a seat of the cognition path's, and its answer is admitted
    // and recorded by the functions under either path.
    let seat: &dyn DecisionSeat = &First;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    let answer = runtime
        .block_on(seat.choose(&question))
        .expect("a fixed answer");
    assert!(nika_compile_cognition::decide::admit(&question, &answer).is_ok());
    assert_eq!(RECORD(&question, Ok(&answer))["choice"], "a");
    assert_eq!(
        member::decide::record(&question, Ok(&answer)),
        nika_compile_cognition::decide::record(&question, Ok(&answer))
    );
    // A room of the cognition path's trait is a room of the member's.
    let room: &dyn member::rehearse::Rehearse = &Refusing;
    let report: member::rehearse::RehearsalReport =
        runtime.block_on(room.rehearse("nika: c\n", &[]));
    assert!(report.effects.is_none());
    assert_eq!(report.attempt, member::rehearse::Attempt::NeverAttempted);
    assert_eq!(room.bound(), Duration::from_secs(1));
}

/// What changed for a consumer that looks at metadata: a type's run-time name names the member
/// (the same name under both paths, and no longer the cognition's).
#[test]
fn only_the_run_time_type_name_names_the_member() {
    for (cognition, owner) in [
        (
            type_name::<ChoiceQuestion>(),
            type_name::<member::decide::ChoiceQuestion>(),
        ),
        (
            type_name::<RehearsalReport>(),
            type_name::<member::rehearse::RehearsalReport>(),
        ),
        (
            type_name::<nika_compile_cognition::decide::ProviderChoice<'static, Never>>(),
            type_name::<member::decide::ProviderChoice<'static, Never>>(),
        ),
    ] {
        assert_eq!(cognition, owner);
        assert!(owner.starts_with("nika_compile_seats::"), "{owner}");
    }
}

/// A provider no test calls: only its type is named.
type Never = nika_compile_cognition::NoProvider;
