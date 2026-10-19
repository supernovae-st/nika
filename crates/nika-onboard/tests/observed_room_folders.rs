// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A room holds the folders and patterns a request names: a folder is copied with each file
//! under it, an empty one made empty, a pattern with each file it matches, and the candidate lists
//! and removes over that copy only (live gauntlet r05, 2026-10-17: « inbox/ » named a folder the
//! room refused as no regular file, and `nika:glob` was outside its surface, so nothing was
//! rehearsed and the first run with an empty inbox failed). A listing no observed input holds is
//! refused before any room, as a read is.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;
use nika_onboard::compile::rehearse::Rehearsal;
use room_support::{World, completed, final_text, not_run_because, rehearsed};

/// A count of the inbox's texts, written, then each text removed from the room's copy.
fn sorting(world: &World) -> String {
    let (inbox, out) = (world.path("inbox"), world.path("out/count.txt"));
    format!(
        r#"nika: sorting
permits:
  tools: ["nika:glob", "nika:write", "nika:remove_file"]
  fs: {{ read: ["{inbox}/**"], write: ["{out}", "{inbox}/**"] }}
tasks:
  listed:
    invoke: {{ tool: "nika:glob", args: {{ pattern: "{inbox}/*.txt" }} }}
  count:
    with: {{ files: "${{{{ tasks.listed.output }}}}" }}
    invoke: {{ tool: "nika:write", args: {{ path: "{out}", content: "files: ${{{{ size(with.files) }}}}", overwrite: true, create_dirs: true }} }}
  remove:
    after: {{ count: success }}
    with: {{ files: "${{{{ tasks.listed.output }}}}" }}
    for_each: {{ items: "${{{{ with.files }}}}" }}
    invoke: {{ tool: "nika:remove_file", args: {{ path: "${{{{ item }}}}" }} }}
"#
    )
}

/// A world whose inbox holds `files`, the originals' bytes before any room.
fn inbox(files: &[(&str, &str)]) -> World {
    let world = World::new(files);
    std::fs::create_dir_all(world.project_path("inbox")).unwrap();
    world
}

#[tokio::test]
async fn a_folder_the_request_names_is_rehearsed_with_its_files() {
    const TEST: &str = concat!(module_path!(), "::a_folder_the_request_names_is_rehearsed");
    let world = inbox(&[("inbox/a.txt", "a\n"), ("inbox/b.txt", "b\n")]);
    let before = world.files();
    let candidate = sorting(&world);
    let inputs = [world.path("inbox/")];
    let report = rehearsed(TEST, "folder", &world.room(), &candidate, &inputs, &[]).await;
    assert!(
        matches!(report.outcome, Rehearsal::Passed { .. }),
        "{report:?}"
    );
    assert!(completed(&report), "{report:?}");
    let out = world.path("out/count.txt");
    assert_eq!(final_text(&report, &out), Some("files: 2"), "{report:?}");
    assert_eq!(world.files(), before, "the originals end as they began");
}

/// An empty folder is a day the workflow must survive: the room makes it, and the run lists
/// nothing in it.
#[tokio::test]
async fn an_empty_folder_is_rehearsed_as_the_empty_day() {
    const TEST: &str = concat!(module_path!(), "::an_empty_folder_is_rehearsed");
    let world = inbox(&[]);
    let candidate = sorting(&world);
    let inputs = [world.path("inbox/")];
    let report = rehearsed(TEST, "empty", &world.room(), &candidate, &inputs, &[]).await;
    assert!(
        matches!(report.outcome, Rehearsal::Passed { .. }),
        "{report:?}"
    );
    let out = world.path("out/count.txt");
    assert_eq!(final_text(&report, &out), Some("files: 0"), "{report:?}");
}

/// A pattern the request names is copied by its matches, and nothing else of its folder.
#[tokio::test]
async fn a_pattern_the_request_names_is_copied_by_its_matches() {
    const TEST: &str = concat!(module_path!(), "::a_pattern_the_request_names_is_copied");
    let world = inbox(&[
        ("inbox/a.txt", "a\n"),
        ("inbox/b.txt", "b\n"),
        ("inbox/notes.md", "not a text\n"),
    ]);
    let candidate = sorting(&world);
    let inputs = [world.path("inbox/*.txt")];
    let report = rehearsed(TEST, "pattern", &world.room(), &candidate, &inputs, &[]).await;
    assert!(
        matches!(report.outcome, Rehearsal::Passed { .. }),
        "{report:?}"
    );
    let out = world.path("out/count.txt");
    assert_eq!(final_text(&report, &out), Some("files: 2"), "{report:?}");
    let copied: Vec<&str> = (report.observation.copies.iter())
        .map(|copy| copy.path.as_str())
        .collect();
    assert_eq!(copied.len(), 2, "{copied:?}");
}

/// A listing no observed input holds is refused before any room, as a read is.
#[tokio::test]
async fn a_listing_no_observed_input_holds_is_refused() {
    const TEST: &str = concat!(module_path!(), "::a_listing_no_observed_input_holds");
    let world = inbox(&[("inbox/a.txt", "a\n"), ("other.txt", "other\n")]);
    let candidate = sorting(&world);
    let inputs = [world.path("other.txt")];
    let report = rehearsed(TEST, "unheld", &world.room(), &candidate, &inputs, &[]).await;
    assert!(not_run_because(&report, "not observed"), "{report:?}");
}
