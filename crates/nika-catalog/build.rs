// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

// Build script: runs at build time, never shipped to users. The cargo
// protocol uses println!/eprintln! + process::exit — relaxed here.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::print_stderr,
    clippy::print_stdout
)]

//! Thin adapter over [`nika_catalog_codegen`] (D-2026-06-10-N3 · wire-
//! or-nuke resolved WIRE) · parses `data/*.toml` and emits one `.rs`
//! file per enabled catalog feature into `$OUT_DIR` for `include!()`
//! from `src/data/`. All parsing · validation · emission logic lives in
//! the unit-tested library crate — proven byte-identical to the
//! previous in-tree generator on the live catalog data before the
//! in-tree twin was deleted.

use std::env;
use std::path::{Path, PathBuf};
use std::process;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let data_dir = manifest_dir.join("data");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap_or_default());

    let features = nika_catalog_codegen::FeatureSet::from_env();
    match nika_catalog_codegen::generate(&data_dir, &out_dir, features) {
        Ok(emitted) => {
            for path in &emitted.rerun_paths {
                println!("cargo:rerun-if-changed={}", path.display());
            }
            println!("cargo:rerun-if-changed=build.rs");
            register_rerun_dir(&data_dir);
        }
        Err(err) => {
            eprintln!("nika-catalog build: {err}");
            process::exit(1);
        }
    }
}

/// Re-run when the data dir itself changes (a NEW toml appearing must
/// trigger a rebuild even though no registered file changed).
fn register_rerun_dir(data_dir: &Path) {
    println!("cargo:rerun-if-changed={}", data_dir.display());
}
