// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Reporting is lent by the exact answer act, never by wire text or outcome prose.
#![allow(clippy::expect_used, clippy::panic)]

use super::*;
use nika_session::RefusalClass;
use nika_session::outcome::Incarnation;
use std::sync::Arc;

fn bound(id: QuestionId, key: &str, value: &str, reading: work::ValueSource) -> work::Answered {
    work::Answered::new(
        id,
        work::AnswerAct::Bound {
            key: key.to_owned(),
            value: value.to_owned(),
            reading,
        },
    )
}

#[test]
fn each_bound_source_is_reported_truthfully_without_claiming_save_or_run() {
    let incarnation = Arc::new(Incarnation);
    let id = QuestionId::new("same-wire-witness".into(), &incarnation);
    for (reading, words) in [
        (work::ValueSource::AsTyped, "as you typed it"),
        (work::ValueSource::OfferedKey, "one of the offered answers"),
        (
            work::ValueSource::ModelRead,
            "a verbatim part of your reply, chosen by one reading call",
        ),
        (
            work::ValueSource::SeatDefault,
            "the offered default from your selected connection",
        ),
    ] {
        let answered = bound(id.clone(), "const.currency", "EUR", reading);
        let beat = read(Some(&answered), &id, "const.currency", "the currency code")
            .expect("the same act lends the taken value");
        let Beat::Say(committed) = beat else {
            panic!("reporting never submits: {beat:?}")
        };
        assert_eq!(committed.kind, Kind::Notice);
        assert_eq!(
            committed.text,
            format!("Answer taken · the currency code\n«EUR»\n{words}")
        );
        assert_eq!(committed.question_witness(), None);
        assert_eq!(
            read(Some(&answered), &id, "const.currency", "the currency code"),
            Some(Beat::Say(committed)),
            "snapshot reread is stable"
        );
    }
}

#[test]
fn a_wire_match_from_another_incarnation_and_a_semantic_key_mismatch_report_nothing() {
    let first = Arc::new(Incarnation);
    let second = Arc::new(Incarnation);
    let id = QuestionId::new("same-wire-witness".into(), &first);
    let other = QuestionId::new("same-wire-witness".into(), &second);
    assert_eq!(id.as_str(), other.as_str());
    assert_ne!(id, other);
    let answered = bound(other, "const.currency", "EUR", work::ValueSource::AsTyped);
    assert_eq!(
        read(Some(&answered), &id, "const.currency", "currency"),
        None
    );
    let answered = bound(
        id.clone(),
        "const.column",
        "amount",
        work::ValueSource::AsTyped,
    );
    assert_eq!(
        read(Some(&answered), &id, "const.currency", "currency"),
        None
    );
    let revised = QuestionId::new("new-revision".into(), &first);
    assert_eq!(
        read(Some(&answered), &revised, "const.column", "column"),
        None
    );
}

#[test]
fn a_missing_or_unbound_act_never_claims_a_taken_value() {
    let incarnation = Arc::new(Incarnation);
    let id = QuestionId::new("asked".into(), &incarnation);
    assert_eq!(read(None, &id, "const.currency", "currency"), None);
    for act in [
        work::AnswerAct::Dropped {
            key: "const.currency".into(),
        },
        work::AnswerAct::Restated {
            key: "const.currency".into(),
        },
        work::AnswerAct::Waits {
            key: "const.currency".into(),
            why: "nothing bound".into(),
        },
        work::AnswerAct::Refused {
            class: RefusalClass::WrongState,
        },
    ] {
        let answered = work::Answered::new(id.clone(), act);
        assert_eq!(
            read(Some(&answered), &id, "const.currency", "currency"),
            None
        );
    }
}

#[test]
fn the_report_preserves_the_bound_value_and_original_label_without_normalizing_them() {
    let incarnation = Arc::new(Incarnation);
    let id = QuestionId::new("asked".into(), &incarnation);
    let answered = bound(
        id.clone(),
        "const.destination",
        "exports/été\nfinal  ",
        work::ValueSource::AsTyped,
    );
    let beat = read(
        Some(&answered),
        &id,
        "const.destination",
        "Destination\r\npath  ",
    )
    .expect("the exact value");
    let Beat::Say(committed) = beat else {
        panic!("notice")
    };
    assert_eq!(
        committed.text,
        "Answer taken · Destination\r\npath  \n«exports/été\nfinal  »\nas you typed it"
    );
}
