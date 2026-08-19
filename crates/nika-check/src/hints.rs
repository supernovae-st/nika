// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Improvement hints — the deterministic « ameliorateur ».
//!
//! Findings say « this is broken »; hints say « this could be BETTER »,
//! each with the concrete change that unlocks a stronger static
//! guarantee. They are advisory (never fail the check — `is_clean`
//! ignores them) and fully deterministic: the same workflow always
//! yields the same hints, because each one is derived from a structural
//! property the analyzer already computed.
//!
//! The hint classes, ranked by unlocked value ·
//!
//! - **unbounded cost** (`cost`) — an `infer:`/`agent:` task with no
//!   token bound: add one and the cost report becomes a hard ceiling.
//! - **degenerate cap** (`zero-cap`) — a declared `max_tokens: 0` /
//!   `max_tokens_total: 0`: arithmetically a true $0/0 Wh output
//!   ceiling, practically a call no provider will honor.
//! - **thinking budget** (`thinking-budget`) — a reasoning-capable
//!   model (the catalog knows) seated with `max_tokens` but no
//!   `thinking:`: the reasoning share lives INSIDE that budget, and a
//!   heavy think ends in a paid blank answer (NIKA-INFER-004 at run).
//! - **unconsumed output** (`dead-spend`) — a pure `infer:` task whose
//!   output no one reads (no task references it · not in `outputs:`):
//!   every token it spends is dead spend.
//! - **opaque consumed output** (`typing`) — a task whose output IS
//!   deeply referenced (`tasks.X.output.field`) but declares no
//!   `schema:` / `output:` bindings: declare a shape and the dataflow
//!   typer starts proving those references.
//! - **no boundary** (`permits`) — RETIRED by F-O8: absent + effects is
//!   the `NIKA-AUTH-006` ERROR (the escape scan owns it) · absent + pure
//!   compute gets the legal-zero hint from `check()`.
//! - **open schema** (`strictness`) — an object schema admitting
//!   undeclared keys: close it and the output shape is deterministic.
//! - **grammar-blind constraint** (`schema-portability`) — keywords no
//!   provider grammar enforces — see [`push_portability_hint`].
//! - **non-tightening after** (`redundant-gate`) — `after: {x:
//!   terminal}` beside a value edge to `x` changes nothing (edges
//!   compose by intersection · spec 03 §one obvious way /008): tighten
//!   to `success` or drop the entry.
//! - **retry on uncontracted effects** (`retry-effects`) — see
//!   [`push_retry_effects_hint`].
//! - **concurrent same-path writers** — RETIRED by F-P15 (NEP-0014 law
//!   1): the write-write race is the `NIKA-SEC-012` FINDING now (the
//!   DAG analysis pass owns it · a hint is not a boundary).
//! - **over-cap DAG read** (`analysis`) — H6: past the analysis task
//!   cap the width/pinch/blast read and the pair scan of the
//!   write-write law are skipped (the O(n²) `DoS` floor) — the skip is
//!   STATED here, never silent; the closure-free `for_each` same-path
//!   flavor still judged.
//! - **exec with a native path** (`native-first`) — emitted by the
//!   `check/native_first.rs` pass (the `native-first/001..005` ruleset:
//!   http/file/data/media/helper commands a builtin or MCP tool
//!   covers); `nika check --native-strict` promotes them to failures.
//! - **exec the run will refuse** (`exec-floor`) — RETIRED by #605: the
//!   argv-form command the runtime's exec floor refuses is the
//!   `NIKA-SEC-001` FINDING now (`check/exec_floor.rs` judges the SAME
//!   `nika-types::exec` predicate the run does — an error owns its
//!   repair, never a hint · the write-conflict precedent, F-P15).
//! - **unproven human-gate route** (`consent`) — emitted by the
//!   `check/consent.rs` lane (P0-2 · NEP-0020): an egress-capable
//!   descendant of a confirm-mode `nika:prompt` sits behind a gate the
//!   checker cannot PROVE consumes the answer (a nested binding · a
//!   non-fragment expression) — the PROVEN non-affirmative route is the
//!   `NIKA-SEC-014` refusal, this hint is the undecidable remainder
//!   (sound, never a false red); the advice teaches the `with: go` +
//!   `when:` pattern and the risk grade reads it as a High signal.
//! - **exec JSON stdout capture** (`exec-json-capture`) — an `exec:` task
//!   declares `capture: structured`, a binding parses `.stdout | fromjson`,
//!   and NO binding reads `exit_code`/`stderr`; use `capture: stdout` for
//!   JSON-producing helpers so non-zero exits fail as `NIKA-EXEC-001`
//!   instead of becoming data (a task branching on the record keeps
//!   `structured` — the hint stays silent there).
//! - **unwrapped reference** (`unwrapped-ref`) — a workflow `outputs:`
//!   value that spells a reference path (`tasks.X.output…` · `vars.X` · …)
//!   without the `${{ }}` wrapper rides as the LITERAL STRING (the run
//!   returns the path text, not the value); the hint names the wrap.
//! - **envelope bound into outputs** (`envelope-output`) — an
//!   `outputs:` binding referencing a BARE `tasks.X` captures the whole
//!   envelope (status · timestamps · output), so `nika test` goldens
//!   drift on the timestamps every run; bind `tasks.X.output` for the
//!   value. Suppresses `dead-spend` for the same task (the output IS
//!   consumed — in trap form).
//! - **deadline against an undeclared clock** (`run-clock`) — a task
//!   `timeout:` whose time source the envelope never names: the deadline
//!   rides the ambient system clock (the honest status quo · WARN-dur,
//!   NEVER a refusal — the existing corpus cannot turn red overnight);
//!   declare `run: { clock: … }` to pin the choice (F-P3).
//! - **markdown glob eats README** (`glob-readme`) — `nika:glob` of
//!   `*.md` without excluding README: the next infer classifies the
//!   table of contents as a record.
//! - **bare `map(` after `. as $`** (`jq-as-map`) — `nika:jq` binds
//!   `. as $c` then `map(`s the *current* value (often a pair). Write
//!   `($c | map(...))`.
//! - **assert after a write** (`assert-quarantine`) — a red
//!   `nika:assert` quarantines `out/` to `.nika/quarantine/<trace>/`.
//! - **the model names the verdict** (`infer-as-law`) — an `infer:`
//!   prompt asks the model to assign a belt / pick a level / score
//!   the grade. Extract integer facts; `nika:jq` (or `nika:decide`)
//!   is the law. A "never assign a belt" extract stays silent.
//! - **the law is unproven** (`unproven-law`) — `nika:jq` / `nika:decide`
//!   scores an infer extract and no const-fixture `nika:assert` proves
//!   the law on known answers. `is_clean` does not compile the law.

use std::collections::BTreeSet;

use crate::walk::{consumed_outputs, deeply_referenced, envelope_bound_outputs};
use nika_schema::expression::scan_templates;
use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use nika_schema::types::CaptureMode;

/// One advisory improvement with its concrete unlock.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct Hint {
    /// The hint class — the closed set today: `cost` · `zero-cap` ·
    /// `thinking-budget` · `dead-spend` ·
    /// `typing` · `permits` · `strictness` · `schema-portability` ·
    /// `redundant-gate` · `retry-effects` ·
    /// `secrets-store` · `native-first` ·
    /// `exec-json-capture` ·
    /// `unwrapped-ref` · `envelope-output` · `policy-soft` · `run-clock`
    /// · `analysis` · `consent` · `digit-string-enum` · `inspect-unwired`
    /// · `glob-readme` · `assert-quarantine` · `jq-as-map` · `infer-as-law`
    /// · `unproven-law`
    /// (additive · agents route on it; the module doc describes each).
    /// The paid-run family ([`PAID_RUN_KINDS`]) is what [`paid_ready`]
    /// reads — never `is_clean`.
    /// `parallel-writers` is RETIRED (F-P15 · promoted to the
    /// NIKA-SEC-012 finding — an error owns its repair, never a hint).
    /// `exec-floor` is RETIRED (#605 · promoted to the NIKA-SEC-001
    /// finding — the check judges the SAME predicate the run does).
    pub kind: &'static str,
    /// The task it concerns (`-` for workflow-level hints).
    pub task: String,
    /// What to change and what it unlocks.
    pub advice: String,
}

/// Hint kinds that mean the file is legal but must not leave `mock/`.
/// A green `is_clean` with any of these is the 2026-08-19 paid-run class.
pub const PAID_RUN_KINDS: &[&str] = &[
    "digit-string-enum",
    "glob-readme",
    "inspect-unwired",
    "jq-as-map",
    "infer-as-law",
    "unproven-law",
];

impl Hint {
    /// Whether this hint is in the paid-run family.
    #[must_use]
    pub fn is_paid_run(&self) -> bool {
        PAID_RUN_KINDS.contains(&self.kind)
    }
}

/// The paid-run hints still on the file, in scan order.
#[must_use]
pub fn paid_blockers(hints: &[Hint]) -> Vec<&Hint> {
    hints.iter().filter(|h| h.is_paid_run()).collect()
}

/// True iff no paid-run hint fired. Never consults `is_clean`.
#[must_use]
pub fn paid_ready(hints: &[Hint]) -> bool {
    !hints.iter().any(Hint::is_paid_run)
}

/// True iff no `unproven-law` hint fired. A file with no law is compiled.
/// Never consults `is_clean` or `paid_ready`.
#[must_use]
pub fn compiled(hints: &[Hint]) -> bool {
    !hints.iter().any(|h| h.kind == "unproven-law")
}

/// Stamp `paid_ready` / `paid_blockers` / `compiled` / `next` onto a
/// serialized check report. Additive · `report_version` stays 1 ·
/// `clean` is untouched. `next` is the first paid blocker plus its
/// advice — the one repair an agent should do now.
pub fn stamp_paid_ready(obj: &mut serde_json::Map<String, serde_json::Value>, hints: &[Hint]) {
    let paid = paid_blockers(hints);
    obj.insert(
        "paid_ready".to_owned(),
        serde_json::Value::Bool(paid.is_empty()),
    );
    obj.insert(
        "compiled".to_owned(),
        serde_json::Value::Bool(compiled(hints)),
    );
    if let Some(h) = paid.first() {
        obj.insert(
            "next".to_owned(),
            serde_json::json!({
                "kind": h.kind,
                "task": h.task,
                "advice": h.advice,
            }),
        );
        obj.insert(
            "paid_blockers".to_owned(),
            serde_json::Value::Array(
                paid.iter()
                    .map(|b| serde_json::json!({ "kind": b.kind, "task": b.task }))
                    .collect(),
            ),
        );
    }
}

/// Compute the improvement hints for a workflow.
#[must_use]
pub(super) fn scan_hints(wf: &RawWorkflow) -> Vec<Hint> {
    let consumed = consumed_outputs(wf);
    let deep_referenced = deeply_referenced(wf);
    let envelope_bound = envelope_bound_outputs(wf);
    let envelope_ids: BTreeSet<&str> = envelope_bound.iter().map(|(_, id)| id.as_str()).collect();
    let mut hints = Vec::new();
    for (name, id) in &envelope_bound {
        hints.push(Hint {
            kind: "envelope-output",
            task: id.clone(),
            advice: format!(
                "outputs.{name} binds the whole ENVELOPE of `{id}` (status · timestamps · \
                 output) — `nika test` goldens drift on its timestamps every run; for the \
                 value alone bind ${{{{ tasks.{id}.output }}}}"
            ),
        });
    }

    for task in &wf.tasks {
        let t = &task.value;
        let id = t.id.value.as_str();
        push_redundant_gate_hints(&mut hints, t, id);
        match &t.action {
            RawAction::Infer(a) => {
                push_infer_hints(
                    &mut hints,
                    t,
                    id,
                    a,
                    &consumed,
                    &envelope_ids,
                    &deep_referenced,
                );
                push_infer_as_law_hint(&mut hints, id, a);
            }
            RawAction::Agent(a) => {
                if a.max_tokens_total.is_none() {
                    hints.push(hint("cost", id, format!(
                        "declare `max_tokens_total` on `{id}` — the agent loop gets a hard budget instead of UNBOUNDED"
                    )));
                }
                // Same degenerate-cap class as the infer arm.
                if a.max_tokens_total.as_ref().is_some_and(|t| t.value == 0) {
                    hints.push(hint("zero-cap", id, format!(
                        "`max_tokens_total: 0` on `{id}` forbids all output — the agent loop cannot spend anything; set a real budget or remove the task"
                    )));
                }
                push_strictness_hint(&mut hints, id, a.schema.as_ref().map(|s| &s.value));
                push_portability_hint(&mut hints, id, a.schema.as_ref().map(|s| &s.value));
                push_digit_enum_hint(&mut hints, id, a.schema.as_ref().map(|s| &s.value));
            }
            RawAction::Exec(exec) => {
                push_exec_json_capture_hint(&mut hints, t, exec);
            }
            RawAction::Invoke(a) => {
                push_headless_prompt_hint(&mut hints, id, a);
                push_inspect_unwired_hint(&mut hints, id, a);
                push_glob_readme_hint(&mut hints, id, a);
                push_jq_as_map_hint(&mut hints, id, a);
            }
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown action: {other:?}"),
        }
        push_retry_effects_hint(&mut hints, t);
    }

    // F-O8 · the old « no `permits:` boundary declared » advisory is
    // RETIRED: absent + effects is the NIKA-AUTH-006 ERROR now (the
    // `capability_escapes` lane owns it), and absent + pure compute gets
    // the legal-zero hint from `check()` (the exact, escapes-based
    // condition — this lane cannot see the escape scan).
    push_unresolvable_secret_hints(&mut hints, wf);
    push_unwrapped_output_ref_hints(&mut hints, wf);
    push_swallowed_exit_hints(&mut hints, wf);
    push_run_clock_hint(&mut hints, wf);
    push_assert_quarantine_hint(&mut hints, wf);
    push_unproven_law_hints(&mut hints, wf);
    hints
}

/// The infer arm of [`scan_hints`] — cost cap · degenerate zero-cap ·
/// dead-spend · deep-ref typing · the shared schema pair (split at the
/// 100-line function law).
fn push_infer_hints(
    hints: &mut Vec<Hint>,
    t: &nika_schema::raw::RawTask,
    id: &str,
    a: &nika_schema::raw::RawInferAction,
    consumed: &BTreeSet<String>,
    envelope_ids: &BTreeSet<&str>,
    deep_referenced: &BTreeSet<String>,
) {
    if a.max_tokens.is_none() {
        hints.push(hint("cost", id, format!(
            "declare `max_tokens` on `{id}` — the cost report becomes a hard ceiling instead of UNBOUNDED"
        )));
    }
    // The degenerate cap (probe 2026-07-30): `max_tokens: 0` parses,
    // prices to a true-$0 output ceiling, and sailed through green —
    // but a zero budget forbids ALL output, so the call cannot produce
    // anything (providers refuse it). The COST/ENERGY zeros are
    // arithmetically true; the declaration is still a bug worth naming.
    if a.max_tokens.as_ref().is_some_and(|t| t.value == 0) {
        hints.push(hint("zero-cap", id, format!(
            "`max_tokens: 0` on `{id}` forbids all output — the call cannot produce anything (providers refuse a zero budget); set a real cap or remove the task"
        )));
    }
    // The thinking-budget teaching (#651 · leg 3): a reasoning-capable
    // model with `max_tokens` but no `thinking:` can burn the whole
    // budget on its reasoning trace and conclude with a blank visible
    // answer — the typed NIKA-INFER-004 failure at run since leg 1. The
    // hint teaches the declaration BEFORE a token is spent. A literal
    // seat only: a templated model defers to the run's resolution.
    if a.thinking.is_none()
        && a.max_tokens.is_some()
        && let Some(m) = &a.model
        && !m.value.contains("${{")
        && let Some((provider, name)) = m.value.split_once('/')
        && nika_catalog::model_capabilities(provider, name).reasoning
    {
        hints.push(hint("thinking-budget", id, format!(
            "`{id}` seats a reasoning-capable model with `max_tokens` but no `thinking:` — the reasoning share lives INSIDE that budget; declare `thinking:` (or a no-think variant) before a heavy think ends NIKA-INFER-004 (a paid blank answer)"
        )));
    }
    if !consumed.contains(id) && !envelope_ids.contains(id) {
        hints.push(hint("dead-spend", id, format!(
            "no task or output consumes `tasks.{id}.output` — every token this infer spends is unread; consume it or remove the task"
        )));
    }
    // `returns:` is a declared shape too — and the PREFERRED one
    // (one-obvious-way/011): advising `schema:` on a task that
    // already carries `returns:` would advise the exact pair
    // NIKA-TYPE-003 refuses. Measured 2026-07-29 on a corpus
    // lesson: `returns: Entities` + a deep ref drew this hint.
    if deep_referenced.contains(id)
        && a.schema.is_none()
        && t.returns.is_none()
        && t.extract.is_empty()
    {
        hints.push(hint("typing", id, format!(
            "deep references into `tasks.{id}.output.<field>` exist but `{id}` declares no output shape — declare `returns:` and `nika check` starts proving those field names"
        )));
    }
    push_strictness_hint(hints, id, a.schema.as_ref().map(|s| &s.value));
    push_portability_hint(hints, id, a.schema.as_ref().map(|s| &s.value));
    push_digit_enum_hint(hints, id, a.schema.as_ref().map(|s| &s.value));
}

/// The deadline-vs-undeclared-clock hint (F-P3 finding (b)): a task
/// `timeout:` is a deadline ρ — and a deadline whose clock the envelope
/// never names rides the ambient system clock. That is the honest status
/// quo (WARN-dur · NEVER a refusal: the existing corpus cannot turn red
/// overnight), so the hint teaches the declaration that pins the choice.
/// One workflow-level row, however many tasks carry a deadline. The
/// clock counts as named when `run.clock` is explicit OR when a
/// deterministic `run.entropy` binds it by law (the virtual clock —
/// `entropy: none | seeded` implies it); under those the deadline's
/// time source IS declared.
fn push_run_clock_hint(hints: &mut Vec<Hint>, wf: &RawWorkflow) {
    let clock_named = wf.run.as_ref().is_some_and(|run| {
        let decl = &run.value;
        decl.clock.is_some() || decl.entropy_or_default().is_deterministic()
    });
    if clock_named {
        return;
    }
    let deadlines = wf
        .tasks
        .iter()
        .filter(|task| task.value.timeout.is_some())
        .count();
    if deadlines == 0 {
        return;
    }
    hints.push(hint(
        "run-clock",
        "-",
        format!(
            "{deadlines} task(s) declare `timeout:` against an undeclared clock — the \
             deadline rides the ambient system clock (the honest default, never a refusal); \
             declare `run: {{ clock: system }}` to pin the choice out loud, or \
             `clock: virtual` for a simulated clock (F-P3)"
        ),
    ));
}

/// The `redundant-gate` hint (6. non-tightening after) — `after:
/// {x: terminal}` beside a value edge changes nothing (edges compose by
/// intersection: {success, skipped} ∩ terminal = the value edge alone ·
/// one-obvious-way/008); tighten to `success` or drop it.
/// The `headless-prompt` hint: a `nika:prompt` with no `default:` cannot
/// answer itself — unattended, the run PARKS at this gate (ADR-099
/// durable pause · exit 4 · the resume line taught on the frame); at a
/// terminal the gate asks directly. The one-pass unattended answer is
/// `--answer <task>=<value>` at launch.
///
/// The absence of a `default:` is a STATIC fact, visible in the file
/// before anything runs. Saying nothing about it at check time is how an
/// agent hands over a workflow it just audited green and the human
/// watches it park on the first run — reported from Cursor 2026-07-28
/// (when the park was still a hard NIKA-BUILTIN-PROMPT-001 death: the
/// agent did its job correctly and the oracle was the one that lied),
/// then again live 2026-07-31 (seo-live-review, 13ms, 22 cancelled
/// rows — the arc that armed the pause on every lane).
///
/// Advisory, not a refusal: an interactive workflow is a legitimate
/// thing to author. The hint names the behavior, it does not forbid it.
fn push_headless_prompt_hint(
    hints: &mut Vec<Hint>,
    id: &str,
    a: &nika_schema::raw::RawInvokeAction,
) {
    let nika_schema::raw::RawInvokeTarget::Tool(tool) = &a.target else {
        return;
    };
    if tool.value != "nika:prompt" {
        return;
    }
    if a.args
        .as_ref()
        .and_then(|args| args.value.get("default"))
        .is_some()
    {
        return;
    }
    hints.push(hint(
        "headless-prompt",
        id,
        format!(
            "`nika:prompt` on `{id}` declares no `default:` — unattended (CI, or an \
             agent handing it over) the run pauses at this gate awaiting a human \
             (exit 4 · the resume line taught on the frame); at a terminal it asks \
             directly. Answer it in one pass with `nika run <file> --answer \
             {id}=<value>`, or declare the `default:` the unattended path should take"
        ),
    ));
}

fn push_redundant_gate_hints(hints: &mut Vec<Hint>, t: &RawTask, id: &str) {
    for (target, pred) in &t.after {
        if !matches!(pred.value, nika_schema::types::AfterPredicate::Terminal) {
            continue;
        }
        let has_value_edge = t.with.iter().any(|(_k, v)| {
            let mut refs = Vec::new();
            crate::analyzer::edges::task_refs_in_value(&v.value, &mut refs);
            refs.iter().any(|(rid, field)| {
                rid == &target.value
                    && matches!(
                        crate::analyzer::edges::role_of_field(field.as_deref()),
                        crate::analyzer::edges::EdgeKind::Value
                    )
            })
        });
        if has_value_edge {
            hints.push(Hint {
                kind: "redundant-gate",
                task: id.to_owned(),
                advice: format!(
                    "`after: {{{t}: terminal}}` beside a value edge to `{t}` is a non-tightening restatement \u{2014} the composed gate is the value edge's {{success, skipped}} either way (spec 03 \u{a7}one obvious way /010); drop the entry or tighten to `success`",
                    t = target.value
                ),
            });
        }
    }
}

/// The `unwrapped-ref` hint (output gauntlet 2026-07-11): a workflow
/// `outputs:` value that LOOKS like a reference (`tasks.<id>.output…` ·
/// `vars.<x>` · `env.<x>` · `with.<x>` · `secrets.<x>`) but carries no
/// `${{ }}` island rides as the LITERAL STRING — the run returns
/// `"tasks.data.output.count"`, not the extracted value. A silent footgun
/// (the workflow « works » and returns the wrong thing); the hint names
/// the wrap. Advisory: a literal string that happens to spell a namespace
/// path is legal (absurd, but the author's call), so this teaches, never
/// fails. The pattern is distinctive — a bare namespace-dotted path is
/// almost never a wanted constant.
fn push_unwrapped_output_ref_hints(hints: &mut Vec<Hint>, wf: &RawWorkflow) {
    const NAMESPACES: [&str; 5] = ["tasks.", "vars.", "env.", "with.", "secrets."];
    for (name, decl) in &wf.outputs {
        let value = &decl.value().value;
        // Already interpolated (any `${{ }}`) → the author knows the wrapper.
        if value.contains("${{") {
            continue;
        }
        let trimmed = value.trim();
        if NAMESPACES.iter().any(|ns| trimmed.starts_with(ns)) {
            hints.push(hint(
                "unwrapped-ref",
                &name.value,
                format!(
                    "output `{}` is the literal string `{trimmed}` — it looks like a reference; \
                     wrap it to interpolate: `${{{{ {trimmed} }}}}`",
                    name.value
                ),
            ));
        }
    }
}

/// The `swallowed-exit` hint: `capture: structured` turns a non-zero exit
/// into DATA — the task SUCCEEDS and `exit_code` is the branch (spec 02
/// §exec · the one-obvious-way split). That is coherent when someone
/// reads the branch, and a silent failure when nobody does.
///
/// Reported 2026-07-28: a run showed 23/23 green while four tasks had
/// failed. Two of them exited non-zero under `structured` and were
/// reported as successes; the error surfaced three waves later on an
/// unrelated `jq` that indexed an empty string, so the diagnosis pointed
/// at the wrong task entirely.
///
/// The narrower `exec-json-capture` hint above fires only on the
/// `.stdout | fromjson` shape. This one asks the question that actually
/// matters: does ANYONE read `exit_code`, here or downstream? If not,
/// the task cannot fail on the command failing, and the author almost
/// never meant that.
///
/// Advisory: branching on the code is legitimate, and so is deliberately
/// ignoring it. The hint names what was traded away, it does not refuse.
fn push_swallowed_exit_hints(hints: &mut Vec<Hint>, wf: &RawWorkflow) {
    // Every text surface of the workflow, once — a read of `exit_code`
    // anywhere (this task's own `output:` bindings, a downstream `with:`
    // binding, an `outputs:` entry) means the branch is being used.
    let mut corpus = String::new();
    for task in &wf.tasks {
        for field in task_text_fields(&task.value) {
            corpus.push_str(field);
            corpus.push('\n');
        }
        for (_, binding) in &task.value.extract {
            corpus.push_str(&binding.value);
            corpus.push('\n');
        }
    }
    for (_, decl) in &wf.outputs {
        corpus.push_str(&decl.value().value);
        corpus.push('\n');
    }
    if corpus.contains("exit_code") {
        return;
    }
    for task in &wf.tasks {
        let t = &task.value;
        let RawAction::Exec(action) = &t.action else {
            continue;
        };
        if !matches!(
            action.capture.as_ref().map(|capture| capture.value),
            Some(CaptureMode::Structured)
        ) {
            continue;
        }
        let id = t.id.value.as_str();
        hints.push(hint(
            "swallowed-exit",
            id,
            format!(
                "`capture: structured` on `{id}` makes a non-zero exit DATA, not a \
                 failure — the task reports success whatever the command returns, and \
                 nothing in this workflow reads `exit_code`. A command that fails here \
                 is invisible until something downstream chokes on its empty output. \
                 Read the branch (`${{{{ tasks.{id}.output.exit_code }}}}`, or a \
                 `nika:assert` on it), or use a text capture mode so a non-zero exit \
                 fails the task as NIKA-EXEC-001"
            ),
        ));
    }
}

/// `capture: structured` is for branching on `{stdout, stderr, exit_code}`
/// as data. When a binding parses `.stdout` as JSON and NO binding reads the
/// record's other fields (`exit_code` · `stderr`), the one-obvious-way is
/// `capture: stdout` + `fromjson`: a missing helper or non-zero subprocess
/// then fails as `NIKA-EXEC-001` with stderr preserved, rather than
/// surfacing later as an output-binding cardinality error. A task that DOES
/// branch on `exit_code`/`stderr` uses `structured` legitimately — the hint
/// stays silent there (its own advice would break that binding).
fn push_exec_json_capture_hint(
    hints: &mut Vec<Hint>,
    task: &RawTask,
    action: &nika_schema::raw::RawExecAction,
) {
    if !matches!(
        action.capture.as_ref().map(|capture| capture.value),
        Some(CaptureMode::Structured)
    ) {
        return;
    }
    // The `.stdout | fromjson` chain, whitespace-insensitive — an unrelated
    // field that merely CONTAINS the substrings (`.stderr | fromjson |
    // .stdout_field`) is not the pattern.
    let parses_stdout_json = task.extract.iter().any(|(_, binding)| {
        let compact: String = binding
            .value
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        compact.contains(".stdout|fromjson")
    });
    // Another binding consuming the structured record's OTHER fields means
    // `structured` is the point, not an accident.
    let reads_record_fields = task.extract.iter().any(|(_, binding)| {
        binding.value.contains(".exit_code") || binding.value.contains(".stderr")
    });
    if parses_stdout_json && !reads_record_fields {
        let id = task.id.value.as_str();
        hints.push(hint(
            "exec-json-capture",
            id,
            format!(
                "`{id}` parses `.stdout | fromjson` while using `capture: structured` and no binding reads `exit_code`/`stderr` — for a JSON-producing helper, use `capture: stdout` and bindings like `fromjson`: a failing subprocess then errors as NIKA-EXEC-001 instead of becoming data"
            ),
        ));
    }
}

/// The `secrets-store` hint (MINOR-B): a referenced `secrets.X` whose
/// `source` the runtime cannot resolve yet (`vault`) — without this the
/// check is GREEN but the value fails at runtime with NIKA-1702 (an
/// unresolved reference). The hint names the gap so the author switches the
/// store (`env`/`file`) or waits for vault wiring, rather than hitting a
/// green-check → runtime-1702 surprise. Only fires for a REFERENCED secret
/// (a declared-but-unused vault secret is harmless). Advisory — never fails
/// the check.
fn push_unresolvable_secret_hints(hints: &mut Vec<Hint>, wf: &RawWorkflow) {
    use nika_schema::types::SecretSource;
    if wf.secrets.is_empty() {
        return;
    }
    let referenced = referenced_secrets(wf);
    for (name, secret) in &wf.secrets {
        // `env`/`file` are wired; only the not-yet-resolvable sources warn.
        if matches!(secret.value.source, SecretSource::Env | SecretSource::File) {
            continue;
        }
        if referenced.contains(name.value.as_str()) {
            hints.push(hint(
                "secrets-store",
                "-",
                format!(
                    "`secrets.{name}` uses source `{source}`, not yet runtime-resolvable \u{2014} the check is green but `${{{{ secrets.{name} }}}}` will fail at run with NIKA-1702; use `source: env` or `source: file` until vault resolution ships",
                    name = name.value,
                    source = secret.value.source,
                ),
            ));
        }
    }
}

/// Every `secrets.<name>` referenced anywhere in the workflow's `${{ }}`
/// islands (task fields · `with:` · `outputs:`) — drives the `secrets-store`
/// hint so it fires only for a USED secret.
fn referenced_secrets(wf: &RawWorkflow) -> BTreeSet<String> {
    use nika_schema::expression::{NamespaceRef, expr_refs};
    let mut out = BTreeSet::new();
    let mut collect = |text: &str| {
        if let Ok(islands) = scan_templates(text) {
            for island in &islands {
                for r in expr_refs(&island.expr) {
                    if let NamespaceRef::Secrets(name) = r {
                        out.insert(name);
                    }
                }
            }
        }
    };
    for task in &wf.tasks {
        for text in task_text_fields(&task.value) {
            collect(text);
        }
    }
    for (_, decl) in &wf.outputs {
        collect(decl.value().value.as_str());
    }
    out
}

/// Every authored text fragment of a task that may carry a `${{ secrets.X }}`
/// island (effect fields · `with:` values · `when:` body) — the surface
/// [`referenced_secrets`] scans.
fn task_text_fields(t: &nika_schema::raw::RawTask) -> Vec<&str> {
    let mut fields = Vec::new();
    match &t.action {
        RawAction::Exec(a) => {
            fields.extend(a.command.text_fragments());
            if let Some(stdin) = &a.stdin {
                fields.push(stdin.value.as_str());
            }
            for (_, v) in &a.env {
                fields.push(v.value.as_str());
            }
        }
        RawAction::Invoke(a) => {
            if let Some(args) = a.args.as_ref() {
                collect_json_strings_into(&args.value, &mut fields);
            }
        }
        RawAction::Infer(a) => {
            fields.push(a.prompt.value.as_str());
            if let Some(s) = &a.system {
                fields.push(s.value.as_str());
            }
        }
        RawAction::Agent(a) => {
            fields.push(a.prompt.value.as_str());
            if let Some(s) = &a.system {
                fields.push(s.value.as_str());
            }
        }
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
    for (_, v) in &t.with {
        collect_json_strings_into(&v.value, &mut fields);
    }
    fields
}

/// Every string leaf of a JSON value (with-values · invoke args).
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

/// The retry-safety hint (class `retry-effects`): `retry:` replays the
/// WHOLE attempt on transient failure — at-least-once semantics. For
/// effect classes with no idempotency contract that means duplicated
/// side effects (a subprocess killed mid-write already mutated the
/// world; the replay mutates it again). Conservative scope — only the
/// classes whose contract is genuinely unknown or genuinely absent:
///
/// - `exec:` — arbitrary subprocess, side effects unknowable;
/// - `invoke: mcp:*` — external tool, no idempotency contract;
/// - `invoke: nika:notify` — the v0.1 channel is a raw webhook send
///   (nika-builtin `defs.rs` §net): no dedup key rides the payload, so a
///   replay delivers the notification twice;
/// - `invoke: nika:fetch` with a NON-idempotent method — `defs.rs`
///   declares `POST | PUT | DELETE | PATCH` beside the GET default and
///   the http effect doc (nika-kernel-core `io/http.rs`) states « GET
///   idempotent · POST must pair an idempotency key ». A declared
///   `idempotency-key` header discharges the hazard.
///
/// The OTHER `nika:` builtins carry documented idempotent-or-pure
/// semantics (atomic-overwrite `write` · data transforms · reads) and an
/// `infer:` retry re-spends tokens but mutates nothing external — no
/// claim on those. An `agent:` retry DOES replay
/// its whole tool loop, but which effects that re-dispatches depends on
/// the runtime whitelist state — tool-mediated and out of this static
/// rung's scope (a dedicated agent-retry read would own it). The formal
/// idempotency treatments verify ENGINES, not workflow files (Rehearsal
/// · Shambaugh et al. · PLDI 2016 · Puppet manifests via SMT; Durable
/// Functions · Burckhardt et al. · OOPSLA 2021 · deterministic-replay
/// semantics) — this hint is the static, file-level read of the hazard.
fn push_retry_effects_hint(hints: &mut Vec<Hint>, t: &nika_schema::raw::RawTask) {
    let retries = t.retry.as_ref().is_some_and(|r| r.value.max_attempts > 1);
    if !retries {
        return;
    }
    let id = t.id.value.as_str();
    match &t.action {
        RawAction::Exec(_) => {
            hints.push(hint("retry-effects", id, format!(
                "`{id}` retries a subprocess — a transient failure mid-effect replays side effects already applied (at-least-once); make the command idempotent or guard it with a pre-check"
            )));
        }
        RawAction::Invoke(a) if a.tool().is_some_and(|t| t.value.starts_with("mcp:")) => {
            let tool = a.tool().map_or("", |t| t.value.as_str());
            hints.push(hint("retry-effects", id, format!(
                "`{id}` retries `{tool}` — external MCP tools carry no idempotency contract; a transient failure replays the call's side effects (at-least-once)"
            )));
        }
        RawAction::Invoke(a) => {
            let Some(tool) = a.tool().map(|t| t.value.as_str()) else {
                return; // a `workflow:` child call — the child's own check owns its effects
            };
            let args = a.args.as_ref().map(|s| &s.value);
            match tool.strip_prefix("nika:") {
                Some("notify") => {
                    hints.push(hint("retry-effects", id, format!(
                        "`{id}` retries `nika:notify` — a webhook send carries no idempotency contract; a transient failure after the send replays it (duplicate notification · at-least-once) — make the receiver dedup on a key in `data:` or drop the `retry:`"
                    )));
                }
                Some("fetch") if fetch_method_replays_effects(args) => {
                    if declares_idempotency_key(args) {
                        return; // the declared key lets the receiver dedup the replay
                    }
                    let method =
                        fetch_method(args).map_or_else(|| "?".to_owned(), str::to_ascii_uppercase);
                    hints.push(hint("retry-effects", id, format!(
                        "`{id}` retries `nika:fetch` with method {method} — non-idempotent HTTP replays the request's side effects (at-least-once); pair an `idempotency-key` header (the receiver dedups) or drop the `retry:` — GET/HEAD retry free"
                    )));
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// The `args.method` string of a `nika:fetch` call (absent = the GET
/// default · nika-builtin `defs.rs` §fetch).
fn fetch_method(args: Option<&serde_json::Value>) -> Option<&str> {
    args?.get("method")?.as_str()
}

/// Whether a `nika:fetch` call's method replays side effects on retry:
/// POST/PUT/DELETE/PATCH do (nika-kernel-core `io/http.rs`: « POST must
/// pair an idempotency key ») · GET/HEAD and the GET default replay
/// nothing · an UNRECOGNIZED method makes no claim here (the builtin
/// shape ladder owns the invalid-method finding).
fn fetch_method_replays_effects(args: Option<&serde_json::Value>) -> bool {
    fetch_method(args).is_some_and(|m| {
        matches!(
            m.to_ascii_uppercase().as_str(),
            "POST" | "PUT" | "DELETE" | "PATCH"
        )
    })
}

/// Whether the call declares an `idempotency-key` header (any case) —
/// the dedup contract that discharges the retry-replay hazard.
fn declares_idempotency_key(args: Option<&serde_json::Value>) -> bool {
    args.and_then(|v| v.get("headers"))
        .and_then(serde_json::Value::as_object)
        .is_some_and(|h| h.keys().any(|k| k.eq_ignore_ascii_case("idempotency-key")))
}

/// The structured-output determinism hint (class `strictness`): an
/// object node declaring `properties` but NOT `additionalProperties:
/// false` admits undeclared keys — the model can emit extra fields and
/// the validated shape varies across providers/runs. Closing it pins
/// the shape (the recipe provider-native strict modes require). One
/// hint per task, however many open nodes.
///
/// The verb is CLOSE, never "add": the same hint fires on a node that
/// omits `additionalProperties` and on one that spells out
/// `additionalProperties: true`, and telling the second author to *add*
/// the key they already wrote reads as a bug in the checker. "Close its
/// object nodes with" is true of both, and needs no second branch to
/// say so.
fn push_strictness_hint(hints: &mut Vec<Hint>, id: &str, schema: Option<&serde_json::Value>) {
    if schema.is_some_and(has_open_object) {
        hints.push(hint("strictness", id, format!(
            "`{id}`'s schema admits undeclared keys — close its object nodes with `additionalProperties: false` for a deterministic output shape across providers"
        )));
    }
}

/// Visit every child subschema of one node — the ONE composite descent the
/// schema walkers share (`properties` values · `items` · branch keywords).
fn for_each_subschema(
    obj: &serde_json::Map<String, serde_json::Value>,
    f: &mut impl FnMut(&serde_json::Value),
) {
    let props = obj.get("properties").and_then(serde_json::Value::as_object);
    props
        .into_iter()
        .flat_map(serde_json::Map::values)
        .for_each(&mut *f);
    for key in [
        "items", "not", "if", "then", "else", "anyOf", "oneOf", "allOf",
    ] {
        match obj.get(key) {
            Some(serde_json::Value::Array(kids)) => kids.iter().for_each(&mut *f),
            Some(kid) => f(kid),
            None => {}
        }
    }
}

/// Whether any object node in the schema declares `properties` without
/// closing `additionalProperties`; `$ref` is opaque (no claim).
fn has_open_object(node: &serde_json::Value) -> bool {
    node.as_object()
        .filter(|o| !o.contains_key("$ref"))
        .is_some_and(|obj| {
            let closed = obj.get("additionalProperties") == Some(&serde_json::Value::Bool(false));
            let has_props = obj
                .get("properties")
                .and_then(serde_json::Value::as_object)
                .is_some();
            let mut open = !closed && has_props;
            for_each_subschema(obj, &mut |child| open = open || has_open_object(child));
            open
        })
}

/// The `schema-portability` hint: keywords NO provider grammar enforces
/// (proven live 2026-07-07) — only LOCAL validation holds them, per-retry.
fn push_portability_hint(hints: &mut Vec<Hint>, id: &str, schema: Option<&serde_json::Value>) {
    let mut found = BTreeSet::new();
    schema.inspect(|s| collect_grammar_blind(s, &mut found));
    if !found.is_empty() {
        let list = found.into_iter().collect::<Vec<_>>().join("` · `");
        hints.push(hint("schema-portability", id, format!(
            "`{id}`'s schema relies on `{list}` — provider grammars accept but do NOT enforce these keywords (constrained decoding emits violating values unchecked); only Nika's local validation holds them, spending schema retries when the model strays. Express the constraint structurally (`enum` · item bounds · closed objects) where possible"
        )));
    }
}

/// Binding occurrences only (`uniqueItems: false` / a bare `if` constrain
/// nothing — no claim); property NAMES are never keywords; `$ref` opaque.
fn collect_grammar_blind(node: &serde_json::Value, out: &mut BTreeSet<&'static str>) {
    if let Some(obj) = node.as_object().filter(|o| !o.contains_key("$ref")) {
        let cond = obj.contains_key("if") && (obj.contains_key("then") || obj.contains_key("else"));
        let unique = obj.get("uniqueItems").and_then(serde_json::Value::as_bool) == Some(true);
        out.extend(unique.then_some("uniqueItems"));
        out.extend(obj.contains_key("not").then_some("not"));
        out.extend(cond.then_some("if/then/else"));
        for_each_subschema(obj, &mut |kid| collect_grammar_blind(kid, out));
    }
}

/// String `enum` of digits only (`"0"|"1"|"3"`). Models emit JSON
/// numbers; provider grammars may reject the call before Nika coerce
/// stringifies. Prefer `type: integer`.
fn push_digit_enum_hint(hints: &mut Vec<Hint>, id: &str, schema: Option<&serde_json::Value>) {
    let mut paths = Vec::new();
    if let Some(node) = schema {
        collect_digit_string_enums(node, "", &mut paths);
    }
    if paths.is_empty() {
        return;
    }
    let list = paths.join("` · `");
    hints.push(hint(
        "digit-string-enum",
        id,
        format!(
            "`{id}` declares a string enum of digits only at `{list}` — models emit JSON numbers \
             (`3` not `\"3\"`); constrained decoding can reject the call before Nika's coerce \
             stringifies. Prefer `type: integer` with a numeric enum"
        ),
    ));
}

fn collect_digit_string_enums(node: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    let Some(obj) = node.as_object().filter(|o| !o.contains_key("$ref")) else {
        return;
    };
    let types: Vec<&str> = match obj.get("type") {
        Some(serde_json::Value::String(t)) => vec![t.as_str()],
        Some(serde_json::Value::Array(list)) => list.iter().filter_map(|t| t.as_str()).collect(),
        _ => Vec::new(),
    };
    let string_only = types == ["string"];
    if string_only
        && let Some(variants) = obj.get("enum").and_then(serde_json::Value::as_array)
        && !variants.is_empty()
        && variants.iter().all(|v| {
            v.as_str().is_some_and(|s| {
                !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b'-')
            })
        })
    {
        out.push(if path.is_empty() {
            "/".to_owned()
        } else {
            path.to_owned()
        });
    }
    if let Some(props) = obj.get("properties").and_then(serde_json::Value::as_object) {
        for (key, child) in props {
            collect_digit_string_enums(child, &format!("{path}/properties/{key}"), out);
        }
    }
    if let Some(items) = obj.get("items") {
        collect_digit_string_enums(items, &format!("{path}/items"), out);
    }
}

/// `nika:inspect` is catalogued but the runtime injects a `NoWorkflow`
/// today — every view returns `available: false`. Say so at check time
/// instead of letting an author discover it after a paid infer wave.
fn push_inspect_unwired_hint(
    hints: &mut Vec<Hint>,
    id: &str,
    a: &nika_schema::raw::RawInvokeAction,
) {
    let Some(tool) = a.tool() else {
        return;
    };
    if tool.value != "nika:inspect" {
        return;
    }
    hints.push(hint(
        "inspect-unwired",
        id,
        format!(
            "`nika:inspect` on `{id}` has no live run context in this engine — every view \
             returns `available: false`. Read cost/DAG from `nika trace show` until the \
             runtime injects WorkflowIntrospect (ADR-088 wiring gap)"
        ),
    ));
}

/// A markdown glob that will also match a README sitting in the same
/// tree. Authors then spend a paid infer wave classifying the README.
fn push_glob_readme_hint(hints: &mut Vec<Hint>, id: &str, a: &nika_schema::raw::RawInvokeAction) {
    let Some(tool) = a.tool() else {
        return;
    };
    if tool.value != "nika:glob" {
        return;
    }
    let Some(args) = a.args.as_ref() else {
        return;
    };
    let Some(pattern) = args.value.get("pattern").and_then(|v| v.as_str()) else {
        return;
    };
    if pattern.contains("${{") {
        return;
    }
    let md_glob = std::path::Path::new(pattern)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
    if !md_glob {
        return;
    }
    let excluded = match args.value.get("exclude") {
        Some(serde_json::Value::String(s)) => s.to_ascii_lowercase().contains("readme"),
        Some(serde_json::Value::Array(list)) => list.iter().any(|v| {
            v.as_str()
                .is_some_and(|s| s.to_ascii_lowercase().contains("readme"))
        }),
        _ => false,
    };
    if excluded {
        return;
    }
    hints.push(hint(
        "glob-readme",
        id,
        format!(
            "`nika:glob` on `{id}` matches `{pattern}` — that set includes a README \
             sitting in the same directory. Pin the notes (`exclude: \"**/README.md\"` \
             or glob a folder that has no README) before a paid infer classifies the \
             table of contents as a record"
        ),
    ));
}

/// `. as $c` then a bare `map(` maps the CURRENT value (often a pair),
/// not `$c`. The jury-jq class: `($c | map(...))`.
fn push_jq_as_map_hint(hints: &mut Vec<Hint>, id: &str, a: &nika_schema::raw::RawInvokeAction) {
    let Some(tool) = a.tool() else {
        return;
    };
    if tool.value != "nika:jq" {
        return;
    }
    let Some(expr) = a
        .args
        .as_ref()
        .and_then(|args| args.value.get("expression"))
        .and_then(|v| v.as_str())
    else {
        return;
    };
    if !jq_maps_the_current_after_bind(expr) {
        return;
    }
    hints.push(hint(
        "jq-as-map",
        id,
        format!(
            "`nika:jq` on `{id}` binds `. as $name` then calls `map(` on the current \
             value — after a later construct the current value is often a pair, not \
             the bound array. Write `($name | map(...))`"
        ),
    ));
}

/// A failed last `nika:assert` quarantines writes to
/// `.nika/quarantine/<trace>/`. Say so when the DAG both writes and
/// asserts — authors hunt an empty `out/` otherwise.
fn push_assert_quarantine_hint(hints: &mut Vec<Hint>, wf: &RawWorkflow) {
    let mut asserts = Vec::new();
    let mut writes = false;
    for task in &wf.tasks {
        let RawAction::Invoke(a) = &task.value.action else {
            continue;
        };
        let Some(tool) = a.tool() else {
            continue;
        };
        match tool.value.as_str() {
            "nika:assert" => asserts.push(task.value.id.value.as_str()),
            "nika:write" | "nika:edit" | "nika:chart" | "nika:emit" => writes = true,
            _ => {}
        }
    }
    if !writes {
        return;
    }
    for id in asserts {
        hints.push(hint(
            "assert-quarantine",
            id,
            format!(
                "`nika:assert` on `{id}` shares the DAG with a write — a red assert \
                 moves `out/` into `.nika/quarantine/<trace>/`. Look there before \
                 assuming the write never happened; keep the assert off the last \
                 wave if you need the artifacts from a red run"
            ),
        ));
    }
}

/// An `infer:` that asks the model to name the verdict (belt · level ·
/// score). The cheaper one-way is facts-then-law (`13-extract-then-law`).
/// A prompt that *forbids* the assignment (`never assign a belt`) is
/// the extract shape and stays silent.
fn push_infer_as_law_hint(hints: &mut Vec<Hint>, id: &str, a: &nika_schema::raw::RawInferAction) {
    let prompt = a.prompt.value.as_str();
    let system = a.system.as_ref().map_or("", |s| s.value.as_str());
    if !asks_model_to_name_the_law(prompt) && !asks_model_to_name_the_law(system) {
        return;
    }
    hints.push(hint(
        "infer-as-law",
        id,
        format!(
            "`{id}` asks the model to name a belt/level/score — that is the law, \
             not a fact. Extract integer facts (`type: integer` + numeric enum), \
             then `nika:jq` or `nika:decide`. Shape: `nika try 13-extract-then-law`"
        ),
    ));
}

const LAW_PHRASES: &[&str] = &[
    "assign the belt",
    "assign a belt",
    "assign its belt",
    "pick the level",
    "pick a level",
    "choose the level",
    "name the belt",
    "name the level",
    "score the level",
    "which belt",
    "which level",
    "give it a belt",
    "give it a level",
];

fn asks_model_to_name_the_law(text: &str) -> bool {
    let p = text.to_ascii_lowercase();
    LAW_PHRASES.iter().any(|ph| {
        let Some(i) = p.find(ph) else {
            return false;
        };
        let clause = p[..i].rsplit(['.', '\n', ';']).next().unwrap_or(&p[..i]);
        !clause.contains("never")
            && !clause.contains("do not")
            && !clause.contains("don't")
            && !clause.contains("not ")
    })
}

/// `. as $c | map(...)` maps the *current* value. `($c | map(...))` is
/// the one-way. A one-liner used to slip past the line-start detector.
fn jq_maps_the_current_after_bind(expr: &str) -> bool {
    let names = bound_jq_names(expr);
    if names.is_empty() || !expr.contains("map(") {
        return false;
    }
    let mut rest = squash_ws(expr);
    for name in &names {
        rest = rest.replace(&format!("(${name} | map("), "");
        rest = rest.replace(&format!("(${name}|map("), "");
    }
    rest.contains("map(")
}

fn bound_jq_names(expr: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = expr;
    while let Some(i) = rest.find(". as $") {
        let after = &rest[i + 6..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() {
            rest = after;
            continue;
        }
        names.push(name.clone());
        rest = after.get(name.len()..).unwrap_or("");
    }
    names
}

/// `infer` → `nika:jq`/`nika:decide` with no const-fixture assert.
/// `is_clean` does not prove the law (tests-first compile · 2026-08-19).
fn push_unproven_law_hints(hints: &mut Vec<Hint>, wf: &RawWorkflow) {
    let infer_ids: BTreeSet<&str> = wf
        .tasks
        .iter()
        .filter_map(|task| match task.value.action {
            RawAction::Infer(_) => Some(task.value.id.value.as_str()),
            _ => None,
        })
        .collect();
    if infer_ids.is_empty() {
        return;
    }
    let mut law = Vec::new();
    let mut prove = Vec::new();
    for task in &wf.tasks {
        let t = &task.value;
        if !is_jq_or_decide(t) {
            continue;
        }
        if task_mentions_infer(t, &infer_ids) {
            law.push(t.id.value.as_str());
        } else {
            prove.push(t.id.value.as_str());
        }
    }
    if law.is_empty() {
        return;
    }
    let asserted = asserted_jq_decide_ids(wf);
    if prove.iter().any(|id| asserted.contains(id)) {
        return;
    }
    for id in law {
        if asserted.contains(id) {
            continue;
        }
        hints.push(hint(
            "unproven-law",
            id,
            format!(
                "`{id}` applies a jq/decide law to an infer extract — `is_clean` \
                 does not prove the law. Feed known facts through the same law \
                 and `nika:assert` (`condition` reads `with.`) against a const \
                 map you computed by hand. Shape: `nika try 13-extract-then-law`"
            ),
        ));
    }
}

fn is_jq_or_decide(t: &RawTask) -> bool {
    let RawAction::Invoke(a) = &t.action else {
        return false;
    };
    a.tool()
        .is_some_and(|tool| tool.value == "nika:jq" || tool.value == "nika:decide")
}

fn task_mentions_infer(t: &RawTask, infer_ids: &BTreeSet<&str>) -> bool {
    t.with
        .iter()
        .any(|(_, v)| value_mentions_tasks(&v.value, infer_ids))
        || invoke_args(t).is_some_and(|args| value_mentions_tasks(args, infer_ids))
}

fn invoke_args(t: &RawTask) -> Option<&serde_json::Value> {
    match &t.action {
        RawAction::Invoke(a) => a.args.as_ref().map(|s| &s.value),
        _ => None,
    }
}

fn asserted_jq_decide_ids(wf: &RawWorkflow) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    let mut jq_decide: BTreeSet<&str> = BTreeSet::new();
    for task in &wf.tasks {
        if is_jq_or_decide(&task.value) {
            jq_decide.insert(task.value.id.value.as_str());
        }
    }
    for task in &wf.tasks {
        let t = &task.value;
        let RawAction::Invoke(a) = &t.action else {
            continue;
        };
        let Some(tool) = a.tool() else {
            continue;
        };
        if tool.value != "nika:assert" {
            continue;
        }
        let cond = a
            .args
            .as_ref()
            .and_then(|args| args.value.get("condition"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if !cond.contains("with.") {
            continue;
        }
        for (_, v) in &t.with {
            for id in task_ids_in_value(&v.value) {
                if let Some(&hit) = jq_decide.get(id.as_str()) {
                    out.insert(hit);
                }
            }
        }
    }
    out
}

fn task_ids_in_value(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::String(s) => {
            let mut out = Vec::new();
            let mut rest = s.as_str();
            while let Some(i) = rest.find("tasks.") {
                let after = &rest[i + 6..];
                let name: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if name.is_empty() {
                    rest = after;
                    continue;
                }
                out.push(name.clone());
                rest = after.get(name.len()..).unwrap_or("");
            }
            out
        }
        serde_json::Value::Object(map) => map.values().flat_map(task_ids_in_value).collect(),
        serde_json::Value::Array(arr) => arr.iter().flat_map(task_ids_in_value).collect(),
        _ => Vec::new(),
    }
}

fn value_mentions_tasks(v: &serde_json::Value, ids: &BTreeSet<&str>) -> bool {
    task_ids_in_value(v)
        .iter()
        .any(|id| ids.contains(id.as_str()))
}

fn squash_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut gap = false;
    for c in s.chars() {
        if c.is_whitespace() {
            gap = true;
        } else {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        }
    }
    out
}

fn hint(kind: &'static str, task: &str, advice: String) -> Hint {
    Hint {
        kind,
        task: task.to_owned(),
        advice,
    }
}
#[cfg(test)]
mod tests;
