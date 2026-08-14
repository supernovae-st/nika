// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The lethal-trifecta lane (NEP-0002 · `NIKA-SEC-009`) — needle-thin BY
//! DESIGN (the `super::policy` precedent): this module only PROJECTS the
//! workflow; the pure judge lives in `nika-cap`, the realized-flow facts
//! come from `super::content_flow`.
//!
//! - **egress-capable** = `exec:` · `invoke:` with a net/fs-write effect
//!   (the ONE effect table) · `mcp:` (fail-closed) · `agent:` whose
//!   whitelist admits an egress-effecting tool. `infer:` is not egress; a
//!   child-workflow call is spec 14's (`NIKA-COMP-002`), never here.
//! - **blocking human gate** = an `invoke:` of `nika:prompt` with NO
//!   `default:` arg (the NEP's escape is the HUMAN decision). Gated on a
//!   valid DAG + a DECLARED `permits:` block (skipped, never wrong).

use nika_cap::TrifectaSubject;

use crate::analyzer::Edge;
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::source::Spanned;

/// Judge the trifecta over the derived graph. Empty unless the workflow
/// declares `permits:` AND the trifecta legs all hold AND an egress-capable
/// task the untrusted content reaches escapes gate dominance.
#[must_use]
pub fn scan_trifecta(
    wf: &RawWorkflow,
    edges: &[Edge],
    topo_waves: &[Vec<usize>],
) -> Vec<nika_cap::TrifectaViolation> {
    let Some(permits) = wf.permits.as_ref() else {
        return Vec::new();
    };
    // The realized-flow facts (v2.0). The MCP trust closure is the
    // catalog mark's seam — until it lands, every server is untrusted.
    let mcp_trusted = |_: &str| false;
    let witnesses = super::content_flow::analyze_content_flow(wf, topo_waves, &mcp_trusted);
    let mut subjects: Vec<TrifectaSubject> = wf
        .tasks
        .iter()
        .map(|t| {
            let task = &t.value;
            TrifectaSubject::new(
                task.id.value.clone(),
                egress_capable(&task.action),
                human_gate(&task.action),
            )
            .with_ingress_source(super::content_flow::classify(&task.action, &mcp_trusted).0)
        })
        .collect();
    for e in edges {
        if let Some(s) = subjects.get_mut(e.to) {
            s.parents.push(e.from);
        }
    }
    let topo_flat: Vec<usize> = topo_waves.iter().flatten().copied().collect();
    nika_cap::trifecta_violations(&permits.value, &subjects, &witnesses, &topo_flat)
}

/// The task's egress capability (see the module doc for the table).
pub(crate) fn egress_capable(action: &RawAction) -> bool {
    match action {
        RawAction::Exec(_) => true,
        RawAction::Agent(a) => agent_egress(&a.tools),
        RawAction::Infer(_) => false,
        RawAction::Invoke(inv) => {
            let Some(tool) = inv.tool() else {
                // A child-workflow call: spec 14 (COMP-002) owns the child's
                // boundary — this lane does not re-judge it.
                return false;
            };
            let id = tool.value.as_str();
            if id.starts_with("mcp:") {
                // Server-side effects — the `tools:` grant is the boundary.
                return true;
            }
            let args = inv.args.as_ref().map(|a| &a.value);
            match nika_cap::builtin_effect(id, args) {
                Some(nika_cap::BuiltinEffect::Net { .. }) => true,
                Some(nika_cap::BuiltinEffect::Fs { writes, .. }) => writes,
                None => false,
            }
        }
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
}

/// An agent's whitelist admits an egress-effecting tool (v2.0's aim) — any
/// glob covering a net/fs-write builtin (the ONE effect table) or `mcp:*`
/// (fail-closed). A pure-compute whitelist (`["nika:jq"]`) is NOT egress.
fn agent_egress(tools: &[Spanned<String>]) -> bool {
    tools.iter().any(|t| {
        let g = t.value.as_str();
        !g.starts_with('!')
            && (g.starts_with("mcp:")
                || nika_catalog::all_builtins()
                    .iter()
                    .map(|b| format!("nika:{}", b.name))
                    .any(|id| nika_cap::glob_matches(g, &id) && nika_cap::builtin_egresses(&id)))
    })
}

/// A BLOCKING `invoke: nika:prompt` (no `default:` arg) is the NEP's gate.
fn human_gate(action: &RawAction) -> bool {
    let RawAction::Invoke(inv) = action else {
        return false;
    };
    let Some(tool) = inv.tool() else {
        return false;
    };
    if tool.value != nika_cap::HUMAN_GATE_TOOL {
        return false;
    }
    inv.args
        .as_ref()
        .and_then(|a| a.value.as_object())
        .is_none_or(|o| !o.contains_key("default"))
}

#[cfg(test)]
mod tests {
    use crate::check;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// The six NEP-0002 conformance cases (the spec-side fixtures under
    /// `conformance/security/` mirror these one-for-one).
    const TRIFECTA: &str = "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\", \"nika:write\", \"nika:prompt\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  leak:\n    after: { fetch_page: success }\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" }\n";

    /// ①∧②∧③ declared, no gate → the diagnostic, once per ungated egress
    /// task THE CONTENT REACHES (v2.0: `leak` is the realized sink;
    /// `fetch_page` is the SOURCE — its own args are operator content, so
    /// it is not itself tainted), message opening with the NEP's verbatim.
    #[test]
    fn trifecta_complete_refuse() {
        let r = report(TRIFECTA);
        assert_eq!(
            r.trifecta_findings.len(),
            1,
            "only the realized sink is flagged (v2.0 precision): {:?}",
            r.trifecta_findings
        );
        let v = &r.trifecta_findings[0];
        assert_eq!(v.task, "leak");
        assert_eq!(v.source.as_deref(), Some("fetch_page"));
        assert!(
            v.detail
                .starts_with("lethal trifecta complete · human gate required"),
            "{}",
            v.detail
        );
        assert!(
            v.detail.contains("`fetch_page` reaches egress task `leak`"),
            "the flow witness is named: {}",
            v.detail
        );
        assert!(!r.is_clean(), "the lane gates the check");
        let f = r
            .findings
            .iter()
            .find(|f| f.kind == "trifecta")
            .expect("trifecta row in findings[]");
        assert_eq!(f.gate, "TRIFECTA");
        assert_eq!(f.code.as_deref(), Some("NIKA-SEC-009"));
        assert_eq!(
            f.docs_url.as_deref(),
            Some("https://nika.sh/language/errors/NIKA-SEC-009")
        );
        // The conformance-code surface speaks the same code (one voice).
        let codes: Vec<String> = r
            .extra_conformance_codes()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            codes.iter().filter(|c| *c == "NIKA-SEC-009").count(),
            1,
            "one SEC-009 per finding: {codes:?}"
        );
    }

    /// A blocking `nika:prompt` dominating every egress path → clean.
    #[test]
    fn trifecta_gated_pass() {
        let gated = TRIFECTA.replacen(
            "tasks:\n  fetch_page:",
            "tasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"choice\", message: \"exfiltrate?\", choices: [\"no\", \"yes\"] }\n  fetch_page:\n    after: { ask: success }",
            1,
        );
        let r = report(&gated);
        assert!(
            r.trifecta_findings.is_empty(),
            "the gate dominates fetch_page AND leak: {:?}",
            r.trifecta_findings
        );
    }

    /// Drop each leg in turn → clean (the Rule of Two holds unattended).
    #[test]
    fn two_of_three_pass() {
        // No ① (no fs.read).
        let no_read = TRIFECTA.replacen(
            "  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n",
            "  fs: { write: [\"./out/**\"] }\n",
            1,
        );
        assert!(
            report(&no_read).trifecta_findings.is_empty(),
            "① dropped → clean"
        );
        // No ② (no fetch task, no tools grant) — the write stays, gated
        // by nothing, but with no ingress there is no trifecta.
        let no_ingress = "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\ntasks:\n  think:\n    infer: { prompt: \"summarize\", max_tokens: 9 }\n";
        assert!(
            report(no_ingress).trifecta_findings.is_empty(),
            "② dropped → clean"
        );
        // No ③ (no net · workspace-confined writes · no exec) — fetch
        // still pulls untrusted content, but nothing can leave.
        let no_egress = "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  tools: [\"nika:fetch\", \"nika:write\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  save:\n    after: { fetch_page: success }\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/save.txt\", content: \"${{ with.body }}\" }\n";
        assert!(
            report(no_egress).trifecta_findings.is_empty(),
            "③ dropped → clean"
        );
    }

    /// A gate on a SIBLING branch dominates nothing → the diagnostic.
    #[test]
    fn gate_present_not_dominating_refuse() {
        let bypass = TRIFECTA.replacen(
            "tasks:\n  fetch_page:",
            "tasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"anything?\" }\n  fetch_page:",
            1,
        );
        // `ask` is an entry with no downstream edge — fetch_page/leak run
        // on a parallel branch the gate never dominates.
        let r = report(&bypass);
        assert_eq!(
            r.trifecta_findings.len(),
            1,
            "a bypassable gate mitigates nothing: {:?}",
            r.trifecta_findings
        );
        assert_eq!(r.trifecta_findings[0].task, "leak");
    }

    /// THE LAW (A-1b · user gauntlet 2026-07-31 · G-07 · Nina): when a
    /// blocking gate EXISTS but fails to dominate, the finding names the
    /// gate, names the edge that reaches the sink without crossing it,
    /// and teaches the placement that works — upstream of the ingress.
    /// Nina applied the printed fix verbatim (a gate parked between the
    /// judge and the sink) and got a byte-identical finding with the
    /// rule never explained; this pins the explanation.
    #[test]
    fn undominated_gate_names_itself_the_bypass_and_the_upstream_placement() {
        // The gauntlet shape: ingress → judge → gate → sink, where the
        // sink ALSO reads the judge's output (`with:` data edge) — the
        // exact placement the old fix taught, and the exact edge that
        // defeats it.
        let y = "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\", \"nika:write\", \"nika:prompt\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  approve:\n    after: { fetch_page: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"ship?\" }\n  leak:\n    after: { approve: success }\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" }\n";
        let r = report(y);
        assert_eq!(r.trifecta_findings.len(), 1, "{:?}", r.trifecta_findings);
        let d = &r.trifecta_findings[0].detail;
        assert!(
            d.contains("the blocking gate `approve` does not dominate it"),
            "the existing gate is NAMED: {d}"
        );
        assert!(
            d.contains("the edge from `fetch_page` reaches `leak` without crossing the gate"),
            "the bypassing edge is named: {d}"
        );
        assert!(
            d.contains("a `with:` data edge is a path too"),
            "the rule itself is spoken: {d}"
        );
        assert!(
            d.contains("fix: place the blocking `invoke: nika:prompt` upstream of `fetch_page`"),
            "the placement that works is taught — upstream of the ingress: {d}"
        );
        // And the gate placed THERE goes clean (the taught fix, applied,
        // actually changes the verdict — the zero-delta trap dies).
        let upstream = "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\", \"nika:write\", \"nika:prompt\"]\ntasks:\n  approve:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"scan and ship?\" }\n  fetch_page:\n    after: { approve: success }\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  leak:\n    after: { fetch_page: success }\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" }\n";
        assert!(
            report(upstream).trifecta_findings.is_empty(),
            "the taught placement, applied, goes clean"
        );
    }

    /// The no-gate arm keeps the generic clause AND teaches the same
    /// upstream placement (the old fix said « gate the egress path » —
    /// the one position the data edges defeat).
    #[test]
    fn no_gate_finding_teaches_the_upstream_placement() {
        let r = report(TRIFECTA);
        let d = &r.trifecta_findings[0].detail;
        assert!(
            d.contains("no blocking `invoke: nika:prompt` dominates every path to it"),
            "{d}"
        );
        assert!(
            d.contains("upstream of `fetch_page`"),
            "the fix points at the ingress end, never the egress end: {d}"
        );
        assert!(
            !d.contains("gate the egress path"),
            "the defeated placement is no longer taught: {d}"
        );
    }

    /// A `default:`-carrying prompt is NOT a gate (the run proceeds
    /// unattended — the NEP wants the human decision, not the fallback).
    #[test]
    fn a_defaulted_prompt_is_not_blocking() {
        let defaulted = TRIFECTA.replacen(
            "tasks:\n  fetch_page:",
            "tasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"ok?\", default: true }\n  fetch_page:\n    after: { ask: success }",
            1,
        );
        let r = report(&defaulted);
        assert_eq!(
            r.trifecta_findings.len(),
            1,
            "default: true answers without a human: {:?}",
            r.trifecta_findings
        );
        assert_eq!(r.trifecta_findings[0].task, "leak");
    }

    /// No `permits:` block → the legs are not decidable as declared → the
    /// lane is inert (the default-deny/floor lanes own that world). The
    /// fixture is conformance-clean so the test measures the LANE, not
    /// the broken-DAG skip.
    #[test]
    fn no_declared_boundary_no_claim() {
        let r = report(
            "nika: t\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  leak:\n    after: { fetch_page: success }\n    exec: { command: [\"echo\", \"x\"] }\n",
        );
        assert!(
            r.conformance.is_empty(),
            "fixture must be conformance-clean: {:?}",
            r.conformance
        );
        assert!(r.trifecta_findings.is_empty(), "{:?}", r.trifecta_findings);
    }

    /// An unanalyzable DAG yields NO claim (the IFC/policy gating
    /// precedent: skipped, never wrong).
    #[test]
    fn broken_dag_skips_the_lane() {
        let r = report(
            "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  act:\n    after: { ghost: success }\n    exec: { command: [\"echo\", \"x\"] }\n",
        );
        assert!(!r.conformance.is_empty());
        assert!(r.trifecta_findings.is_empty(), "{:?}", r.trifecta_findings);
    }

    // ── v2.0 pins (the realized-flow judgment) ─────────────────────────

    /// The v2.0 behavior-change pin: an ingress-capable GRANT that is
    /// never invoked arms nothing (v1.1 fired here).
    #[test]
    fn granted_not_invoked_pass() {
        let r = report(
            "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\", \"nika:write\"]\ntasks:\n  save:\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/report.txt\", content: \"pure operator content\" }\n",
        );
        assert!(
            r.trifecta_findings.is_empty(),
            "granted-but-never-invoked fetch → no realized source → clean: {:?}",
            r.trifecta_findings
        );
    }

    /// The integrity-inversion pin: a model SUMMARY of attacker content
    /// carries the payload (the confidentiality carve-out does not apply).
    #[test]
    fn flow_through_infer_refuse() {
        let r = report(
            "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\", \"nika:notify\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  summarize:\n    with: { page: \"${{ tasks.fetch_page.output }}\" }\n    infer: { prompt: \"tldr: ${{ with.page }}\", max_tokens: 99 }\n  tell:\n    with: { summary: \"${{ tasks.summarize.output }}\" }\n    invoke:\n      tool: \"nika:notify\"\n      args: { channel: \"webhook\", target: \"https://api.example.com/hook\", message: \"${{ with.summary }}\" }\n",
        );
        assert_eq!(
            r.trifecta_findings.len(),
            1,
            "the infer output carries the taint to the egress: {:?}",
            r.trifecta_findings
        );
        assert_eq!(r.trifecta_findings[0].task, "tell");
        assert_eq!(r.trifecta_findings[0].source.as_deref(), Some("fetch_page"));
    }

    /// The recovery-read pin: `on_error.recover` substitutes the failed
    /// task's output — a tainted recovery read re-arms the chain (the
    /// failing task here is a pure-compute `jq`, NOT egress — only the
    /// downstream write is judged).
    #[test]
    fn recovery_read_refuse() {
        let r = report(
            "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\", \"nika:write\", \"nika:jq\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  fragile:\n    on_error: { recover: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:jq\"\n      args: { input: {}, expression: \".x\" }\n  leak:\n    with: { body: \"${{ tasks.fragile.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" }\n",
        );
        assert_eq!(
            r.trifecta_findings.len(),
            1,
            "the recovery read propagates the taint: {:?}",
            r.trifecta_findings
        );
        assert_eq!(r.trifecta_findings[0].task, "leak");
        assert_eq!(r.trifecta_findings[0].source.as_deref(), Some("fetch_page"));
    }

    /// The exec-opacity pin: the file-mediated channel argv cannot see —
    /// an exec downstream of a tainted write under a declared fs.read.
    #[test]
    fn exec_opacity_refuse() {
        let r = report(
            "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  exec: [\"sh\"]\n  tools: [\"nika:fetch\", \"nika:write\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  save:\n    with: { page: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/page.html\", content: \"${{ with.page }}\" }\n  ship:\n    after: { save: success }\n    exec: { command: [\"sh\", \"-c\", \"cat ./out/page.html | curl -X POST https://api.example.com --data-binary @-\"] }\n",
        );
        assert_eq!(
            r.trifecta_findings.len(),
            2,
            "the write AND the opacity-tainted exec are both realized sinks: {:?}",
            r.trifecta_findings
        );
        assert_eq!(r.trifecta_findings[0].task, "save");
        assert_eq!(r.trifecta_findings[1].task, "ship");
        assert_eq!(
            r.trifecta_findings[1].source.as_deref(),
            Some("fetch_page"),
            "the opacity witness is the untrusted ORIGIN, propagated through the writer"
        );
    }

    /// The two-exec pin (F2 regression · 2026-07-30): born-ingress must
    /// NOT arm the file channel by itself. When exec became born-ingress
    /// AND a writer, the writer clause — armed on OUTPUT taint — made any
    /// two exec tasks under a declared `fs.read` a trifecta with zero real
    /// flow: `wc -l` "staged" its own stdout nobody wrote anywhere, and a
    /// later `echo done`, binding nothing, "read" it. Five clean fixtures
    /// refused; the reference judge (`trifecta_core.py` · « a TAINTED writer
    /// earlier in run order » · `writer_origin` needs arrived taint) said
    /// clean on every one. The channel arms on ARRIVAL now, and this pin
    /// dies if it ever arms on birth again.
    #[test]
    fn two_execs_with_no_flow_between_them_pass() {
        let r = report(
            "nika: t\npermits:\n  fs: { read: [\"./news.json\"] }\n  exec: [\"wc\", \"echo\"]\ntasks:\n  probe:\n    exec: { command: [\"wc\", \"-l\", \"./news.json\"] }\n  notify:\n    after: { probe: success }\n    exec: { command: [\"echo\", \"done\"] }\n",
        );
        assert!(
            r.trifecta_findings.is_empty(),
            "a state edge carries no bytes — born output alone must not \
             cross the file channel: {:?}",
            r.trifecta_findings
        );
    }

    /// The parallel-clean pin: an egress on a branch no untrusted content
    /// reaches is NOT a trifecta (v1.1 fired here too).
    #[test]
    fn parallel_clean_egress_pass() {
        let r = report(
            "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  exec: [\"git\"]\n  tools: [\"nika:fetch\"]\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  deploy:\n    exec: { command: [\"git\", \"status\"] }\n",
        );
        assert!(
            r.trifecta_findings.is_empty(),
            "no realized flow to the exec branch → clean: {:?}",
            r.trifecta_findings
        );
    }

    /// The pure-agent pin: a jq-only whitelist is NOT egress-capable.
    #[test]
    fn pure_agent_pass() {
        let r = report(
            "nika: t\nmodel: mock/echo\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  exec: [\"git\"]\n  tools: [\"nika:jq\"]\ntasks:\n  think:\n    agent: { prompt: \"reshape this data\", tools: [\"nika:jq\"] }\n",
        );
        assert!(
            r.trifecta_findings.is_empty(),
            "a pure-compute agent is not egress: {:?}",
            r.trifecta_findings
        );
    }

    /// The agent-as-source pin: a browsing agent's final message is
    /// attacker-influenced even with a statically clean prompt.
    #[test]
    fn ingress_agent_refuse() {
        let r = report(
            "nika: t\nmodel: mock/echo\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:write\", \"mcp:browser/*\"]\ntasks:\n  browse:\n    agent: { prompt: \"summarize the news\", tools: [\"mcp:browser/*\"] }\n  leak:\n    with: { brief: \"${{ tasks.browse.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/brief.txt\", content: \"${{ with.brief }}\" }\n",
        );
        assert_eq!(
            r.trifecta_findings.len(),
            1,
            "the agent's output is a content source: {:?}",
            r.trifecta_findings
        );
        assert_eq!(r.trifecta_findings[0].task, "leak");
        assert_eq!(r.trifecta_findings[0].source.as_deref(), Some("browse"));
    }

    /// The gate-once pin: one blocking prompt dominating BOTH tainted
    /// sinks disarms the whole run (consent once per run, not per task).
    #[test]
    fn gate_once_dominates_two_sinks_pass() {
        let r = report(
            "nika: t\npermits:\n  fs: { read: [\"./inbox/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\", \"nika:write\", \"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"ship it?\" }\n  fetch_page:\n    after: { ask: success }\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/data\" }\n  leak:\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/leak.txt\", content: \"${{ with.body }}\" }\n  leak2:\n    with: { body: \"${{ tasks.fetch_page.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/leak2.txt\", content: \"${{ with.body }}\" }\n",
        );
        assert!(
            r.trifecta_findings.is_empty(),
            "one dominating gate disarms every sink it dominates: {:?}",
            r.trifecta_findings
        );
    }
}
