// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The passive front door over host-prepared views. The expected screen is the
//! byte-exact text the pre-move host renderer printed for the
//! same machine; it is never produced by the renderer under test.

use super::*;
use crate::theme::Theme;

/// What the host prepares for its synthetic test machine: two editors (one
/// unwired), one keyless engine, a configured key, nothing pulled or drifted.
fn view() -> MachineView {
    MachineView {
        version: "0.0.0-test".to_owned(),
        clients: vec![("cursor".to_owned(), true), ("vscode".to_owned(), false)],
        wire_hint: Some("→ nika wire vscode".to_owned()),
        local_providers: 1,
        endpoints: vec![],
        model_count: 0,
        model_size: "0 B".to_owned(),
        state_metric: "key present · 1 of 2 clouds configured".to_owned(),
        state_cta: "ready for a real run",
        drifted_kits: vec![],
        wired_facet: "3 wired in this build (1 local · 2 cloud · plus mock)".to_owned(),
    }
}

fn counts() -> EngineCounts {
    EngineCounts {
        builtins: 7,
        locals: 1,
        clouds: 2,
        examples: 3,
        templates: 2,
    }
}

/// The pre-move case `synthetic/empty/legacy/plain/raw`.
const STRANGER: &str = "🦋 nika 0.0.0-test — Intent as Code. The workflow language for AI.\n   one file · 4 verbs · one binary · audited BEFORE it runs\n   every run records a tamper-evident, hash-chained trace\n\nthis machine\n  editors    cursor ✓ · vscode ✗\n             → nika wire vscode\n  local      1 keyless engines supported · none probed → nika doctor --ping\n  state      key present · 1 of 2 clouds configured — ready for a real run\n  workspace  git ✗ · no workflows yet · agents not briefed → nika init\n\nthis binary\n  4 verbs · 7 builtins · 3 providers · 3 examples · 2 templates\n  3 wired in this build (1 local · 2 cloud · plus mock)\n\na whole workflow is one file\n  nika: hello\n  model: mock/echo\n  tasks:\n    greet:\n      infer: { prompt: \"say hello to the operator\", max_tokens: 50 }\n\nlearn: nika.sh · docs: docs.nika.sh · ⭐ github.com/supernovae-st/nika\n";

#[test]
fn a_prepared_view_renders_the_pre_move_screen() {
    let empty = Glance {
        git: false,
        workflows: 0,
        agents_md: false,
        complete: true,
    };
    let text = render_with_context(
        &view(),
        empty,
        counts(),
        &ContextView::legacy(),
        Theme::new(false, false, false),
    );
    assert_eq!(text, STRANGER);
}
