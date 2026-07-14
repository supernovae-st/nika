// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `policy:` lane (spec `10-authority.md` · `NIKA-POLICY-001`) —
//! needle-thin BY DESIGN: this module only PROJECTS the workflow (id ·
//! verb · tool · provider pin · direct parents from the ONE edge
//! derivation); the pure judge lives in `nika-cap`
//! ([`nika_cap::policy_violations`] · rule-for-rule mirror of the spec
//! reference evaluator `deep_static.py::policy_errors`). Soft families
//! are recorded by a hint (`check/hints.rs`), never judged.

use nika_cap::{PolicySubject, PolicyViolation, ProviderPin};

use crate::analyzer::Edge;
use crate::expression::scan_templates;
use crate::raw::{RawAction, RawWorkflow};

/// Judge the hard `policy:` families (require · forbid · allow · limits)
/// over the derived edges. The caller gates on a valid DAG — the order
/// rules read ancestors, so an unanalyzable workflow yields NO claim
/// (skipped, never wrong · the IFC/gate-lane precedent).
pub(super) fn scan_policy(wf: &RawWorkflow, edges: &[Edge]) -> Vec<PolicyViolation> {
    let Some(policy) = wf.policy.as_ref() else {
        return Vec::new();
    };
    let root_model = wf.model.as_ref().map(|m| m.value.as_str());
    let mut subjects: Vec<PolicySubject> = wf
        .tasks
        .iter()
        .map(|t| {
            let a = &t.value.action;
            let tool = match a {
                RawAction::Invoke(inv) => Some(inv.tool.value.clone()),
                _ => None,
            };
            let pin = provider_pin(a, root_model);
            PolicySubject::new(t.value.id.value.clone(), a.verb(), tool, pin)
        })
        .collect();
    for e in edges {
        if let Some(s) = subjects.get_mut(e.to) {
            s.parents.push(e.from);
        }
    }
    nika_cap::policy_violations(&policy.value, &subjects)
}

/// The static provider resolution (spec 10 §allow.providers) — the
/// task's `model:`, else the workflow default; templated or absent =
/// [`ProviderPin::Undeterminable`] (fail-closed under an allowlist).
fn provider_pin(action: &RawAction, root_model: Option<&str>) -> ProviderPin {
    let task_model = match action {
        RawAction::Infer(a) => a.model.as_ref().map(|m| m.value.as_str()),
        RawAction::Agent(a) => a.model.as_ref().map(|m| m.value.as_str()),
        RawAction::Exec(_) | RawAction::Invoke(_) => return ProviderPin::NotApplicable,
    };
    match task_model.or(root_model) {
        None => ProviderPin::Undeterminable,
        // a `${{ }}` island is not judgeable — the scanner is the one truth
        Some(m) if scan_templates(m).map(|i| !i.is_empty()).unwrap_or(true) => {
            ProviderPin::Undeterminable
        }
        Some(m) => ProviderPin::Literal(m.to_owned()),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use crate::check::check;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn report(yaml: &str) -> crate::check::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// Mirror of core/policy/001+002: the gate rule reads REAL ancestry
    /// through the derived `with:` edge.
    #[test]
    fn human_gate_missing_then_satisfied_through_the_edge() {
        let ungated = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  require:\n    human_gate_before: [exec]\npermits:\n  exec: [\"echo\"]\ntasks:\n  act:\n    exec: { command: [\"echo\", \"unattended\"] }\n",
        );
        assert!(!ungated.is_clean());
        assert_eq!(ungated.policy_findings.len(), 1);
        let f = &ungated.policy_findings[0];
        assert_eq!(f.rule, "require.human_gate_before");
        assert_eq!(f.task.as_deref(), Some("act"));
        assert!(f.detail.contains("no nika:prompt ancestor"), "{}", f.detail);

        let gated = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  require:\n    human_gate_before: [exec]\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  human:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"Proceed?\", default: false }\n  act:\n    with: { go: \"${{ tasks.human.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"shipped\"] }\n",
        );
        assert!(gated.is_clean(), "{:?}", gated.policy_findings);
    }

    /// Mirror of core/policy/003+004: order law over `E_d` — the witness
    /// carries the exact path, and an independent exec stays clean.
    #[test]
    fn exec_after_net_violation_carries_the_exact_path() {
        let base = "nika: v1\nworkflow:\n  id: t\npolicy:\n  forbid:\n    exec_after: [net]\npermits:\n  exec: [\"echo\"]\n  net: { http: [\"example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://example.com/data\" }\n";
        let bad = report(&format!(
            "{base}  act:\n    with: {{ body: \"${{{{ tasks.fetch_page.output }}}}\" }}\n    exec: {{ command: [\"echo\", \"${{{{ with.body }}}}\"] }}\n"
        ));
        assert_eq!(bad.policy_findings.len(), 1);
        assert!(
            bad.policy_findings[0]
                .detail
                .contains("the path is the witness: fetch_page → act"),
            "{}",
            bad.policy_findings[0].detail
        );
        let clean = report(&format!(
            "{base}  act:\n    exec: {{ command: [\"echo\", \"independent\"] }}\n"
        ));
        assert!(clean.is_clean(), "{:?}", clean.policy_findings);
    }

    /// The order law also reads `after:` control edges (spec 10 · « any
    /// path counts, after: edges included »).
    #[test]
    fn exec_after_reads_control_edges_too() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  forbid:\n    exec_after: [net]\npermits:\n  exec: [\"echo\"]\n  net: { http: [\"example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://example.com/data\" }\n  act:\n    after: { fetch_page: succeeded }\n    exec: { command: [\"echo\", \"x\"] }\n",
        );
        assert_eq!(r.policy_findings.len(), 1, "{:?}", r.policy_findings);
    }

    /// Mirror of core/policy/005+006: provider allowlist against the
    /// task-level `model:`.
    #[test]
    fn providers_allowlist_violation_and_clean() {
        let bad = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  allow:\n    providers: [ollama, mistral]\ntasks:\n  s:\n    infer: { prompt: \"summarize\", model: \"openai/gpt-4o\" }\n",
        );
        assert_eq!(bad.policy_findings.len(), 1);
        assert!(
            bad.policy_findings[0]
                .detail
                .contains("'openai' is not in [ollama · mistral]"),
            "{}",
            bad.policy_findings[0].detail
        );
        let clean = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  allow:\n    providers: [ollama, mistral]\ntasks:\n  s:\n    infer: { prompt: \"summarize\", model: \"ollama/llama3.2\" }\n",
        );
        assert!(clean.is_clean(), "{:?}", clean.policy_findings);
    }

    /// The workflow-level `model:` default is the fallback pin (spec 10 ·
    /// « or the run's default model when the task names none »).
    #[test]
    fn root_model_is_the_provider_fallback() {
        let clean = report(
            "nika: v1\nworkflow:\n  id: t\nmodel: ollama/llama3.2\npolicy:\n  allow:\n    providers: [ollama]\ntasks:\n  s:\n    infer: { prompt: \"x\" }\n",
        );
        assert!(clean.is_clean(), "{:?}", clean.policy_findings);
        let bad = report(
            "nika: v1\nworkflow:\n  id: t\nmodel: openai/gpt-4o\npolicy:\n  allow:\n    providers: [ollama]\ntasks:\n  s:\n    infer: { prompt: \"x\" }\n",
        );
        assert_eq!(bad.policy_findings.len(), 1);
    }

    /// Mirror of core/policy/010: a templated `model:` cannot be judged —
    /// fail-closed, and the teaching says « pin the literal ».
    #[test]
    fn templated_model_fails_closed() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\nvars:\n  m: { default: \"ollama/llama3.2\" }\npolicy:\n  allow:\n    providers: [ollama]\ntasks:\n  s:\n    infer: { prompt: \"summarize\", model: \"${{ vars.m }}\" }\n",
        );
        assert_eq!(r.policy_findings.len(), 1);
        assert!(
            r.policy_findings[0]
                .detail
                .contains("fail-closed: pin the literal"),
            "{}",
            r.policy_findings[0].detail
        );
    }

    /// An ABSENT model with a declared allowlist fails closed too (the
    /// other half of « templated or absent »).
    #[test]
    fn absent_model_everywhere_fails_closed() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  allow:\n    providers: [ollama]\ntasks:\n  s:\n    infer: { prompt: \"x\" }\n",
        );
        assert_eq!(r.policy_findings.len(), 1);
        assert!(r.policy_findings[0].detail.contains("templated or absent"));
    }

    /// Mirror of core/policy/007: the workflow-shape bound.
    #[test]
    fn max_tasks_exceeded() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  limits:\n    max_tasks: 2\ntasks:\n  a:\n    infer: { prompt: \"one\" }\n  b:\n    infer: { prompt: \"two\" }\n  c:\n    infer: { prompt: \"three\" }\n",
        );
        assert_eq!(r.policy_findings.len(), 1);
        let f = &r.policy_findings[0];
        assert_eq!(f.rule, "limits.max_tasks");
        assert_eq!(f.task, None, "workflow-level rule names no task");
        assert!(
            f.detail
                .contains("limits.max_tasks: 2 — the workflow declares 3 tasks"),
            "{}",
            f.detail
        );
    }

    /// Mirror of core/policy/008: soft families are INERT — recorded by a
    /// hint, never judged (a non-preferred provider stays clean).
    #[test]
    fn soft_families_record_a_hint_and_judge_nothing() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  prefer:\n    providers: [ollama]\n  optimize: cost\ntasks:\n  s:\n    infer: { prompt: \"summarize\", model: \"openai/gpt-4o\" }\n",
        );
        assert!(r.is_clean(), "{:?}", r.policy_findings);
        assert!(
            r.hints.iter().any(|h| h.kind == "policy-soft"
                && h.advice.contains("soft policy recorded · not judged (v1)")),
            "the recorded-not-judged hint names the contract: {:?}",
            r.hints
        );
        // no soft families → no hint
        let quiet = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  limits: { max_tasks: 5 }\ntasks:\n  s:\n    infer: { prompt: \"x\" }\n",
        );
        assert!(!quiet.hints.iter().any(|h| h.kind == "policy-soft"));
    }

    /// The lane is gated on a valid DAG: a conformance-broken workflow
    /// yields NO policy claim (skipped, never wrong — the ancestors the
    /// order rules read do not exist).
    #[test]
    fn broken_dag_skips_the_policy_lane() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  require:\n    human_gate_before: [exec]\ntasks:\n  act:\n    after: { ghost: succeeded }\n    exec: { command: [\"echo\", \"x\"] }\n",
        );
        assert!(!r.conformance.is_empty());
        assert!(
            r.policy_findings.is_empty(),
            "no claim on an unanalyzable graph: {:?}",
            r.policy_findings
        );
    }

    /// The unified findings list carries the policy class with its
    /// canonical code + docs url + gate label (the one-loop surface).
    #[test]
    fn policy_finding_folds_into_findings_with_its_code() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  limits:\n    max_tasks: 1\ntasks:\n  a:\n    infer: { prompt: \"x\" }\n  b:\n    infer: { prompt: \"y\" }\n",
        );
        assert!(!r.is_clean());
        let f = r
            .findings
            .iter()
            .find(|f| f.kind == "policy")
            .expect("policy row in findings[]");
        assert_eq!(f.gate, "POLICY");
        assert_eq!(f.code.as_deref(), Some("NIKA-POLICY-001"));
        assert_eq!(
            f.docs_url.as_deref(),
            Some("https://nika.sh/errors/NIKA-POLICY-001")
        );
        // and the conformance-code surface speaks the same code
        let codes: Vec<String> = r
            .extra_conformance_codes()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(codes.contains(&"NIKA-POLICY-001".to_owned()), "{codes:?}");
    }
}
