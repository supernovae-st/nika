// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A rehearsal run through the real composition: the admitted pair runs on the existing runtime
//! over a rooted room, under the workflow's own permits.
//!
//! - A copy reads the room's bytes, never a decoy of other bytes at the same relative path under
//!   the process working directory, and lands in the room alone; the room's ledger records it.
//! - Every capability beyond the room is refused where the runtime reaches for it, and counted:
//!   no request leaves, no process spawns, no person is asked, no model is called, no secret
//!   resolves, no nested run starts. The plan here is the pure resolver's, so a model verb
//!   reaches the composition; a host refuses it before any room, by its screen and the
//!   rehearsal plan.
//! - A run stopped at its bound leaves its in-flight operation to the room's drain, which joins it.
//!
//! Each fixture is admitted through the One Door first, by its own guard and again by its test:
//! a refusal there is `HARNESS_INVALID`, never a semantic RED. Every real call leaves evidence
//! lines: `begin` before it (the call is engaged, which proves no run), `return` right after it,
//! before anything is propagated or asserted, and `drain` once the room is drained. They carry
//! identities, kinds and counts, never a source, a runtime message or a path.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bytes::Bytes;
use nika_event::source_id::sha256_hex;
use nika_fs::{EffectLedger, OwnedDir, Phase, RoomLimits, RootedFs};
use nika_kernel::fs::{FileMetadata, FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};
use nika_runtime::{RunOutcome, RuntimeError, TaskStatus};
use serde_json::{Value, json};

use super::replay::{Capture, Captures};
use super::room::OUTSIDE;
use super::{DeniedEffects, DeniedTally};
use crate::{ExecutionAccessPlan, ServiceExecutionDriver};

type TestResult<T> = Result<T, Box<dyn std::error::Error>>; // box-dyn-ok(test-harness): cfg(test) fixtures use heterogeneous setup failures

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A relative prefix no other test of this process uses.
fn prefix() -> String {
    format!(
        "rehearsal-run-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    )
}

/// The copy of `./<prefix>/in/source.txt` to `./<prefix>/out/copied.txt`, granted exactly those.
fn copy(prefix: &str) -> String {
    let (from, to) = (
        format!("./{prefix}/in/source.txt"),
        format!("./{prefix}/out/copied.txt"),
    );
    format!(
        "nika: copy\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"{from}\"]\n    write: [\"{to}\"]\ntasks:\n  read_it:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{from}\" }} }}\n  write_it:\n    with: {{ content: \"${{{{ tasks.read_it.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{to}\", content: \"${{{{ with.content }}}}\", create_dirs: true, overwrite: true }} }}\n"
    )
}

/// A request to a host that never resolves.
const FETCH: &str = "nika: f\npermits:\n  tools: [\"nika:fetch\"]\n  net: { http: [\"example.invalid\"] }\ntasks:\n  get:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.invalid/x\", mode: text } }\n";

/// A process that would leave `sentinel` behind.
fn touching(sentinel: &str) -> String {
    format!(
        "nika: e\npermits: {{ exec: [\"touch\"] }}\ntasks:\n  run:\n    exec: {{ command: [\"touch\", \"{sentinel}\"] }}\n"
    )
}

/// A question to a person.
const PROMPT: &str = "nika: p\npermits: { tools: [\"nika:prompt\"] }\ntasks:\n  ask:\n    invoke: { tool: \"nika:prompt\", args: { message: \"Proceed?\" } }\n";

/// A model turn.
const AGENT: &str = "nika: a\nmodel: mock/echo\npermits: {}\ntasks:\n  act:\n    agent: { prompt: hi, max_turns: 1 }\n";

/// A secret declared and never read: the runtime still asks its resolver once, at run start.
const SECRET: &str = "nika: s\nsecrets:\n  token: { source: env, key: NIKA_REHEARSAL_NEVER_SET }\npermits: { tools: [\"nika:log\"] }\ntasks:\n  say:\n    invoke: { tool: \"nika:log\", args: { message: hello } }\n";

/// A nested run of `CHILD`, which the admitted project holds as `child.nika`.
const PARENT: &str = "nika: parent\npermits: { tools: [\"nika:log\"] }\ntasks:\n  child:\n    invoke: { workflow: \"./child.nika\" }\n";
const CHILD: &str = "nika: child\npermits: { tools: [\"nika:log\"] }\ntasks:\n  say:\n    invoke: { tool: \"nika:log\", args: { message: child } }\n";

type Fixture = (&'static str, String, Vec<(&'static str, &'static str)>);

/// Every fixture this module runs, over `prefix`: its label, its bytes and the files its admitted
/// project holds.
fn fixtures(prefix: &str) -> Vec<Fixture> {
    vec![
        ("copy", copy(prefix), Vec::new()),
        ("request", FETCH.to_owned(), Vec::new()),
        (
            "process",
            touching(&format!("{prefix}-spawned")),
            Vec::new(),
        ),
        ("person", PROMPT.to_owned(), Vec::new()),
        ("model turn", AGENT.to_owned(), Vec::new()),
        ("secret", SECRET.to_owned(), Vec::new()),
        ("nested run", PARENT.to_owned(), vec![("child.nika", CHILD)]),
    ]
}

/// The driver of `source`, admitted from its bytes over `project`, with the pure resolver's plan
/// and the digest the door gave the admitted world.
fn admitted(
    source: &str,
    project: &Path,
) -> TestResult<(ServiceExecutionDriver, ExecutionAccessPlan, String)> {
    let service = nika_execution::ExecutionService::default();
    let admitted = service
        .admit_root_bytes(
            &OwnedDir::open(project)?,
            Path::new("candidate.nika"),
            source.as_bytes(),
        )
        .map_err(|refusal| format!("HARNESS_INVALID: the door refused the fixture: {refusal}"))?;
    let digest = admitted.snapshot().digest().to_owned();
    let session = service.begin(admitted);
    let driver = ServiceExecutionDriver::new(session.context(), project)
        .ok_or("HARNESS_INVALID: the admitted context lost its root")?;
    let plan = driver.resolve_access_plan_over(None, None, &[]);
    Ok((driver, plan, digest))
}

#[test]
fn the_fixtures_are_admitted_before_any_run() -> TestResult<()> {
    // The harness guard, in its own exact invocation before the runs: every fixture this module
    // runs is admitted by the One Door from its bytes. No runtime is called.
    let mut refused = Vec::new();
    for (label, source, files) in fixtures(&prefix()) {
        let project = tempfile::tempdir()?;
        for (path, text) in &files {
            std::fs::write(project.path().join(path), text)?;
        }
        if let Err(why) = admitted(&source, project.path()) {
            refused.push(format!("{label}: {why}"));
        }
    }
    assert!(refused.is_empty(), "HARNESS_INVALID fixtures: {refused:#?}");
    Ok(())
}

/// A room holding `files`, its ledger moved on to the run phase.
async fn room(files: &[(&str, &str)]) -> TestResult<(tempfile::TempDir, Arc<RootedFs>)> {
    let dir = tempfile::tempdir()?;
    let ledger = EffectLedger::new(RoomLimits::new(1 << 20, 64));
    let fs = Arc::new(RootedFs::new(
        OwnedDir::open(dir.path())?,
        Arc::clone(&ledger),
    ));
    for (path, text) in files {
        fs.write(Path::new(path), text.as_bytes()).await?;
    }
    let _ = ledger.seal_and_drain().await;
    if ledger.advance() != Ok(Phase::Run) {
        return Err("the room did not reach its run phase".into());
    }
    Ok((dir, fs))
}

/// One evidence line, captured by the test harness like any test output (`--nocapture` shows
/// it at once).
#[expect(
    clippy::disallowed_macros,
    clippy::print_stdout,
    reason = "the test-only evidence line the qualification runner reads from the test output"
)]
fn evidence(line: &Value) {
    println!("NIKA_REHEARSAL_EVIDENCE_V1 {line}");
}

/// The candidate as a call hands it over: its sha256 and its length, never its text.
fn handed(source: &str) -> Value {
    json!({"sha256": sha256_hex(source.as_bytes()), "bytes": source.len()})
}

/// How the runtime ended: an outcome that settled, with `ok`, or a launch refusal by its code,
/// never its message. A refusal leaves where the run stood unknown.
fn ended(end: &Result<RunOutcome, RuntimeError>) -> Value {
    match end {
        Ok(outcome) => json!({"kind": "outcome", "ok": outcome.ok}),
        Err(error) => json!({"kind": "runtime_error", "code": error.spec_code()}),
    }
}

/// The denied attempts, counted.
fn counts(denied: &DeniedEffects) -> Value {
    json!({
        "network": denied.network, "provider": denied.provider, "spawn": denied.spawn,
        "prompt": denied.prompt, "secret": denied.secret, "child": denied.child,
    })
}

/// One run of `source` (its world seeded with `project` files) in a room holding `files`, as
/// `subrun` of `test`, drained after it: the room, the run's end and what its denied
/// capabilities saw. The `return` line precedes any propagation, the `drain` line the caller's
/// first assertion.
async fn rehearsed(
    test: &str,
    subrun: &str,
    source: &str,
    project: &[(&str, &str)],
    files: &[(&str, &str)],
) -> TestResult<(
    tempfile::TempDir,
    Arc<RootedFs>,
    Result<RunOutcome, RuntimeError>,
    DeniedEffects,
)> {
    let world = tempfile::tempdir()?;
    for (path, text) in project {
        std::fs::write(world.path().join(path), text)?;
    }
    let (driver, plan, digest) = admitted(source, world.path())?;
    let (dir, fs) = room(files).await?;
    let tally = Arc::new(DeniedTally::default());
    evidence(&json!({
        "event": "begin", "test": test, "subrun": subrun, "candidate": handed(source),
        "admitted_digest": digest,
    }));
    let end = driver
        .rehearse_over(Arc::clone(&fs), plan, Arc::clone(&tally))
        .await;
    evidence(&json!({
        "event": "return", "test": test, "subrun": subrun, "candidate": handed(source),
        "end": ended(&end), "denied": counts(&tally.counted()),
    }));
    let drained = fs.ledger().seal_and_drain().await;
    // counted once the room is drained, as the assertions always read it
    let denied = tally.counted();
    evidence(&json!({
        "event": "drain", "test": test, "subrun": subrun, "joined": drained.joined,
        "panicked": drained.panicked, "written": fs.ledger().written().count(),
        "late_refusals": fs.ledger().late_refusals(), "denied": counts(&denied),
    }));
    Ok((dir, fs, end, denied))
}

/// Files under the process working directory at a room's relative paths, under a root this test
/// created and owns: removed when dropped.
struct Decoy(PathBuf);

impl Decoy {
    /// The decoy's root, created exclusively and owned from that instant, then its files. A root
    /// that already exists is `HARNESS_INVALID`: never resumed, overwritten or removed.
    fn new(prefix: &str, files: &[(&str, &str)]) -> TestResult<Self> {
        let root = std::env::current_dir()?.join(prefix);
        std::fs::create_dir(&root).map_err(|error| {
            format!("HARNESS_INVALID: decoy root ./{prefix} is not this test's to make: {error}")
        })?;
        let decoy = Self(root);
        for (rel, text) in files {
            let path = decoy.0.join(rel);
            std::fs::create_dir_all(path.parent().ok_or("a decoy file has a parent")?)?;
            std::fs::write(path, text)?;
        }
        Ok(decoy)
    }

    fn read(&self, rel: &str) -> TestResult<String> {
        Ok(std::fs::read_to_string(self.0.join(rel))?)
    }
}

impl Drop for Decoy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn an_admitted_copy_reads_and_writes_its_room_and_the_ledger_records_it() -> TestResult<()> {
    const TEST: &str = concat!(
        module_path!(),
        "::an_admitted_copy_reads_and_writes_its_room_and_the_ledger_records_it"
    );
    let p = prefix();
    let decoy = Decoy::new(
        &p,
        &[
            ("in/source.txt", "decoy\n"),
            ("out/copied.txt", "decoy out\n"),
        ],
    )?;
    let source = format!("{p}/in/source.txt");
    let (dir, fs, end, denied) = rehearsed(
        TEST,
        "only",
        &copy(&p),
        &[],
        &[(source.as_str(), "alpha\n")],
    )
    .await?;
    let outcome = end?;
    assert!(outcome.ok, "{outcome:?}");
    let copied = std::fs::read_to_string(dir.path().join(&p).join("out/copied.txt"))?;
    assert_eq!(copied, "alpha\n", "the room's bytes, never the decoy's");
    let written: Vec<PathBuf> = fs.ledger().written().collect();
    assert_eq!(written, [PathBuf::from(format!("{p}/out/copied.txt"))]);
    assert_eq!(denied, DeniedEffects::default());
    assert_eq!(decoy.read("in/source.txt")?, "decoy\n");
    assert_eq!(decoy.read("out/copied.txt")?, "decoy out\n");
    Ok(())
}

/// Run `source` as the only call of `test` and assert its task failed, `denied` counted exactly
/// once, nothing else.
async fn refused_and_counted(
    test: &str,
    source: &str,
    project: &[(&str, &str)],
    expected: fn(&DeniedEffects) -> u32,
) -> TestResult<DeniedEffects> {
    let (_dir, _fs, end, denied) = rehearsed(test, "only", source, project, &[]).await?;
    assert!(
        end.as_ref().is_ok_and(|outcome| !outcome.ok),
        "the task failed, the run settled: {end:?}"
    );
    assert_eq!(expected(&denied), 1, "{denied:?}");
    Ok(denied)
}

#[tokio::test]
async fn a_request_is_refused_before_any_socket_and_counted() -> TestResult<()> {
    const TEST: &str = concat!(
        module_path!(),
        "::a_request_is_refused_before_any_socket_and_counted"
    );
    let denied = refused_and_counted(TEST, FETCH, &[], |denied| denied.network).await?;
    assert_eq!(
        denied.provider + denied.spawn + denied.prompt + denied.secret + denied.child,
        0
    );
    Ok(())
}

#[tokio::test]
async fn a_process_never_spawns_and_is_counted() -> TestResult<()> {
    const TEST: &str = concat!(module_path!(), "::a_process_never_spawns_and_is_counted");
    let sentinel = format!("{}-spawned", prefix());
    refused_and_counted(TEST, &touching(&sentinel), &[], |denied| denied.spawn).await?;
    assert!(
        !std::env::current_dir()?.join(&sentinel).exists(),
        "the command never ran"
    );
    Ok(())
}

#[tokio::test]
async fn a_person_is_never_asked_and_the_prompt_is_counted() -> TestResult<()> {
    const TEST: &str = concat!(
        module_path!(),
        "::a_person_is_never_asked_and_the_prompt_is_counted"
    );
    refused_and_counted(TEST, PROMPT, &[], |denied| denied.prompt).await?;
    Ok(())
}

#[tokio::test]
async fn a_model_turn_is_refused_before_any_provider_and_counted() -> TestResult<()> {
    const TEST: &str = concat!(
        module_path!(),
        "::a_model_turn_is_refused_before_any_provider_and_counted"
    );
    refused_and_counted(TEST, AGENT, &[], |denied| denied.provider).await?;
    Ok(())
}

#[tokio::test]
async fn a_secret_never_resolves_and_is_counted() -> TestResult<()> {
    const TEST: &str = concat!(module_path!(), "::a_secret_never_resolves_and_is_counted");
    let (_dir, _fs, end, denied) = rehearsed(TEST, "only", SECRET, &[], &[]).await?;
    assert!(end.is_ok(), "{end:?}");
    assert_eq!(denied.secret, 1, "{denied:?}");
    Ok(())
}

#[tokio::test]
async fn a_nested_run_never_starts_and_is_counted() -> TestResult<()> {
    const TEST: &str = concat!(module_path!(), "::a_nested_run_never_starts_and_is_counted");
    refused_and_counted(TEST, PARENT, &[("child.nika", CHILD)], |denied| {
        denied.child
    })
    .await?;
    Ok(())
}

/// The room, each read first running one slow operation registered in the room's ledger, the
/// way the room registers its own; the operation marks `done` when it finishes.
struct Slow {
    inner: Arc<RootedFs>,
    done: Arc<AtomicBool>,
}

impl Slow {
    async fn slow(&self) -> Result<(), FsError> {
        let done = Arc::clone(&self.done);
        let answer = self
            .inner
            .ledger()
            .run_blocking(move || {
                // the room's own operations are blocking too: this one simply takes its time
                std::thread::sleep(Duration::from_millis(600));
                done.store(true, Ordering::SeqCst);
            })
            .map_err(|refusal| FsError::Io {
                reason: refusal.to_string(),
            })?;
        answer.await.map_err(|_| FsError::Io {
            reason: "the slow operation stopped before it answered".to_owned(),
        })
    }
}

impl FsReadDyn for Slow {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        self.slow().await?;
        self.inner.read(path).await
    }

    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        self.slow().await?;
        self.inner.read_to_string(path).await
    }

    async fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path).await
    }

    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        self.inner.canonicalize(path).await
    }

    async fn read_pinned(&self, path: &Path) -> Result<Bytes, FsError> {
        self.slow().await?;
        self.inner.read_pinned(path).await
    }
}

impl FsWriteDyn for Slow {
    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        self.inner.write(path, contents).await
    }

    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        self.inner.create_dir_all(path).await
    }

    async fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        self.inner.remove_file(path).await
    }
}

impl FsMetaDyn for Slow {
    async fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError> {
        self.inner.metadata(path).await
    }
}

impl FsListDyn for Slow {
    async fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        self.inner.list_dir(path).await
    }

    async fn glob(&self, root: &Path, pattern: &str) -> Result<Vec<PathBuf>, FsError> {
        self.inner.glob(root, pattern).await
    }
}

#[tokio::test]
async fn a_run_stopped_at_its_bound_leaves_its_operation_to_the_drain() -> TestResult<()> {
    const TEST: &str = concat!(
        module_path!(),
        "::a_run_stopped_at_its_bound_leaves_its_operation_to_the_drain"
    );
    let p = prefix();
    let world = tempfile::tempdir()?;
    let source = copy(&p);
    let (driver, plan, digest) = admitted(&source, world.path())?;
    let input = format!("{p}/in/source.txt");
    let (dir, fs) = room(&[(input.as_str(), "alpha\n")]).await?;
    let done = Arc::new(AtomicBool::new(false));
    let slow = Arc::new(Slow {
        inner: Arc::clone(&fs),
        done: Arc::clone(&done),
    });
    let tally = Arc::new(DeniedTally::default());
    evidence(&json!({
        "event": "begin", "test": TEST, "subrun": "only", "candidate": handed(&source),
        "admitted_digest": digest,
    }));
    let started = Instant::now();
    let run = driver.rehearse_over(slow, plan, Arc::clone(&tally));
    let stopped = tokio::time::timeout(Duration::from_millis(150), run).await;
    let end = match &stopped {
        Err(_) => json!({"kind": "stopped_at_the_bound"}),
        Ok(end) => ended(end),
    };
    evidence(&json!({
        "event": "return", "test": TEST, "subrun": "only", "candidate": handed(&source),
        "end": end, "denied": counts(&tally.counted()),
    }));
    let drained = fs.ledger().seal_and_drain().await;
    let measured_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    evidence(&json!({
        "event": "drain", "test": TEST, "subrun": "only", "joined": drained.joined,
        "panicked": drained.panicked, "done": done.load(Ordering::SeqCst),
        "written": fs.ledger().written().count(), "late_refusals": fs.ledger().late_refusals(),
        "measured_ms": measured_ms, "denied": counts(&tally.counted()),
    }));
    assert!(
        stopped.is_err(),
        "the bound stopped the run first: {stopped:?}"
    );
    assert!(
        drained.joined >= 1,
        "the drain took the operation: {drained:?}"
    );
    assert!(
        done.load(Ordering::SeqCst),
        "the operation finished before the drain answered"
    );
    assert!(started.elapsed() >= Duration::from_millis(600));
    assert_eq!(fs.ledger().late_refusals(), 0);
    assert!(
        !dir.path().join(&p).join("out/copied.txt").exists(),
        "nothing was written after the stop"
    );
    Ok(())
}

// ─── the isolated jq evaluator, before any helper runs ──────────────────

fn jq_call(input: &serde_json::Value) -> nika_kernel::tool_executor::ToolCall {
    let args = serde_json::json!({"input": input, "expression": "length"});
    nika_kernel::tool_executor::ToolCall::new("call-1", "nika:jq", args)
}

#[tokio::test]
async fn an_oversized_jq_request_is_bounded_before_any_helper_starts() {
    // The helper path names nothing: a request that reached a spawn would fail to start, a
    // different reason from the one asserted here.
    let helper = super::JqHelper::new("/nonexistent/nika-helper");
    let jq = super::IsolatedJq::new(helper, Instant::now() + Duration::from_secs(5));
    let refused = jq
        .evaluate(&jq_call(&serde_json::json!("x".repeat(3 * 1024 * 1024))))
        .await;
    let error = refused.expect_err("bounded");
    assert!(error.to_string().contains("rehearsal jq bound"), "{error}");
    let bound = jq.bound().expect("the bound is kept");
    assert!(bound.reason.contains("bytes, over the"), "{}", bound.reason);
    assert!(bound.reaped, "nothing started, nothing left");
}

#[tokio::test]
async fn a_helper_that_cannot_start_bounds_the_call_and_the_first_bound_is_kept() {
    let helper = super::JqHelper::new("/nonexistent/nika-helper");
    let jq = super::IsolatedJq::new(helper, Instant::now() + Duration::from_secs(5));
    let first = jq.evaluate(&jq_call(&serde_json::json!([1, 2]))).await;
    assert!(first.is_err());
    let kept = jq.bound().expect("a bound");
    assert!(kept.reason.contains("could not run"), "{}", kept.reason);
    let oversized = serde_json::json!("x".repeat(3 * 1024 * 1024));
    let _ = jq.evaluate(&jq_call(&oversized)).await;
    assert_eq!(
        jq.bound(),
        Some(kept),
        "the first bound of the run is the one kept"
    );
}

// ─── a replay trial: a captured GET answered, nothing else ──────────────

/// The feed a replay trial captured.
const FEED_URL: &str = "https://feed.example/items";

/// A request of `url` by `method`, its text kept whole at `./<prefix>/out/feed.txt`.
fn keeping(prefix: &str, url: &str, method: &str) -> String {
    let to = format!("./{prefix}/out/feed.txt");
    format!(
        "nika: keep\npermits:\n  tools: [\"nika:fetch\", \"nika:write\"]\n  net: {{ http: [\"feed.example\"] }}\n  fs:\n    write: [\"{to}\"]\ntasks:\n  feed:\n    invoke: {{ tool: \"nika:fetch\", args: {{ url: \"{url}\", method: {method}, mode: text }} }}\n  keep:\n    with: {{ text: \"${{{{ tasks.feed.output }}}}\" }}\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{to}\", content: \"${{{{ with.text }}}}\", create_dirs: true, overwrite: true }} }}\n"
    )
}

/// The trial's captures: the feed, as plain text.
fn feed_captures() -> TestResult<Arc<Captures>> {
    let mut captures = Captures::new();
    let text = Some("text/plain".to_owned());
    let page = Capture::new(FEED_URL, 200, text, b"hello feed".to_vec(), 7);
    captures
        .insert(page)
        .map_err(|refused| refused.to_string())?;
    Ok(Arc::new(captures))
}

/// One replay trial of `source` over `captures` as `test`, drained after it: the room, the run's
/// end and what its denied capabilities saw, with the evidence lines of every run here.
async fn replayed(
    test: &str,
    source: &str,
    captures: Arc<Captures>,
) -> TestResult<(
    tempfile::TempDir,
    Result<RunOutcome, RuntimeError>,
    DeniedEffects,
)> {
    let world = tempfile::tempdir()?;
    let (driver, _, digest) = admitted(source, world.path())?;
    let plan = (driver.replay_plan(None)).map_err(|refused| refused.to_string())?;
    let (dir, fs) = room(&[]).await?;
    let tally = Arc::new(DeniedTally::default());
    evidence(&json!({
        "event": "begin", "test": test, "subrun": "replay", "candidate": handed(source),
        "admitted_digest": digest,
    }));
    let end = driver
        .rehearse_over_replaying(
            Arc::clone(&fs),
            plan,
            Arc::clone(&tally),
            None,
            Some(captures),
        )
        .await;
    evidence(&json!({
        "event": "return", "test": test, "subrun": "replay", "candidate": handed(source),
        "end": ended(&end), "denied": counts(&tally.counted()),
    }));
    let drained = fs.ledger().seal_and_drain().await;
    let denied = tally.counted();
    evidence(&json!({
        "event": "drain", "test": test, "subrun": "replay", "joined": drained.joined,
        "panicked": drained.panicked, "written": fs.ledger().written().count(),
        "late_refusals": fs.ledger().late_refusals(), "denied": counts(&denied),
    }));
    Ok((dir, end, denied))
}

#[tokio::test]
async fn a_captured_get_is_replayed_and_nothing_leaves_the_room() -> TestResult<()> {
    const TEST: &str = concat!(
        module_path!(),
        "::a_captured_get_is_replayed_and_nothing_leaves_the_room"
    );
    let p = prefix();
    let source = keeping(&p, FEED_URL, "GET");
    let (dir, end, denied) = replayed(TEST, &source, feed_captures()?).await?;
    let outcome = end?;
    assert!(outcome.ok, "{outcome:?}");
    let kept = std::fs::read_to_string(dir.path().join(&p).join("out/feed.txt"))?;
    assert!(
        kept.contains("hello feed"),
        "the capture's own bytes: {kept:?}"
    );
    assert_eq!(denied, DeniedEffects::default());
    Ok(())
}

#[tokio::test]
async fn an_address_no_capture_holds_is_refused_and_excused() -> TestResult<()> {
    const TEST: &str = concat!(
        module_path!(),
        "::an_address_no_capture_holds_is_refused_and_excused"
    );
    let p = prefix();
    let source = keeping(&p, "https://feed.example/items/", "GET");
    let (dir, end, denied) = replayed(TEST, &source, feed_captures()?).await?;
    let outcome = end?;
    let feed = &outcome.records["feed"];
    let refused = (feed.error.as_ref()).is_some_and(|error| error.message.contains(OUTSIDE));
    assert!(feed.status == TaskStatus::Skipped && refused, "{outcome:?}");
    assert_eq!(denied, DeniedEffects::default(), "excused: {denied:?}");
    assert!(!dir.path().join(&p).join("out/feed.txt").exists());
    Ok(())
}

#[tokio::test]
async fn a_post_is_refused_even_at_a_captured_address() -> TestResult<()> {
    const TEST: &str = concat!(
        module_path!(),
        "::a_post_is_refused_even_at_a_captured_address"
    );
    let p = prefix();
    let source = keeping(&p, FEED_URL, "POST");
    let (dir, end, denied) = replayed(TEST, &source, feed_captures()?).await?;
    let outcome = end?;
    let feed = &outcome.records["feed"];
    let refused = (feed.error.as_ref()).is_some_and(|error| error.message.contains(OUTSIDE));
    assert!(feed.status == TaskStatus::Skipped && refused, "{outcome:?}");
    assert_eq!(denied, DeniedEffects::default(), "excused: {denied:?}");
    assert!(!dir.path().join(&p).join("out/feed.txt").exists());
    Ok(())
}
