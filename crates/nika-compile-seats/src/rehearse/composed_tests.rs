// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The source-only hold on a child workflow and the only way it lifts: a host's clean closure of
//! the exact candidate bytes. Outcomes come from the core's own `finish`, as every door's do.

use super::{
    Attempt, Closure, Composed, EffectCounts, Rehearsal, RehearsalFuture, RehearsalReport,
    Rehearse, changed_children, composed, discharge_children, held_children,
};
use nika_compile::surface::{UNJUDGED_DEPENDENCY, finding, finish, initial, sha256};
use nika_compile::{CompileOutcome, CompileStatus, DiagnosticKind};
use serde_json::json;
use std::time::Duration;

/// The pack's own parent: a native child call, Check clean alone.
fn parent() -> &'static str {
    nika_pack::example("10-compose-pipeline").expect("the pack ships 10")
}

/// The parent as `finish` leaves it: the child call held, nothing else open.
fn finished(source: &str) -> CompileOutcome {
    let mut out = initial();
    finish(source.to_owned(), &mut out);
    out
}

fn clean_of(source: &str) -> Closure {
    let units = vec![("p.nika".to_owned(), sha256(source))];
    Closure::new(sha256(source), "p.nika", ("identity", 1), units)
}

/// The report of a host that runs nothing.
fn not_run(candidate: &str) -> RehearsalFuture<'_> {
    Box::pin(async move {
        let outcome = Rehearsal::NotRun {
            reason: "no room in this test".to_owned(),
        };
        let none = EffectCounts::none();
        RehearsalReport::new(outcome, Attempt::NeverAttempted, none, sha256(candidate))
    })
}

/// A host whose only answer is its composition check.
struct Host(Composed);

impl Rehearse for Host {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        not_run(candidate)
    }
    fn bound(&self) -> Duration {
        Duration::from_secs(1)
    }
    fn compose(&self, _candidate: &str) -> Composed {
        self.0.clone()
    }
}

/// The default port offers nothing: a host that never learned composition keeps the hold.
struct Unaware;

impl Rehearse for Unaware {
    fn rehearse<'a>(&'a self, candidate: &'a str, _inputs: &'a [String]) -> RehearsalFuture<'a> {
        not_run(candidate)
    }
    fn bound(&self) -> Duration {
        Duration::from_secs(1)
    }
}

#[test]
fn the_child_call_is_held_by_finish_and_only_a_clean_closure_of_these_bytes_lifts_it() {
    let source = parent();
    let held = finished(source);
    assert_ne!(held.status, CompileStatus::Ready, "{:#?}", held.diagnostics);
    assert_eq!(held_children(&held), ["call"]);
    // Other bytes than these are no witness: nothing lifts.
    let mut other = held.clone();
    assert!(discharge_children(&mut other, &clean_of("nika: other\n")).is_empty());
    assert_eq!(other.diagnostics, held.diagnostics);
    assert_ne!(other.status, CompileStatus::Ready);
    // These bytes, checked clean: the hold lifts and READY follows the core's own law.
    let mut lifted = held.clone();
    assert_eq!(discharge_children(&mut lifted, &clean_of(source)), ["call"]);
    assert_eq!(
        lifted.status,
        CompileStatus::Ready,
        "{:#?}",
        lifted.diagnostics
    );
    assert!(held_children(&lifted).is_empty());
}

#[test]
fn every_other_hold_stays_and_a_refusal_is_never_erased() {
    let source = parent();
    // The same hold on a task that calls no workflow (an MCP tool, a skill) is not a child's.
    let mut mcp = finished(source);
    finding(
        &mut mcp,
        DiagnosticKind::Unknown,
        "search",
        UNJUDGED_DEPENDENCY,
    );
    assert_eq!(discharge_children(&mut mcp, &clean_of(source)), ["call"]);
    assert!(
        mcp.diagnostics.iter().any(|d| d.target == "search"),
        "{:#?}",
        mcp.diagnostics
    );
    assert_ne!(mcp.status, CompileStatus::Ready);
    // A mandatory question still asked keeps the candidate from READY.
    let mut asked = finished(source);
    nika_compile::surface::question(&mut asked, "const.x", "x", nika_compile::QuestionType::Text);
    discharge_children(&mut asked, &clean_of(source));
    assert_ne!(asked.status, CompileStatus::Ready);
    // A refused outcome is left exactly as it is.
    let mut refused = finished(source);
    refused.status = CompileStatus::Refused;
    let before = refused.diagnostics.clone();
    assert!(discharge_children(&mut refused, &clean_of(source)).is_empty());
    assert_eq!(
        (refused.status, refused.diagnostics),
        (CompileStatus::Refused, before)
    );
}

#[test]
fn the_record_says_what_the_host_answered_and_lifts_only_on_a_clean_closure() {
    let source = parent();
    let verdict = |out: &CompileOutcome| {
        out.provenance
            .decision
            .as_ref()
            .map(|d| d["composition"].clone())
    };
    let mut clean = finished(source);
    composed(&Host(Composed::Clean(clean_of(source))), &mut clean);
    let record = verdict(&clean).expect("recorded");
    assert_eq!(record["verdict"], "clean");
    assert_eq!(record["discharged"], json!(["call"]));
    assert_eq!(record["held"], json!(["call"]));
    assert_eq!(record["logical_root"], "p.nika");
    assert_eq!(record["candidate_sha256"], json!(sha256(source)));
    assert_eq!(record["snapshot_identity"], "identity");
    assert_eq!(clean.status, CompileStatus::Ready);
    for (host, word) in [
        (
            Composed::Refused {
                reason: "NIKA-COMP-001 missing".to_owned(),
            },
            "refused",
        ),
        (
            Composed::Unresolved {
                reason: "no location".to_owned(),
            },
            "unresolved",
        ),
        (Composed::Unoffered, "unoffered"),
    ] {
        let mut held = finished(source);
        composed(&Host(host), &mut held);
        let record = verdict(&held).expect("recorded");
        assert_eq!(record["verdict"], word, "{record:#}");
        assert_eq!(record["discharged"], json!([]));
        assert_ne!(held.status, CompileStatus::Ready);
        assert_eq!(held_children(&held), ["call"]);
    }
    let mut unaware = finished(source);
    composed(&Unaware, &mut unaware);
    assert_eq!(verdict(&unaware).expect("recorded")["verdict"], "unoffered");
    // A candidate that calls no child asks the host nothing and records nothing.
    let mut plain = finished("nika: plain\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n");
    composed(&Host(Composed::Clean(clean_of(source))), &mut plain);
    assert!(verdict(&plain).is_none_or(|record| record.is_null()));
}

#[test]
fn a_closure_differs_from_its_record_only_by_its_changed_children() {
    let unit = |path: &str, digest: &str| (path.to_owned(), digest.to_owned());
    let closure = |units: Vec<(String, String)>| Closure::new("c", "p.nika", ("id", 1), units);
    let checked = closure(vec![unit("p.nika", "root"), unit("child.nika", "one")]);
    let record = json!({"logical_root": "p.nika", "units": checked.units});
    assert!(changed_children(&record, &checked).is_empty());
    // The root's own bytes never count: only what it invokes.
    let rerooted = closure(vec![unit("p.nika", "other"), unit("child.nika", "one")]);
    assert!(changed_children(&record, &rerooted).is_empty());
    let rewritten = closure(vec![unit("p.nika", "root"), unit("child.nika", "two")]);
    assert_eq!(changed_children(&record, &rewritten), ["`child.nika`"]);
    let gone = closure(vec![unit("p.nika", "root")]);
    assert_eq!(changed_children(&record, &gone), ["`child.nika`"]);
    let added = closure(vec![
        unit("p.nika", "root"),
        unit("child.nika", "one"),
        unit("g.nika", "x"),
    ]);
    assert_eq!(changed_children(&record, &added), ["`g.nika`"]);
}
