// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The human render surface of `nika check` — the themed report sections
//! (conformance · plan · models · skills · cost · secrets · types ·
//! tools · args · composition · schema · gates · exec · permits · policy ·
//! trifecta · run · hints) and their row builders. Descended from
//! nika-cli's `verbs::check` 2026-07-29 (the 15k wall · this crate's own
//! precedent — one truth in, text out, no I/O): the render lives beside
//! the theme seam it paints through. The machine (`--json`) surface
//! stays in the CLI's `mod.rs`; both speak the one findings contract.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use nika_check::{CheckReport, ConformanceViolation, UnboundedReason};
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::VarDecl;

use crate::claims::types_claim;
use crate::theme::{Role, Theme};

pub use crate::check_models::{ModelFinding, ModelsAudit};

/// TOOLS/ARGS share this code with the JSON finding fold (`fold_tools`).
const BUILTIN_CONTRACT: &str = "NIKA-BUILTIN-001";

/// Whether the checked bytes have a writable source. The CLI resolves a
/// `registry:` coordinate to a cache path before parsing, so the original
/// provenance must ride separately: a digest-pinned artifact is never a
/// repair target even though its cache entry is a filesystem path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairTarget {
    WorkspaceFile,
    Stdin,
    RegistryArtifact,
    NonRegularSource,
}

/// Section mark: `✔`-class verdict glyphs through the theme seam.
#[must_use]
pub fn mark(theme: Theme, ok: bool) -> String {
    let (uni, asc, role) = if ok {
        ("✔", "ok", Role::Good)
    } else {
        ("✖", "X ", Role::Bad)
    };
    theme.paint(role, if theme.ascii { asc } else { uni })
}

/// One CONFORM finding: the ✖ row, the offline+online fix pointer (the
/// rustc `--explain` move — the same affordance `run` failures print,
/// #145 P2 · one teaching voice on both surfaces), and the source frame
/// when the finding carries a span (rustc-grade caret · the CONFORM row
/// above stays grep-stable).
fn conformance_row(out: &mut String, c: &ConformanceViolation, source: &str, path: &str, t: Theme) {
    // Spec family owns the ladder keyword: `NIKA-PARSE-*` is PARSE even
    // when the analyzer (not `parse()`) emitted the row.
    let gate = if c.code.starts_with("NIKA-PARSE-") {
        "PARSE"
    } else {
        "CONFORM"
    };
    let _ = writeln!(
        out,
        " {} {}  [{}] {}",
        mark(t, false),
        t.paint(Role::Strong, gate),
        c.code,
        c.message
    );
    let _ = writeln!(
        out,
        "   {}",
        t.paint(
            Role::Dim,
            &format!("fix: nika explain {} · {}", c.code, c.docs_url)
        )
    );
    if let Some(span) = c.span {
        let frame = crate::snippet::paint_span(source, path, span, t);
        let _ = writeln!(out, "{frame}");
    }
}

/// Render the human report — every section present, grep-stable keywords.
///
/// `verdict` is THE verdict, computed once by the caller (the CLI folds
/// `report.is_clean()` with the MODELS and SKILLS rungs — both live
/// outside the report). The footer renders it verbatim: it never
/// re-derives a verdict of its own (P0-11 · measured 2026-07-30: a
/// `✖ MODELS` report closed on a green `✔ audited` card while the exit
/// code said 2 and `--json` said `clean: false` — three surfaces, two
/// answers).
#[allow(clippy::too_many_arguments)] // the report's seams, one each — the render.rs:427 precedent
#[must_use]
pub fn render(
    report: &CheckReport,
    wf: &RawWorkflow,
    source: &str,
    path: &str,
    repair_target: RepairTarget,
    t: Theme,
    models_audit: &ModelsAudit,
    skills: &nika_schema::ResolvedSkills,
    drift_hints: &[String],
    verdict: bool,
) -> String {
    let mut out = String::new();
    let name = path.rsplit('/').next().unwrap_or(path);
    let _ = writeln!(
        out,
        "{} {}",
        t.paint(Role::Strong, "nika check"),
        t.paint(Role::Dim, &format!("· {name}"))
    );

    for c in &report.conformance {
        conformance_row(&mut out, c, source, path, t);
    }

    plan(&mut out, report, wf, t);
    crate::check_models::models(&mut out, report, models_audit, t);
    // SKILLS (#473) · silent when nothing is referenced (rows self-teach).
    if let Some((ok_msg, rows)) = skills.rung() {
        section_list(&mut out, t, "SKILLS", &ok_msg, rows);
    }
    cost(&mut out, report, t);
    energy(&mut out, report, t);

    narrowed_rungs(&mut out, report, t);
    composition_rung(&mut out, report, wf, t);
    section_list(
        &mut out,
        t,
        "SCHEMA",
        // "is satisfiable" is a decision this lint does not make. It
        // decides a FAMILY: `required` ∉ `properties`, an unknown `type`
        // name, an empty `enum`, an inverted numeric/length bound, enum
        // values that clash with the declared type. Measured 2026-07-29,
        // both green: `allOf: [{type: string}, {type: integer}]` and
        // `enum: ["x","y"]` beside `const: "z"` — each unsatisfiable,
        // neither in the family. `$ref` is opaque by design (no
        // resolver — never a false claim).
        "no known-unsatisfiable form in an authored schema: · $ref opaque",
        report
            .schema_lints
            .iter()
            .map(|l| format!("task `{}` at {} — {}", l.task, l.path, l.detail))
            .collect(),
    );
    section_or_skip(
        &mut out,
        report,
        t,
        "GATES",
        // The inversion is the whole repair. `reach.rs` PROVES deadness —
        // a gate false under every assignment of an over-approximating
        // status domain — and backs off to "satisfiable" whenever it
        // cannot enumerate (>6 referenced tasks · >256 list items ·
        // non-status atoms → Kleene Unknown). Absence of a proof of death
        // is not a proof of life. Measured 2026-07-29 on one contradiction
        // (`s1=='success' && s1=='failure'`) padded with conjuncts:
        // 6 refs → `✖ NIKA-DAG-006 ... can never run`; 7 refs → the old
        // line's `✔ every task is statically reachable` with `0 hints`.
        "no task proven dead · status literals in vocabulary",
        gate_rows(report),
    );
    writes_rung(&mut out, report, t);
    exec_rung(&mut out, report, t);
    crate::check_laws::order_rung(&mut out, report, t);
    permits(&mut out, report, wf, t);
    trifecta_rung(&mut out, report, wf, t);
    consent_rung(&mut out, report, t);
    crate::check_laws::lift_rung(&mut out, report, wf, t);
    crate::check_journey::journey_rung(&mut out, report, t);
    run_rung(&mut out, report, wf, t);
    hints_and_verdict(
        &mut out,
        report,
        wf,
        path,
        repair_target,
        t,
        drift_hints,
        verdict,
    );
    paint_dag_if_interactive(&mut out, wf, report, t);
    out
}

/// The MAP beside the verdict — the same themed wire art `graph
/// --format ascii` speaks, so the audit READS as the DAG it judged
/// (operator ask 2026-07-12: « quand on fait check, voir la dag »).
/// Interactive surface only; conformance failures skip it (no valid
/// wave order exists to draw).
fn paint_dag_if_interactive(out: &mut String, wf: &RawWorkflow, report: &CheckReport, t: Theme) {
    if t.accents && report.conformance.is_empty() {
        let _ = write!(out, "\n{}", crate::dag_art::ascii_art(wf, report, t));
    }
}

/// The four narrowed rungs — SECRETS · TYPES · TOOLS · ARGS. Each headline
/// claims exactly what its scan covers; the comments carry the measurements
/// that narrowed it.
fn narrowed_rungs(out: &mut String, report: &CheckReport, t: Theme) {
    // Narrowed twice over, and gated on a computable DAG. (1) SCOPE: the
    // IFC engine follows values that originate in a DECLARED `secrets:`
    // entry — a private key read off disk with `nika:read` is not a
    // secret to this lane, and never was. (2) CARVE-OUT: an
    // `infer:`/`agent:` OUTPUT never carries its prompt's taint (ADR-092
    // · flow.rs §4 — a model response is not a verbatim echo). Measured
    // 2026-07-29: `prompt: "Repeat this verbatim: ${{ with.k }}"` →
    // `nika:write out/leak.txt` + `outputs.leaked` printed
    // `✔ SECRETS no information-flow escapes · 0 hints`. The carve-out is
    // a deliberate soundness trade; the UNIVERSAL sentence over it was
    // not.
    let secrets_claim = crate::check_journey::secret_flow_summary(report);
    section_or_skip(
        out,
        report,
        t,
        "SECRETS",
        &secrets_claim,
        secret_rows(report),
    );
    section_list(
        out,
        t,
        "TYPES",
        // Narrowed on purpose — twice. First pass (the comment that
        // stood here): the scan is sound (schema_typing.rs: an opaque
        // shape resolves to "unknown — no finding", never a guess), so
        // the universal sentence died. Second pass (F3 · 2026-07-30):
        // the generic « builtin output has none » read the same on a
        // file WITH deep refs into unshaped outputs as on one without —
        // a vacuous ✔ that dies at run on a missing key. The claim now
        // names THIS file's blind spot (count + refs) when one exists.
        &types_claim(report),
        report
            .schema_findings
            .iter()
            .map(|f| format!("{} (at `{}`) — {}", f.reference, f.site, f.detail))
            .collect(),
    );
    section_list(
        out,
        t,
        "TOOLS",
        // `tools.rs` checks the names a task WRITES: an invoke target, an
        // agent whitelist entry, an `on_finally` cleanup. A glob entry
        // (`nika:*`) is a grant pattern and is skipped, and the `mcp:`
        // namespace is OPEN by design (server-defined, discovered at run).
        // "every nika: tool" covered neither.
        "every named nika: tool is canonical · globs + mcp: not checked",
        unknown_tool_rows(report),
    );
    section_list(
        out,
        t,
        "ARGS",
        // Keyed off the catalog's per-builtin `args` vocabulary, so the
        // claim holds for `nika:` invokes only — `mcp:` args and a
        // `workflow:` target's args are not in that table (spec 14 owns
        // the second, `NIKA-COMP-004`).
        "every builtin invoke arg key is declared + required args present",
        arg_rows(report),
    );
}

/// CONSENT rung (NEP-0020 · P0-2) · silent when no confirm guards an
/// effect — a proven non-affirmative route is NIKA-SEC-014, the same
/// code the `--json` findings[] lane stamps (one voice, one verdict).
fn consent_rung(out: &mut String, report: &CheckReport, t: Theme) {
    if report.consent_findings.is_empty() {
        return;
    }
    section_list(
        out,
        t,
        "CONSENT",
        "every effect behind a confirm crosses an affirmative gate",
        report
            .consent_findings
            .iter()
            .map(|f| format!("[{}] {}", nika_check::ConsentFinding::WIRE_CODE, f.detail))
            .collect(),
    );
}

/// RUN rung (F-P3) · only when the envelope declares `run:` — an absent
/// block is the undeclared status quo and stays SILENT (the existing
/// corpus renders unchanged). The rows are the declaration
/// contradictions, code first (one voice with `--json` findings[]).
fn run_rung(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if wf.run.is_some() {
        section_list(
            out,
            t,
            "RUN",
            // The old line named a CLOCK seam that has no implementation:
            // `run_decl.rs` scans exactly two structural entropy sources
            // (a live `retry:` jitter · a `nika:uuid` named exactly), and
            // only under `entropy: none` — `ambient` and `seeded(N)` are
            // judged not at all, and neither is an `exec` that shells out
            // to `date`. Its own header names the remaining hole: an agent
            // glob whitelist that ADMITS `nika:uuid` without naming it is
            // the undecidable-glob class and stays silent.
            "no named entropy source under entropy: none · agent globs unjudged",
            report
                .run_decl_findings
                .iter()
                .map(|f| {
                    format!(
                        "[{}] task `{}` · {} · fix: {}",
                        f.wire_code(),
                        f.task,
                        f.detail,
                        f.fix
                    )
                })
                .collect(),
        );
    }
}

/// WRITES rung (F-P15 · NEP-0014 law 1) · always present — a universal
/// static law like GATES (parallelism is safe exactly where the writes
/// are provably disjoint).
fn writes_rung(out: &mut String, report: &CheckReport, t: Theme) {
    section_list(
        out,
        t,
        "WRITES",
        // « the same path » alone overreached: the scan proves equality
        // for STATIC keys only (a literal · a bare immutable-authority
        // ref, resolved or identical) — a computed path can still
        // collide at run. Measured 2026-07-30: two unordered writers on
        // the identical `${{ inputs.f }}` — provably the same file,
        // inputs bind once per run — rendered the old green while the
        // literal twin was refused; the scan now catches that class,
        // and the headline names the computed rest it cannot judge.
        "no two unordered tasks write the same static path · computed paths at run",
        report
            .write_conflicts
            .iter()
            .map(|w| format!("[{}] {} · fix: {}", w.wire_code(), w.detail, w.fix))
            .collect(),
    );
}

/// EXEC rung (#605 · NIKA-SEC-001) · always present — the argv floor is
/// an always-on static law like WRITES. The scan judges the SAME
/// predicate the run refuses with (`nika_types::exec::argv_floor_refusal`
/// — one predicate, check ≡ run), so the ✔ names its own blind spot:
/// a `${{ }}`-templated argv is re-judged pre-spawn, never statically
/// claimed.
fn exec_rung(out: &mut String, report: &CheckReport, t: Theme) {
    section_list(
        out,
        t,
        "EXEC",
        "no literal argv the exec floor refuses at run · a templated argv is the RUN's verdict",
        report
            .exec_floor_findings
            .iter()
            .map(|e| {
                format!(
                    "[{}] task `{}` · {} · fix: {}",
                    e.wire_code(),
                    e.task,
                    e.detail,
                    e.fix
                )
            })
            .collect(),
    );
}

/// Declared `inputs:` that the operator MUST pass at run time —
/// `required: true` with no `default:`. The static surface can NAME them
/// (so `check` warns before a bare `run` hits `NIKA-VAR-001`); only the
/// runtime binds them.
#[must_use]
pub fn required_inputs(wf: &RawWorkflow) -> Vec<&str> {
    wf.inputs
        .iter()
        .filter_map(|(name, decl)| match decl {
            VarDecl::Typed {
                required: true,
                default: None,
                ..
            } => Some(name.value.as_str()),
            _ => None,
        })
        .collect()
}

/// The `· fix: did you mean ___?` clause, or empty when no suggestion.
fn fix_clause(suggestion: Option<&str>) -> String {
    suggestion
        .map(|s| format!(" · fix: did you mean `{s}`?"))
        .unwrap_or_default()
}

/// One row per gate-liveness refusal (DAG-006 statically dead · DAG-007
/// out-of-vocabulary status literal) — code first, one-voice.
fn gate_rows(report: &CheckReport) -> Vec<String> {
    report
        .gate_findings
        .iter()
        .map(|g| {
            let fix = g
                .fix
                .as_deref()
                .map(|f| format!(" · fix: {f}"))
                .unwrap_or_default();
            format!(
                "[{}] task `{}` — {}{fix}",
                g.kind.wire_code(),
                g.task,
                g.detail
            )
        })
        .collect()
}

/// One row per `nika:` tool that names no canonical builtin.
fn unknown_tool_rows(report: &CheckReport) -> Vec<String> {
    report
        .unknown_tools
        .iter()
        .map(|u| {
            format!(
                "[{BUILTIN_CONTRACT}] `{}` (task `{}`) is not a canonical builtin{}",
                u.tool,
                u.task,
                fix_clause(u.suggestion.as_deref())
            )
        })
        .collect()
}

/// The ARGS section rows — undeclared arg keys (the typo class) THEN
/// missing required args (the « passed check {} then failed at run » class).
fn arg_rows(report: &CheckReport) -> Vec<String> {
    let mut rows: Vec<String> = report
        .unknown_args
        .iter()
        .map(|u| {
            // With a suggestion the fix is the rename; without one (a
            // wrong-name-entirely miss — `extract` for fetch's `mode`),
            // the closed declared set is the teaching.
            let teach = if u.suggestion.is_some() {
                fix_clause(u.suggestion.as_deref())
            } else {
                format!(" — declared: {}", u.declared.join(" · "))
            };
            format!(
                "[{BUILTIN_CONTRACT}] `{}` (task `{}`) has no `{}` arg{teach}",
                u.tool, u.task, u.arg,
            )
        })
        .collect();
    rows.extend(report.missing_args.iter().map(|m| {
        format!(
            "[{BUILTIN_CONTRACT}] `{}` (task `{}`) is missing required `{}`",
            m.tool, m.task, m.arg
        )
    }));
    rows
}

/// Render every distinct `(identity, advice)` body while returning the
/// smaller set of stable identities used by the verdict count.
fn render_report_hints<'a>(
    out: &mut String,
    report: &'a CheckReport,
    t: Theme,
) -> BTreeSet<&'a str> {
    let mut grouped: BTreeMap<(&str, &str), (usize, BTreeSet<&str>)> = BTreeMap::new();
    for hint in &report.hints {
        let identity = hint.code.unwrap_or(hint.kind);
        let entry = grouped
            .entry((identity, hint.advice.as_str()))
            .or_insert_with(|| (0, BTreeSet::new()));
        entry.0 += 1;
        entry.1.insert(hint.task.as_str());
    }
    for ((identity, advice), (sites, tasks)) in grouped {
        let coded_prefix = format!("{identity} · ");
        let display_advice = advice.strip_prefix(&coded_prefix).unwrap_or(advice);
        let suffix = if sites > 1 {
            format!(
                " · {} across {}",
                crate::vocab::count(sites, "site"),
                crate::vocab::count(tasks.len(), "task")
            )
        } else {
            String::new()
        };
        let _ = writeln!(
            out,
            " {} {}     [{}] {}{}",
            t.paint(Role::Accent, "↳"),
            t.paint(Role::Strong, "HINT"),
            identity,
            display_advice,
            suffix,
        );
    }
    report
        .hints
        .iter()
        .map(|hint| hint.code.unwrap_or(hint.kind))
        .collect()
}

/// The unbounded-cost census, split by WHY (probe 2026-07-30: a capped
/// local-model task was announced as « 1 uncapped task » — its cap was
/// already declared; the missing thing was a PRICE. A count that points
/// at the wrong repair wastes the one edit it asks for). `uncapped` =
/// no token bound, or an unknown `for_each` count · `unpriced` =
/// capped, no catalog price · `unbounded child call` = a composed
/// child carrying uncapped spend of its own (the reason is unknowable
/// across the composition wall).
fn unbounded_census(report: &CheckReport) -> String {
    let (mut uncapped, mut unpriced) = (0usize, 0usize);
    for c in &report.cost.tasks {
        match c.unbounded_reason {
            Some(UnboundedReason::NoPrice) => unpriced += 1,
            Some(_) => uncapped += 1,
            None => {}
        }
    }
    let children = report
        .cost
        .composed
        .iter()
        .filter(|c| c.has_unbounded)
        .count();
    let mut parts = Vec::new();
    if uncapped > 0 {
        parts.push(crate::vocab::count(uncapped, "uncapped task"));
    }
    if unpriced > 0 {
        parts.push(crate::vocab::count(unpriced, "unpriced task"));
    }
    if children > 0 {
        parts.push(crate::vocab::count(children, "unbounded child call"));
    }
    parts.join(" · ")
}

/// The clean verdict as ONE informative card line — what was proven,
/// at a glance: `✔ audited · N tasks · M waves · permits <state> ·
/// est ≤$X · K hints · risk <grade>`. The hints themselves stay above;
/// this line counts them so a scroll-past never misses advice silently.
///
/// The grade is the card's honesty gate (P0-6 · 2026-07-30): past
/// [`nika_check::RiskGrade::Supervised`] the line is NEVER green. `✔ audited · est
/// unbounded` in `Role::Good` shipped exactly that lie — an agent loop
/// at `max_turns: 100` with no token cap closed on a green card while
/// the COST section named the uncapped task three lines up. High and
/// Unbounded render the warn mark and name the grade; only Low and
/// Supervised earn the green mark.
fn audited_line(
    report: &CheckReport,
    _wf: &RawWorkflow,
    distinct_hints: usize,
    hint_sites: usize,
    grade: nika_check::RiskGrade,
    t: Theme,
) -> String {
    let tasks: usize = report.waves.iter().map(Vec::len).sum();
    let permits = permits_glance::permits_glance(report);
    // The green mark is EARNED, never defaulted: grade ≥ High (glob
    // grants · true wildcards · uncapped spend) renders the warn mark
    // and Role::Warn — the audit completed, the readiness did not.
    let ready = grade < nika_check::RiskGrade::High;
    let (mark, role) = if ready {
        (if t.ascii { "ok" } else { "✔" }, Role::Good)
    } else {
        (if t.ascii { "!" } else { "⚠" }, Role::Warn)
    };
    // The COST section speaks CEILING throughout — `≤N tk` per task, and
    // the range labelled "worst-case ceiling". This line used to speak
    // FLOOR (`est ≥$X`) over `min_path_total_usd`, and the two
    // contradicted each other in the same output.
    //
    // The floor was never true. `min_path_total_usd` is the cheapest PATH
    // with every task priced at its own token ceiling, so it bounds
    // nothing from below: measured, a run billed $0.000242 under an
    // announced `est ≥$0.0305` — 126× the other way. Users provision
    // against this number before they launch.
    //
    // Bounded: quote the ceiling the section already computed, with `≤`.
    // Unbounded: no ceiling exists, and no floor is computable either
    // (every bounded task is itself priced at its cap), so claim neither
    // — name WHAT is unbounded, split by why (unbounded_census).
    let est = if report.cost.has_unbounded {
        format!("est unbounded · {}", unbounded_census(report))
    } else {
        // `out` is the narrowing, and it is the same one the COST section
        // carries three lines up. That section already says "worst-case
        // OUTPUT ceiling · prompts, exec + mcp unpriced"; this line said
        // `est ≤$X` flat, which reads as the bill. It is not: F7 measured
        // 328x on the commonest shape a person writes first (fetch a 3.2
        // MB document, summarise it — $2.4563 of input against a printed
        // $0.0075). The card is the line people quote, so it is the line
        // that must not overreach.
        let at_most = crate::vocab::at_most(t.ascii);
        format!(
            "est out {at_most}${}",
            crate::vocab::usd(report.cost.bounded_total_usd)
        )
    };
    // « risk unbounded » used to close the output with no handle
    // (gauntlet 08-01, Camille — an alarm without a remedy, and
    // « 0 hints » confessed it): the footer now carries the one next
    // move. An unpriced-only census is the local-model shape (no
    // dollar meter exists — the cap matters IF a cloud seat is
    // chosen); anything else has a declarable ceiling today.
    let handle = if grade == nika_check::RiskGrade::Unbounded {
        let unpriced_only = report
            .cost
            .tasks
            .iter()
            .all(|c| matches!(c.unbounded_reason, None | Some(UnboundedReason::NoPrice)));
        if unpriced_only {
            // The handle names its VERB. Pasted onto `check` the bare
            // flag exits 2 — `--max-cost-usd` lives on `run`, and a
            // handle that breaks where it is printed is the class this
            // whole wave hunts (gauntlet 08-01, Sofia).
            " — no dollar meter for a local/unknown model · cap a cloud seat on the run: `nika run <file> --max-cost-usd <usd>`"
        } else {
            " — declare max_tokens/ceilings, or cap it on the run: `nika run <file> --max-cost-usd <usd>`"
        }
    } else {
        ""
    };
    let hint_summary = if distinct_hints == hint_sites {
        crate::vocab::count(hint_sites, "hint")
    } else {
        format!(
            "{} across {}",
            crate::vocab::count(distinct_hints, "distinct hint"),
            crate::vocab::count(hint_sites, "site")
        )
    };
    t.paint(
        role,
        &format!(
            "{mark} audited · {} · {} · permits {permits} · {est} · {} · risk {}{handle}",
            crate::vocab::count(tasks, "task"),
            crate::vocab::count(report.waves.len(), "wave"),
            hint_summary,
            grade.as_str(),
        ),
    )
}

/// A finding section: one OK line when empty, one row per finding else.
/// COMPOSITION (spec 14) · silent when the workflow calls no child
/// (the SKILLS precedent) — split out of `render` at the 100-line cap.
fn composition_rung(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if report.composition.is_empty() && !wf_calls_workflows(wf) {
        return;
    }
    section_list(
        out,
        t,
        "COMPOSITION",
        // The four adjectives do not all reach the same distance, and the
        // module header says so: the ROOT's DIRECT calls carry the full
        // law set; the reachable closure is walked for ACYCLICITY only,
        // because "each file answers for its own contract". Measured
        // 2026-07-29 on root → child → grandchild, where the grandchild's
        // `nika:fetch` is outside the root boundary: checking the child
        // directly reports `✖ NIKA-COMP-002`, checking the root reports
        // the old line's `✔ every child call is …` and a green card.
        "direct child calls are static, typed and contained · closure acyclic",
        report
            .composition
            .iter()
            .map(nika_check::CompositionFinding::row)
            .collect(),
    );
}

/// Whether any task (main or `on_finally`) calls a child workflow —
/// the COMPOSITION rung renders only then (silent-when-absent · the
/// SKILLS precedent).
fn wf_calls_workflows(wf: &RawWorkflow) -> bool {
    wf.tasks.iter().any(|task| {
        let is_call = |a: &RawAction| {
            matches!(a, RawAction::Invoke(inv)
                if matches!(inv.target, nika_schema::raw::RawInvokeTarget::Workflow(_)))
        };
        is_call(&task.value.action)
    })
}

/// The SECRETS rows — one per leak into an effect, then one per egress
/// through the workflow `outputs:` — read from the ONE findings fold
/// (`nika-check::findings` owns the fix text: the human voice IS the
/// `--json` voice, one contract, never a second renderer).
fn secret_rows(report: &CheckReport) -> Vec<String> {
    report
        .findings
        .iter()
        .filter(|f| f.kind == "secret_leak" || f.kind == "secret_egress")
        .map(|f| f.message.clone())
        .collect()
}

/// A finding section for a lane that needs a valid DAG. When conformance
/// fails the lane did not run, so it announces the SKIP instead of a
/// verdict — the PLAN line's posture, applied to the rest of the ladder.
///
/// This is the false-green class caught at its purest. Four lanes are
/// gated on a computable order (`lib.rs`): SECRETS and GATES read the
/// topological waves, POLICY and TRIFECTA are wrapped in an explicit
/// `if conformance.is_empty()`. All four rendered `✔` regardless.
/// Measured 2026-07-29, one file, one line apart — a secret piped
/// straight into `exec curl` reports
/// `✖ SECRETS leak into exec (task 'send')`; add ONE task depending on a
/// name that does not exist and the same leak reports
/// `✔ SECRETS no information-flow escapes`. The green did not mean the
/// leak was gone. It meant nobody looked.
pub(crate) fn section_or_skip(
    out: &mut String,
    report: &CheckReport,
    t: Theme,
    label: &str,
    ok_msg: &str,
    rows: Vec<String>,
) {
    if report.conformance.is_empty() {
        section_list(out, t, label, ok_msg, rows);
        return;
    }
    let padded = format!("{label:<8}");
    let _ = writeln!(
        out,
        " {} {} {}",
        t.paint(Role::Dim, "○"),
        t.paint(Role::Strong, &padded),
        t.paint(
            Role::Dim,
            "(skipped — no valid DAG order while conformance fails)"
        )
    );
}

pub(crate) fn section_list(
    out: &mut String,
    t: Theme,
    label: &str,
    ok_msg: &str,
    rows: Vec<String>,
) {
    // Pad BEFORE painting — ANSI escapes break `{:<8}` width arithmetic
    // (the format pads bytes, not display columns).
    let padded = format!("{label:<8}");
    if rows.is_empty() {
        let _ = writeln!(
            out,
            " {} {} {}",
            mark(t, true),
            t.paint(Role::Strong, &padded),
            t.paint(Role::Dim, ok_msg)
        );
        return;
    }
    for row in rows {
        let _ = writeln!(
            out,
            " {} {} {row}",
            mark(t, false),
            t.paint(Role::Strong, &padded)
        );
    }
}

fn plan(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    if report.waves.is_empty() {
        if !report.conformance.is_empty() {
            let _ = writeln!(
                out,
                " {}     {}",
                t.paint(Role::Strong, "PLAN"),
                t.paint(
                    Role::Dim,
                    "(skipped — no valid DAG order while conformance fails)"
                )
            );
        }
        return;
    }
    let tasks: usize = report.waves.iter().map(Vec::len).sum();
    let max_par = report.waves.iter().map(Vec::len).max().unwrap_or(1);
    // The wave peak is what the wave scheduler executes; the DAG's exact
    // width (Dilworth · report.analysis) can exceed it — say so when it
    // does, with the witness teaching WHICH tasks could run together.
    let width_note = report
        .analysis
        .as_ref()
        .filter(|a| a.width > max_par)
        .map(|a| {
            let mut sample = a.width_witness.clone();
            sample.truncate(4);
            let suffix = if a.width_witness.len() > 4 {
                " · …"
            } else {
                ""
            };
            format!(
                " · width {} (the DAG permits {} concurrent · e.g. {}{suffix})",
                a.width,
                a.width,
                sample.join(" · "),
            )
        })
        .unwrap_or_default();
    let _ = writeln!(
        out,
        " {} {}     {} · {} · max parallelism {max_par}{width_note}",
        mark(t, true),
        t.paint(Role::Strong, "PLAN"),
        crate::vocab::count(report.waves.len(), "wave"),
        crate::vocab::count(tasks, "task"),
    );
    // The membership — WHAT dispatches WHEN (the dry-run answer: check
    // is the dry-run; this line is what `run` will do, wave by wave).
    // Compact workflows only: past 12 tasks the summary line carries it.
    if tasks <= 12 {
        for (i, wave) in report.waves.iter().enumerate() {
            let members: Vec<String> = wave
                .iter()
                .filter_map(|&ix| wf.tasks.get(ix))
                .map(|task| {
                    let (verb, target) = verb_of(&task.value.action, wf);
                    match target {
                        Some(target) => format!("{} ({verb} · {target})", task.value.id.value),
                        None => format!("{} ({verb})", task.value.id.value),
                    }
                })
                .collect();
            let _ = writeln!(
                out,
                "      {} {}",
                t.paint(Role::Dim, &format!("wave {}", i + 1)),
                members.join(" · ")
            );
        }
    }
}

/// A task's verb + dispatch target (model · tool · `argv[0]`) for the plan.
fn verb_of<'w>(action: &'w RawAction, wf: &'w RawWorkflow) -> (&'static str, Option<String>) {
    match action {
        RawAction::Infer(a) => (
            "infer",
            a.model
                .as_ref()
                .map(|m| m.value.clone())
                .or_else(|| wf.model.as_ref().map(|m| m.value.clone())),
        ),
        RawAction::Agent(a) => (
            "agent",
            a.model
                .as_ref()
                .map(|m| m.value.clone())
                .or_else(|| wf.model.as_ref().map(|m| m.value.clone())),
        ),
        RawAction::Exec(a) => (
            "exec",
            match &a.command {
                nika_schema::raw::RawCommand::Shell(_) => Some("sh -c".to_owned()),
                nika_schema::raw::RawCommand::Argv(argv) => argv.first().map(|w| w.value.clone()),
                // #[non_exhaustive] — a future command form names itself.
                _ => None,
            },
        ),
        RawAction::Invoke(a) => match &a.target {
            nika_schema::raw::RawInvokeTarget::Tool(t) => ("invoke", Some(t.value.clone())),
            nika_schema::raw::RawInvokeTarget::Workflow(w) => {
                ("invoke", Some(format!("workflow:{}", w.value)))
            }
        },
        // #[non_exhaustive] — a future verb must not break this build.
        _ => ("task", None),
    }
}

/// The composition arm of [`cost`] (spec 14 · the 2026-07-29 finding): no
/// OWN inference task, but resolvable children are priced — printing
/// `$0.00` here told the operator a free story about a bill the child
/// explains at `≤$X`. The totals already carry the children; the uncapped
/// count names the composed half only (no own task exists to count).
fn cost_composed_only(out: &mut String, report: &CheckReport, t: Theme) {
    let calls = crate::vocab::count(report.cost.composed.len(), "composed child call");
    if report.cost.has_unbounded {
        let _ = writeln!(
            out,
            " {} {}     {}",
            t.paint(Role::Warn, if t.ascii { "! " } else { "⚠ " }),
            t.paint(Role::Strong, "COST"),
            t.paint(
                Role::Warn,
                &format!(
                    "bounded portion ${} · no total ceiling · {} · {}",
                    crate::vocab::usd(report.cost.bounded_total_usd),
                    calls,
                    unbounded_census(report)
                )
            )
        );
    } else {
        let money = format!(
            "${} – ${} worst-case output ceiling · {} · own inference $0.00",
            crate::vocab::usd(report.cost.min_path_total_usd),
            crate::vocab::usd(report.cost.bounded_total_usd),
            calls
        );
        let _ = writeln!(
            out,
            " {} {}     {}",
            mark(t, true),
            t.paint(Role::Strong, "COST"),
            money
        );
    }
}

/// The COST arm for a workflow with NO own inference task — the `$0.00`
/// that used to claim the whole bill from a lane that prices
/// `infer:`/`agent:` and nothing else; the composed-children arm joins
/// here (extracted under the fn-length law).
fn cost_empty_arm(out: &mut String, report: &CheckReport, t: Theme) {
    if !report.cost.composed.is_empty() {
        return cost_composed_only(out, report, t);
    }
    let _ = writeln!(
        out,
        " {} {}     {}",
        mark(t, true),
        t.paint(Role::Strong, "COST"),
        // An `exec:` runs an arbitrary program, and the programs authors
        // reach for first are billed LLM CLIs; an `mcp:` call is a third
        // party's meter. Measured 2026-07-29: a lone
        // `exec: ["claude", "-p", "write a novel"]` printed
        // `✔ COST no inference tasks · $0.00` and
        // `✔ audited · est ≤$0.0000`.
        t.paint(
            Role::Dim,
            "no infer/agent tasks · $0.00 · exec + mcp spend unpriced"
        )
    );
}

/// The per-task rows of [`cost`] — a priced row at its cap, or the
/// UNBOUNDED row with its named reason (extracted under the fn-length law).
fn cost_task_rows(out: &mut String, report: &CheckReport, t: Theme) {
    let le = crate::vocab::at_most(t.ascii);
    for c in &report.cost.tasks {
        let model = c.model.as_deref().unwrap_or("?");
        match (&c.usd, &c.unbounded_reason) {
            (Some(worst), _) => {
                let _ = writeln!(
                    out,
                    "   {}  {}  {le}{} tk  ${}",
                    c.task,
                    t.paint(Role::Dim, model),
                    c.max_tokens.unwrap_or(0),
                    crate::vocab::usd(*worst),
                );
            }
            (None, reason) => {
                let why = match reason {
                    Some(UnboundedReason::NoTokenLimit) => "no max_tokens declared",
                    Some(UnboundedReason::NoPrice) => "no catalog price (local/unknown model)",
                    Some(UnboundedReason::UnknownIterations) => {
                        "for_each over an expression (unknown count)"
                    }
                    _ => "unbounded",
                };
                let _ = writeln!(
                    out,
                    "   {}  {}  {} {}",
                    c.task,
                    t.paint(Role::Dim, model),
                    t.paint(Role::Warn, "UNBOUNDED"),
                    t.paint(Role::Dim, &format!("— {why}")),
                );
            }
        }
    }
}

fn cost(out: &mut String, report: &CheckReport, t: Theme) {
    if report.cost.tasks.is_empty() {
        return cost_empty_arm(out, report, t);
    }
    // OUTPUT ceiling, and the word is load-bearing. `cost::ceiling` prices
    // `max_tokens`, which the spec defines as "Max OUTPUT tokens"
    // (02-verbs §infer), against `output_price_per_million`. The prompt is
    // not underweighted in that sum — it is absent, and
    // `input_per_million` has no reader anywhere in `nika-check`.
    //
    // The gap is not academic. Measured 2026-07-28 on the most common shape
    // a person writes first — fetch a document, summarise it: a 3.2 MB body
    // interpolated into one prompt is ~818k input tokens, $2.4563 at that
    // model's published input rate, against a line reading $0.0075. 328x,
    // under a green mark, with the input price sitting four lines above the
    // output price the sum reads in the same catalog block.
    //
    // Pricing it properly needs a static bound on interpolated content,
    // which is real work (the shape is the one `for_each` already uses:
    // literal is known, expression is unbounded). Until that lands the
    // verdict NARROWS instead of overreaching — a claim that covers what it
    // computes is always available, and is the only honest thing to print
    // while the other half has no bound.
    //
    // Unbounded cost is a WARNING posture (is_clean ignores it): the
    // report stays honest about the floor without failing the file.
    // The unbounded arm used to print `$min – $bounded` under the word
    // FLOOR — and `audited_line` documents why both halves are false
    // (`min_path_total_usd` bounds nothing from below · measured 126× the
    // other way). Same decision as there: claim neither bound, show the
    // only true number (the priced portion), and name the uncapped tasks.
    let (cost_mark, money, bound) = if report.cost.has_unbounded {
        (
            t.paint(Role::Warn, if t.ascii { "! " } else { "⚠ " }),
            format!(
                "bounded portion ${}",
                crate::vocab::usd(report.cost.bounded_total_usd)
            ),
            t.paint(
                Role::Warn,
                &format!("no total ceiling · {}", unbounded_census(report)),
            ),
        )
    } else {
        (
            mark(t, true),
            format!(
                "${} – ${}",
                crate::vocab::usd(report.cost.min_path_total_usd),
                crate::vocab::usd(report.cost.bounded_total_usd)
            ),
            "worst-case output ceiling".to_owned(),
        )
    };
    // The price table's date rides WITH the number. A ceiling is a
    // promise, and a promise computed against prices that have since
    // moved is a promise about the past — the `--json` lane has carried
    // `pricing.snapshot.as_of` since the models rung shipped, but the
    // human lane never showed it, so the one reader who cannot query the
    // payload was the one who could not tell.
    //
    // This is not hypothetical drift: vendor intro pricing expires on
    // announced dates, so a workflow audited in one month can bill more
    // in the next with the file unchanged.
    let snap = nika_catalog::pricing_snapshot();
    let _ = writeln!(
        out,
        " {cost_mark} {}     {} {bound} {}",
        t.paint(Role::Strong, "COST"),
        t.paint(Role::Strong, &money),
        // `exec` + `mcp` join `prompts` in the unpriced list for the same
        // reason the empty-cost branch above names them: this ceiling
        // sums `infer:`/`agent:` output tokens, so a workflow that mixes
        // an `infer:` with an `exec:` gets a ceiling with the exec's whole
        // bill missing — and the ✔ does not say so.
        t.paint(
            Role::Dim,
            &format!("· prompts, exec + mcp unpriced · prices {}", snap.as_of)
        ),
    );
    cost_task_rows(out, report, t);
}

/// `≤ N Wh` at a ceiling-honest display grain: a tiny bound rounds UP
/// to 0.001 — this rung never prints `0.0 Wh` (a zero would claim free
/// inference · NEP-0018 « unknown stays unknown »).
fn fmt_wh(wh: f64) -> String {
    if wh >= 1.0 {
        format!("{wh:.1}")
    } else {
        format!("{:.3}", (wh * 1000.0).ceil() / 1000.0)
    }
}

/// One class → `≤ X Wh` (the class rides the count line) · several →
/// `gpu ≤ X Wh · fleet ≤ Y Wh`, each subtotal wearing its class.
fn fmt_scope_totals(subs: &[(String, f64)]) -> String {
    match subs {
        [] => String::new(),
        [(_, wh)] => format!("≤ {} Wh", fmt_wh(*wh)),
        many => many
            .iter()
            .map(|(scope, wh)| format!("{scope} ≤ {} Wh", fmt_wh(*wh)))
            .collect::<Vec<_>>()
            .join(" · "),
    }
}

/// The ENERGY reading (NEP-0018 · nika-spec `governance/`) — the render
/// half of `report.energy` (the classification + ceiling math lives in
/// `nika_check::energy` since 2026-07-29 · the 15k wall · compute
/// descends, render stays). Same ladder as COST, same four words:
///
/// - a **ceiling** (`≤ N Wh`) only where BOTH a `max_tokens` cap and a
///   sourced figure exist — measured rows render with their axes, so
///   two honest numbers stay comparable;
/// - **UNBOUNDED** tasks are counted here and NAMED at COST three lines
///   up (same tasks, same reasons — one voice, no double list);
/// - a model without a figure is **unpriced**, never `0 Wh`;
/// - watt-hours sum WITHIN a scope class and never across it — a mixed
///   set gets one subtotal per class, not a refusal and not a
///   meaningless sum.
fn energy(out: &mut String, report: &CheckReport, t: Theme) {
    if report.cost.tasks.is_empty() {
        return; // no infer/agent tasks — the ladder says so at COST
    }
    let e = &report.energy;
    energy_lines(out, &e.tasks, &e.counts, &e.scope_subtotals, t);
}

/// The headline + measured rows for [`energy`] (split for the 100-line
/// function law — classification lives in `nika_check::energy`).
fn energy_lines(
    out: &mut String,
    measured: &[nika_check::EnergyTask],
    n: &nika_check::EnergyCounts,
    subs: &[(String, f64)],
    t: Theme,
) {
    let label = t.paint(Role::Strong, "ENERGY");
    let counts = {
        let mut parts = vec![format!("{} of {} tasks measured", measured.len(), n.total)];
        for (k, noun) in [
            (n.unpriced, "unpriced"),
            (n.uncapped, "uncapped"),
            (n.never_runs, "never-run"),
        ] {
            if k > 0 {
                parts.push(format!("{k} {noun}"));
            }
        }
        parts.join(" · ")
    };
    if n.uncapped > 0 {
        energy_unbounded_headline(out, subs, &counts, t, &label);
    } else if measured.is_empty() {
        let local = if n.unpriced_local > 0 {
            " · a local model draws your watts"
        } else {
            ""
        };
        let why = if n.unpriced == 0 && n.never_runs > 0 {
            "no task can run (empty for_each) — nothing to bound".to_owned()
        } else {
            format!("unpriced — no sourced Wh figure for any task model{local}")
        };
        let _ = writeln!(
            out,
            " {} {}   {}",
            t.paint(Role::Dim, "○"),
            label,
            t.paint(Role::Dim, &format!("{why} · never 0 Wh (NEP-0018)")),
        );
        return;
    } else {
        // One class or several, the claim is the same KIND of claim — a
        // per-class ceiling. Several classes is not a warning: nothing is
        // unknown, the partition simply refuses to add apples to oranges.
        let scope_note = if subs.len() == 1 {
            format!("· {} scope ", subs[0].0)
        } else {
            "· per scope ".to_owned()
        };
        let _ = writeln!(
            out,
            " {} {}   {} {}",
            mark(t, true),
            label,
            t.paint(
                Role::Strong,
                &format!("{} worst-case OUTPUT ceiling", fmt_scope_totals(subs))
            ),
            t.paint(
                Role::Dim,
                &format!("{scope_note}· {counts} · prompts unpriced")
            ),
        );
    }
    for m in measured {
        let _ = writeln!(
            out,
            "   {}  {}  ≤{} tk  ≤ {} Wh  {}",
            m.task,
            t.paint(Role::Dim, &m.model),
            m.per_call_tokens,
            fmt_wh(m.wh),
            t.paint(
                Role::Dim,
                &format!("({} · {} · {})", m.provenance, m.scope, m.measured_at)
            ),
        );
    }
}

/// The uncapped arm of [`energy_lines`] — the COST rows above already
/// NAME each uncapped task and why, so this rung claims no total and
/// says so (one voice, no double list). The bounded PORTION still
/// reports, per scope class.
fn energy_unbounded_headline(
    out: &mut String,
    subs: &[(String, f64)],
    counts: &str,
    t: Theme,
    label: &str,
) {
    let bounded = if subs.is_empty() {
        String::new()
    } else {
        format!(
            "{} ",
            t.paint(
                Role::Strong,
                &format!("bounded portion {}", fmt_scope_totals(subs))
            )
        )
    };
    let _ = writeln!(
        out,
        " {} {label}   {bounded}{} {}",
        t.paint(Role::Warn, if t.ascii { "! " } else { "⚠ " }),
        t.paint(Role::Warn, "no total energy ceiling"),
        t.paint(Role::Dim, &format!("· {counts} · never 0 Wh (NEP-0018)")),
    );
}

pub fn permits(out: &mut String, report: &CheckReport, wf: &RawWorkflow, t: Theme) {
    // F-O8 « absent = zero authority »: with no boundary declared AND
    // nothing escaping (pure compute), the panel states the zero —
    // informational, and `permits: {}` is taught as the legal explicit
    // form. Any escape (absent → NIKA-AUTH-006 · declared → NIKA-SEC-004
    // · floor → NIKA-SEC-005) MUST render, or `✖ findings above` points
    // at nothing (the mute-diagnostic the battery re-run caught). The
    // NEP-0004 taint findings (interpolated bound → NIKA-AUTH-007 ·
    // untrusted argument escape → NIKA-AUTH-008) ride the SAME panel.
    //
    // …but a CLEAN verdict here is only worth what the analysis behind it
    // saw, and this lane never got the 2026-07-29 guard the four
    // `section_or_skip` lanes did (« the green did not mean the leak was
    // gone. It meant nobody looked. »). Measured 2026-08-15: a body whose
    // jq program reached for the ambient environment reported
    // `✖ CONFORM` on one line and « the body is pure compute so nothing
    // escapes » on the next — a sentence that was false about that body,
    // printed beside the finding that proved it false. Escapes still
    // render below (the mute-diagnostic law is untouched); only the CLAIM
    // is withheld while conformance is red.
    if !report.conformance.is_empty() {
        permits_unjudged(out, report, t);
        return;
    }
    if wf.permits.is_none() && report.capability_escapes.is_empty() {
        let _ = writeln!(
            out,
            " {} {}  {}",
            t.paint(Role::Dim, "○"),
            t.paint(Role::Strong, "PERMITS"),
            // `{}` and NOT `{{}}`: this literal is an ARGUMENT to `writeln!`,
            // not part of its format string, so braces are not unescaped
            // here. The doubled form shipped, and it is not a cosmetic slip —
            // `permits: {{}}` is refused by YAML itself, so the line taught a
            // form no parser accepts while the HINT one row below printed the
            // right one. Two lines of the same output disagreeing.
            t.paint(
                Role::Dim,
                "zero authority (no `permits:` declared · F-O8) · pure compute · `permits: {}` states it"
            )
        );
        return;
    }
    if report.capability_escapes.is_empty()
        && report.permit_taints.is_empty()
        && report.sink_findings.is_empty()
    {
        // The exec-grant clause (user gauntlet 2026-07-31 · G-10): a
        // granted `exec:` lets sub-processes touch files the fs lists
        // never admitted, so a green PERMITS must not read as a sealed
        // fence — the same honesty the run banner speaks.
        let exec_open = wf.permits.as_ref().is_some_and(|p| p.value.allows_exec());
        let _ = writeln!(
            out,
            " {} {}  {}",
            mark(t, true),
            t.paint(Role::Strong, "PERMITS"),
            // What `permits_fit` judges is the ARGUMENT AS WRITTEN, and
            // only when it resolves: a literal, or a bare
            // `${{ const.<name> }}` through the const table
            // (`judgeable_arg`). Everything else is the runtime
            // `NIKA-SEC-004`'s — a path from `inputs.`, a `with:` binding,
            // an upstream output, a shell-string `exec`. And the RESOLVED
            // path is never this lane's: a literal `data/link.csv` that
            // symlinks out of the tree fits here and is refused at run.
            // Measured 2026-07-29, both `✔ body fits the declared
            // boundary`. Naming the two halves costs one clause and stops
            // the line meaning more than it checked.
            // « at run » alone read as a footnote, not a boundary of the
            // claim (V7-2 · wave-3: four personas took this ✔ as a seal
            // over their glob-fed paths, then died SEC-004 at run). The
            // clause now states WHOSE verdict the computed half is.
            t.paint(
                Role::Dim,
                if exec_open {
                    "literal + const: args fit the boundary · computed paths + symlinks \
                     are the RUN's verdict · exec outside the fs bounds"
                } else {
                    "literal + const: args fit the boundary · computed paths + symlinks \
                     are the RUN's verdict"
                }
            )
        );
        loopback_declassification_lines(out, wf, t);
        return;
    }
    permits_escape_rows(out, report, t);
}

/// The PERMITS panel while conformance is RED — the escapes that WERE found
/// still render (the mute-diagnostic law: `✖ findings above` must point at
/// something), but the clean sentence is replaced by the honest one.
///
/// The boundary analysis reads a parsed, conformant workflow; when conformance
/// fails there is no such workflow to read, so « pure compute · nothing
/// escapes » would be a claim about a body nobody analysed. Same shape as the
/// four `section_or_skip` lanes, different reason (they are gated on a
/// computable DAG order; this one on there being a judgeable file at all).
fn permits_unjudged(out: &mut String, report: &CheckReport, t: Theme) {
    if report.capability_escapes.is_empty()
        && report.permit_taints.is_empty()
        && report.sink_findings.is_empty()
    {
        let _ = writeln!(
            out,
            " {} {}  {}",
            t.paint(Role::Dim, "○"),
            t.paint(Role::Strong, "PERMITS"),
            t.paint(
                Role::Dim,
                "(skipped — the body is not judged while conformance fails)"
            )
        );
        return;
    }
    permits_escape_rows(out, report, t);
}

/// The escape/sink/taint rows of [`permits`] (extracted under the
/// fn-length law) — one per finding, code-first, same voices as
/// `findings[]`.
fn permits_escape_rows(out: &mut String, report: &CheckReport, t: Theme) {
    for e in &report.capability_escapes {
        let fix = e
            .fix
            .as_deref()
            .map(|f| format!(" · fix: {f}"))
            .unwrap_or_default();
        // The wire code leads the row (agent battery A2 · 2026-07-11):
        // CONFORM rows print `[NIKA-…]` and teach `nika explain <CODE>`;
        // the PERMITS rows printed only the category, so the one panel
        // whose findings are security-graded was the one panel a user
        // could not ask the engine to explain. Same code the findings[]
        // machine list stamps (floor → SEC-005 · absent boundary →
        // AUTH-006 · declared boundary → SEC-004).
        let code = if e.floor {
            "NIKA-SEC-005"
        } else if e.undeclared {
            "NIKA-AUTH-006"
        } else {
            "NIKA-SEC-004"
        };
        let _ = writeln!(
            out,
            " {} {}  [{code} · {}] task `{}` · {}{fix}",
            mark(t, false),
            t.paint(Role::Strong, "PERMITS"),
            e.category,
            e.task,
            e.detail,
        );
    }
    for sink in &report.sink_findings {
        // NEP-0006 · the finding's own code (the same one findings[] and
        // extra_conformance_codes read).
        let code = sink.wire_code();
        let _ = writeln!(
            out,
            " {} {}  [{code}] task `{}` · {} · fix: {}",
            mark(t, false),
            t.paint(Role::Strong, "PERMITS"),
            sink.task,
            sink.detail,
            sink.fix,
        );
    }
    for taint in &report.permit_taints {
        // NEP-0004 — the finding's own kind IS the code (one arm, the
        // same one findings[] and extra_conformance_codes read).
        let code = taint.wire_code();
        let fix = taint
            .fix
            .as_deref()
            .map(|f| format!(" · fix: {f}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            " {} {}  [{code}] task `{}` · {}{fix}",
            mark(t, false),
            t.paint(Role::Strong, "PERMITS"),
            taint.task,
            taint.detail,
        );
    }
}

/// The loopback-declassification statement (#395), one dim line per
/// exact loopback literal in the declared `net.http`: a green panel must
/// TEACH that the always-on SSRF floor is cleared for that host by the
/// author's explicit permit — silence would hide a security-relevant
/// clearing. Informational only (never a finding — the clearing is the
/// declared intent working as designed); the JSON twin rides
/// `report.permits.notes`.
fn loopback_declassification_lines(out: &mut String, wf: &RawWorkflow, t: Theme) {
    let Some(permits) = wf.permits.as_ref() else {
        return;
    };
    let Some(net) = permits.value.net.as_ref() else {
        return;
    };
    for entry in &net.http {
        if nika_types::net::is_exact_loopback_literal(entry) {
            let _ = writeln!(
                out,
                "   {}",
                t.paint(
                    Role::Dim,
                    &format!(
                        "`{entry}` — exact loopback literal: the explicit permit \
                         clears the always-on SSRF floor (NIKA-SEC-005) for that host"
                    )
                ),
            );
        }
    }
}
mod footer;
mod permits_glance;
mod slots;
use footer::hints_and_verdict;
pub use slots::stamp_unfilled_slots;

#[cfg(test)]
mod tests;
mod trifecta;
use trifecta::trifecta_rung;
