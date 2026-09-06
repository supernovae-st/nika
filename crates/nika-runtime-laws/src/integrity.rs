// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The F-O1 runtime integrity walk (PR-1 · additive — no gate consumes
//! the label yet). Computes one task's coarse [`Integrity`] from its
//! STATIC reference surface + the already-settled upstream records, and
//! the settle spine stamps it on the record.
//!
//! The algorithm MIRRORS `nika_check`'s content-taint pass
//! (`content_flow.rs`) deliberately — the same extractor
//! (`nika_schema::expression::{scan_templates, expr_refs}`), the same
//! effect-surface table ([`nika_check::action_effect_fields`]), the same
//! born-source predicates ([`nika_cap::tool_grant_admits_ingress`] ·
//! [`nika_cap::invoke_tool_is_ingress`]) — so the runtime label and the
//! static witness CANNOT drift (check≡run by construction, ENGINE.md
//! step 1). Differences from the check, all declared:
//!
//! - it reads the LIVE records (the run's own truth), not a projected
//!   index — same least-fixpoint shape, since a task's refs only ever
//!   point at earlier waves (checker law · the wave-frozen view);
//! - `${{ inputs.X }}` reads are untrusted (the caller boundary — the
//!   mission's v1 law; the static twin lands in PR-3);
//! - NO file channel (a tainted writer → an exec reader under `fs.read`
//!   is the declared v1 residual, ENGINE.md risk (d));
//! - `mcp:*` outputs are untrusted WITHOUT consulting the catalog's
//!   `content_trust` mark (fail-closed — the check's absent-mark
//!   default; a catalog-trusted server is over-tainted at runtime, the
//!   safe direction).
//!
//! `infer:`/`agent:` are NOT born sources: their outputs carry the join
//! of what their prompt sees (RS-06 · « un LLM n'élève jamais
//! l'intégrité ») — which falls out of the effect-field walk, since the
//! prompt+system ARE effect fields of those verbs.
//!
//! Why a settle-time static walk and not ENGINE.md's step-4 render
//! out-param: identical precision (the extractor is static on both
//! sides — a dynamic CEL path is out of the subset for BOTH), zero
//! `Scope`/`render` call-site churn, and parity by import instead of
//! parity by convention. PR-2 consumes the SAME walk per template
//! through [`ValueTaint`] — the dispatch re-gate's per-element oracle
//! (an argv element · an invoke arg leaf · the exec `cwd`).

use std::collections::BTreeMap;

use nika_cap::Integrity;
use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
use nika_schema::raw::{ForEachValue, RawAction, RawTask};
use nika_schema::types::OnErrorAction;
use serde_json::Value;

use crate::record::TaskRecord;

/// One task's coarse output integrity: born-source first (the witness is
/// the task's own id), else the join of every taint its effect surface
/// reads, else its `on_error: recover` reads (the content-flow
/// priority — the witness named is the earliest taint origin).
///
/// `records` is the wave-frozen view (same-wave tasks never reference
/// each other — checker law), so every `tasks.X` read resolves against
/// a FINAL label.
pub fn task_integrity(task: &RawTask, records: &BTreeMap<String, TaskRecord>) -> Integrity {
    let taint = ValueTaint::of_task(task, records);

    // Effect taint: the verb's effect-carrying fields (exec argv/shell
    //    · invoke args · infer/agent prompt+system — the infer/agent
    //    prompt-taint inversion falls out of THIS walk).
    let effect = join_refs(
        &nika_check::action_effect_fields(&task.action)
            .into_iter()
            .flat_map(refs_in_str)
            .collect::<Vec<_>>(),
        &taint.with_taint,
        taint.item_taint.as_ref(),
        records,
        &taint.declassified,
    );

    // Born-source wins the witness; else the effect flows out; else the
    //    recover reads (the content-flow output priority).
    if born_untrusted(&task.action) {
        return Integrity::untrusted(task.id.value.clone());
    }
    if effect.is_untrusted() {
        return effect;
    }
    recover_taint(
        task,
        &taint.with_taint,
        taint.item_taint.as_ref(),
        records,
        &taint.declassified,
    )
}

/// The per-template taint oracle (F-O1 PR-2 · the dispatch re-gate's
/// lookup). Precomputes the task-local taints ONCE per task — the
/// progressive `with:` walk and the `for_each` item taint, exactly the
/// steps [`task_integrity`] opens with, so a per-element verdict and the
/// task's own label can never drift — then labels ANY template string
/// the dispatch is about to render (an argv element · an invoke arg
/// leaf · the exec `cwd`) against the wave-frozen records.
///
/// The oracle is iteration-agnostic: a fan-out's `with:` is re-rendered
/// per item but its TAINT is a static function of the templates (an
/// `item` read joins the collection's taint, not the item's value).
///
/// F-O1 PR-3 (NEP-0004 law 5): the task's `declassify:` declarations ride
/// the oracle — a `from:` binding reads TRUSTED here (scoped to THIS
/// task, the only door through the re-gate); the receipt event is the
/// settle spine's.
pub struct ValueTaint<'a> {
    /// `with:` slot taints, progressive (a with-value may reference an
    /// EARLIER with key — declaration order, the content-flow walk).
    with_taint: BTreeMap<&'a str, Integrity>,
    /// `for_each` item taint: the collection's refs taint the loop-local
    /// `item` within the task (a literal list is authored — clean).
    item_taint: Option<Integrity>,
    /// The bindings THIS task declassifies (`declassify.from` verbatim ·
    /// the dotted canonical form `inputs.p` · `tasks.dl.output`).
    declassified: std::collections::BTreeSet<String>,
}

impl<'a> ValueTaint<'a> {
    /// The task-local taints (the first two steps of the integrity
    /// walk), borrowed from the task's own templates.
    #[must_use]
    pub fn of_task(task: &'a RawTask, records: &BTreeMap<String, TaskRecord>) -> Self {
        // the `taint` doors only — `data-as-code` raises no binding
        let declassified: std::collections::BTreeSet<String> = task
            .taint_lifts()
            .filter_map(|entry| entry.from.as_ref())
            .map(|from| from.value.clone())
            .collect();
        let mut with_taint: BTreeMap<&str, Integrity> = BTreeMap::new();
        for (key, value) in &task.with {
            let taint = join_refs(
                &refs_in_json(&value.value),
                &with_taint,
                None,
                records,
                &declassified,
            );
            if taint.is_untrusted() {
                with_taint.insert(key.value.as_str(), taint);
            }
        }
        let item_taint: Option<Integrity> = task.for_each.as_ref().and_then(|f| match &f.value {
            ForEachValue::Expression(src) => {
                let taint = join_refs(&refs_in_str(src), &with_taint, None, records, &declassified);
                taint.is_untrusted().then_some(taint)
            }
            ForEachValue::List(_) => None,
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown for_each form: {other:?}"),
        });
        Self {
            with_taint,
            item_taint,
            declassified,
        }
    }

    /// No task-local taints — the `on_finally:` mini-task shape (no
    /// `with:` · no `for_each` · no `declassify:` — a cleanup never
    /// declares the door); the records/inputs lookups of
    /// [`ValueTaint::label`] still apply.
    #[must_use]
    pub fn bare() -> ValueTaint<'static> {
        ValueTaint {
            with_taint: BTreeMap::new(),
            item_taint: None,
            declassified: std::collections::BTreeSet::new(),
        }
    }

    /// The label of ONE template's rendered value: the join of every
    /// reference the template reads (the SAME `join_refs` + `source_of`
    /// law as the task-level walk — `inputs.X` is the caller boundary,
    /// `tasks.X` reads the settled record's label, a declassified binding
    /// reads trusted HERE).
    #[must_use]
    pub fn label(&self, template: &str, records: &BTreeMap<String, TaskRecord>) -> Integrity {
        join_refs(
            &refs_in_str(template),
            &self.with_taint,
            self.item_taint.as_ref(),
            records,
            &self.declassified,
        )
    }
}

/// Is the task's verb a born untrusted-ingress source (v1 · the shared
/// nika-cap predicates): an invoked `nika:fetch` · any `mcp:*` tool
/// (fail-closed — no catalog consult at runtime) · an `agent:` whose
/// whitelist admits ingress · an `exec:` (F2 · 2026-07-30 · operator
/// decision A: a subprocess is a trust boundary — its stdout is content
/// the workflow did not author, the same class `nika:fetch` is judged
/// for; the check twin is `content_flow::classify`, check≡run by
/// construction). A child `workflow:` call is not (spec 14 owns its
/// boundary — `tool()` is `None`).
fn born_untrusted(action: &RawAction) -> bool {
    match action {
        RawAction::Exec(_) => true,
        RawAction::Invoke(inv) => inv
            .tool()
            .is_some_and(|t| nika_cap::invoke_tool_is_ingress(t.value.as_str(), false)),
        RawAction::Agent(a) => a
            .tools
            .iter()
            .any(|t| nika_cap::tool_grant_admits_ingress(t.value.as_str())),
        _ => false,
    }
}

/// The `on_error: recover` value's taint (recover reads propagate — the
/// recovered output embeds what the template saw).
fn recover_taint(
    task: &RawTask,
    with_taint: &BTreeMap<&str, Integrity>,
    item_taint: Option<&Integrity>,
    records: &BTreeMap<String, TaskRecord>,
    declassified: &std::collections::BTreeSet<String>,
) -> Integrity {
    let Some(on_error) = &task.on_error else {
        return Integrity::trusted();
    };
    let OnErrorAction::Recover(value) = &on_error.value.action else {
        return Integrity::trusted();
    };
    join_refs(
        &refs_in_json(&value.value),
        with_taint,
        item_taint,
        records,
        declassified,
    )
}

/// The join of a ref set's taints — the FIRST untrusted witness sticks
/// (deterministic · the earliest origin a fix must address).
fn join_refs(
    refs: &[NamespaceRef],
    with_taint: &BTreeMap<&str, Integrity>,
    item_taint: Option<&Integrity>,
    records: &BTreeMap<String, TaskRecord>,
    declassified: &std::collections::BTreeSet<String>,
) -> Integrity {
    refs.iter().fold(Integrity::trusted(), |acc, r| {
        acc.join(source_of(r, with_taint, item_taint, records, declassified))
    })
}

/// One reference's taint (the content-flow `source_of`, plus the v1
/// caller-boundary law): `tasks.X` reads the settled record's label ·
/// `with.K` / `item` read the task-local taints · `inputs.X` is the
/// external boundary (Perl-taint slot rule — untrusted whether or not
/// THIS run overrode the default). `config:`/`const:` are the operator's
/// authorities · `secrets:` is the confidentiality axis's business — all
/// trusted on THIS lattice. F-O1 PR-3 (NEP-0004 law 5): a binding the
/// task's `declassify:` names reads TRUSTED here — the only door,
/// scoped to this task (the receipt event is the settle spine's).
fn source_of(
    r: &NamespaceRef,
    with_taint: &BTreeMap<&str, Integrity>,
    item_taint: Option<&Integrity>,
    records: &BTreeMap<String, TaskRecord>,
    declassified: &std::collections::BTreeSet<String>,
) -> Integrity {
    match r {
        NamespaceRef::Tasks { id, .. } => {
            if declassified.contains(&format!("tasks.{id}.output")) {
                return Integrity::trusted();
            }
            records
                .get(id)
                .map_or_else(Integrity::default, |rec| rec.integrity.clone())
        }
        NamespaceRef::With(key) => with_taint.get(key.as_str()).cloned().unwrap_or_default(),
        NamespaceRef::Item => item_taint.cloned().unwrap_or_default(),
        NamespaceRef::Inputs(name) => {
            if declassified.contains(&format!("inputs.{name}")) {
                return Integrity::trusted();
            }
            Integrity::untrusted(format!("inputs.{name}"))
        }
        _ => Integrity::trusted(),
    }
}

/// The `${{ … }}` references inside a string — the real extractor, the
/// SAME path the check's flow pass uses (so the runtime label and
/// `NIKA-VAR-001` agree by construction).
fn refs_in_str(text: &str) -> Vec<NamespaceRef> {
    let Ok(islands) = scan_templates(text) else {
        return Vec::new();
    };
    islands.iter().flat_map(|i| expr_refs(&i.expr)).collect()
}

/// References inside any string within a JSON value (with-values ·
/// recover templates).
fn refs_in_json(value: &Value) -> Vec<NamespaceRef> {
    let mut strings = Vec::new();
    collect_json_strings(value, &mut strings);
    strings.into_iter().flat_map(refs_in_str).collect()
}

/// Every string leaf of a JSON value (the check's `collect_json_strings`
/// shape — trivial enough to own locally; the LAW that must stay shared,
/// the extraction itself, lives in `nika-schema`).
fn collect_json_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(s) => out.push(s.as_str()),
        Value::Array(items) => {
            for item in items {
                collect_json_strings(item, out);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_json_strings(v, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::record::{TaskStatus, TerminalCause};

    fn parse_task(yaml: &str) -> RawTask {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        wf.tasks.into_iter().next().expect("one task").value
    }

    fn settled(id: &str, integrity: Integrity) -> (String, TaskRecord) {
        let mut rec = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
        rec.attempts = Some(1);
        rec.output = Value::String(format!("{id} content"));
        rec.integrity = integrity;
        (id.to_owned(), rec)
    }

    fn records(entries: Vec<(String, TaskRecord)>) -> BTreeMap<String, TaskRecord> {
        entries.into_iter().collect()
    }

    const HEAD: &str = "nika: w\ntasks:\n";

    #[test]
    fn fetch_output_is_born_untrusted_with_its_own_id_as_witness() {
        let task = parse_task(&format!(
            "{HEAD}  dl:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"https://x\" }} }}\n"
        ));
        let label = task_integrity(&task, &records(Vec::new()));
        assert_eq!(label, Integrity::untrusted("dl"));
    }

    #[test]
    fn mcp_output_is_born_untrusted_fail_closed() {
        let task = parse_task(&format!(
            "{HEAD}  page:\n    invoke: {{ tool: \"mcp:browser/open\", args: {{ url: \"x\" }} }}\n"
        ));
        assert_eq!(
            task_integrity(&task, &records(Vec::new())),
            Integrity::untrusted("page")
        );
    }

    #[test]
    fn a_first_party_builtin_is_not_ingress() {
        let task = parse_task(&format!(
            "{HEAD}  r:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"x\" }} }}\n"
        ));
        assert_eq!(
            task_integrity(&task, &records(Vec::new())),
            Integrity::trusted()
        );
    }

    #[test]
    fn an_agent_is_born_untrusted_only_when_its_whitelist_admits_ingress() {
        let browsing = parse_task(&format!(
            "{HEAD}  a:\n    agent: {{ prompt: \"go\", tools: [\"mcp:browser/*\"] }}\n"
        ));
        assert_eq!(
            task_integrity(&browsing, &records(Vec::new())),
            Integrity::untrusted("a")
        );
        let local = parse_task(&format!(
            "{HEAD}  a:\n    agent: {{ prompt: \"go\", tools: [\"nika:read\"] }}\n"
        ));
        assert_eq!(
            task_integrity(&local, &records(Vec::new())),
            Integrity::trusted()
        );
        let negated = parse_task(&format!(
            "{HEAD}  a:\n    agent: {{ prompt: \"go\", tools: [\"!mcp:x\"] }}\n"
        ));
        assert_eq!(
            task_integrity(&negated, &records(Vec::new())),
            Integrity::trusted()
        );
    }

    #[test]
    fn taint_flows_from_an_upstream_output_through_with_into_the_effect() {
        // The consumer is an INFER (never a born source): the output
        // label is the pure propagation story. (It used to ride an exec —
        // since F2 2026-07-30 exec is born-untrusted, and the born
        // witness would mask the propagation this test is about.)
        let task = parse_task(&format!(
            "{HEAD}  use:\n    with: {{ page: \"${{{{ tasks.dl.output }}}}\" }}\n    infer: {{ prompt: \"${{{{ with.page }}}}\", max_tokens: 5 }}\n"
        ));
        let recs = records(vec![settled("dl", Integrity::untrusted("dl"))]);
        assert_eq!(task_integrity(&task, &recs), Integrity::untrusted("dl"));
        let clean = records(vec![settled("dl", Integrity::trusted())]);
        assert_eq!(task_integrity(&task, &clean), Integrity::trusted());
    }

    #[test]
    fn with_values_reference_earlier_with_keys() {
        let task = parse_task(&format!(
            "{HEAD}  use:\n    with:\n      a: \"${{{{ tasks.dl.output }}}}\"\n      b: \"${{{{ with.a }}}}\"\n    infer: {{ prompt: \"${{{{ with.b }}}}\", max_tokens: 5 }}\n"
        ));
        let recs = records(vec![settled("dl", Integrity::untrusted("dl"))]);
        assert_eq!(task_integrity(&task, &recs), Integrity::untrusted("dl"));
    }

    #[test]
    fn inputs_reads_are_the_caller_boundary() {
        let task = parse_task(&format!(
            "{HEAD}  t:\n    infer: {{ prompt: \"${{{{ inputs.q }}}}\", max_tokens: 5 }}\n"
        ));
        assert_eq!(
            task_integrity(&task, &records(Vec::new())),
            Integrity::untrusted("inputs.q")
        );
    }

    #[test]
    fn an_infer_never_launders_taint_but_a_clean_prompt_stays_trusted() {
        // RS-06 · « un LLM n'élève jamais l'intégrité »: the model output
        // carries the join of what its prompt sees — no more, no less.
        let tainted_prompt = parse_task(&format!(
            "{HEAD}  sum:\n    infer: {{ prompt: \"summarize ${{{{ tasks.dl.output }}}}\", max_tokens: 5 }}\n"
        ));
        let recs = records(vec![settled("dl", Integrity::untrusted("dl"))]);
        assert_eq!(
            task_integrity(&tainted_prompt, &recs),
            Integrity::untrusted("dl")
        );
        let clean_prompt = parse_task(&format!(
            "{HEAD}  sum:\n    infer: {{ prompt: \"summarize the plan\", max_tokens: 5 }}\n"
        ));
        assert_eq!(task_integrity(&clean_prompt, &recs), Integrity::trusted());
    }

    #[test]
    fn exec_is_a_born_source() {
        // F2 · 2026-07-30 (operator decision A, run 5): a subprocess is a
        // trust boundary — its stdout is content the workflow did not
        // author, judged exactly like `nika:fetch`'s. The check twin is
        // `content_flow::classify` (check≡run). The file channel (a
        // tainted writer → this reader) stays the declared v1 residual
        // (ENGINE.md risk (d)) — orthogonal to this born law.
        let task = parse_task(&format!(
            "{HEAD}  t:\n    exec: {{ shell: \"cat out.txt\" }}\n"
        ));
        assert_eq!(
            task_integrity(&task, &records(Vec::new())),
            Integrity::untrusted("t")
        );
    }

    #[test]
    fn recover_reads_propagate_when_the_effect_is_clean() {
        let task = parse_task(&format!(
            "{HEAD}  t:\n    infer: {{ prompt: \"hi\", max_tokens: 5 }}\n    on_error: {{ recover: \"${{{{ tasks.dl.output }}}}\" }}\n"
        ));
        let recs = records(vec![settled("dl", Integrity::untrusted("dl"))]);
        assert_eq!(task_integrity(&task, &recs), Integrity::untrusted("dl"));
    }

    #[test]
    fn for_each_item_carries_the_collections_taint() {
        let task = parse_task(&format!(
            "{HEAD}  t:\n    for_each: {{ items: \"${{{{ tasks.dl.output }}}}\" }}\n    infer: {{ prompt: \"${{{{ item }}}}\", max_tokens: 5 }}\n"
        ));
        let recs = records(vec![settled("dl", Integrity::untrusted("dl"))]);
        assert_eq!(task_integrity(&task, &recs), Integrity::untrusted("dl"));
        // A literal list is authored — clean even with an item read.
        let literal = parse_task(&format!(
            "{HEAD}  t:\n    for_each: {{ items: [\"a\", \"b\"] }}\n    infer: {{ prompt: \"${{{{ item }}}}\", max_tokens: 5 }}\n"
        ));
        assert_eq!(
            task_integrity(&literal, &records(Vec::new())),
            Integrity::trusted()
        );
    }

    #[test]
    fn born_source_wins_the_witness_over_propagated_taint() {
        // A fetch whose args read ANOTHER tainted task: the witness is the
        // task itself (the content-flow priority — born first).
        let task = parse_task(&format!(
            "{HEAD}  dl2:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"${{{{ tasks.dl1.output }}}}\" }} }}\n"
        ));
        let recs = records(vec![settled("dl1", Integrity::untrusted("dl1"))]);
        assert_eq!(task_integrity(&task, &recs), Integrity::untrusted("dl2"));
    }

    #[test]
    fn value_taint_labels_one_template_with_the_same_law() {
        // The re-gate's per-element oracle: a with-chain, a literal, an
        // inputs read, and a mixed template — the walk is the task's own.
        let task = parse_task(&format!(
            "{HEAD}  use:\n    with: {{ page: \"${{{{ tasks.dl.output }}}}\" }}\n    exec: {{ command: [\"echo\", \"${{{{ with.page }}}}\"] }}\n"
        ));
        let recs = records(vec![settled("dl", Integrity::untrusted("dl"))]);
        let oracle = ValueTaint::of_task(&task, &recs);
        assert_eq!(
            oracle.label("${{ with.page }}", &recs),
            Integrity::untrusted("dl"),
            "a with-slot tainted by an upstream fetch"
        );
        assert_eq!(
            oracle.label("prefix-${{ with.page }}-suffix", &recs),
            Integrity::untrusted("dl"),
            "a mixed template joins every island it reads"
        );
        assert_eq!(
            oracle.label("${{ inputs.q }}", &recs),
            Integrity::untrusted("inputs.q"),
            "the caller boundary"
        );
        assert_eq!(
            oracle.label("a literal element", &recs),
            Integrity::trusted()
        );
        assert_eq!(
            oracle.label("${{ const.home }}", &recs),
            Integrity::trusted(),
            "the authored constant stays trusted"
        );
    }

    #[test]
    fn value_taint_item_reads_the_collections_taint() {
        let task = parse_task(&format!(
            "{HEAD}  t:\n    for_each: {{ items: \"${{{{ tasks.dl.output }}}}\" }}\n    exec: {{ command: [\"echo\", \"${{{{ item }}}}\"] }}\n"
        ));
        let recs = records(vec![settled("dl", Integrity::untrusted("dl"))]);
        let oracle = ValueTaint::of_task(&task, &recs);
        assert_eq!(
            oracle.label("${{ item }}", &recs),
            Integrity::untrusted("dl")
        );
        // A literal list is authored — the item stays clean.
        let literal = parse_task(&format!(
            "{HEAD}  t:\n    for_each: {{ items: [\"a\", \"b\"] }}\n    exec: {{ command: [\"echo\", \"${{{{ item }}}}\"] }}\n"
        ));
        assert_eq!(
            ValueTaint::of_task(&literal, &recs).label("${{ item }}", &recs),
            Integrity::trusted()
        );
    }

    #[test]
    fn value_taint_bare_keeps_the_records_and_inputs_lookups() {
        // The `on_finally:` mini-task shape: no with/for_each, but a
        // `tasks.X` or `inputs.X` read still taints (the cleanup re-gate).
        let oracle = ValueTaint::bare();
        let recs = records(vec![settled("dl", Integrity::untrusted("dl"))]);
        assert_eq!(
            oracle.label("${{ tasks.dl.output }}", &recs),
            Integrity::untrusted("dl")
        );
        assert_eq!(
            oracle.label("${{ inputs.q }}", &recs),
            Integrity::untrusted("inputs.q")
        );
        assert_eq!(
            oracle.label("${{ with.page }}", &recs),
            Integrity::trusted(),
            "a mini-task has no with: to taint"
        );
    }

    #[test]
    fn declassify_lifts_the_named_binding_scoped_to_the_task() {
        // NEP-0004 law 5 — the ONLY door: a task-level `declassify:` entry
        // raises its `from:` binding to trusted HERE (the re-gate oracle
        // AND the task's own output label), and leaves every OTHER
        // binding tainted (never a blanket lift).
        let task = parse_task(&format!(
            "{HEAD}  load:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"${{{{ inputs.p }}}}\" }}\n    lift:\n      - {{ law: taint, from: inputs.p, because: \"reviewed vendor path\" }}\n"
        ));
        let recs = records(Vec::new());
        let oracle = ValueTaint::of_task(&task, &recs);
        assert_eq!(
            oracle.label("${{ inputs.p }}", &recs),
            Integrity::trusted(),
            "the declassified binding reads trusted"
        );
        assert_eq!(
            oracle.label("${{ inputs.other }}", &recs),
            Integrity::untrusted("inputs.other"),
            "an unnamed binding stays tainted — the lift is scoped"
        );
        // …and the task's own output label follows (the author vouched).
        assert_eq!(task_integrity(&task, &recs), Integrity::trusted());

        // A tasks.<id>.output door lifts the record's label at the
        // CONSUMER (the record itself keeps its provenance). (The consumer
        // rides an invoke, never a born source — since F2 an exec's own
        // output is untrusted regardless of what it read, which would
        // mask the door this test is about.)
        let consumer = parse_task(&format!(
            "{HEAD}  use:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"${{{{ tasks.dl.output }}}}\" }}\n    lift:\n      - {{ law: taint, from: tasks.dl.output, because: \"pinned artifact, hash-reviewed\" }}\n"
        ));
        let recs = records(vec![settled("dl", Integrity::untrusted("dl"))]);
        assert_eq!(
            ValueTaint::of_task(&consumer, &recs).label("${{ tasks.dl.output }}", &recs),
            Integrity::trusted(),
            "the tasks.* door lifts the consumer's read"
        );
        assert_eq!(task_integrity(&consumer, &recs), Integrity::trusted());
        // …but a DIFFERENT tainted record still taints the same task.
        let recs2 = records(vec![
            settled("dl", Integrity::untrusted("dl")),
            settled("page", Integrity::untrusted("page")),
        ]);
        assert_eq!(
            ValueTaint::of_task(&consumer, &recs2).label("${{ tasks.page.output }}", &recs2),
            Integrity::untrusted("page"),
            "the door names ONE binding — page stays tainted"
        );
    }
}
