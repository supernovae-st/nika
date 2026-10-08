// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! A served run's relative file paths resolve at the root its host selected,
//! never at the working directory of the process that serves it.
//!
//! A resident service runs every admitted project from one long-lived
//! process whose working directory belongs to none of them. The host hands
//! the admitted project to [`service_runtime`] as its `sandbox_root`; the
//! builtin file plane must then read `./notes/brief.md` and write
//! `./out/copy.md` there. Each test composes the real production plane over
//! a temporary project that is not the test process's directory (Cargo runs
//! integration tests from the package directory) and observes the bytes on
//! disk, the task records, the event stream, and the process directory left
//! as it was. The workflows call file builtins only: no provider is called.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nika_event::{Event, EventKind};
use nika_runtime::child::{ChildCall, ChildOutcome, ChildRunRefusal, ChildRunner};
use nika_runtime::compose::{capabilities_of, service_runtime};
use nika_runtime::{DeterministicStamper, RunOutcome, TaskStatus, VecSink};
use nika_schema::source::FileId;
use nika_schema::{ParseMode, parse};
use serde_json::json;

/// The copy journey a served Session saves for « read ./notes/brief.md and
/// write it to ./out/copy.md »: one read, one write, the written path out.
const COPY: &str = r#"
nika: resident-copy
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./notes/brief.md"]
    write: ["./out/copy.md"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args: { path: "./notes/brief.md" }
  write_copy:
    with:
      brief: ${{ tasks.read_source.output }}
    invoke:
      tool: "nika:write"
      args: { path: "./out/copy.md", content: "${{ with.brief }}" }
outputs:
  copied: ${{ tasks.write_copy.output }}
"#;

/// A listing and a removal under relative grants.
const EFFECTS: &str = r#"
nika: resident-effects
permits:
  tools: ["nika:glob", "nika:remove_file"]
  fs:
    read: ["./notes/**"]
    write: ["./out/stale.md"]
tasks:
  listed:
    invoke:
      tool: "nika:glob"
      args: { pattern: "./notes/*.md" }
  removed:
    invoke:
      tool: "nika:remove_file"
      args: { path: "./out/stale.md" }
"#;

/// A parent admitted as `workflows/parent.nika` delegating to its child.
const PARENT: &str = r#"
nika: resident-parent
tasks:
  delegate:
    invoke:
      workflow: "./sub/child.nika"
"#;

/// The child admitted as `workflows/sub/child.nika`: its data paths are the
/// project's, not its own directory's.
const CHILD: &str = r#"
nika: resident-child
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./notes/brief.md"]
    write: ["./out/child.md"]
tasks:
  read_brief:
    invoke:
      tool: "nika:read"
      args: { path: "./notes/brief.md" }
  write_brief:
    with:
      brief: ${{ tasks.read_brief.output }}
    invoke:
      tool: "nika:write"
      args: { path: "./out/child.md", content: "${{ with.brief }}" }
outputs:
  copied: ${{ tasks.write_brief.output }}
"#;

/// A multi-line UTF-8 brief: the copy must carry these exact bytes.
const BRIEF: &str = "Brief · première ligne\nsecond line, « quoted »\n\ttabbed tail\n";

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A temporary admitted project, created exclusively and removed on drop.
struct Project(PathBuf);

impl Project {
    fn new(tag: &str) -> Self {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nika-resident-root-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir(&root).expect("HARNESS_INVALID: the project root is not ours");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn put(&self, rel: &str, bytes: &[u8]) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().expect("a nested fixture")).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn bytes(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.0.join(rel)).unwrap_or_else(|e| panic!("{rel} under the project: {e}"))
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The relative names these fixtures use must not exist beside the process:
/// a run that resolved against the process directory would read or write
/// there instead.
fn assert_process_dir_lacks_fixture_names() {
    let cwd = std::env::current_dir().unwrap();
    for name in ["notes", "out", "workflows"] {
        assert!(
            !cwd.join(name).exists(),
            "`{name}` appeared under the process directory {}",
            cwd.display()
        );
    }
}

/// Compose the real service plane at `root` (with `children` as its child
/// surface, when given) and run `yaml` to its end.
async fn run_with(
    root: &Path,
    yaml: &str,
    children: Option<Arc<dyn ChildRunner>>,
) -> (RunOutcome, Vec<Event>) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture checks clean: {report:#?}");
    let mut runtime = service_runtime("", capabilities_of(&wf), None, root.to_path_buf())
        .expect("the production plane composes");
    if let Some(children) = children {
        runtime = runtime.with_child_runner(children);
    }
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles");
    (outcome, sink.into_events())
}

async fn run_at(root: &Path, yaml: &str) -> (RunOutcome, Vec<Event>) {
    run_with(root, yaml, None).await
}

fn task_field(event: &Event) -> Option<&str> {
    event.fields.iter().find(|f| f.key == "task").and_then(|f| {
        if let nika_types::resource::Value::String(s) = &f.value {
            Some(s.as_str())
        } else {
            None
        }
    })
}

/// The run settled green: every named record succeeded, the stream completed
/// each task once with no failure, and the run closed on its completed frame.
fn assert_settled(outcome: &RunOutcome, events: &[Event], tasks: &[&str]) {
    for task in tasks {
        let record = &outcome.records[*task];
        assert_eq!(
            record.status,
            TaskStatus::Success,
            "{task}: {:?}",
            record.error
        );
        let completed = events
            .iter()
            .filter(|e| e.kind == EventKind::TaskCompleted && task_field(e) == Some(task))
            .count();
        assert_eq!(completed, 1, "{task} completes exactly once");
    }
    assert!(outcome.ok, "the run settles green: {:#?}", outcome.records);
    assert!(
        !events.iter().any(|e| e.kind == EventKind::TaskFailed),
        "no task failed"
    );
    assert!(
        events
            .iter()
            .any(|e| e.kind == EventKind::WorkflowCompleted),
        "the run closes on its completed frame"
    );
}

/// The caller half of composition, shaped like the service driver's child
/// lane: the target resolves relative to its parent's location, its source
/// comes from the admitted snapshot (nothing is read from disk), and the
/// child composes through the same production seam at the same root.
struct SnapshotChildren {
    root: PathBuf,
    parent_dir: &'static str,
    snapshot: BTreeMap<String, &'static str>,
    resolved: Mutex<Vec<String>>,
}

impl ChildRunner for SnapshotChildren {
    fn run_child<'a>(
        &'a self,
        call: ChildCall,
    ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
        Box::pin(async move {
            let logical = format!(
                "{}/{}",
                self.parent_dir,
                call.target.trim_start_matches("./")
            );
            let Some(source) = self.snapshot.get(&logical) else {
                return Err(ChildRunRefusal {
                    code: "NIKA-COMP-001".to_owned(),
                    message: format!("no admitted unit `{logical}`"),
                });
            };
            self.resolved.lock().unwrap().push(logical);
            let (outcome, _) = run_at(&self.root, source).await;
            let failure = outcome.records.values().find_map(|record| {
                record
                    .error
                    .as_ref()
                    .map(|error| (error.code.clone(), error.message.clone()))
            });
            Ok(ChildOutcome {
                ok: outcome.ok,
                outputs: outcome.outputs,
                cost_usd: None,
                trace: None,
                failure,
            })
        })
    }
}

#[tokio::test]
async fn a_served_copy_reads_and_writes_at_the_admitted_project() {
    let project = Project::new("copy");
    project.put("notes/brief.md", BRIEF.as_bytes());
    let cwd = std::env::current_dir().unwrap();
    assert_process_dir_lacks_fixture_names();

    let (outcome, events) = run_at(project.path(), COPY).await;

    assert_settled(&outcome, &events, &["read_source", "write_copy"]);
    assert_eq!(outcome.records["read_source"].output, json!(BRIEF));
    assert_eq!(
        project.bytes("out/copy.md"),
        BRIEF.as_bytes(),
        "the copy holds the brief's exact bytes, under the admitted project"
    );
    assert_eq!(
        outcome.outputs["copied"],
        json!("./out/copy.md"),
        "the write answers with the path as the workflow spelled it"
    );
    assert_eq!(
        std::env::current_dir().unwrap(),
        cwd,
        "no run moved the process"
    );
    assert_process_dir_lacks_fixture_names();
}

#[tokio::test]
async fn two_admitted_projects_run_the_same_names_concurrently_without_crossing() {
    let first = Project::new("first");
    let second = Project::new("second");
    first.put("notes/brief.md", b"the first project's brief\n");
    second.put("notes/brief.md", b"the second project's brief\n");
    let cwd = std::env::current_dir().unwrap();
    assert_process_dir_lacks_fixture_names();

    let ((a, a_events), (b, b_events)) =
        tokio::join!(run_at(first.path(), COPY), run_at(second.path(), COPY));

    assert_settled(&a, &a_events, &["read_source", "write_copy"]);
    assert_settled(&b, &b_events, &["read_source", "write_copy"]);
    assert_eq!(first.bytes("out/copy.md"), b"the first project's brief\n");
    assert_eq!(second.bytes("out/copy.md"), b"the second project's brief\n");
    assert_eq!(
        std::env::current_dir().unwrap(),
        cwd,
        "no run moved the process"
    );
    assert_process_dir_lacks_fixture_names();
}

#[tokio::test]
async fn a_relative_listing_and_removal_act_at_the_admitted_project() {
    let project = Project::new("effects");
    project.put("notes/a.md", b"a");
    project.put("notes/brief.md", BRIEF.as_bytes());
    project.put("out/stale.md", b"stale");
    project.put("out/keep.md", b"keep");
    assert_process_dir_lacks_fixture_names();

    let (outcome, events) = run_at(project.path(), EFFECTS).await;

    assert_settled(&outcome, &events, &["listed", "removed"]);
    assert_eq!(
        outcome.records["listed"].output,
        json!(["./notes/a.md", "./notes/brief.md"]),
        "the listing names the project's files as the workflow spelled the root"
    );
    assert_eq!(outcome.records["removed"].output, json!("./out/stale.md"));
    assert!(
        !project.path().join("out/stale.md").exists(),
        "the stale file is gone"
    );
    assert_eq!(project.bytes("out/keep.md"), b"keep", "its neighbour stays");
    assert_process_dir_lacks_fixture_names();
}

#[tokio::test]
async fn a_child_reads_and_writes_project_relative_data_at_the_same_root() {
    let project = Project::new("child");
    project.put("notes/brief.md", BRIEF.as_bytes());
    // Were data paths rebased to the child's own directory, this decoy
    // would be copied instead and the copy would land beside it.
    project.put(
        "workflows/sub/notes/brief.md",
        b"the child directory's decoy\n",
    );
    assert_process_dir_lacks_fixture_names();
    let children = Arc::new(SnapshotChildren {
        root: project.path().to_path_buf(),
        parent_dir: "workflows",
        snapshot: BTreeMap::from([("workflows/sub/child.nika".to_owned(), CHILD)]),
        resolved: Mutex::new(Vec::new()),
    });
    let surface: Arc<dyn ChildRunner> = children.clone();

    let (outcome, events) = run_with(project.path(), PARENT, Some(surface)).await;

    assert_settled(&outcome, &events, &["delegate"]);
    assert_eq!(
        outcome.records["delegate"].output,
        json!({ "copied": "./out/child.md" }),
        "the child's outputs are the parent task's value"
    );
    assert_eq!(
        *children.resolved.lock().unwrap(),
        ["workflows/sub/child.nika"],
        "the definition resolved once, relative to its parent"
    );
    assert_eq!(project.bytes("out/child.md"), BRIEF.as_bytes());
    assert!(!project.path().join("workflows/sub/out").exists());
    assert_eq!(
        project.bytes("workflows/sub/notes/brief.md"),
        b"the child directory's decoy\n"
    );
    assert_process_dir_lacks_fixture_names();
}
