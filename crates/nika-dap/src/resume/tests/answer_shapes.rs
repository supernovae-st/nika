// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
/// #1284 (c) · duplicate `--answer` flags for ONE task: identical
/// values dedupe silently (a retried CI line is not an error);
/// CONFLICTING values refuse — last-wins was a silent coin toss on
/// the strongest mechanism of the release.
#[test]
fn duplicate_conflicting_answers_refuse_and_identical_ones_dedupe() {
    const WF: &str = "nika: t\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"go?\" }\n";
    let wf = nika_schema::parse(
        WF,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    let same = parse_answers(&["ask=true".to_owned(), "ask=true".to_owned()], &wf)
        .expect("identical duplicates dedupe");
    assert_eq!(same["ask"], serde_json::json!(true));
    let err = parse_answers(&["ask=true".to_owned(), "ask=false".to_owned()], &wf)
        .expect_err("conflicting duplicates refuse");
    assert!(
        err.contains("conflicting") && err.contains("true") && err.contains("false"),
        "the refusal names both values: {err}"
    );
}

/// #1284 (c) · the answer's SHAPE is judged against the gate's
/// declared mode at parse time — a `confirm` typo (`banana`) used to
/// ride to the builtin, fail PROMPT-001 and silently pause. A
/// literal mode refuses upfront with the teaching; a templated mode
/// stays un-judged here (the runtime gate owns the resolved mode).
#[test]
fn answer_shapes_are_judged_against_the_declared_mode() {
    let parse = |yaml: &str| {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses")
    };
    let confirm = parse(
        "nika: t\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"go?\" }\n",
    );
    let err =
        parse_answers(&["ask=banana".to_owned()], &confirm).expect_err("confirm wants a bool");
    assert!(
        err.contains("confirm") && err.contains("true or false"),
        "teaches the shape: {err}"
    );
    // The default mode IS confirm (stdlib §prompt) — same law.
    let bare = parse(
        "nika: t\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"go?\" }\n",
    );
    assert!(parse_answers(&["ask=yes".to_owned()], &bare).is_ok());
    assert!(parse_answers(&["ask=true".to_owned()], &bare).is_ok());
    let input = parse(
        "nika: t\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"input\", message: \"name?\" }\n",
    );
    assert!(parse_answers(&["ask=amel".to_owned()], &input).is_ok());
    // An input gate takes the text typed, a number it spells included.
    let typed = parse_answers(&["ask=42".to_owned()], &input).expect("input takes text");
    assert_eq!(typed["ask"], serde_json::json!("42"));
    let choice = parse(
        "nika: t\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"choice\", message: \"pick?\", choices: [\"a\", \"b\"] }\n",
    );
    assert!(parse_answers(&["ask=a".to_owned()], &choice).is_ok());
    let err = parse_answers(&["ask=z".to_owned()], &choice).expect_err("outside the choices");
    assert!(
        err.contains("choices") && err.contains('a') && err.contains('b'),
        "names the accepted set: {err}"
    );
}

#[test]
fn templated_choices_defer_membership_to_runtime() {
    let wf = nika_schema::parse("nika: t\ntasks:\n  ask:\n    invoke:\n      tool: nika:prompt\n      args: { mode: choice, message: pick, choices: [a, '${{ inputs.other }}'] }\n", nika_schema::FileId::new(0), nika_schema::ParseMode::Strict).expect("parses");
    assert!(parse_answers(&["ask=b".to_owned()], &wf).is_ok());
    // The text typed rides to the runtime, which judges it against the resolved choices.
    let typed = parse_answers(&["ask=42".to_owned()], &wf).expect("text for the runtime");
    assert_eq!(typed["ask"], serde_json::json!("42"));
}

/// A choice gate whose choices spell numbers or booleans takes them as the resume line prints
/// them (`--answer review=42`), never refused as a number that is not one of its strings; a JSON
/// string still gives its text, and a word outside the choices is still refused.
#[test]
fn a_choice_spelling_a_number_is_answered_as_the_resume_line_prints_it() {
    let wf = nika_schema::parse(
        "nika: t\ntasks:\n  review:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"choice\", message: \"score?\", choices: [\"42\", \"17\", \"true\"] }\n",
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parses");
    for (typed, chosen) in [
        ("review=42", "42"),
        ("review=\"17\"", "17"),
        ("review=true", "true"),
    ] {
        let answers = parse_answers(&[typed.to_owned()], &wf).expect("one of the choices");
        assert_eq!(answers["review"], serde_json::json!(chosen), "{typed}");
    }
    let err = parse_answers(&["review=43".to_owned()], &wf).expect_err("not a choice");
    assert!(
        err.contains("42") && err.contains("17"),
        "names the choices: {err}"
    );
}
