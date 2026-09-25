// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Real child pipes exercise the production whole-document reader (no HTTP).
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::print_stdout
)]
use super::*;
use nika_providers::admission::{CostHostEvidence, CostReview, CostRoute, PendingCostReview};
use std::io::BufRead as _;
use std::process::{Command, Stdio};
fn pending() -> PendingCostReview {
    let route = CostRoute::observe(
        "deepseek/unpriced-fixture",
        nika_providers::ProvidersConfig::new(),
    )
    .unwrap();
    PendingCostReview::new(
        CostReview::new(
            "candidate".into(),
            "invocation".into(),
            route,
            CostHostEvidence::unmanaged_interactive_local(),
            None,
            None,
        )
        .unwrap(),
        "source".into(),
        "inputs".into(),
    )
}
#[test]
fn pipe_fixture_child() {
    if !std::path::Path::new(".s85-pipe-fixture").exists() {
        return;
    }
    let p = pending();
    let route = p.challenge().route.clone();
    let answer = ReviewChannel::Stdio.ask(p.challenge());
    let admitted = answer.and_then(|answer| p.confirm(&answer, "candidate", &route));
    println!(
        "{}",
        if admitted.is_ok() {
            "FIXTURE_CONFIRMED"
        } else {
            "FIXTURE_REFUSED"
        }
    );
}
#[test]
fn whole_response_eof_duplicate_malformed_cross_child_and_no_are_distinct() {
    for mode in [
        "yes",
        "no",
        "eof",
        "duplicate",
        "malformed",
        "foreign",
        "oversized",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".s85-pipe-fixture"), "test only").unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "run_cost::exchange::tests::pipe_fixture_child",
                "--nocapture",
                // Serial libtest otherwise prefixes the JSON frame with the test name.
                "--quiet",
            ])
            .current_dir(root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let challenge = loop {
            let mut line = String::new();
            assert!(out.read_line(&mut line).unwrap() > 0);
            if let Ok(challenge) = CostChallenge::parse(&line) {
                break challenge;
            }
        };
        let good = serde_json::to_vec(&challenge.response(true)).unwrap();
        let bytes = match mode {
            "yes" => good.clone(),
            "no" => serde_json::to_vec(&challenge.response(false)).unwrap(),
            "eof" => Vec::new(),
            "duplicate" => [good.clone(), good].concat(),
            "foreign" => serde_json::to_vec(&pending().challenge().response(true)).unwrap(),
            "oversized" => vec![b'x'; 16_385],
            _ => b"{\"yes\":true}".to_vec(),
        };
        let mut input = child.stdin.take().unwrap();
        input.write_all(&bytes).unwrap();
        if mode == "yes" {
            // A complete-looking JSON prefix is still not approval until EOF;
            // leave the pipe open and verify the live child has not completed.
            assert!(child.try_wait().unwrap().is_none());
        }
        drop(input);
        let mut rest = String::new();
        out.read_to_string(&mut rest).unwrap();
        assert!(child.wait().unwrap().success(), "{mode}: {rest}");
        assert!(
            rest.contains(if mode == "yes" {
                "FIXTURE_CONFIRMED"
            } else {
                "FIXTURE_REFUSED"
            }),
            "{mode}: {rest}"
        );
    }
}

/// A challenge whose every identity is a token no sentence of the copy contains.
fn distinct() -> CostChallenge {
    let route = CostRoute::observe(
        "deepseek/unpriced-fixture",
        nika_providers::ProvidersConfig::new(),
    )
    .unwrap();
    PendingCostReview::new(
        CostReview::new(
            "cand-s90".into(),
            "inv-s90".into(),
            route,
            CostHostEvidence::unmanaged_interactive_local(),
            None,
            None,
        )
        .unwrap(),
        "src-s90".into(),
        "in-s90".into(),
    )
    .challenge()
    .clone()
}

/// The lane's retained child asks `challenge` over the negotiated pipe and
/// copies its one-use reply pipe to `reply.json` until EOF.
fn retained(root: &std::path::Path, challenge: &CostChallenge) -> crate::lane::PendingRun {
    let (busy, _) = std::sync::mpsc::channel();
    let slot: crate::lane::ChildSlot = std::sync::Arc::default();
    let args = vec![
        "-c".to_owned(),
        "printf '%s\\n' \"$1\"; cat > reply.json".to_owned(),
        "fixture".to_owned(),
        serde_json::to_string(challenge).unwrap(),
    ];
    let shell = std::path::Path::new("/bin/sh");
    match crate::lane::drive_reviewed_child(shell, &args, root, &busy, &slot) {
        crate::lane::RunProgress::Review(pending) => *pending,
        crate::lane::RunProgress::Complete(result) => panic!("the child asked nothing: {result:?}"),
    }
}

#[test]
fn retained_review_details_write_nothing_and_the_one_answer_echoes_the_challenge() {
    let root = tempfile::tempdir().unwrap();
    let challenge = distinct();
    let pending = retained(root.path(), &challenge);
    assert_eq!(pending.question(), challenge.display());
    assert_eq!(pending.details(), challenge.details());
    assert_eq!(pending.details(), pending.details());
    let reply = root.path().join("reply.json");
    assert!(
        std::fs::read(&reply).unwrap_or_default().is_empty(),
        "reading the details answered the child"
    );
    let (busy, _) = std::sync::mpsc::channel();
    let (code, _, story) = pending.answer(true, &busy);
    assert_eq!(code, 0, "{story:?}");
    let sent: serde_json::Value = serde_json::from_slice(&std::fs::read(&reply).unwrap()).unwrap();
    assert_eq!(sent["yes"], true);
    assert_eq!(sent["challenge"], serde_json::to_value(&challenge).unwrap());
}

#[test]
fn a_dropped_retained_review_ends_the_child_without_a_reply() {
    let root = tempfile::tempdir().unwrap();
    let pending = retained(root.path(), &distinct());
    assert!(pending.details().contains("Source SHA-256: src-s90\n"));
    drop(pending);
    let reply = std::fs::read(root.path().join("reply.json")).unwrap_or_default();
    assert!(reply.is_empty(), "a cancelled review answered: {reply:?}");
}
