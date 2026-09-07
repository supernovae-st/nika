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
/// by every door. `clean` means VALID + CAPACITY FIT, what it always
/// folded (ADR-123); the lanes ride [`Verdict::strict_clean`].
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
    /// static model exists). Always true off the operational lane.
    #[must_use]
    pub fn profile_clean(&self, operational: bool) -> bool {
        !operational || (self.grade < RiskGrade::High && self.layers.access_ready != Some(false))
    }

    /// The lane-folded verdict: `clean` + the operational gate + the
    /// native-strict gate (no surviving native-first hint).
    #[must_use]
    pub fn strict_clean(&self, report: &CheckReport, lanes: Lanes) -> bool {
        self.clean
            && self.profile_clean(lanes.operational)
            && (!lanes.native_strict || native_hints(report) == 0)
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
/// report, the advisory hint rows, `clean` · `models_resolve` · the
/// model rows · the `access_plan` rows (ADR-122) · the four `verdicts`
/// (ADR-123) · `judged` · the skills · `pricing` · `risk_grade` · the
/// engine identity · the lane keys (`operational_clean` ·
/// `native_strict_clean`) · the paid-ready stamp. A door adds its own
/// decorations AFTER (the CLI's cwd budget · the oracle's next actions);
/// none of them changes a key this function wrote.
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
    obj.insert("clean".to_owned(), Value::Bool(verdict.clean));
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
    let strict_clean = verdict.strict_clean(report, lanes);
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
        }
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
    }
}
