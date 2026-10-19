// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The first result, then guided feedback (NIK-174/175): a digest the person asked to save and
//! run completes and writes its report; the optional offer at the free prompt proposes what to
//! adjust; two dimensions picked reach the author, which asks the one precision missing, amends
//! the workflow and proposes it; its own run then offers a neutral opinion bound to that
//! adjustment. Every answer is recorded with the exact request, revision, run and report it is
//! about, and none saves, runs or sends anything. Words typed instead go to the conversation
//! whatever their bytes read as, a mixed set is refused, another prompt keeps the next line, a
//! run's review closes the offer, bytes already offered are not offered again (a reopen
//! included), and a new request never inherits an adjustment.

use serde_json::{Value, json};

use crate::peer::Kind;
use crate::scenarios::{
    BRACKETED, DIGEST, FIRST_REPORT, LATER_IN_WORDS, SECOND_REPORT, TOTAL, UNRELATED,
};
use crate::{outcome_kind, run, waiting, work};

/// The offer the step's snapshot shows (`null` when none).
fn offer(step: &Value) -> &Value {
    &work(step)["feedback"]["offer"]
}

fn recorded(step: &Value) -> Vec<Value> {
    (work(step)["feedback"]["recorded"].as_array().cloned()).unwrap_or_default()
}

fn option_keys(offer: &Value) -> Vec<String> {
    (offer["question"]["options"]
        .as_array()
        .into_iter()
        .flatten())
    .filter_map(|option| option["key"].as_str().map(str::to_owned))
    .collect()
}

fn standing(offer: &Value) -> Vec<String> {
    (offer["question"]["options"]
        .as_array()
        .into_iter()
        .flatten())
    .filter(|option| option["exclusive"] == true)
    .filter_map(|option| option["key"].as_str().map(str::to_owned))
    .collect()
}

/// The Session's witness of `text`'s bytes, as it witnesses a project file it reads again.
fn witness(text: &str) -> String {
    nika_session::change::Witness::of(text.as_bytes()).0
}

/// The first result's offer: what to adjust, from the digest's own tasks, « Nothing to change »
/// and « Later » alone, following nothing, at the free prompt, bound to the run's report. Its
/// scope.
fn first_offer(first: &Value) -> Value {
    assert_eq!(outcome_kind(first), "facts", "{first:#}");
    let offered = offer(first);
    assert_eq!(offered["kind"], "adjust", "{first:#}");
    assert_eq!(
        option_keys(offered),
        [
            "depth",
            "length",
            "topics",
            "sources",
            "presentation",
            "nothing",
            "later"
        ]
    );
    assert_eq!(standing(offered), ["nothing", "later"]);
    assert_eq!(offered["question"]["multi_select"], true);
    assert!(
        offered["follows"].is_null(),
        "a first result follows nothing"
    );
    assert_eq!(
        offered["scope"]["artifacts"],
        json!([{"path": DIGEST, "witness": witness(FIRST_REPORT)}])
    );
    assert_eq!(waiting(first), "free", "an offer never takes the next line");
    offered["scope"].clone()
}

/// The corrected result's offer: the neutral opinion, nothing recommended, following the
/// `adjustment` record, a later revision of the same request than `scope`, bound to the amended
/// run's report. Its scope.
fn opinion_offer(second: &Value, scope: &Value, adjustment: &Value) -> Value {
    let opinion = offer(second);
    assert_eq!(opinion["kind"], "opinion", "{second:#}");
    assert_eq!(
        option_keys(opinion),
        ["suitable", "changes", "wrong", "unevaluated", "later"]
    );
    let none_recommended = (opinion["question"]["options"]
        .as_array()
        .into_iter()
        .flatten())
    .all(|option| option["recommended"] == false);
    assert!(none_recommended, "{opinion:#}");
    assert_eq!(
        opinion["follows"], adjustment["question"],
        "the adjustment it amends"
    );
    assert_eq!(
        opinion["scope"]["request"], scope["request"],
        "the same request"
    );
    assert!(
        opinion["scope"]["revision"].as_u64() > scope["revision"].as_u64(),
        "a later revision: {opinion:#}"
    );
    let started = scope["source_sha256"].as_str().expect("started bytes");
    assert_ne!(
        opinion["scope"]["source_sha256"], started,
        "the amended bytes"
    );
    assert_eq!(
        opinion["scope"]["artifacts"],
        json!([{"path": DIGEST, "witness": witness(SECOND_REPORT)}])
    );
    opinion["scope"].clone()
}

/// The whole first witness: the result, the offer, two dimensions, one precision, the amended
/// run on the person's consent, the opinion bound to the adjustment; each answer bound to its
/// request, revision, run and report.
#[test]
fn a_result_then_a_guided_correction_then_a_neutral_opinion() {
    let journey = run("feedback_guided_correction");
    let scope = first_offer(journey.step(3));

    let picked = journey.step(4);
    assert!(offer(picked).is_null(), "answered: {picked:#}");
    let records = recorded(picked);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["kind"], "adjust");
    assert_eq!(records[0]["picked"], json!(["depth", "topics"]));
    assert_eq!(
        records[0]["scope"], scope,
        "bound to the offer's exact result"
    );
    assert_eq!(
        outcome_kind(picked),
        "question",
        "the author asks the one precision"
    );
    let told = (journey.agent.iter()).any(|seen| {
        seen.text()
            .contains("chose to adjust: Depth of the summary, Topics covered")
    });
    assert!(
        told,
        "the picked dimensions reach the author by their labels"
    );

    let corrected = opinion_offer(journey.step(7), &scope, &records[0]);
    let noted = journey.step(8);
    assert_eq!(outcome_kind(noted), "facts", "{noted:#}");
    let records = recorded(noted);
    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["kind"], "opinion");
    assert_eq!(records[1]["picked"], json!(["suitable"]));
    assert_eq!(records[1]["scope"], corrected);
    assert_eq!(records[1]["follows"], records[0]["question"]);
    for at in [4, 8] {
        assert_ne!(
            outcome_kind(journey.step(at)),
            "run_requested",
            "an answer runs nothing"
        );
    }
    let readings = (journey.seen.iter())
        .filter(|seen| seen.kind == Kind::Pick)
        .count();
    assert_eq!(
        readings, 4,
        "the acceptance, two consents and the precision; never an answer"
    );
}

/// Words typed at the free prompt are the person's next line to the conversation, whatever
/// their bytes read as (a bracket that opens a list of keys, an offered key): the offer closes
/// unanswered, records nothing, and its identity answers nothing afterwards.
#[test]
fn words_at_the_prompt_are_lines_never_an_answer() {
    let journey = run("feedback_words_are_lines");
    assert_eq!(offer(journey.step(3))["kind"], "adjust");
    let typed = journey.step(4);
    assert!(offer(typed).is_null(), "{typed:#}");
    assert!(recorded(typed).is_empty());
    for words in [BRACKETED, LATER_IN_WORDS] {
        assert!(
            journey.agent.iter().any(|seen| seen.text().contains(words)),
            "{words}"
        );
    }
    assert!(
        recorded(journey.step(5)).is_empty(),
        "a key in words picks nothing"
    );
    let late = journey.step(6);
    assert_eq!(outcome_kind(late), "refusal", "{late:#}");
    assert!(recorded(late).is_empty());
}

/// « Nothing to change » mixed with a dimension is refused and the offer waits; alone, it is
/// recorded and nothing else happens.
#[test]
fn nothing_to_change_stands_alone() {
    let journey = run("feedback_mixed_refused");
    let mixed = journey.step(4);
    assert_eq!(outcome_kind(mixed), "refusal", "{mixed:#}");
    assert_eq!(mixed["outcome"]["class"], "not_allowed");
    assert_eq!(offer(mixed)["kind"], "adjust", "the offer waits");
    let alone = journey.step(5);
    assert_eq!(outcome_kind(alone), "facts", "{alone:#}");
    assert_eq!(recorded(alone)[0]["picked"], json!(["nothing"]));
    assert!(offer(alone).is_null());
    assert_eq!(journey.agent.len(), 5, "no answer reached the author");
}

/// While another prompt owns the next line, the offer's identity answers nothing: refused, nothing
/// recorded, the author never told, the offer still listed for when nothing else waits.
#[test]
fn another_prompt_keeps_the_next_line() {
    let journey = run("feedback_waits_behind_a_prompt");
    assert_ne!(waiting(journey.step(4)), "free", "{:#}", journey.step(4));
    let refused = journey.step(5);
    assert_eq!(outcome_kind(refused), "refusal", "{refused:#}");
    assert_eq!(refused["outcome"]["class"], "wrong_state");
    assert!(recorded(refused).is_empty());
    assert_eq!(offer(refused)["kind"], "adjust", "the offer still waits");
    assert_eq!(journey.agent.len(), 5, "no answer reached the author");
}

/// A run's review closes the offer about the earlier result: its identity answers nothing.
#[test]
fn a_run_review_closes_the_offer() {
    let journey = run("feedback_closed_by_a_run_review");
    assert!(offer(journey.step(4)).is_null(), "{:#}", journey.step(4));
    let late = journey.step(5);
    assert_eq!(outcome_kind(late), "refusal", "{late:#}");
    assert!(recorded(late).is_empty());
    assert_eq!(journey.agent.len(), 5, "no answer reached the author");
}

/// Bytes offered and left unanswered are not offered again once the conversation reopened and
/// ran them again: the history kept them, never a live offer.
#[test]
fn a_reopen_never_offers_the_same_bytes_again() {
    let journey = run("feedback_reopened_offers_nothing_again");
    assert_eq!(offer(journey.step(3))["kind"], "adjust");
    assert!(offer(journey.step(5)).is_null(), "no live offer comes back");
    assert_eq!(outcome_kind(journey.step(6)), "run_requested");
    let again = journey.step(7);
    assert!(offer(again).is_null(), "{again:#}");
    assert!(recorded(again).is_empty());
}

/// An adjustment picked, then a new request replaces the work: the new result is offered the
/// ordinary adjustment from its own tasks, bound to its own request, never the opinion of a
/// corrected result.
#[test]
fn a_new_request_never_inherits_an_adjustment() {
    let journey = run("feedback_new_request_never_inherits");
    let first = offer(journey.step(3)).clone();
    assert_eq!(recorded(journey.step(4))[0]["picked"], json!(["topics"]));
    assert!(
        journey
            .agent
            .iter()
            .any(|seen| seen.text().contains(UNRELATED))
    );
    let counted = journey.step(7);
    let offered = offer(counted);
    assert_eq!(offered["kind"], "adjust", "{counted:#}");
    assert!(offered["follows"].is_null(), "{counted:#}");
    assert_ne!(
        offered["scope"]["request"], first["scope"]["request"],
        "another request"
    );
    assert_eq!(
        option_keys(offered),
        ["topics", "sources", "presentation", "nothing", "later"],
        "the count's own tasks: no model writes it"
    );
    assert_eq!(
        offered["scope"]["artifacts"],
        json!([{"path": "./out/total.txt", "witness": witness(TOTAL)}])
    );
}

/// The same saved bytes run again offer nothing again: one offer per revision run.
#[test]
fn the_same_bytes_run_again_offer_nothing_again() {
    let journey = run("feedback_rerun_offers_nothing");
    assert_eq!(recorded(journey.step(4))[0]["picked"], json!(["later"]));
    assert_eq!(outcome_kind(journey.step(5)), "run_requested");
    let again = journey.step(6);
    assert!(offer(again).is_null(), "{again:#}");
    assert_eq!(recorded(again).len(), 1);
}
