// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a `nika:video_compose` call starts, in the plan's and the review's
//! words (stdlib §`nika:video_compose` · the host program).
//!
//! The line is STATIC: it names the program and where the engine looks for
//! it, never what this host has (a host-dependent line would break check ≡
//! run and the wasm checker). It says the program runs under the sandbox the
//! engine gives `exec:` tasks unless the operator waives it (the waiver is
//! the operator's, read at the run, so the line names it rather than
//! guessing), and never that it has no network: that holds only where the
//! host's jail denies loopback too.

use nika_cap::video::{FFMPEG, FFMPEG_CANDIDATES, FFMPEG_ENV, FFMPEG_FLOOR, H264_ENCODER};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_types::SandboxPolicy;

/// The program a `nika:video_compose` call starts, the places it is looked
/// for, the floor, and how it runs.
#[must_use]
pub(crate) fn host_encoder_clause() -> String {
    format!(
        "starts the host's {FFMPEG} itself ({FFMPEG_ENV}, else {} · FFmpeg {FFMPEG_FLOOR} or \
         later with {H264_ENCODER}) under the exec sandbox unless {}={} waives it · not exec:, \
         no permits.exec",
        FFMPEG_CANDIDATES.join(" · "),
        SandboxPolicy::ENV_VAR,
        SandboxPolicy::Off.as_str()
    )
}

/// Does `tool` start the host's encoder itself?
pub(crate) fn starts_encoder(tool: &str) -> bool {
    tool == nika_cap::video::TOOL
}

/// Does any task start the host's encoder itself?
pub(crate) fn starts_host_encoder(wf: &RawWorkflow) -> bool {
    wf.tasks.iter().any(|task| match &task.value.action {
        RawAction::Invoke(invoke) => match &invoke.target {
            RawInvokeTarget::Tool(tool) => starts_encoder(&tool.value),
            RawInvokeTarget::Workflow(_) => false,
        },
        _ => false,
    })
}
