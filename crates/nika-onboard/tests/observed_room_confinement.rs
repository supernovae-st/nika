// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A rehearsal reads and writes its own room and nothing else. A decoy under the process working
//! directory holds the same relative paths with other bytes: a read that resolves against the
//! working directory instead of the room would see the decoy's rows, and a write would land in
//! it. After every rehearsal, whatever its outcome, the world (the project, the secret beside it
//! and the scratch parent) and the decoy are byte for byte what they were: the originals are
//! never written, nothing lands outside the room, and the room itself is gone. The candidate is
//! admitted from its bytes over an empty namespace of its own: a data file sharing its logical
//! name keeps its own bytes, and no observed file is ever read as part of the admitted world.
//! The room needs only read handles on the original: a write-protected original still
//! rehearses. Every run here is a copy of files; no jq or convert step runs in a rehearsal.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;
use nika_onboard::compile::rehearse::Rehearsal;
use nika_onboard::compile::room::ObservedRoom;
use room_support::{
    DECOY_SALES, MCP_SERVERS, SALES, World, admitted_digest_of, bound_to, completed,
    computed_target, copy_file, copy_granted, copy_raw, mentions_mcp, never_attempted,
    not_run_because, published, reads_the_logical_name, rehearsed, sales_input, without_permits,
    write_only,
};

#[test]
fn the_fixtures_are_well_formed_before_any_verdict() {
    // The harness guard: a failure here makes the tests using that fixture harness-invalid.
    room_support::assert_catalog_is_well_formed(&World::new(&[("data/sales.csv", SALES)]));
}

/// The copy of the world's sales file to `out/copy.txt`.
fn sales_copy(world: &World) -> String {
    copy_file(world, "data/sales.csv", "out/copy.txt")
}

/// The text of the first output of a passed run.
fn first_text(report: &nika_onboard::compile::rehearse::RehearsalReport) -> Option<&str> {
    match &report.outcome {
        Rehearsal::Passed { outputs } => outputs.first().map(|output| output.text.as_str()),
        _ => None,
    }
}

#[tokio::test]
async fn reads_come_from_the_room_copy_never_from_a_decoy_working_directory() {
    const TEST: &str = concat!(
        module_path!(),
        "::reads_come_from_the_room_copy_never_from_a_decoy_working_directory"
    );
    let world = World::new(&[("data/sales.csv", SALES)]);
    let decoy = world.decoy(&[("data/sales.csv", DECOY_SALES)]);
    let (before, decoy_before) = (world.files(), decoy.files());
    let source = sales_copy(&world);
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &sales_input(&world),
        &[],
    )
    .await;
    assert_eq!(first_text(&report), Some(SALES), "{report:?}");
    assert!(completed(&report) && report.effects.is_none(), "{report:?}");
    assert!(report.room.prepared && report.room.cleaned, "{report:?}");
    assert_eq!(
        report.room.late_refused, 0,
        "nothing outlived its phase: {report:?}"
    );
    assert!(bound_to(&report, &source), "{report:?}");
    assert_eq!(world.files(), before, "the world is as it was");
    assert_eq!(decoy.files(), decoy_before, "the decoy is as it was");
}

#[tokio::test]
async fn a_write_only_candidate_writes_only_in_its_room() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_write_only_candidate_writes_only_in_its_room"
    );
    let world = World::new(&[]);
    let decoy = world.decoy(&[("keep.txt", "decoy")]);
    let (before, decoy_before) = (world.files(), decoy.files());
    let report = rehearsed(TEST, "only", &world.room(), &write_only(&world), &[], &[]).await;
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs.len() == 1 && outputs[0].text == "hello" && outputs[0].written),
        "{report:?}"
    );
    assert!(report.effects.is_none(), "{report:?}");
    assert_eq!(world.files(), before);
    assert_eq!(
        decoy.files(),
        decoy_before,
        "no out/ appears in the working directory"
    );
}

#[tokio::test]
async fn two_rooms_at_once_each_read_their_own_inputs() {
    const TEST: &str = concat!(
        module_path!(),
        "::two_rooms_at_once_each_read_their_own_inputs"
    );
    let prefix = room_support::unique_prefix();
    let left = World::with_prefix(&prefix, &[("data/sales.csv", SALES)]);
    let right = World::with_prefix(&prefix, &[("data/sales.csv", DECOY_SALES)]);
    let source = sales_copy(&left);
    let (left_room, right_room) = (left.room(), right.room());
    let inputs = sales_input(&left);
    let (a, b) = tokio::join!(
        rehearsed(TEST, "left", &left_room, &source, &inputs, &[]),
        rehearsed(TEST, "right", &right_room, &source, &inputs, &[])
    );
    assert_eq!(first_text(&a), Some(SALES), "{a:?}");
    assert_eq!(first_text(&b), Some(DECOY_SALES), "{b:?}");
}

#[tokio::test]
async fn an_absolute_target_computed_at_run_time_never_escapes() {
    const TEST: &str = concat!(
        module_path!(),
        "::an_absolute_target_computed_at_run_time_never_escapes"
    );
    let world = World::new(&[("data/sales.csv", SALES)]);
    let (before, target) = (world.files(), world.base().join("escape.json"));
    let source = computed_target(&world, &target.display().to_string());
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(
        matches!(&report.outcome, Rehearsal::Failed { task, .. } if task == "write_output"),
        "the room refuses the write, the task fails: {report:?}"
    );
    assert!(
        completed(&report) && published(&report).is_empty(),
        "{report:?}"
    );
    assert!(!target.exists());
    assert_eq!(world.files(), before);
}

#[tokio::test]
async fn a_traversal_target_computed_at_run_time_never_escapes() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_traversal_target_computed_at_run_time_never_escapes"
    );
    let world = World::new(&[("data/sales.csv", SALES)]);
    let before = world.files();
    let source = computed_target(&world, "./out/../../../../escape.json");
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(
        matches!(&report.outcome, Rehearsal::Failed { task, .. } if task == "write_output"),
        "{report:?}"
    );
    assert!(published(&report).is_empty(), "{report:?}");
    assert_eq!(world.files(), before, "nothing escaped, the room is gone");
}

#[cfg(unix)]
#[tokio::test]
async fn the_original_input_is_read_only_to_the_room() {
    const TEST: &str = concat!(
        module_path!(),
        "::the_original_input_is_read_only_to_the_room"
    );
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let world = World::new(&[("data/sales.csv", SALES)]);
    let original = world.project_path("data/sales.csv");
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o444)).unwrap();
    let (before, inode) = (world.files(), std::fs::metadata(&original).unwrap().ino());
    // The run overwrites its own input path: in the room, never in the project.
    let sales = world.path("data/sales.csv");
    let source = copy_raw(&sales, &sales);
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs.len() == 1 && outputs[0].path == sales && outputs[0].written),
        "{report:?}"
    );
    assert_eq!(world.files(), before);
    let meta = std::fs::metadata(&original).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o444);
    assert_eq!(
        meta.ino(),
        inode,
        "the original is the same file, not a replacement"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_input_file_is_never_copied() {
    const TEST: &str = concat!(module_path!(), "::a_symlinked_input_file_is_never_copied");
    let world = World::new(&[]);
    std::fs::write(world.base().join("outside.csv"), SALES).unwrap();
    std::fs::create_dir_all(world.project_path("data")).unwrap();
    std::os::unix::fs::symlink(
        world.base().join("outside.csv"),
        world.project_path("data/sales.csv"),
    )
    .unwrap();
    let before = world.files();
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &sales_copy(&world),
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(not_run_because(&report, "symlink"), "{report:?}");
    assert!(
        never_attempted(&report) && report.room.cleaned,
        "{report:?}"
    );
    assert_eq!(world.files(), before);
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlinked_directory_component_is_never_followed() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_symlinked_directory_component_is_never_followed"
    );
    let world = World::new(&[]);
    std::fs::create_dir_all(world.base().join("elsewhere")).unwrap();
    std::fs::write(world.base().join("elsewhere/sales.csv"), SALES).unwrap();
    std::os::unix::fs::symlink(world.base().join("elsewhere"), world.project_path("data")).unwrap();
    let before = world.files();
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &sales_copy(&world),
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(not_run_because(&report, "symlink"), "{report:?}");
    assert!(
        never_attempted(&report) && report.room.cleaned,
        "{report:?}"
    );
    assert_eq!(world.files(), before);
}

#[cfg(unix)]
#[tokio::test]
async fn a_special_file_input_is_never_copied() {
    const TEST: &str = concat!(module_path!(), "::a_special_file_input_is_never_copied");
    let world = World::new(&[]);
    std::fs::create_dir_all(world.project_path("data")).unwrap();
    // A socket at the input path: opening it must neither block nor be copied.
    let _socket =
        std::os::unix::net::UnixListener::bind(world.project_path("data/sales.csv")).unwrap();
    let before = world.files();
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &sales_copy(&world),
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(not_run_because(&report, "regular file"), "{report:?}");
    assert!(
        never_attempted(&report) && report.room.cleaned,
        "{report:?}"
    );
    assert_eq!(world.files(), before);
}

#[tokio::test]
async fn an_input_nobody_observed_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::an_input_nobody_observed_is_never_run");
    let world = World::new(&[("data/sales.csv", SALES)]);
    let before = world.files();
    let report = rehearsed(TEST, "only", &world.room(), &sales_copy(&world), &[], &[]).await;
    assert!(not_run_because(&report, "not observed"), "{report:?}");
    assert!(
        never_attempted(&report) && !report.room.prepared,
        "{report:?}"
    );
    assert_eq!(world.files(), before);
}

#[tokio::test]
async fn an_admission_refusal_after_preparation_reports_its_phase_and_cleanup() {
    const TEST: &str = concat!(
        module_path!(),
        "::an_admission_refusal_after_preparation_reports_its_phase_and_cleanup"
    );
    // No permits block: zero authority. The room is prepared, admission refuses, no run
    // begins, and cleanup is verified: the report states each fact.
    let world = World::new(&[("data/sales.csv", SALES)]);
    let before = world.files();
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &without_permits(&world),
        &[],
        &[],
    )
    .await;
    assert!(not_run_because(&report, "admission"), "{report:?}");
    assert!(never_attempted(&report), "no run began: {report:?}");
    assert!(
        report.room.prepared && report.room.cleaned,
        "prepared, then cleaned: {report:?}"
    );
    assert_eq!(world.files(), before);
}

#[tokio::test]
async fn a_broad_glob_grant_still_reads_and_writes_only_in_the_room() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_broad_glob_grant_still_reads_and_writes_only_in_the_room"
    );
    let world = World::new(&[("data/sales.csv", SALES)]);
    let decoy = world.decoy(&[("data/sales.csv", DECOY_SALES)]);
    let (before, decoy_before) = (world.files(), decoy.files());
    let source = copy_granted(
        &world.path("data/sales.csv"),
        &world.path("out/top.csv"),
        "**",
        "**",
    );
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &sales_input(&world),
        &[],
    )
    .await;
    assert_eq!(first_text(&report), Some(SALES), "{report:?}");
    assert_eq!(world.files(), before);
    assert_eq!(decoy.files(), decoy_before);
}

#[tokio::test]
async fn a_literal_write_outside_the_room_is_never_run() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_literal_write_outside_the_room_is_never_run"
    );
    let world = World::new(&[("data/sales.csv", SALES)]);
    let before = world.files();
    let source = copy_raw(&world.path("data/sales.csv"), "../escape.json");
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(not_run_because(&report, "outside the room"), "{report:?}");
    assert!(
        never_attempted(&report) && !report.room.prepared,
        "{report:?}"
    );
    assert_eq!(world.files(), before, "no escape.json anywhere");
}

#[tokio::test]
async fn a_literal_read_outside_the_room_is_never_run() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_literal_read_outside_the_room_is_never_run"
    );
    let world = World::new(&[("data/sales.csv", SALES)]);
    let before = world.files();
    let source = copy_raw("../secret.txt", &world.path("out/top.json"));
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &["../secret.txt".to_owned()],
        &[],
    )
    .await;
    assert!(not_run_because(&report, "outside the room"), "{report:?}");
    assert!(!format!("{report:?}").contains("TOP-SECRET"), "{report:?}");
    assert_eq!(world.files(), before);
}

#[tokio::test]
async fn an_absolute_literal_path_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::an_absolute_literal_path_is_never_run");
    let world = World::new(&[("data/sales.csv", SALES)]);
    let before = world.files();
    let target = world.base().join("absolute.json");
    let source = copy_raw(
        &world.path("data/sales.csv"),
        target.to_string_lossy().as_ref(),
    );
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(not_run_because(&report, "absolute path"), "{report:?}");
    assert!(never_attempted(&report), "{report:?}");
    assert_eq!(world.files(), before, "no absolute.json");
}

#[tokio::test]
async fn a_data_file_sharing_the_logical_name_keeps_its_own_bytes() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_data_file_sharing_the_logical_name_keeps_its_own_bytes"
    );
    let world = World::new(&[]);
    world.put_at_root(ObservedRoom::LOGICAL_ROOT, "observed data\n");
    let before = world.files();
    let source = reads_the_logical_name(&world);
    let inputs = vec![format!("./{}", ObservedRoom::LOGICAL_ROOT)];
    let report = rehearsed(TEST, "only", &world.room(), &source, &inputs, &[]).await;
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs.len() == 1 && outputs[0].text == "observed data\n"),
        "the run read the data, never the candidate source: {report:?}"
    );
    assert!(bound_to(&report, &source), "{report:?}");
    assert_eq!(world.files(), before);
}

#[tokio::test]
async fn the_admitted_world_is_the_candidate_alone_whatever_the_room_holds() {
    const TEST: &str = concat!(
        module_path!(),
        "::the_admitted_world_is_the_candidate_alone_whatever_the_room_holds"
    );
    // The candidate's text mentions `mcp:`, which makes the admission door read a registry
    // from the project it admits from, and the room holds an observed file of that name.
    // Admitted over an empty namespace, the world is the candidate alone: its digest is the
    // one the door gives the candidate's bytes by themselves, and the file stays data.
    let world = World::new(&[("data/sales.csv", SALES)]);
    world.put_at_root(MCP_SERVERS, "{\"mcpServers\": {}}\n");
    let before = world.files();
    let source = mentions_mcp(&world);
    let inputs = vec![world.path("data/sales.csv"), format!("./{MCP_SERVERS}")];
    let report = rehearsed(TEST, "only", &world.room(), &source, &inputs, &[]).await;
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs.len() == 1 && outputs[0].text == format!("mcp: {SALES}")),
        "{report:?}"
    );
    assert_eq!(
        report.admitted_digest,
        admitted_digest_of(&source),
        "no observed file joined the admitted world: {report:?}"
    );
    assert!(bound_to(&report, &source), "{report:?}");
    assert_eq!(world.files(), before);
}

#[cfg(unix)]
#[tokio::test]
async fn a_write_protected_original_still_rehearses_and_is_untouched() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_write_protected_original_still_rehearses_and_is_untouched"
    );
    // Every directory of the original made 0o555 and the input 0o444: any write the room
    // attempted on the original would fail, so a pass proves read handles were enough.
    let world = World::new(&[("data/sales.csv", SALES)]);
    let _protected = room_support::Protected::new(&[
        world.project(),
        world.project_path(""),
        world.project_path("data"),
        world.project_path("data/sales.csv"),
    ]);
    let before = world.files();
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &sales_copy(&world),
        &sales_input(&world),
        &[],
    )
    .await;
    assert_eq!(first_text(&report), Some(SALES), "{report:?}");
    assert!(report.room.prepared && report.room.cleaned, "{report:?}");
    assert_eq!(
        world.files(),
        before,
        "the original is byte for byte unchanged"
    );
}

#[tokio::test]
async fn a_run_may_write_data_at_the_logical_name() {
    const TEST: &str = concat!(module_path!(), "::a_run_may_write_data_at_the_logical_name");
    let world = World::new(&[]);
    let before = world.files();
    let source = room_support::writes_the_logical_name();
    let report = rehearsed(TEST, "only", &world.room(), &source, &[], &[]).await;
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs.len() == 1 && outputs[0].text == "data" && outputs[0].written),
        "{report:?}"
    );
    assert!(
        bound_to(&report, &source),
        "the admitted candidate is unchanged: {report:?}"
    );
    assert_eq!(world.files(), before);
}
