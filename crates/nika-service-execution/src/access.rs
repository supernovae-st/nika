// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE composition of « what this run needs » with « what this
//! machine offers » (One Door · wave 1): the checked requirements joined
//! with each task's verb, the `--model` override applied, the `--access`
//! pin carried, this machine's probe rows collected once — resolved into
//! the frozen [`ExecutionAccessPlan`] every door EXECUTES: `nika run`,
//! the answered gate leg, `nika serve`'s resident jobs, an ARM beat.
//! Descended from `nika-cli-host` in wave 1b so the resident door and
//! the CLI door read one resolver (the host crate re-exports these).
//! Before wave 1 the same question was answered five times on one run
//! path, and the answers could disagree.

use nika_check::CheckReport;
use nika_providers::probe::ProviderProbe;
use nika_providers::{ExecutionAccessPlan, ModelNeed, VerbNeeds, resolve_execution_plan_declared};
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_types::access::{AccessRejection, AccessRequirement};

/// This machine's access-probe rows: the provider rows (key presence ·
/// endpoint overrides · the locals) PLUS the harness rows when the
/// feature is on — ONE door, so the run's gate, `check`, `explain` and
/// the resident's jobs can never judge different rows.
#[must_use]
pub fn access_probes_env() -> Vec<ProviderProbe> {
    nika_providers::probe::collect_access_probes_env(nika_runtime::compose::config_from_env())
}

/// Effective provider endpoints and key presence only. This does not query
/// harness authentication, local ports, model listings or inference.
#[must_use]
pub fn provider_probes_env() -> Vec<ProviderProbe> {
    let registry =
        nika_providers::ProviderRegistry::without_http(nika_runtime::compose::config_from_env());
    nika_providers::probe::collect_provider_probes(&registry)
}

/// The verbs that read each static model — the checked requirements
/// (task `model:` ?? envelope) joined with the action kind of every
/// task that resolves to the model. The eligibility facts a harness
/// candidate is judged against (an ACP-only seat drives `agent:`,
/// never a one-shot `infer:`).
#[must_use]
pub fn model_needs(wf: &RawWorkflow, report: &CheckReport) -> Vec<ModelNeed> {
    report
        .requirements
        .models
        .iter()
        .map(|req| {
            let (infer, agent) = req.tasks.iter().fold((false, false), |(infer, agent), id| {
                let action = wf
                    .tasks
                    .iter()
                    .find(|task| task.value.id.value == *id)
                    .map(|task| &task.value.action);
                match action {
                    Some(RawAction::Infer(_)) => (true, agent),
                    Some(RawAction::Agent(_)) => (infer, true),
                    _ => (infer, agent),
                }
            });
            ModelNeed::new(req.model.clone(), infer, agent)
        })
        .collect()
}

/// Resolve the frozen plan for one execution attempt over THIS
/// machine's probe rows: the effective models (`--model` applied; a per-task
/// `model:` keeps winning), their verbs, and the explicit `--access` pin.
/// When none requires model access, no provider or harness probe runs.
/// Otherwise the probes may check authentication through an installed CLI.
#[must_use]
pub fn resolve_plan(
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
    pin: Option<&str>,
) -> ExecutionAccessPlan {
    resolve_plan_using(wf, report, model_override, pin, access_probes_env)
}

fn resolve_plan_using(
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
    pin: Option<&str>,
    collect: impl FnOnce() -> Vec<ProviderProbe>,
) -> ExecutionAccessPlan {
    let needs = effective_needs(wf, report, model_override);
    let verbs = verb_needs(wf);
    let requirement = declared_requirement(wf);
    let selects = requirement
        .as_ref()
        .is_some_and(AccessRequirement::selects_path);
    let probes = if needs.is_empty() && !verbs.infer && !verbs.agent && pin.is_none() && !selects {
        Vec::new()
    } else {
        collect()
    };
    resolve_execution_plan_declared(&needs, &probes, pin, verbs, requirement.as_ref())
}

/// The workflow's authored access requirement (`run.access` · `run.reasoning`),
/// when it declares one — resolved by the SAME plan as an `--access` pin, so an
/// ordinary run honors the file with no flag.
#[must_use]
pub fn declared_requirement(wf: &RawWorkflow) -> Option<AccessRequirement> {
    wf.run
        .as_ref()
        .and_then(|run| run.value.access_requirement())
}

/// [`resolve_plan`] over INJECTED probe rows — the pure half (tests
/// drive this; the process environment is never read here).
#[must_use]
pub fn resolve_plan_over(
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
    pin: Option<&str>,
    probes: &[ProviderProbe],
) -> ExecutionAccessPlan {
    let needs = effective_needs(wf, report, model_override);
    let requirement = declared_requirement(wf);
    resolve_execution_plan_declared(&needs, probes, pin, verb_needs(wf), requirement.as_ref())
}

fn effective_needs(
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
) -> Vec<ModelNeed> {
    match model_override {
        Some(model) => {
            let swapped = nika_check::with_model_override(wf, model);
            let report = nika_check::check(&swapped);
            model_needs(&swapped, &report)
        }
        None => model_needs(wf, report),
    }
}

/// The verbs the WORKFLOW carries, whatever its models say (W3-F1: a
/// model-less `infer:` task yields no model need; the pin judge and the
/// readiness layer must still see the infer).
#[must_use]
pub fn verb_needs(wf: &RawWorkflow) -> VerbNeeds {
    let mut infer = false;
    let mut agent = false;
    for task in &wf.tasks {
        match &task.value.action {
            RawAction::Infer(_) => infer = true,
            RawAction::Agent(_) => agent = true,
            _ => {}
        }
    }
    VerbNeeds::new(infer, agent)
}

pub use nika_runtime::first_modelless_task;

/// The ONE machine shape of an access lane (One Door · wave 2 · the W1
/// gauntlet met three): `check --json`'s `access_plan[]`, `run --dry-run
/// --json`'s `access.plans[]` and the trace's boot manifest `access_plan`
/// all carry exactly these rows — `model` · `provider` · `resolved` ·
/// `access` (the id that serves) · `chosen` (its class) · `billing` ·
/// `trust` (declared · discovered · observed · ADR-134 · `null` on a
/// refused lane: no path serves, the rejected candidates carry their own
/// witnesses) · `pinned` · `rejected[]` with `access` · `dimension` · `layer` ·
/// `witness`. A refused lane carries `resolved: false` and its witnesses.
#[must_use]
pub fn lane_rows(plan: &ExecutionAccessPlan) -> Vec<serde_json::Value> {
    plan.lanes
        .iter()
        .map(|(model, verdict)| match verdict {
            nika_providers::LaneVerdict::Admitted(lane) => serde_json::json!({
                "model": model,
                "provider": lane.plan.provider,
                "resolved": true,
                "access": lane.plan.access,
                "chosen": lane.plan.chosen.as_str(),
                "billing": lane.plan.billing.as_str(),
                "trust": lane.plan.trust.as_str(),
                "pinned": lane.plan.pinned,
                "rejected": rejection_rows(&lane.plan.rejected),
                "outranked": rejection_rows(&lane.plan.outranked),
                "candidates": lane.candidates,
            }),
            nika_providers::LaneVerdict::Refused(refusal) => serde_json::json!({
                "model": model,
                "provider": refusal.provider,
                "resolved": false,
                "trust": serde_json::Value::Null,
                "rejected": rejection_rows(&refusal.rejected),
            }),
            // `#[non_exhaustive]` · a verdict this build does not know is
            // never rendered as admitted (fail closed on the machine face).
            _ => serde_json::json!({
                "model": model,
                "resolved": false,
                "trust": serde_json::Value::Null,
                "rejected": [],
                "note": "lane verdict unknown to this build",
            }),
        })
        .collect()
}

fn rejection_rows(rejected: &[AccessRejection]) -> Vec<serde_json::Value> {
    rejected
        .iter()
        .map(|r| {
            serde_json::json!({
                "access": r.access,
                "dimension": r.dimension.as_str(),
                "layer": r.layer.as_str(),
                "witness": r.witness,
            })
        })
        .collect()
}

/// Boot evidence projected from the exact plan attached to this runtime.
/// Root and child attempts stamp their own lanes and the inherited explicit pin.
#[must_use]
pub fn boot_access_fields(
    plan: &ExecutionAccessPlan,
) -> Vec<(&'static str, nika_types::resource::Value)> {
    use nika_types::resource::Value as FieldValue;
    let mut fields = Vec::new();
    if let Some(pin) = &plan.pin {
        fields.push(("access_pin", FieldValue::String(pin.clone())));
    }
    let rows = lane_rows(plan);
    if !rows.is_empty() {
        fields.push((
            "access_plan",
            FieldValue::String(serde_json::Value::Array(rows).to_string()),
        ));
    }
    fields
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use nika_providers::probe::{ExecutionLocus, ProviderReadiness};
    use nika_types::access::AccessClass;

    use super::*;

    fn parse(src: &str) -> RawWorkflow {
        nika_schema::parse(
            src,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    fn api_probe(id: &str, key_present: bool) -> ProviderProbe {
        ProviderProbe::new(
            id,
            true,
            key_present,
            format!("{}_API_KEY", id.to_uppercase()),
            false,
            ProviderReadiness::new(
                true,
                key_present,
                None,
                None,
                true,
                ExecutionLocus::Cloud,
                AccessClass::Api,
            ),
            "https://api.example.com",
        )
    }

    /// The needs join the requirement's tasks with their verbs: one
    /// model read by an infer AND an agent task carries both flags.
    #[test]
    fn needs_join_each_model_with_the_verbs_that_read_it() {
        let wf = parse(
            "nika: t\nmodel: mistral/mistral-small-latest\ntasks:\n  a:\n    infer: { prompt: hi }\n  b:\n    agent: { prompt: go, tools: [] }\n  c:\n    infer: { prompt: hi, model: \"mock/echo\" }\n",
        );
        let report = nika_check::check(&wf);
        let mut needs = model_needs(&wf, &report);
        needs.sort_by(|a, b| a.model.cmp(&b.model));
        assert_eq!(needs.len(), 2);
        assert_eq!(needs[0].model, "mistral/mistral-small-latest");
        assert!(needs[0].infer && needs[0].agent, "{:?}", needs[0]);
        assert_eq!(needs[1].model, "mock/echo");
        assert!(needs[1].infer && !needs[1].agent, "{:?}", needs[1]);
    }

    /// `--model` swaps the ENVELOPE model before the plan is resolved —
    /// the plan speaks about the run, never about the file (the shipped
    /// door announced the file's model under `--model mock/echo`).
    #[test]
    fn the_override_is_what_the_plan_resolves() {
        let wf = parse(
            "nika: t\nmodel: mistral/mistral-small-latest\ntasks:\n  a:\n    infer: { prompt: hi }\n",
        );
        let report = nika_check::check(&wf);
        let probes = [api_probe("mistral", false)];
        let file_plan = resolve_plan_over(&wf, &report, None, None, &probes);
        assert!(
            !file_plan.is_admitted(),
            "no mistral key → the file's lane refuses"
        );
        let run_plan = resolve_plan_over(&wf, &report, Some("mock/echo"), None, &probes);
        assert!(run_plan.is_admitted(), "the run rides mock");
        assert!(run_plan.lane("mock/echo").is_some());
        assert!(run_plan.lane("mistral/mistral-small-latest").is_none());
    }

    /// The one shape: an admitted lane and a refused lane render the
    /// same keys on every machine surface (`resolved` tells them apart).
    #[test]
    fn the_lane_rows_carry_one_shape_for_admitted_and_refused() {
        let wf = parse(
            "nika: t\nmodel: mistral/mistral-small-latest\ntasks:\n  a:\n    infer: { prompt: hi }\n  b:\n    infer: { prompt: hi, model: \"mock/echo\" }\n",
        );
        let report = nika_check::check(&wf);
        let probes = [api_probe("mistral", false)];
        let plan = resolve_plan_over(&wf, &report, None, None, &probes);
        let rows = lane_rows(&plan);
        assert_eq!(rows.len(), 2, "{rows:?}");
        let refused = rows
            .iter()
            .find(|r| r["model"] == "mistral/mistral-small-latest")
            .expect("row");
        assert_eq!(refused["resolved"], false);
        assert_eq!(refused["provider"], "mistral");
        assert!(
            refused["trust"].is_null(),
            "ADR-134 §3 · every row says trust · a refused lane says null: {refused}"
        );
        assert!(
            refused["rejected"]
                .as_array()
                .is_some_and(|r| !r.is_empty()),
            "{refused}"
        );
        let admitted = rows
            .iter()
            .find(|r| r["model"] == "mock/echo")
            .expect("row");
        assert_eq!(admitted["resolved"], true);
        assert_eq!(admitted["chosen"], "mock");
        assert_eq!(admitted["access"], "mock");
        assert_eq!(admitted["pinned"], false);
        assert_eq!(
            admitted["trust"], "observed",
            "the mock is the engine's own"
        );
    }

    #[test]
    fn a_model_free_workflow_never_collects_access_probes() {
        for (envelope, model_override) in [
            ("", None),
            ("model: mistral/mistral-small-latest\n", None),
            ("", Some("mistral/mistral-small-latest")),
        ] {
            let wf = parse(&format!(
                "nika: copy\n{envelope}tasks:\n  read:\n    invoke: {{ tool: nika:read, args: {{ path: ./input.txt }} }}\n"
            ));
            let report = nika_check::check(&wf);
            let plan = resolve_plan_using(&wf, &report, model_override, None, || {
                panic!("a file operation must not inspect model credentials or launch a CLI")
            });
            assert!(plan.is_admitted());
            assert!(plan.lanes.is_empty() && plan.pin.is_none() && plan.seat.is_none());
        }
    }

    #[test]
    fn model_verbs_collect_once_and_keep_the_admitted_model() {
        for action in ["infer: { prompt: hi }", "agent: { prompt: hi, tools: [] }"] {
            let wf = parse(&format!(
                "nika: model\nmodel: mistral/mistral-small-latest\ntasks:\n  ask:\n    {action}\n"
            ));
            let report = nika_check::check(&wf);
            let calls = std::cell::Cell::new(0);
            let plan = resolve_plan_using(&wf, &report, None, None, || {
                calls.set(calls.get() + 1);
                vec![api_probe("mistral", true)]
            });
            assert_eq!(calls.get(), 1);
            assert!(plan.is_admitted());
            assert!(plan.lane("mistral/mistral-small-latest").is_some());
        }
    }

    #[test]
    fn a_model_less_model_verb_still_collects_access_probes() {
        for action in ["infer: { prompt: hi }", "agent: { prompt: hi, tools: [] }"] {
            let wf = parse(&format!("nika: missing\ntasks:\n  ask:\n    {action}\n"));
            let report = nika_check::check(&wf);
            let calls = std::cell::Cell::new(0);
            let plan = resolve_plan_using(&wf, &report, None, None, || {
                calls.set(calls.get() + 1);
                Vec::new()
            });
            assert_eq!(calls.get(), 1, "the verb still requires model access");
            assert!(plan.lanes.is_empty(), "no static model was invented");
        }
    }

    #[test]
    fn an_explicit_access_pin_still_collects_for_a_model_free_workflow() {
        let wf = parse(
            "nika: copy\ntasks:\n  read:\n    invoke: { tool: nika:read, args: { path: ./input.txt } }\n",
        );
        let report = nika_check::check(&wf);
        let calls = std::cell::Cell::new(0);
        let plan = resolve_plan_using(&wf, &report, None, Some("api"), || {
            calls.set(calls.get() + 1);
            vec![api_probe("mistral", true)]
        });
        assert_eq!(calls.get(), 1, "an explicit pin keeps its access judgment");
        assert_eq!(plan.pin.as_deref(), Some("api"));
        assert!(plan.lanes.is_empty());
    }

    /// A signed-in codex seat: the ACP speaker on PATH, its login answered.
    fn codex_probe() -> ProviderProbe {
        ProviderProbe::new(
            "codex",
            false,
            true,
            "",
            false,
            ProviderReadiness::new(
                true,
                true,
                None,
                None,
                false,
                ExecutionLocus::Loopback,
                AccessClass::Harness,
            ),
            "",
        )
        .with_serves(vec!["openai".to_owned()])
    }

    const DECLARED: &str = "nika: t\nmodel: openai/gpt-6-astra\nrun:\n  access: { via: codex, \
         protocol: acp, fallback: none }\n  reasoning: { effort: high }\ntasks:\n  a:\n    \
         agent: { prompt: go }\n";

    /// NIK-13 · the ordinary door honors the FILE: no flag, a ready
    /// `openai` key beside a signed-in codex seat, and the plan seats codex
    /// under the authored requirement — the key is never substituted.
    #[test]
    fn a_declared_route_is_honored_with_no_flag() {
        let wf = parse(DECLARED);
        let report = nika_check::check(&wf);
        let probes = [api_probe("openai", true), codex_probe()];
        let plan = resolve_plan_over(&wf, &report, None, None, &probes);
        assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
        assert_eq!(plan.pin, None, "no operator flag was given");
        assert_eq!(plan.seat.as_deref(), Some("codex"));
        let lane = plan.lane("openai/gpt-6-astra").expect("admitted");
        assert_eq!(lane.plan.access, "codex");
        assert_eq!(lane.plan.chosen, AccessClass::Harness);
        let requirement = declared_requirement(&wf).expect("declared");
        assert_eq!(plan.requirement.as_ref(), Some(&requirement));
        assert_eq!(requirement.effort.as_deref(), Some("high"));
    }

    /// A file without `run.access`/`run.reasoning` resolves exactly as
    /// before: the same plan the undeclared resolver returns.
    #[test]
    fn an_undeclared_file_keeps_its_plan() {
        let wf =
            parse("nika: t\nmodel: openai/gpt-6-astra\ntasks:\n  a:\n    agent: { prompt: go }\n");
        let report = nika_check::check(&wf);
        let probes = [api_probe("openai", true), codex_probe()];
        let plan = resolve_plan_over(&wf, &report, None, None, &probes);
        let old = nika_providers::resolve_execution_plan_for(
            &model_needs(&wf, &report),
            &probes,
            None,
            verb_needs(&wf),
        );
        assert_eq!(plan, old);
        assert_eq!(
            plan.lane("openai/gpt-6-astra").map(|l| l.plan.chosen),
            Some(AccessClass::Api),
            "unpinned, the ready key outranks the unproven seat as before"
        );
    }

    /// A declared route is judged over this machine's rows even when no
    /// static model lane exists (a templated model), like a flag.
    #[test]
    fn a_declared_route_collects_probes_without_a_static_lane() {
        let wf = parse(
            "nika: t\ninputs:\n  m: { type: string, default: openai/gpt-6-astra }\nrun:\n  \
             access: { via: codex, protocol: acp }\ntasks:\n  a:\n    agent: { prompt: go, \
             model: \"${{ inputs.m }}\" }\n",
        );
        let report = nika_check::check(&wf);
        let calls = std::cell::Cell::new(0);
        let plan = resolve_plan_using(&wf, &report, None, None, || {
            calls.set(calls.get() + 1);
            vec![codex_probe()]
        });
        assert_eq!(calls.get(), 1, "the file's route is judged on real rows");
        assert_eq!(plan.seat.as_deref(), Some("codex"));
        assert_eq!(plan.seat_for("${{ inputs.m }}"), Some("codex"));
    }

    /// A flag that contradicts the file refuses before task 1.
    #[test]
    fn a_contradicting_flag_refuses_the_plan() {
        let wf = parse(DECLARED);
        let report = nika_check::check(&wf);
        let probes = [api_probe("openai", true), codex_probe()];
        let plan = resolve_plan_over(&wf, &report, None, Some("openai"), &probes);
        assert!(!plan.is_admitted());
        assert!(
            matches!(
                &plan.pin_refusal,
                Some(nika_providers::PinRefusal::PinUnsatisfied { message })
                    if message.contains("run.access.via: codex")
            ),
            "{:?}",
            plan.pin_refusal
        );
    }

    static RUN_DOOR_PROBES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    fn run_door_probe() -> Vec<ProviderProbe> {
        RUN_DOOR_PROBES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        vec![api_probe("openai", true), codex_probe()]
    }

    /// The run door itself (`resolve_access_plan`, what `nika run` and
    /// every other door call with no flag): a model-less agent task yields
    /// no static lane, yet the declared route is judged over this
    /// machine's rows — never over none, which would refuse a signed-in
    /// seat as absent.
    #[test]
    fn the_run_door_judges_a_declared_route_on_this_machine() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = "nika: root\nrun:\n  access: { via: codex, protocol: acp, fallback: none }\n  \
                      reasoning: { effort: high }\ntasks:\n  act:\n    agent: { prompt: go, \
                      tools: [] }\n";
        std::fs::write(directory.path().join("root.nika"), source).expect("write");
        let project = nika_fs::OwnedDir::open(directory.path()).expect("project");
        let service = nika_execution::ExecutionService::default();
        let admitted = service
            .admit_with_model_override(&project, std::path::Path::new("root.nika"), None)
            .expect("admitted");
        let session = service.begin(admitted);
        let mut driver =
            crate::ServiceExecutionDriver::new(session.context(), std::path::PathBuf::new())
                .expect("driver");
        driver.probe_source = run_door_probe;
        let plan = driver.resolve_access_plan(None, None);
        assert_eq!(RUN_DOOR_PROBES.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(plan.is_admitted(), "{:?}", plan.pin_refusal);
        assert_eq!(plan.pin, None, "no operator flag was given");
        assert_eq!(plan.seat.as_deref(), Some("codex"));
        assert_eq!(
            plan.requirement.as_ref().and_then(|r| r.effort.as_deref()),
            Some("high")
        );
    }
}
