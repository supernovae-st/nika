// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types)]

//! The public help does not turn an envelope override into an offline promise.
#[test]
fn run_help_names_the_retained_models_and_live_effects() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_nika"))
        .args(["run", "--help"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("NIKA_KEYCHAIN", "off")
        .output()
        .expect("CLI help");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout)
        .expect("UTF-8 help")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for claim in [
        "Override only this workflow's default `model:`",
        "Task `model:` pins and invoked child workflows keep their own models",
        "`mock/echo` does not make their calls or other effects offline",
    ] {
        assert!(text.contains(claim), "missing {claim:?}: {text}");
    }
}
