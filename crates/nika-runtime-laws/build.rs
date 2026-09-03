// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::disallowed_types,
    clippy::print_stderr,
    clippy::print_stdout
)]

#[path = "../nika-runtime/build_support.rs"] // ADR-127 · one pin file, two members
mod build_support;

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=NIKA_BUILD_SHA");
    watch_git();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let pin_path = manifest.join("../../SPEC_PIN");
    let pack_path = manifest.join("../nika-pack/pack/SPEC_SHA");
    println!("cargo:rerun-if-changed={}", pin_path.display());
    println!("cargo:rerun-if-changed={}", pack_path.display());
    let pin = read(&pin_path);
    let pack = read(&pack_path);
    let spec_sha = match build_support::matching_spec_sha(&pin, &pack) {
        Ok(sha) => sha,
        Err(message) => fail(&message),
    };
    let build_sha = std::env::var("NIKA_BUILD_SHA")
        .ok()
        .map(|raw| raw.lines().next().unwrap_or_default().trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(git_stamp);
    let version = env!("CARGO_PKG_VERSION");
    let long = if build_sha == "unknown" {
        version.to_owned()
    } else {
        format!("{version} ({build_sha})")
    };
    println!("cargo:rustc-env=NIKA_BUILD_SHA={build_sha}");
    println!("cargo:rustc-env=NIKA_VERSION_LONG={long}");
    println!("cargo:rustc-env=NIKA_SPEC_SHA={spec_sha}");
}

fn read(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => fail(&format!("cannot read {}: {error}", path.display())),
    }
}

fn fail(message: &str) -> ! {
    eprintln!("nika-runtime build identity error: {message}");
    std::process::exit(1)
}

fn git_stamp() -> String {
    let Some(head) = git(&["rev-parse", "--short=9", "HEAD"]) else {
        return "unknown".to_owned();
    };
    let dirty = git(&["status", "--porcelain"]).is_some_and(|out| !out.is_empty());
    if dirty { format!("{head}-dirty") } else { head }
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).ok()?;
    let trimmed = stdout.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn watch_git() {
    for path in build_support::git_watch_paths(
        |name| git(&["rev-parse", "--git-path", name]).map(PathBuf::from),
        |path| std::fs::read_to_string(path).ok(),
    ) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
