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
use crate::types::OutputDecl;

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
    /// Task index → the taint reaching its EFFECT (`None` = clean). The
    /// leak sink: an `exec`/`invoke` with `Some` re-emits a secret.
    effect_taint: BTreeMap<usize, TaintTrace>,
    /// Task index → the taint of its OUTPUT (`tasks.<id>.output`).
    output_taint: BTreeMap<usize, TaintTrace>,
    /// Workflow `outputs:` entry name → the taint reaching it (egress).
    egress: BTreeMap<String, TaintTrace>,
}

impl FlowFacts {
    /// The taint reaching `task`'s effect, if any (the leak sink).
    #[must_use]
    pub fn effect_taint(&self, task: usize) -> Option<&TaintTrace> {
        self.effect_taint.get(&task)
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
            propagate_task(idx, task, &declared, &id_of, &mut facts);
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
    let effect = effect_text_fields(task).into_iter().find_map(|text| {
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
        facts.effect_taint.insert(idx, trace.clone());
    }

    // 4. Output taint: an exec/invoke whose effect saw taint can embed it in
    //    the captured output. infer/agent are provider-bound (no output taint).
    let taints_output = matches!(task.action, RawAction::Exec(_) | RawAction::Invoke(_));
    if taints_output && let Some(trace) = &effect {
        facts
            .output_taint
            .insert(idx, trace.via(format!("tasks.{}.output", task.id.value)));
    }
}

/// The text fields of a task's EFFECT (the sink surface). `infer`/`agent`
/// prompts are EXCLUDED — a secret in a prompt is provider-bound by design
/// (ADR-092 carve-out · not a leak, not a taint source for the output).
fn effect_text_fields(task: &RawTask) -> Vec<&str> {
    match &task.action {
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
        // provider-bound by design — not an observable re-emitting sink
        RawAction::Infer(_) | RawAction::Agent(_) => Vec::new(),
    }
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
    fn infer_prompt_secret_does_not_taint_output() {
        // Provider-bound by design: a secret in an infer prompt is not a leak
        // and does NOT taint the model's response (ADR-092 carve-out).
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: a\n    infer: {{ prompt: \"use ${{{{ secrets.api_key }}}}\", max_tokens: 10 }}\n  - id: b\n    depends_on: [a]\n    exec: {{ command: \"echo ${{{{ tasks.a.output }}}}\" }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(
            f.effect_taint(idx(&wf, "a")).is_none(),
            "infer effect is not a sink"
        );
        assert!(
            f.effect_taint(idx(&wf, "b")).is_none(),
            "infer output not tainted"
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
    fn clean_secret_use_in_a_provider_call_is_not_flagged() {
        // The legitimate pattern: a secret into an infer model call only.
        let y = format!(
            "nika: v1\nworkflow: w\n{S}tasks:\n  - id: t\n    infer: {{ prompt: \"hi ${{{{ secrets.api_key }}}}\", max_tokens: 5 }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(f.effect_taint(idx(&wf, "t")).is_none());
        assert!(f.egresses().is_empty());
    }
}
