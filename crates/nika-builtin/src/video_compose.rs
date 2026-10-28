// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:video_compose` — native video assembly (stdlib §Media graduate
//! #6): still images and their narration become ONE MP4, cut from still to
//! still, with audio as the clock.
//!
//! This build carries the call's contract and no encoder. [`args`] parses
//! the resolved arguments against the bounds and closed sets the static
//! check judges literals with (`nika_cap::video`); [`plan`] holds the
//! integer timing, the keyframe placement and the budgets every backend
//! receives. The backend that prepares the raw samples and drives the host's
//! ffmpeg is not part of this build, so a call stops after its arguments are
//! judged: `-010`, before any read, scratch or write — never a fallback to
//! another program, and never a pretended artifact.

pub(crate) mod args;
pub(crate) mod plan;

use crate::{Args, BuiltinFailure, BuiltinOutcome};

/// Invalid arguments (shape · closed sets · bounds · a duration shorter
/// than its clip · a segment that would show no frame).
pub(crate) const C_ARGS: &str = "NIKA-BUILTIN-VIDEO_COMPOSE-001";
/// An unsupported input format (a clip's channels or rate · clips at
/// different rates · an `aac` rate outside the MPEG-4 table).
pub(crate) const C_FORMAT: &str = "NIKA-BUILTIN-VIDEO_COMPOSE-003";
/// A budget exceeded (segments · duration · the plan's intermediates · an
/// input's byte cap · a scaled box) — never transient.
pub(crate) const C_BUDGET: &str = "NIKA-BUILTIN-VIDEO_COMPOSE-005";
/// No usable encoder: this build carries no video backend.
pub(crate) const C_NO_ENCODER: &str = "NIKA-BUILTIN-VIDEO_COMPOSE-010";

/// Run the builtin: judge the arguments (`-001` · `-005`), then refuse
/// `-010` — nothing is read, staged or written.
///
/// # Errors
/// Every call fails: `-001`/`-005` for its arguments, else `-010`.
pub(crate) fn run(call: &Args) -> BuiltinOutcome {
    args::parse(call)?;
    Err(no_encoder())
}

/// The refusal of a build with no video backend: what is missing and what
/// the call did not do.
pub(crate) fn no_encoder() -> BuiltinFailure {
    BuiltinFailure::new(
        C_NO_ENCODER,
        "no usable encoder: this engine build carries no video backend, so the call stopped \
         after its arguments were judged — nothing was read, staged or written · fix: run the \
         workflow with an engine build that ships the video backend",
    )
}

#[cfg(test)]
mod tests;
