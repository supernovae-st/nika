// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A schedule's binding values are asked beside the candidate without blocking it: the
//! timezone, the missed-run policy, the overlap policy and the per-run ceiling, as
//! non-mandatory questions whose closed options are the owning grammars' own spellings.
//! An answer is admitted against those spellings and echoed on `requested_trigger`; a wrong
//! answer is a finding and the question stays. The candidate's bytes never carry them.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    CompileOutcome, CompileQuestion, CompileRequest, CompileStatus, DiagnosticKind, QuestionType,
    Strategy, TriggerKind, TriggerStatus, compile, outcome_document,
};
use serde_json::Value;

const WEEKDAYS: &str = "Every weekday at 8, read ./tickets.json, keep only the rows whose status is open and write them to ./open.json";

fn question<'a>(out: &'a CompileOutcome, key: &str) -> &'a CompileQuestion {
    out.questions
        .iter()
        .find(|q| q.key == key)
        .expect("the binding question is asked")
}

fn keys(out: &CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

#[test]
fn a_schedule_asks_its_four_binding_values_without_blocking_the_candidate() {
    let out = compile(&CompileRequest::create(WEEKDAYS)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    let trigger = out.requested_trigger.as_ref().expect("a trigger");
    assert_eq!(trigger.kind, TriggerKind::Schedule);
    assert_eq!(trigger.status, TriggerStatus::RequiresBinding);
    assert_eq!(trigger.cadence.as_deref(), Some("weekdays"));
    assert_eq!(trigger.at.as_deref(), Some("08:00"));
    assert_eq!(
        keys(&out),
        [
            "trigger.timezone",
            "trigger.missed",
            "trigger.overlap",
            "trigger.ceiling"
        ],
        "{out:#?}"
    );
    assert!(out.questions.iter().all(|q| !q.mandatory), "{out:#?}");
    assert_eq!(
        question(&out, "trigger.timezone").answer_type,
        QuestionType::Text
    );
    assert_eq!(
        question(&out, "trigger.ceiling").answer_type,
        QuestionType::Literal
    );
    let missed = question(&out, "trigger.missed");
    assert_eq!(missed.answer_type, QuestionType::Choice);
    assert_eq!(
        missed
            .options
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["rattraper", "rattraper-une-fois", "sauter"]
    );
    let overlap = question(&out, "trigger.overlap");
    assert_eq!(overlap.answer_type, QuestionType::Choice);
    assert_eq!(
        overlap
            .options
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["sauter", "file", "remplacer"]
    );
    // The candidate's bytes carry no cadence, timezone or policy.
    let candidate = out.candidate.as_deref().unwrap();
    assert!(
        !candidate.contains("weekday") && !candidate.contains("08:00"),
        "{candidate}"
    );
    assert!(!candidate.contains("trigger"), "{candidate}");
    // The wire: a choice question carries its options; the trigger carries the four values.
    let doc = outcome_document(&out);
    let wire: Vec<&Value> = doc["questions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|q| q["type"] == "choice")
        .collect();
    assert_eq!(wire.len(), 2, "{doc:#}");
    assert_eq!(wire[0]["options"][0]["key"], "rattraper", "{doc:#}");
    assert!(wire[0]["options"][0]["label"].is_string(), "{doc:#}");
    assert!(
        doc["questions"][0]["options"].is_null(),
        "a text question carries no options: {doc:#}"
    );
    assert!(doc["requested_trigger"]["timezone"].is_null(), "{doc:#}");
    assert_eq!(doc["status"], "ready");
    assert_eq!(doc["provenance"]["suggested_file"], "open.nika", "{doc:#}");
}

#[test]
fn answered_binding_values_are_admitted_against_the_grammar_and_echoed() {
    let out = compile(
        &CompileRequest::create(WEEKDAYS)
            .answer("trigger.timezone", r#""Europe/Paris""#)
            .answer("trigger.missed", r#""rattraper-une-fois""#)
            .answer("trigger.overlap", r#""file""#)
            .answer("trigger.ceiling", "0.25"),
    )
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).is_empty(), "{out:#?}");
    let trigger = out.requested_trigger.as_ref().unwrap();
    assert_eq!(trigger.timezone.as_deref(), Some("Europe/Paris"));
    assert_eq!(trigger.missed.as_deref(), Some("rattraper-une-fois"));
    assert_eq!(trigger.overlap.as_deref(), Some("file"));
    assert_eq!(trigger.ceiling.as_deref(), Some("0.25"));
    let doc = outcome_document(&out);
    assert_eq!(doc["requested_trigger"]["missed"], "rattraper-une-fois");
    assert_eq!(doc["requested_trigger"]["ceiling"], "0.25");
    // A wrong value is a finding, the question stays and nothing is guessed.
    let out = compile(
        &CompileRequest::create(WEEKDAYS)
            .answer("trigger.missed", r#""catch-up""#)
            .answer("trigger.ceiling", "-1"),
    )
    .unwrap();
    assert!(keys(&out).contains(&"trigger.missed"), "{out:#?}");
    assert!(keys(&out).contains(&"trigger.ceiling"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Missed && d.target == "trigger.missed"),
        "{out:#?}"
    );
    assert!(out.requested_trigger.as_ref().unwrap().missed.is_none());
    // No trigger, no binding question.
    let out = compile(&CompileRequest::create(
        "Read ./tickets.json, keep only the rows whose status is open and write them to ./open.json",
    ))
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).is_empty(), "{out:#?}");
    assert!(out.requested_trigger.is_none());
    assert_eq!(out.provenance.suggested_file.as_deref(), Some("open.nika"));
}

/// The closed options are the owning grammars' spellings, never the compiler's: a drift of
/// either grammar fails here before it reaches a product.
#[test]
fn the_choice_options_are_the_grammars_own_spellings() {
    let out = compile(&CompileRequest::create(WEEKDAYS)).unwrap();
    let overlap: Vec<String> = question(&out, "trigger.overlap")
        .options
        .iter()
        .map(|o| o.key.clone())
        .collect();
    // The cadence grammar reads each offered key as its own policy, and reads nothing else.
    let read =
        |key: &str| serde_json::from_value::<nika_cadence::Overlap>(Value::String(key.to_owned()));
    assert_eq!(read(&overlap[0]).unwrap(), nika_cadence::Overlap::Sauter);
    assert_eq!(read(&overlap[1]).unwrap(), nika_cadence::Overlap::File);
    assert_eq!(read(&overlap[2]).unwrap(), nika_cadence::Overlap::Remplacer);
    assert_eq!(overlap.len(), 3);
    assert!(read("queue").is_err());
    let missed: Vec<String> = question(&out, "trigger.missed")
        .options
        .iter()
        .map(|o| o.key.clone())
        .collect();
    let project: Vec<String> = [
        nika_vocab::project::MissPolicy::Rattraper,
        nika_vocab::project::MissPolicy::RattraperUneFois,
        nika_vocab::project::MissPolicy::Sauter,
    ]
    .iter()
    .map(|policy| policy.as_str().to_owned())
    .collect();
    assert_eq!(missed, project);
}
