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
#[command(after_help = nika_cli_host::help_card::CHECK_EXITS)]
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
    /// Judge the access plan under this pin (the same value `run --access` takes).
    #[arg(long)]
    pub access: Option<String>,
    /// Internal SDK adapter: emit the report with its immutable snapshot.
    #[arg(long, hide = true)]
    pub sdk_snapshot: bool,
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
    (model, access): (Option<&str>, Option<&str>),
    theme: Theme,
) -> VerbOutput {
    let targets: Vec<CheckTarget> = files.iter().cloned().map(CheckTarget::workspace).collect();
    dispatch_targets(&targets, flags, fix, (model, access), theme)
}

/// [`dispatch`] over already-acquired inputs whose registry provenance has
/// been retained by the binary's resolution seam.
#[must_use]
pub fn dispatch_targets(
    targets: &[CheckTarget],
    flags: &CheckFlags,
    fix: bool,
    (model, access): (Option<&str>, Option<&str>),
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
            run_target_with_profile(target, json, native_strict, profile, (model, access), theme)
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
        run_many_targets(targets, native_strict, profile, (model, access), theme)
    }
}

use std::fmt::Write as _;

use nika_check::CheckReport;
use nika_check::infer_permits;
use nika_schema::raw::RawWorkflow;

use crate::display::theme::{Role, Theme};
use crate::verbs::{RunSource, VerbOutput, load_checked, load_checked_run_source};

mod budget;
mod drift;
pub(crate) mod energy;
pub(crate) mod models_rung;
mod project;
use models_rung::{
    ModelFinding, ModelsAudit, VerdictLayers, capacity_findings, pricing_section,
    thinking_findings, unresolvable_models,
};

use nika_display::check_render::{RepairTarget, render};
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
        (model_override, None),
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
    overrides: (Option<&str>, Option<&str>),
    theme: Theme,
) -> VerbOutput {
    run_target_with_profile(
        &CheckTarget::workspace(path),
        json,
        native_strict,
        profile,
        overrides,
        theme,
    )
}

fn run_target_with_profile(
    target: &CheckTarget,
    json: bool,
    native_strict: bool,
    profile: Profile,
    overrides: (Option<&str>, Option<&str>),
    theme: Theme,
) -> VerbOutput {
    run_target_with_profile_and_slots(
        target,
        json,
        native_strict,
        profile,
        overrides,
        theme,
        false,
    )
}

/// Audit a freshly scaffolded recipe. An unfilled scaffold is expected at
/// this boundary, so a report containing only SLOT findings is a successful
/// founding receipt. Every other finding keeps the normal exit-2 refusal.
#[must_use]
pub(crate) fn run_scaffold(path: &str, theme: Theme) -> VerbOutput {
    run_target_with_profile_and_slots(
        &CheckTarget::workspace(path),
        false,
        false,
        Profile::Advisory,
        (None, None),
        theme,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_target_with_profile_and_slots(
    target: &CheckTarget,
    json: bool,
    native_strict: bool,
    profile: Profile,
    overrides: (Option<&str>, Option<&str>),
    theme: Theme,
    allow_slot_only: bool,
) -> VerbOutput {
    if let Some(out) = project_route(&target.path, json) {
        return out;
    }
    let source = match RunSource::capture_with_repair_target(&target.path, target.repair_target) {
        Ok(source) => source,
        Err(out) if json => return parse_fatal_json(&out),
        Err(out) => return out,
    };
    run_source_with_profile_and_slots(
        &source,
        json,
        native_strict,
        profile,
        overrides,
        theme,
        allow_slot_only,
    )
}

pub(crate) fn run_source_with_profile(
    source: &RunSource,
    json: bool,
    native_strict: bool,
    profile: Profile,
    overrides: (Option<&str>, Option<&str>),
    theme: Theme,
) -> VerbOutput {
    run_source_with_profile_and_slots(
        source,
        json,
        native_strict,
        profile,
        overrides,
        theme,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_source_with_profile_and_slots(
    source: &RunSource,
    json: bool,
    native_strict: bool,
    profile: Profile,
    (model_override, access_pin): (Option<&str>, Option<&str>),
    theme: Theme,
    allow_slot_only: bool,
) -> VerbOutput {
    let (wf, report) = match load_checked_run_source(source) {
        Ok(pair) => pair,
        Err(out) if json => return parse_fatal_json(&out),
        Err(out) => return out,
    };
    let path = source.logical_path();
    let (wf, report) = overridden(wf, report, model_override);
    let skills = super::resolve_workflow_skills(&wf, super::workflow_base(path));
    let slot_only = allow_slot_only
        && !report.slot_findings.is_empty()
        && report.findings.iter().all(|finding| finding.kind == "slot")
        && unresolvable_models(&report, &wf).findings.is_empty()
        && thinking_findings(&wf).is_empty()
        && capacity_findings(&wf).is_empty()
        && skills.findings.is_empty();
    let out = render_checked_with_profile(
        source.source(),
        path,
        source.repair_target(),
        &wf,
        &report,
        &skills,
        json,
        native_strict,
        profile,
        access_pin,
        theme,
    );
    if slot_only {
        VerbOutput::ok(out.text)
    } else {
        out
    }
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
        None,
        theme,
    )
}

/// Emit the normal machine check report plus its exact execution snapshot.
///
/// This is an explicit adapter API rather than part of [`run`]: ordinary
/// `check --json` responses stay small, while SDK callers that need a durable
/// owned-byte world opt into the `execution_snapshot` field. The workflow is
/// captured and judged once through `nika-execution`, so the report and the
/// exported bytes cannot observe different filesystem states.
#[must_use]
pub fn run_snapshot_export(path: &str, theme: Theme) -> VerbOutput {
    if path == "-" {
        return snapshot_export_refusal(
            "stdin snapshot export requires an already-held project adapter",
            crate::verbs::exit::ENV,
        );
    }
    let (project, root) = match snapshot_project(path) {
        Ok(parts) => parts,
        Err(error) => return snapshot_export_refusal(&error, crate::verbs::exit::ENV),
    };
    let service = nika_execution::ExecutionService::default();
    let admitted = match service.admit(&project, &root) {
        Ok(admitted) => admitted,
        Err(error) => {
            let code = if matches!(&error, nika_execution::ExecutionError::Io { .. }) {
                crate::verbs::exit::ENV
            } else {
                crate::verbs::exit::FILE
            };
            return snapshot_export_refusal(&error.to_string(), code);
        }
    };
    let snapshot = admitted.snapshot();
    let encoded = match snapshot.encode() {
        Ok(encoded) => encoded,
        Err(error) => {
            return snapshot_export_refusal(&error.to_string(), crate::verbs::exit::FILE);
        }
    };
    let Some(source) = snapshot.text(snapshot.root()) else {
        return snapshot_export_refusal(
            "execution snapshot lost its UTF-8 root unit",
            crate::verbs::exit::FILE,
        );
    };
    let target = CheckTarget::workspace(path);
    let out = run_admitted_pair(
        source,
        path,
        target.repair_target,
        admitted.workflow(),
        admitted.check(),
        admitted.skills(),
        true,
        theme,
    );
    attach_execution_snapshot(out, encoded)
}

fn attach_execution_snapshot(mut out: VerbOutput, encoded: String) -> VerbOutput {
    let mut payload: serde_json::Value = match serde_json::from_str(&out.text) {
        Ok(payload) => payload,
        Err(error) => {
            return snapshot_export_refusal(
                &format!("cannot extend machine check report: {error}"),
                crate::verbs::exit::ENV,
            );
        }
    };
    let Some(object) = payload.as_object_mut() else {
        return snapshot_export_refusal(
            "machine check report is not a JSON object",
            crate::verbs::exit::ENV,
        );
    };
    object.insert(
        "execution_snapshot".to_owned(),
        serde_json::Value::String(encoded),
    );
    out.text = format!("{payload:#}");
    out
}

fn snapshot_export_refusal(message: &str, code: u8) -> VerbOutput {
    let payload = serde_json::json!({
        "error": {
            "message": format!("cannot export execution snapshot: {message}"),
        },
    });
    VerbOutput {
        text: format!("{payload:#}"),
        code,
    }
}

fn snapshot_project(path: &str) -> Result<(nika_fs::OwnedDir, std::path::PathBuf), String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let authored = std::path::Path::new(path);
    let absolute = if authored.is_absolute() {
        authored.to_path_buf()
    } else {
        cwd.join(authored)
    };
    let absolute = lexical_snapshot_path(&absolute);
    let (project_root, logical_root) = absolute.strip_prefix(&cwd).map_or_else(
        |_| {
            let parent = absolute
                .parent()
                .ok_or_else(|| format!("`{path}` has no project directory"))?;
            let name = absolute
                .file_name()
                .ok_or_else(|| format!("`{path}` has no workflow filename"))?;
            Ok::<_, String>((parent.to_path_buf(), std::path::PathBuf::from(name)))
        },
        |relative| Ok((cwd, relative.to_path_buf())),
    )?;
    let project = nika_fs::OwnedDir::open(&project_root)
        .map_err(|error| format!("cannot hold project `{}`: {error}", project_root.display()))?;
    Ok((project, logical_root))
}

fn lexical_snapshot_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => {
                normalized.push(std::path::Path::new(std::path::MAIN_SEPARATOR_STR));
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

/// The MODELS rung's fold + the four layers (wave 2), computed ONCE
/// beside the exit code: a `model:` this binary cannot resolve is a
/// FINDING (exit 2), never a green audit; the thinking and capacity
/// judgments ride the same rung. VALID is the definition (ladder +
/// resolution + skills), CAPACITY FIT the seat against the declaration;
/// `clean` folds both (P0-11: one verdict, every surface). ACCESS READY
/// is the frozen plan this machine resolves for the EFFECTIVE models
/// (`--model` already applied to `wf`) under `--access` — presence
/// only, never a dial.
fn fold_verdicts(
    wf: &nika_schema::raw::RawWorkflow,
    report: &CheckReport,
    skills: &nika_schema::ResolvedSkills,
    access_pin: Option<&str>,
) -> (
    ModelsAudit,
    bool,
    nika_providers::ExecutionAccessPlan,
    VerdictLayers,
) {
    let mut models_audit = unresolvable_models(report, wf);
    let valid = report.is_clean() && models_audit.findings.is_empty() && skills.findings.is_empty();
    let mut capacity = thinking_findings(wf);
    capacity.extend(capacity_findings(wf));
    models_audit.findings.extend(capacity.iter().cloned());
    let plan = nika_cli_host::access::resolve_plan(wf, report, None, access_pin);
    let layers = models_rung::verdict_layers(&plan, valid, &capacity);
    (models_audit, valid && capacity.is_empty(), plan, layers)
}

/// The operational profile's access footer: RUN READY false is a
/// `--profile` outcome (exit 2), and the line names the blocker.
fn access_footer(
    text: &mut String,
    theme: Theme,
    profile: Profile,
    clean: bool,
    layers: &VerdictLayers,
) {
    if profile == Profile::Operational && clean && layers.access_ready == Some(false) {
        let _ = writeln!(
            text,
            " {}",
            theme.paint(
                Role::Bad,
                &format!(
                    "✖ operational · access not ready — {}",
                    layers.blockers.first().map_or("", String::as_str)
                )
            )
        );
    }
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
    access_pin: Option<&str>,
    theme: Theme,
) -> VerbOutput {
    // Declared-vs-used drift (NIKA-DRIFT-001) — advisory, never an exit input.
    let drift_hints = drift::scan(wf);
    let native_hints = native_hint_count(report);
    let (models_audit, clean, plan, layers) = fold_verdicts(wf, report, skills, access_pin);
    // The risk grade (P0-6): a pure projection — advisory by default;
    // the operational profile folds grade ≥ High AND an unready lane
    // into the exit-2 verdict (RUN READY false is a `--profile` outcome).
    let grade = nika_check::risk_grade(report);
    let profile_clean = profile != Profile::Operational
        || (grade < nika_check::RiskGrade::High && layers.access_ready != Some(false));
    let strict_clean = clean && profile_clean && (!native_strict || native_hints == 0);

    // W8 metrics: a green audit is the content-free check_passed event.
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
            (clean, strict_clean, native_strict),
            grade,
            profile,
            (&plan, &layers),
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
        // exit code rides it (P0-11).
        clean,
        &layers,
    );
    strict_footers(
        &mut text,
        theme,
        native_strict && report.is_clean() && native_hints > 0,
        native_hints,
        profile == Profile::Operational && clean && !profile_clean,
        grade,
    );
    access_footer(&mut text, theme, profile, clean, &layers);
    naming_note(&mut text, theme, path, wf);
    budget::footnote(&mut text, theme);
    // The `--ascii` byte contract (P1): the report folds through the
    // ONE enforcement seam — ASCII by construction.
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

fn extend_model_audit(
    object: &mut serde_json::Map<String, serde_json::Value>,
    audit: &ModelsAudit,
) {
    if audit.unjudged > 0 {
        object.insert(
            "models_unjudged".to_owned(),
            serde_json::json!(audit.unjudged),
        );
    }
    for (key, findings) in [
        ("model_findings", audit.findings.as_slice()),
        ("models_catalog_warnings", audit.catalog_warnings.as_slice()),
    ] {
        if !findings.is_empty() {
            object.insert(key.to_owned(), model_finding_rows(findings));
        }
    }
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
    (clean, strict_clean, native_strict): (bool, bool, bool),
    grade: nika_check::RiskGrade,
    profile: Profile,
    (plan, layers): (&nika_providers::ExecutionAccessPlan, &VerdictLayers),
) -> VerbOutput {
    let model_findings = &models_audit.findings;
    let mut payload = match serde_json::to_value(report) {
        Ok(v) => v,
        Err(e) => return VerbOutput::env(format!("cannot serialize report: {e}")),
    };
    let identity = match serde_json::to_value(nika_runtime::engine_identity()) {
        Ok(serde_json::Value::Object(identity)) => identity,
        Ok(_) => return VerbOutput::env("engine identity is not a JSON object".to_owned()),
        Err(error) => {
            return VerbOutput::env(format!("cannot serialize engine identity: {error}"));
        }
    };
    if let Some(obj) = payload.as_object_mut() {
        if let Some(hints) = obj
            .get_mut("hints")
            .and_then(serde_json::Value::as_array_mut)
        {
            push_advisory_json_hints(hints, drift_hints, wf);
        }
        obj.insert("clean".to_owned(), serde_json::Value::Bool(clean));
        obj.insert(
            "models_resolve".to_owned(),
            serde_json::Value::Bool(model_findings.is_empty()),
        );
        // Presence-gated model truth stays advisory; `clean` is untouched.
        extend_model_audit(obj, models_audit);
        // The access-plan rows (D-2026-08-04-N1 · P2.5): HOW this
        // machine would reach each judged model — MACHINE truth (env
        // key presence), presence-gated and advisory like its
        // siblings; `clean` and the exit codes never read it.
        let access_rows = nika_service_execution::access::lane_rows(plan);
        if !access_rows.is_empty() {
            obj.insert(
                "access_plan".to_owned(),
                serde_json::Value::Array(access_rows),
            );
        }
        // Wave 2 · the four layered verdicts, additive beside `clean`
        // (`report_version` stays 1 · `clean` keeps meaning VALID +
        // CAPACITY FIT, what it always folded).
        obj.insert(
            "verdicts".to_owned(),
            serde_json::json!({
                "valid": layers.valid,
                "access_ready": layers.access_ready,
                "capacity_fit": layers.capacity_fit,
                "run_ready": layers.run_ready(),
                "blockers": layers.blockers,
            }),
        );
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
        obj.extend(identity);
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
    run_many_targets(
        &targets,
        native_strict,
        profile,
        (model_override, None),
        theme,
    )
}

fn run_many_targets(
    targets: &[CheckTarget],
    native_strict: bool,
    profile: Profile,
    overrides: (Option<&str>, Option<&str>),
    theme: Theme,
) -> VerbOutput {
    let mut texts = Vec::with_capacity(targets.len());
    let mut worst = crate::verbs::exit::OK;
    for target in targets {
        let out = run_target_with_profile(target, false, native_strict, profile, overrides, theme);
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
    let (wf, report) = match load_checked(path) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    let inferred = infer_permits(&wf);
    let mut yaml = tighten_exec_yaml(&wf, &inferred.to_yaml());
    let code = if report.is_clean() {
        crate::verbs::exit::OK
    } else {
        crate::verbs::exit::FILE
    };
    if json {
        let payload = serde_json::json!({
            "permits_yaml": yaml,
            "notes": inferred.notes,
        });
        return VerbOutput {
            text: format!("{payload:#}"),
            code,
        };
    }
    if !inferred.notes.is_empty() {
        yaml.push_str("\n# review — effects too dynamic to pin statically:\n");
        for note in &inferred.notes {
            let _ = writeln!(yaml, "#   · {note}");
        }
    }
    VerbOutput { text: yaml, code }
}

/// B15 / #1279 adjacent: never paste `exec: true` as the ready block.
/// A shell-form task runs via `sh`; an argv task names its program.
/// Dynamic heads stay comment-only — widening to `true` undoes the
/// tightest-grant teaching `nika explain NIKA-SEC-004` just made.
fn tighten_exec_yaml(wf: &RawWorkflow, yaml: &str) -> String {
    if !yaml.contains("exec: true") {
        return yaml.to_owned();
    }
    let mut programs = std::collections::BTreeSet::new();
    let mut dynamic = false;
    for task in &wf.tasks {
        if let nika_schema::raw::RawAction::Exec(exec) = &task.value.action {
            if let Some(prog) = exec.command.argv_program() {
                if prog.contains("${{") || prog.is_empty() {
                    dynamic = true;
                } else {
                    programs.insert(prog.to_owned());
                }
            } else if let Some(shell) = exec.command.shell_str() {
                if shell.contains("${{") {
                    dynamic = true;
                } else {
                    // The binary that actually runs (`/bin/sh -c`).
                    programs.insert("sh".to_owned());
                }
            } else {
                dynamic = true;
            }
        }
    }
    if dynamic || programs.is_empty() {
        return yaml.replace(
            "  exec: true\n",
            "  # exec: true is not a paste-ready grant — name the binary \
             (`exec: [\"sh\"]` for shell: · the argv program for command:)\n",
        );
    }
    let list = programs
        .iter()
        .map(|p| format!("\"{p}\""))
        .collect::<Vec<_>>()
        .join(", ");
    yaml.replace("  exec: true\n", &format!("  exec: [{list}]\n"))
}

#[cfg(test)]
mod json_and_permits;
#[cfg(test)]
mod lints_surface;
#[cfg(test)]
mod repair_tests;
#[cfg(test)]
mod tests;
