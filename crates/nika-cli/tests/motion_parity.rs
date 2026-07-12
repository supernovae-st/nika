// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as bin_smoke: a parity TEST's whole job is to die
// loudly naming the drift — expect/panic ARE its assertion voice.

//! The motion parity gate — `nika-display`'s spinner constants MIRROR
//! the vendored `design/motion.yaml` (spec #65 · the SSOT the site's
//! tiles and the VS Code canvas derive from). Hand-edited drift between
//! the terminal frames and the design system dies HERE, not in a
//! screenshot diff three surfaces later.

use nika_cli::display::theme::{ROUNDTRIP, SAMPLING, SCANLINE, SPINNER};

/// Pull one family's `frames:` list out of the vendored YAML — a line
/// parser, not a YAML dependency: the pack file is generated, its shape
/// is the projector's contract.
fn frames_of(family: &str) -> Vec<char> {
    let src = nika_pack::design_motion().expect("the pack carries design-motion.yaml");
    let fam = src
        .split("  families:")
        .nth(1)
        .and_then(|s| s.split(&format!("    {family}:")).nth(1))
        .unwrap_or_else(|| panic!("family `{family}` in design-motion.yaml"));
    let line = fam
        .lines()
        .find(|l| l.trim_start().starts_with("frames:"))
        .unwrap_or_else(|| panic!("frames: line for `{family}`"));
    line.split('[')
        .nth(1)
        .and_then(|s| s.split(']').next())
        .expect("frames list")
        .split(',')
        .map(|cell| {
            let cleaned = cell.trim().trim_matches('"');
            let mut chars = cleaned.chars();
            let c = chars.next().expect("one frame char");
            assert!(chars.next().is_none(), "1-cell law: {cleaned:?}");
            c
        })
        .collect()
}

/// Every verb family's terminal frames match the display constants —
/// the ONE motion vocabulary, engine-side (gate 1 of the motion SSOT).
#[test]
fn terminal_frames_mirror_the_design_motion_ssot() {
    assert_eq!(frames_of("infer"), SAMPLING.to_vec(), "infer · sampling");
    assert_eq!(frames_of("exec"), SCANLINE.to_vec(), "exec · scanline");
    assert_eq!(
        frames_of("invoke"),
        ROUNDTRIP.to_vec(),
        "invoke · roundtrip"
    );
    assert_eq!(frames_of("agent"), SPINNER.to_vec(), "agent · orbit");
}
