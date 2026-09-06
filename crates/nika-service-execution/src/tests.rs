// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_event::EventKind;
use nika_fs::OwnedDir;
use nika_types::resource::{KeyValue, Value as FieldValue};

/// `Permits::allows_exec`, reached through one locally named alias.
///
/// The predicate is nika-schema's and is unchanged; only the spelling at the
/// call sites below is local, so the shell axis reads as a noun here
/// (`GRANTS_SHELL(&permits)`) rather than as a bare accessor.
const GRANTS_SHELL: fn(&Permits) -> bool = Permits::allows_exec;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>; // box-dyn-ok(test-harness): cfg(test) fixtures use heterogeneous setup and execution failures

fn admitted_driver(files: &[(&str, &str)]) -> TestResult<ServiceExecutionDriver> {
    admitted_driver_with_override(files, None)
}

fn admitted_driver_with_override(
    files: &[(&str, &str)],
    model_override: Option<&str>,
) -> TestResult<ServiceExecutionDriver> {
    let directory = tempfile::tempdir()?;
    for (path, source) in files {
        let path = directory.path().join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, source)?;
    }
    let project = OwnedDir::open(directory.path())?;
    let service = nika_execution::ExecutionService::default();
    let admitted =
        service.admit_with_model_override(&project, Path::new("root.nika.yaml"), model_override)?;
    let session = service.begin(admitted);
    ServiceExecutionDriver::new(session.context(), PathBuf::new())
        .ok_or_else(|| std::io::Error::other("admitted context lost its root").into())
}

#[derive(Default)]
struct RecordedEvents(Vec<Event>);

impl EventSink for RecordedEvents {
    fn emit(&mut self, event: Event) {
        self.0.push(event);
    }
}

fn cloud_after_marker_driver() -> TestResult<ServiceExecutionDriver> {
    // If admission regresses, this marker fails before the cloud task can
    // dispatch, so the negative fixture never needs a provider connection.
    let root = r#"nika: root
model: openai/gpt-4.1
permits: { tools: ["nika:jq"] }
tasks:
  marker:
    invoke:
      tool: nika:jq
      args: { input: 1, expression: 'error("marker reached")' }
  cloud:
    after: { marker: success }
    infer: { prompt: hi }
"#;
    let mut driver = admitted_driver(&[("root.nika.yaml", root)])?;
    driver.access_probes.clear();
    Ok(driver)
}

#[tokio::test]
async fn planless_authorized_runtime_refuses_before_the_marker_task() -> TestResult<()> {
    let mut driver = cloud_after_marker_driver()?;
    for surface in [DriverSurface::Service, DriverSurface::Local] {
        driver.surface = surface;
        let runtime = driver.compose("openai/gpt-4.1")?;
        assert!(runtime.access_plan().is_none());
        let mut events = RecordedEvents::default();
        let mut stamper = RunSeams::of(None).stamper();
        let error = runtime
            .run(stamper.as_mut(), &mut events)
            .await
            .expect_err("an authorized runtime must carry a frozen plan");
        assert!(matches!(&error, RuntimeError::AccessNoPath { .. }));
        assert_eq!(error.spec_code(), "NIKA-1800");
        assert!(error.wire_message().contains("frozen access plan"));
        assert!(events.0.is_empty(), "not even the prologue may run");
    }
    Ok(())
}

#[tokio::test]
async fn execute_derives_an_omitted_plan_from_its_census_and_effective_model() -> TestResult<()> {
    let driver = cloud_after_marker_driver()?;
    let result = driver.execute(ServiceExecutionOptions::new()).await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Refused);
    assert_eq!(result.error().expect("access refusal").0, "NIKA-1800");
    assert!(result.events().is_empty(), "the marker must never start");
    assert!(result.settlement().is_none(), "no task result was produced");

    // The same captured census admits an effective mock override. The
    // marker then fails in the runtime, proving this is not a blanket refusal.
    let result = driver
        .execute(ServiceExecutionOptions::new().with_model_override(Some("mock/echo".to_owned())))
        .await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Failed);
    assert!(result.settlement().is_some());
    assert!(
        result
            .events()
            .iter()
            .any(|event| event.kind() == "task_failed")
    );
    assert!(
        result
            .error()
            .expect("marker failure")
            .1
            .contains("marker reached")
    );
    Ok(())
}

#[tokio::test]
async fn resident_and_local_runs_rejudge_the_effective_model_before_any_event() -> TestResult<()> {
    let root = "nika: root\nmodel: openai/gpt-5.2\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  first:\n    invoke: { tool: \"nika:jq\", args: { input: 7, expression: \".\" } }\n  say:\n    after: { first: success }\n    infer: { prompt: hi, max_tokens: 32 }\n";
    let mut driver = admitted_driver_with_override(&[("root.nika.yaml", root)], Some("mock/echo"))?;
    driver.access_probes.clear();
    let result = driver.execute(ServiceExecutionOptions::new()).await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Refused);
    assert!(
        result.events().is_empty(),
        "no runtime prologue or task effects"
    );
    assert!(result.settlement().is_none());
    let result = driver
        .execute(ServiceExecutionOptions::new().with_model_override(Some("mock/echo".to_owned())))
        .await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Succeeded);

    // CLI and ARM use this same local-surface authorized runner.
    let mut local = driver;
    local.surface = DriverSurface::Local;
    let runtime = local
        .compose("mock/echo")?
        .with_access_plan(local.resolve_access_plan(None, None))?;
    let mut events = RecordedEvents::default();
    let mut stamper = RunSeams::of(None).stamper();
    assert!(matches!(
        runtime.run(stamper.as_mut(), &mut events).await,
        Err(RuntimeError::DirtyReport)
    ));
    assert!(events.0.is_empty());
    let runtime = runtime
        .with_model_override(Some("mock/echo".to_owned()))
        .with_access_plan(local.resolve_access_plan(Some("mock/echo"), None))?;
    assert_eq!(
        runtime
            .run(stamper.as_mut(), &mut events)
            .await?
            .settlement
            .state,
        RunState::Succeeded
    );
    Ok(())
}

#[tokio::test]
async fn an_effective_override_supplies_a_missing_envelope_model() -> TestResult<()> {
    let root = "nika: root\ntasks:\n  explicit:\n    infer: { model: mock/echo, prompt: first }\n  missing:\n    after: { explicit: success }\n    infer: { prompt: second }\n";
    let mut driver = admitted_driver(&[("root.nika.yaml", root)])?;
    driver.access_probes.clear();
    let plan = driver.resolve_access_plan(None, None);
    assert!(plan.is_admitted());
    assert!(plan.lane("mock/echo").is_some());
    let result = driver.execute(ServiceExecutionOptions::new()).await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Refused);
    assert_eq!(result.error().expect("missing model").0, "NIKA-1800");
    assert!(
        result.events().is_empty(),
        "an unrelated lane cannot waive the missing model"
    );
    assert!(result.settlement().is_none());

    let result = driver
        .execute(ServiceExecutionOptions::new().with_model_override(Some("mock/echo".to_owned())))
        .await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Succeeded);
    assert_eq!(
        result.settlement().expect("executed").state,
        RunState::Succeeded
    );
    assert!(
        driver.workflow().model.is_none(),
        "the authored world stays unchanged"
    );
    Ok(())
}

#[tokio::test]
async fn an_effective_override_never_replaces_an_explicit_task_model() -> TestResult<()> {
    let root = r#"nika: root
permits: { tools: ["nika:jq"] }
tasks:
  marker:
    invoke:
      tool: nika:jq
      args: { input: 1, expression: 'error("marker reached")' }
  explicit:
    after: { marker: success }
    infer: { model: openai/gpt-4.1, prompt: first }
  missing:
    after: { explicit: success }
    infer: { prompt: second }
"#;
    let mut driver = admitted_driver(&[("root.nika.yaml", root)])?;
    driver.access_probes.clear();
    let plan = driver.resolve_access_plan(Some("mock/echo"), None);
    assert!(plan.lane("mock/echo").is_some());
    assert!(
        plan.first_refused()
            .is_some_and(|(model, _)| model == "openai/gpt-4.1")
    );
    let result = driver
        .execute(ServiceExecutionOptions::new().with_model_override(Some("mock/echo".to_owned())))
        .await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Refused);
    assert_eq!(result.error().expect("explicit cloud model").0, "NIKA-1800");
    assert!(result.events().is_empty());
    assert!(result.settlement().is_none());
    Ok(())
}

#[tokio::test]
async fn public_settlement_redacts_the_message_and_preserves_all_other_runtime_facts()
-> TestResult<()> {
    let root = r#"nika: root
permits: { tools: ["nika:jq"] }
tasks:
  boom:
    invoke:
      tool: nika:jq
      args: { input: 1, expression: 'error("cannot open /private/operator-only/secret")' }
"#;
    let driver = admitted_driver(&[("root.nika.yaml", root)])?;
    let runtime = driver
        .compose("")?
        .with_access_plan(driver.resolve_access_plan(None, None))?;
    let mut events = RecordedEvents::default();
    let mut stamper = RunSeams::of(None).stamper();
    let outcome = runtime.run(stamper.as_mut(), &mut events).await;
    let raw = &outcome.as_ref().expect("task failure settles").settlement;
    assert!(
        raw.error
            .as_ref()
            .expect("failed task")
            .message
            .contains("/private/operator-only/secret")
    );
    let before = raw.clone();
    let result = ServiceExecutionResult::from_runtime(&outcome, Vec::new());
    assert_eq!(result.status(), ServiceExecutionStatus::Failed);
    assert!(
        !result
            .error()
            .expect("public diagnosis")
            .1
            .contains("/private/")
    );
    let mut expected = before.clone();
    let error = expected.error.as_mut().expect("failed task");
    error.message = redact_service_message(&error.message);
    assert_eq!(
        result.settlement(),
        Some(&expected),
        "only diagnostic text changes"
    );
    assert_eq!(*raw, before, "local outcome remains forensic truth");
    assert!(
        events.0.iter().any(|event| event
            .str_field("error_message")
            .is_some_and(|message| message.contains("/private/operator-only/secret"))),
        "the local terminal frame retains the original diagnosis"
    );

    let result = driver.execute(ServiceExecutionOptions::new()).await?;
    assert!(
        !result
            .settlement()
            .expect("settlement")
            .error
            .as_ref()
            .expect("failed task")
            .message
            .contains("/private/")
    );
    Ok(())
}

#[test]
fn public_message_redaction_caps_unicode_on_a_character_boundary() {
    let raw = format!(
        "failure /private/secret C:\\private\\secret {}",
        "€".repeat(100)
    );
    let public = redact_service_message(&raw);
    assert!(!public.contains("private"));
    assert!(public.len() <= 240);
    assert!(public.starts_with("failure €"));
}

#[derive(Clone, Default)]
struct RecordedChildTrace(
    Arc<std::sync::Mutex<Vec<Event>>>,
    Arc<std::sync::atomic::AtomicUsize>,
);

impl EventSink for RecordedChildTrace {
    fn emit(&mut self, event: Event) {
        self.0.lock().expect("event lock").push(event);
    }
}

impl ChildTrace for RecordedChildTrace {
    fn sink(&mut self) -> &mut dyn EventSink {
        self
    }
    fn metadata(&self) -> ChildTraceMetadata {
        ChildTraceMetadata::default()
    }
}

impl ChildTraceFactory for RecordedChildTrace {
    fn create(&self) -> Box<dyn ChildTrace> {
        self.1.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Box::new(self.clone())
    }
}

fn child_call() -> ChildCall {
    ChildCall {
        target: "./child.nika.yaml".to_owned(),
        args: BTreeMap::new(),
        depth: 1,
        remaining_budget_usd: None,
        deadline: None,
        parent_permits: None,
    }
}

fn codex_probe() -> nika_providers::probe::ProviderProbe {
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
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

#[test]
fn root_and_child_plans_read_the_same_driver_probe_snapshot() -> TestResult<()> {
    let root = "nika: root\nmodel: openai/gpt-4.1\ntasks:\n  say:\n    infer: { prompt: hi }\n  call:\n    invoke: { workflow: ./child.nika.yaml }\n";
    let child =
        "nika: child\nmodel: openai/gpt-4.1\ntasks:\n  say:\n    infer: { prompt: child }\n";
    let mut driver = admitted_driver(&[("root.nika.yaml", root), ("child.nika.yaml", child)])?;
    // Replace the captured facts, not the process environment: the production
    // resolver must read this field rather than probe again. No seat is run.
    driver.access_probes = vec![codex_probe()];
    let root_plan = driver.resolve_access_plan(None, None);
    assert_eq!(root_plan.seat.as_deref(), Some("codex"));
    let runtime = driver
        .compose("openai/gpt-4.1")?
        .with_access_plan(root_plan.clone())?;
    driver.access_probes.clear();
    assert!(driver.resolve_access_plan(None, None).seat.is_none());
    let child_driver = &runtime.child_driver;
    let (_, _, workflow, report) = child_driver
        .load_child(&child_call())
        .map_err(|error| std::io::Error::other(error.message))?;
    let child_plan = child_driver.child_access_plan(&workflow, &report);
    assert_eq!(
        access::lane_rows(&child_plan),
        access::lane_rows(&root_plan)
    );
    assert_eq!(child_plan.seat.as_deref(), Some("codex"));
    Ok(())
}

#[test]
fn a_child_resolves_its_own_model_under_the_parents_explicit_pin() -> TestResult<()> {
    let root = "nika: root\nmodel: openai/gpt-4.1\ntasks:\n  say:\n    infer: { prompt: hi }\n  call:\n    invoke: { workflow: ./child.nika.yaml }\n";
    let child =
        "nika: child\nmodel: openai/gpt-4.1-mini\ntasks:\n  say:\n    infer: { prompt: child }\n";
    let driver = admitted_driver(&[("root.nika.yaml", root), ("child.nika.yaml", child)])?;
    let probes = vec![codex_probe()];
    let plan = access::resolve_plan_over(
        driver.workflow(),
        driver.report(),
        None,
        Some("codex"),
        &probes,
    );
    assert!(plan.is_admitted(), "{plan:?}");
    assert!(plan.lane("openai/gpt-4.1-mini").is_none());
    let runtime = driver
        .compose("openai/gpt-4.1")?
        .with_access_probes(probes)
        .with_access_plan(plan)?;
    let child_driver = &runtime.child_driver;
    let (_, _, workflow, report) = child_driver
        .load_child(&child_call())
        .map_err(|error| std::io::Error::other(error.message))?;
    let child_plan = child_driver.child_access_plan(&workflow, &report);
    assert!(child_plan.is_admitted(), "{child_plan:?}");
    assert_eq!(child_plan.pin.as_deref(), Some("codex"));
    assert_eq!(child_plan.seat.as_deref(), Some("codex"));
    assert!(child_plan.lane("openai/gpt-4.1").is_none());
    let rows = access::lane_rows(&child_plan);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["model"], "openai/gpt-4.1-mini");
    assert_eq!(rows[0]["chosen"], "harness");
    assert_eq!(rows[0]["access"], "codex");
    assert_eq!(rows[0]["pinned"], true);
    Ok(())
}

#[tokio::test]
async fn an_unavailable_child_pin_refuses_before_starting_its_trace_or_tasks() -> TestResult<()> {
    let root = "nika: root\ntasks:\n  call:\n    invoke: { workflow: ./child.nika.yaml }\n";
    let child = "nika: child\nmodel: mock/echo\ntasks:\n  say:\n    infer: { prompt: child }\n";
    let trace = RecordedChildTrace::default();
    let driver = admitted_driver(&[("root.nika.yaml", root), ("child.nika.yaml", child)])?
        .with_child_trace_factory(Arc::new(trace.clone()));
    let plan = driver.resolve_access_plan_over(None, Some("codex"), &[]);
    let runtime = driver
        .compose("")?
        .with_access_pin(Some("codex".to_owned()))
        .with_access_probes(Vec::new())
        .with_access_plan(plan)?;
    let result = runtime.child_driver.run_child(child_call()).await;
    assert!(
        result.is_err(),
        "the child must not silently take its ready mock lane"
    );
    assert!(trace.0.lock().expect("trace lock").is_empty());
    assert_eq!(
        trace.1.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "even the effectful child-trace factory remains uncalled"
    );
    Ok(())
}

#[tokio::test]
async fn a_captured_child_runs_and_stamps_its_own_frozen_plan() -> TestResult<()> {
    let root = "nika: root\nmodel: openai/gpt-4.1\ntasks:\n  say:\n    infer: { prompt: parent }\n  call:\n    invoke: { workflow: ./child.nika.yaml }\n";
    let child = "nika: child\nmodel: mock/echo\ntasks:\n  say:\n    infer: { prompt: child }\n";
    let trace = RecordedChildTrace::default();
    let driver = admitted_driver(&[("root.nika.yaml", root), ("child.nika.yaml", child)])?
        .with_child_trace_factory(Arc::new(trace.clone()));
    let plan = access::resolve_plan_over(
        driver.workflow(),
        driver.report(),
        Some("mock/echo"),
        Some("mock"),
        &[],
    );
    let runtime = driver
        .compose("mock/echo")?
        .with_model_override(Some("mock/echo".to_owned()))
        .with_access_probes(Vec::new())
        .with_access_plan(plan)?;
    let mut stamper = RunSeams::of(None).stamper();
    let mut root_events = RecordedEvents::default();
    let result = runtime.run(stamper.as_mut(), &mut root_events).await?;
    assert_eq!(result.settlement.state, RunState::Succeeded);
    let child_events = trace.0.lock().expect("trace lock");
    let boot = child_events
        .iter()
        .find(|event| event.str_field("access_plan").is_some())
        .expect("child boot carries the attached plan");
    assert_eq!(boot.str_field("access_pin"), Some("mock"));
    let rows: serde_json::Value =
        serde_json::from_str(boot.str_field("access_plan").expect("plan"))?;
    assert_eq!(rows[0]["model"], "mock/echo");
    assert_eq!(rows[0]["access"], "mock");
    assert_eq!(rows[0]["pinned"], true);
    assert!(
        child_events
            .iter()
            .any(|event| event.kind == EventKind::WorkflowCompleted)
    );
    Ok(())
}

#[test]
fn service_event_projection_drops_every_runtime_field() {
    let execution = ExecutionId::from_bytes([7; 16]);
    let mut sink = ServiceEventSink::new(Some(execution));
    let event = Event::new(
        EventId::nil(),
        Timestamp::from_unix_ms(10),
        EventKind::TaskCompleted,
    )
    .with_field(KeyValue::new(
        "output",
        FieldValue::string("secret-shaped-provider-payload"),
    ));
    sink.emit(event);

    let projected = sink.into_events();
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].kind(), "task_completed");
    assert_eq!(projected[0].execution(), Some(execution));
    assert!(!format!("{projected:?}").contains("secret-shaped"));
    assert!(!format!("{projected:?}").contains("output"));
}

#[test]
fn absent_parent_caps_every_child_at_zero() {
    let effective = effective_permits(None, None);
    assert_eq!(effective, Permits::new());
    assert!(!GRANTS_SHELL(&effective));
    assert!(!effective.allows_tool("nika:fetch"));

    let mut child = Permits::new();
    child.exec = Some(nika_schema::types::ExecPermit::Any);
    assert!(!GRANTS_SHELL(&effective_permits(Some(&child), None)));

    let mut parent = Permits::new();
    parent.exec = Some(nika_schema::types::ExecPermit::Any);
    assert!(GRANTS_SHELL(&effective_permits(None, Some(&parent))));
    assert!(GRANTS_SHELL(&effective_permits(
        Some(&child),
        Some(&parent)
    )));
}

#[tokio::test]
async fn service_driver_runs_a_child_from_the_owned_snapshot() -> TestResult<()> {
    let root = "nika: root\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  call:\n    invoke: { workflow: \"./child.nika.yaml\" }\noutputs:\n  value: ${{ tasks.call.output.value }}\n";
    let child = "nika: child\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 7, expression: \".\" }\noutputs:\n  value: ${{ tasks.value.output }}\n";
    let driver = admitted_driver(&[("root.nika.yaml", root), ("child.nika.yaml", child)])?;
    let execution_id = driver.execution_id();
    let result = driver.execute(ServiceExecutionOptions::new()).await?;
    let (status, events) = result.into_parts();

    assert_eq!(status, ServiceExecutionStatus::Succeeded);
    assert!(!events.is_empty());
    assert!(
        events
            .iter()
            .all(|event| event.execution() == Some(execution_id))
    );
    Ok(())
}

#[tokio::test]
async fn failed_root_run_projects_a_nika_code_without_paths() -> TestResult<()> {
    let root = "nika: root\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  boom:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \"error(\\\"gauntlet\\\")\" }\n";
    let driver = admitted_driver(&[("root.nika.yaml", root)])?;
    let result = driver.execute(ServiceExecutionOptions::new()).await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Failed);
    let (code, message) = result.error().expect("failed run diagnosis");
    assert!(code.starts_with("NIKA-"), "{code} {message}");
    assert!(!message.contains('/'), "{message}");
    Ok(())
}

#[tokio::test]
async fn independently_parsed_workflow_and_report_cannot_replace_the_admitted_pair()
-> TestResult<()> {
    let admitted = "nika: admitted\npermits: { tools: [\"nika:jq\"] }\ntasks:\n  value:\n    invoke: { tool: \"nika:jq\", args: { input: 7, expression: \".\" } }\n";
    let independent = "nika: independent\ntasks:\n  broken:\n    after: { missing: success }\n    invoke: { tool: \"nika:jq\", args: { input: 9, expression: \".\" } }\n";
    let independent_workflow = nika_schema::parse(independent, FileId::new(0), ParseMode::Strict)?;
    let independent_report = nika_check::check(&independent_workflow);
    assert!(!independent_report.is_clean());

    let driver = admitted_driver(&[("root.nika.yaml", admitted)])?;
    assert_eq!(
        driver
            .workflow()
            .workflow
            .as_ref()
            .map(|name| name.value.as_str()),
        Some("admitted")
    );
    assert!(driver.report().is_clean());
    let result = driver.execute(ServiceExecutionOptions::new()).await?;

    assert_eq!(result.status(), ServiceExecutionStatus::Succeeded);
    Ok(())
}

#[tokio::test]
async fn service_result_exposes_declared_outputs_without_debug_or_event_leakage() -> TestResult<()>
{
    const SECRET: &str = "sk-live-service-output-must-stay-redacted";
    let root = format!(
        "nika: output-redaction\npermits: {{ tools: [\"nika:jq\"] }}\ntasks:\n  value:\n    invoke: {{ tool: \"nika:jq\", args: {{ input: \"{SECRET}\", expression: \".\" }} }}\noutputs:\n  value: ${{{{ tasks.value.output }}}}\n"
    );
    let driver = admitted_driver(&[("root.nika.yaml", &root)])?;
    let result = driver.execute(ServiceExecutionOptions::new()).await?;
    let debug = format!("{result:?}");
    let accessors = format!("{:?} {:?}", result.status(), result.events());
    assert_eq!(result.outputs()["value"], serde_json::json!(SECRET));
    let (status, events) = result.into_parts();
    let parts = format!("{status:?} {events:?}");

    assert_eq!(status, ServiceExecutionStatus::Succeeded);
    assert!(!debug.contains(SECRET));
    assert!(!accessors.contains(SECRET));
    assert!(!parts.contains(SECRET));
    Ok(())
}

#[tokio::test]
async fn service_result_never_exposes_pause_material() -> TestResult<()> {
    const SECRET_QUESTION: &str = "pause-with-sk-live-question-material";
    let root = format!(
        "nika: pause-redaction\npermits: {{ tools: [\"nika:prompt\"] }}\ntasks:\n  ask:\n    invoke: {{ tool: \"nika:prompt\", args: {{ mode: \"input\", message: \"{SECRET_QUESTION}\" }} }}\n"
    );
    let driver = admitted_driver(&[("root.nika.yaml", &root)])?;
    let result = driver.execute(ServiceExecutionOptions::new()).await?;
    let debug = format!("{result:?}");
    let accessors = format!("{:?} {:?}", result.status(), result.events());
    let (status, events) = result.into_parts();
    let parts = format!("{status:?} {events:?}");

    assert_eq!(status, ServiceExecutionStatus::Paused);
    assert!(!debug.contains(SECRET_QUESTION));
    assert!(!accessors.contains(SECRET_QUESTION));
    assert!(!parts.contains(SECRET_QUESTION));
    Ok(())
}

/// One Door · wave 1b: the driver executes the plan the door hands in —
/// an admitted mock lane runs; a lane with no ready path refuses BEFORE
/// any task (`NIKA-1800` on the refused status).
#[tokio::test]
async fn the_options_plan_is_executed_as_resolved() -> TestResult<()> {
    let root = "nika: root\nmodel: mock/echo\ntasks:\n  say:\n    infer: { prompt: hi }\n";
    let driver = admitted_driver(&[("root.nika.yaml", root)])?;
    // Resolved over the real probe rows: mock is compiled in, always admitted.
    let plan = driver.resolve_access_plan(None, None);
    assert!(plan.is_admitted(), "{plan:?}");
    let result = driver
        .execute(ServiceExecutionOptions::new().with_access_plan(plan))
        .await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Succeeded);

    // The same file under a plan whose lane is refused: nothing runs.
    let refused = nika_providers::ExecutionAccessPlan::new(
        BTreeMap::from([(
            "mock/echo".to_owned(),
            nika_providers::LaneVerdict::Refused(
                nika_providers::resolve_access::AccessRefusal::new("mock/echo", "mock", Vec::new()),
            ),
        )]),
        None,
        None,
        None,
    );
    let result = driver
        .execute(ServiceExecutionOptions::new().with_access_plan(refused))
        .await?;
    assert_eq!(result.status(), ServiceExecutionStatus::Refused);
    let (code, message) = result.error().expect("a refusal names its code");
    assert_eq!(code, "NIKA-1800", "{message}");
    assert!(result.events().is_empty(), "nothing ran");
    assert!(result.settlement().is_none());
    Ok(())
}
