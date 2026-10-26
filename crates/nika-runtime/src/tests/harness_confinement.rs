// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIK-266 · a seated harness agent is held to the workflow's permits through the WHOLE
//! runtime: the session is rooted at the admitted project (never the host process's own
//! directory), it requires the confinement its permits project, and the session mode plus
//! every bridged permission are journaled as `permit_checked` frames on the run's stream.
//! Disposable directories only, no network, no provider.

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use nika_kernel::ai::harness::{
    DynAgentBackend, HarnessConfinement, HarnessError, HarnessEvent, HarnessEventStream,
    HarnessOutcome, HarnessRequest, PermissionDecision, PermissionReply,
};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

use super::*;

/// Records each request, then plays the client's beats for it: the confinement it settled,
/// one edit ask on `location`, the terminal outcome.
struct BridgeTape {
    location: String,
    requests: Mutex<Vec<HarnessRequest>>,
    verdicts: Arc<Mutex<Vec<PermissionDecision>>>,
}

impl DynAgentBackend for BridgeTape {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>,
    > {
        self.requests.lock().unwrap().push(request);
        let verdicts = Arc::clone(&self.verdicts);
        let events = vec![
            Ok(HarnessEvent::SessionConfined {
                mode: Some("read-only".to_owned()),
                selected: true,
                unasked: "Without asking, the harness reads any file".to_owned(),
            }),
            Ok(HarnessEvent::PermissionAsked {
                question: "Edit files".to_owned(),
                reply: PermissionReply::new(Box::new(move |d| verdicts.lock().unwrap().push(d))),
                kind: Some("edit".to_owned()),
                locations: vec![self.location.clone()],
                command: Vec::new(),
                url: None,
            }),
            Ok(HarnessEvent::Completed {
                outcome: Box::new(HarnessOutcome::new("written")),
            }),
        ];
        Box::pin(
            async move { Ok(Box::pin(futures_util::stream::iter(events)) as HarnessEventStream) },
        )
    }
}

type Seated = Runtime<
    MockShell,
    MockToolExecutor,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
>;

fn runtime(root: &std::path::Path) -> Seated {
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(nika_providers::ProviderRegistry::without_http(
                nika_providers::ProvidersConfig::new(),
            )),
            "mock/echo",
        ),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default().with_sandbox_root(root.to_path_buf()),
    )
}

struct World(std::path::PathBuf);

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `(plane, gate, decision)` of every `permit_checked` frame the task `draw` journaled.
fn witnesses(events: &[Event]) -> Vec<(String, String, String)> {
    let field = |e: &Event, key: &str| {
        e.fields
            .iter()
            .find(|f| f.key == key)
            .and_then(|f| match &f.value {
                FieldValue::String(text) => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default()
    };
    events
        .iter()
        .filter(|e| e.kind == EventKind::PermitChecked && field(e, "task") == "draw")
        .map(|e| (field(e, "plane"), field(e, "gate"), field(e, "decision")))
        .collect()
}

async fn run(runtime: &Seated, permits: &str) -> (RunOutcome, Vec<Event>) {
    let yaml = format!(
        "nika: confined\nmodel: mock/echo\npermits: {permits}\ntasks:\n  draw:\n    agent:\n      \
         prompt: \"write the report\"\n"
    );
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{report:?}");
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles");
    (outcome, sink.into_events())
}

#[tokio::test]
async fn a_seated_agent_is_rooted_confined_and_every_decision_is_journaled() {
    let base =
        std::env::temp_dir().join(format!("nika-harness-confinement-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("out")).unwrap();
    let world = World(std::fs::canonicalize(&base).unwrap());
    assert_ne!(
        std::env::current_dir().unwrap(),
        world.0,
        "the process runs elsewhere"
    );
    let backend = Arc::new(BridgeTape {
        location: "out/report.md".to_owned(),
        requests: Mutex::new(Vec::new()),
        verdicts: Arc::new(Mutex::new(Vec::new())),
    });
    let runtime = runtime(&world.0)
        .with_harness_backend(backend.clone(), "tape".into())
        .expect("the project holds the seat");
    let (outcome, events) = run(&runtime, "{ fs: { write: [\"./out/**\"] } }").await;
    assert!(outcome.ok, "{events:?}");
    let requests = backend.requests.lock().unwrap();
    assert_eq!(
        requests[0].cwd, world.0,
        "the session root is the admitted project, never the host process's cwd"
    );
    assert_eq!(
        requests[0].confinement,
        Some(HarnessConfinement::WorkspaceWrite)
    );
    assert_eq!(
        *backend.verdicts.lock().unwrap(),
        [PermissionDecision::AllowOnce]
    );
    assert_eq!(
        witnesses(&events),
        [
            (
                "agent".to_owned(),
                "session mode · read-only".to_owned(),
                "confine".to_owned()
            ),
            (
                "fs".to_owned(),
                "edit · out/report.md".to_owned(),
                "allow".to_owned()
            ),
        ],
        "both decisions ride the run's journal"
    );
}

/// Without a write grant the same edit is outside: the run pauses for the operator, nothing
/// is allowed, and the escalation is journaled beside the read-only confinement.
#[tokio::test]
async fn an_edit_without_a_write_grant_pauses_and_is_journaled() {
    let base = std::env::temp_dir().join(format!(
        "nika-harness-confinement-ro-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let world = World(std::fs::canonicalize(&base).unwrap());
    let backend = Arc::new(BridgeTape {
        location: "out/report.md".to_owned(),
        requests: Mutex::new(Vec::new()),
        verdicts: Arc::new(Mutex::new(Vec::new())),
    });
    let runtime = runtime(&world.0)
        .with_harness_backend(backend.clone(), "tape".into())
        .expect("the project holds the seat");
    let (outcome, events) = run(&runtime, "{}").await;
    assert!(!outcome.ok, "an out-of-grants ask never completes the task");
    assert_eq!(
        backend.requests.lock().unwrap()[0].confinement,
        Some(HarnessConfinement::ReadOnly)
    );
    assert!(
        backend.verdicts.lock().unwrap().is_empty(),
        "the ask was never answered allow"
    );
    let journaled = witnesses(&events);
    assert_eq!(journaled[0].2, "confine", "{journaled:?}");
    assert_eq!(
        (journaled[1].0.as_str(), journaled[1].2.as_str()),
        ("fs", "escalate"),
        "{journaled:?}"
    );
}

/// `nika run` pauses a run on the harness gate. The paused task never settles, yet its
/// decisions, the session's confinement and the ask that escalated, are journaled before the
/// pause frame; they used to vanish with the unsettled task, so the paused trace named no
/// decision at all (the NIK-266 live check).
#[tokio::test]
async fn a_paused_gate_journals_the_confinement_and_the_escalation_before_the_pause() {
    let base = std::env::temp_dir().join(format!(
        "nika-harness-confinement-paused-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let world = World(std::fs::canonicalize(&base).unwrap());
    let backend = Arc::new(BridgeTape {
        location: "out/report.md".to_owned(),
        requests: Mutex::new(Vec::new()),
        verdicts: Arc::new(Mutex::new(Vec::new())),
    });
    let runtime = runtime(&world.0)
        .with_prompt_pause(true)
        .with_harness_backend(backend.clone(), "tape".into())
        .expect("the project holds the seat");
    let (outcome, events) = run(&runtime, "{}").await;
    let pause = outcome.paused.as_ref().expect("the run paused on the gate");
    assert_eq!(
        (pause.task.as_str(), pause.message.as_deref()),
        ("draw", Some("Edit files"))
    );
    assert!(
        backend.verdicts.lock().unwrap().is_empty(),
        "nothing allowed"
    );
    assert_eq!(
        witnesses(&events),
        [
            (
                "agent".to_owned(),
                "session mode · read-only".to_owned(),
                "confine".to_owned()
            ),
            (
                "fs".to_owned(),
                "edit · out/report.md".to_owned(),
                "escalate".to_owned()
            ),
        ]
    );
    let paused_at = (events.iter())
        .position(|e| e.kind == EventKind::WorkflowPaused)
        .expect("the pause frame");
    let last_witness = (events.iter())
        .rposition(|e| e.kind == EventKind::PermitChecked)
        .expect("a witness");
    assert!(last_witness < paused_at, "the decisions precede the pause");
}
