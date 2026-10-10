// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A trial captures each public GET source once for its conversation and says, in words a
//! person reads, what it ran on and what it did not run.

use std::sync::{Arc, Mutex};

use nika_service_execution::replay::Capture;

use super::{Observed, Observer, prepare};

const FEED: &str = "https://feed.example/items";

/// A digest: the feed, a model step, and a post to a hook.
fn digest() -> String {
    format!(
        "nika: digest\nmodel: vllm/trial\npermits:\n  tools: [\"nika:fetch\"]\n  net: {{ http: [\"feed.example\", \"hooks.example\"] }}\ntasks:\n  feed:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{FEED}\", mode: text }} }}\n  summarize:\n    with: {{ text: \"${{{{ tasks.feed.output }}}}\" }}\n    infer: {{ prompt: \"${{{{ with.text }}}}\" }}\n  post:\n    with: {{ text: \"${{{{ tasks.summarize.output }}}}\" }}\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://hooks.example/x\", method: POST, body: \"${{{{ with.text }}}}\" }} }}\n"
    )
}

/// An observer serving `FEED` (at 10:42 UTC on day 1), counting what it was asked.
fn serving(asked: &Arc<Mutex<Vec<String>>>) -> Observer {
    let asked = Arc::clone(asked);
    Arc::new(move |url: &str| {
        asked.lock().unwrap().push(url.to_owned());
        if url == FEED {
            let at = (24 * 3600 + 10 * 3600 + 42 * 60) * 1000;
            Ok(Capture::new(url, 200, None, b"hello".to_vec(), at))
        } else {
            Err("not served".to_owned())
        }
    })
}

#[test]
fn a_trial_says_what_it_ran_on_and_what_it_did_not_run() {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let (root, mut held) = (std::env::temp_dir(), Observed::new());
    let trial = prepare(
        &digest(),
        "fais un résumé",
        (root, None),
        &mut held,
        &serving(&asked),
    );
    let lines: Vec<&str> = trial.words.lines().collect();
    assert_eq!(
        lines[0],
        format!("tried on the pages observed at 10:42 UTC: {FEED} (5 bytes)")
    );
    assert_eq!(
        lines[1],
        "not run: the model step (summarize) · a request other than GET (post) · any effect outside the room"
    );
    assert_eq!(lines.len(), 2, "{lines:?}");
}

#[test]
fn a_source_is_observed_once_per_conversation() {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let observe = serving(&asked);
    let mut held = Observed::new();
    for _ in 0..2 {
        let _ = prepare(
            &digest(),
            "x",
            (std::env::temp_dir(), None),
            &mut held,
            &observe,
        );
    }
    assert_eq!(*asked.lock().unwrap(), [FEED]);
    assert!(held.contains_key(FEED));
}

#[test]
fn a_source_that_cannot_be_observed_is_said_and_never_replayed() {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let other = digest().replace(FEED, "https://feed.example/elsewhere");
    let mut held = Observed::new();
    let trial = prepare(
        &other,
        "x",
        (std::env::temp_dir(), None),
        &mut held,
        &serving(&asked),
    );
    assert!(
        trial
            .words
            .starts_with("tried on no page observed from the network\n")
    );
    assert!(
        trial.words.contains("the network (feed)"),
        "{}",
        trial.words
    );
    assert!(
        trial
            .words
            .contains("not observed: https://feed.example/elsewhere (not served)")
    );
    assert!(held.is_empty());
}

#[test]
fn a_conversation_holds_its_pages_within_a_bound() {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut held = Observed::new();
    for k in 0..64 {
        let at = format!("https://feed.example/{k}");
        held.insert(at.clone(), Capture::new(at, 200, None, Vec::new(), 0));
    }
    let trial = prepare(
        &digest(),
        "x",
        (std::env::temp_dir(), None),
        &mut held,
        &serving(&asked),
    );
    assert!(asked.lock().unwrap().is_empty(), "nothing more is fetched");
    assert!(
        trial.words.contains("holds 64 pages already"),
        "{}",
        trial.words
    );
    assert_eq!(held.len(), 64);
}
