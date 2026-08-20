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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use nika_schema::types::{EgressRule, OutputDecl, Permits};

use super::declass;

type Taints = BTreeMap<String, TaintTrace>;
type WithTaints<'a> = BTreeMap<&'a str, Taints>;

/// One link in a [`TaintTrace`] hop chain — a structurally-shared cons-list.
/// `via` prepends a link in O(1) (an `Arc` bump of the shared tail), never a
/// copy, so carrying a length-k trace through k propagation hops stays O(k)
/// total instead of O(k²). Materialized to a string only when a diagnostic
/// renders a real leak (rare) — never on the propagation hot path.
#[derive(Debug, PartialEq, Eq)]
struct Hop {
    label: Arc<str>,
    /// The earlier hop (one step closer to the source); `None` at the root.
    prev: Option<Arc<Hop>>,
}

/// How a value became `Secret` — the chain from the originating secret to
/// the current slot, for auditable diagnostics (« via `with.tok` ← via
/// `tasks.a.output` ← `secrets.api_key` »).
///
/// The hop chain is an `Arc` cons-list (the private `Hop`): extending or cloning a
/// trace is O(1), so the IFC pass can carry one trace per tainted slot across
/// up to `MAX_TASKS` waves without the O(n²) memory + copy blow-up a
/// per-trace `Vec<String>` caused (a 0.89 MiB workflow → 5.2 GB · 2026-06-16
/// Gate-11 finding). The reachable hops are unchanged; only the storage is.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TaintTrace {
    /// The originating `secrets.<name>`.
    pub secret: String,
    /// The most-recent hop; walk `prev` to reach the source.
    head: Arc<Hop>,
}

impl TaintTrace {
    /// A fresh trace rooted at a secret reference.
    fn source(secret: &str) -> Self {
        Self {
            secret: secret.to_owned(),
            head: Arc::new(Hop {
                label: format!("secrets.{secret}").into(),
                prev: None,
            }),
        }
    }

    /// Extend the chain by one hop, preserving the origin — O(1): the existing
    /// chain is shared by `Arc`, not copied.
    fn via(&self, hop: String) -> Self {
        Self {
            secret: self.secret.clone(),
            head: Arc::new(Hop {
                label: hop.into(),
                prev: Some(Arc::clone(&self.head)),
            }),
        }
    }

    /// The chain rendered for a diagnostic (`secrets.x → with.t → ...`),
    /// source-first. Walks the shared cons-list — O(depth), only on a leak.
    #[must_use]
    pub fn render(&self) -> String {
        let mut labels: Vec<&str> = Vec::new();
        let mut cur = Some(&self.head);
        while let Some(node) = cur {
            labels.push(&node.label);
            cur = node.prev.as_ref();
        }
        labels.reverse();
        labels.join(" → ")
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
    /// Task index → every UNSANCTIONED secret edge reaching that effect.
    /// Kept separate from the singular propagation fact: one captured
    /// output still carries the historical first trace, while the sink
    /// judge must assess every secret independently.
    effect_leaks: BTreeMap<usize, Taints>,
    /// Task index → the taint of its OUTPUT (`tasks.<id>.output`).
    output_taint: BTreeMap<usize, TaintTrace>,
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

    /// The first UNSANCTIONED effect leak of `task`, if any. Preserves the
    /// historical first propagation trace when that edge is uncleared;
    /// otherwise returns the first independently judged uncleared edge.
    #[must_use]
    pub fn effect_leak(&self, task: usize) -> Option<&TaintTrace> {
        let leaks = self.effect_leaks.get(&task)?;
        self.effect_taint
            .get(&task)
            .filter(|trace| leaks.contains_key(trace.secret.as_str()))
            .or_else(|| leaks.values().next())
    }

    /// Every independently judged UNSANCTIONED edge at `task`'s effect.
    pub(crate) fn effect_leaks(&self, task: usize) -> impl Iterator<Item = &TaintTrace> {
        self.effect_leaks
            .get(&task)
            .into_iter()
            .flat_map(BTreeMap::values)
    }

    /// The taint reaching one of `task`'s `on_finally` cleanup effects,
    /// with the sink kind (`exec`/`invoke`) and the cleanup's index within
    /// `on_finally` (the declassification check reads that exact action).
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
    let declared: BTreeSet<&str> = wf.secrets.iter().map(|(n, _)| n.value.as_str()).collect();
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

    // …then the UNWIND tasks, which carry effects but never enter wave
    // assignment (spec 03 · E_f never schedules). The waves are this
    // pass's ORDERING device, not its census: a task missing from them
    // is a task no judge sees, and an unseen effect carrier is exactly
    // the hole `unwind` was minted to close. They run after their
    // producer settles, so appending them here IS their order.
    //
    // This replaces the old `on_finally` special case: the compensation
    // is gone because the units are ordinary tasks now, but the ORDERING
    // still has to name them.
    let seen: BTreeSet<usize> = waves.iter().flatten().copied().collect();
    for idx in 0..wf.tasks.len() {
        if seen.contains(&idx) {
            continue;
        }
        let task = &wf.tasks[idx].value;
        propagate_task(
            idx, task, &declared, &egress_of, permits, &id_of, &mut facts,
        );
    }

    // Egress: a workflow `outputs:` entry referencing a tainted slot —
    // UNLESS the owning secret declassifies the workflow boundary itself
    // (`egress: [{ to: "outputs" }]` · spec 01-envelope §egress · the DLM
    // owner's act, same as every sink; absent = default-deny, the report
    // stands). Sink-only and secret-SPECIFIC: it clears nothing but THIS
    // secret's taints reaching `outputs:` — never a send.
    for (name, decl) in &wf.outputs {
        if let Some(trace) = taint_of_refs(
            refs_in_str(decl_value(decl)),
            &declared,
            None,
            &id_of,
            &facts,
        ) {
            let egress = egress_of.get(trace.secret.as_str()).copied().unwrap_or(&[]);
            if egress.iter().any(|rule| rule.to == "outputs") {
                continue;
            }
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
    declared: &BTreeSet<&str>,
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
        nika_schema::raw::ForEachValue::Expression(src) => {
            taint_of_refs(refs_in_str(src), declared, Some(&with_taint), id_of, facts)
                .map(|t| t.via(format!("item @ {}", task.id.value)))
        }
        nika_schema::raw::ForEachValue::List(_) => None, // literal list = no taint
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown for_each form: {other:?}"),
    });
    let (with_taints, item_taints) = task_local_taints(task, declared, id_of, facts);

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
    let effect_taints = all_taints_of_refs(
        action_effect_fields(&task.action)
            .into_iter()
            .flat_map(refs_in_str)
            .collect(),
        declared,
        &with_taints,
        Some(&item_taints),
        id_of,
        facts,
    );
    if let Some(trace) = &effect {
        // Propagation fact (always · feeds output taint below).
        facts.effect_taint.insert(idx, trace.clone());
    }
    record_effect_leaks(idx, task, effect_taints, egress_of, permits, facts);

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
}

fn task_local_taints<'a>(
    task: &'a RawTask,
    declared: &BTreeSet<&str>,
    id_of: &BTreeMap<&str, usize>,
    facts: &FlowFacts,
) -> (WithTaints<'a>, Taints) {
    let mut with_taints = WithTaints::new();
    for (key, value) in &task.with {
        let taints = all_taints_of_refs(
            refs_in_json(&value.value),
            declared,
            &WithTaints::new(),
            None,
            id_of,
            facts,
        )
        .into_iter()
        .map(|(secret, trace)| {
            (
                secret,
                trace.via(format!("with.{} @ {}", key.value, task.id.value)),
            )
        })
        .collect();
        with_taints.insert(key.value.as_str(), taints);
    }
    let item_refs = task.for_each.as_ref().map(|f| match &f.value {
        nika_schema::raw::ForEachValue::Expression(src) => refs_in_str(src),
        nika_schema::raw::ForEachValue::List(list) => refs_in_json(list),
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown for_each form: {other:?}"),
    });
    let item_taints = item_refs
        .map(|refs| all_taints_of_refs(refs, declared, &with_taints, None, id_of, facts))
        .unwrap_or_default()
        .into_iter()
        .map(|(secret, trace)| (secret, trace.via(format!("item @ {}", task.id.value))))
        .collect();
    (with_taints, item_taints)
}

fn record_effect_leaks(
    idx: usize,
    task: &RawTask,
    effect_taints: Taints,
    egress_of: &BTreeMap<&str, &[EgressRule]>,
    permits: Option<&Permits>,
    facts: &mut FlowFacts,
) {
    let leaks: Taints = effect_taints
        .into_iter()
        .filter(|(_, trace)| {
            let egress = egress_of.get(trace.secret.as_str()).copied().unwrap_or(&[]);
            !declass::is_sanctioned(&trace.secret, egress, &task.action, permits)
        })
        .collect();
    if !leaks.is_empty() {
        facts.effect_leaks.insert(idx, leaks);
    }
}

/// Record the FIRST UNSANCTIONED `on_finally` cleanup leak of a task. A
/// cleanup `exec`/`invoke` that sees a secret re-emits it exactly like a
/// The text fields of an action's EFFECT (the sink surface) — works for a
/// task's main verb AND an `on_finally` cleanup verb.
///
/// The `infer:`/`agent:` `prompt:` + `system:` ARE sinks: a secret in a
/// prompt LEAVES the run to a third-party provider (the same third-party-
/// network exposure as an `mcp:` tool, which is already flagged). A secret
/// reaching one with no `egress:` sanction is a leak (BUG#3 · now CANONICAL ·
/// spec `01-envelope.md` §egress aligned to this strict behavior · F-03).
/// The OUTPUT carve-out is SEPARATE and PRESERVED (see `propagate_task`
/// step 4 · the model response is not a verbatim echo, so infer/agent OUTPUT
/// is never tainted from a prompt secret · ADR-092). To send a secret to a
/// provider, the author sanctions it explicitly (`egress: [{ to: "infer" }]`
/// / `{ to: "agent" }`).
///
/// `pub` (F-O1 PR-1): `nika-runtime`'s integrity walk reads the SAME
/// effect-surface table — check≡run by construction, never a duplicated
/// field list.
#[must_use]
pub fn action_effect_fields(action: &RawAction) -> Vec<&str> {
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
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
}

/// The `prompt:` + optional `system:` text of an `infer:`/`agent:` action —
/// the provider-egress sink surface (BUG#3).
pub(crate) fn prompt_system_fields<'a>(
    prompt: &'a str,
    system: Option<&'a nika_schema::Spanned<String>>,
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
    declared: &BTreeSet<&str>,
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
    declared: &BTreeSet<&str>,
    with_taint: &BTreeMap<&str, TaintTrace>,
    item_taint: Option<&TaintTrace>,
    id_of: &BTreeMap<&str, usize>,
    facts: &FlowFacts,
) -> Option<TaintTrace> {
    for r in refs {
        match r {
            NamespaceRef::Secrets(name) if declared.contains(name.as_str()) => {
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

/// Resolve every distinct secret reaching one effect-local surface.
///
/// Upstream outputs deliberately retain the singular propagation fact;
/// this function widens only direct refs and task-local `with`/`item`
/// aliases. Repeated aliases are merged once, bounding work by authored
/// references plus the distinct secret set instead of their product.
fn all_taints_of_refs(
    refs: Vec<NamespaceRef>,
    declared: &BTreeSet<&str>,
    with_taints: &WithTaints<'_>,
    item_taints: Option<&Taints>,
    id_of: &BTreeMap<&str, usize>,
    facts: &FlowFacts,
) -> Taints {
    let mut out = BTreeMap::new();
    let mut seen_with = BTreeSet::new();
    let mut seen_tasks = BTreeSet::new();
    let mut saw_item = false;
    for r in refs {
        match r {
            NamespaceRef::Secrets(name) if declared.contains(name.as_str()) => {
                out.entry(name.clone())
                    .or_insert_with(|| TaintTrace::source(&name));
            }
            NamespaceRef::With(key) if seen_with.insert(key.clone()) => {
                if let Some(taints) = with_taints.get(key.as_str()) {
                    extend_first(&mut out, taints.values().cloned());
                }
            }
            NamespaceRef::Item if !saw_item => {
                saw_item = true;
                if let Some(taints) = item_taints {
                    extend_first(&mut out, taints.values().cloned());
                }
            }
            NamespaceRef::Tasks { id, .. } if seen_tasks.insert(id.clone()) => {
                if let Some(&upstream) = id_of.get(id.as_str())
                    && let Some(trace) = facts.output_taint.get(&upstream)
                {
                    out.entry(trace.secret.clone())
                        .or_insert_with(|| trace.clone());
                }
            }
            _ => {}
        }
    }
    out
}

fn extend_first(target: &mut Taints, traces: impl Iterator<Item = TaintTrace>) {
    for trace in traces {
        target.entry(trace.secret.clone()).or_insert(trace);
    }
}

/// The `${{ … }}` references inside a string (via the real extractor — the
/// same path the analyzer uses, so taint and `NIKA-VAR-001` agree).
pub(crate) fn refs_in_str(text: &str) -> Vec<NamespaceRef> {
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
pub(crate) fn collect_json_strings(value: &serde_json::Value) -> Vec<&str> {
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
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

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
            "nika: w\n{S}tasks:\n  t:\n    exec: {{ shell: \"curl -H ${{{{ secrets.api_key }}}}\" }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(f.effect_taint(idx(&wf, "t")).is_some());
    }

    #[test]
    fn plural_sink_judgment_preserves_the_historical_first_trace() {
        let (wf, f) = facts(
            r#"
nika: first-trace
secrets:
  token: { source: env, key: TOKEN }
permits:
  exec: [printf]
tasks:
  send:
    with: { alias: "${{ secrets.token }}" }
    exec:
      command: [printf, "${{ secrets.token }}"]
      stdin: "${{ with.alias }}"
"#,
        );
        let task = idx(&wf, "send");
        assert_eq!(
            f.effect_taint(task).expect("taint").render(),
            "secrets.token"
        );
        assert_eq!(f.effect_leak(task).expect("leak").render(), "secrets.token");
        assert_eq!(f.effect_leaks(task).count(), 1);
    }

    #[test]
    fn first_leak_accessor_falls_through_a_sanctioned_first_edge() {
        let (wf, f) = facts(
            r#"
nika: first-cleared
secrets:
  alpha:
    source: env
    key: ALPHA
    egress: [{ to: exec }]
  omega: { source: env, key: OMEGA }
permits:
  exec: [printf]
tasks:
  send:
    exec:
      command: [printf, "${{ secrets.alpha }}:${{ secrets.omega }}"]
"#,
        );
        let task = idx(&wf, "send");
        assert_eq!(f.effect_taint(task).expect("taint").secret, "alpha");
        assert_eq!(f.effect_leak(task).expect("uncleared edge").secret, "omega");
    }

    #[test]
    fn long_secret_chain_stays_linear_not_quadratic() {
        // DoS regression — Gate-11 finding 2026-06-16. A length-N secret-taint
        // chain (each task reads the prior task's tainted output) gives task k
        // a length-k trace. With the old `Vec<String>` hops cloned 2-3× per
        // task that was O(N²) time + memory — a 0.89 MiB workflow OOM'd at
        // 5.2 GB. The Arc cons-list makes via()/clone() O(1), so this chain
        // settles in milliseconds. The assertion is correctness (the secret
        // propagates the whole way + the trace stays intact); the implicit
        // guard is completion — a regression to O(N²) makes this test crawl.
        use std::fmt::Write as _;
        const N: usize = 3_000;
        let mut y = format!("nika: w\n{S}tasks:\n");
        y.push_str("  t0:\n    exec: { command: [\"echo\", \"${{ secrets.api_key }}\"] }\n");
        for k in 1..N {
            let p = k - 1;
            write!(
                y,
                "  t{k}:\n    with: {{ prev: \"${{{{ tasks.t{p}.output }}}}\" }}\n    exec: {{ shell: \"echo ${{{{ with.prev }}}}\" }}\n"
            )
            .expect("write to String is infallible");
        }
        let (wf, f) = facts(&y);
        let tail = f
            .effect_taint(idx(&wf, &format!("t{}", N - 1)))
            .expect("the secret must propagate through the whole chain");
        assert_eq!(tail.secret, "api_key");
        let rendered = tail.render();
        assert!(
            rendered.starts_with("secrets.api_key"),
            "trace must be source-first: {rendered:.40}"
        );
        assert!(
            rendered.contains("tasks.t0.output"),
            "the first propagation hop must be in the intact trace"
        );
    }

    #[test]
    fn with_aliased_secret_is_traced_to_the_effect() {
        // The false negative the review found: with: { tok: secret } then
        // ${{ with.tok }} into exec — one hop the old substring scan missed.
        let y = format!(
            "nika: w\n{S}tasks:\n  t:\n    with: {{ tok: \"${{{{ secrets.api_key }}}}\" }}\n    exec: {{ shell: \"curl -H ${{{{ with.tok }}}}\" }}\n"
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
            "nika: w\n{S}tasks:\n  a:\n    exec: {{ shell: \"echo ${{{{ secrets.api_key }}}}\" }}\n  b:\n    with: {{ upstream: \"${{{{ tasks.a.output }}}}\" }}\n    exec: {{ shell: \"echo ${{{{ with.upstream }}}}\" }}\n"
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
            "nika: w\n{S}tasks:\n  a:\n    infer: {{ prompt: \"use ${{{{ secrets.api_key }}}}\", max_tokens: 10 }}\n  b:\n    with: {{ upstream: \"${{{{ tasks.a.output }}}}\" }}\n    exec: {{ shell: \"echo ${{{{ with.upstream }}}}\" }}\n"
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
    fn outputs_egress_declass_clears_the_boundary_report() {
        // The api-upload class (night battery 2026-07-10): the capture
        // stays tainted, outputs is where it LEAVES — `to: "outputs"` is
        // the owner's declassification of the workflow boundary itself.
        let y = "\
nika: w
secrets:
  api_key:
    source: vault
    key: x
    egress:
      - to: \"nika:fetch\"
      - to: \"outputs\"
tasks:
  up:
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://api.example.com/u\", headers: { x-api-key: \"${{ secrets.api_key }}\" } }
outputs:
  result: \"${{ tasks.up.output }}\"
";
        let (_wf, f) = facts(y);
        assert!(
            f.egresses().is_empty(),
            "the declared boundary declass clears the outputs report"
        );
    }

    #[test]
    fn outputs_egress_stays_default_deny_without_the_rule() {
        // Same workflow, no `to: "outputs"` — the report STANDS.
        let y = "\
nika: w
secrets:
  api_key:
    source: vault
    key: x
    egress:
      - to: \"nika:fetch\"
tasks:
  up:
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://api.example.com/u\", headers: { x-api-key: \"${{ secrets.api_key }}\" } }
outputs:
  result: \"${{ tasks.up.output }}\"
";
        let (_wf, f) = facts(y);
        assert_eq!(f.egresses().len(), 1, "default-deny · the report stands");
    }

    #[test]
    fn outputs_egress_never_authorizes_the_send() {
        // `to: "outputs"` ALONE: the boundary is cleared but the SEND to
        // nika:fetch is still an unsanctioned leak (no cross-sink grant).
        let y = "\
nika: w
secrets:
  api_key:
    source: vault
    key: x
    egress:
      - to: \"outputs\"
tasks:
  up:
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://api.example.com/u\", headers: { x-api-key: \"${{ secrets.api_key }}\" } }
outputs:
  result: \"${{ tasks.up.output }}\"
";
        let (wf, f) = facts(y);
        assert!(
            f.effect_leak(idx(&wf, "up")).is_some(),
            "the send stays a leak — outputs clears only the boundary"
        );
        assert!(f.egresses().is_empty(), "the boundary itself is cleared");
    }

    #[test]
    fn sanctioned_infer_prompt_is_not_a_leak() {
        // BUG#3: an explicit `to: "infer"` egress sanctions the prompt send.
        let y = "\
nika: w
secrets:
  api_key:
    source: vault
    key: x
    egress:
      - to: \"infer\"
tasks:
  a:
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
            "nika: w\n{S}tasks:\n  a:\n    exec: {{ shell: \"echo ${{{{ secrets.api_key }}}}\" }}\noutputs:\n  leaked: ${{{{ tasks.a.output }}}}\n"
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
            "nika: w\n{S}tasks:\n  a:\n    exec: {{ shell: \"echo ${{{{ secrets.api_key }}}}\" }}\n  b:\n    with: {{ items: \"${{{{ tasks.a.output }}}}\" }}\n    for_each: {{ items: \"${{{{ with.items }}}}\" }}\n    exec: {{ shell: \"echo ${{{{ item }}}}\" }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(
            f.effect_taint(idx(&wf, "b")).is_some(),
            "item taint reaches the effect"
        );
    }

    #[test]
    fn no_secrets_declared_is_empty() {
        let y = "nika: w\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n";
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
            "nika: w\n{S}tasks:\n  t:\n    infer: {{ prompt: \"hi ${{{{ secrets.api_key }}}}\", max_tokens: 5 }}\n"
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
            "nika: w\n{S}tasks:\n  t:\n    agent: {{ prompt: \"do ${{{{ secrets.api_key }}}}\", max_turns: 2 }}\n"
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
            "nika: w\n{S}tasks:\n  t:\n    infer: {{ prompt: \"go\", system: \"key ${{{{ secrets.api_key }}}}\", max_tokens: 5 }}\n"
        );
        let (wf, f) = facts(&y);
        assert!(
            f.effect_leak(idx(&wf, "t")).is_some(),
            "a secret in system: is a provider-egress sink"
        );
    }

    #[test]
    fn secret_nested_in_invoke_args_array_is_a_sink() {
        // Kills `collect_json_strings_into` 470 — delete the
        // `Value::Array(items)` arm. The invoke args carry the secret leaf
        // inside a JSON ARRAY (`items: ["${{ secrets.api_key }}"]`). The
        // string collector MUST recurse into the array to surface the leaf →
        // the effect sees `secrets.api_key` → tainted. Deleting the Array arm
        // drops the array's strings, so the secret is invisible and the
        // effect wrongly reads clean.
        let y = format!(
            "nika: w\n{S}tasks:\n  t:\n    invoke: {{ tool: \"mcp:srv/run\", args: {{ items: [\"${{{{ secrets.api_key }}}}\"] }} }}\n"
        );
        let (wf, f) = facts(&y);
        let trace = f
            .effect_taint(idx(&wf, "t"))
            .expect("secret nested in the invoke args ARRAY must be seen by the effect");
        assert_eq!(trace.secret, "api_key");
    }

    #[test]
    fn undeclared_secret_ref_is_not_tainted() {
        // Kills `taint_of_refs_full` 417 — forcing the guard
        // `declared.contains(&name.as_str())` → `true`. The resolver is
        // handed a `secrets.unknown` ref while `declared` lists only
        // `api_key`; the guard must REJECT the undeclared name and the
        // resolver returns `None` (no taint). The forced-true mutant would
        // treat ANY `secrets.<name>` as a live secret and return
        // `Some(source("unknown"))`. (Driven directly because the public
        // `analyze` path rejects an undeclared `secrets.X` ref before the IFC
        // pass ever runs, so the guard cannot be reached end-to-end.)
        let declared: BTreeSet<&str> = BTreeSet::from(["api_key"]);
        let no_facts = FlowFacts::default();
        let id_of: BTreeMap<&str, usize> = BTreeMap::new();
        let with_taint: BTreeMap<&str, TaintTrace> = BTreeMap::new();

        // Undeclared name → the guard fails → no taint.
        let undeclared = vec![NamespaceRef::Secrets("unknown".to_owned())];
        assert_eq!(
            taint_of_refs_full(undeclared, &declared, &with_taint, None, &id_of, &no_facts),
            None,
            "an undeclared secret name must NOT be classified as a live secret"
        );

        // Sanity anchor · the SAME resolver with the DECLARED name DOES taint,
        // proving the test exercises the live guard (not a dead path).
        let declared_ref = vec![NamespaceRef::Secrets("api_key".to_owned())];
        let got = taint_of_refs_full(
            declared_ref,
            &declared,
            &with_taint,
            None,
            &id_of,
            &no_facts,
        )
        .expect("a declared secret name is tainted");
        assert_eq!(got.secret, "api_key");
    }
}
