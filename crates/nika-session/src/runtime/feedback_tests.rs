// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The optional offer about a result, piece by piece: what an answer picks (exactly offered keys,
//! « Nothing to change » and « Later » alone), the dimensions a result is offered from its own
//! tasks, the neutral opinion's choices, and what a reopened conversation keeps (records, the
//! bytes already offered and the adjustment awaiting its amended run, never a live offer). The
//! whole path, from a run's journal to the author's amendment and the opinion, is the
//! `open_ux_journeys` witness.

use std::sync::Arc;

use super::feedback::{Feedback, dimensions, opinions, picked};
use crate::outcome::{Incarnation, QuestionId};
use crate::work::{
    AskedQuestion, FeedbackArtifact, FeedbackKind, FeedbackOffer, FeedbackRecord, FeedbackScope,
    Offer,
};

/// A digest: two pages fetched, a summary a model writes, the report written.
const DIGEST: &str = r#"nika: news-digest
model: vllm/oux-author
permits:
  tools: ["nika:fetch", "nika:write"]
  net:
    http: ["news.ycombinator.com"]
  fs:
    write: ["./news/digest.md"]
tasks:
  hacker_news:
    invoke:
      tool: "nika:fetch"
      args: { url: "https://news.ycombinator.com/", method: GET }
  summarize:
    with: { page: "${{ tasks.hacker_news.output }}" }
    infer:
      max_tokens: 800
      prompt: "Summarize in Markdown: ${{ with.page }}"
  write_digest:
    with: { digest: "${{ tasks.summarize.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./news/digest.md", content: "${{ with.digest }}" }
"#;

/// A copy: one file read, written as is, no model.
const COPY: &str = r#"nika: copy
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./in/source.txt"]
    write: ["./out/copied.txt"]
tasks:
  load:
    invoke:
      tool: "nika:read"
      args: { path: "./in/source.txt" }
  save:
    with: { text: "${{ tasks.load.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/copied.txt", content: "${{ with.text }}" }
"#;

fn keys(offers: &[Offer]) -> Vec<&str> {
    offers.iter().map(|offer| offer.key.as_str()).collect()
}

fn alone(offers: &[Offer]) -> Vec<&str> {
    (offers.iter())
        .filter(|offer| offer.exclusive)
        .map(|offer| offer.key.as_str())
        .collect()
}

/// An offer's question over `options`, several checked together when `multiple`.
fn asked(options: Vec<Offer>, multiple: bool) -> AskedQuestion {
    let id = QuestionId::new("feedback".to_owned(), &Arc::new(Incarnation));
    AskedQuestion::new(id, "adjust", "What would you like to adjust?")
        .with_options(options, true, multiple)
}

#[test]
fn a_result_is_offered_the_dimensions_its_own_tasks_have() {
    let digest = dimensions(DIGEST);
    assert_eq!(
        keys(&digest),
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
    assert_eq!(alone(&digest), ["nothing", "later"]);
    assert!(digest.iter().all(|offer| !offer.recommended), "no default");
    let copy = dimensions(COPY);
    assert_eq!(
        keys(&copy),
        ["topics", "sources", "presentation", "nothing", "later"],
        "no model writes it: no depth nor length"
    );
    assert_eq!(
        keys(&dimensions("not a workflow")),
        ["presentation", "nothing", "later"]
    );
}

#[test]
fn the_opinion_is_neutral_and_later_stands_alone() {
    let opinion = opinions();
    assert_eq!(
        keys(&opinion),
        ["suitable", "changes", "wrong", "unevaluated", "later"]
    );
    assert!(opinion.iter().all(|offer| !offer.recommended), "no default");
    assert_eq!(alone(&opinion), ["later"]);
}

#[test]
fn an_answer_picks_exactly_offered_keys_in_their_order() {
    let adjust = asked(dimensions(DIGEST), true);
    assert_eq!(
        picked(&adjust, r#"["topics","depth"]"#),
        Ok(vec!["depth".to_owned(), "topics".to_owned()])
    );
    assert_eq!(picked(&adjust, " length "), Ok(vec!["length".to_owned()]));
    assert_eq!(
        picked(&adjust, r#"["nothing"]"#),
        Ok(vec!["nothing".to_owned()])
    );
    for refused in [
        "",
        "[]",
        r#"["depth","depth"]"#,
        r#"["depth","colour"]"#,
        "make it shorter",
        "[not json",
        "[urgent] make it shorter",
    ] {
        assert!(picked(&adjust, refused).is_err(), "{refused}");
    }
    for mixed in [
        r#"["nothing","depth"]"#,
        r#"["later","topics"]"#,
        r#"["nothing","later"]"#,
    ] {
        let why = picked(&adjust, mixed).expect_err(mixed);
        assert!(why.contains("stand alone"), "{mixed}: {why}");
    }
    let opinion = asked(opinions(), false);
    assert_eq!(picked(&opinion, "wrong"), Ok(vec!["wrong".to_owned()]));
    assert!(
        picked(&opinion, r#"["suitable","wrong"]"#).is_err(),
        "one only"
    );
}

/// The answer `picked` to an offer of `kind` about revision `revision` of request `request`,
/// whose started bytes hash to `source`, following the adjustment `follows`.
fn record(
    (kind, picked): (FeedbackKind, &[&str]),
    (request, revision, source): (u64, u64, &str),
    follows: Option<&str>,
) -> FeedbackRecord {
    let artifact = FeedbackArtifact::new("news/digest.md", "ab".repeat(32));
    let scope = FeedbackScope::new(
        (request, revision),
        source,
        "01a10897-52a2-7758-aa31-024a0098d5b1",
        vec![artifact],
    );
    let options = if kind == FeedbackKind::Adjust {
        dimensions(DIGEST)
    } else {
        opinions()
    };
    let question = asked(options, kind == FeedbackKind::Adjust);
    let offer = FeedbackOffer::new(kind, question, scope, follows.map(str::to_owned));
    FeedbackRecord::new(&offer, picked.iter().map(|key| (*key).to_owned()).collect())
}

#[test]
fn a_reopened_conversation_keeps_records_never_a_live_offer() {
    let kept = vec![
        record(
            (FeedbackKind::Adjust, &["depth", "topics"]),
            (0, 2, "s1"),
            None,
        ),
        record(
            (FeedbackKind::Opinion, &["suitable"]),
            (0, 3, "s2"),
            Some("feedback"),
        ),
    ];
    let value = serde_json::json!({"recorded": serde_json::to_value(&kept).expect("records")});
    let restored = Feedback::restored(Some(&value));
    let projected = restored.projected();
    assert!(projected.offer.is_none(), "no live offer comes back");
    assert_eq!(projected.recorded, kept);
    assert_eq!(projected.recorded[1].follows.as_deref(), Some("feedback"));
    let again = restored.kept().expect("kept");
    assert_eq!(again["recorded"], value["recorded"]);
    assert_eq!(
        again["offered"],
        serde_json::json!(["s1", "s2"]),
        "answered bytes stay offered"
    );
    assert!(Feedback::restored(None).projected().is_empty());
    assert!(
        Feedback::restored(None).kept().is_none(),
        "nothing offered: nothing kept"
    );
    let unreadable = serde_json::json!({"recorded": "not records"});
    assert!(Feedback::restored(Some(&unreadable)).projected().is_empty());
}

#[test]
fn bytes_offered_but_never_answered_stay_offered_after_a_reopen() {
    let value = serde_json::json!({
        "offered": ["s9"],
        "adjusted": {"request": 4, "revision": 2, "answer": "q"}
    });
    let restored = Feedback::restored(Some(&value));
    assert!(
        restored.projected().is_empty(),
        "no record and no live offer"
    );
    assert_eq!(
        restored.kept(),
        Some(value),
        "the offered bytes and the lineage are kept as read"
    );
}
