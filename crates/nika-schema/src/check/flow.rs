// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Information-flow control — the taint engine (ADR-092 · the keystone).
//!
//! A confidentiality lattice (Denning · *A Lattice Model of Secure
//! Information Flow* · CACM 1976) traces `secrets.X` taint through the
//! workflow ·
//!
//! - **sources** — every `${{ secrets.X }}` reference is `Secret`.
//! - **propagation** — a `with:` binding that references a secret makes
//!   that `with.K` slot secret WITHIN its task; an `exec`/`invoke` task
//!   whose effect-carrying fields reference a secret (directly, via a
//!   `with:` alias, or via a tainted upstream output) makes its OUTPUT
//!   secret (the captured stdout/tool-result can embed the value);
//!   `infer`/`agent` outputs are NOT tainted from a prompt secret — the
//!   provider is operator-chosen and trusted by that choice, and the model
//!   response is not a verbatim echo (ADR-092 carve-out).
//! - **sinks** — a secret reaching an `exec`/`invoke` effect is a LEAK;
//!   a secret reaching the workflow `outputs:` is an EGRESS (the value
//!   leaves the run as the return).
//!
//! **Soundness.** Because the dependency graph is acyclic BY CONSTRUCTION
//! (ADR-002 · cycle = `NIKA-DAG-001`), a single pass in topological order
//! computes the least fixpoint of this monotone propagation — every slot a
//! task reads was finalized in an earlier wave. No iteration to convergence
//! is needed; the topological order IS the fixpoint order.

use std::collections::BTreeMap;

use crate::expression::{NamespaceRef, expr_refs, scan_templates};
use crate::raw::{RawAction, RawTask, RawWorkflow};
use crate::types::{EgressRule, OutputDecl, Permits};

use super::declass;

/// How a value became `Secret` — the chain from the originating secret to
/// the current slot, for auditable diagnostics (« via `with.tok` ← via
/// `tasks.a.output` ← `secrets.api_key` »).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TaintTrace {
    /// The originating `secrets.<name>`.
    pub secret: String,
    /// Human hops from the source to this slot (source first).
    pub hops: Vec<String>,
}

impl TaintTrace {
    /// A fresh trace rooted at a secret reference.
    fn source(secret: &str) -> Self {
        Self {
            secret: secret.to_owned(),
            hops: vec![format!("secrets.{secret}")],
        }
    }

    /// Extend the chain by one hop, preserving the origin.
    fn via(&self, hop: String) -> Self {
        let mut hops = self.hops.clone();
        hops.push(hop);
        Self {
            secret: self.secret.clone(),
            hops,
        }
    }

    /// The chain rendered for a diagnostic (`secrets.x → with.t → ...`).
    #[must_use]
    pub fn render(&self) -> String {
        self.hops.join(" → ")
    }
}

/// The fact base produced by the single topological pass — the taint slice
/// of the unified flow IR (ADR-092 #3). Reports read from this; they never
/// re-walk the AST for taint.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct FlowFacts {
    /// Task index → the taint reaching its EFFECT (`None` = clean). This
    /// is the PROPAGATION fact — it feeds `output_taint` regardless of any
    /// egress sanction (a sanctioned egress still lets the captured output
    /// embed the secret · the sanction clears the SEND, not the capture).
    /// The REPORTABLE leak is [`Self::effect_leak`] (sanction-filtered).
    effect_taint: BTreeMap<usize, TaintTrace>,
    /// Task index → the UNSANCTIONED effect leak (what `scan_leaks`
    /// reports). A secret reaching an `exec`/`invoke` effect whose egress
    /// is NOT author-sanctioned (declass · ADR-092). A sanctioned egress
    /// is in `effect_taint` (propagation) but absent here (no leak).
    effect_leak: BTreeMap<usize, TaintTrace>,
    /// Task index → the taint of its OUTPUT (`tasks.<id>.output`).
    output_taint: BTreeMap<usize, TaintTrace>,
    /// Owner task index → the FIRST UNSANCTIONED taint reaching one of its
    /// `on_finally` cleanup effects, with the sink kind (`exec`/`invoke`)
    /// and the cleanup's index within `on_finally`. A cleanup whose egress
    /// is author-sanctioned (declass · ADR-092) is skipped here exactly
    /// like a sanctioned main effect. Cleanups run under the same boundary
    /// and ALWAYS run.
    finally_effect_taint: BTreeMap<usize, (TaintTrace, &'static str, usize)>,
    /// Workflow `outputs:` entry name → the taint reaching it (egress).
    egress: BTreeMap<String, TaintTrace>,
}

impl FlowFacts {
    /// The taint reaching `task`'s effect, if any — the PROPAGATION fact
    /// (drives output taint · independent of any egress sanction).
    #[must_use]
    pub fn effect_taint(&self, task: usize) -> Option<&TaintTrace> {
        self.effect_taint.get(&task)
    }

    /// The UNSANCTIONED effect leak of `task`, if any — what `scan_leaks`
    /// reports. `Some` ⇒ a secret reaches the effect AND its egress is not
    /// author-sanctioned (declass · ADR-092).
    #[must_use]
    pub fn effect_leak(&self, task: usize) -> Option<&TaintTrace> {
        self.effect_leak.get(&task)
    }

    /// The taint reaching one of `task`'s `on_finally` cleanup effects,
    /// with the sink kind (`exec`/`invoke`) and the cleanup's index within
    /// `on_finally` (the declassification check reads that exact action).
    #[must_use]
    pub fn finally_effect_taint(&self, task: usize) -> Option<(&TaintTrace, &'static str, usize)> {
        self.finally_effect_taint
            .get(&task)
            .map(|(trace, sink, cleanup_idx)| (trace, *sink, *cleanup_idx))
    }

    /// Every workflow-output egress (name → trace), sorted by name.
    #[must_use]
    pub fn egresses(&self) -> Vec<(&str, &TaintTrace)> {
        self.egress.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }
}

/// Run the IFC analysis over a workflow, in the given topological wave
/// order (reuse the analyzer's `topo_waves` — same order the engine runs).
#[must_use]
pub(super) fn analyze_flow(wf: &RawWorkflow, waves: &[Vec<usize>]) -> FlowFacts {
    let declared: Vec<&str> = wf.secrets.iter().map(|(n, _)| n.value.as_str()).collect();
    let mut facts = FlowFacts::default();
    if declared.is_empty() {
        return facts; // no secrets declared → nothing can be tainted
    }
    // The per-secret egress policy (declass · ADR-092) — looked up by name
    // at every leak edge. Empty list / absent = default-deny.
    let egress_of: BTreeMap<&str, &[EgressRule]> = wf
        .secrets
        .iter()
        .map(|(n, s)| (n.value.as_str(), s.value.egress.as_slice()))
        .collect();
    let permits = wf.permits.as_ref().map(|p| &p.value);
    let id_of: BTreeMap<&str, usize> = wf
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.value.id.value.as_str(), i))
        .collect();

    // Single topological pass = the least fixpoint (acyclic ⇒ no iteration).
    for wave in waves {
        for &idx in wave {
            let task = &wf.tasks[idx].value;
            propagate_task(
                idx, task, &declared, &egress_of, permits, &id_of, &mut facts,
            );
        }
    }

    // Egress: a workflow `outputs:` entry referencing a tainted slot.
    for (name, decl) in &wf.outputs {
        if let Some(trace) = taint_of_refs(
            refs_in_str(decl_value(decl)),
            &declared,
            None,
            &id_of,
            &facts,
        ) {
            facts
                .egress
                .insert(name.value.clone(), trace.via("outputs".to_owned()));
        }
    }

    facts
}

/// Compute one task's `with`-slot taints, effect taint, and output taint,
/// recording them in `facts`. Runs once, in topological order, so every
/// upstream `tasks.U.output` taint it reads is already final.
fn propagate_task(
    idx: usize,
    task: &RawTask,
    declared: &[&str],
    egress_of: &BTreeMap<&str, &[EgressRule]>,
    permits: Option<&Permits>,
    id_of: &BTreeMap<&str, usize>,
    facts: &mut FlowFacts,
) {
    // 1. `with:` slot taints (scoped to this task). A with-value referencing
    //    a secret / a tainted upstream output taints that with-key.
    let mut with_taint: BTreeMap<&str, TaintTrace> = BTreeMap::new();
    for (key, value) in &task.with {
        if let Some(trace) = taint_of_refs(refs_in_json(&value.value), declared, None, id_of, facts)
        {
            with_taint.insert(
                key.value.as_str(),
                trace.via(format!("with.{} @ {}", key.value, task.id.value)),
            );
        }
    }

    // 2. `for_each` item taint: if the collection source references a tainted
    //    slot, the loop-local `item` carries that taint within the task.
    let item_taint = task.for_each.as_ref().and_then(|f| match &f.value {
        crate::raw::ForEachValue::Expression(src) => {
            taint_of_refs(refs_in_str(src), declared, Some(&with_taint), id_of, facts)
                .map(|t| t.via(format!("item @ {}", task.id.value)))
        }
        crate::raw::ForEachValue::List(_) => None, // literal list = no taint
    });

    // 3. Effect taint: do this task's effect-carrying fields see a secret?
    let effect = action_effect_fields(&task.action)
        .into_iter()
        .find_map(|text| {
            taint_of_refs_full(
                refs_in_str(text),
                declared,
                &with_taint,
                item_taint.as_ref(),
                id_of,
                facts,
            )
        });
    if let Some(trace) = &effect {
        // Propagation fact (always · feeds output taint below).
        facts.effect_taint.insert(idx, trace.clone());
        // Reportable leak — UNLESS this secret's egress sanctions this sink
        // (declass · L1∧L2∧L3). A sanctioned egress propagates but is no
        // leak (the author cleared the SEND; the capture stays tainted).
        let egress = egress_of.get(trace.secret.as_str()).copied().unwrap_or(&[]);
        if !declass::is_sanctioned(&trace.secret, egress, &task.action, permits) {
            facts.effect_leak.insert(idx, trace.clone());
        }
    }

    // 4. Output taint: an exec/invoke whose effect saw taint can embed it in
    //    the captured output. infer/agent are provider-bound (no output taint).
    let taints_output = matches!(task.action, RawAction::Exec(_) | RawAction::Invoke(_));
    if taints_output && let Some(trace) = &effect {
        facts
            .output_taint
            .insert(idx, trace.via(format!("tasks.{}.output", task.id.value)));
    }

    // 5. `on_finally` cleanup effects are sinks too (extracted for the
    //    100-LOC fn cap · same env / same declass filter as the main effect).
    propagate_cleanups(
        idx,
        task,
        declared,
        egress_of,
        permits,
        &with_taint,
        item_taint.as_ref(),
        id_of,
        facts,
    );
}

/// Record the FIRST UNSANCTIONED `on_finally` cleanup leak of a task. A
/// cleanup `exec`/`invoke` that sees a secret re-emits it exactly like a
/// main effect (and a cleanup ALWAYS runs). Same `with`/`item` env;
/// cleanups have no addressable output, so no output taint here. A cleanup
/// whose egress is author-sanctioned is skipped (declass · ADR-092), and
/// scanning continues so a sanctioned cleanup never masks a later leak.
#[allow(clippy::too_many_arguments)] // the IFC pass threads a wide context
fn propagate_cleanups(
    idx: usize,
    task: &RawTask,
    declared: &[&str],
    egress_of: &BTreeMap<&str, &[EgressRule]>,
    permits: Option<&Permits>,
    with_taint: &BTreeMap<&str, TaintTrace>,
    item_taint: Option<&TaintTrace>,
    id_of: &BTreeMap<&str, usize>,
    facts: &mut FlowFacts,
) {
    for (cleanup_idx, cleanup) in task.on_finally.iter().enumerate() {
        if facts.finally_effect_taint.contains_key(&idx) {
            break; // any one tainted cleanup is enough to flag the task
        }
        let action = &cleanup.value.action;
        let sink = match action {
            RawAction::Exec(_) => "exec",
            RawAction::Invoke(_) => "invoke",
            // A cleanup infer/agent prompt is a provider-egress sink too
            // (BUG#3 · sanctioned via `to: "infer"`/`"agent"`). A cleanup
            // ALWAYS runs, so a secret in its prompt always leaves the run.
            RawAction::Infer(_) => "infer",
            RawAction::Agent(_) => "agent",
        };
        if let Some(trace) = action_effect_fields(action).into_iter().find_map(|text| {
            taint_of_refs_full(
                refs_in_str(text),
                declared,
                with_taint,
                item_taint,
                id_of,
                facts,
            )
        }) {
            // A cleanup whose egress is author-sanctioned is no leak — skip
            // it (and keep scanning later cleanups for a real one).
            let egress = egress_of.get(trace.secret.as_str()).copied().unwrap_or(&[]);
            if declass::is_sanctioned(&trace.secret, egress, action, permits) {
                continue;
            }
            facts.finally_effect_taint.insert(
                idx,
                (
                    trace.via(format!("on_finally @ {}", task.id.value)),
                    sink,
                    cleanup_idx,
                ),
            );
        }
    }
}

/// The text fields of an action's EFFECT (the sink surface) — works for a
/// task's main verb AND an `on_finally` cleanup verb.
///
/// The `infer:`/`agent:` `prompt:` + `system:` ARE sinks: a secret in a
/// prompt LEAVES the run to a third-party provider (the same third-party-
/// network exposure as an `mcp:` tool, which is already flagged). A secret
/// reaching one with no `egress:` sanction is a leak (BUG#3 · now CANONICAL ·
/// spec `01-envelope.md` §egress aligned to this strict behavior · F-03).
/// The OUTPUT carve-out is SEPARATE and PRESERVED (see [`propagate_task`]
/// step 4 · the model response is not a verbatim echo, so infer/agent OUTPUT
/// is never tainted from a prompt secret · ADR-092). To send a secret to a
/// provider, the author sanctions it explicitly (`egress: [{ to: "infer" }]`
/// / `{ to: "agent" }`).
fn action_effect_fields(action: &RawAction) -> Vec<&str> {
    match action {
        RawAction::Exec(a) => {
            let mut fields = a.command.text_fragments();
            if let Some(stdin) = &a.stdin {
                fields.push(stdin.value.as_str());
            }
            for (_, v) in &a.env {
                fields.push(v.value.as_str());
            }
            fields
        }
        RawAction::Invoke(a) => a
            .args
            .as_ref()
            .map(|args| collect_json_strings(&args.value))
            .unwrap_or_default(),
        // The prompt (+ system) is a third-party egress sink — a secret
        // reaching it is a leak unless the author sanctions `to: "infer"`/
        // `"agent"` (BUG#3). Output taint stays excluded (propagate_task §4).
        RawAction::Infer(a) => prompt_system_fields(a.prompt.value.as_str(), a.system.as_ref()),
        RawAction::Agent(a) => prompt_system_fields(a.prompt.value.as_str(), a.system.as_ref()),
    }
}

/// The `prompt:` + optional `system:` text of an `infer:`/`agent:` action —
/// the provider-egress sink surface (BUG#3).
fn prompt_system_fields<'a>(
    prompt: &'a str,
    system: Option<&'a crate::Spanned<String>>,
) -> Vec<&'a str> {
    let mut fields = vec![prompt];
    if let Some(system) = system {
        fields.push(system.value.as_str());
    }
    fields
}

/// Resolve a set of refs to a taint, considering secrets + tainted upstream
/// outputs (no `with`/`item` context — for envelope `outputs:` + with-values).
fn taint_of_refs(
    refs: Vec<NamespaceRef>,
    declared: &[&str],
    with_taint: Option<&BTreeMap<&str, TaintTrace>>,
    id_of: &BTreeMap<&str, usize>,
    facts: &FlowFacts,
) -> Option<TaintTrace> {
    taint_of_refs_full(
        refs,
        declared,
        with_taint.unwrap_or(&BTreeMap::new()),
        None,
        id_of,
        facts,
    )
}

/// The full resolver — secrets, `with`-aliases, `for_each` item, and
/// tainted upstream outputs. Returns the FIRST taint found (any one secret
/// reaching the slot is enough to flag it).
fn taint_of_refs_full(
    refs: Vec<NamespaceRef>,
    declared: &[&str],
    with_taint: &BTreeMap<&str, TaintTrace>,
    item_taint: Option<&TaintTrace>,
    id_of: &BTreeMap<&str, usize>,
    facts: &FlowFacts,
) -> Option<TaintTrace> {
    for r in refs {
        match r {
            NamespaceRef::Secrets(name) if declared.contains(&name.as_str()) => {
                return Some(TaintTrace::source(&name));
            }
            NamespaceRef::With(key) => {
                if let Some(trace) = with_taint.get(key.as_str()) {
                    return Some(trace.clone());
                }
            }
            NamespaceRef::Item => {
                if let Some(trace) = item_taint {
                    return Some(trace.clone());
                }
            }
            NamespaceRef::Tasks { id, .. } => {
                if let Some(&upstream) = id_of.get(id.as_str())
                    && let Some(trace) = facts.output_taint.get(&upstream)
                {
                    return Some(trace.clone());
                }
            }
            _ => {}
        }
    }
    None
}

/// The `${{ … }}` references inside a string (via the real extractor — the
/// same path the analyzer uses, so taint and `NIKA-VAR-001` agree).
fn refs_in_str(text: &str) -> Vec<NamespaceRef> {
    let Ok(islands) = scan_templates(text) else {
        return Vec::new();
    };
    islands.iter().flat_map(|i| expr_refs(&i.expr)).collect()
}

/// References inside any string within a JSON value (with-values, args).
fn refs_in_json(value: &serde_json::Value) -> Vec<NamespaceRef> {
    collect_json_strings(value)
        .into_iter()
        .flat_map(refs_in_str)
        .collect()
}

/// Every string leaf of a JSON value (for the ref/taint scan).
fn collect_json_strings(value: &serde_json::Value) -> Vec<&str> {
    let mut out = Vec::new();
    collect_json_strings_into(value, &mut out);
    out
}

fn collect_json_strings_into<'a>(value: &'a serde_json::Value, out: &mut Vec<&'a str>) {
    match value {
        serde_json::Value::String(s) => out.push(s.as_str()),
        serde_json::Value::Array(items) => {
            for it in items {
                collect_json_strings_into(it, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_json_strings_into(v, out);
            }
        }
        _ => {}
    }
}

/// The `${{ }}` reference expression of an `outputs:` declaration.
fn decl_value(decl: &OutputDecl) -> &str {
    decl.value().value.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::analyze;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn facts(yaml: &str) -> (RawWorkflow, FlowFacts) {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let analyzed = analyze(&wf).expect("analyze");
        let facts = analyze_flow(&wf, &analyzed.topo_waves);
        (wf, facts)
    }

    const S: &str = "secrets:\n  api_key:\n    source: vault\n    key: x\n";

    fn idx(wf: &RawWorkflow, id: &str) -> usize {
        wf.tasks
            .iter()
            .position(|t| t.value.id.value == id)
            .expect("task")
    }

    #[test]
    fn direct_secret_into_exec_is_effect_tainted() {
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: t\n    exec: {{ command: \"curl -H ${{{{ secrets.api_key }}}}\" }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(f.effect_taint(idx(&wf, "t")).is_some());
    }

    #[test]
    fn with_aliased_secret_is_traced_to_the_effect() {
        // The false negative the review found: with: { tok: secret } then
        // ${{ with.tok }} into exec — one hop the old substring scan missed.
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: t\n    with: {{ tok: \"${{{{ secrets.api_key }}}}\" }}\n    exec: {{ command: \"curl -H ${{{{ with.tok }}}}\" }}\n"
        );
        let (wf, f) = facts(&y);
        let trace = f
            .effect_taint(idx(&wf, "t"))
            .expect("with-aliased taint caught");
        assert_eq!(trace.secret, "api_key");
        assert!(
            trace.render().contains("with.tok"),
            "trace shows the alias: {}",
            trace.render()
        );
    }

    #[test]
    fn secret_into_on_finally_cleanup_is_a_sink() {
        // A cleanup exec that sees a secret re-emits it like a main effect —
        // and a cleanup ALWAYS runs (the review's PROVEN blind spot).
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: t\n    exec: {{ command: \"echo build\" }}\n    on_finally:\n      - exec: {{ command: \"curl -d ${{{{ secrets.api_key }}}} x\" }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(
            f.effect_taint(idx(&wf, "t")).is_none(),
            "the MAIN effect is clean"
        );
        let (trace, sink, _cleanup_idx) = f
            .finally_effect_taint(idx(&wf, "t"))
            .expect("cleanup taint caught");
        assert_eq!(sink, "exec");
        assert!(
            trace.render().contains("on_finally"),
            "trace marks the cleanup hop: {}",
            trace.render()
        );
    }

    #[test]
    fn taint_propagates_transitively_through_task_output() {
        // a leaks secret into its exec → a.output tainted → b consuming
        // a.output into ITS exec is also tainted (transitive, via the DAG).
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: a\n    exec: {{ command: \"echo ${{{{ secrets.api_key }}}}\" }}\n  - id: b\n    depends_on: [a]\n    exec: {{ command: \"echo ${{{{ tasks.a.output }}}}\" }}\n"
        );
        let (wf, f) = facts(&y);
        let tb = f.effect_taint(idx(&wf, "b")).expect("transitive taint");
        assert_eq!(tb.secret, "api_key");
        assert!(
            tb.render().contains("tasks.a.output"),
            "trace: {}",
            tb.render()
        );
    }

    #[test]
    fn infer_prompt_is_a_sink_but_does_not_taint_output() {
        // BUG#3: a secret in an infer prompt IS a sink (it leaves the run to
        // a third-party provider) — but the model's RESPONSE is NOT tainted
        // (not a verbatim echo · the OUTPUT carve-out is preserved · ADR-092).
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: a\n    infer: {{ prompt: \"use ${{{{ secrets.api_key }}}}\", max_tokens: 10 }}\n  - id: b\n    depends_on: [a]\n    exec: {{ command: \"echo ${{{{ tasks.a.output }}}}\" }}\n"
        );
        let (wf, f) = facts(&y);
        // The prompt is now a sink (an unsanctioned provider egress · leak).
        assert!(
            f.effect_leak(idx(&wf, "a")).is_some(),
            "infer prompt is a provider-egress sink (no egress → leak)"
        );
        // …but task a's OUTPUT is NOT tainted — so b consuming a.output is clean.
        assert!(
            f.effect_taint(idx(&wf, "b")).is_none(),
            "infer output not tainted → downstream stays clean"
        );
    }

    #[test]
    fn sanctioned_infer_prompt_is_not_a_leak() {
        // BUG#3: an explicit `to: "infer"` egress sanctions the prompt send.
        let y = "\
nika: v1
workflow: w
secrets:
  api_key:
    source: vault
    key: x
    egress:
      - to: \"infer\"
tasks:
  - id: a
    infer: { prompt: \"use ${{ secrets.api_key }}\", max_tokens: 10 }
";
        let (wf, f) = facts(y);
        assert!(
            f.effect_leak(idx(&wf, "a")).is_none(),
            "a sanctioned `to: infer` egress clears the prompt send"
        );
    }

    #[test]
    fn secret_reaching_outputs_is_an_egress() {
        // a secret leaving the run as the return value — the literal exfil.
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: a\n    exec: {{ command: \"echo ${{{{ secrets.api_key }}}}\" }}\noutputs:\n  leaked: ${{{{ tasks.a.output }}}}\n"
        );
        let (_wf, f) = facts(&y);
        let eg = f.egresses();
        assert_eq!(eg.len(), 1);
        assert_eq!(eg[0].0, "leaked");
        assert_eq!(eg[0].1.secret, "api_key");
    }

    #[test]
    fn for_each_item_from_tainted_source_taints_the_body() {
        // for_each over a tainted upstream output → item is tainted → an
        // exec using ${{ item }} re-emits it.
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: a\n    exec: {{ command: \"echo ${{{{ secrets.api_key }}}}\" }}\n  - id: b\n    depends_on: [a]\n    for_each: ${{{{ tasks.a.output }}}}\n    exec: {{ command: \"echo ${{{{ item }}}}\" }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(
            f.effect_taint(idx(&wf, "b")).is_some(),
            "item taint reaches the effect"
        );
    }

    #[test]
    fn no_secrets_declared_is_empty() {
        let y = "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"echo hi\" }\n";
        let (_wf, f) = facts(y);
        assert!(f.effect_taint(0).is_none());
        assert!(f.egresses().is_empty());
    }

    #[test]
    fn unsanctioned_secret_into_a_provider_call_is_a_leak() {
        // BUG#3: an UNSANCTIONED secret into an infer prompt is a leak (it
        // leaves the run to a third-party provider). The legitimate pattern
        // requires an explicit `to: "infer"` egress (see
        // `sanctioned_infer_prompt_is_not_a_leak`).
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: t\n    infer: {{ prompt: \"hi ${{{{ secrets.api_key }}}}\", max_tokens: 5 }}\n"
        );
        let (wf, f) = facts(&y);
        let leak = f
            .effect_leak(idx(&wf, "t"))
            .expect("unsanctioned provider send is a leak");
        assert_eq!(leak.secret, "api_key");
        // No workflow `outputs:` egress (the prompt sink is the leak, not
        // a return-value exfiltration).
        assert!(f.egresses().is_empty());
    }

    #[test]
    fn agent_prompt_is_a_sink_too() {
        // BUG#3: the agent verb's prompt is the same provider-egress sink.
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: t\n    agent: {{ prompt: \"do ${{{{ secrets.api_key }}}}\", max_turns: 2 }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(
            f.effect_leak(idx(&wf, "t")).is_some(),
            "agent prompt is a provider-egress sink"
        );
    }

    #[test]
    fn secret_in_infer_system_field_is_a_sink() {
        // The `system:` field is part of the prompt egress surface too.
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: t\n    infer: {{ prompt: \"go\", system: \"key ${{{{ secrets.api_key }}}}\", max_tokens: 5 }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(
            f.effect_leak(idx(&wf, "t")).is_some(),
            "a secret in system: is a provider-egress sink"
        );
    }
}
