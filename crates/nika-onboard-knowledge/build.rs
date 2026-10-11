// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

// A build script speaks to cargo on stdout: `println!` is its protocol, not a log.
#![allow(clippy::disallowed_macros, clippy::print_stdout)]

//! The current knowledge release is embedded from its directory (`include_dir!`), which tracks an
//! edited file but not an added or removed one (measured on the spec pack, see nika-pack's build
//! script). A rerun-if-changed on the directory makes cargo scan it, additions and removals
//! included, so the build never serves an inventory its tree no longer holds.
fn main() {
    println!("cargo:rerun-if-changed=assets/knowledge-release-r2");
}
