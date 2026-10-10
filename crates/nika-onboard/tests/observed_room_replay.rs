// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A room lent captures is a replay trial of a conversation's candidate:
//! - a plain GET of exactly a captured address is answered with the capture's own bytes, and the
//!   run that wrote them is read back, with no request leaving the room;
//! - a model step the trial cannot exercise, and the write behind it, are left to the room and
//!   never fail the trial: the run completes and passes on what it did exercise;
//! - the same candidates lent nothing are refused before any room, as every rehearsal's are.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod room_support;
use nika_onboard::compile::rehearse::Rehearsal;
use nika_service_execution::replay::{Capture, Captures};
use room_support::{World, completed, refused_before_any_room, rehearsed};

/// The feed every candidate here reads.
const FEED: &str = "https://feed.example/items";

fn captures(body: &str) -> Captures {
    let mut captures = Captures::new();
    let text = Some("text/plain".to_owned());
    let page = Capture::new(FEED, 200, text, body.as_bytes().to_vec(), 0);
    captures.insert(page).unwrap();
    captures
}

/// A GET of the feed, its text written at `out/feed.txt`.
fn keeping(world: &World) -> String {
    let to = world.path("out/feed.txt");
    format!(
        "nika: keep\npermits:\n  tools: [\"nika:fetch\", \"nika:write\"]\n  net: {{ http: [\"feed.example\"] }}\n  fs:\n    write: [\"{to}\"]\ntasks:\n  feed:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{FEED}\", mode: text }} }}\n  keep:\n    with: {{ text: \"${{{{ tasks.feed.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{to}\", content: \"${{{{ with.text }}}}\", create_dirs: true, overwrite: true }} }}\n"
    )
}

/// The feed summarized by a model step on a local engine no room reaches, then written.
fn summarizing(world: &World) -> String {
    let to = world.path("out/digest.md");
    format!(
        "nika: digest\nmodel: vllm/room-trial\npermits:\n  tools: [\"nika:fetch\", \"nika:write\"]\n  net: {{ http: [\"feed.example\"] }}\n  fs:\n    write: [\"{to}\"]\ntasks:\n  feed:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{FEED}\", mode: text }} }}\n  summarize:\n    with: {{ text: \"${{{{ tasks.feed.output }}}}\" }}\n    infer: {{ prompt: \"Résume : ${{{{ with.text }}}}\", max_tokens: 64 }}\n  write_digest:\n    with: {{ digest: \"${{{{ tasks.summarize.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{to}\", content: \"${{{{ with.digest }}}}\", create_dirs: true, overwrite: true }} }}\n"
    )
}

#[tokio::test]
async fn a_captured_feed_is_replayed_and_its_write_read_back() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_captured_feed_is_replayed_and_its_write_read_back"
    );
    let world = World::new(&[]);
    let before = world.files();
    let room = world.room().with_captures(captures("hello feed"));
    let target = world.path("out/feed.txt");
    let report = rehearsed(TEST, "replay", &room, &keeping(&world), &[], &[target]).await;
    assert!(completed(&report), "{report:?}");
    let Rehearsal::Passed { outputs } = &report.outcome else {
        panic!("the replay passed: {report:?}");
    };
    assert_eq!(outputs.len(), 1, "{outputs:?}");
    assert!(outputs[0].written && outputs[0].text.contains("hello feed"));
    assert!(report.effects.is_none(), "{:?}", report.effects);
    assert!(report.room.prepared && report.room.cleaned);
    assert_eq!(world.files(), before, "the project is untouched");
}

#[tokio::test]
async fn a_model_step_and_the_write_behind_it_never_fail_the_trial() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_model_step_and_the_write_behind_it_never_fail_the_trial"
    );
    let world = World::new(&[]);
    let room = world.room().with_captures(captures("hello feed"));
    let target = world.path("out/digest.md");
    let candidate = summarizing(&world);
    let report = rehearsed(TEST, "replay", &room, &candidate, &[], &[target]).await;
    assert!(completed(&report), "{report:?}");
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { .. }),
        "the trial passes on what it exercised: {report:?}"
    );
    assert_eq!(report.effects.network, 0, "{:?}", report.effects);
}

#[tokio::test]
async fn lent_nothing_the_same_candidates_are_refused_before_any_room() {
    const TEST: &str = concat!(
        module_path!(),
        "::lent_nothing_the_same_candidates_are_refused_before_any_room"
    );
    let world = World::new(&[]);
    for (subrun, candidate) in [("keep", keeping(&world)), ("digest", summarizing(&world))] {
        let report = rehearsed(TEST, subrun, &world.room(), &candidate, &[], &[]).await;
        assert!(refused_before_any_room(&report, "network"), "{report:?}");
    }
}
