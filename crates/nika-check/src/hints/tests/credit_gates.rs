// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

// The issue's admitted gate consumes both ingress sources and dominates the
// external egress. All checks are static: no file, network or provider call.
const GATED: &str = r#"
nika: gated-blocking
model: mock/echo
permits:
  fs: { read: ["./notes/private.md"] }
  net: { http: ["api.corp-hooks-example.com", "www.corp-news-example.com"] }
  tools: ["nika:read", "nika:fetch", "nika:prompt"]
tasks:
  brief:
    invoke: { tool: "nika:read", args: { path: "./notes/private.md" } }
  page:
    invoke: { tool: "nika:fetch", args: { url: "https://www.corp-news-example.com/today", mode: text } }
  human:
    with: { brief: "${{ tasks.brief.output }}", page: "${{ tasks.page.output }}" }
    invoke:
      tool: "nika:prompt"
      args: { message: "Send this brief and this page? ${{ with.brief }} / ${{ with.page }}" }
  send:
    with: { ok: "${{ tasks.human.output }}" }
    when: "${{ with.ok == true }}"
    invoke:
      tool: "nika:fetch"
      args: { url: "https://api.corp-hooks-example.com/hook", method: POST, body: { decision: "${{ with.ok }}" } }
outputs:
  sent: ${{ tasks.send.output }}
"#;

#[test]
fn credited_blocking_gate_can_reach_zero_hints() {
    let report = check_of(GATED);
    assert!(report.is_clean(), "{:?}", report.findings);
    assert!(
        report
            .trifecta_mitigations
            .iter()
            .any(|m| m.gate == "human" && m.sink == "send")
    );
    assert!(
        report.hints.is_empty(),
        "credited gate adds no advisory: {:?}",
        report.hints
    );
    let json = serde_json::to_value(&report).expect("existing report wire");
    assert_eq!(json["hints"], serde_json::json!([]));
    assert!(
        !json["trifecta_mitigations"]
            .as_array()
            .expect("mitigations")
            .is_empty()
    );
}

#[test]
fn uncredited_prompt_keeps_its_headless_hint() {
    let plain = check_of(
        "nika: t\npermits: { tools: [nika:prompt] }\ntasks:\n  human:\n    invoke: { tool: nika:prompt, args: { mode: confirm, message: 'ship?' } }\n",
    );
    assert!(plain.trifecta_mitigations.is_empty());
    let hint = plain
        .hints
        .iter()
        .find(|h| h.kind == "headless-prompt" && h.task == "human")
        .expect("uncredited prompt remains advisory");
    assert!(hint.advice.contains("`default:`"));
    assert!(hint.advice.contains("--answer human=<value>"));
}

#[test]
fn a_false_default_still_refuses_with_sec009() {
    let report = check_of(&GATED.replace("args: { message:", "args: { default: false, message:"));
    assert!(!report.is_clean());
    assert!(report.trifecta_mitigations.is_empty());
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code.as_deref() == Some("NIKA-SEC-009")),
        "{:?}",
        report.findings
    );
}

#[test]
fn credit_suppresses_only_the_matching_headless_hint_and_preserves_order() {
    let proof = check_of(GATED).trifecta_mitigations;
    assert!(!proof.is_empty(), "use the judge's actual evidence");
    let make = |kind, task: &str| Hint {
        kind,
        code: None,
        task: task.to_owned(),
        advice: "unchanged".to_owned(),
    };
    let unrelated = make("consent", "human");
    let uncredited = make("headless-prompt", "human_extra");
    let mut hints = vec![
        unrelated.clone(),
        make("headless-prompt", "human"),
        uncredited.clone(),
    ];
    let original = hints.clone();
    credit_gates(&mut hints, &[]);
    assert_eq!(hints, original, "no evidence changes nothing");
    credit_gates(&mut hints, &proof);
    assert_eq!(hints, [unrelated, uncredited]);
}
