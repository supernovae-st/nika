// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

// Build script: runs at build time, never shipped to users. The cargo
// protocol uses println!/eprintln! + process::exit — relaxed here.
// `std::process::Command` is likewise relaxed: the workspace ban points
// at the nika-kernel ShellExecutor trait, which a build script cannot
// ride (the kernel is a crate of the very workspace being built, and a
// build-dependency on it would invert the layering) — spawning `git`
// here IS the build's own machinery, not engine runtime I/O.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::disallowed_types,
    clippy::print_stderr,
    clippy::print_stdout
)]

//! Build provenance (#774): stamp the binary with the commit it was
//! built from, so a between-tags source build can never again read as
//! the tagged release it merely shares a `version =` with — the skew
//! that made `check --infer-permits` look machine-dependent.
//!
//! Resolution order, NEVER a build failure:
//! 1. the `NIKA_BUILD_SHA` env override (a packager pinning a tarball
//!    build — `cargo package` strips `.git`),
//! 2. `git rev-parse --short=9 HEAD` + `git status --porcelain`
//!    (local calls, no network) → `<sha>` or `<sha>-dirty`,
//! 3. the string `unknown` → `--version` prints the bare version.
//!
//! Emits two compile-time env vars for every target of this crate:
//! `NIKA_BUILD_SHA` (the stamp) and `NIKA_VERSION_LONG` (the composed
//! `<version> (<sha>[-dirty])`, or the bare version when unknown).

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=NIKA_BUILD_SHA");
    watch_git();
    let sha = std::env::var("NIKA_BUILD_SHA")
        .ok()
        .map(|raw| raw.lines().next().unwrap_or_default().trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(git_stamp);
    let version = env!("CARGO_PKG_VERSION");
    let long = if sha == "unknown" {
        version.to_owned()
    } else {
        format!("{version} ({sha})")
    };
    println!("cargo:rustc-env=NIKA_BUILD_SHA={sha}");
    println!("cargo:rustc-env=NIKA_VERSION_LONG={long}");
}

/// `<short-sha>` · `<short-sha>-dirty` · `unknown` when git cannot say
/// (no binary · no `.git` · an unreadable answer — provenance is a
/// stamp, never a build error).
fn git_stamp() -> String {
    let Some(head) = git(&["rev-parse", "--short=9", "HEAD"]) else {
        return "unknown".to_owned();
    };
    let dirty = git(&["status", "--porcelain"]).is_some_and(|out| !out.is_empty());
    if dirty { format!("{head}-dirty") } else { head }
}

/// One local git call → its trimmed stdout, `None` on any refusal.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).ok()?;
    let trimmed = stdout.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Re-stamp when the commit moves: HEAD (a checkout rewrites it) and
/// the loose ref it names (a commit advances it) — existing files only,
/// a missing watch would rerun the script on every build. A gitless
/// tree (crates.io · tarball) emits nothing and keeps cargo's
/// whole-package scan default. The dirty half can lag between commits
/// (the worktree is not watchable for free); the sha — the half #774
/// needs — is always exact at stamp time.
fn watch_git() {
    let Some(gitdir) = git(&["rev-parse", "--absolute-git-dir"]) else {
        return;
    };
    let root = std::path::Path::new(&gitdir);
    let head = root.join("HEAD");
    println!("cargo:rerun-if-changed={}", head.display());
    let Ok(target) = std::fs::read_to_string(&head) else {
        return;
    };
    let Some(reference) = target.trim().strip_prefix("ref: ") else {
        return; // detached HEAD: the HEAD watch above covers the move.
    };
    let loose = root.join(reference);
    if loose.exists() {
        println!("cargo:rerun-if-changed={}", loose.display());
    }
}
