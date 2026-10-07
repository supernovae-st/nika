// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:remove_file` proved over a private scratch directory (the real host
//! backend, every target inside the test's own directory): arguments and the
//! raw path shape refused before any effect; the write boundary witnessed
//! once, without a read grant and without creating a parent; the judged
//! regular-only removal; the ordinary failures as the tool's own code; an
//! authority refusal keeping its code even when it happens after the first
//! judgment; a backend without the override failing without I/O.
//!
//! Not proved here: a FIFO through the callable (the backend's own tests
//! cover it, this crate has no `mkfifo` seam), a substitution of the final
//! name between the backend's check and unlink, and any workflow-level
//! ordering, consent or composition law (Check and the runtime own those).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use bytes::Bytes;
use nika_fs::TokioFs;
use nika_kernel::fs::{FsError, FsReadDyn, FsWriteDyn};
use nika_kernel::runtime::tool_executor::{ToolCall, ToolExecuteDyn};
use nika_kernel_mock::{MockClock, MockHttp};

use crate::FsBoundary;

const BINARY: &[u8] = &[0x00, 0xff, 0x10, b'\n'];

/// A scratch directory the test alone owns: `allowed/` is the write bound,
/// `outside/` holds every target a link may name.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nika-remove-{}", uuid::Uuid::new_v4())); // seam-bypass-ok: unique test scratch name, not workflow entropy.
        std::fs::create_dir_all(root.join("allowed")).unwrap(); // seam-bypass-ok: test owns its real filesystem scratch.
        std::fs::create_dir_all(root.join("outside")).unwrap(); // seam-bypass-ok: test owns its real filesystem scratch.
        Self(root)
    }

    fn at(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    fn put(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = self.at(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap(); // seam-bypass-ok: test fixture.
        std::fs::write(&path, bytes).unwrap(); // seam-bypass-ok: test fixture.
        path
    }

    /// The write bound `allowed/**`, and nothing readable.
    fn write_bound(&self) -> FsBoundary {
        FsBoundary::declared(vec![], vec![format!("{}/allowed/**", self.0.display())])
    }

    /// Every name under the scratch with its bytes (a link's target as its
    /// bytes): what a refused removal must leave as it was.
    fn inventory(&self) -> Vec<(String, Vec<u8>)> {
        let mut found = Vec::new();
        let mut stack = vec![self.0.clone()];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir).unwrap(); // seam-bypass-ok: independent test observation.
            for entry in entries {
                let path = entry.unwrap().path();
                let kind = std::fs::symlink_metadata(&path).unwrap().file_type(); // seam-bypass-ok: independent test observation.
                let bytes = if kind.is_symlink() {
                    let target = std::fs::read_link(&path).unwrap(); // seam-bypass-ok: independent test observation.
                    target.into_os_string().into_encoded_bytes()
                } else if kind.is_dir() {
                    stack.push(path.clone());
                    b"<dir>".to_vec()
                } else {
                    std::fs::read(&path).unwrap() // seam-bypass-ok: independent test observation.
                };
                found.push((path.display().to_string(), bytes));
            }
        }
        found.sort();
        found
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0); // seam-bypass-ok: test owns its real filesystem scratch.
    }
}

type HostDispatcher = crate::BuiltinDispatcher<
    TokioFs,
    MockHttp,
    MockClock,
    crate::NullEmitter,
    crate::NonInteractive,
    crate::NoWorkflow,
>;

fn dispatcher(boundary: FsBoundary) -> HostDispatcher {
    crate::BuiltinDispatcher::new(
        Arc::new(TokioFs),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(crate::NullEmitter),
        Arc::new(crate::NonInteractive),
        Arc::new(crate::NoWorkflow),
    )
    .with_fs_boundary(boundary)
}

/// One call under a scoped witness: `(is_error, content, decisions)`.
async fn remove(
    dispatcher: &HostDispatcher,
    args: serde_json::Value,
) -> (bool, String, Vec<(String, String)>) {
    let witness = Arc::new(nika_cap::PermitWitness::new());
    let call = ToolCall::new("drop", "nika:remove_file", args);
    let result = crate::witness::scope_attempt_witness(witness.clone(), dispatcher.execute(call))
        .await
        .unwrap();
    let decisions = witness
        .take()
        .into_iter()
        .map(|d| (d.gate, d.decision.to_owned()))
        .collect();
    (result.is_error, result.content, decisions)
}

// ─── The permitted operation ────────────────────────────────────────────────

#[tokio::test]
async fn a_permitted_regular_file_is_removed_and_its_path_returned() {
    let scratch = Scratch::new();
    let victim = scratch.put("allowed/out/victim.bin", BINARY);
    scratch.put("allowed/out/neighbour.bin", BINARY);
    let mut expected = scratch.inventory();
    let (failed, content, decisions) = remove(
        &dispatcher(scratch.write_bound()),
        serde_json::json!({ "path": victim }),
    )
    .await;
    assert!(!failed, "{content}");
    assert!(content.contains(&victim.display().to_string()), "{content}");
    expected.retain(|(name, _)| *name != victim.display().to_string());
    assert_eq!(scratch.inventory(), expected, "only the victim is gone");
    assert_eq!(decisions.len(), 1, "one witnessed decision: {decisions:?}");
    assert!(
        decisions[0].0.starts_with("permits.fs.write "),
        "{decisions:?}"
    );
    assert_eq!(decisions[0].1, "allow");
}

#[tokio::test]
async fn an_absent_name_or_missing_parent_is_the_tools_failure_and_creates_nothing() {
    let scratch = Scratch::new();
    let before = scratch.inventory();
    let d = dispatcher(scratch.write_bound());
    for rel in ["allowed/gone.bin", "allowed/missing/deeper/gone.bin"] {
        let (failed, content, decisions) =
            remove(&d, serde_json::json!({ "path": scratch.at(rel) })).await;
        assert!(failed, "{rel}");
        assert!(
            content.starts_with("NIKA-BUILTIN-REMOVE_FILE-002"),
            "{rel}: {content}"
        );
        assert_eq!(decisions.len(), 1, "{rel}: the boundary was reached once");
    }
    assert_eq!(scratch.inventory(), before);
    assert!(!scratch.at("allowed/missing").exists(), "no parent created");
}

#[tokio::test]
async fn a_directory_or_a_link_inside_the_bound_is_the_tools_failure_and_stays() {
    let scratch = Scratch::new();
    scratch.put("allowed/dir/inner.bin", BINARY);
    let target = scratch.put("allowed/real.bin", BINARY);
    std::os::unix::fs::symlink(&target, scratch.at("allowed/link")).unwrap(); // seam-bypass-ok: test fixture.
    std::os::unix::fs::symlink(
        scratch.at("allowed/nothing"),
        scratch.at("allowed/dangling"),
    )
    .unwrap(); // seam-bypass-ok: test fixture.
    let before = scratch.inventory();
    let d = dispatcher(scratch.write_bound());
    for rel in ["allowed/dir", "allowed/link", "allowed/dangling"] {
        let (failed, content, _) = remove(&d, serde_json::json!({ "path": scratch.at(rel) })).await;
        assert!(failed, "{rel}");
        assert!(
            content.starts_with("NIKA-BUILTIN-REMOVE_FILE-002"),
            "{rel}: {content}"
        );
    }
    assert_eq!(scratch.inventory(), before, "no name and no target touched");
}

// ─── Authority ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_path_resolving_outside_the_write_bound_is_an_authority_refusal() {
    let scratch = Scratch::new();
    let outside = scratch.put("outside/victim.bin", BINARY);
    scratch.put("outside/dir/victim.bin", BINARY);
    std::os::unix::fs::symlink(&outside, scratch.at("allowed/escape")).unwrap(); // seam-bypass-ok: test fixture.
    std::os::unix::fs::symlink(scratch.at("outside/dir"), scratch.at("allowed/linkdir")).unwrap(); // seam-bypass-ok: test fixture.
    let before = scratch.inventory();
    let d = dispatcher(scratch.write_bound());
    let escapes = [
        outside.clone(),
        scratch.at("allowed/escape"),
        scratch.at("allowed/linkdir/victim.bin"),
        scratch.at("allowed/../outside/victim.bin"),
    ];
    for path in escapes {
        let (failed, content, decisions) = remove(&d, serde_json::json!({ "path": path })).await;
        assert!(failed, "{}", path.display());
        assert!(
            content.starts_with("NIKA-SEC-004"),
            "{}: {content}",
            path.display()
        );
        assert_eq!(decisions.len(), 1, "{}", path.display());
        assert_eq!(decisions[0].1, "deny");
    }
    assert_eq!(scratch.inventory(), before);
}

#[tokio::test]
async fn a_read_grant_never_authorizes_a_removal() {
    let scratch = Scratch::new();
    let victim = scratch.put("allowed/victim.bin", BINARY);
    let read_only =
        FsBoundary::declared(vec![format!("{}/allowed/**", scratch.0.display())], vec![]);
    let (failed, content, _) = remove(
        &dispatcher(read_only),
        serde_json::json!({ "path": victim }),
    )
    .await;
    assert!(failed);
    assert!(content.starts_with("NIKA-SEC-004"), "{content}");
    assert_eq!(std::fs::read(&victim).unwrap(), BINARY); // seam-bypass-ok: independent test observation.
}

// ─── Arguments and raw shape, before any effect ─────────────────────────────

#[tokio::test]
async fn invalid_resolved_arguments_are_refused_before_the_boundary() {
    let scratch = Scratch::new();
    let victim = scratch.put("allowed/victim.bin", BINARY);
    let shown = victim.display().to_string();
    let before = scratch.inventory();
    let d = dispatcher(scratch.write_bound());
    let cases = [
        serde_json::json!({}),
        serde_json::json!({ "path": 42 }),
        serde_json::json!({ "path": shown, "missing_ok": true }),
        serde_json::json!({ "path": "" }),
        serde_json::json!({ "path": format!("{shown}/") }),
        serde_json::json!({ "path": format!("{shown}/.") }),
        serde_json::json!({ "path": format!("{shown}/..") }),
        serde_json::json!({ "path": "/" }),
    ];
    for args in cases {
        let (failed, content, decisions) = remove(&d, args.clone()).await;
        assert!(failed, "{args}");
        assert!(
            content.starts_with("NIKA-BUILTIN-REMOVE_FILE-001"),
            "{args}: {content}"
        );
        assert!(decisions.is_empty(), "{args}: refused before the boundary");
    }
    assert_eq!(scratch.inventory(), before);
}

#[test]
fn the_runtime_and_static_shape_laws_agree_on_every_literal() {
    for raw in [
        "out/note",
        "./out/stale.txt",
        "out/./note",
        "out/part/../note",
        "out/...",
        " ",
        "out/?.txt",
        "out/*.txt",
        "out\\back.txt",
        "",
        "/",
        "out/sub/",
        "out/.",
        "out/..",
        ".",
        "..",
        "out//",
    ] {
        let runtime = super::removal_path_refusal(raw).is_some();
        let static_ = !nika_cap::builtin_shape_findings(
            "nika:remove_file",
            Some(&serde_json::json!({ "path": raw })),
        )
        .is_empty();
        assert_eq!(runtime, static_, "{raw:?}");
    }
}

// ─── A late authority refusal ───────────────────────────────────────────────

/// The host backend, except that the target's canonical form moves outside
/// the bound after the first judgment: the at-removal re-judgment denies what
/// the dispatch guard allowed.
struct MovesAfterJudgment {
    target: PathBuf,
    elsewhere: PathBuf,
    judged: AtomicUsize,
}

impl FsReadDyn for MovesAfterJudgment {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        TokioFs.read(path).await
    }

    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        TokioFs.read_to_string(path).await
    }

    async fn exists(&self, path: &Path) -> bool {
        TokioFs.exists(path).await
    }

    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        if path == self.target && self.judged.fetch_add(1, Ordering::SeqCst) > 0 {
            return Ok(self.elsewhere.clone());
        }
        TokioFs.canonicalize(path).await
    }
}

impl FsWriteDyn for MovesAfterJudgment {
    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        TokioFs.write(path, contents).await
    }

    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        TokioFs.create_dir_all(path).await
    }

    async fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        TokioFs.remove_file(path).await
    }

    async fn remove_regular_file(&self, path: &Path) -> Result<(), FsError> {
        TokioFs.remove_regular_file(path).await
    }
}

#[tokio::test]
async fn a_late_write_denial_keeps_its_authority_code() {
    let scratch = Scratch::new();
    let victim = scratch.put("allowed/victim.bin", BINARY);
    let fs = MovesAfterJudgment {
        target: victim.clone(),
        elsewhere: scratch.at("outside/victim.bin"),
        judged: AtomicUsize::new(0),
    };
    let boundary = scratch.write_bound();
    let args = serde_json::from_value(serde_json::json!({ "path": victim })).unwrap();
    let witness = Arc::new(nika_cap::PermitWitness::new());
    let result = crate::witness::scope_attempt_witness(
        witness.clone(),
        super::remove(&fs, &boundary, &args),
    )
    .await;
    let failure = result.expect_err("the late judgment refuses");
    assert_eq!(failure.code, "NIKA-SEC-004", "{}", failure.message);
    assert!(
        fs.judged.load(Ordering::SeqCst) >= 2,
        "the removal re-judged"
    );
    let decisions = witness.take();
    assert_eq!(decisions.len(), 1, "the confirmation is unwitnessed");
    assert_eq!(decisions[0].decision, "allow", "the first judgment allowed");
    assert_eq!(std::fs::read(&victim).unwrap(), BINARY); // seam-bypass-ok: independent test observation.
}

// ─── Backends and the judged view ───────────────────────────────────────────

/// A backend that predates the regular removal: only the required write
/// methods, counted. Its reads are the host's, for the boundary judgment.
#[derive(Default)]
struct Prior {
    writes: AtomicUsize,
    mkdirs: AtomicUsize,
    raw_removes: AtomicUsize,
}

impl FsReadDyn for Prior {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        TokioFs.read(path).await
    }

    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        TokioFs.read_to_string(path).await
    }

    async fn exists(&self, path: &Path) -> bool {
        TokioFs.exists(path).await
    }

    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        TokioFs.canonicalize(path).await
    }
}

impl FsWriteDyn for Prior {
    async fn write(&self, _: &Path, _: &[u8]) -> Result<(), FsError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn create_dir_all(&self, _: &Path) -> Result<(), FsError> {
        self.mkdirs.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn remove_file(&self, _: &Path) -> Result<(), FsError> {
        self.raw_removes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn a_backend_without_the_override_fails_without_any_io() {
    let scratch = Scratch::new();
    let victim = scratch.put("allowed/victim.bin", BINARY);
    let fs = Prior::default();
    let args = serde_json::from_value(serde_json::json!({ "path": victim })).unwrap();
    let failure = super::remove(&fs, &scratch.write_bound(), &args)
        .await
        .expect_err("the refusing default");
    assert_eq!(failure.code, "NIKA-BUILTIN-REMOVE_FILE-002");
    assert!(
        failure.message.contains("unsupported"),
        "{}",
        failure.message
    );
    let counts = [&fs.writes, &fs.mkdirs, &fs.raw_removes].map(|c| c.load(Ordering::SeqCst));
    assert_eq!(counts, [0, 0, 0], "never the raw removal, never a mkdir");
    assert_eq!(std::fs::read(&victim).unwrap(), BINARY); // seam-bypass-ok: independent test observation.
}

#[tokio::test]
async fn the_judged_view_reaches_the_override_and_keeps_the_raw_removal() {
    let scratch = Scratch::new();
    let victim = scratch.put("allowed/victim.bin", BINARY);
    let target = scratch.put("outside/target.bin", BINARY);
    let link = scratch.at("allowed/link");
    std::os::unix::fs::symlink(&target, &link).unwrap(); // seam-bypass-ok: test fixture.
    let boundary = scratch.write_bound();
    let judged = crate::judged_fs::JudgedFs::new(&TokioFs, &boundary);
    FsWriteDyn::remove_regular_file(&judged, &victim)
        .await
        .unwrap();
    assert!(!victim.exists(), "the backend override removed it");
    let refused = FsWriteDyn::remove_regular_file(&judged, &link).await;
    assert!(
        matches!(refused, Err(FsError::SymlinkRefused { .. })),
        "{refused:?}"
    );
    FsWriteDyn::remove_file(&judged, &link).await.unwrap();
    let unlinked = std::fs::symlink_metadata(&link).is_err(); // seam-bypass-ok: independent test observation.
    assert!(unlinked, "the raw removal unlinks the name");
    assert_eq!(std::fs::read(&target).unwrap(), BINARY, "never its target"); // seam-bypass-ok: independent test observation.
}

#[tokio::test]
async fn an_unbounded_embedder_removes_without_a_decision_to_witness() {
    let scratch = Scratch::new();
    let victim = scratch.put("allowed/victim.bin", BINARY);
    let (failed, content, decisions) = remove(
        &dispatcher(FsBoundary::unbounded()),
        serde_json::json!({ "path": victim }),
    )
    .await;
    assert!(!failed, "{content}");
    assert!(!victim.exists());
    assert!(decisions.is_empty(), "no boundary in force, no decision");
}
