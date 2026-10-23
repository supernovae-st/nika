// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A destination the candidate reads too is its own state (a journal it appends to): the room
//! holds it as the project has it, or as the absence of the first day, and the run is rehearsed
//! on either (live gauntlet wave 11, 2026-10-20: r05's and r08's journals were « read but not
//! observed », so nothing was rehearsed, and r10's first day failed at its first real run).
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;
use nika_onboard::compile::copy::{Allowance, Limits, Usage, native};
use nika_onboard::compile::rehearse::{Rehearsal, Rehearse as _};
use room_support::{World, final_text};

/// A journal read if it is there (nothing the first day), one line appended, written back.
fn appending(world: &World) -> String {
    let journal = world.path("journal.md");
    format!(
        r#"nika: journal-append
permits:
  tools: ["nika:read", "nika:write"]
  fs: {{ read: ["{journal}"], write: ["{journal}"] }}
tasks:
  previous:
    invoke: {{ tool: "nika:read", args: {{ path: "{journal}" }} }}
    on_error:
      on_codes: [NIKA-BUILTIN-READ-001]
      recover: ""
  save:
    with: {{ old: "${{{{ tasks.previous.output }}}}" }}
    invoke: {{ tool: "nika:write", args: {{ path: "{journal}", content: "${{{{ with.old }}}}- une ligne\n", overwrite: true }} }}
"#
    )
}

#[tokio::test]
async fn a_journal_the_candidate_appends_to_is_rehearsed_on_the_first_day_and_after() {
    for held in [None, Some("- ancienne\n")] {
        let world = World::new(&[]);
        if let Some(text) = held {
            world.put("journal.md", text);
        }
        let before = world.files();
        let candidate = appending(&world);
        let journal = world.path("journal.md");
        let (inputs, targets) = ([journal.clone()], [journal.clone()]);
        let limits = Limits::new(3, 3, 6 * 1024 * 1024, 30_000);
        let allowance = Allowance::new(limits, limits, Usage::default());
        let scoped = native::Scoped::new(world.project(), Box::new(world.room()), allowance);
        let report = scoped.rehearse_reading(&candidate, &inputs, &targets).await;
        assert!(
            matches!(report.outcome, Rehearsal::Passed { .. }),
            "{held:?}: {report:?}"
        );
        let expected = format!("{}- une ligne\n", held.unwrap_or_default());
        assert_eq!(
            final_text(&report, &journal),
            Some(expected.as_str()),
            "{report:?}"
        );
        let (_, preview) = scoped.finish().into_parts();
        let preview = preview.expect("an open account").expect("a live decision");
        let (witness, lines) = preview.into_parts();
        assert!(
            lines.starts_with("Rehearsed on a copy of your files"),
            "{lines}"
        );
        assert!(witness.is_some(), "the run binds its world");
        assert_eq!(world.files(), before, "the originals end as they began");
    }
}
