// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-cli` — the operator surface of the engine (L4 · ADMITTED 2026-06-21).
//!
//! Seeded 2026-06-11 with the **display fold**: `RunView` (a pure fold over
//! the real [`nika_event::Event`] stream) + `frame` rendering (spec
//! `docs/crate-specs/nika-cli.md` §3) + the deterministic demo storyboards.
//! The first-15-min verb suite (D-2026-06-10-N6) is here; the law every
//! addition obeys: **render = pure function of the event stream** — terminal,
//! `--json`, SSE and the DAG webview are four views of one truth.
//!
//! Status: ADMITTED — all 12 gates (crate-spec §11). The crate is `nika-cli`
//! but the binary's public name is `nika` (clap `#[command(name = "nika")]`);
//! the release renames the artifact to `nika`. The L5 composition root will
//! later own that name as a <500-LOC wrapper over this surface.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::unreachable)
)]

// The display surface DESCENDED to `nika-display` (2026-07-10 · the 15k
// wall · nika-dap/nika-cap precedent) — re-exported at the old paths so
// every `crate::display::…` / `crate::demo` call site stays untouched.
pub use nika_display as display;
pub use nika_display::demo;
// The agent-run contract, pinned against the in-repo kit source
// (RAMS-17 · the prose must match the structure guard already judges).
// The file rides the `tests.rs` name so the prod-LOC counter excludes it
// (the check/tests.rs idiom · a `*_tests.rs` basename counts as prod).
#[cfg(test)]
#[path = "agent_kit/tests.rs"]
mod agent_kit_tests;
pub mod anchor;
// The process's ONE current-directory lease (#1192) — every site that chdirs
// takes it, tests included. Documented in the module itself: an outer `///`
// here MERGES with the module's `//!` and rustdoc then resolves the merged
// text in THIS scope, where `hold` and `enter` do not exist.
pub(crate) mod cwd;
pub use nika_cli_host::metrics;
pub mod registry;
pub mod seal;
pub(crate) use nika_cli_host::text;
pub mod verbs;
pub mod wires;

pub use display::render::{frame, frame_with_outputs, verdict_frame};
pub use display::state::{RunView, TaskRow, TaskState};
pub use display::theme::{Role, Theme};

#[cfg(test)]
mod help_postcard_tests {
    use nika_cli_host::help_card::{human_help, permits_teaching};

    #[test]
    fn default_help_names_try_and_new() {
        let help = human_help();
        assert!(help.contains("try"), "C11 try: {help}");
        assert!(help.contains("new"), "C11 new: {help}");
    }

    #[test]
    fn default_help_glosses_permits() {
        let help = human_help();
        assert!(
            help.contains("what this file is allowed to touch"),
            "UX-2: {help}"
        );
    }

    #[test]
    fn default_help_documents_isolation() {
        let help = human_help();
        assert!(help.contains("env -i"), "B08 env -i: {help}");
        assert!(help.contains("HOME=$scratch"), "B08 HOME scratch: {help}");
    }

    #[test]
    fn default_help_does_not_invite_nika_permits_as_a_command() {
        let help = human_help();
        assert!(
            !help.lines().any(|l| l.starts_with("     permits")),
            "permits must not sit in the verb column: {help}"
        );
        assert!(
            !help
                .lines()
                .any(|l| l.trim_start().starts_with("nika permits")),
            "must not teach `nika permits` as a command: {help}"
        );
    }

    #[test]
    fn permits_teaching_names_the_file_doors() {
        let text = permits_teaching();
        assert!(text.contains("not a command"), "{text}");
        assert!(text.contains("lives in the file"), "{text}");
        assert!(text.contains("nika explain FILE"), "{text}");
        assert!(text.contains("nika check --infer-permits FILE"), "{text}");
    }
}

#[cfg(test)]
mod pause_card_tests {
    use crate::display::demo;
    use crate::display::render::frame;
    use crate::display::state::RunView;
    use crate::display::theme::Theme;

    #[test]
    fn pause_card_teaches_answer_boolean() {
        let mut view = RunView::new();
        for ev in demo::paused() {
            view.apply(&ev);
        }
        let joined = frame(&view, &Theme::new(false, false, false), 0).join("\n");
        assert!(
            joined.contains("--answer summarize=true"),
            "paused card must name --answer key=true: {joined}"
        );
        assert!(
            !joined.contains("=yes"),
            "boolean true/false, not yes: {joined}"
        );
    }
}
