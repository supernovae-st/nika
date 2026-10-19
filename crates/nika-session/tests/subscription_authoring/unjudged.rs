// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Native subscription continuation, using the parent test's credential-free child launcher.
use super::*;
const INTENT: &str =
    "Je veux que sortie.txt contienne exactement les octets présents dans entree.txt.";
/// The verifier's answer for the request's part judged again over the whole trial run of the
/// copy (`entree.txt` an observed input, `sortie.txt` its target): its output shows it done.
const PART_CARRIED: &str = r#"{"choice":"carried"}"#;
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
    std::fs::write(dir.join("carried"), events(PART_CARRIED, false)).unwrap();
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
elif [ "$n" = {retried} ]; then /bin/cat {dir}/approved
elif [ "$n" = {part} ]; then /bin/cat {dir}/carried
else printf '%s\n' 'synthetic unexpected call' >&2; exit 1
fi
"#,
        dir = shell(dir),
        authored = authored(),
        retried = authored() + 1,
        part = authored() + 2,
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
    // The request is French: so is the status row.
    assert!(s.status_line().contains("Pas prêt"), "{}", s.status_line());
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
/// The state and the option keys of the closed choice the double received as call `n`.
fn shown(out: &Value, n: usize) -> (Value, Vec<String>) {
    let prompt = out["prompts"][n].as_str().unwrap();
    let (state, options) = (prompt.split_once("STATE:\n"))
        .and_then(|(_, rest)| rest.split_once("\n\nOPTIONS:\n"))
        .expect("a closed choice's state and options");
    let keys = (options.lines())
        .filter_map(|line| Some(line.strip_prefix("- ")?.split_once(": ")?.0.to_owned()))
        .collect();
    (serde_json::from_str(state).unwrap(), keys)
}
fn proof(scenario: &str) {
    let out = run(scenario);
    assert_eq!(
        out["calls"],
        authored() + 3,
        "the author calls, failed judge, retry judge, its part over the trial run"
    );
    for n in [authored(), authored() + 1] {
        assert!(out["prompts"][n].as_str().unwrap().contains("unfaithful"));
    }
    // The last call asks the request's part again over the whole trial run of the bytes the
    // retried judgment read: a part's options, the run that copied the input whole.
    let (judged, _) = shown(&out, authored() + 1);
    let (part, keys) = shown(&out, authored() + 2);
    assert_eq!(keys, ["carried", "missing", "unexercised", "none"]);
    assert_eq!(part["request"], INTENT);
    assert_eq!(part["candidate_nika"], judged["candidate_nika"]);
    let clause = part["clause"]["text"].as_str().unwrap();
    assert!(
        INTENT.contains(clause) && clause.contains("sortie.txt"),
        "{clause}"
    );
    let candidate = part["candidate_nika"].as_str().unwrap();
    let observed = &part["observation"];
    assert_eq!(observed["candidate_sha256"], common::sha256(candidate));
    let output = &observed["outputs"][0];
    let path = output["path"].as_str().unwrap_or_default();
    assert!(path.ends_with("sortie.txt"), "{observed:#}");
    assert_eq!(
        output["text"], "A\n",
        "the input copied whole: {observed:#}"
    );
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
