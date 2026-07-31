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
//! - **exec the run will refuse** (`exec-floor`) — emitted by the
//!   `check/exec_floor.rs` mirror (P0-13): an argv-form command whose
//!   interpreter inline-eval flag or subcommand the runtime's exec
//!   floor refuses positionally — the check predicts the refusal the
//!   run would apply (script file or `pre_validated` instead).
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
    /// `dead-spend` ·
    /// `typing` · `permits` · `strictness` · `schema-portability` ·
    /// `redundant-gate` · `retry-effects` ·
    /// `secrets-store` · `native-first` · `exec-floor` ·
    /// `exec-json-capture` ·
    /// `unwrapped-ref` · `envelope-output` · `policy-soft` · `run-clock`
    /// · `analysis` · `consent` (additive · agents route on it; the
    /// module doc describes each).
    /// `parallel-writers` is RETIRED (F-P15 · promoted to the
    /// NIKA-SEC-012 finding — an error owns its repair, never a hint).
    pub kind: &'static str,
    /// The task it concerns (`-` for workflow-level hints).
    pub task: String,
    /// What to change and what it unlocks.
    pub advice: String,
}

/// Compute the improvement hints for a workflow.
#[must_use]
pub(super) fn scan_hints(wf: &RawWorkflow) -> Vec<Hint> {
    let consumed = consumed_outputs(wf);
    let deep_referenced = deeply_referenced(wf);
    let envelope_bound = envelope_bound_outputs(wf);
    let envelope_ids: BTreeSet<&str> = envelope_bound.iter().map(|(_, id)| id.as_str()).collect();
    let mut hints = Vec::new();
    // SOFT policy families (spec 10) — a constraint that cannot be judged
    // must never look judged: the hint records them, nothing reads them.
    if let Some(p) = &wf.policy
        && p.value.has_soft_families()
    {
        hints.push(hint(
            "policy-soft",
            "-",
            "soft policy recorded · not judged (v1) — `prefer:`/`optimize:` record \
             intent; no v1 judge reads them (spec 10 §policy)"
                .to_owned(),
        ));
    }
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
            }
            RawAction::Exec(exec) => {
                push_exec_json_capture_hint(&mut hints, t, exec);
            }
            RawAction::Invoke(a) => push_headless_prompt_hint(&mut hints, id, a),
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
        && t.output.is_empty()
    {
        hints.push(hint("typing", id, format!(
            "deep references into `tasks.{id}.output.<field>` exist but `{id}` declares no output shape — declare `returns:` and `nika check` starts proving those field names"
        )));
    }
    push_strictness_hint(hints, id, a.schema.as_ref().map(|s| &s.value));
    push_portability_hint(hints, id, a.schema.as_ref().map(|s| &s.value));
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
        for (_, binding) in &task.value.output {
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
    let parses_stdout_json = task.output.iter().any(|(_, binding)| {
        let compact: String = binding
            .value
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        compact.contains(".stdout|fromjson")
    });
    // Another binding consuming the structured record's OTHER fields means
    // `structured` is the point, not an accident.
    let reads_record_fields = task.output.iter().any(|(_, binding)| {
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
fn push_strictness_hint(hints: &mut Vec<Hint>, id: &str, schema: Option<&serde_json::Value>) {
    if schema.is_some_and(has_open_object) {
        hints.push(hint("strictness", id, format!(
            "`{id}`'s schema admits undeclared keys — add `additionalProperties: false` to its object nodes for a deterministic output shape across providers"
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

fn hint(kind: &'static str, task: &str, advice: String) -> Hint {
    Hint {
        kind,
        task: task.to_owned(),
        advice,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn hints_of(yaml: &str) -> Vec<Hint> {
        scan_hints(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
    }

    #[test]
    fn a_structured_capture_nobody_branches_on_names_what_it_swallows() {
        // Reported 2026-07-28: a run showed 23/23 green with four tasks
        // failed. Under `structured` a non-zero exit is DATA, so the task
        // succeeds whatever the command returns; the error surfaced three
        // waves later on an unrelated jq and pointed at the wrong task.
        let blind = hints_of(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  exec: true\ntasks:\n  s:\n    exec: { command: [\"false\"], capture: structured }\n",
        );
        let hit = blind
            .iter()
            .find(|h| h.kind == "swallowed-exit")
            .expect("a structured capture nobody branches on names what it swallows");
        assert_eq!(hit.task, "s");
        assert!(
            hit.advice.contains("NIKA-EXEC-001"),
            "the hint names the failure the author gave up: {}",
            hit.advice
        );

        // Reading the branch IS the legitimate use — nothing to say.
        let branched = hints_of(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  exec: true\ntasks:\n  s:\n    exec: { command: [\"false\"], capture: structured }\n  guard:\n    with:\n      code: ${{ tasks.s.output.exit_code }}\n    exec: { command: [\"true\"] }\n",
        );
        assert!(
            !branched.iter().any(|h| h.kind == "swallowed-exit"),
            "a workflow that reads exit_code is using structured on purpose"
        );
    }

    #[test]
    fn a_prompt_without_a_default_names_its_headless_cost() {
        // Reported from Cursor 2026-07-28 (a green audit, then a dead
        // first run) and again live 2026-07-31 (seo-live-review): the
        // oracle was silent about a fact sitting in the file. The hint
        // now teaches the BEHAVIOR (unattended = durable pause) and the
        // two one-command answers, not a code the CLI no longer dies on.
        let bare = hints_of(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  tools: [\"nika:prompt\"]\ntasks:\n  confirm:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"ship?\" }\n",
        );
        let hit = bare
            .iter()
            .find(|h| h.kind == "headless-prompt")
            .expect("a prompt with no default names its headless cost");
        assert_eq!(hit.task, "confirm");
        for lesson in [
            "pauses at this gate",
            "--answer confirm=<value>",
            "`default:`",
        ] {
            assert!(
                hit.advice.contains(lesson),
                "the hint teaches `{lesson}`: {}",
                hit.advice
            );
        }

        // A declared default IS the unattended path — nothing to say.
        let defaulted = hints_of(
            "nika: v1\nworkflow:\n  id: w\npermits:\n  tools: [\"nika:prompt\"]\ntasks:\n  confirm:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"ship?\", default: false }\n",
        );
        assert!(
            !defaulted.iter().any(|h| h.kind == "headless-prompt"),
            "a declared default silences the hint"
        );
    }

    #[test]
    fn non_tightening_after_terminal_beside_value_edge_is_redundant() {
        // one-obvious-way/008 — `after: {a: terminal}` beside a value
        // edge to `a` composes to the value edge's own pass-set:
        // {success, skipped} ∩ terminal changes nothing.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    exec: { shell: \"true\" }\n  b:\n    after: { a: terminal }\n    with: { data: \"${{ tasks.a.output }}\" }\n    exec: { shell: \"true\" }\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "redundant-gate" && x.task == "b")
            .expect("the /008 hint fires");
        assert!(hit.advice.contains("tighten to `success`"), "{hit:?}");
    }

    #[test]
    fn tightening_after_success_beside_value_edge_is_meaningful() {
        // `success` NARROWS the composed gate ({success, skipped} ∩
        // {success} = {success} — the skipped-null case is excluded), so
        // the restatement is meaningful; the spec's own tightened form
        // (conformance dag-topology/009) must never be flagged.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    exec: { shell: \"true\" }\n  b:\n    after: { a: success }\n    with: { data: \"${{ tasks.a.output }}\" }\n    exec: { shell: \"true\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "redundant-gate"), "{h:?}");
    }

    #[test]
    fn after_terminal_without_a_value_edge_is_not_flagged() {
        // The two legitimate terminal shapes stay silent · the pure
        // always-pattern (no binding at all) and the report pattern
        // (terminal + a `.status` OBSERVATION — not a value edge, the
        // pairing the spec itself teaches in 03 §after).
        let always = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    exec: { shell: \"true\" }\n  b:\n    after: { a: terminal }\n    exec: { shell: \"true\" }\n",
        );
        assert!(
            !always.iter().any(|x| x.kind == "redundant-gate"),
            "{always:?}"
        );
        let report = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    exec: { shell: \"true\" }\n  b:\n    after: { a: terminal }\n    with: { outcome: \"${{ tasks.a.status }}\" }\n    exec: { shell: \"true\" }\n",
        );
        assert!(
            !report.iter().any(|x| x.kind == "redundant-gate"),
            "{report:?}"
        );
    }

    #[test]
    fn unbounded_infer_gets_a_cost_hint() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\" }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(h.iter().any(|x| x.kind == "cost" && x.task == "a"), "{h:?}");
    }

    #[test]
    fn unconsumed_infer_is_dead_spend() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    exec: { shell: \"echo done\" }\n",
        );
        assert!(h.iter().any(|x| x.kind == "dead-spend" && x.task == "a"));
        // consumed via outputs: → no dead-spend hint
        let h2 = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(!h2.iter().any(|x| x.kind == "dead-spend"), "{h2:?}");
    }

    /// The first-day trap, taught where it is born: `outputs.r: ${{
    /// tasks.a }}` binds the ENVELOPE — the hint names the output, the
    /// task, the drift, and the fix; the contradictory dead-spend voice
    /// (« nothing consumes it ») is suppressed for that task.
    #[test]
    fn envelope_bound_output_teaches_and_silences_dead_spend() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a }}\n",
        );
        let env: Vec<_> = h.iter().filter(|x| x.kind == "envelope-output").collect();
        assert_eq!(env.len(), 1, "{h:?}");
        assert_eq!(env[0].task, "a");
        assert!(env[0].advice.contains("outputs.r"), "{}", env[0].advice);
        assert!(
            env[0].advice.contains("${{ tasks.a.output }}"),
            "the fix is spelled: {}",
            env[0].advice
        );
        assert!(
            !h.iter().any(|x| x.kind == "dead-spend"),
            "one voice — the envelope binding IS consumption: {h:?}"
        );
        // A bare envelope in a GATE is plumbing, not a trap — silent.
        let gate = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    with: { a_out: \"${{ tasks.a.output }}\" }\n    when: ${{ size(with.a_out) > 0 }}\n    exec: { shell: \"echo go\" }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(
            !gate.iter().any(|x| x.kind == "envelope-output"),
            "{gate:?}"
        );
    }

    #[test]
    fn deeply_referenced_unschema_d_output_gets_a_typing_hint() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    with: { f: \"${{ tasks.a.output.field }}\" }\n    exec: { shell: \"echo ${{ with.f }}\" }\n",
        );
        assert!(
            h.iter().any(|x| x.kind == "typing" && x.task == "a"),
            "{h:?}"
        );
        // shallow consumption only → no typing hint
        let h2 = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    with: { whole: \"${{ tasks.a.output }}\" }\n    exec: { shell: \"echo ${{ with.whole }}\" }\n",
        );
        assert!(!h2.iter().any(|x| x.kind == "typing"), "{h2:?}");
    }

    #[test]
    fn effectful_without_permits_gets_no_hint_the_error_owns_it() {
        // F-O8: absent + effects is the NIKA-AUTH-006 ERROR (the
        // capability_escapes lane), so THIS lane stays silent — an
        // advisory hint next to the hard finding would double-teach.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    exec: { shell: \"echo hi\" }\n",
        );
        assert!(
            !h.iter().any(|x| x.kind == "permits"),
            "the error owns the absent+effects case: {h:?}"
        );
        // boundary declared → no hint either (unchanged).
        let h2 = hints_of(
            "nika: v1\nworkflow:\n  id: w\npermits: { exec: true }\ntasks:\n  t:\n    exec: { shell: \"echo hi\" }\n",
        );
        assert!(!h2.iter().any(|x| x.kind == "permits"), "{h2:?}");
    }

    #[test]
    fn structured_exec_parsing_stdout_json_gets_capture_hint() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\npermits: { exec: true }\ntasks:\n  crawl:\n    exec:\n      command: [\"node\", \"helper.mjs\"]\n      capture: structured\n    output:\n      crawl: \".stdout | fromjson\"\n      url: \".stdout | fromjson | .url\"\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "exec-json-capture" && x.task == "crawl")
            .expect("capture hint");
        assert!(hit.advice.contains("capture: stdout"), "{hit:?}");
        assert!(hit.advice.contains("exit_code"), "{hit:?}");

        let intentional = hints_of(
            "nika: v1\nworkflow:\n  id: w\npermits: { exec: true }\ntasks:\n  probe:\n    exec:\n      command: [\"false\"]\n      capture: structured\n    output:\n      exit_code: \".exit_code\"\n",
        );
        assert!(
            !intentional.iter().any(|x| x.kind == "exec-json-capture"),
            "{intentional:?}"
        );

        // The MIXED task — one binding parses stdout JSON, ANOTHER branches on
        // exit_code. `structured` is the point (switching would break `ok`);
        // the hint must stay silent (Gate-11 review: the any-vs-all misfire).
        let mixed = hints_of(
            "nika: v1\nworkflow:\n  id: w\npermits: { exec: true }\ntasks:\n  health:\n    exec:\n      command: [\"curl\", \"-s\", \"https://api.example/health\"]\n      capture: structured\n    output:\n      body: \".stdout | fromjson\"\n      ok: \".exit_code == 0\"\n",
        );
        assert!(
            !mixed.iter().any(|x| x.kind == "exec-json-capture"),
            "{mixed:?}"
        );

        // Substring lookalike — the binding CONTAINS both `.stdout` and
        // `fromjson` (the old independent-substring predicate fired) but they
        // never form the `.stdout | fromjson` chain; no hint.
        let lookalike = hints_of(
            "nika: v1\nworkflow:\n  id: w\npermits: { exec: true }\ntasks:\n  diag:\n    exec:\n      command: [\"node\", \"diag.mjs\"]\n      capture: structured\n    output:\n      log: \".raw | fromjson | .stdout_field\"\n",
        );
        assert!(
            !lookalike.iter().any(|x| x.kind == "exec-json-capture"),
            "{lookalike:?}"
        );
    }

    #[test]
    fn consumption_inside_nested_with_json_counts() {
        // the output is consumed inside a NESTED `with:` JSON value —
        // the visit_json walker path; with it blinded, a phantom
        // dead-spend hint would fire here.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\n  b:\n    with: { payload: { content: \"${{ tasks.a.output }}\" } }\n    invoke: { tool: \"nika:write\", args: { path: \"./o\", content: \"${{ with.payload }}\" } }\n",
        );
        assert!(
            !h.iter().any(|x| x.kind == "dead-spend"),
            "consumed via nested with JSON: {h:?}"
        );
    }

    #[test]
    fn pure_compute_hints_stay_silent_in_this_lane() {
        // infer-only → no permits hint HERE (the legal-zero hint for
        // absent + pure compute is stamped by `check()` itself, which
        // sees the escape scan — this lane's half of the F-O8 split).
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer: { prompt: \"x\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(!h.iter().any(|x| x.kind == "permits"), "{h:?}");
    }

    #[test]
    fn open_object_schema_gets_the_strictness_hint() {
        // properties declared but additionalProperties unclosed → the
        // model can emit undeclared keys → shape varies across providers.
        let open = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          s: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(
            open.iter().any(|h| h.kind == "strictness" && h.task == "a"),
            "{open:?}"
        );
        // closed at every object node → no hint
        let closed = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          s: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(!closed.iter().any(|h| h.kind == "strictness"), "{closed:?}");
    }

    #[test]
    fn nested_open_object_is_found_one_hint_per_task() {
        // the root is closed but a nested items-object is open — still
        // hinted, and only ONCE for the task.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          tags:\n            type: array\n            items:\n              type: object\n              properties:\n                name: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert_eq!(
            h.iter().filter(|x| x.kind == "strictness").count(),
            1,
            "{h:?}"
        );
    }

    // ─── schema-portability hint · grammar-blind keywords ─────────────

    #[test]
    fn grammar_blind_keywords_get_the_portability_hint() {
        // uniqueItems:true + not — every provider wire ACCEPTS this
        // schema and no grammar enforces either keyword (llama.cpp +
        // ollama proven live 2026-07-07); the hint names the local-
        // validation-only reality, once per task, listing both.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          tags:\n            type: array\n            uniqueItems: true\n            items:\n              type: string\n              not: { enum: [forbidden] }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "schema-portability")
            .expect("hint");
        assert_eq!(hit.task, "a");
        assert!(
            hit.advice.contains("`uniqueItems`") && hit.advice.contains("`not`"),
            "{hit:?}"
        );
        assert_eq!(
            h.iter().filter(|x| x.kind == "schema-portability").count(),
            1,
            "one hint per task: {h:?}"
        );
    }

    #[test]
    fn conditional_family_flags_only_when_it_binds() {
        // `if` + `then` binds → hinted; a bare `if` without then/else
        // constrains nothing anywhere — not even locally — so no claim.
        let bound = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          x: { type: string }\n        if:\n          properties:\n            x: { const: a }\n        then:\n          required: [x]\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(
            bound
                .iter()
                .any(|x| x.kind == "schema-portability" && x.advice.contains("`if/then/else`")),
            "{bound:?}"
        );
        let bare = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          x: { type: string }\n        if:\n          required: [x]\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(
            !bare.iter().any(|x| x.kind == "schema-portability"),
            "{bare:?}"
        );
    }

    #[test]
    fn portability_hint_reads_keywords_not_property_names() {
        // a property NAMED `not` + `uniqueItems: false` (the default,
        // binds nothing) → silence; the walker reads keys only at
        // schema-node positions.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: false\n        properties:\n          not: { type: string }\n          tags:\n            type: array\n            uniqueItems: false\n            items: { type: string }\noutputs:\n  r: ${{ tasks.a.output }}\n",
        );
        assert!(!h.iter().any(|x| x.kind == "schema-portability"), "{h:?}");
    }

    #[test]
    fn schema_d_task_gets_no_typing_hint() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          field: { type: string }\n  b:\n    with: { f: \"${{ tasks.a.output.field }}\" }\n    exec: { shell: \"echo ${{ with.f }}\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "typing"), "{h:?}");
    }

    #[test]
    fn retried_exec_warns_at_least_once_semantics() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  deploy:\n    retry: { max_attempts: 3 }\n    exec: { shell: \"./deploy.sh\" }\n",
        );
        let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
        assert_eq!(hit.task, "deploy");
        assert!(hit.advice.contains("at-least-once"), "{hit:?}");
    }

    #[test]
    fn retried_mcp_tool_warns_no_idempotency_contract() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  post:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: mcp:slack/send\n      args: { text: \"hi\" }\n",
        );
        let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
        assert!(hit.advice.contains("mcp:slack/send"), "{hit:?}");
    }

    #[test]
    fn retried_notify_webhook_warns_duplicate_side_effect() {
        // P0-17 — `nika:notify` is a webhook send (nika-builtin defs.rs):
        // NOT idempotent, so a retry can deliver the notification twice.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  alert:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: nika:notify\n      args: { target: \"https://hooks.example.com/x\", message: \"boom\" }\n",
        );
        let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
        assert_eq!(hit.task, "alert");
        assert!(hit.advice.contains("nika:notify"), "{hit:?}");
    }

    #[test]
    fn retried_fetch_post_warns_without_idempotency_key() {
        // P0-17 — `nika:fetch` accepts POST (defs.rs · http.rs: « POST must
        // pair an idempotency key »): a bare retried POST replays the
        // request's side effects.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  post:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/items\", method: POST, body: { a: 1 } }\n",
        );
        let hit = h.iter().find(|x| x.kind == "retry-effects").expect("hint");
        assert_eq!(hit.task, "post");
        assert!(hit.advice.contains("POST"), "{hit:?}");
    }

    #[test]
    fn retried_fetch_post_with_idempotency_key_makes_no_claim() {
        // The declared `idempotency-key` header discharges the hazard —
        // the receiver can dedup the replay.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  post:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: nika:fetch\n      args:\n        url: \"https://api.example.com/items\"\n        method: POST\n        headers: { idempotency-key: \"${{ inputs.order_id }}\" }\n        body: { a: 1 }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "retry-effects"), "{h:?}");
    }

    #[test]
    fn retried_fetch_get_makes_no_claim() {
        // GET (explicit OR the default) is idempotent — retrying it
        // replays nothing (http.rs: « GET idempotent »).
        let explicit = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  probe:\n    retry: { max_attempts: 3 }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/health\", method: GET }\n",
        );
        assert!(
            !explicit.iter().any(|x| x.kind == "retry-effects"),
            "{explicit:?}"
        );
        let defaulted = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  probe:\n    retry: { max_attempts: 3 }\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://api.example.com/health\" }\n",
        );
        assert!(
            !defaulted.iter().any(|x| x.kind == "retry-effects"),
            "{defaulted:?}"
        );
    }

    #[test]
    fn retried_write_makes_no_claim() {
        // `nika:write` is an atomic temp+rename overwrite — the replay
        // lands the SAME final bytes: idempotent, no hint.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  save:\n    retry: { max_attempts: 2 }\n    invoke:\n      tool: nika:write\n      args: { path: out.md, content: \"x\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "retry-effects"), "{h:?}");
    }

    #[test]
    fn retry_on_contracted_effects_makes_no_claim() {
        // infer retries re-spend tokens (covered by cost) · `nika:write`
        // is a documented atomic overwrite (idempotent — P0-17 narrowed
        // this test's blanket « nika: builtins are idempotent » claim:
        // `nika:notify` and non-GET `nika:fetch` are NOT, the two tests
        // above pin their hint) · max_attempts 1 is no retry at all —
        // none of these hint.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  ask:\n    retry: { max_attempts: 3 }\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n  save:\n    retry: { max_attempts: 3 }\n    with: { content: \"${{ tasks.ask.output }}\" }\n    invoke:\n      tool: nika:write\n      args: { path: out.md, content: \"${{ with.content }}\" }\n  once:\n    retry: { max_attempts: 1 }\n    after: { save: success }\n    exec: { shell: \"true\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "retry-effects"), "{h:?}");
    }

    // ─── secrets-store hint pipeline ───────────────────────────────────
    // push_unresolvable_secret_hints → referenced_secrets → task_text_fields
    //   → collect_json_strings_into. These functions are exercised both
    //   behaviorally (through scan_hints) and as units below.

    fn wf_of(yaml: &str) -> RawWorkflow {
        parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse")
    }

    #[test]
    fn referenced_vault_secret_gets_the_secrets_store_hint() {
        // a vault-source secret that IS referenced via `${{ secrets.FOO }}`
        // in a task field → push_unresolvable_secret_hints must emit a
        // `secrets-store` hint naming FOO. Kills:
        //   - push_unresolvable_secret_hints → () (no hint at all)
        //   - referenced_secrets → {} / {""} / {"xyzzy"} (FOO not in set →
        //     the `referenced.contains(name)` guard fails → no hint)
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nsecrets:\n  FOO:\n    source: vault\n    key: prod/foo\ntasks:\n  t:\n    exec: { shell: \"echo ${{ secrets.FOO }}\" }\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "secrets-store")
            .expect("secrets-store hint");
        assert_eq!(hit.task, "-");
        assert!(hit.advice.contains("secrets.FOO"), "{hit:?}");
    }

    #[test]
    fn unreferenced_vault_secret_gets_no_hint() {
        // declared-but-unused vault secret is harmless — the hint fires
        // ONLY for a referenced secret. If referenced_secrets returned a
        // spurious {"FOO"} (the from_iter(["xyzzy"]) family with a
        // matching name would not, but a hardcoded set could) this would
        // also catch over-collection.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nsecrets:\n  FOO:\n    source: vault\n    key: prod/foo\ntasks:\n  t:\n    exec: { shell: \"echo hi\" }\n",
        );
        assert!(!h.iter().any(|x| x.kind == "secrets-store"), "{h:?}");
    }

    #[test]
    fn referenced_secrets_collects_exactly_the_referenced_names() {
        // Direct unit on referenced_secrets — FOO referenced in a prompt,
        // BAR referenced in an output, BAZ declared but never referenced.
        // The returned set must be exactly {FOO, BAR}. Kills the
        // referenced_secrets → BTreeSet::new() / from_iter([""]) /
        // from_iter(["xyzzy"]) mutations precisely (wrong cardinality OR
        // wrong contents).
        let wf = wf_of(
            "nika: v1\nworkflow:\n  id: w\nsecrets:\n  FOO:\n    source: vault\n    key: a\n  BAR:\n    source: vault\n    key: b\n  BAZ:\n    source: vault\n    key: c\ntasks:\n  t:\n    infer: { prompt: \"use ${{ secrets.FOO }}\", max_tokens: 10 }\noutputs:\n  r: ${{ secrets.BAR }}\n",
        );
        let refs = referenced_secrets(&wf);
        let got: Vec<&str> = refs.iter().map(String::as_str).collect();
        assert_eq!(got, vec!["BAR", "FOO"], "BTreeSet is sorted");
    }

    #[test]
    fn referenced_secrets_empty_when_none_referenced() {
        // No `${{ secrets.X }}` island anywhere → empty set. This is the
        // baseline the from_iter([""]) / from_iter(["xyzzy"]) mutations
        // violate (they would return a non-empty set here).
        let wf = wf_of(
            "nika: v1\nworkflow:\n  id: w\nsecrets:\n  FOO:\n    source: vault\n    key: a\ntasks:\n  t:\n    exec: { shell: \"echo plain\" }\n",
        );
        assert!(referenced_secrets(&wf).is_empty());
    }

    #[test]
    fn secret_referenced_only_in_task_field_is_found() {
        // Isolates task_text_fields: the secret appears ONLY inside a task
        // field (the infer prompt), NEVER in outputs. If task_text_fields
        // returns vec![] / vec![""] / vec!["xyzzy"], the prompt island is
        // never scanned → FOO is absent → the secrets-store hint vanishes.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\nsecrets:\n  FOO:\n    source: vault\n    key: a\ntasks:\n  t:\n    infer: { prompt: \"call with ${{ secrets.FOO }}\", max_tokens: 10 }\noutputs:\n  r: ${{ tasks.t.output }}\n",
        );
        assert!(
            h.iter()
                .any(|x| x.kind == "secrets-store" && x.advice.contains("secrets.FOO")),
            "{h:?}"
        );
    }

    #[test]
    fn task_text_fields_collects_every_action_text_surface() {
        // Direct unit on task_text_fields across the action variants +
        // `with:`. Kills task_text_fields → vec![] / vec![""] /
        // vec!["xyzzy"] (the real surfaces are none of those) and confirms
        // the exec/invoke/infer/agent + with arms each contribute.

        // exec: command + stdin + env values
        let exec = wf_of(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    exec:\n      shell: \"run CMD\"\n      stdin: \"STDIN\"\n      env: { K: \"ENVVAL\" }\n",
        );
        let f = task_text_fields(&exec.tasks[0].value);
        assert!(f.contains(&"run CMD"), "{f:?}");
        assert!(f.contains(&"STDIN"), "{f:?}");
        assert!(f.contains(&"ENVVAL"), "{f:?}");

        // infer: prompt + system
        let infer = wf_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: \"PROMPT\", system: \"SYSTEM\", max_tokens: 10 }\n",
        );
        let f = task_text_fields(&infer.tasks[0].value);
        assert!(f.contains(&"PROMPT") && f.contains(&"SYSTEM"), "{f:?}");

        // agent: prompt + system
        let agent = wf_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    agent: { prompt: \"APROMPT\", system: \"ASYSTEM\", max_tokens_total: 10 }\n",
        );
        let f = task_text_fields(&agent.tasks[0].value);
        assert!(f.contains(&"APROMPT") && f.contains(&"ASYSTEM"), "{f:?}");

        // invoke args JSON strings + with JSON strings
        let invoke = wf_of(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  t:\n    with: { wkey: \"WITHVAL\" }\n    invoke: { tool: \"nika:write\", args: { path: \"ARGVAL\" } }\n",
        );
        let f = task_text_fields(&invoke.tasks[0].value);
        assert!(f.contains(&"ARGVAL"), "invoke args string: {f:?}");
        assert!(f.contains(&"WITHVAL"), "with value string: {f:?}");
    }

    #[test]
    fn collect_json_strings_into_gathers_all_nested_string_leaves() {
        // Direct unit on collect_json_strings_into. Feed
        // {"a":"x","b":["y",{"c":"z"}]} → ALL of x,y,z must be collected.
        //   - String arm deleted → top-level "x" (and nested) dropped
        //   - Array arm deleted → "y" and the object under it dropped
        //   - Object arm deleted → "z" (object inside array) + "x"/"y"
        //     (top object) dropped
        //   - whole fn → () → nothing collected
        let value = serde_json::json!({ "a": "x", "b": ["y", { "c": "z" }] });
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        out.sort_unstable();
        assert_eq!(out, vec!["x", "y", "z"], "all nested leaves: {out:?}");
    }

    #[test]
    fn collect_json_strings_into_array_arm_descends() {
        // Targeted at the Array match arm: a top-level array of strings.
        // Deleting the Array arm drops both leaves; the String/Object arms
        // alone cannot reach them.
        let value = serde_json::json!(["one", "two"]);
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        out.sort_unstable();
        assert_eq!(out, vec!["one", "two"], "{out:?}");
    }

    #[test]
    fn collect_json_strings_into_object_arm_descends() {
        // Targeted at the Object match arm: a flat object. Deleting the
        // Object arm drops the leaf entirely.
        let value = serde_json::json!({ "k": "deep" });
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        assert_eq!(out, vec!["deep"], "{out:?}");
    }

    #[test]
    fn collect_json_strings_into_string_arm_pushes_the_leaf() {
        // Targeted at the String match arm: a bare string value. Deleting
        // the String arm drops it; the `_ => {}` catch-all would swallow it.
        let value = serde_json::json!("bare");
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        assert_eq!(out, vec!["bare"], "{out:?}");
    }

    #[test]
    fn collect_json_strings_into_ignores_non_string_scalars() {
        // numbers/bools/null contribute nothing (the `_ => {}` arm). This
        // pins the boundary the deleted-arm mutants must not cross.
        let value = serde_json::json!({ "n": 1, "b": true, "z": null, "s": "keep" });
        let mut out = Vec::new();
        collect_json_strings_into(&value, &mut out);
        assert_eq!(out, vec!["keep"], "{out:?}");
    }

    #[test]
    fn secret_referenced_inside_invoke_args_json_is_found() {
        // End-to-end: a secret reachable ONLY through the invoke-args JSON
        // walk (collect_json_strings_into via task_text_fields). With any
        // of the collect arms blinded, FOO is never seen → no hint.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nsecrets:\n  FOO:\n    source: vault\n    key: a\ntasks:\n  t:\n    invoke: { tool: \"nika:write\", args: { path: \"./o\", content: \"${{ secrets.FOO }}\" } }\n",
        );
        assert!(
            h.iter()
                .any(|x| x.kind == "secrets-store" && x.advice.contains("secrets.FOO")),
            "{h:?}"
        );
    }

    #[test]
    fn unwrapped_output_ref_is_hinted_wrapped_is_silent() {
        // Output gauntlet (2026-07-11): a bare `tasks.X.output…` output
        // value is the LITERAL STRING (the run returns the path text, not
        // the value) — hint the wrap. The pattern is distinctive across
        // the five reference namespaces.
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\ntasks:\n  data:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: { count: 42 } } }\noutputs:\n  just_count: tasks.data.output.count\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "unwrapped-ref")
            .unwrap_or_else(|| panic!("expected unwrapped-ref: {h:?}"));
        assert_eq!(hit.task, "just_count");
        assert!(
            hit.advice.contains("literal string")
                && hit.advice.contains("${{ tasks.data.output.count }}"),
            "{}",
            hit.advice
        );

        // A properly wrapped output is SILENT (the common correct case).
        let wrapped = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\ntasks:\n  data:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: { count: 42 } } }\noutputs:\n  just_count: ${{ tasks.data.output.count }}\n",
        );
        assert!(
            !wrapped.iter().any(|x| x.kind == "unwrapped-ref"),
            "{wrapped:?}"
        );

        // A genuine string constant that is NOT a namespace path is silent.
        let plain = hints_of(
            "nika: v1\nworkflow:\n  id: w\nmodel: mock/echo\ntasks:\n  data:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", input: {} } }\noutputs:\n  label: production\n",
        );
        assert!(
            !plain.iter().any(|x| x.kind == "unwrapped-ref"),
            "{plain:?}"
        );
    }

    // ── F-P3 · the run-clock hint (finding (b) · WARN-dur, never a refusal) ──

    #[test]
    fn a_deadline_against_an_undeclared_clock_is_hinted() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  slow:\n    exec: { command: [\"sleep\", \"1\"] }\n    timeout: \"5m\"\n",
        );
        let hit = h
            .iter()
            .find(|x| x.kind == "run-clock")
            .expect("the undeclared-clock hint fires");
        assert_eq!(hit.task, "-", "one workflow-level row");
        assert!(hit.advice.contains("1 task(s)"), "{}", hit.advice);
        assert!(
            hit.advice.contains("run: { clock: system }"),
            "the fix is spelled: {}",
            hit.advice
        );
    }

    #[test]
    fn the_run_clock_hint_stays_silent_when_the_clock_is_named() {
        // Explicit clock: — named.
        let explicit = hints_of(
            "nika: v1\nworkflow:\n  id: w\nrun: { clock: system }\ntasks:\n  slow:\n    exec: { command: [\"sleep\", \"1\"] }\n    timeout: \"5m\"\n",
        );
        assert!(
            !explicit.iter().any(|x| x.kind == "run-clock"),
            "{explicit:?}"
        );
        // Deterministic entropy binds the virtual clock by law — named too.
        let seeded = hints_of(
            "nika: v1\nworkflow:\n  id: w\nrun: { entropy: { seeded: 42 } }\ntasks:\n  slow:\n    exec: { command: [\"sleep\", \"1\"] }\n    timeout: \"5m\"\n",
        );
        assert!(!seeded.iter().any(|x| x.kind == "run-clock"), "{seeded:?}");
        // No deadline at all — nothing to teach.
        let no_timeout = hints_of(
            "nika: v1\nworkflow:\n  id: w\ntasks:\n  fast:\n    exec: { command: [\"true\"] }\n",
        );
        assert!(
            !no_timeout.iter().any(|x| x.kind == "run-clock"),
            "{no_timeout:?}"
        );
    }

    #[test]
    fn the_run_clock_hint_counts_every_deadline_once() {
        let h = hints_of(
            "nika: v1\nworkflow:\n  id: w\nrun: { entropy: ambient }\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n    timeout: \"30s\"\n  b:\n    exec: { command: [\"true\"] }\n    timeout: \"30s\"\n",
        );
        let hits: Vec<_> = h.iter().filter(|x| x.kind == "run-clock").collect();
        assert_eq!(hits.len(), 1, "one deduped row: {hits:?}");
        assert!(hits[0].advice.contains("2 task(s)"), "{}", hits[0].advice);
    }
}
