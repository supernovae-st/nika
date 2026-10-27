// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Spec 02 §Vision inputs at run, on the REAL builtin plane: the production `BuiltinDispatcher`
//! over a real directory (`AnchoredFs`, the production fs), bounded by the workflow's own
//! `permits.fs` exactly as the composition roots bound it. The provider is a capturing
//! OpenAI-compatible wire, so "zero provider calls" is a count of requests, not a hope.
//!
//! Absolute paths make the witnesses sharp: before this lane the verb opened them itself with
//! no boundary, so an ungranted secret, a symlink and an absent block all reached the wire.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
use nika_event::{Event, EventKind};
use nika_fs::AnchoredFs;
use nika_kernel::http::HttpRequest;
use nika_kernel::secret::Secret;
use nika_kernel_mock::{MockClock, MockHttp, MockProvider, MockShell, MockToolDefinitionProvider};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

use crate::dispatch::infer_deadline_tests::CapturingHttp;
use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};

/// A PNG signature and a tag: an image the wire can be searched for.
fn png(tag: &str) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    bytes.extend_from_slice(tag.as_bytes());
    bytes
}

/// A run's project directory on the real filesystem (canonical, so a grant and a path name
/// one identity), removed on drop.
struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nika-vision-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("project root");
        Self(root.canonicalize().expect("canonical root"))
    }

    fn file(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(&path, bytes).expect("seed file");
        path
    }

    fn abs(&self, relative: &str) -> String {
        self.0.join(relative).to_string_lossy().into_owned()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Ran {
    outcome: RunOutcome,
    events: Vec<Event>,
    wire: Vec<HttpRequest>,
}

impl Ran {
    /// The task's failure code (the record a `tasks.X.error.code` reads).
    fn code(&self, task: &str) -> String {
        let record = &self.outcome.records[task];
        record
            .error
            .as_ref()
            .expect("a failure record")
            .code
            .clone()
    }

    fn message(&self, task: &str) -> String {
        let record = &self.outcome.records[task];
        record
            .error
            .as_ref()
            .expect("a failure record")
            .message
            .clone()
    }

    /// Every `permit_checked` fs frame as `(gate, decision)`.
    fn fs_frames(&self) -> Vec<(String, String)> {
        self.events
            .iter()
            .filter(|e| e.kind == EventKind::PermitChecked && e.str_field("plane") == Some("fs"))
            .map(|e| {
                (
                    e.str_field("gate").unwrap_or_default().to_owned(),
                    e.str_field("decision").unwrap_or_default().to_owned(),
                )
            })
            .collect()
    }
}

/// Check `yaml` (it must be check-clean: the run is the judge here), then run it on the real
/// builtin plane rooted at `project`, its boundary the workflow's own, its inputs `inputs`.
async fn run(project: &Project, yaml: &str, inputs: &[(&str, &str)]) -> Ran {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "the run judges this one: {report:?}");
    let http = Arc::new(CapturingHttp::default());
    let registry = Arc::new(ProviderRegistry::new(
        Arc::clone(&http),
        ProvidersConfig::new().with_key("openai", Secret::new("sk-test")),
    ));
    let plane = Arc::new(
        BuiltinDispatcher::new(
            Arc::new(AnchoredFs::new(project.0.clone())),
            Arc::new(MockHttp::new()),
            Arc::new(MockClock::new()),
            Arc::new(NullEmitter::default()),
            Arc::new(NonInteractive::default()),
            Arc::new(NoWorkflow::default()),
        )
        .with_fs_boundary(crate::compose::fs_boundary_of(&wf)),
    );
    let invoke = Arc::new(InvokeVerb::new(plane));
    let overrides: BTreeMap<String, serde_json::Value> = inputs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
        .collect();
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "openai/gpt-4o-mini"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
    .with_var_overrides(overrides);
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the run settles (a task failure is data)");
    Ran {
        outcome,
        events: sink.into_events(),
        wire: http.captured(),
    }
}

/// One `infer:` task `look` reading `path` under the `permits` lines; a path that renders
/// `inputs.pic` gets that input declared (default `./diagrams/flow.png`).
fn look(permits: &str, path: &str) -> String {
    let inputs = if path.contains("inputs.pic") {
        "inputs:\n  pic: { type: string, default: \"./diagrams/flow.png\" }\n"
    } else {
        ""
    };
    let path_line = format!("          path: \"{path}\"\n");
    [
        "nika: vision-read\n",
        "model: openai/gpt-4o-mini\n",
        permits,
        inputs,
        "tasks:\n",
        "  look:\n",
        "    infer:\n",
        "      prompt: \"Describe the flow\"\n",
        "      max_tokens: 16\n",
        "      vision:\n",
        "        - source: file\n",
        path_line.as_str(),
    ]
    .concat()
}

fn grant(read: &str) -> String {
    format!("permits:\n  fs: {{ read: [\"{read}\"] }}\n")
}

/// The data URL of `bytes` as the openai-compatible wire carries it.
fn wire_image(request: &HttpRequest) -> Option<String> {
    let body: serde_json::Value =
        serde_json::from_slice(request.body.as_ref().expect("a body")).expect("json body");
    body["messages"][0]["content"]
        .as_array()?
        .iter()
        .find(|part| part["type"] == "image_url")
        .and_then(|part| part["image_url"]["url"].as_str())
        .map(str::to_owned)
}

fn data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// The granted read: through `nika:read` under the workflow's boundary, anchored at the run's
/// root (never the process directory), witnessed by one allowed fs frame, its bytes on the wire.
#[tokio::test]
async fn a_granted_vision_file_is_read_inside_the_boundary_and_rides_the_wire() {
    let project = Project::new("granted");
    let bytes = png("granted-flow");
    project.file("diagrams/flow.png", &bytes);
    assert!(
        !Path::new("./diagrams/flow.png").exists(),
        "the test process directory holds no such file: only the run's root does"
    );
    let ran = run(
        &project,
        &look(&grant("./diagrams/**"), "./diagrams/flow.png"),
        &[],
    )
    .await;
    assert!(ran.outcome.ok, "{:?}", ran.outcome.records["look"].error);
    assert_eq!(ran.wire.len(), 1, "one provider round-trip");
    assert_eq!(wire_image(&ran.wire[0]), Some(data_url(&bytes)));
    let frames = ran.fs_frames();
    assert_eq!(frames.len(), 1, "one decision, one frame: {frames:?}");
    assert!(
        frames[0].0.contains("./diagrams/flow.png") && frames[0].1 == "allow",
        "{frames:?}"
    );
}

/// A rendered path outside the grant fails the task `NIKA-SEC-004` before the file is opened
/// or any provider is called, and the refusal is witnessed as a denied fs frame.
#[tokio::test]
async fn a_rendered_path_outside_the_grant_is_refused_before_any_provider_call() {
    let project = Project::new("outside");
    project.file("diagrams/flow.png", &png("flow"));
    project.file("secrets/key.png", b"API_KEY=sk-live-not-an-image\n");
    let ran = run(
        &project,
        &look(&grant("./diagrams/**"), "${{ inputs.pic }}"),
        &[("pic", &project.abs("secrets/key.png"))],
    )
    .await;
    assert!(!ran.outcome.ok);
    assert_eq!(ran.code("look"), "NIKA-SEC-004", "{}", ran.message("look"));
    assert!(
        ran.wire.is_empty(),
        "the secret never reached a provider: {} request(s)",
        ran.wire.len()
    );
    let frames = ran.fs_frames();
    assert!(
        frames
            .iter()
            .any(|(gate, decision)| gate.contains("secrets/key.png") && decision == "deny"),
        "{frames:?}"
    );
}

/// No `permits:` block is zero authority (F-O8): a rendered vision path deferred by the check
/// meets the empty boundary at run.
#[tokio::test]
async fn an_absent_block_refuses_a_rendered_vision_file_at_run() {
    let project = Project::new("absent");
    project.file("diagrams/flow.png", &png("flow"));
    let ran = run(
        &project,
        &look("", "${{ inputs.pic }}"),
        &[("pic", &project.abs("diagrams/flow.png"))],
    )
    .await;
    assert_eq!(ran.code("look"), "NIKA-SEC-004", "{}", ran.message("look"));
    assert!(ran.wire.is_empty(), "nothing was sent");
}

/// Symlinks and `..` are judged on the resolved path, never the spelling: a granted name that
/// is a symlink to a secret, and a `..` climbing out of the grant, both refuse.
#[cfg(unix)]
#[tokio::test]
async fn symlink_and_dot_dot_escapes_are_judged_on_the_resolved_path() {
    let project = Project::new("escape");
    project.file("secrets/key.png", &png("secret"));
    std::fs::create_dir_all(project.0.join("diagrams")).expect("dir");
    std::os::unix::fs::symlink(
        project.0.join("secrets/key.png"),
        project.0.join("diagrams/flow.png"),
    )
    .expect("symlink");
    let diagrams = format!("{}/**", project.abs("diagrams"));
    let linked = run(
        &project,
        &look(&grant(&diagrams), &project.abs("diagrams/flow.png")),
        &[],
    )
    .await;
    assert_eq!(
        linked.code("look"),
        "NIKA-SEC-004",
        "{}",
        linked.message("look")
    );
    assert!(linked.wire.is_empty(), "the link's target was never sent");
    let climbed = run(
        &project,
        &look(&grant(&diagrams), "${{ inputs.pic }}"),
        &[(
            "pic",
            &format!("{}/../secrets/key.png", project.abs("diagrams")),
        )],
    )
    .await;
    assert_eq!(
        climbed.code("look"),
        "NIKA-SEC-004",
        "{}",
        climbed.message("look")
    );
    assert!(climbed.wire.is_empty());
}

/// #1135 kept: a granted path with no file fails `NIKA-432` (`vision`) with zero provider calls.
#[tokio::test]
async fn a_missing_vision_file_fails_before_any_provider_call() {
    let project = Project::new("missing");
    let ran = run(
        &project,
        &look(&grant("./diagrams/**"), "./diagrams/absent.png"),
        &[],
    )
    .await;
    assert!(!ran.outcome.ok, "a missing image file is never a green run");
    assert_eq!(ran.code("look"), "NIKA-432", "InvalidParam wire form");
    assert!(
        ran.message("look").contains("vision"),
        "the vision param is named: {}",
        ran.message("look")
    );
    assert!(ran.wire.is_empty(), "the file gate fires first");
}
