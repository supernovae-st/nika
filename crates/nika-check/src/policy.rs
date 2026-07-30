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
use nika_schema::expression::scan_templates;
use nika_schema::raw::{RawAction, RawWorkflow};

/// The wire code a policy finding stamps (spec 10 · F-P4 · F-P23) —
/// ONE match, every surface: the `--json` `findings[]` fold
/// (`check/findings.rs`), the console POLICY rung
/// (`nika-display/check_render.rs`), and the LSP projection all read
/// THIS. The `approval.*` rules (NEP-0013) speak the
/// approval-capability code NIKA-SEC-010, the `endorsement.*` rules
/// (NEP-0017) speak NIKA-SEC-013, every other rule the policy-lane
/// NIKA-POLICY-001.
#[must_use]
pub fn policy_wire_code(rule: &str) -> &'static str {
    if rule.starts_with("approval.") {
        "NIKA-SEC-010"
    } else if rule.starts_with("endorsement.") {
        "NIKA-SEC-013"
    } else {
        "NIKA-POLICY-001"
    }
}

/// Judge the hard `policy:` families (require · forbid · allow · limits
/// · endorsement) over the derived edges. The caller gates on a valid
/// DAG — the order rules read ancestors, so an unanalyzable workflow
/// yields NO claim (skipped, never wrong · the IFC/gate-lane precedent).
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
                RawAction::Invoke(inv) => match &inv.target {
                    nika_schema::raw::RawInvokeTarget::Tool(t) => Some(t.value.clone()),
                    // a child call is a distinct subject — the policy
                    // vocabulary sees the target as written (spec 14
                    // keeps tools/workflows separate · G25)
                    nika_schema::raw::RawInvokeTarget::Workflow(w) => Some(w.value.clone()),
                },
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
    let mut violations = nika_cap::policy_violations(&policy.value, &subjects);
    // F-P4 (NEP-0013 law 3) — the heterogeneous batch rides the same
    // projection (declared `require.human_gate_before` only · the judge
    // itself is inert without it).
    violations.extend(nika_cap::approval_batch_violations(
        &policy.value,
        &subjects,
    ));
    // F-P23 (NEP-0017) — the named solo mode rides the same projection:
    // a human gate under no declared endorsement mode refuses (F-F5
    // fail-closed), and a declared solo with more than one gate lies.
    violations.extend(nika_cap::endorsement_solo_violations(
        &policy.value,
        &subjects,
    ));
    violations
}

/// The static provider resolution (spec 10 §allow.providers) — the
/// task's `model:`, else the workflow default; templated or absent =
/// [`ProviderPin::Undeterminable`] (fail-closed under an allowlist).
fn provider_pin(action: &RawAction, root_model: Option<&str>) -> ProviderPin {
    let task_model = match action {
        RawAction::Infer(a) => a.model.as_ref().map(|m| m.value.as_str()),
        RawAction::Agent(a) => a.model.as_ref().map(|m| m.value.as_str()),
        RawAction::Exec(_) | RawAction::Invoke(_) => return ProviderPin::NotApplicable,
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
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
    use super::policy_wire_code;
    use crate::check;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// 2a · the ONE mapping every surface reads: `approval.*` speaks
    /// NIKA-SEC-010 (NEP-0013), `endorsement.*` NIKA-SEC-013 (NEP-0017),
    /// every other rule the policy-lane NIKA-POLICY-001 (spec 10).
    #[test]
    fn the_wire_code_mapping_is_prefix_exact() {
        assert_eq!(
            policy_wire_code("approval.heterogeneous_batch"),
            "NIKA-SEC-010"
        );
        assert_eq!(policy_wire_code("endorsement.solo_count"), "NIKA-SEC-013");
        assert_eq!(
            policy_wire_code("endorsement.undeclared_mode"),
            "NIKA-SEC-013"
        );
        assert_eq!(
            policy_wire_code("require.human_gate_before"),
            "NIKA-POLICY-001"
        );
        assert_eq!(policy_wire_code("limits.max_tasks"), "NIKA-POLICY-001");
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
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  require:\n    human_gate_before: [exec]\n  endorsement: solo\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  human:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"Proceed?\", default: false }\n  act:\n    with: { go: \"${{ tasks.human.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"shipped\"] }\n",
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
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  forbid:\n    exec_after: [net]\npermits:\n  exec: [\"echo\"]\n  net: { http: [\"example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://example.com/data\" }\n  act:\n    after: { fetch_page: success }\n    exec: { command: [\"echo\", \"x\"] }\n",
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
            "nika: v1\nworkflow:\n  id: t\nconst:\n  m: { default: \"ollama/llama3.2\" }\npolicy:\n  allow:\n    providers: [ollama]\ntasks:\n  s:\n    infer: { prompt: \"summarize\", model: \"${{ const.m }}\" }\n",
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
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  require:\n    human_gate_before: [exec]\ntasks:\n  act:\n    after: { ghost: success }\n    exec: { command: [\"echo\", \"x\"] }\n",
        );
        assert!(!r.conformance.is_empty());
        assert!(
            r.policy_findings.is_empty(),
            "no claim on an unanalyzable graph: {:?}",
            r.policy_findings
        );
    }

    // ── F-P4 (NEP-0013 law 3) · the heterogeneous batch ──────────────

    /// Fixture (d): ONE prompt whose yes unleashes TWO effect classes
    /// (an exec AND a fetch) is the fatigue machine — refused at check,
    /// speaking NIKA-SEC-010 (never the policy-lane code).
    #[test]
    fn approval_batch_heterogeneous_is_refused_with_the_approval_code() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  require:\n    human_gate_before: [exec, net]\n  endorsement: solo\npermits:\n  exec: [\"echo\"]\n  net: { http: [\"example.com\"] }\n  tools: [\"nika:prompt\", \"nika:fetch\"]\ntasks:\n  gate:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"Proceed?\", default: false }\n  act:\n    with: { go: \"${{ tasks.gate.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"shipped\"] }\n  page:\n    with: { go: \"${{ tasks.gate.output }}\" }\n    when: ${{ with.go == true }}\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://example.com/data\" }\n",
        );
        assert!(!r.is_clean());
        let batch: Vec<_> = r
            .policy_findings
            .iter()
            .filter(|f| f.rule == "approval.heterogeneous_batch")
            .collect();
        assert_eq!(batch.len(), 1, "{:?}", r.policy_findings);
        let f = batch[0];
        assert_eq!(
            f.task.as_deref(),
            Some("gate"),
            "the bundling prompt is named"
        );
        assert!(
            f.detail.contains("exec · net")
                && f.detail.contains("act")
                && f.detail.contains("page"),
            "the classes + the witness tasks: {}",
            f.detail
        );
        // The unified fold speaks the approval-capability code (NEP-0013),
        // never the spec-10 policy code.
        let u = r
            .findings
            .iter()
            .find(|f| f.kind == "policy" && f.message.contains("heterogeneous_batch"))
            .expect("the batch row in findings[]");
        assert_eq!(u.code.as_deref(), Some("NIKA-SEC-010"));
        assert!(
            r.extra_conformance_codes()
                .iter()
                .any(|c| c.to_string() == "NIKA-SEC-010"),
            "the codes surface speaks it too"
        );
    }

    /// Homogeneous batches stay legal: ONE prompt gating two execs is
    /// one class — the runtime dedups identical content, nothing to refuse.
    #[test]
    fn approval_batch_homogeneous_is_clean() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  require:\n    human_gate_before: [exec]\n  endorsement: solo\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  gate:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"Proceed?\", default: false }\n  one:\n    with: { go: \"${{ tasks.gate.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"a\"] }\n  two:\n    with: { go: \"${{ tasks.gate.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"b\"] }\n",
        );
        assert!(r.is_clean(), "{:?}", r.policy_findings);
    }

    /// The batch law is scoped to the DECLARED gate lane: no
    /// `require.human_gate_before`, no judgment (the green templates keep
    /// their shape — a prompt before an exec+notify is legal until the
    /// author declares the gate contract).
    #[test]
    fn approval_batch_is_inert_without_the_declared_lane() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\", \"nika:notify\"]\n  net: { http: [\"hooks.slack.com\"] }\ntasks:\n  gate:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"Proceed?\", default: false }\n  act:\n    with: { go: \"${{ tasks.gate.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"x\"] }\n  record:\n    after: { act: success }\n    invoke:\n      tool: \"nika:notify\"\n      args: { url: \"https://hooks.slack.com/x\", message: \"done\" }\n",
        );
        assert!(
            r.policy_findings.is_empty(),
            "no declared lane, no claim: {:?}",
            r.policy_findings
        );
    }

    /// The nearest gate owns its closure: a first prompt whose only
    /// descendant is a SECOND prompt is not a batch — the second is
    /// judged on what IT unleashes (here: two classes → it is refused).
    #[test]
    fn approval_batch_stops_at_the_nearest_gate() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  require:\n    human_gate_before: [exec, net]\npermits:\n  exec: [\"echo\"]\n  net: { http: [\"example.com\"] }\n  tools: [\"nika:prompt\", \"nika:fetch\"]\ntasks:\n  first:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"one?\", default: false }\n  second:\n    after: { first: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"two?\", default: false }\n  act:\n    after: { second: success }\n    exec: { command: [\"echo\", \"x\"] }\n  page:\n    after: { second: success }\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://example.com/x\" }\n",
        );
        let batch: Vec<_> = r
            .policy_findings
            .iter()
            .filter(|f| f.rule == "approval.heterogeneous_batch")
            .collect();
        assert_eq!(batch.len(), 1, "{:?}", r.policy_findings);
        assert_eq!(
            batch[0].task.as_deref(),
            Some("second"),
            "the NEAREST gate owns the batch: {:?}",
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

    // ── F-P23 (NEP-0017) · the named solo mode of endorsement ────────

    /// A human gate under a declared `policy:` block that names NO
    /// endorsement mode refuses at check (F-F5 fail-closed) — the wire
    /// code is NIKA-SEC-013 (never the policy-lane code) and the row
    /// folds into findings[] like every `PolicyViolation`.
    #[test]
    fn endorsement_undeclared_with_a_gate_is_refused_with_the_sec_code() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  limits: { max_tasks: 5 }\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  human:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"Proceed?\", default: false }\n  act:\n    with: { go: \"${{ tasks.human.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"shipped\"] }\n",
        );
        assert!(!r.is_clean());
        assert_eq!(
            r.policy_findings.len(),
            1,
            "the endorsement refusal is the only claim: {:?}",
            r.policy_findings
        );
        let f = &r.policy_findings[0];
        assert_eq!(f.rule, "endorsement.undeclared_mode");
        assert_eq!(f.task.as_deref(), Some("human"), "the gate is named");
        assert!(
            f.detail.contains("declare `policy: { endorsement: solo }`"),
            "the fix is taught: {}",
            f.detail
        );
        // The unified fold speaks NIKA-SEC-013 (NEP-0017), never the
        // spec-10 policy code — same discrimination as the approval batch.
        let u = r
            .findings
            .iter()
            .find(|f| f.kind == "policy" && f.message.contains("endorsement.undeclared_mode"))
            .expect("the endorsement row in findings[]");
        assert_eq!(u.code.as_deref(), Some("NIKA-SEC-013"));
        assert_eq!(
            u.docs_url.as_deref(),
            Some("https://nika.sh/errors/NIKA-SEC-013")
        );
        assert!(
            r.extra_conformance_codes()
                .iter()
                .any(|c| c.to_string() == "NIKA-SEC-013"),
            "the codes surface speaks it too"
        );
    }

    /// The positive half of the law: the declared solo + ONE gate (the
    /// F-P4 approval machinery carries the bound-logged authorization) —
    /// PASS, nothing to judge.
    #[test]
    fn endorsement_solo_declared_with_one_gate_is_clean() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  endorsement: solo\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  human:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"Proceed?\", default: false }\n  act:\n    with: { go: \"${{ tasks.human.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"shipped\"] }\n",
        );
        assert!(r.is_clean(), "{:?}", r.policy_findings);
    }

    /// A declared solo with TWO gates is the declaration lying — refused
    /// as `endorsement.solo_count` (workflow-level: no task witness), in
    /// the same NIKA-SEC-013 voice.
    #[test]
    fn endorsement_solo_with_two_gates_is_the_declaration_lying() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npolicy:\n  endorsement: solo\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  first:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"one?\", default: false }\n  second:\n    after: { first: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"two?\", default: false }\n  act:\n    after: { second: success }\n    exec: { command: [\"echo\", \"shipped\"] }\n",
        );
        assert!(!r.is_clean());
        assert_eq!(
            r.policy_findings.len(),
            1,
            "the count lie is the only claim: {:?}",
            r.policy_findings
        );
        let f = &r.policy_findings[0];
        assert_eq!(f.rule, "endorsement.solo_count");
        assert_eq!(f.task, None, "workflow-level — the claim is judged");
        assert!(
            f.detail.contains("first") && f.detail.contains("second"),
            "both gates named: {}",
            f.detail
        );
        let u = r
            .findings
            .iter()
            .find(|f| f.kind == "policy" && f.message.contains("solo_count"))
            .expect("the solo_count row in findings[]");
        assert_eq!(u.code.as_deref(), Some("NIKA-SEC-013"));
    }

    /// No `policy:` block at all = no named law bound — the lane stays
    /// out (the vendored templates keep their shape; only a DECLARED
    /// block triggers the fail-closed mode law).
    #[test]
    fn endorsement_lane_is_inert_without_a_policy_block() {
        let r = report(
            "nika: v1\nworkflow:\n  id: t\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  human:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"Proceed?\", default: false }\n  act:\n    with: { go: \"${{ tasks.human.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"echo\", \"shipped\"] }\n",
        );
        assert!(
            r.policy_findings.is_empty(),
            "no policy block, no claim: {:?}",
            r.policy_findings
        );
    }
}
