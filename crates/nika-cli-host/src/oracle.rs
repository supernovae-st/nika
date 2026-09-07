// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE static audit every machine door projects (ADR-124): the CLI
//! `check` verb, the MCP `nika_check` tool and the session read the same
//! judgment — parse · the ladder (composed when a reader is given,
//! child-blind when it is not, and the verdict SAYS which) · the MODELS
//! rung (resolution · thinking · capacity · the templated-default law) ·
//! the frozen access plan (ADR-122) · the four layered verdicts (ADR-123)
//! · the risk grade — and [`audit_json`] renders the ONE verdict object.
//! Formatting may differ per door; the semantic verdict may not (the
//! one-door pack's oracle law): a divergence is a failing test
//! (`oracle_parity_e2e.rs`), never a gauntlet finding.
//!
//! A lane a door runs under (`--native-strict` · `--profile
//! operational`) refuses ON that same object: its refusal is a typed
//! [`LaneFinding`] row on `findings[]`, and `clean` is computed from the
//! facts the exit code reads — never a second key a consumer has to know
//! to consult (measured on 0.118.7: `clean: true` beside exit 2).

use std::path::Path;

use nika_check::{CheckReport, RiskGrade};
use nika_display::check_render::{ModelFinding, ModelsAudit, VerdictLayers};
use nika_providers::ExecutionAccessPlan;
use nika_schema::raw::RawWorkflow;
use nika_schema::{ParseMode, ResolvedSkills};
use serde_json::{Map, Value};

use crate::models_rung::{
    capacity_findings, dials_a_model, pricing_section, thinking_findings, unresolvable_models,
    verdict_layers_for,
};

/// The filesystem edge an audit may be given — composition
/// (`invoke: { workflow: … }`) resolves through it. A door without one
/// (the oracle: source only) audits child-blind, and says so.
pub type Reader<'a> = &'a mut dyn FnMut(&str) -> Result<String, String>;

/// How the audit is asked to judge — the two knobs `check` and `run`
/// share: the seat override applied BEFORE judging (`--model`) and the
/// pin the plan is resolved under (`--access`, the one `run` takes).
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct AuditOptions<'a> {
    /// `--model <provider/name>` — swapped into the envelope default
    /// before the ladder runs; per-task `model:` keeps winning.
    pub model_override: Option<&'a str>,
    /// `--access <pin>` — the frozen plan is resolved under it.
    pub access_pin: Option<&'a str>,
}

impl<'a> AuditOptions<'a> {
    /// The two knobs, both optional.
    #[must_use]
    pub fn new(model_override: Option<&'a str>, access_pin: Option<&'a str>) -> Self {
        Self {
            model_override,
            access_pin,
        }
    }
}

/// The lanes a door folds ON TOP of `clean` — additive keys, never a
/// different audit: `--native-strict` (native-first hints refuse) and
/// `--profile operational` (grade ≥ High or access not ready refuses).
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Lanes {
    /// Native-first hints turn the verdict red.
    pub native_strict: bool,
    /// The operational profile: grade ≥ High · ACCESS READY false refuse.
    pub operational: bool,
}

impl Lanes {
    /// Both lanes, explicit.
    #[must_use]
    pub fn new(native_strict: bool, operational: bool) -> Self {
        Self {
            native_strict,
            operational,
        }
    }
}

/// The lane that refused — the typed discriminator a consumer routes
/// on: `findings[].kind` carries [`Lane::kind`], `findings[].gate`
/// carries [`Lane::gate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Lane {
    /// `--native-strict`: a surviving native-first hint refuses.
    NativeStrict,
    /// `--profile operational`, the grade gate: High or worse refuses.
    OperationalRisk,
    /// `--profile operational`, the access gate: ACCESS READY false refuses.
    OperationalAccess,
}

impl Lane {
    /// The `findings[].kind` slug — the unified-finding convention
    /// (`snake_case` · a closed set that only grows).
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NativeStrict => "native_strict",
            Self::OperationalRisk | Self::OperationalAccess => "operational",
        }
    }

    /// The `findings[].gate` keyword — the ladder rung the human render
    /// files the refusal under: the two footers' own word
    /// (`✖ native-strict ·` · `✖ operational ·`) upper-cased, and the
    /// ACCESS rung for the access row (the rung above the footer already
    /// printed that refusal).
    #[must_use]
    pub const fn gate(self) -> &'static str {
        match self {
            Self::NativeStrict => "NATIVE-STRICT",
            Self::OperationalRisk => "OPERATIONAL",
            Self::OperationalAccess => "ACCESS",
        }
    }
}

/// The one remedy the native-strict gate accepts. The human footer and
/// the machine row print THIS string — a second wording once offered the
/// exec ledger as an escape it is not.
pub const NATIVE_STRICT_FIX: &str = "replace each `exec:` with the builtin its hint names \
     (the exec ledger documents intent for a reviewer; it does not clear this gate)";

/// The remedy the operational grade gate names at High, where the
/// audited line's handle is empty and the lanes above carry the cause
/// (a glob grant · an unconsumed human gate · an unpinned secret
/// egress); the grade says WHY.
pub const OPERATIONAL_GRADE_FIX: &str = "cap the spend or narrow the grant: glob authority, \
     an unconsumed human gate or an unpinned secret egress block readiness";

/// One refusal a lane folds ON TOP of the report — the typed fact the
/// `--json` `findings[]` row and the human footer both project, so the
/// two cannot disagree. The verdict object's `clean` is false exactly
/// when the report is dirty or one of these exists: the predicate the
/// exit code reads.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LaneFinding {
    /// The lane that refused.
    pub lane: Lane,
    /// The resolvable identity (`native-first/00N` · `nika explain`
    /// teaches it) when the lane promotes a coded hint. The operational
    /// gate carries none: a conjured code would not resolve.
    pub code: Option<String>,
    /// The task it concerns, when it names one.
    pub task: Option<String>,
    /// What was refused, in the hint's or the gate's own words.
    pub detail: String,
    /// The repair, when the gate knows one.
    pub fix: Option<String>,
}

impl LaneFinding {
    /// The unified `message` shape every report finding uses: the
    /// detail, then ` — fix: …` when a repair exists.
    #[must_use]
    pub fn message(&self) -> String {
        match &self.fix {
            Some(fix) => format!("{} — fix: {fix}", self.detail),
            None => self.detail.clone(),
        }
    }

    /// The `findings[]` row: the keys a report finding carries (`kind` ·
    /// `gate` · `severity` · `code` · `task` · `message`) plus `fix`
    /// when one exists — the typed repair beside the prose.
    fn row(&self) -> Value {
        let mut row = serde_json::json!({
            "kind": self.lane.kind(),
            "gate": self.lane.gate(),
            "severity": "error",
            "message": self.message(),
        });
        if let Some(code) = &self.code {
            row["code"] = Value::String(code.clone());
        }
        if let Some(task) = &self.task {
            row["task"] = Value::String(task.clone());
        }
        if let Some(fix) = &self.fix {
            row["fix"] = Value::String(fix.clone());
        }
        row
    }

    /// A native-first hint the strict lane promotes: its code and task,
    /// its advice as the detail (the `code · ` prefix the advice repeats
    /// is dropped — `code` carries it), the one remedy that clears the
    /// gate.
    fn native_first(hint: &nika_check::Hint) -> Self {
        let detail = hint
            .code
            .and_then(|code| hint.advice.strip_prefix(format!("{code} · ").as_str()))
            .unwrap_or(hint.advice.as_str())
            .to_owned();
        Self {
            lane: Lane::NativeStrict,
            code: hint.code.map(str::to_owned),
            task: Some(hint.task.clone()),
            detail,
            fix: Some(NATIVE_STRICT_FIX.to_owned()),
        }
    }

    /// An operational gate row — no code, no task: the gate judges the
    /// whole file.
    fn operational(lane: Lane, detail: String, fix: Option<&str>) -> Self {
        Self {
            lane,
            code: None,
            task: None,
            detail,
            fix: fix.map(str::to_owned),
        }
    }
}

/// What the audit judged WITH the filesystem and what it could not — the
/// verdict carries this, so a child-blind lane never reads as a clean
/// composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Judged {
    /// `invoke: { workflow: … }` children were read and judged.
    pub composition: bool,
    /// `skills:` files were read and judged.
    pub skills: bool,
}

impl Judged {
    /// The CLI's posture: everything the operator named is read.
    pub const FULL: Self = Self {
        composition: true,
        skills: true,
    };

    /// The two halves, explicit.
    #[must_use]
    pub fn new(composition: bool, skills: bool) -> Self {
        Self {
            composition,
            skills,
        }
    }
}

/// The typed verdict — computed ONCE from a judged workflow, projected
/// by every door. The `clean` FIELD is VALID + CAPACITY FIT, what
/// [`judge`] folds with no lane in hand (ADR-123); the verdict OBJECT's
/// `clean` key is [`Verdict::strict_clean`] — the lanes fold into it as
/// typed [`LaneFinding`] rows, the same rows the exit code reads.
#[non_exhaustive]
pub struct Verdict {
    /// The MODELS rung: resolver refusals + the thinking and capacity
    /// findings (the fold `clean` reads) + the catalog warnings.
    pub models: ModelsAudit,
    /// The thinking + capacity subset — the CAPACITY FIT layer.
    pub capacity: Vec<ModelFinding>,
    /// The frozen access plan this machine resolves (ADR-122).
    pub plan: ExecutionAccessPlan,
    /// VALID · ACCESS READY · CAPACITY FIT · RUN READY (ADR-123).
    pub layers: VerdictLayers,
    /// The risk grade — advisory by default, a gate under operational.
    pub grade: RiskGrade,
    /// VALID + CAPACITY FIT.
    pub clean: bool,
    /// Declared-vs-used drift (NIKA-DRIFT-001) — advisory rows.
    pub drift: Vec<String>,
    /// What was judged with the filesystem.
    pub judged: Judged,
    /// The child workflows the file names (`invoke: { workflow: … }`) — a
    /// child-blind lane must SAY it did not read them (W3-F2).
    pub children: Vec<String>,
}

impl Verdict {
    /// The operational gate: grade below High AND access ready (when a
    /// static model exists). Always true off the operational lane — and
    /// false exactly when [`Self::lane_findings`] carries an operational
    /// row.
    #[must_use]
    pub fn profile_clean(&self, operational: bool) -> bool {
        let (grade_red, access_red) = self.operational_red();
        !operational || !(grade_red || access_red)
    }

    /// The lane-folded verdict — what the exit code and the verdict
    /// object's `clean` both read: `clean` AND no lane refusal.
    #[must_use]
    pub fn strict_clean(&self, report: &CheckReport, lanes: Lanes) -> bool {
        self.clean && self.lane_findings(report, lanes).is_empty()
    }

    /// The lane refusals on top of the report, in lane order: one row
    /// per surviving native-first hint under `--native-strict`; under
    /// `--profile operational` the grade row when it is High or worse
    /// and the access row when ACCESS READY is false — each only when
    /// its own gate failed (the footer once told a low-grade file to cap
    /// its spend because a refused pin had failed the OTHER gate).
    #[must_use]
    pub fn lane_findings(&self, report: &CheckReport, lanes: Lanes) -> Vec<LaneFinding> {
        let mut out = Vec::new();
        if lanes.native_strict {
            out.extend(
                report
                    .hints
                    .iter()
                    .filter(|h| h.kind == "native-first")
                    .map(LaneFinding::native_first),
            );
        }
        if lanes.operational {
            out.extend(self.operational_findings(report));
        }
        out
    }

    /// The operational gate's two predicates — the grade (High or worse)
    /// and the access blocker — the ONE place [`Self::profile_clean`]
    /// and the typed rows both read.
    fn operational_red(&self) -> (bool, bool) {
        (
            self.grade >= RiskGrade::High,
            self.layers.access_ready == Some(false),
        )
    }

    /// The operational gate's rows. The grade row carries the audited
    /// line's own cause clause ([`nika_display::check_render::risk_handle`])
    /// when the grade is Unbounded — the handle names WHICH grant or
    /// spend and the door that narrows it, so the row carries no fix of
    /// its own; at High the handle is empty, the lanes above carry the
    /// cause, and the fix is [`OPERATIONAL_GRADE_FIX`]. The access row
    /// carries the blocker.
    fn operational_findings(&self, report: &CheckReport) -> Vec<LaneFinding> {
        let (grade_red, access_red) = self.operational_red();
        let mut out = Vec::new();
        if grade_red {
            let grade = self.grade.as_str();
            let handle = nika_display::check_render::risk_handle(report, self.grade);
            out.push(if handle.is_empty() {
                LaneFinding::operational(
                    Lane::OperationalRisk,
                    format!("risk {grade}"),
                    Some(OPERATIONAL_GRADE_FIX),
                )
            } else {
                LaneFinding::operational(
                    Lane::OperationalRisk,
                    format!("risk {grade}{handle}"),
                    None,
                )
            });
        }
        if access_red {
            let blocker = self
                .layers
                .blockers
                .iter()
                .find(|b| b.starts_with("access:"))
                .or_else(|| self.layers.blockers.first())
                .map_or("", String::as_str);
            out.push(LaneFinding::operational(
                Lane::OperationalAccess,
                format!("access not ready — {blocker}"),
                None,
            ));
        }
        out
    }
}

/// How many `native-first` hints survive — the count `--native-strict`
/// folds into the verdict, and the only hint family that ever does.
#[must_use]
pub fn native_hints(report: &CheckReport) -> usize {
    report
        .hints
        .iter()
        .filter(|h| h.kind == "native-first")
        .count()
}

/// Judge an already-checked workflow — the fold every door shares: the
/// MODELS rung, the frozen plan under `access_pin`, the layered verdicts,
/// the grade, the drift rows.
#[must_use]
pub fn judge(
    wf: &RawWorkflow,
    report: &CheckReport,
    skills: &ResolvedSkills,
    judged: Judged,
    access_pin: Option<&str>,
) -> Verdict {
    let mut models = unresolvable_models(report, wf);
    let valid = report.is_clean() && models.findings.is_empty() && skills.findings.is_empty();
    let mut capacity = thinking_findings(wf);
    capacity.extend(capacity_findings(wf));
    models.findings.extend(capacity.iter().cloned());
    let plan = crate::access::resolve_plan(wf, report, None, access_pin);
    // The effective workflow already carries any audit_source override.
    // An unrelated static lane does not supply a missing task model.
    let modelless = nika_service_execution::access::first_modelless_task(wf);
    // The ACCESS question's premise: a file with no `infer:`/`agent:`
    // task is not waiting on a seat, it will never ask for one.
    let layers =
        verdict_layers_for(&plan, valid, &capacity, modelless).with_access_moot(!dials_a_model(wf));
    let grade = nika_check::risk_grade(report);
    let drift = nika_dap::drift::scan(wf);
    let children = child_references(wf);
    Verdict {
        clean: valid && capacity.is_empty(),
        models,
        capacity,
        plan,
        layers,
        grade,
        drift,
        judged,
        children,
    }
}

/// The child workflows a file names (`invoke: { workflow: … }`), in task
/// order — what a child-blind audit did not read (W3-F2).
#[must_use]
pub fn child_references(wf: &RawWorkflow) -> Vec<String> {
    wf.tasks
        .iter()
        .filter_map(|task| match &task.value.action {
            nika_schema::raw::RawAction::Invoke(invoke) => match &invoke.target {
                nika_schema::raw::RawInvokeTarget::Workflow(target) => Some(target.value.clone()),
                nika_schema::raw::RawInvokeTarget::Tool(_) => None,
            },
            _ => None,
        })
        .collect()
}

/// A judged workflow the facade owns — the parse, the report, the
/// skills and the verdict, from one source text.
#[non_exhaustive]
pub struct Audit {
    /// The parsed workflow, the `--model` override applied.
    pub wf: RawWorkflow,
    /// The ladder's report (composed when a reader was given).
    pub report: CheckReport,
    /// The `skills:` resolution (empty and unjudged without a base).
    pub skills: ResolvedSkills,
    /// The typed verdict.
    pub verdict: Verdict,
}

/// Parse and judge a workflow SOURCE — the ONE door. `read` is the
/// filesystem edge composition resolves through (`None`: child-blind,
/// and the verdict says `judged.composition: false`); `skills_base` is
/// the directory `skills:` paths resolve against (`None`: unjudged).
/// The report carries the semantic hash of the workflow it judged (the
/// runtime's trust gate refuses a report about other bytes).
///
/// # Errors
///
/// The parser's refusal when the source is not a workflow.
pub fn audit_source(
    source: &str,
    logical_path: &str,
    read: Option<Reader<'_>>,
    skills_base: Option<&Path>,
    opts: AuditOptions<'_>,
) -> Result<Audit, nika_schema::SchemaError> {
    let wf = nika_schema::parse(source, nika_schema::FileId::new(0), ParseMode::Strict)?;
    let wf = match opts.model_override {
        Some(model) => nika_check::with_model_override(&wf, model),
        None => wf,
    };
    let (mut report, composition) = match read {
        Some(read) => (nika_check::check_composed(&wf, logical_path, read), true),
        None => (nika_check::check(&wf), false),
    };
    report.workflow_semantic =
        nika_runtime::proof::ir::semantic_ir_hash(&wf).map(|h| h.as_hex().to_owned());
    let (skills, skills_judged) = match skills_base {
        Some(base) => (
            nika_schema::resolve_skills(&wf, &mut |p| {
                std::fs::read_to_string(base.join(p)).map_err(|e| e.to_string())
            }),
            true,
        ),
        None => (ResolvedSkills::default(), false),
    };
    let verdict = judge(
        &wf,
        &report,
        &skills,
        Judged::new(composition, skills_judged),
        opts.access_pin,
    );
    Ok(Audit {
        wf,
        report,
        skills,
        verdict,
    })
}

/// The MODELS rung's rows in the ONE machine shape — `model` · `tasks`
/// · `why` (+ `code` when the resolver named one).
#[must_use]
pub fn model_finding_rows(findings: &[ModelFinding]) -> Vec<Value> {
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
        .collect()
}

/// Presence-gated model truth on the verdict object — `models_unjudged`
/// · `model_findings` · `models_catalog_warnings`; `clean` never reads
/// the warnings.
fn extend_model_audit(object: &mut Map<String, Value>, audit: &ModelsAudit) {
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
            object.insert(key.to_owned(), Value::Array(model_finding_rows(findings)));
        }
    }
}

/// Drift + one-obvious-way rows on the machine `hints[]`. Native-first
/// already rides `CheckReport.hints`; one-obvious-way lives in
/// `nika-lints` and cannot join the report without a nika-check →
/// nika-lints cycle, so this edge is the public door (#763).
fn push_advisory_hints(hints: &mut Vec<Value>, drift: &[String], wf: &RawWorkflow) {
    for advice in drift {
        hints.push(serde_json::json!({
            "kind": "drift",
            "task": "-",
            "advice": advice,
            "code": nika_dap::drift::DRIFT_CODE,
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

/// The ONE verdict object every machine lane emits: the serialized
/// report, the advisory hint rows, the lane rows folded into
/// `findings[]` ([`LaneFinding`]), `clean` (the lane-folded verdict —
/// false exactly when the exit is 2) · `models_resolve` · the model
/// rows · the `access_plan` rows (ADR-122) · the four `verdicts`
/// (ADR-123) · `judged` · the skills · `pricing` · `risk_grade` · the
/// engine identity · the lane keys (`operational_clean` ·
/// `native_strict_clean` · each repeats `clean` on its lane) · the
/// paid-ready stamp. A door adds its own decorations AFTER (the CLI's
/// cwd budget · the oracle's next actions); none of them changes a key
/// this function wrote.
///
/// # Errors
///
/// When the report or the engine identity cannot be serialized.
pub fn audit_json(
    wf: &RawWorkflow,
    report: &CheckReport,
    skills: &ResolvedSkills,
    verdict: &Verdict,
    lanes: Lanes,
) -> Result<Map<String, Value>, String> {
    let Value::Object(mut obj) =
        serde_json::to_value(report).map_err(|e| format!("cannot serialize report: {e}"))?
    else {
        return Err("the check report is not a JSON object".to_owned());
    };
    let identity = match serde_json::to_value(nika_runtime::engine_identity()) {
        Ok(Value::Object(identity)) => identity,
        Ok(_) => return Err("engine identity is not a JSON object".to_owned()),
        Err(error) => return Err(format!("cannot serialize engine identity: {error}")),
    };
    if let Some(hints) = obj.get_mut("hints").and_then(Value::as_array_mut) {
        push_advisory_hints(hints, &verdict.drift, wf);
    }
    // The lane refusals ride `findings[]` and `clean` reads them: the
    // same rows, the same predicate the exit code reads.
    let lane_rows: Vec<Value> = verdict
        .lane_findings(report, lanes)
        .iter()
        .map(LaneFinding::row)
        .collect();
    match obj.get_mut("findings").and_then(Value::as_array_mut) {
        Some(findings) => findings.extend(lane_rows),
        None => {
            obj.insert("findings".to_owned(), Value::Array(lane_rows));
        }
    }
    let strict_clean = verdict.strict_clean(report, lanes);
    obj.insert("clean".to_owned(), Value::Bool(strict_clean));
    obj.insert(
        "models_resolve".to_owned(),
        Value::Bool(verdict.models.findings.is_empty()),
    );
    extend_model_audit(&mut obj, &verdict.models);
    let access_rows = nika_service_execution::access::lane_rows(&verdict.plan);
    if !access_rows.is_empty() {
        obj.insert("access_plan".to_owned(), Value::Array(access_rows));
    }
    obj.insert(
        "verdicts".to_owned(),
        serde_json::json!({
            "valid": verdict.layers.valid,
            "access_ready": verdict.layers.access_ready,
            "capacity_fit": verdict.layers.capacity_fit,
            "run_ready": verdict.layers.run_ready(),
            "blockers": verdict.layers.blockers,
        }),
    );
    obj.insert(
        "judged".to_owned(),
        serde_json::json!({
            "composition": verdict.judged.composition,
            "skills": verdict.judged.skills,
            "children": verdict.children,
        }),
    );
    skills.extend_check_json(&mut obj);
    obj.insert(
        "pricing".to_owned(),
        pricing_section(report, &verdict.models.findings),
    );
    obj.insert(
        "risk_grade".to_owned(),
        Value::String(verdict.grade.as_str().to_owned()),
    );
    obj.extend(identity);
    if lanes.operational {
        obj.insert("operational_clean".to_owned(), Value::Bool(strict_clean));
    }
    if lanes.native_strict {
        obj.insert("native_strict_clean".to_owned(), Value::Bool(strict_clean));
    }
    nika_check::stamp_paid_ready(&mut obj, &report.hints);
    Ok(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit(source: &str) -> Audit {
        audit_source(source, "w.nika.yaml", None, None, AuditOptions::default())
            .expect("the fixture parses")
    }

    const MIXED_MODELS: &str = "nika: mixed\ntasks:\n  explicit:\n    infer: { prompt: hi, max_tokens: 10, model: mock/echo }\n  needs_model:\n    infer: { prompt: hi, max_tokens: 10 }\n";

    /// The operations sceptic's ops5 scenario: three builtin/exec tasks, an
    /// envelope `model:` nothing dials, and a run that exited 0 with 3/3
    /// green while the card printed `run ready ○`. « No blocker is
    /// named, no flag flips it, `--access mock` changes nothing. The
    /// readiness line is a verdict the run contradicts. »
    const BUILTIN_ONLY: &str = "nika: ops5\nmodel: mock/echo\npermits:\n  fs: { read: [\"./source.txt\"], write: [\"./out/summary.md\"] }\n  exec: [\"date\"]\n  tools: [\"nika:read\", \"nika:write\"]\ntasks:\n  grab:\n    invoke:\n      tool: \"nika:read\"\n      args: { path: \"./source.txt\" }\n  measure:\n    exec: { command: [\"date\", \"-u\"] }\n  save:\n    with: { t: \"${{ tasks.grab.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/summary.md\", content: \"${{ with.t }}\" }\n";

    /// The same shape WITH something that dials, whose model is only
    /// known at run time — the access question that really is open.
    const RUN_TIME_MODEL: &str = "nika: tmpl\ninputs:\n  seat: { type: string, required: true }\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"${{ inputs.seat }}\" }\n";

    #[test]
    fn a_workflow_that_never_dials_is_run_ready_not_unjudged() {
        let audit = audit(BUILTIN_ONLY);
        assert!(
            audit.verdict.layers.access_moot,
            "no infer/agent task: the ACCESS question has no subject"
        );
        assert_eq!(audit.verdict.layers.access_ready, None);
        assert_eq!(
            audit.verdict.layers.run_ready(),
            Some(true),
            "the run exits 0; the card must not contradict it: {:?}",
            audit.verdict.layers
        );
        assert!(
            audit.verdict.layers.blockers.is_empty(),
            "{:?}",
            audit.verdict.layers.blockers
        );
    }

    #[test]
    fn a_run_time_model_stays_genuinely_unjudged() {
        let audit = audit(RUN_TIME_MODEL);
        assert!(
            !audit.verdict.layers.access_moot,
            "a task DOES dial here — the question is open, not moot"
        );
        assert_eq!(audit.verdict.layers.run_ready(), None);
    }

    fn has_modelless_blocker(audit: &Audit) -> bool {
        audit
            .verdict
            .layers
            .blockers
            .iter()
            .any(|line| line.contains("task `needs_model` names no model"))
    }

    /// One admitted static lane cannot supply another task's absent model.
    /// Both the typed verdict and its machine projection must say not ready.
    #[test]
    fn a_mixed_mock_and_modelless_workflow_is_not_access_ready() {
        for access_pin in [None, Some("mock")] {
            let audit = audit_source(
                MIXED_MODELS,
                "w.nika.yaml",
                None,
                None,
                AuditOptions::new(None, access_pin),
            )
            .expect("parses");
            assert!(
                audit.verdict.clean,
                "model-less is legal, access is separate"
            );
            assert!(audit.verdict.plan.lane("mock/echo").is_some());
            assert!(audit.verdict.plan.is_admitted());
            assert_eq!(audit.verdict.layers.access_ready, Some(false));
            assert_eq!(audit.verdict.layers.run_ready(), Some(false));
            assert!(!audit.verdict.profile_clean(true));
            assert!(has_modelless_blocker(&audit));
            let obj = audit_json(
                &audit.wf,
                &audit.report,
                &audit.skills,
                &audit.verdict,
                Lanes::new(false, true),
            )
            .expect("serializes");
            assert_eq!(obj["verdicts"]["access_ready"], false);
            assert_eq!(obj["verdicts"]["run_ready"], false);
            assert_eq!(obj["operational_clean"], false);
            // The verdict object's `clean` is the lane-folded verdict —
            // the key a consumer reads must not contradict the exit.
            assert_eq!(obj["clean"], false, "{obj:?}");
            let rows = operational_rows(&obj);
            assert!(
                rows.iter().any(|m| m.starts_with("risk unbounded — ")
                    && m.contains("--max-cost-usd")
                    && !m.contains(" — fix: ")),
                "the grade row carries the handle (the spend cause and its door), no generic fix: {rows:?}"
            );
            assert!(
                rows.iter()
                    .any(|m| m.starts_with("access not ready — access: task `needs_model`")),
                "the access row carries the blocker: {rows:?}"
            );
        }
    }

    /// The `message` of every operational row on a verdict object.
    fn operational_rows(obj: &Map<String, Value>) -> Vec<String> {
        obj["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .filter(|f| f["kind"] == "operational")
            .map(|f| f["message"].as_str().expect("message").to_owned())
            .collect()
    }

    /// A LOW-grade file whose only failed operational gate is ACCESS (a
    /// refused pin): one row, the access one — the grade row that told
    /// this file to cap its spend (0.118.7) pronounced a remedy the
    /// grade never asked for. Off the lane the object is clean and
    /// carries no row at all.
    #[test]
    fn the_operational_lane_types_only_the_gate_that_failed() {
        let audit = audit_source(
            "nika: w\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
            "w.nika.yaml",
            None,
            None,
            AuditOptions::new(None, Some("not-a-real-access-pin")),
        )
        .expect("parses");
        assert!(audit.verdict.clean);
        assert!(
            audit.verdict.grade < RiskGrade::High,
            "{:?}",
            audit.verdict.grade
        );
        assert_eq!(audit.verdict.layers.access_ready, Some(false));
        let rows = audit
            .verdict
            .lane_findings(&audit.report, Lanes::new(false, true));
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].lane, Lane::OperationalAccess);
        assert_eq!(rows[0].lane.kind(), "operational");
        assert_eq!(
            rows[0].lane.gate(),
            "ACCESS",
            "the rung that printed the refusal"
        );
        assert!(rows[0].code.is_none() && rows[0].task.is_none() && rows[0].fix.is_none());
        assert!(
            rows[0]
                .detail
                .starts_with("access not ready — access: pin `not-a-real-access-pin` refused"),
            "{}",
            rows[0].detail
        );
        assert_eq!(
            rows[0].message(),
            rows[0].detail,
            "no fix, no ` — fix:` tail"
        );
        assert!(!audit.verdict.profile_clean(true));
        assert!(
            !audit
                .verdict
                .strict_clean(&audit.report, Lanes::new(false, true))
        );
        let obj = audit_json(
            &audit.wf,
            &audit.report,
            &audit.skills,
            &audit.verdict,
            Lanes::new(false, true),
        )
        .expect("serializes");
        assert_eq!(obj["clean"], false);
        assert_eq!(obj["operational_clean"], false);
        let rows = operational_rows(&obj);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!(!rows[0].contains("cap the spend"), "{rows:?}");
        let access_row = obj["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .find(|f| f["kind"] == "operational")
            .expect("the access row");
        assert_eq!(access_row["gate"], "ACCESS", "{access_row}");
        let off = audit_json(
            &audit.wf,
            &audit.report,
            &audit.skills,
            &audit.verdict,
            Lanes::default(),
        )
        .expect("serializes");
        assert_eq!(off["clean"], true);
        assert!(
            off["findings"].as_array().is_some_and(Vec::is_empty),
            "{off:?}"
        );
    }

    /// A HIGH grade (a glob grant, every token capped) is the grade row
    /// and nothing else: the gate reads `>= High`, so High itself
    /// refuses, not only Unbounded.
    #[test]
    fn the_operational_lane_refuses_a_high_grade_on_its_own() {
        let audit = audit(
            "nika: h\nmodel: mock/echo\npermits:\n  tools: [\"nika:*\"]\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 256 }\n",
        );
        assert!(
            audit.verdict.clean,
            "high-grade glob-grant fixture must stay default-clean"
        );
        assert_eq!(audit.verdict.grade, RiskGrade::High);
        let rows = audit
            .verdict
            .lane_findings(&audit.report, Lanes::new(false, true));
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].lane, Lane::OperationalRisk);
        assert_eq!(rows[0].lane.gate(), "OPERATIONAL");
        assert_eq!(rows[0].detail, "risk high");
        assert_eq!(rows[0].fix.as_deref(), Some(OPERATIONAL_GRADE_FIX));
        assert_eq!(
            rows[0].message(),
            format!("risk high — fix: {OPERATIONAL_GRADE_FIX}")
        );
        assert!(
            audit
                .verdict
                .lane_findings(&audit.report, Lanes::new(true, false))
                .is_empty(),
            "no native-first hint, no native-strict row"
        );
    }

    /// An Unbounded grade whose only ceiling-less thing is a `**` write
    /// grant: the typed grade row carries the audited line's own handle
    /// — the grant it was graded on and the door that narrows it — and
    /// no generic fix, so the machine twin says the same cause the
    /// human footer does.
    #[test]
    fn the_unbounded_grade_row_carries_the_handle_that_names_the_grant() {
        let audit = audit(
            "nika: ops\nmodel: mock/echo\npermits:\n  fs: { read: [\"./source.txt\"], write: [\"./out/**\"] }\n  tools: [\"nika:read\", \"nika:write\"]\ntasks:\n  grab:\n    invoke:\n      tool: \"nika:read\"\n      args: { path: \"./source.txt\" }\n  save:\n    with: { t: \"${{ tasks.grab.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/summary.md\", content: \"${{ with.t }}\" }\n",
        );
        assert!(
            audit.verdict.clean,
            "unbounded write-glob fixture must stay default-clean"
        );
        assert_eq!(audit.verdict.grade, RiskGrade::Unbounded);
        let rows = audit
            .verdict
            .lane_findings(&audit.report, Lanes::new(false, true));
        assert_eq!(rows.len(), 1, "nothing dials: no access row · {rows:?}");
        assert_eq!(rows[0].lane, Lane::OperationalRisk);
        assert!(
            rows[0]
                .detail
                .starts_with("risk unbounded — no ceiling on the grant: fs.write ./out/**"),
            "{}",
            rows[0].detail
        );
        assert!(
            rows[0].detail.contains("--infer-permits"),
            "{}",
            rows[0].detail
        );
        assert!(
            rows[0].fix.is_none(),
            "the handle names the cause and the door"
        );
        assert_eq!(rows[0].message(), rows[0].detail);
        let obj = audit_json(
            &audit.wf,
            &audit.report,
            &audit.skills,
            &audit.verdict,
            Lanes::new(false, true),
        )
        .expect("serializes");
        assert_eq!(obj["clean"], false);
        let row = obj["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .find(|f| f["kind"] == "operational")
            .expect("the grade row");
        assert_eq!(row["gate"], "OPERATIONAL", "{row}");
        assert!(row.get("fix").is_none(), "{row}");
        assert!(
            row["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("risk unbounded — no ceiling on the grant")),
            "{row}"
        );
    }

    /// The existing envelope override fills the missing model before the
    /// oracle judges it; the explicit task model is still its own choice.
    #[test]
    fn a_mock_override_supplies_the_mixed_workflows_missing_model() {
        let audit = audit_source(
            MIXED_MODELS,
            "w.nika.yaml",
            None,
            None,
            AuditOptions::new(Some("mock/echo"), None),
        )
        .expect("parses");
        assert!(audit.verdict.clean);
        assert_eq!(audit.verdict.layers.access_ready, Some(true));
        assert_eq!(audit.verdict.layers.run_ready(), Some(true));
        assert!(!has_modelless_blocker(&audit));
        let model = audit
            .report
            .requirements
            .models
            .iter()
            .find(|model| model.model == "mock/echo")
            .expect("effective mock lane");
        assert!(model.tasks.iter().any(|task| task == "explicit"));
        assert!(model.tasks.iter().any(|task| task == "needs_model"));
    }

    /// A model override never repairs an invalid access pin. Without the
    /// override, the missing model is also disclosed beside that refusal.
    #[test]
    fn a_mixed_workflows_refused_pin_survives_model_overrides() {
        for model_override in [None, Some("mock/echo")] {
            let audit = audit_source(
                MIXED_MODELS,
                "w.nika.yaml",
                None,
                None,
                AuditOptions::new(model_override, Some("not-a-real-access-pin")),
            )
            .expect("parses");
            assert!(audit.verdict.plan.pin_refusal.is_some());
            assert_eq!(audit.verdict.layers.access_ready, Some(false));
            assert_eq!(audit.verdict.layers.run_ready(), Some(false));
            assert!(
                audit.verdict.layers.blockers[0].contains("pin `not-a-real-access-pin` refused")
            );
            assert_eq!(has_modelless_blocker(&audit), model_override.is_none());
        }
    }

    /// Only tasks inheriting the envelope take the override. An explicit
    /// unresolvable provider remains refused, with or without a mock pin.
    #[test]
    fn a_refused_explicit_lane_survives_mixed_workflow_overrides_and_pins() {
        let source = format!(
            "{MIXED_MODELS}  refused:\n    infer: {{ prompt: hi, max_tokens: 10, model: unavailable-provider/model }}\n"
        );
        for model_override in [None, Some("mock/echo")] {
            for access_pin in [None, Some("mock")] {
                let audit = audit_source(
                    &source,
                    "w.nika.yaml",
                    None,
                    None,
                    AuditOptions::new(model_override, access_pin),
                )
                .expect("parses");
                assert!(audit.verdict.plan.lane("mock/echo").is_some());
                assert!(matches!(
                    audit.verdict.plan.lanes.get("unavailable-provider/model"),
                    Some(nika_providers::LaneVerdict::Refused(_))
                ));
                assert_eq!(
                    audit.verdict.plan.pin_refusal.is_some(),
                    access_pin.is_some()
                );
                assert_eq!(audit.verdict.layers.access_ready, Some(false));
                assert_eq!(audit.verdict.layers.run_ready(), Some(false));
                assert_eq!(has_modelless_blocker(&audit), model_override.is_none());
                assert!(audit.verdict.layers.blockers.iter().any(|line| {
                    line.contains("unavailable-provider/model → no path on this machine")
                }));
            }
        }
    }

    /// The facade's verdict object carries the keys both doors agree on,
    /// and says what it could not judge.
    #[test]
    fn the_verdict_object_names_what_was_judged() {
        let audit = audit(
            "nika: w\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
        );
        assert!(audit.verdict.clean, "{:?}", audit.report.findings);
        let obj = audit_json(
            &audit.wf,
            &audit.report,
            &audit.skills,
            &audit.verdict,
            Lanes::default(),
        )
        .expect("serializes");
        assert_eq!(obj["clean"], Value::Bool(true));
        assert_eq!(obj["models_resolve"], Value::Bool(true));
        assert_eq!(obj["judged"]["composition"], Value::Bool(false));
        assert_eq!(obj["judged"]["skills"], Value::Bool(false));
        assert!(obj.contains_key("verdicts") && obj.contains_key("risk_grade"));
        assert!(
            !obj.contains_key("native_strict_clean") && !obj.contains_key("operational_clean"),
            "a lane key rides only on its lane: {obj:?}"
        );
        assert!(
            obj["findings"].as_array().is_some_and(Vec::is_empty),
            "a clean object carries no lane row: {obj:?}"
        );
    }

    /// A reader makes the composition judged; the CLI's posture.
    #[test]
    fn a_reader_marks_the_composition_judged() {
        let mut read = |_: &str| Err::<String, String>("no such child".to_owned());
        let audit = audit_source(
            "nika: w\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n",
            "w.nika.yaml",
            Some(&mut read),
            None,
            AuditOptions::default(),
        )
        .expect("parses");
        assert!(audit.verdict.judged.composition);
        assert!(!audit.verdict.judged.skills);
    }

    /// The MODELS rung rides the facade: a provider this binary cannot
    /// drive is a finding on every door, the templated default judged.
    #[test]
    fn an_unresolvable_model_is_a_finding_on_the_facade() {
        let audit = audit(
            "nika: w\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10, model: \"azure/gpt-4o\" }\n",
        );
        assert!(!audit.verdict.clean);
        assert_eq!(audit.verdict.models.findings.len(), 1);
        assert!(!audit.verdict.layers.valid);
        let obj = audit_json(
            &audit.wf,
            &audit.report,
            &audit.skills,
            &audit.verdict,
            Lanes::default(),
        )
        .expect("serializes");
        assert_eq!(obj["models_resolve"], Value::Bool(false));
        assert!(
            obj["model_findings"]
                .as_array()
                .is_some_and(|r| r.len() == 1)
        );
    }

    /// The strict lanes fold on top of `clean`: a native-first hint reds
    /// only the native-strict lane, and the key rides only there.
    #[test]
    fn the_native_strict_lane_folds_the_hint() {
        let audit = audit(
            "nika: t\npermits: { exec: [\"curl\"], net: { http: [\"acme.test\"] } }\ntasks:\n  grab:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n",
        );
        let hints = native_hints(&audit.report);
        assert!(
            hints > 0,
            "the exec of an interpreter is a native-first hint"
        );
        assert!(audit.verdict.clean, "advisory: clean");
        assert!(
            !audit
                .verdict
                .strict_clean(&audit.report, Lanes::new(true, false))
        );
        assert!(
            audit
                .verdict
                .strict_clean(&audit.report, Lanes::new(false, false))
        );
        let obj = audit_json(
            &audit.wf,
            &audit.report,
            &audit.skills,
            &audit.verdict,
            Lanes::new(true, false),
        )
        .expect("serializes");
        assert_eq!(obj["native_strict_clean"], Value::Bool(false));
        // `clean` is the SAME verdict — the key a consumer reads first.
        assert_eq!(obj["clean"], Value::Bool(false), "{obj:?}");
        // The refusal is a typed row: the hint's own resolvable code,
        // its task, the gate, the remedy — never only a lane key.
        let rows: Vec<&Value> = obj["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .filter(|f| f["kind"] == "native_strict")
            .collect();
        assert_eq!(rows.len(), hints, "one row per surviving hint: {obj:?}");
        let row = rows[0];
        assert_eq!(row["code"], "native-first/001");
        assert_eq!(row["task"], "grab");
        assert_eq!(row["gate"], "NATIVE-STRICT");
        assert_eq!(row["severity"], "error");
        let message = row["message"].as_str().expect("message");
        assert!(
            message.starts_with("`curl`") && message.contains(" — fix: replace each `exec:`"),
            "the detail is the hint's advice without its code prefix, then the fix: {message}"
        );
        assert_eq!(row["fix"], NATIVE_STRICT_FIX);
        assert!(
            row.get("docs_url").is_none(),
            "no conjured docs page: {row}"
        );
        // Off the lane: the same file is clean and carries no row.
        let off = audit_json(
            &audit.wf,
            &audit.report,
            &audit.skills,
            &audit.verdict,
            Lanes::default(),
        )
        .expect("serializes");
        assert_eq!(off["clean"], Value::Bool(true));
        assert!(
            off["findings"].as_array().is_some_and(Vec::is_empty),
            "{off:?}"
        );
        // An exec-only file has no static model: ACCESS READY is None,
        // the grade is bounded — the operational lane carries no row.
        assert!(
            audit
                .verdict
                .lane_findings(&audit.report, Lanes::new(false, true))
                .is_empty()
        );
    }
}
