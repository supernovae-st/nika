// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check` — ADR-092 static ladder. Human report + `--json` machine surface.

/// Flags for one check dispatch. Several files use [`run_many`]; `--json` is one-file.
pub struct CheckFlags {
    pub json: bool,
    pub infer_permits: bool,
    pub native_strict: bool,
    pub profile: Profile,
}

/// `--profile`: advisory displays the grade; operational fails at High/Unbounded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Profile {
    /// Grade displayed, never gating (the default).
    #[default]
    Advisory,
    /// Grade ≥ High fails the audit (exit 2).
    Operational,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(clap::Args)]
pub struct CheckArgs {
    /// Workflow file(s), `-`, or `registry:owner/name[@version]`.
    #[arg(num_args = 0..)]
    pub files: Vec<String>,
    /// Machine projection (`report_version: 1`).
    #[arg(long)]
    pub json: bool,
    /// Print an inferred `permits:` boundary.
    #[arg(long)]
    pub infer_permits: bool,
    /// Apply typed rename repairs and re-audit.
    #[arg(long)]
    pub fix: bool,
    /// Fail when any `native-first` hint remains.
    #[arg(long)]
    pub native_strict: bool,
    /// Advisory displays the grade; operational fails at High/Unbounded.
    #[arg(long, value_enum, default_value_t = Profile::Advisory)]
    pub profile: Profile,
    /// Price as if this `<provider>/<model>` replaced the envelope default.
    #[arg(long)]
    pub model: Option<String>,
}

/// Check input plus `--fix` provenance; a registry cache path is never writable.
#[derive(Debug, Clone)]
pub struct CheckTarget {
    pub(crate) path: String,
    pub(crate) repair_target: nika_display::check_render::RepairTarget,
}

impl CheckTarget {
    #[must_use]
    pub fn workspace(path: impl Into<String>) -> Self {
        let path = path.into();
        let repair_target = crate::registry::repair_target_for_path(&path);
        Self {
            path,
            repair_target,
        }
    }

    #[must_use]
    pub fn registry_artifact(cache_path: impl Into<String>) -> Self {
        Self {
            path: cache_path.into(),
            repair_target: nika_display::check_render::RepairTarget::RegistryArtifact,
        }
    }

    fn is_stdin(&self) -> bool {
        self.repair_target == nika_display::check_render::RepairTarget::Stdin
    }

    fn is_registry_artifact(&self) -> bool {
        self.repair_target == nika_display::check_render::RepairTarget::RegistryArtifact
    }

    fn is_non_regular_source(&self) -> bool {
        self.repair_target == nika_display::check_render::RepairTarget::NonRegularSource
    }
}

#[must_use]
pub fn dispatch(
    files: &[String],
    flags: &CheckFlags,
    fix: bool,
    model: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let targets: Vec<CheckTarget> = files.iter().cloned().map(CheckTarget::workspace).collect();
    dispatch_targets(&targets, flags, fix, model, theme)
}

/// [`dispatch`] over already-acquired inputs whose registry provenance has
/// been retained by the binary's resolution seam.
#[must_use]
pub fn dispatch_targets(
    targets: &[CheckTarget],
    flags: &CheckFlags,
    fix: bool,
    model: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let CheckFlags {
        json,
        infer_permits,
        native_strict,
        profile,
    } = *flags;
    if fix {
        // --fix rewrites one regular workspace file.
        if json || infer_permits {
            return crate::verbs::fix::refuse(
                "--fix pairs with the plain audit only (not --json / --infer-permits)",
            );
        }
        return match targets {
            [target] if target.is_registry_artifact() => crate::verbs::fix::refuse(
                "a registry artifact is digest-pinned — copy it into your workspace, then fix the copy",
            ),
            [target] if target.is_non_regular_source() => crate::verbs::fix::refuse(
                "a device, FIFO, or other non-regular source cannot be rewritten — save or copy it into a regular workspace file, then fix the copy",
            ),
            [target] if !target.is_stdin() => {
                crate::verbs::fix::run(&target.path, native_strict, model, theme)
            }
            [_] => {
                crate::verbs::fix::refuse("stdin (`-`) has no file to rewrite — name a real path")
            }
            _ => crate::verbs::fix::refuse(
                "one file per repair loop — loop the files, one --fix per call",
            ),
        };
    }
    if let [target] = targets {
        if infer_permits {
            run_infer_permits(&target.path, json)
        } else {
            run_target_with_profile(target, json, native_strict, profile, model, theme)
        }
    } else if json || infer_permits {
        VerbOutput {
            text: "check: --json and --infer-permits report ONE file per call \
                   (report_version 1 is a per-file contract)\n  fix: loop the \
                   files, one check per call\n"
                .to_owned(),
            code: crate::verbs::exit::ENV,
        }
    } else if targets.iter().any(CheckTarget::is_stdin) {
        VerbOutput {
            text: "check: stdin (`-`) cannot join a multi-file audit\n  fix: \
                   pipe one call per stream, or name the files\n"
                .to_owned(),
            code: crate::verbs::exit::ENV,
        }
    } else {
        run_many_targets(targets, native_strict, profile, model, theme)
    }
}

use std::fmt::Write as _;

use nika_check::CheckReport;
use nika_check::infer_permits;
#[cfg(test)]
use nika_schema::raw::RawWorkflow;

use crate::display::theme::{Role, Theme};
use crate::verbs::{RunSource, VerbOutput, load_checked, load_checked_run_source};

mod budget;
mod drift;
pub(crate) mod energy;
pub(crate) mod models_rung;
mod project;
use models_rung::{ModelFinding, ModelsAudit, pricing_section, unresolvable_models};

use nika_display::check_render::{RepairTarget, render, stamp_unfilled_slots};
#[cfg(test)]
use nika_display::check_render::{permits, required_inputs};

/// `nika check <file>`. `native_strict` promotes native-first hints to exit 2.
#[must_use]
pub fn run(
    path: &str,
    json: bool,
    native_strict: bool,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    run_with_profile(
        path,
        json,
        native_strict,
        Profile::Advisory,
        model_override,
        theme,
    )
}

/// Recompute the report as if `nika run --model` overrode the envelope.
fn overridden(
    wf: nika_schema::raw::RawWorkflow,
    report: nika_check::CheckReport,
    model_override: Option<&str>,
) -> (nika_schema::raw::RawWorkflow, nika_check::CheckReport) {
    match model_override {
        Some(m) => {
            let wf = crate::verbs::with_model_override(&wf, m);
            let report = nika_check::check(&wf);
            (wf, report)
        }
        None => (wf, report),
    }
}

/// Native-strict and operational-profile footers, only when their gate fired.
fn strict_footers(
    text: &mut String,
    theme: Theme,
    native_red: bool,
    native_hints: usize,
    operational_red: bool,
    grade: nika_check::RiskGrade,
) {
    if native_red {
        let hint_word = if native_hints == 1 { "hint" } else { "hints" };
        let _ = writeln!(
            text,
            " {}",
            theme.paint(
                Role::Bad,
                &format!(
                    "✖ native-strict · {native_hints} native-first {hint_word} above — \
                     replace each one with the builtin its hint names \
                     (the exec ledger documents intent for a reviewer; \
                     it does not clear this gate)"
                ),
            )
        );
    }
    if operational_red {
        let _ = writeln!(
            text,
            " {}",
            theme.paint(
                Role::Bad,
                // The grade names WHY; the fix direction mirrors the
                // COST/hint lanes (cap the spend · narrow the grant).
                &format!(
                    "✖ operational · risk {} — cap the spend or narrow the grant: \
                     glob/wildcard authority and uncapped autonomy block readiness \
                     under --profile operational (advisory by default)",
                    grade.as_str()
                )
            )
        );
    }
}

/// How many `native-first` hints survive — the count `--native-strict`
/// folds into the verdict, and the only hint family that ever does.
fn native_hint_count(report: &CheckReport) -> usize {
    report
        .hints
        .iter()
        .filter(|h| h.kind == "native-first")
        .count()
}

/// The project-file route, taken BEFORE the workflow envelope is applied.
///
/// The envelope cannot describe a project file — it refuses `ceiling:` as
/// an unknown field and demands a `tasks:` map, which is the destructive
/// advice this route exists to end. The discriminant is the spec's
/// (`01-envelope` §The type discriminant): a `tasks:` key means WORKFLOW,
/// its absence means PROJECT, at full coverage and independent of the
/// filename.
fn project_route(path: &str, json: bool) -> Option<VerbOutput> {
    let yaml = read_source(path)?;
    nika_vocab::project::is_project_document(&yaml).then(|| project::judge(path, &yaml, json))
}

/// Read the document once for the discriminant.
///
/// `-` (stdin) is deliberately NOT routed: stdin can be consumed only
/// once, and silently swallowing it here would break the workflow lane
/// that owns it. A project file piped on stdin still meets the workflow
/// envelope — a known edge, named rather than half-handled.
fn read_source(path: &str) -> Option<String> {
    if path == "-" {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// Like [`run`], with an explicit readiness profile.
#[must_use]
pub fn run_with_profile(
    path: &str,
    json: bool,
    native_strict: bool,
    profile: Profile,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    run_target_with_profile(
        &CheckTarget::workspace(path),
        json,
        native_strict,
        profile,
        model_override,
        theme,
    )
}

fn run_target_with_profile(
    target: &CheckTarget,
    json: bool,
    native_strict: bool,
    profile: Profile,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    if let Some(out) = project_route(&target.path, json) {
        return out;
    }
    let source = match RunSource::capture_with_repair_target(&target.path, target.repair_target) {
        Ok(source) => source,
        Err(out) if json => return parse_fatal_json(&out),
        Err(out) => return out,
    };
    run_source_with_profile(&source, json, native_strict, profile, model_override, theme)
}

pub(crate) fn run_source_with_profile(
    source: &RunSource,
    json: bool,
    native_strict: bool,
    profile: Profile,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let (wf, report) = match load_checked_run_source(source) {
        Ok(pair) => pair,
        Err(out) if json => return parse_fatal_json(&out),
        Err(out) => return out,
    };
    let path = source.logical_path();
    let (wf, report) = overridden(wf, report, model_override);
    let skills = super::resolve_workflow_skills(&wf, super::workflow_base(path));
    render_checked_with_profile(
        source.source(),
        path,
        source.repair_target(),
        &wf,
        &report,
        &skills,
        json,
        native_strict,
        profile,
        theme,
    )
}

/// Render a check verdict from bytes and semantic products already admitted by
/// the execution service. This path never reopens `path` or resolves skills
/// from the filesystem.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_admitted_pair(
    source: &str,
    path: &str,
    repair_target: RepairTarget,
    wf: &nika_schema::raw::RawWorkflow,
    report: &nika_check::CheckReport,
    skills: &nika_schema::ResolvedSkills,
    json: bool,
    theme: Theme,
) -> VerbOutput {
    render_checked_with_profile(
        source,
        path,
        repair_target,
        wf,
        report,
        skills,
        json,
        false,
        Profile::Advisory,
        theme,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_checked_with_profile(
    source: &str,
    path: &str,
    repair_target: RepairTarget,
    wf: &nika_schema::raw::RawWorkflow,
    report: &nika_check::CheckReport,
    skills: &nika_schema::ResolvedSkills,
    json: bool,
    native_strict: bool,
    profile: Profile,
    theme: Theme,
) -> VerbOutput {
    // The declared-vs-used drift family (NIKA-DRIFT-001 · drift.rs) —
    // advisory in both projections, never an exit-code input.
    let drift_hints = drift::scan(wf);
    let native_hints = native_hint_count(report);
    // The MODELS rung (#320): the ladder validated TOOLS but not MODELS —
    // the exact asymmetry a hallucinating agent hits. A `model:` this
    // binary cannot resolve is a FINDING (exit 2), never a green audit.
    let models_audit = unresolvable_models(report, wf);
    // #1066 · `# SLOT:` comments die with the YAML parse, so the AST
    // cannot refuse a scaffold. Scan the source after a successful
    // parse: a Hint is the wrong well (hints never fail).
    let slots = nika_check::scan_unfilled_slots(source);
    let clean = report.is_clean()
        && models_audit.findings.is_empty()
        && skills.findings.is_empty()
        && slots.is_empty();
    // The risk grade (P0-6): a pure projection of the report — uncapped
    // spend or wildcard grants never turn the verdict green by silence.
    // Advisory by default; the operational profile folds grade ≥ High
    // into the exit-2 verdict (the readiness gate).
    let grade = nika_check::risk_grade(report);
    let profile_clean = profile != Profile::Operational || grade < nika_check::RiskGrade::High;
    let strict_clean = clean && profile_clean && (!native_strict || native_hints == 0);

    // W8 metrics (audit UX 2026-07-30): a green audit is the check_passed
    // event — content-free by construction, off unless NIKA_METRICS=1.
    if strict_clean {
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::CheckPassed,
            crate::metrics::Facts::none(),
        );
    }

    if json {
        return json_verdict(
            report,
            wf,
            &models_audit,
            skills,
            &drift_hints,
            &slots,
            clean,
            strict_clean,
            native_strict,
            grade,
            profile,
        );
    }

    let mut text = render(
        report,
        wf,
        source,
        path,
        repair_target,
        theme,
        &models_audit,
        skills,
        &drift_hints,
        // THE verdict, computed once above — the footer shows it, the
        // exit code rides it, `--json` serializes it (P0-11: the human
        // surface used to re-decide on `report.is_clean()` alone and
        // painted `✔ audited` under a `✖ MODELS` rung at exit 2).
        clean,
    );
    stamp_unfilled_slots(&mut text, &slots, theme);
    strict_footers(
        &mut text,
        theme,
        native_strict && report.is_clean() && native_hints > 0,
        native_hints,
        profile == Profile::Operational && clean && !profile_clean,
        grade,
    );
    naming_note(&mut text, theme, path, wf);
    budget::footnote(&mut text, theme);
    // The `--ascii` byte contract (P1 · audit UX 2026-07-30): the finished
    // report folds through the ONE enforcement seam — the glyph twins stay
    // the primary mechanism, this fold is what makes the emitted bytes
    // ASCII by construction (a no-op on the unicode register).
    if strict_clean {
        VerbOutput::ok(nika_display::vocab::sober(theme, &text))
    } else {
        VerbOutput::file(nika_display::vocab::sober(theme, &text))
    }
}

/// The accidental-rename note (INFO, never a refusal).
///
/// Copy `foo.nika.yaml` to `bar.nika.yaml`, forget the header, and every
/// trace keeps saying `foo`: the file moved, its identity did not.
///
/// A NOTE because divergence is usually deliberate — the spec's own
/// example is `deploy.nika.yaml` carrying `nika: deploy-to-prod`, and a
/// numbered path puts curriculum order in the filename. So an ordering
/// prefix is stripped before comparing; a different WORD is the
/// accidental shape. The filename is a location `git mv` may change, the
/// name is an identity that rides traces — renaming one must never
/// silently re-identify the other.
fn naming_note(text: &mut String, theme: Theme, path: &str, wf: &nika_schema::raw::RawWorkflow) {
    let Some(name) = wf.workflow.as_ref().map(|n| n.value.as_str()) else {
        return;
    };
    let Some(stem) = std::path::Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .and_then(|f| {
            f.strip_suffix(".nika.yaml")
                .or_else(|| f.strip_suffix(".nika.yml"))
        })
    else {
        return;
    };
    // An ordering prefix (`01-`, `02_`) is a deliberate convention.
    let bare = stem
        .find(|c: char| !c.is_ascii_digit())
        .filter(|&i| i > 0 && matches!(stem.as_bytes()[i], b'-' | b'_'))
        .map_or(stem, |i| &stem[i + 1..]);
    if bare == name {
        return;
    }
    let _ = write!(
        text,
        "\n {} the file is `{stem}` and its name is `{name}` — traces, journal \
         events and errors will all say `{name}`. Deliberate is fine; a \
         forgotten header after a copy is not.\n",
        theme.paint(Role::Dim, "note ·")
    );
}

/// `--json` parse-fatal verdict: one findings row, `parse_fatal: true`.
fn parse_fatal_json(out: &VerbOutput) -> VerbOutput {
    let text = out.text.trim();
    // The plain voice is `PARSE ✗  [NIKA-…] message` on the FIRST line;
    // a span-carrying refusal (#1075) appends a rustc-grade frame under
    // it. Scrape the diagnostic line only — the frame is human, not
    // the finding's message.
    // An env-class refusal (unreadable file) has no code and stays codeless.
    let line = text.lines().next().unwrap_or(text);
    let code = line
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(code, _)| code.to_owned());
    let message = line.split_once("] ").map_or(line, |(_, m)| m).to_owned();
    let mut finding = serde_json::json!({
        "kind": "parse",
        "gate": "PARSE",
        "severity": "error",
        "message": message,
    });
    if let Some(c) = &code {
        finding["code"] = serde_json::json!(c);
        finding["docs_url"] = serde_json::json!(format!("{}/{c}", nika_check::ERROR_DOCS_BASE));
    }
    let payload = serde_json::json!({
        "report_version": nika_check::REPORT_VERSION,
        "parse_fatal": true,
        "clean": false,
        "findings": [finding],
    });
    VerbOutput {
        text: format!("{payload:#}"),
        code: out.code,
    }
}

/// Shared `--json` MODELS row shape. `code` rides only when the refusal
/// is a spec claim (`NIKA-PROVIDER` for a missing/unknown prefix); the
/// azure class stays engine-local (#761).
fn model_finding_rows(findings: &[ModelFinding]) -> serde_json::Value {
    serde_json::Value::Array(
        findings
            .iter()
            .map(|f| {
                let mut row = serde_json::json!({
                    "model": f.model,
                    "tasks": f.tasks,
                    "why": f.why,
                });
                if let Some(code) = &f.code {
                    row["code"] = serde_json::json!(code);
                }
                row
            })
            .collect(),
    )
}

/// Fold unfilled SLOT comments into `--json` `findings[]` so a consumer
/// looping that list sees the same refusal the human card prints.
fn stamp_slot_findings(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    slots: &[nika_check::SlotMarker],
) {
    if slots.is_empty() {
        return;
    }
    let mut row = serde_json::Map::new();
    row.insert("kind".into(), serde_json::Value::String("slot".into()));
    row.insert("gate".into(), serde_json::Value::String("SLOT".into()));
    row.insert("severity".into(), serde_json::Value::String("error".into()));
    row.insert(
        "message".into(),
        serde_json::Value::String(nika_check::slot_refusal_message(slots)),
    );
    if let Some(first) = slots.first() {
        row.insert(
            "span".into(),
            serde_json::json!({ "start": first.span.start, "end": first.span.end }),
        );
    }
    if let Some(findings) = obj
        .get_mut("findings")
        .and_then(serde_json::Value::as_array_mut)
    {
        findings.insert(0, serde_json::Value::Object(row));
    }
}

fn stamp_engine_identity(obj: &mut serde_json::Map<String, serde_json::Value>) {
    let identity = nika_runtime::engine_identity();
    obj.insert("engine_version".into(), identity.engine_version().into());
    obj.insert("build_sha".into(), identity.build_sha().into());
    obj.insert("spec_sha".into(), identity.spec_sha().into());
}

/// `--json` verdict object. Drift and one-obvious-way rows append to
/// `hints[]` plus their `code`. Both families are warnings — `clean`
/// never reads them.
#[allow(clippy::too_many_arguments)] // the verdict's seams, one each — the render.rs:427 precedent
fn json_verdict(
    report: &CheckReport,
    wf: &nika_schema::raw::RawWorkflow,
    models_audit: &ModelsAudit,
    skills: &nika_schema::ResolvedSkills,
    drift_hints: &[String],
    slots: &[nika_check::SlotMarker],
    clean: bool,
    strict_clean: bool,
    native_strict: bool,
    grade: nika_check::RiskGrade,
    profile: Profile,
) -> VerbOutput {
    let model_findings = &models_audit.findings;
    let mut payload = match serde_json::to_value(report) {
        Ok(v) => v,
        Err(e) => return VerbOutput::env(format!("cannot serialize report: {e}")),
    };
    if let Some(obj) = payload.as_object_mut() {
        if let Some(hints) = obj
            .get_mut("hints")
            .and_then(serde_json::Value::as_array_mut)
        {
            push_advisory_json_hints(hints, drift_hints, wf);
        }
        stamp_slot_findings(obj, slots);
        obj.insert("clean".to_owned(), serde_json::Value::Bool(clean));
        obj.insert(
            "models_resolve".to_owned(),
            serde_json::Value::Bool(model_findings.is_empty()),
        );
        // Presence-gated: judged-green ≠ never-judged.
        if models_audit.unjudged > 0 {
            obj.insert(
                "models_unjudged".to_owned(),
                serde_json::json!(models_audit.unjudged),
            );
        }
        if !model_findings.is_empty() {
            obj.insert(
                "model_findings".to_owned(),
                model_finding_rows(model_findings),
            );
        }
        // Presence-gated like its siblings: the catalog cross-check
        // (advisory — `clean` is untouched; a machine consumer that
        // wants to block on it can).
        if !models_audit.catalog_warnings.is_empty() {
            obj.insert(
                "models_catalog_warnings".to_owned(),
                model_finding_rows(&models_audit.catalog_warnings),
            );
        }
        // The access-plan rows (D-2026-08-04-N1 · P2.5): HOW this
        // machine would reach each judged model — MACHINE truth (env
        // key presence), presence-gated and advisory like its
        // siblings; `clean` and the exit codes never read it.
        let access_rows = models_rung::access_plan_rows(report);
        if !access_rows.is_empty() {
            obj.insert(
                "access_plan".to_owned(),
                serde_json::Value::Array(access_rows),
            );
        }
        skills.extend_check_json(obj);
        budget::stamp_json(obj);
        obj.insert(
            "pricing".to_owned(),
            pricing_section(report, model_findings),
        );
        // The grade rides EVERY payload (advisory included) — text, JSON
        // and the exit code render the one verdict (P0-11's law).
        obj.insert(
            "risk_grade".to_owned(),
            serde_json::Value::String(grade.as_str().to_owned()),
        );
        if profile == Profile::Operational {
            obj.insert(
                "operational_clean".to_owned(),
                serde_json::Value::Bool(strict_clean),
            );
        }
        if native_strict {
            obj.insert(
                "native_strict_clean".to_owned(),
                serde_json::Value::Bool(strict_clean),
            );
        }
        stamp_engine_identity(obj);
        nika_check::stamp_paid_ready(obj, &report.hints);
    }
    let text = format!("{payload:#}");
    if strict_clean {
        VerbOutput::ok(text)
    } else {
        VerbOutput::file(text)
    }
}

/// Drift + one-obvious-way rows on the machine `hints[]`.
///
/// Native-first already rides `CheckReport.hints` (kind + numbered
/// `code` + task + advice that starts with the rule id). One-obvious-way
/// lives in `nika-lints` and cannot join that report without a
/// nika-check → nika-lints cycle, so this edge is the public door (#763).
fn push_advisory_json_hints(
    hints: &mut Vec<serde_json::Value>,
    drift_hints: &[String],
    wf: &nika_schema::raw::RawWorkflow,
) {
    for advice in drift_hints {
        hints.push(serde_json::json!({
            "kind": "drift",
            "task": "-",
            "advice": advice,
            "code": drift::DRIFT_CODE,
        }));
    }
    for lint in nika_lints::one_obvious_way(wf) {
        hints.push(serde_json::json!({
            "kind": "one-obvious-way",
            "code": lint.rule,
            "task": lint.task_id,
            "advice": format!("{} · {}", lint.rule, lint.message),
        }));
    }
}

/// Several files through the same per-file ladder — the pre-commit / CI
/// shape (`nika check a.nika.yaml b.nika.yaml`). Each file gets the FULL
/// [`run`] report (its header names the file), every file still audits
/// after an earlier failure (no stop-at-first — the hook UX law), and the
/// worst spec-§4 exit survives (3 environment > 2 findings). The machine
/// modes stay one-file-per-call — `report_version: 1` is a per-file
/// contract — so `main` refuses `--json`/`--infer-permits` upstream
/// before this is reached.
#[must_use]
pub fn run_many(
    paths: &[String],
    native_strict: bool,
    profile: Profile,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let targets: Vec<CheckTarget> = paths.iter().cloned().map(CheckTarget::workspace).collect();
    run_many_targets(&targets, native_strict, profile, model_override, theme)
}

fn run_many_targets(
    targets: &[CheckTarget],
    native_strict: bool,
    profile: Profile,
    model_override: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    let mut texts = Vec::with_capacity(targets.len());
    let mut worst = crate::verbs::exit::OK;
    for target in targets {
        let out =
            run_target_with_profile(target, false, native_strict, profile, model_override, theme);
        texts.push(out.text);
        worst = worst.max(out.code);
    }
    VerbOutput {
        text: texts.join("\n"),
        code: worst,
    }
}

/// `nika check --infer-permits` — write the boundary FOR the operator.
#[must_use]
pub fn run_infer_permits(path: &str, json: bool) -> VerbOutput {
    let (wf, _report) = match load_checked(path) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    let inferred = infer_permits(&wf);
    if json {
        let payload = serde_json::json!({
            "permits_yaml": inferred.to_yaml(),
            "notes": inferred.notes,
        });
        return VerbOutput::ok(format!("{payload:#}"));
    }
    let mut text = inferred.to_yaml();
    if !inferred.notes.is_empty() {
        text.push_str("\n# review — effects too dynamic to pin statically:\n");
        for note in &inferred.notes {
            let _ = writeln!(text, "#   · {note}");
        }
    }
    VerbOutput::ok(text)
}

#[cfg(test)]
mod lints_surface;
#[cfg(test)]
mod repair_tests;
#[cfg(test)]
mod tests;
