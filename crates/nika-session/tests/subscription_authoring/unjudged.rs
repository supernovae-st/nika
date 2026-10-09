// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Native subscription continuation, using the parent test's credential-free child launcher.
use super::*;
const INTENT: &str =
    "Je veux que sortie.txt contienne exactement les octets présents dans entree.txt.";
/// The author calls of one semantic CREATE of [`INTENT`].
fn authored() -> usize {
    common::semantic_copy().len()
}
pub(super) fn install(dir: &Path) {
    std::fs::write(dir.join("root/entree.txt"), "A\n").unwrap();
    for (n, answer) in common::semantic_copy().into_iter().enumerate() {
        std::fs::write(dir.join(format!("reply-{n}")), events(&answer, false)).unwrap();
    }
    std::fs::write(dir.join("approved"), events(JUDGE_APPROVES, false)).unwrap();
    let observed = shell(&dir.join("observed"));
    let script = format!(
        r#"#!/bin/sh
set -eu
if [ "${{1:-}}" = --version ]; then printf '%s\n' '2.1.280 (Claude Code)'; exit 0; fi
if [ "${{1:-}}" != -p ]; then exit 64; fi
n=0
if [ -f {observed}/count ]; then n=$(/bin/cat {observed}/count); fi
printf '%s' "$((n+1))" > {observed}/count
printf '%s\n' "$@" > {observed}/argv-$n
/bin/cat > {observed}/prompt-$n
if [ "$n" -lt {authored} ]; then /bin/cat {dir}/reply-$n
elif [ "$n" = {authored} ]; then printf '%s\n' 'synthetic judge unavailable' >&2; exit 1
else /bin/cat {dir}/approved
fi
"#,
        dir = shell(dir),
        authored = authored(),
    );
    let bin = dir.join("bin/claude");
    std::fs::write(&bin, script).unwrap();
    std::fs::set_permissions(bin, std::fs::Permissions::from_mode(0o755)).unwrap();
}
fn session(root: &Path, home: &Path) -> SessionRuntime {
    let mut resolved = ResolvedSessionIntelligence::resolve(
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        &IntelligenceCensus::empty(),
    );
    // The fixture supplies availability without probing an installed account.
    resolved.kind = IntelligenceKind::Harness {
        seat: "claude-code".into(),
        transport: nika_types::access::HarnessTransport::Native,
    };
    resolved.model = Some("anthropic/mechanical-requested".into());
    resolved.locus = DataLocus::Remote {
        product: "claude-code".into(),
    };
    resolved.ready = true;
    resolved.why = None;
    let mut s = SessionRuntime::open(
        root,
        resolved,
        Box::new(HarnessReasoner {
            seat: "claude-code".into(),
        }),
    );
    s.set_authoring_context(authoring_context("unjudged"));
    s.enable_continuous_preparation();
    s.with_classifier(Box::new(RouteOnly));
    s.enable_history(home).unwrap();
    s
}
pub(super) fn child(root: &Path, scenario: &str) {
    let home = std::env::var("HOME").unwrap();
    let mut s = session(root, Path::new(&home));
    let first = s.turn(INTENT);
    assert!(
        matches!(first, TurnOutcome::Facts(_)),
        "a failed native judge must leave a factual continuation"
    );
    assert!(s.pending_proposal().is_none() && s.pending_question().is_none());
    assert!(s.status_line().contains("Not ready"));
    if scenario == "unjudged-reopen" {
        drop(s);
        s = session(root, Path::new(&home));
        assert!(matches!(s.restore_round(), TurnOutcome::Facts(_)));
        assert!(s.pending_proposal().is_none());
    }
    let next = s.turn("continue");
    assert!(
        matches!(next, TurnOutcome::Proposal { .. }),
        "a fresh native judgment must make the kept candidate reviewable"
    );
    assert!(!root.join("compiled-workflow.nika").exists());
    assert!(!root.join("sortie.txt").exists());
    std::fs::write(
        std::env::var("SUBSCRIPTION_TEST_REPORT").unwrap(),
        json!({"ready":true}).to_string(),
    )
    .unwrap();
}
fn proof(scenario: &str) {
    let out = run(scenario);
    assert_eq!(
        out["calls"],
        authored() + 2,
        "the author calls, failed judge, retry judge"
    );
    for n in [authored(), authored() + 1] {
        assert!(out["prompts"][n].as_str().unwrap().contains("unfaithful"));
    }
    assert_eq!(out["ready"], true);
}
#[test]
fn native_unjudged_candidate_retries_the_judge_without_regeneration() {
    proof("unjudged");
}
#[test]
fn native_unjudged_candidate_survives_close_restore_and_retries_only_its_judge() {
    proof("unjudged-reopen");
}
