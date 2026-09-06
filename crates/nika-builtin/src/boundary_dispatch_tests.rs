// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Boundary enforcement through the REAL dispatcher route (see the module
//! declaration in `lib.rs` for what it proves).
//!
//! Moved out of `lib.rs` 2026-09-06 at 1568/1500 LOC — the two dispatcher
//! tests for the OBS-E `warning` lane crossed the file cap, and the cap is
//! the reason the module lives here now. `super` still resolves to the
//! crate root: semantics unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nika_fs::TokioFs;
use nika_kernel::runtime::tool_executor::{ToolCall, ToolExecuteDyn};
use nika_kernel_mock::{MockClock, MockHttp};

use super::{BuiltinDispatcher, FsBoundary, NoWorkflow, NonInteractive, NullEmitter};

type RealFsDispatcher =
    BuiltinDispatcher<TokioFs, MockHttp, MockClock, NullEmitter, NonInteractive, NoWorkflow>;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn scratch() -> std::path::PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("nika-builtin-bnd-{}-{n}", std::process::id()));
    std::fs::create_dir_all(root.join("allowed")).unwrap();
    std::fs::write(root.join("allowed/in.txt"), b"inside").unwrap();
    std::fs::write(root.join("secret.txt"), b"OUTSIDE").unwrap();
    root
}

fn dispatcher_with(boundary: FsBoundary) -> RealFsDispatcher {
    BuiltinDispatcher::new(
        Arc::new(TokioFs),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(NullEmitter),
        Arc::new(NonInteractive),
        Arc::new(NoWorkflow),
    )
    .with_fs_boundary(boundary)
}

#[tokio::test]
async fn write_under_declared_permit_creates_the_subdir() {
    // #433 option C · end-to-end through the dispatcher: a `nika:write`
    // to a NEW sub-directory inside the declared write permit succeeds
    // WITHOUT `create_dirs` — the permit for `allowed/**` is the intent.
    // (Pre-fix this returned NIKA-BUILTIN-WRITE-001 « parent does not
    // exist »; chart always auto-created — the disagreement #433 named.)
    let root = scratch();
    let boundary = FsBoundary::declared(vec![], vec![format!("{}/allowed/**", root.display())]);
    let dispatcher = dispatcher_with(boundary);
    let target = root.join("allowed/fresh/report.md");
    let result = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:write",
            serde_json::json!({
                "path": target.to_string_lossy(),
                "content": "# under a declared tree",
            }),
        ))
        .await
        .expect("dispatches");
    assert!(
        !result.is_error,
        "declared intent · no create_dirs: {}",
        result.content
    );
    assert!(
        target.exists(),
        "the file landed in the freshly-created subdir"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn write_to_new_subdir_without_permit_still_gates() {
    // The un-declared corner is UNCHANGED: no boundary → `nika:write`
    // keeps its safety default (a missing parent refuses without
    // `create_dirs`). Additive fix · nothing that gated stops gating.
    let root = scratch();
    let dispatcher = dispatcher_with(FsBoundary::unbounded());
    let target = root.join("nowhere/report.md");
    let result = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:write",
            serde_json::json!({
                "path": target.to_string_lossy(),
                "content": "x",
            }),
        ))
        .await
        .expect("dispatches");
    assert!(result.is_error, "no permit → the safety gate still fires");
    assert!(
        result.content.contains("create_dirs"),
        "the teach is intact: {}",
        result.content
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn write_traversal_is_refused_before_the_io() {
    let root = scratch();
    let boundary = FsBoundary::declared(vec![], vec![format!("{}/allowed/**", root.display())]);
    let dispatcher = dispatcher_with(boundary);
    let escape = root.join("allowed/../TRAVERSED.txt");
    let result = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:write",
            serde_json::json!({
                "path": escape.to_string_lossy(),
                "content": "pwn",
            }),
        ))
        .await
        .expect("dispatches");
    assert!(result.is_error, "the boundary must refuse the traversal");
    assert!(
        result.content.starts_with("NIKA-SEC-004"),
        "the permits-denied code: {}",
        result.content
    );
    // …and the I/O never happened (the file is not on disk OUTSIDE).
    assert!(
        !root.join("TRAVERSED.txt").exists(),
        "the gate is BEFORE the write — nothing escaped"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn read_inside_boundary_still_works_through_the_dispatcher() {
    let root = scratch();
    let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
    let dispatcher = dispatcher_with(boundary);
    let result = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:read",
            serde_json::json!({ "path": root.join("allowed/in.txt").to_string_lossy() }),
        ))
        .await
        .expect("dispatches");
    assert!(
        !result.is_error,
        "in-boundary read is allowed: {}",
        result.content
    );
    assert_eq!(result.content, "inside");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn read_traversal_to_a_real_outside_file_is_refused() {
    // The /etc/passwd-class leak: a `..` chain to a real file outside
    // the boundary must NOT be read (it would otherwise succeed).
    let root = scratch();
    let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
    let dispatcher = dispatcher_with(boundary);
    let leak = root.join("allowed/../secret.txt");
    let result = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:read",
            serde_json::json!({ "path": leak.to_string_lossy() }),
        ))
        .await
        .expect("dispatches");
    assert!(result.is_error, "the leak must be refused");
    assert!(
        result.content.starts_with("NIKA-SEC-004"),
        "{}",
        result.content
    );
    assert!(
        !result.content.contains("OUTSIDE"),
        "the outside file's bytes must never surface"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn grep_through_an_escaping_symlink_is_refused() {
    // P0 regression: `nika:grep` walks a directory, finds an IN-boundary
    // symlink leaf, then `read_to_string` FOLLOWS it. A link whose target
    // escapes the declared `permits.fs` must NOT leak that target's bytes
    // (the per-file sibling of `read`'s symlink-escape guard · grep was the
    // one fs builtin missing it · NIKA-SEC-004). Proven BOTH ways so the
    // test is not vacuous: bounded → skipped, unbounded → leaks (the walk
    // genuinely reaches and reads the link).
    let root = scratch();
    let link = root.join("allowed/leak");
    std::os::unix::fs::symlink(root.join("secret.txt"), &link).unwrap();
    let call = || {
        ToolCall::new(
            "t",
            "nika:grep",
            serde_json::json!({
                "pattern": "OUTSIDE",
                "path": root.join("allowed").to_string_lossy(),
            }),
        )
    };

    // Bounded to `allowed/**`: the escaping leaf is skipped like any
    // unreadable entry — the grep succeeds with zero hits, the secret's
    // bytes never surface.
    let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
    let bounded = dispatcher_with(boundary)
        .execute(call())
        .await
        .expect("dispatches");
    assert!(
        !bounded.is_error,
        "the in-boundary grep itself is fine: {}",
        bounded.content
    );
    assert!(
        !bounded.content.contains("OUTSIDE"),
        "the escaping symlink's target must never leak through grep: {}",
        bounded.content
    );

    // Unbounded (pre-permits floor): the SAME grep DOES follow the link and
    // surface the secret — the bypass the boundary closes.
    let leaked = dispatcher_with(FsBoundary::unbounded())
        .execute(call())
        .await
        .expect("dispatches");
    assert!(
        leaked.content.contains("OUTSIDE"),
        "without a boundary the link is followed (the bug the gate fixes): {}",
        leaked.content
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn grep_does_not_descend_an_escaping_dir_symlink() {
    // Defense-in-depth sibling of the leaf case: a symlink to a DIRECTORY
    // outside the boundary must not leak its contents either. TWO layers
    // stop it — (a) the kernel walk treats a symlinked dir as a
    // non-following leaf (never recurses · the property lives a crate away
    // in nika-fs), and (b) were it to recurse, the per-file guard would
    // refuse each escaped leaf. Run UNBOUNDED to isolate (a): if the walk
    // never follows even with no gate, the secret can never surface.
    let root = scratch();
    std::fs::create_dir_all(root.join("outside")).unwrap();
    std::fs::write(root.join("outside/loot.txt"), b"SECRET-LOOT").unwrap();
    std::os::unix::fs::symlink(root.join("outside"), root.join("allowed/dirlink")).unwrap();
    let result = dispatcher_with(FsBoundary::unbounded())
        .execute(ToolCall::new(
            "t",
            "nika:grep",
            serde_json::json!({
                "pattern": "SECRET-LOOT",
                "path": root.join("allowed").to_string_lossy(),
            }),
        ))
        .await
        .expect("dispatches");
    assert!(
        !result.content.contains("SECRET-LOOT"),
        "a symlinked dir outside the boundary must not be descended/leaked: {}",
        result.content
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn no_boundary_leaves_the_pre_permits_floor() {
    // With no boundary declared, the dispatcher does NOT gate fs ops
    // (today's behaviour) — a read outside "allowed" still works.
    let root = scratch();
    let dispatcher = dispatcher_with(FsBoundary::unbounded());
    let result = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:read",
            serde_json::json!({ "path": root.join("secret.txt").to_string_lossy() }),
        ))
        .await
        .expect("dispatches");
    assert!(
        !result.is_error,
        "no boundary → no gate: {}",
        result.content
    );
    assert_eq!(result.content, "OUTSIDE");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn absolute_glob_outside_the_boundary_is_refused() {
    // F2 security pin: now that an absolute pattern globs from its real
    // root (not silently `[]`), the boundary must gate THAT root — an
    // absolute glob aimed OUTSIDE the declared `permits.fs` must fail
    // before the walk, never silently leak the outside tree.
    let root = scratch();
    let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
    let dispatcher = dispatcher_with(boundary);
    // `<root>/**` walks `<root>` — NOT under `allowed/**` → refused.
    let outside = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:glob",
            serde_json::json!({ "pattern": format!("{}/**", root.display()) }),
        ))
        .await
        .expect("dispatches");
    assert!(outside.is_error, "an out-of-boundary glob must be refused");
    assert!(
        outside.content.starts_with("NIKA-SEC-004"),
        "{}",
        outside.content
    );
    // …while an absolute glob INSIDE the boundary still works.
    let inside = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:glob",
            serde_json::json!({ "pattern": format!("{}/allowed/**", root.display()) }),
        ))
        .await
        .expect("dispatches");
    assert!(
        !inside.is_error,
        "an in-boundary absolute glob is allowed: {}",
        inside.content
    );
    assert!(
        inside.content.contains("in.txt"),
        "the in-boundary file surfaces: {}",
        inside.content
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// V9 wave 3 · p10: a folder named like the pattern (`item-07.md`
/// under `*.md`) vanished from a 12-candidate batch with no signal
/// anywhere — 11 ran, the trace read `succeeded`. The VALUE stays the
/// file list (a consumer's contract, byte-identical); the result's
/// OBS-E `warning` names what the walk left out, and a tree with no
/// directory match says nothing (a clean success never grows one).
#[tokio::test]
async fn glob_names_the_directory_it_left_out_beside_an_unchanged_file_list() {
    let root = scratch();
    std::fs::create_dir_all(root.join("allowed/item-07.md")).unwrap();
    std::fs::write(root.join("allowed/item-06.md"), b"six").unwrap();
    std::fs::write(root.join("allowed/item-08.md"), b"eight").unwrap();
    let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
    let dispatcher = dispatcher_with(boundary);
    let pattern = format!("{}/allowed/*.md", root.display());
    let result = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:glob",
            serde_json::json!({ "pattern": pattern }),
        ))
        .await
        .expect("dispatches");
    assert!(!result.is_error, "{}", result.content);
    let files = result.structured.clone().expect("the file list");
    assert_eq!(
        files,
        serde_json::json!([
            format!("{}/allowed/item-06.md", root.display()),
            format!("{}/allowed/item-08.md", root.display()),
        ]),
        "the value a consumer receives is the file list, unchanged"
    );
    assert_eq!(result.content, files.to_string(), "the text plane agrees");
    let warning = result
        .warning
        .expect("the frame says what the walk left out");
    assert!(warning.contains("files only"), "{warning}");
    assert!(warning.contains("1 directory also matched"), "{warning}");
    assert!(
        warning.contains(&format!("{}/allowed/item-07.md", root.display())),
        "the left-out directory is named: {warning}"
    );
    // The control: no directory match → no warning at all.
    std::fs::remove_dir_all(root.join("allowed/item-07.md")).unwrap();
    let clean = dispatcher
        .execute(ToolCall::new(
            "t2",
            "nika:glob",
            serde_json::json!({ "pattern": pattern }),
        ))
        .await
        .expect("dispatches");
    assert!(!clean.is_error, "{}", clean.content);
    assert!(clean.warning.is_none(), "{:?}", clean.warning);
    let _ = std::fs::remove_dir_all(&root);
}

/// The deep shape: a `**` pattern descends the way the walker does
/// (hidden directories stay closed · the walker's own files are never
/// re-counted), names a nested directory match, and honours the
/// author's `exclude:` — a directory the author excluded is their
/// own drop, never a warning.
#[tokio::test]
async fn glob_descends_for_a_deep_pattern_and_honours_exclude() {
    let root = scratch();
    std::fs::create_dir_all(root.join("allowed/sub/x.md")).unwrap();
    std::fs::create_dir_all(root.join("allowed/.hidden/y.md")).unwrap();
    std::fs::create_dir_all(root.join("allowed/skip/z.md")).unwrap();
    std::fs::write(root.join("allowed/sub/real.md"), b"real").unwrap();
    let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
    let dispatcher = dispatcher_with(boundary);
    let pattern = format!("{}/allowed/**/*.md", root.display());
    let result = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:glob",
            serde_json::json!({ "pattern": pattern, "exclude": ["**/skip/**"] }),
        ))
        .await
        .expect("dispatches");
    assert!(!result.is_error, "{}", result.content);
    assert_eq!(
        result.structured.clone().expect("files"),
        serde_json::json!([format!("{}/allowed/sub/real.md", root.display())]),
        "only the real file is the value"
    );
    let warning = result.warning.expect("the nested directory is named");
    assert!(
        warning.contains(&format!("{}/allowed/sub/x.md", root.display())),
        "{warning}"
    );
    assert!(
        !warning.contains(".hidden"),
        "a hidden directory is not entered, as the walker does not: {warning}"
    );
    assert!(
        !warning.contains("skip/z.md"),
        "an excluded directory is the author's own drop: {warning}"
    );
    assert!(warning.contains("1 directory also matched"), "{warning}");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn write_to_an_existing_directory_is_taught_not_os_error_21() {
    // The measured half that reached the seam (Harness-Bench 005 ·
    // 0.118.7): the name exists AND is a directory. Through the REAL
    // dispatcher on a real tree, under a declared write permit — the
    // verdict teaches the file-inside form instead of the kernel's
    // `Is a directory (os error 21)`, and the directory is untouched.
    let root = scratch();
    let boundary = FsBoundary::declared(vec![], vec![format!("{}/allowed/**", root.display())]);
    let dispatcher = dispatcher_with(boundary);
    let target = root.join("allowed/replies");
    std::fs::create_dir(&target).unwrap();
    let result = dispatcher
        .execute(ToolCall::new(
            "t",
            "nika:write",
            serde_json::json!({ "path": target.to_string_lossy(), "content": "x" }),
        ))
        .await
        .expect("dispatches");
    assert!(
        result.is_error && result.content.starts_with("NIKA-BUILTIN-WRITE-001"),
        "a directory target refuses: {}",
        result.content
    );
    assert!(
        result.content.contains("names a directory")
            && result.content.contains("create_dirs: true"),
        "the teach reaches the model: {}",
        result.content
    );
    assert!(target.is_dir(), "the directory is untouched");
    let _ = std::fs::remove_dir_all(&root);
}
