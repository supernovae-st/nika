// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

// A build script speaks to cargo on stdout: `println!` is its protocol, not a log.
#![allow(clippy::disallowed_macros, clippy::print_stdout)]

//! The embedded pack follows its directory. `include_dir!` expands to one `include_bytes!` per
//! file that exists at compile time, so an edit to a file is tracked but a file ADDED to the
//! pack is not: the crate keeps its old embedding until something else rebuilds it. Measured
//! 2026-09-22 (push #17's gate): the authoring card, a new page under `pack/stdlib/`, was
//! absent from a target dir that had built the crate before the page existed; the same test
//! passed after `cargo clean -p nika-pack`. A rerun-if-changed on the directory makes cargo
//! scan it — additions and removals included.
fn main() {
    println!("cargo:rerun-if-changed=pack");
}
