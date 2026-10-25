// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::disallowed_types)]

//! The public help does not turn an envelope override into an offline promise.

/// The help a verb prints, whitespace-folded (clap wraps by terminal width).
fn help(verb: &str) -> String {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_nika"))
        .args([verb, "--help"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("NIKA_KEYCHAIN", "off")
        .output()
        .expect("CLI help");
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .expect("UTF-8 help")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn run_help_names_the_retained_models_and_live_effects() {
    let text = help("run");
    for claim in [
        "Override only this workflow's default `model:`",
        "Task `model:` pins and invoked child workflows keep their own models",
        "`mock/echo` does not make their calls or other effects offline",
    ] {
        assert!(text.contains(claim), "missing {claim:?}: {text}");
    }
}

/// #1605 · the try rehearsal replaces the model, not the network:
/// `nika try snippets/use-a-tool` GETs nika.sh on the default seat, so the
/// help names the mock model and the fetch, never « offline ».
#[test]
fn try_help_names_the_mock_model_and_the_fetch_it_keeps() {
    let text = help("try");
    for claim in [
        "on the mock model by default (zero keys · no model call",
        "a step that fetches still reaches its host",
    ] {
        assert!(text.contains(claim), "missing {claim:?}: {text}");
    }
    assert!(
        !text.to_lowercase().contains("offline"),
        "the try help promises no offline rehearsal: {text}"
    );
}
