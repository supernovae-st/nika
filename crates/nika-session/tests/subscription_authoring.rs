// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(all(unix, feature = "access-harness"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::disallowed_types
)]

//! Hermetic public Session → real Compiler → native harness transport tests.
//! Only the executable's bytes are scripted; no compiler outcome is injected.
//! Child environments carry no credentials, and every executable is fixture-local.
mod common;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nika_session::intelligence::{
    DataLocus, IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence,
    UserIntelligencePreference,
};
use nika_session::reasoner::{HarnessReasoner, NoReasoner};
use nika_session::turn::{
    RoutingMethod, SessionPhase, TurnAct, TurnClassifier, TurnContext, TurnDecision,
};
use nika_session::{SessionRuntime, TurnOutcome};
use serde_json::{Value, json};

fn shell(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}
fn events(answer: &str, tool: bool) -> String {
    // Structured output is the entire native response; a suffix is preserved
    // as a string value so Compiler rejects it instead of extracting a prefix.
    let structured =
        serde_json::from_str::<Value>(answer).unwrap_or_else(|_| Value::String(answer.into()));
    json!({"type":"result", "is_error":false, "result":answer,
        "structured_output":structured,
        "permission_denials":if tool { vec![json!({"tool_name":"Bash"})] } else { vec![] },
        "usage":{"input_tokens":1,"output_tokens":1},
        "modelUsage":{"observed-fixture-model":{}}})
    .to_string()
        + "\n"
}
fn install_fixture(dir: &Path, scenario: &str) {
    let answer = common::native_answer(&common::candidate("mock/echo", false));
    let answer = if scenario == "suffix" {
        format!("{answer} trailing {{\"second\":true}}")
    } else {
        answer
    };
    let second = common::native_answer(&common::candidate("openai/gpt-4.1-mini", true));
    for (name, text) in [("one", answer), ("two", second)] {
        std::fs::write(dir.join(name), events(&text, scenario == "tool")).unwrap();
    }
    let observed = dir.join("observed");
    let bin = dir.join("bin/claude");
    let script = format!(
        r#"#!/bin/sh
set -eu
if [ "${{1:-}}" = --version ]; then printf '%s\n' '2.1.280 (Claude Code)'; exit 0; fi
if [ "${{1:-}}" = auth ] && [ "${{2:-}}" = status ]; then
    printf '%s\n' '{{"loggedIn":false}}'
    printf '%s\n' auth-status >> {observed}/probes
    exit 0
fi
if [ "${{1:-}}" != -p ]; then exit 64; fi
n=0
if [ -f {observed}/count ]; then n=$(/bin/cat {observed}/count); fi
printf '%s' "$((n+1))" > {observed}/count
printf '%s\n' "$@" > {observed}/argv-$n
/bin/cat > {observed}/prompt-$n
if [ "$n" = 0 ] || [ {bad} = yes ]; then /bin/cat {one}; else /bin/cat {two}; fi
"#,
        observed = shell(&observed),
        one = shell(&dir.join("one")),
        two = shell(&dir.join("two")),
        bad = if matches!(scenario, "tool" | "suffix") {
            "yes"
        } else {
            "no"
        }
    );
    std::fs::write(&bin, script).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
}
fn run(scenario: &str) -> Value {
    let dir = tempfile::tempdir().unwrap();
    for name in ["bin", "root", "home", "observed"] {
        std::fs::create_dir(dir.path().join(name)).unwrap();
    }
    let root = dir.path().join("root");
    std::fs::write(root.join("a.md"), "Fixture text\n").unwrap();
    let foundry = common::Foundry::create(&dir.path().join("knowledge"));
    install_fixture(dir.path(), scenario);
    let observed = dir.path().join("observed");
    let report = dir.path().join("report.json");
    let child_log = dir.path().join("child.log");
    let log = std::fs::File::create(&child_log).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_clear()
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", dir.path().join("bin").display()),
        )
        .env("HOME", dir.path().join("home"))
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_AUTHORING_STRATEGY", "only")
        .env(
            "NIKA_KNOWLEDGE",
            if scenario == "no-knowledge" {
                Path::new("")
            } else {
                &foundry.snapshot
            },
        )
        .env("SUBSCRIPTION_TEST_SCENARIO", scenario)
        .env("SUBSCRIPTION_TEST_ROOT", &root)
        .env("SUBSCRIPTION_TEST_REPORT", &report)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("bounded synthetic child timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.success(),
        "child {scenario}: {}\nobserved prompts: {:?}",
        std::fs::read_to_string(child_log).unwrap(),
        std::fs::read_dir(&observed)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                let n = entry.file_name();
                let n = n.to_string_lossy();
                n.starts_with("prompt-") || n.starts_with("argv-")
            })
            .map(|entry| (
                entry.file_name(),
                std::fs::read_to_string(entry.path()).unwrap()
            ))
            .collect::<Vec<_>>()
    );
    let out = receipt(&report, &observed);
    assert!(!root.join("b.md").exists() && !root.join("c.md").exists());
    assert!(
        !root.join("clever-rewrite.nika").exists(),
        "no consent, Save or Run"
    );
    out
}

fn receipt(report: &Path, observed: &Path) -> Value {
    let mut out: Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    let count = std::fs::read_to_string(observed.join("count"))
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    out["calls"] = json!(count);
    out["availability_probes"] = json!(
        std::fs::read_to_string(observed.join("probes"))
            .unwrap_or_default()
            .lines()
            .count()
    );
    out["prompts"] = json!(
        (0..count)
            .map(|n| std::fs::read_to_string(observed.join(format!("prompt-{n}"))).unwrap())
            .collect::<Vec<_>>()
    );
    out["argv"] = json!(
        (0..count)
            .map(|n| std::fs::read_to_string(observed.join(format!("argv-{n}"))).unwrap())
            .collect::<Vec<_>>()
    );
    out
}

struct RouteOnly;
impl TurnClassifier for RouteOnly {
    fn classify(&mut self, context: &TurnContext, _: &str) -> TurnDecision {
        TurnDecision::new(
            match context.phase {
                SessionPhase::QuestionPending => TurnAct::Answer,
                SessionPhase::ProposalPending => TurnAct::Modify,
                _ => TurnAct::NewWork,
            },
            RoutingMethod::Model,
        )
    }
}
fn step(out: TurnOutcome) -> Value {
    match out {
        TurnOutcome::Proposal { id, preview } => {
            json!({"kind":"proposal","id":id.to_string(),"text":preview})
        }
        TurnOutcome::Question { key, question } => {
            json!({"kind":"question","key":key,"text":question})
        }
        TurnOutcome::Held { id, preview } => {
            json!({"kind":"held","id":id.to_string(),"text":preview})
        }
        TurnOutcome::Refusal(refusal) => json!({"kind":"refusal","text":refusal.text}),
        other => json!({"kind":"other","text":format!("{other:?}")}),
    }
}
#[test]
#[ignore = "invoked by the isolated fixture parents only"]
fn child() {
    let Ok(scenario) = std::env::var("SUBSCRIPTION_TEST_SCENARIO") else {
        return;
    };
    let root = std::env::var("SUBSCRIPTION_TEST_ROOT").unwrap();
    let none = scenario == "none";
    let mut resolved = ResolvedSessionIntelligence::resolve(
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        &IntelligenceCensus::empty(),
    );
    // Supply fixture availability through the public fields. This does not
    // claim to have probed an installed product or authenticated account.
    if !none {
        let seat = if scenario == "codex-unavailable" {
            "codex"
        } else {
            "claude-code"
        };
        resolved.kind = IntelligenceKind::Harness { seat: seat.into() };
        resolved.model = Some("anthropic/mechanical-requested".into());
        resolved.locus = DataLocus::Remote {
            product: seat.into(),
        };
    }
    let reasoner: Box<dyn nika_session::reasoner::SessionReasoner> =
        if none || scenario == "unsupported" {
            Box::new(NoReasoner)
        } else {
            Box::new(HarnessReasoner {
                seat: if scenario == "codex-unavailable" {
                    "codex"
                } else {
                    "claude-code"
                }
                .into(),
            })
        };
    let mut session = SessionRuntime::open(Path::new(&root), resolved, reasoner);
    // `open` takes explicit library inputs; only the host door reads its environment.
    session.set_authoring_context(nika_session::authoring::AuthoringContext::from_env());
    session.with_classifier(Box::new(RouteOnly));
    let intent = if scenario == "money" {
        "Read ./a.md and do something clever with it, then write ./b.md; budget 10 USD"
    } else if none {
        "Read ./a.md and write it to ./b.md"
    } else {
        common::INTENT
    };
    let first = step(session.turn(intent));
    let mut steps = vec![first];
    let mut old_rejected = false;
    let mut details_answer = String::new();
    let mut meaning_answer = Value::Null;
    if matches!(scenario.as_str(), "route" | "no-knowledge") {
        assert_eq!(steps[0]["kind"], "question", "{steps:?}");
        steps.push(step(session.turn("openai/gpt-4.1-mini")));
        details_answer = session.details();
        meaning_answer = step(session.consent("/meaning"));
        let old = session.pending_proposal().unwrap_or_else(|| {
            panic!("first proposal absent: {steps:#?}; meaning={meaning_answer}")
        });
        steps.push(step(session.consent(common::CHANGE)));
        let fresh = session.pending_proposal().expect("revised proposal");
        assert_ne!(old, fresh);
        old_rejected = matches!(session.consent_to(&old, "yes"), TurnOutcome::Refusal(_));
        assert_eq!(session.pending_proposal(), Some(fresh));
    }
    let meaning = step(if session.pending_proposal().is_some() {
        session.consent("/meaning")
    } else {
        session.turn("/meaning")
    });
    let out = json!({"steps":steps,"details":session.details(),"details_answer":details_answer,"meaning_answer":meaning_answer,"meaning":meaning,"old_consent_rejected":old_rejected,
        "money_unknown":session.monetary_decision().is_some_and(|m| m.observed_cost_usd.is_none()),
        "seat":format!("{:?}",session.authoring_seat())});
    std::fs::write(
        std::env::var("SUBSCRIPTION_TEST_REPORT").unwrap(),
        out.to_string(),
    )
    .unwrap();
}

#[test]
fn subscription_authors_then_answers_and_revises_through_the_same_native_compiler() {
    let out = run("route");
    assert_eq!(
        out["calls"], 2,
        "answer continuation makes no new call: {out:#}"
    );
    assert_eq!(out["steps"][1]["kind"], "proposal");
    assert_eq!(out["steps"][2]["kind"], "proposal");
    assert_eq!(out["old_consent_rejected"], true);
    assert_eq!(out["money_unknown"], true);
    let details = out["details"].as_str().unwrap();
    assert!(
        details.contains("authoring backend: subscription claude-code"),
        "{details}"
    );
    assert!(details.contains("responding model: observed-fixture-model"));
    assert!(details.contains("cost: subscription invoice unknown"));
    assert!(
        out["meaning"]["text"]
            .as_str()
            .unwrap()
            .contains("cost: subscription invoice unknown")
    );
    assert_eq!(
        out["meaning"]["kind"], "held",
        "Meaning preserves pending review"
    );
    assert!(
        details.contains("knowledge: knowledge-s03") && details.contains("presented to the seat")
    );
    assert!(
        out["argv"][0]
            .as_str()
            .unwrap()
            .contains("S03-PATTERN-MARKER")
    );
    for argv in out["argv"].as_array().unwrap() {
        assert!(
            argv.as_str()
                .unwrap()
                .contains("--model\nmechanical-requested\n")
        );
        assert!(argv.as_str().unwrap().contains("--tools\n\n"));
    }
}
#[test]
fn tools_and_suffix_answers_never_become_a_reviewable_candidate() {
    for scenario in ["tool", "suffix"] {
        let out = run(scenario);
        assert!(out["calls"].as_u64().unwrap() > 0);
        assert_ne!(out["steps"][0]["kind"], "proposal", "{out:#}");
        assert_ne!(
            out["steps"][0]["kind"], "question",
            "invalid model answer is not a user clarification"
        );
    }
}
#[test]
fn missing_capability_refuses_and_no_intelligence_stays_deterministic() {
    let out = run("unsupported");
    assert_eq!(out["calls"], 0, "{out:#}");
    assert_eq!(out["steps"][0]["kind"], "refusal");
    let out = run("none");
    assert_eq!(out["calls"], 0, "{out:#}");
    assert_eq!(out["steps"][0]["kind"], "proposal");
}

#[test]
fn replay_retains_subscription_receipt_without_optional_knowledge() {
    let out = run("no-knowledge");
    assert_eq!(out["calls"], 2, "replay adds no call: {out:#}");
    for text in [
        out["details_answer"].as_str().unwrap(),
        out["meaning_answer"]["text"].as_str().unwrap(),
    ] {
        assert!(text.contains("subscription claude-code"), "{text}");
        assert!(
            text.contains("this clarification replay made zero calls"),
            "{text}"
        );
        assert!(text.contains("subscription invoice unknown"), "{text}");
    }
    assert_eq!(out["old_consent_rejected"], true);
}

#[test]
fn a_monetary_ceiling_does_not_authorize_or_meter_a_subscription() {
    let out = run("money");
    assert_eq!(out["calls"], 0, "no provider-account bypass: {out:#}");
    assert_ne!(out["steps"][0]["kind"], "proposal");
    assert_eq!(out["money_unknown"], true);
}

#[test]
fn codex_refuses_before_any_call_until_native_tools_can_be_disabled() {
    let out = run("codex-unavailable");
    assert_eq!(out["calls"], 0, "no Codex or fallback call");
    assert_eq!(out["steps"][0]["kind"], "refusal");
    assert!(
        out["steps"][0]["text"]
            .as_str()
            .unwrap()
            .contains("pre-execution tool disabling")
    );
}
