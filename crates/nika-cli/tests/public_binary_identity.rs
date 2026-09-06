// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// The workspace bans std::process::Command (production spawns ride the
// kernel ShellExecutor seam). This test's whole job is to ask cargo itself
// what the workspace's executables are named — the build contract, not the
// lib — so it spawns `cargo metadata` (the same carve-out class as
// bin_smoke.rs, which executes the real binary).
#![allow(clippy::disallowed_types)]

//! The public executable is born `nika` (ADR-135): one identity from
//! `cargo build --bin nika` to the user's prompt. Two readings, both cargo's
//! own: the `CARGO_BIN_EXE_nika` variable exists at compile time only if the
//! bin target carries that name (a rename cannot even compile this file), and
//! `cargo metadata` says there is exactly one bin target named `nika` in the
//! workspace, owned by `nika-cli`, which runs it by default. The packaging
//! half (the release line · the flake · the tests' variable) is
//! `scripts/ci/check-public-binary.sh`.

use std::path::Path;
use std::process::Command;

#[test]
fn the_executable_cargo_builds_is_named_nika() {
    let exe = Path::new(env!("CARGO_BIN_EXE_nika"));
    let stem = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("the built executable has a name");
    assert_eq!(
        stem,
        "nika",
        "the executable cargo built is {}",
        exe.display()
    );
    assert!(exe.is_file(), "{} exists", exe.display());
}

#[test]
fn cargo_metadata_names_exactly_one_public_executable() {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let meta: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("cargo metadata is JSON");
    let packages = meta["packages"].as_array().expect("packages");
    let mut named_nika = Vec::new();
    let mut named_nika_cli = Vec::new();
    for package in packages {
        let owner = package["name"].as_str().unwrap_or_default();
        for target in package["targets"].as_array().into_iter().flatten() {
            let is_bin = target["kind"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|kind| kind == "bin");
            if !is_bin {
                continue;
            }
            match target["name"].as_str() {
                Some("nika") => named_nika.push(owner.to_owned()),
                Some("nika-cli") => named_nika_cli.push(owner.to_owned()),
                _ => {}
            }
        }
    }
    assert_eq!(
        named_nika,
        vec!["nika-cli".to_owned()],
        "exactly one bin target named nika, owned by nika-cli"
    );
    assert!(
        named_nika_cli.is_empty(),
        "no bin target may be named nika-cli (found in {named_nika_cli:?})"
    );
    let cli = packages
        .iter()
        .find(|package| package["name"] == "nika-cli")
        .expect("nika-cli is a workspace member");
    assert_eq!(
        cli["default_run"].as_str(),
        Some("nika"),
        "nika-cli runs its one executable by default"
    );
}
