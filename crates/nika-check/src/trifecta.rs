// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The lethal-trifecta lane (NEP-0002 · `NIKA-SEC-009`) — needle-thin BY
//! DESIGN (the `super::policy` precedent): this module only PROJECTS the
//! workflow; the pure judge lives in `nika-cap`, the realized-flow facts
//! come from `super::content_flow`.
//!
//! - **egress-capable** = `exec:` · `invoke:` with a net/fs-write effect
//!   (the ONE effect table) · `mcp:` (fail-closed) · `agent:` whose
//!   whitelist admits an egress-effecting tool. `infer:` is not egress.
//! - **an `invoke: workflow:` call** is judged through the child closure a
//!   composed check resolved ([`scan_trifecta_composed`] · NIK-273): an
//!   ingress when the closure brings untrusted content in, a source when
//!   that content reaches the call's output or a file, and an egress-capable
//!   sink when untrusted content (its own, the call's arguments, or a file a
//!   writer left) reaches an egress task in it. A closure the check cannot
//!   resolve is all of these (fail-closed). A blocking gate inside the child
//!   is not credited here: the caller's own gate upstream of the call is.
//!   Only a composed check resolves calls.
//! - **blocking human gate** = an `invoke:` of `nika:prompt` with NO
//!   `default:` arg (the NEP's escape is the HUMAN decision). Gated on a
//!   valid DAG + a DECLARED `permits:` block (skipped, never wrong).

use nika_cap::TrifectaSubject;

use crate::ComposedCall;
use crate::analyzer::Edge;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_schema::source::Spanned;

/// Judge the trifecta over the derived graph. Empty unless the workflow
/// declares `permits:` AND the trifecta legs all hold AND an egress-capable
/// task the untrusted content reaches escapes gate dominance.
///
/// Reader-less: no child workflow is resolved, so an `invoke: workflow:`
/// call is never an ingress here (the [`crate::check`] posture, like its
/// composition and cost lanes). [`scan_trifecta_composed`] judges the calls
/// a composed check resolved.
#[must_use]
pub fn scan_trifecta(
    wf: &RawWorkflow,
    edges: &[Edge],
    topo_waves: &[Vec<usize>],
) -> nika_cap::TrifectaVerdict {
    scan_trifecta_composed(wf, edges, topo_waves, &[])
}

/// [`scan_trifecta`] over the child calls a composed check resolved
/// ([`crate::CheckReport::composed_calls`] · spec 14 · NIK-273): each call
/// stands for the tasks of its child closure, exactly as they would be
/// judged inline (module doc), and a refusal it feeds or receives names the
/// child task behind it. A call `calls` does not list is judged child-blind:
/// no ingress, no egress, its arguments flowing to its output.
#[must_use]
pub fn scan_trifecta_composed(
    wf: &RawWorkflow,
    edges: &[Edge],
    topo_waves: &[Vec<usize>],
    calls: &[ComposedCall],
) -> nika_cap::TrifectaVerdict {
    let Some(permits) = wf.permits.as_ref() else {
        return nika_cap::TrifectaVerdict::default();
    };
    // The realized-flow facts (v2.0). The MCP trust closure is the
    // catalog mark's seam — until it lands, every server is untrusted.
    let mcp_trusted = |_: &str| false;
    let sources = super::content_flow::Sources {
        mcp_trusted: &mcp_trusted,
        calls,
        entry: super::content_flow::Entry::Born,
        exec_reads_files: None,
    };
    let flow = super::content_flow::analyze_content_flow(wf, topo_waves, &sources);
    let mut subjects: Vec<TrifectaSubject> = wf
        .tasks
        .iter()
        .map(|t| {
            let task = &t.value;
            let call = listed_call(calls, &task.id.value, &task.action);
            TrifectaSubject::new(
                task.id.value.clone(),
                egress_capable(&task.action) || call.is_some_and(call_egress_capable),
                human_gate(&task.action),
            )
            .with_defaulted_gate(defaulted_gate(&task.action))
            .with_ingress_source(
                super::content_flow::classify(&task.action, &mcp_trusted).0
                    || call.is_some_and(|c| c.ingress.is_some()),
            )
        })
        .collect();
    for e in edges {
        if let Some(s) = subjects.get_mut(e.to) {
            s.parents.push(e.from);
        }
    }
    let topo_flat: Vec<usize> = topo_waves.iter().flatten().copied().collect();
    let mut verdict =
        nika_cap::trifecta_verdict(&permits.value, &subjects, &flow.witnesses, &topo_flat);
    name_composed_sources(&mut verdict, wf, calls);
    verdict
}

/// The listed call a task stands for (a workflow call `calls` names).
pub(crate) fn listed_call<'a>(
    calls: &'a [ComposedCall],
    task_id: &str,
    action: &RawAction,
) -> Option<&'a ComposedCall> {
    let RawAction::Invoke(inv) = action else {
        return None;
    };
    if !matches!(inv.target, RawInvokeTarget::Workflow(_)) {
        return None;
    }
    calls.iter().find(|c| c.task == task_id)
}

/// A call whose closure some untrusted content reaches an egress task of.
fn call_egress_capable(call: &ComposedCall) -> bool {
    call.born.egress.is_some() || call.args.egress.is_some() || call.files.egress.is_some()
}

/// A refusal a composed call takes part in names the child task behind it,
/// after the NEP's sentence: the caller's file shows only `invoke:
/// workflow:`, and « untrusted content from `call` » alone does not say why.
/// Only the refusals whose source or sink IS the call are named; an inline
/// refusal is untouched.
fn name_composed_sources(
    verdict: &mut nika_cap::TrifectaVerdict,
    wf: &RawWorkflow,
    calls: &[ComposedCall],
) {
    for v in &mut verdict.violations {
        // A call that sends out what it brings in is named once, as the sink.
        let as_source = v
            .source
            .as_deref()
            .filter(|s| *s != v.task)
            .and_then(|source| {
                let task = wf.tasks.iter().find(|t| t.value.id.value == source)?;
                let call = listed_call(calls, source, &task.value.action)?;
                let why = call
                    .born
                    .output
                    .as_deref()
                    .or(call.born.file.as_deref())
                    .or(call.born.egress.as_deref())
                    .or(call.ingress.as_deref())?;
                Some(format!(
                    " · `{source}` calls a workflow that brings untrusted content in: {why} \
                 (spec 14: a call carries what its child's body brings in)"
                ))
            });
        let as_sink = wf
            .tasks
            .iter()
            .find(|t| t.value.id.value == v.task)
            .and_then(|task| listed_call(calls, &v.task, &task.value.action))
            .and_then(|call| {
                let born_here = v.source.as_deref() == Some(v.task.as_str());
                if born_here {
                    let why = call.born.egress.as_deref()?;
                    let brought = call.ingress.as_deref().unwrap_or(why);
                    return Some(format!(
                        " · `{}` calls a workflow that brings untrusted content in ({brought}) \
                         and sends it out: {why} (spec 14: the child runs inside this file's \
                         boundary)",
                        v.task
                    ));
                }
                let why = call
                    .args
                    .egress
                    .as_deref()
                    .or(call.files.egress.as_deref())?;
                Some(format!(
                    " · `{}` calls a workflow that sends it out: {why} \
                     (spec 14: the child runs inside this file's boundary)",
                    v.task
                ))
            });
        for tail in [as_source, as_sink].into_iter().flatten() {
            v.detail.push_str(&tail);
        }
    }
}

/// The task's egress capability (see the module doc for the table).
pub(crate) fn egress_capable(action: &RawAction) -> bool {
    match action {
        RawAction::Exec(_) => true,
        RawAction::Agent(a) => agent_egress(&a.tools),
        RawAction::Infer(_) => false,
        RawAction::Invoke(inv) => {
            let Some(tool) = inv.tool() else {
                // A child-workflow call: one task's own bytes say nothing of
                // its child's effects. The trifecta judges the call through
                // its composed closure (`scan_trifecta_composed` · NIK-273).
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

/// A `nika:prompt` that CARRIES a `default:` — the shape the judge never
/// credits; the violation names it so the fix says « remove `default:` ».
fn defaulted_gate(action: &RawAction) -> bool {
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
        .is_some_and(|o| o.contains_key("default"))
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
            d.contains("the blocking gate `approve` does not dominate `leak`"),
            "the existing gate is NAMED: {d}"
        );
        assert!(
            d.contains("its parent `fetch_page` reaches it without crossing the gate"),
            "the bypassing edge is named: {d}"
        );
        assert!(
            d.contains("a `with:` data edge is a path too"),
            "the rule itself is spoken: {d}"
        );
        assert!(
            d.contains("place it upstream of `fetch_page`"),
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

    /// #1394 — the gauntlet's shape: a private read AND an untrusted
    /// fetch both feed the egress through `with:`. The old first refusal
    /// said « upstream of `fetch_untrusted` »; a gate placed exactly there
    /// still lost to the data edge from `read_secret`. The judge names
    /// EVERY entry the gate must precede, and the applied placement is
    /// clean.
    #[test]
    fn the_first_refusal_names_every_entry_the_gate_must_precede() {
        let y = "nika: t\npermits:\n  fs: { read: [\"./secrets/**\"], write: [\"./out/**\"] }\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:read\", \"nika:fetch\", \"nika:write\", \"nika:prompt\"]\ntasks:\n  read_secret:\n    invoke:\n      tool: \"nika:read\"\n      args: { path: \"./secrets/key.txt\" }\n  fetch_untrusted:\n    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \"https://api.example.com/page\" }\n  write_both:\n    with: { a: \"${{ tasks.read_secret.output }}\", b: \"${{ tasks.fetch_untrusted.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/both.txt\", content: \"${{ with.a }} ${{ with.b }}\" }\n";
        let r = report(y);
        assert_eq!(r.trifecta_findings.len(), 1, "{:?}", r.trifecta_findings);
        let d = &r.trifecta_findings[0].detail;
        assert!(
            d.contains("must dominate EVERY path to `write_both`, data edges (`with:`) included"),
            "the dominance rule is stated in full on the FIRST refusal: {d}"
        );
        assert!(
            d.contains("upstream of `read_secret` and `fetch_untrusted`"),
            "every entry the gate must precede is named: {d}"
        );
        assert!(
            d.contains("`permits.tools`") && d.contains("`when:`") && d.contains("`with:`"),
            "the whole repair shape is one sentence: {d}"
        );
        // The taught placement, applied: a gate both entries wait on.
        let gated = y.replace(
            "tasks:\n  read_secret:\n",
            "tasks:\n  approve:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"read and fetch?\" }\n  read_secret:\n    after: { approve: success }\n",
        ).replace(
            "  fetch_untrusted:\n    invoke:",
            "  fetch_untrusted:\n    after: { approve: success }\n    invoke:",
        );
        assert!(
            report(&gated).trifecta_findings.is_empty(),
            "a gate upstream of every entry dominates: {:?}",
            report(&gated).trifecta_findings
        );
    }

    /// #1399 — a purely local shell pipeline trips the trifecta because an
    /// exec output is untrusted ingress and an exec grant is external
    /// egress. The finding SAYS so, and the bypass sentence names a
    /// PARENT, never a second « source » that reads as a contradiction.
    #[test]
    fn a_local_shell_pipeline_names_its_legs_and_speaks_of_a_parent() {
        let y = "nika: etl\npermits:\n  fs: { read: [\"./data/**\"], write: [\"./out/**\"] }\n  exec: true\n  tools: [\"nika:prompt\"]\ntasks:\n  extract:\n    exec: { command: [\"cat\", \"./data/in.csv\"] }\n  gate:\n    after: { extract: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"confirm\", message: \"load?\" }\n  transform:\n    after: { gate: success }\n    with: { rows: \"${{ tasks.extract.output }}\" }\n    exec: { command: [\"sort\"], stdin: \"${{ with.rows }}\" }\n";
        let r = report(y);
        assert_eq!(r.trifecta_findings.len(), 1, "{:?}", r.trifecta_findings);
        let d = &r.trifecta_findings[0].detail;
        assert!(
            d.contains("legs here:")
                && d.contains("`permits.exec` (an exec output is untrusted content)")
                && d.contains("`permits.exec` (a program reaches anywhere)"),
            "why the three legs are legs HERE is spoken: {d}"
        );
        assert!(
            d.contains("its parent `extract` reaches it without crossing the gate"),
            "the bypass names a PARENT, not a second source: {d}"
        );
        assert!(
            !d.contains("the edge from"),
            "the contradictory phrasing is gone: {d}"
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
        // The why names the default, instead of telling the author to add
        // the gate they just added (wave 3 · persona 07 · the headless-prompt
        // hint had recommended `default:` two lines above).
        let d = &r.trifecta_findings[0].detail;
        for lesson in ["`ask`", "carries `default:`", "--answer ask=<value>"] {
            assert!(d.contains(lesson), "the refusal teaches `{lesson}`: {d}");
        }
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

    /// The file channel's builtin readers (NIK-273 follow-up): a gate
    /// clears the subprocess and the write that drops its output in
    /// `./drop/page.txt`; `pick` then reads that file through `nika:read`
    /// and `leak`, which no gate dominates, sends it out with a private
    /// note. The read carries the content on, as an `exec:` reader does.
    const GATED_DROP: &str = "nika: t\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:read\", \"nika:grep\", \"nika:glob\", \"nika:write\", \"nika:prompt\"]\n  fs:\n    read: [\"./private.txt\", \"./drop\", \"./drop/**\"]\n    write: [\"./drop/**\", \"/tmp/out/**\"]\ninputs:\n  name: { type: string, default: \"page.txt\" }\ntasks:\n  ask:\n    invoke: { tool: \"nika:prompt\", args: { message: \"cache the page?\" } }\n  page:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: \"${{ with.go == true }}\"\n    exec: { command: [\"echo\", \"untrusted page body\"] }\n  save:\n    with: { go: \"${{ tasks.ask.output }}\", body: \"${{ tasks.page.output }}\" }\n    when: \"${{ with.go == true }}\"\n    invoke: { tool: \"nika:write\", args: { path: \"./drop/page.txt\", content: \"${{ with.body }}\" } }\n  pick:\n    after: { save: success }\n    invoke: { tool: \"nika:read\", args: { path: \"./drop/page.txt\" } }\n  secret_note:\n    invoke: { tool: \"nika:read\", args: { path: \"./private.txt\" } }\n  leak:\n    with: { page: \"${{ tasks.pick.output }}\", note: \"${{ tasks.secret_note.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"/tmp/out/leak.txt\", content: \"page=${{ with.page }} note=${{ with.note }}\" } }\n";

    /// `GATED_DROP` with `pick` reading through `invoke`.
    fn drop_read(invoke: &str) -> String {
        GATED_DROP.replacen(
            "    invoke: { tool: \"nika:read\", args: { path: \"./drop/page.txt\" } }\n",
            &format!("    invoke: {invoke}\n"),
            1,
        )
    }

    #[test]
    fn a_builtin_read_of_a_file_a_tainted_writer_left_is_arrived_content() {
        let r = report(GATED_DROP);
        assert!(r.conformance.is_empty(), "{:?}", r.conformance);
        assert_eq!(r.trifecta_findings.len(), 1, "{:?}", r.trifecta_findings);
        let v = &r.trifecta_findings[0];
        assert_eq!(v.task, "leak");
        assert_eq!(
            v.source.as_deref(),
            Some("page"),
            "the origin is the subprocess, through the writer and the read"
        );
        assert!(
            v.detail
                .starts_with("lethal trifecta complete · human gate required"),
            "the inline message, unchanged: {}",
            v.detail
        );
    }

    /// Every read slot nika-cap models reads the channel, and a read the
    /// written path cannot reach stays clean (the path is the channel).
    #[test]
    fn the_file_channel_follows_the_written_path() {
        let tainted = [
            "{ tool: \"nika:read\", args: { path: \"./drop/../drop/page.txt\" } }",
            "{ tool: \"nika:read\", args: { path: \"drop/page.txt\" } }",
            "{ tool: \"nika:read\", args: { path: \"./drop/${{ inputs.name }}\" } }",
            "{ tool: \"nika:grep\", args: { pattern: \"body\", path: \"./drop\" } }",
            "{ tool: \"nika:glob\", args: { pattern: \"./drop/*.txt\" } }",
            // No `path`: grep searches the workspace, `./drop` included.
            "{ tool: \"nika:grep\", args: { pattern: \"body\" } }",
        ];
        for invoke in tainted {
            let r = report(&drop_read(invoke));
            assert!(r.conformance.is_empty(), "{invoke}: {:?}", r.conformance);
            let sinks: Vec<&str> = r
                .trifecta_findings
                .iter()
                .map(|v| v.task.as_str())
                .collect();
            assert_eq!(sinks, ["leak"], "{invoke} reads the dropped page");
        }
        let clean = [
            "{ tool: \"nika:read\", args: { path: \"./drop/other.txt\" } }",
            "{ tool: \"nika:grep\", args: { pattern: \"body\", path: \"./drop/archive\" } }",
            "{ tool: \"nika:read\", args: { path: \"./private.txt\" } }",
        ];
        for invoke in clean {
            let r = report(&drop_read(invoke));
            assert!(r.conformance.is_empty(), "{invoke}: {:?}", r.conformance);
            assert!(
                r.trifecta_findings.is_empty(),
                "{invoke} cannot reach `./drop/page.txt`: {:?}",
                r.trifecta_findings
            );
        }
    }

    /// The channel keeps every tainted writer, not only the first: a read
    /// of the second file is tainted too.
    #[test]
    fn a_read_of_a_later_writers_file_is_tainted() {
        let two = GATED_DROP.replacen(
            "  pick:\n",
            "  save_again:\n    with: { go: \"${{ tasks.ask.output }}\", body: \"${{ tasks.page.output }}\" }\n    when: \"${{ with.go == true }}\"\n    after: { save: success }\n    invoke: { tool: \"nika:write\", args: { path: \"./drop/second.txt\", content: \"${{ with.body }}\" } }\n  pick:\n",
            1,
        );
        let two = two.replacen(
            "    after: { save: success }\n    invoke: { tool: \"nika:read\", args: { path: \"./drop/page.txt\" } }\n",
            "    after: { save_again: success }\n    invoke: { tool: \"nika:read\", args: { path: \"./drop/second.txt\" } }\n",
            1,
        );
        let r = report(&two);
        assert!(r.conformance.is_empty(), "{:?}", r.conformance);
        let sinks: Vec<&str> = r
            .trifecta_findings
            .iter()
            .map(|v| v.task.as_str())
            .collect();
        assert_eq!(sinks, ["leak"], "{:?}", r.trifecta_findings);
    }

    /// The file outlives the run order (NIK-273 review): a read in the
    /// writer's own wave runs beside it (a retry waits for the file), and a
    /// read the run order puts first reads what the previous run left. A
    /// builtin read meets every tainted write of its path, earlier or not.
    #[test]
    fn a_builtin_read_meets_a_tainted_write_whatever_the_order() {
        let pick = "  pick:\n    after: { save: success }\n    invoke: { tool: \"nika:read\", args: { path: \"./drop/page.txt\" } }\n";
        let unmoored = GATED_DROP.replacen(pick, "", 1);
        let same_wave = unmoored.replacen(
            "  save:\n",
            "  pick:\n    after: { page: success }\n    invoke: { tool: \"nika:read\", args: { path: \"./drop/page.txt\" } }\n  save:\n",
            1,
        );
        let read_first = unmoored.replacen(
            "  ask:\n",
            "  pick:\n    invoke: { tool: \"nika:read\", args: { path: \"./drop/page.txt\" } }\n  ask:\n",
            1,
        );
        for (why, yaml) in [("same wave", same_wave), ("read first", read_first)] {
            let r = report(&yaml);
            assert!(r.conformance.is_empty(), "{why}: {:?}", r.conformance);
            let found: Vec<(&str, Option<&str>)> = r
                .trifecta_findings
                .iter()
                .map(|v| (v.task.as_str(), v.source.as_deref()))
                .collect();
            assert_eq!(found, [("leak", Some("page"))], "{why}");
        }
        // A read of another path stays clean in either order.
        let other = unmoored.replacen(
            "  ask:\n",
            "  pick:\n    invoke: { tool: \"nika:read\", args: { path: \"./drop/other.txt\" } }\n  ask:\n",
            1,
        );
        let r = report(&other);
        assert!(r.conformance.is_empty(), "{:?}", r.conformance);
        assert!(r.trifecta_findings.is_empty(), "{:?}", r.trifecta_findings);
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
