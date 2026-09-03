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
    let admitted = service.admit(&project, Path::new("root.nika.yaml"))?;
    let session = service.begin(admitted);
    ServiceExecutionDriver::new(session.context(), PathBuf::new())
        .ok_or_else(|| std::io::Error::other("admitted context lost its root").into())
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
    assert!(
        result.events().iter().all(|e| e.kind() != "task_started"),
        "nothing ran"
    );
    Ok(())
}
