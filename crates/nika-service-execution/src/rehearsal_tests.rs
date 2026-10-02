// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A rehearsal's access plan never reads this machine. Each test installs its
//! own counting probe source, so the proof is the absence of the call itself
//! (the source never ran and the probe cell stayed empty), not a zero cost
//! ceiling. A requested pin, a model lane and a model-less `infer:` or
//! `agent:` task are refused before any probe, even when rows collected
//! earlier could serve the lane; a model-free candidate gets the empty plan,
//! and so does one whose envelope names a model no task uses.
//!
//! The model-less fixtures rely on admission accepting them (the model-less
//! refusal belongs to launch, not to Check); if admission refuses one, its test
//! is harness-invalid for that run, never a semantic RED.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use nika_fs::OwnedDir;
use nika_providers::probe::ProviderProbe;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>; // box-dyn-ok(test-harness): cfg(test) fixtures use heterogeneous setup failures

const MODEL_FREE: &str = "nika: root\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  one:\n    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }\n";

const MODELED: &str =
    "nika: root\nmodel: openai/gpt-4.1\ntasks:\n  say:\n    infer: { prompt: hi }\n";

/// No model anywhere: the infer yields no lane, yet would run on the default model.
const MODEL_LESS_INFER: &str = "nika: root\ntasks:\n  say:\n    infer: { prompt: hi }\n";

/// No model anywhere: the agent yields no lane either.
const MODEL_LESS_AGENT: &str = "nika: root\ntasks:\n  act:\n    agent: { prompt: go, tools: [] }\n";

/// The envelope names a model, but no task uses one: no lane, no model verb.
const ENVELOPE_MODEL_ONLY: &str = "nika: root\nmodel: openai/gpt-4.1\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  one:\n    invoke: { tool: \"nika:jq\", args: { input: 1, expression: \".\" } }\n";

fn admitted(root: &str) -> TestResult<ServiceExecutionDriver> {
    let directory = tempfile::tempdir()?;
    std::fs::write(directory.path().join("root.nika"), root)?;
    let project = OwnedDir::open(directory.path())?;
    let service = nika_execution::ExecutionService::default();
    let admitted = service.admit_with_model_override(&project, Path::new("root.nika"), None)?;
    let session = service.begin(admitted);
    ServiceExecutionDriver::new(session.context(), PathBuf::new())
        .ok_or_else(|| std::io::Error::other("admitted context lost its root").into())
}

/// One harness row that could serve an `openai` lane.
fn codex_probe() -> ProviderProbe {
    use nika_providers::probe::{ExecutionLocus, ProviderReadiness};
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
            ExecutionLocus::Cloud,
            nika_types::access::AccessClass::Harness,
        ),
        "",
    )
    .with_serves(vec!["openai".to_owned()])
}

static MODEL_LANE_PROBES: AtomicUsize = AtomicUsize::new(0);
fn model_lane_probe() -> Vec<ProviderProbe> {
    MODEL_LANE_PROBES.fetch_add(1, Ordering::SeqCst);
    vec![codex_probe()]
}

static PIN_PROBES: AtomicUsize = AtomicUsize::new(0);
fn pin_probe() -> Vec<ProviderProbe> {
    PIN_PROBES.fetch_add(1, Ordering::SeqCst);
    vec![codex_probe()]
}

static MODEL_FREE_PROBES: AtomicUsize = AtomicUsize::new(0);
fn model_free_probe() -> Vec<ProviderProbe> {
    MODEL_FREE_PROBES.fetch_add(1, Ordering::SeqCst);
    vec![codex_probe()]
}

static COLLECTED_PROBES: AtomicUsize = AtomicUsize::new(0);
fn collected_probe() -> Vec<ProviderProbe> {
    COLLECTED_PROBES.fetch_add(1, Ordering::SeqCst);
    vec![codex_probe()]
}

static INFER_PROBES: AtomicUsize = AtomicUsize::new(0);
fn infer_probe() -> Vec<ProviderProbe> {
    INFER_PROBES.fetch_add(1, Ordering::SeqCst);
    vec![codex_probe()]
}

static AGENT_PROBES: AtomicUsize = AtomicUsize::new(0);
fn agent_probe() -> Vec<ProviderProbe> {
    AGENT_PROBES.fetch_add(1, Ordering::SeqCst);
    vec![codex_probe()]
}

static ENVELOPE_PROBES: AtomicUsize = AtomicUsize::new(0);
fn envelope_probe() -> Vec<ProviderProbe> {
    ENVELOPE_PROBES.fetch_add(1, Ordering::SeqCst);
    vec![codex_probe()]
}

static DOOR_LANE_PROBES: AtomicUsize = AtomicUsize::new(0);
fn door_lane_probe() -> Vec<ProviderProbe> {
    DOOR_LANE_PROBES.fetch_add(1, Ordering::SeqCst);
    vec![codex_probe()]
}

static DOOR_VERB_PROBES: AtomicUsize = AtomicUsize::new(0);
fn door_verb_probe() -> Vec<ProviderProbe> {
    DOOR_VERB_PROBES.fetch_add(1, Ordering::SeqCst);
    vec![codex_probe()]
}

#[test]
fn a_model_lane_is_refused_before_any_probe() -> TestResult<()> {
    let mut driver = admitted(MODELED)?;
    driver.probe_source = model_lane_probe;
    let plan = driver.rehearsal_plan(None);
    assert_eq!(plan, Err(RehearsalPlanRefusal::ModelLane), "{plan:?}");
    assert_eq!(
        MODEL_LANE_PROBES.load(Ordering::SeqCst),
        0,
        "the probe source never ran"
    );
    assert!(driver.access_probes.get().is_none(), "no row was collected");
    Ok(())
}

#[test]
fn a_requested_pin_is_refused_before_any_probe() -> TestResult<()> {
    let mut driver = admitted(MODEL_FREE)?;
    driver.probe_source = pin_probe;
    let plan = driver.rehearsal_plan(Some("codex"));
    assert_eq!(plan, Err(RehearsalPlanRefusal::Pin), "{plan:?}");
    assert_eq!(
        PIN_PROBES.load(Ordering::SeqCst),
        0,
        "the probe source never ran"
    );
    assert!(driver.access_probes.get().is_none(), "no row was collected");
    Ok(())
}

#[test]
fn rows_collected_earlier_never_serve_a_rehearsal_lane() -> TestResult<()> {
    let mut driver = admitted(MODELED)?;
    driver.probe_source = collected_probe;
    driver.access_probes = Arc::new(OnceLock::from(vec![codex_probe()]));
    let plan = driver.rehearsal_plan(None);
    assert_eq!(plan, Err(RehearsalPlanRefusal::ModelLane), "{plan:?}");
    assert_eq!(
        COLLECTED_PROBES.load(Ordering::SeqCst),
        0,
        "the probe source never ran"
    );
    Ok(())
}

#[test]
fn a_model_free_candidate_gets_the_empty_plan_without_any_probe() -> TestResult<()> {
    let mut driver = admitted(MODEL_FREE)?;
    driver.probe_source = model_free_probe;
    let plan = driver.rehearsal_plan(None);
    assert_eq!(
        plan,
        Ok(driver.resolve_access_plan_over(None, None, &[])),
        "{plan:?}"
    );
    let empty = plan.as_ref().is_ok_and(|resolved| {
        resolved.lanes.is_empty() && resolved.pin.is_none() && resolved.seat.is_none()
    });
    assert!(empty, "{plan:?}");
    assert_eq!(
        MODEL_FREE_PROBES.load(Ordering::SeqCst),
        0,
        "the probe source never ran"
    );
    assert!(driver.access_probes.get().is_none(), "no row was collected");
    Ok(())
}

#[test]
fn a_model_less_infer_task_is_refused_before_any_probe() -> TestResult<()> {
    let mut driver = admitted(MODEL_LESS_INFER)?;
    driver.probe_source = infer_probe;
    let plan = driver.rehearsal_plan(None);
    assert_eq!(plan, Err(RehearsalPlanRefusal::ModelVerb), "{plan:?}");
    assert_eq!(
        INFER_PROBES.load(Ordering::SeqCst),
        0,
        "the probe source never ran"
    );
    assert!(driver.access_probes.get().is_none(), "no row was collected");
    Ok(())
}

#[test]
fn a_model_less_agent_task_is_refused_before_any_probe() -> TestResult<()> {
    let mut driver = admitted(MODEL_LESS_AGENT)?;
    driver.probe_source = agent_probe;
    let plan = driver.rehearsal_plan(None);
    assert_eq!(plan, Err(RehearsalPlanRefusal::ModelVerb), "{plan:?}");
    assert_eq!(
        AGENT_PROBES.load(Ordering::SeqCst),
        0,
        "the probe source never ran"
    );
    assert!(driver.access_probes.get().is_none(), "no row was collected");
    Ok(())
}

#[test]
fn an_envelope_model_no_task_uses_still_gets_the_empty_plan_without_any_probe() -> TestResult<()> {
    // The guard against over-refusal: native candidates carry an envelope model even when
    // no task infers, and their plan has no lane.
    let mut driver = admitted(ENVELOPE_MODEL_ONLY)?;
    driver.probe_source = envelope_probe;
    let plan = driver.rehearsal_plan(None);
    assert_eq!(
        plan,
        Ok(driver.resolve_access_plan_over(None, None, &[])),
        "{plan:?}"
    );
    assert!(
        plan.as_ref()
            .is_ok_and(|resolved| resolved.lanes.is_empty()),
        "{plan:?}"
    );
    assert_eq!(
        ENVELOPE_PROBES.load(Ordering::SeqCst),
        0,
        "the probe source never ran"
    );
    assert!(driver.access_probes.get().is_none(), "no row was collected");
    Ok(())
}

// ─── The run's own door, characterized (unchanged code, passes before and after) ───
//
// These two read today's `resolve_access_plan`, not the rehearsal: they are the real
// difference between the two doors, recorded on code this change does not touch.

#[test]
fn the_run_door_reads_this_machine_for_a_model_lane() -> TestResult<()> {
    let mut driver = admitted(MODELED)?;
    driver.probe_source = door_lane_probe;
    let plan = driver.resolve_access_plan(None, None);
    assert!(!plan.lanes.is_empty(), "{plan:?}");
    assert_eq!(
        DOOR_LANE_PROBES.load(Ordering::SeqCst),
        1,
        "the run door ran the probe source"
    );
    assert!(
        driver.access_probes.get().is_some(),
        "the rows were collected"
    );
    Ok(())
}

#[test]
fn the_run_door_gives_a_model_less_infer_the_empty_plan() -> TestResult<()> {
    // A model-less infer yields no lane, so a lane check alone would admit it: the rehearsal
    // refuses the verb itself.
    let mut driver = admitted(MODEL_LESS_INFER)?;
    driver.probe_source = door_verb_probe;
    let plan = driver.resolve_access_plan(None, None);
    assert!(plan.lanes.is_empty() && plan.pin.is_none(), "{plan:?}");
    assert_eq!(DOOR_VERB_PROBES.load(Ordering::SeqCst), 0);
    Ok(())
}
