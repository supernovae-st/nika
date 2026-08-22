// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika explain <file>` — the human story of a workflow (the file form of
//! `explain` · the code form lives in [`super::explain`]).
//!
//! One more RENDERER over the one projection (`load_checked` → `graph::
//! project` — the same seam `inspect` and `graph` read): what this
//! workflow does, the story wave by wave, the cost BEFORE a token is
//! spent (honesty rules: unknown stays unknown, never `$0` · a local
//! model is unpriced compute, never « free »), what it touches, the
//! structural risks, how to run it, and what the flight recorder already
//! holds. Deterministic, offline, zero LLM — a narration derived from
//! facts the checker proved, never a summary something imagined.
//!
//! One trace-first law sits above all of it (P0-12 · the 2026-07-30 UX
//! audit): when the LATEST run of THIS workflow failed, the render OPENS
//! on the recovery rail — task · cause · `trace show` pointer · the
//! targeted `--resume` route (ADR-099) — and the naked re-run CTA steps
//! aside until the failure is audited.
//!
//! `--json` emits the versioned machine twin (`explain_version: 1`),
//! reusing the check report's own serialized vocabulary (cost ·
//! requirements · hints · analysis) so agents read ONE dialect across
//! `check --json` and `explain --json`.

use std::fmt::Write as _;
use std::path::Path;

use nika_check::{CheckReport, UnboundedReason};
use nika_event::{Event, EventKind};

use crate::verbs::graph::{GraphDoc, Node, project};
use crate::verbs::run::{lf_normal_form, recover_events, sha256_hex};
use crate::verbs::{VerbOutput, load_checked_with_source};
use nika_dap::store::{TraceState, fold_facts};

/// Route `explain`'s positional: an existing path or a path-shaped string
/// (`/` · `.yaml`/`.yml` · `-`) narrates the FILE; everything else teaches
/// the CODE (`NIKA-440` · `DAG-003` · bare `440`). Hint identities that
/// themselves contain a slash (`native-first/006` · the token `nika check`
/// prints in `[brackets]`) are CODE, not a path — unless a file of that
/// name actually exists on disk. A file literally named like a code still
/// routes as a file when it exists — the pathological tie goes to the
/// thing that provably exists.
#[must_use]
pub fn dispatch(
    query: &str,
    json: bool,
    forecast: bool,
    theme: crate::display::theme::Theme,
) -> VerbOutput {
    let yaml_ext = Path::new(query)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("yaml") || e.eq_ignore_ascii_case("yml"));
    let exists = Path::new(query).exists();
    let slash = query.contains('/') || query.contains('\\');
    // A slash used to mean FILE unconditionally, so `nika explain
    // native-first/006` (the taught next-gesture after a HINT row) 404'd
    // as a missing path. Known hint identities keep the code form; a
    // file that actually exists still wins the tie.
    let file_shaped =
        query == "-" || yaml_ext || exists || (slash && nika_check::hint_help(query).is_none());
    if file_shaped {
        return run(query, json, forecast);
    }
    if json {
        // The code form is prose-by-design (one paragraph, one voice) —
        // refusing beats silently ignoring a flag an agent relied on.
        return VerbOutput::file(
            "--json rides the FILE form (`nika explain <file> --json`); error codes teach in prose"
                .to_owned(),
        );
    }
    if forecast {
        // Same law as --json: a forecast is learned from a WORKFLOW's
        // history — an error code has none. Refuse loudly, never ignore.
        return VerbOutput::file(
            "--forecast rides the FILE form (`nika explain <file> --forecast`); error codes have no run history"
                .to_owned(),
        );
    }
    super::explain::run(query, theme)
}

/// The `nika explain <file>` verb.
#[must_use]
pub fn run(path: &str, json: bool, forecast: bool) -> VerbOutput {
    run_with_traces(
        path,
        json,
        forecast,
        Path::new(".nika").join("traces").as_path(),
    )
}

/// [`run`] with an explicit trace directory — the testable seam: tests
/// stage their own recorder history; a real invocation keeps the fixed
/// relative path the sink writes.
#[must_use]
pub(crate) fn run_with_traces(
    path: &str,
    json: bool,
    forecast: bool,
    traces_dir: &Path,
) -> VerbOutput {
    let (yaml, wf, report) = match load_checked_with_source(path) {
        Ok(triple) => triple,
        Err(out) => return out,
    };
    let permits_declared = wf.permits.is_some();
    if !report.conformance.is_empty() {
        // No valid DAG order → no wave story. Explain stays useful:
        // name the findings and hand over to the fixer, never invent
        // a story the checker refused to prove.
        return dirty(path, &report, json);
    }
    let doc = project(&wf, &report);
    let traces = traces_glance(traces_dir);
    // P0-12 · the trace-first law: the newest run of THIS workflow
    // failed → the render opens on the recovery rail and the naked
    // re-run CTA steps aside. The human surface only — the JSON twin
    // stays the check report's dialect, unchanged.
    let failure = last_failure(traces_dir, &doc.workflow);
    // Learned truth rides beside the static story: gather is bounded +
    // fail-open, and skipped entirely when nothing was ever recorded
    // (the glance already knows) unless the flag asks explicitly.
    let fc = if forecast || traces.is_some() {
        let identity = super::forecast::WorkflowIdentity {
            name: doc.workflow.clone(),
            sha256: sha256_hex(yaml.as_bytes()),
            sha256_lf: sha256_hex(lf_normal_form(&yaml).as_bytes()),
        };
        let gathered = super::forecast::gather::gather(traces_dir, &identity.name);
        Some(super::forecast::compute(&identity, &gathered))
    } else {
        None
    };
    let fc_view = fc
        .as_ref()
        .filter(|r| forecast || r.runs.total >= super::forecast::AUTO_FORECAST_MIN_RUNS);
    if json {
        return VerbOutput::ok(render_json(
            path,
            &doc,
            &report,
            permits_declared,
            traces.as_ref(),
            fc_view,
        ));
    }
    VerbOutput::ok(render_human(
        path,
        &doc,
        &report,
        permits_declared,
        traces.as_ref(),
        fc_view,
        failure.as_ref(),
    ))
}

/// The findings-first partial for a non-conformant file — explain never
/// narrates a DAG the checker could not order.
fn dirty(path: &str, report: &CheckReport, json: bool) -> VerbOutput {
    if json {
        let v = serde_json::json!({
            "explain_version": 1,
            "file": path,
            "clean": false,
            "findings": report.conformance.len(),
            "fix": format!("nika check {path}"),
        });
        return VerbOutput::file(v.to_string());
    }
    let mut s = String::new();
    let _ = writeln!(
        s,
        "this workflow does not check clean yet — {}:",
        crate::text::count(report.conformance.len(), "finding")
    );
    for c in report.conformance.iter().take(3) {
        let _ = writeln!(s, "  [{}] {}", c.code, c.message);
    }
    if report.conformance.len() > 3 {
        let _ = writeln!(s, "  … +{} more", report.conformance.len() - 3);
    }
    let _ = writeln!(
        s,
        "\nfix first: nika check {path}   # every finding explains itself"
    );
    VerbOutput::file(s)
}

/// One task, one sentence — the plain-words gloss of the verb model.
fn task_line(node: &Node) -> String {
    let mut line = match node.verb {
        "infer" => match &node.model {
            Some(m) => format!("asks {m}"),
            None => "asks the workflow model".to_owned(),
        },
        "exec" => "runs a command".to_owned(),
        "invoke" => match &node.tool {
            Some(t) => format!("calls {t}"),
            None => "calls a tool".to_owned(),
        },
        "agent" => match &node.model {
            Some(m) => format!("runs an agent loop on {m}"),
            None => "runs an agent loop".to_owned(),
        },
        other => other.to_owned(),
    };
    if let Some(fan) = &node.fan_out {
        match fan.count {
            Some(n) => {
                let _ = write!(line, " · ×{n} fan-out");
            }
            None => line.push_str(" · fan-out (count known at run time)"),
        }
    }
    if let Some(when) = &node.when {
        let _ = write!(line, " · only when {when}");
    }
    line
}

/// Why a task's cost has no ceiling — the honesty gloss (never `$0`).
fn unbounded_gloss(task: &str, model: Option<&str>, reason: UnboundedReason) -> String {
    match reason {
        UnboundedReason::NoTokenLimit => {
            format!("{task}: no max_tokens declared — spend has no ceiling")
        }
        UnboundedReason::NoPrice => format!(
            "{task}: {} has no catalog price — unknown stays unknown (never $0)",
            model.unwrap_or("the model")
        ),
        UnboundedReason::UnknownIterations => {
            format!("{task}: fan-out count resolves at run time")
        }
        // The enum is #[non_exhaustive]-shaped by policy: a future reason
        // renders honestly rather than silently bounding.
        _ => format!("{task}: cost not statically boundable"),
    }
}

/// The local providers of THIS build (`requires_key == false`) — derived
/// from the same registry a run composes, never a hardcoded id list.
fn local_provider_ids() -> Vec<String> {
    let registry =
        nika_providers::ProviderRegistry::without_http(nika_runtime::compose::config_from_env());
    registry
        .profiles()
        .iter()
        .filter(|p| !p.requires_key && p.id != "mock")
        .map(|p| p.id.to_owned())
        .collect()
}

/// The flight-recorder glance: how many runs the local trace dir holds and
/// the latest journal (names are ISO-timestamped → lexicographic max IS
/// newest). Presence only — no journal is parsed, so no wrong claim.
fn traces_glance(dir: &Path) -> Option<(usize, String)> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.ends_with(".ndjson").then_some(name)
        })
        .collect();
    if names.is_empty() {
        return None;
    }
    names.sort_unstable();
    let latest = names.last()?.clone();
    Some((names.len(), format!("{}/{latest}", dir.display())))
}

/// The tamper-evidence verdict the rail prints — `Intact` is the only
/// CLAIMED state; everything else the walk returns is honest unverified
/// (a broken chain warns, never reassures).
enum ChainNote {
    /// Every line chained and the run's lifecycle closed.
    Intact,
    /// A recorded `chain` field contradicts the recomputation.
    Broken,
    /// Pre-chain, unreadable, torn or incomplete — nothing to claim.
    Unverified,
}

/// What the recovery rail needs from the failed newest run (P0-12).
struct FailedRun {
    /// `dir/name` — the path `trace show` and `--resume` both name.
    trace: String,
    /// The task whose terminal frame carried the failure (`None` when
    /// the run died at the workflow level — e.g. the budget abort).
    task: Option<String>,
    /// The coded detail (`NIKA-1234 · message`) off that frame, else the
    /// workflow-level detail the terminal frame carried.
    cause: Option<String>,
    /// The trace carries ADR-099 resume keys — a skip plan could fold.
    resumable: bool,
    /// The tamper-evidence verdict over the raw journal.
    chain: ChainNote,
}

impl FailedRun {
    /// Fold the failed run's facts through the SAME seams every reader
    /// uses — the resume plan fold (ADR-099) and the chain walk — never
    /// a parallel parser.
    fn fold(dir: &Path, name: &str, raw: &str, events: &[Event]) -> Self {
        let failed = events
            .iter()
            .rev()
            .find(|e| e.kind == EventKind::TaskFailed);
        let task = failed.and_then(|e| str_field(e, "task")).map(str::to_owned);
        let cause = failed
            .and_then(|e| str_field(e, "detail"))
            .or_else(|| {
                events
                    .iter()
                    .rev()
                    .find(|e| e.kind == EventKind::WorkflowFailed)
                    .and_then(|e| str_field(e, "detail"))
            })
            .map(str::to_owned);
        let resumable = !nika_dap::resume::fold_plan(events).plan.is_empty();
        let chain = match nika_dap::chain::walk(raw) {
            nika_dap::chain::Verdict::Intact { .. } => ChainNote::Intact,
            nika_dap::chain::Verdict::Broken { .. } => ChainNote::Broken,
            _ => ChainNote::Unverified,
        };
        Self {
            trace: format!("{}/{name}", dir.display()),
            task,
            cause,
            resumable,
            chain,
        }
    }
}

/// The trace-first recovery fold (P0-12): the newest run OF THIS
/// WORKFLOW failed → the rail's facts; anything else → `None` and the
/// render stays untouched. Newest-first lexicographic order is the
/// glance's own law; the FIRST matching trace decides — a clean newest
/// run keeps the naked CTA, and a newer trace of ANOTHER workflow never
/// masks this workflow's failure. Fail-open like the forecast reader: an
/// unreadable file is skipped, never an error.
fn last_failure(dir: &Path, workflow: &str) -> Option<FailedRun> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            name.ends_with(".ndjson").then_some(name)
        })
        .collect();
    names.sort_unstable();
    names.reverse();
    for name in names {
        // Retention GC can race the scan — a vanished file is skipped.
        let Ok(raw) = std::fs::read_to_string(dir.join(&name)) else {
            continue;
        };
        let Ok(recovered) = recover_events(&raw, &name) else {
            continue;
        };
        let (run_workflow, state, _paused_task) = fold_facts(&recovered.events);
        if run_workflow != workflow {
            continue;
        }
        if state != TraceState::Failed {
            return None;
        }
        return Some(FailedRun::fold(dir, &name, &raw, &recovered.events));
    }
    None
}

/// One string field off an event (the journal's additive KV vocabulary
/// — the same ~6-line twin the forecast reader carries by design).
fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.fields.iter().find(|kv| kv.key == key).and_then(|kv| {
        if let nika_types::resource::Value::String(s) = &kv.value {
            Some(s.as_str())
        } else {
            None
        }
    })
}

/// The human narration — one section helper per beat, composed here (the
/// 100-line fn cap forced this shape and the shape is better: each beat
/// is independently testable prose).
#[allow(clippy::too_many_arguments)] // one render context per beat — the compose seam
fn render_human(
    path: &str,
    doc: &GraphDoc,
    report: &CheckReport,
    permits_declared: bool,
    traces: Option<&(usize, String)>,
    forecast: Option<&super::forecast::ForecastReport>,
    failure: Option<&FailedRun>,
) -> String {
    let mut s = String::new();
    if let Some(f) = failure {
        // The rail OPENS the render — the repair is the story until the
        // failure is audited; the naked re-run CTA steps aside below.
        recovery_section(&mut s, path, f);
        let _ = writeln!(s);
    }
    // The name alone: `description:` died with the envelope nuke
    // (2026-08-12), and the old fallback line taught the dead key.
    let _ = writeln!(s, "{}", doc.workflow);
    let _ = writeln!(
        s,
        "  {} · {} · checks clean",
        crate::text::count(doc.nodes.len(), "task"),
        crate::text::count(report.waves.len(), "wave")
    );
    story_section(&mut s, doc, report);
    cost_section(&mut s, report);
    touches_section(&mut s, doc, report, permits_declared);
    access_section(&mut s, report);
    risks_section(&mut s, path, report);
    if let Some(fc) = forecast {
        super::forecast::render::forecast_section(&mut s, fc);
    }
    if failure.is_none() {
        run_section(&mut s, path, report);
    }
    recorder_section(&mut s, traces);
    s
}

/// The recovery rail (P0-12 · the 2026-07-30 UX audit): when the newest
/// run of THIS workflow failed, explain OPENS on the repair — the faulty
/// task and its coded cause, the `trace show` pointer, the chain verdict
/// (claimed only when the walk proved it), and the ONE route forward:
/// the targeted `--resume` when the trace carries ADR-099 keys, a
/// re-check when it does not. The naked `run it` never competes with it.
fn recovery_section(s: &mut String, path: &str, failure: &FailedRun) {
    let _ = writeln!(s, "last run failed");
    match (&failure.task, &failure.cause) {
        (Some(task), Some(cause)) => {
            let _ = writeln!(s, "  {task} — {cause}");
        }
        (Some(task), None) => {
            let _ = writeln!(s, "  {task} failed (the frame carries no detail)");
        }
        (None, Some(cause)) => {
            let _ = writeln!(s, "  {cause}");
        }
        (None, None) => {
            let _ = writeln!(s, "  the terminal frame carries no detail");
        }
    }
    let _ = writeln!(s, "  see it   nika trace show {}", failure.trace);
    match failure.chain {
        ChainNote::Intact => {
            let _ = writeln!(s, "  chain intact · tamper-evidence verified");
        }
        ChainNote::Broken => {
            let _ = writeln!(
                s,
                "  chain BROKEN — the journal was edited after the fact: trust nothing, re-run fresh"
            );
        }
        ChainNote::Unverified => {
            let _ = writeln!(s, "  chain unverified (pre-chain or unreadable journal)");
        }
    }
    if failure.resumable {
        let _ = writeln!(
            s,
            "  resume   nika run {path} --resume {}   # attested successes replay · the rest re-runs",
            failure.trace
        );
    } else {
        let _ = writeln!(
            s,
            "  repair   nika check {path}   # no resume keys in this trace — re-check, then re-run fresh"
        );
    }
}

/// The story, wave by wave — projection order IS wave order.
fn story_section(s: &mut String, doc: &GraphDoc, report: &CheckReport) {
    // The shape FIRST — the eye reads the wiring before the prose (real
    // wires when the layout can be truthful; multi-task graphs only, a
    // single box is noise). Plain theme: explain's wires stay copyable.
    if doc.nodes.len() > 1
        && let Some(art) = crate::wires::render(
            doc,
            &report.waves,
            crate::display::theme::Theme::new(false, false, false),
        )
    {
        let _ = writeln!(s, "\nthe shape");
        let _ = writeln!(s, "{art}");
    }
    let _ = writeln!(s, "\nthe story");
    let wave_sizes: Vec<usize> = report.waves.iter().map(Vec::len).collect();
    let mut cursor = 0usize;
    for (i, &size) in wave_sizes.iter().enumerate() {
        let end = cursor.saturating_add(size).min(doc.nodes.len());
        let members = &doc.nodes[cursor..end];
        cursor = end;
        let label = if members.len() > 1 {
            format!("wave {} — {} in parallel", i + 1, members.len())
        } else {
            format!("wave {}", i + 1)
        };
        let _ = writeln!(s, "  {label}");
        for node in members {
            let _ = writeln!(s, "    {} — {}", node.id, task_line(node));
        }
    }
}

/// Cost BEFORE a token is spent — the honesty section (no false bound is
/// ever claimed: unknown never renders as $0, local never renders as
/// « free », and a bounded PORTION is named as exactly that).
fn cost_section(s: &mut String, report: &CheckReport) {
    let _ = writeln!(s, "\ncost before a token is spent");
    if report.cost.tasks.is_empty() {
        if report.cost.composed.is_empty() {
            let _ = writeln!(s, "  no inference tasks · $0 model spend");
        } else {
            // The composition arm (spec 14 · the 2026-07-29 finding): no
            // own inference task but priced children — `$0 model spend`
            // was the lie the handoff measured (`≤$0.0011` explained away
            // as zero).
            let calls = report.cost.composed.len();
            if report.cost.has_unbounded {
                let _ = writeln!(
                    s,
                    "  composed spend: bounded portion ${:.4} · no total ceiling · {calls} child call(s)",
                    report.cost.bounded_total_usd
                );
            } else {
                let _ = writeln!(
                    s,
                    "  composed spend: ≤ ${:.4} worst case · ≥ ${:.4} cheapest path · {calls} child call(s) · own inference $0.00",
                    report.cost.bounded_total_usd, report.cost.min_path_total_usd
                );
            }
        }
    } else if report.cost.has_unbounded {
        // One voice with the COST rung (check/render.rs) and inspect.rs:
        // `≥ $FLOOR` claimed a bound over a number that bounds nothing
        // from below (render.rs documents the 126× measurement) — claim
        // neither bound; show the priced portion and name the uncapped.
        let uncapped_tasks = report
            .cost
            .tasks
            .iter()
            .filter(|c| c.unbounded_reason.is_some())
            .count();
        let uncapped_children = report
            .cost
            .composed
            .iter()
            .filter(|c| c.has_unbounded)
            .count();
        let _ = writeln!(
            s,
            "  bounded portion ${:.4} · no total ceiling · {}",
            report.cost.bounded_total_usd,
            crate::text::count(uncapped_tasks + uncapped_children, "uncapped")
        );
        for t in report
            .cost
            .tasks
            .iter()
            .filter_map(|t| t.unbounded_reason.map(|r| (t, r)))
            .take(4)
        {
            let _ = writeln!(
                s,
                "    {}",
                unbounded_gloss(&t.0.task, t.0.model.as_deref(), t.1)
            );
        }
        for c in report
            .cost
            .composed
            .iter()
            .filter(|c| c.has_unbounded)
            .take(2)
        {
            let _ = writeln!(
                s,
                "    child call `{}` → {} — uncapped (no static ceiling)",
                c.task, c.target
            );
        }
    } else {
        let _ = writeln!(
            s,
            "  ≤ ${:.4} worst case · ≥ ${:.4} cheapest path",
            report.cost.bounded_total_usd, report.cost.min_path_total_usd
        );
    }
    let locals = local_provider_ids();
    let uses_local = report.requirements.models.iter().any(|m| {
        m.model
            .split_once('/')
            .is_some_and(|(p, _)| locals.iter().any(|l| l == p))
    });
    if uses_local {
        let _ = writeln!(
            s,
            "  local models: your compute · tokens unpriced — not « free »"
        );
    }
}

/// HOW this machine reaches each model (D-2026-08-04-N1 · P2.5) — the
/// admission-time access decision with its witnesses: the same
/// [`crate::verbs::check::models_rung::access_decisions`] rows
/// `check --json` carries as `access_plan`. Machine truth (key
/// presence), so the narration names THIS install's paths.
fn access_section(s: &mut String, report: &CheckReport) {
    let decisions = crate::verbs::check::models_rung::access_decisions(report);
    if decisions.is_empty() {
        return;
    }
    let _ = writeln!(s, "\naccess (this machine)");
    for (model, decision) in decisions {
        match decision {
            Ok(plan) => {
                let _ = writeln!(
                    s,
                    "  {model} → {} ({} · {})",
                    plan.access, plan.chosen, plan.billing
                );
                for r in &plan.rejected {
                    let _ = writeln!(s, "    ✗ {}", r.witness_line());
                }
            }
            Err(refusal) => {
                let _ = writeln!(s, "  {model} → no access path on this machine");
                for r in &refusal.rejected {
                    let _ = writeln!(s, "    ✗ {}", r.witness_line());
                }
            }
        }
    }
}

/// What it touches: models · tools · secret/env NAMES · the permits stance.
fn touches_section(s: &mut String, doc: &GraphDoc, report: &CheckReport, permits_declared: bool) {
    let _ = writeln!(s, "\nwhat it touches");
    if report.requirements.models.is_empty() {
        let _ = writeln!(s, "  models   none (no inference)");
    } else {
        let models: Vec<String> = report
            .requirements
            .models
            .iter()
            .map(|m| {
                format!(
                    "{} ({})",
                    m.model,
                    crate::text::count(m.tasks.len(), "task")
                )
            })
            .collect();
        let _ = writeln!(s, "  models   {}", models.join(" · "));
    }
    let tools: Vec<&str> = {
        let mut t: Vec<&str> = doc.nodes.iter().filter_map(|n| n.tool.as_deref()).collect();
        t.sort_unstable();
        t.dedup();
        t
    };
    if !tools.is_empty() {
        let _ = writeln!(s, "  tools    {}", tools.join(" · "));
    }
    let mut needs: Vec<String> = report
        .requirements
        .secrets
        .iter()
        .map(|sec| format!("secrets.{}", sec.name))
        .collect();
    needs.extend(
        report
            .requirements
            .inputs_read
            .iter()
            .map(|e| format!("inputs.{e}")),
    );
    if !needs.is_empty() {
        let _ = writeln!(
            s,
            "  needs    {} (names only — values stay in your environment)",
            needs.join(" · ")
        );
    }
    let _ = writeln!(
        s,
        "  permits  {}",
        if permits_declared {
            "declared boundary (default-deny beyond it)"
        } else {
            "zero authority (F-O8) — `nika check --infer-permits` prints the tightest boundary"
        }
    );
}

/// Structural risks — only what the checker proved, never speculation.
fn risks_section(s: &mut String, path: &str, report: &CheckReport) {
    let risky = !report.hints.is_empty()
        || report
            .analysis
            .as_ref()
            .is_some_and(|a| !a.blast_radius.is_empty());
    if !risky {
        return;
    }
    let paid: Vec<_> = report.hints.iter().filter(|h| h.is_paid_run()).collect();
    let rest: Vec<_> = report.hints.iter().filter(|h| !h.is_paid_run()).collect();
    if !paid.is_empty() {
        let _ = writeln!(s, "\nbefore a paid model");
        for h in &paid {
            let _ = writeln!(s, "  [{}] {}", h.kind, h.advice);
        }
        let _ = writeln!(
            s,
            "  paid_ready: false · nika check --json {path} | jq .paid_ready"
        );
    }
    if rest.is_empty()
        && report
            .analysis
            .as_ref()
            .is_none_or(|a| a.blast_radius.is_empty())
    {
        return;
    }
    let _ = writeln!(s, "\nworth knowing");
    for h in rest.iter().take(8) {
        let _ = writeln!(s, "  [{}] {}", h.kind, h.advice);
    }
    if rest.len() > 8 {
        let _ = writeln!(
            s,
            "  … +{} → nika check {path}",
            crate::text::count(rest.len() - 8, "more hint")
        );
    }
    if let Some(a) = report.analysis.as_ref()
        && let Some(b) = a.blast_radius.first()
    {
        // Noun AND verb agree — `1 downstream task never runs`.
        let _ = writeln!(
            s,
            "  if {} fails, {} never {}",
            b.task,
            crate::text::count(b.blocks, "downstream task"),
            if b.blocks == 1 { "runs" } else { "run" }
        );
    }
}

/// Run it — plus the mock rehearsal when the workflow bills real models.
fn run_section(s: &mut String, path: &str, report: &CheckReport) {
    let _ = writeln!(s, "\nrun it");
    let _ = writeln!(s, "  nika run {path}");
    let all_mock = report
        .requirements
        .models
        .iter()
        .all(|m| m.model.starts_with("mock/"));
    if !report.requirements.models.is_empty() && !all_mock {
        if report.data_journey.writes.is_empty() {
            let _ = writeln!(
                s,
                "  nika run {path} --model mock/echo   # offline rehearsal · zero keys"
            );
        } else {
            // The mock swaps the MODEL, not the effects — a mock re-run
            // after a real one overwrites the real artifacts (gauntlet
            // 2026-07-31: a taught rehearsal destroyed a user's real
            // CHANGELOG). A writing workflow's rehearsal line says so.
            let _ = writeln!(
                s,
                "  nika run {path} --model mock/echo   # mock model · file writes STILL land — rehearse before the real run, not after"
            );
        }
    }
}

/// The flight recorder — what already happened here, and that it is
/// provable. The full path prints ONCE (the read command); verify names
/// the same file with « it » — three repetitions of a 45-char path was
/// the wall-of-text tell the 80-column read caught.
fn recorder_section(s: &mut String, traces: Option<&(usize, String)>) {
    match traces {
        Some((n, latest)) => {
            let _ = writeln!(
                s,
                "\nflight recorder\n  {} in .nika/traces · latest:\n  \
                 nika trace show {latest}\n  \
                 nika trace verify <same file>   # prove the hash chain",
                crate::text::count(*n, "run")
            );
        }
        None => {
            let _ = writeln!(
                s,
                "\nflight recorder\n  no runs recorded here yet — every run writes a \
                 tamper-evident, hash-chained trace to .nika/traces/"
            );
        }
    }
}

/// The versioned machine twin — reuses the report's own serialized
/// vocabulary so `check --json` and `explain --json` speak one dialect.
fn render_json(
    path: &str,
    doc: &GraphDoc,
    report: &CheckReport,
    permits_declared: bool,
    traces: Option<&(usize, String)>,
    forecast: Option<&super::forecast::ForecastReport>,
) -> String {
    let wave_sizes: Vec<usize> = report.waves.iter().map(Vec::len).collect();
    let mut waves: Vec<Vec<&str>> = Vec::with_capacity(wave_sizes.len());
    let mut cursor = 0usize;
    for &size in &wave_sizes {
        let end = cursor.saturating_add(size).min(doc.nodes.len());
        waves.push(
            doc.nodes[cursor..end]
                .iter()
                .map(|n| n.id.as_str())
                .collect(),
        );
        cursor = end;
    }
    let tasks: Vec<serde_json::Value> = doc
        .nodes
        .iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "verb": n.verb,
                "story": task_line(n),
                "model": n.model,
                "tool": n.tool,
                "when": n.when,
            })
        })
        .collect();
    let mut v = serde_json::json!({
        "explain_version": 1,
        "file": path,
        "workflow": doc.workflow,
        "clean": true,
        "tasks": tasks,
        "waves": waves,
        "cost": report.cost,
        "requirements": report.requirements,
        "permits_declared": permits_declared,
        "hints": report.hints,
        "analysis": report.analysis,
        "traces": traces.map(|(n, latest)| serde_json::json!({"count": n, "latest": latest})),
    });
    // Under --forecast the key is ALWAYS present (an agent that asked
    // receives the honest empty shape); without the flag, presence is
    // gated at the auto threshold — an absent key is the deliberate
    // "not gathered" signal, never an ambiguous null.
    if let Some(fc) = forecast {
        v["forecast"] = serde_json::to_value(fc).unwrap_or(serde_json::Value::Null);
    }
    v.to_string()
}

#[cfg(test)]
mod tests;
