// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A candidate that reaches anything beyond its room's files is refused before any room
//! exists, and the proof is not only the refusal text:
//! - a counting local listener sees no connection;
//! - a spawn sentinel the command would have created stays absent;
//! - the report's effect counts, filled by the room's denied seams, stay at zero;
//! - no room was prepared.
//!
//! The screen reads every task whether or not it is reachable, so a branch that only runs on
//! failure, on unwind, in a fan-out or behind a false condition is screened like any other. An
//! unknown tool and any jq or convert step (no data bound is established for them, whatever the
//! expression) refuse before any evaluator exists; a tool named by a template is refused sooner,
//! by the parser, which requires a literal reference. A run past its bound is stopped, joined
//! and cleaned, and nothing is written afterwards.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;
use nika_onboard::compile::rehearse::{Attempt, Refusal, Rehearsal, RehearsalReport};
use room_support::{
    AGENT, HIGHEST_FIRST, INFER, MCP, NOTIFY, OUTSIDE_THE_JQ_SUBSET, SALES, SECRET, TEMPLATED_TOOL,
    UNKNOWN_TOOL, World, completed, exec_touching, fetch, generate, nested_parent, not_run_because,
    prompt_gate, ranking, refused_before_any_room, rehearsed, sales_input, slow, templated_jq,
    with_branch,
};
use std::io::ErrorKind;
use std::net::TcpListener;
use std::time::{Duration, Instant};

#[test]
fn the_fixtures_are_well_formed_before_any_verdict() {
    // The harness guard: a failure here makes the tests using that fixture harness-invalid.
    room_support::assert_catalog_is_well_formed(&World::new(&[("data/sales.csv", SALES)]));
}

/// A local listener that counts the connections made to it (none expected).
struct Listener {
    socket: TcpListener,
}

impl Listener {
    fn new() -> Self {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        Self { socket }
    }
    fn url(&self) -> String {
        format!("http://{}/", self.socket.local_addr().unwrap())
    }
    /// Whether any connection reached the listener.
    fn contacted(&self) -> bool {
        match self.socket.accept() {
            Ok(_) => true,
            Err(error) => error.kind() != ErrorKind::WouldBlock,
        }
    }
}

async fn rehearse_in(test: &str, world: &World, source: &str) -> RehearsalReport {
    rehearsed(
        test,
        "only",
        &world.room(),
        source,
        &sales_input(world),
        &[],
    )
    .await
}

fn sales_world() -> World {
    World::new(&[("data/sales.csv", SALES)])
}

#[tokio::test]
async fn a_fetch_to_a_counting_local_listener_is_never_run() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_fetch_to_a_counting_local_listener_is_never_run"
    );
    let (world, listener) = (sales_world(), Listener::new());
    let before = world.files();
    let report = rehearse_in(TEST, &world, &fetch(&listener.url())).await;
    assert!(refused_before_any_room(&report, "network"), "{report:?}");
    assert!(!listener.contacted(), "no connection reached the listener");
    assert_eq!(world.files(), before);
}

#[tokio::test]
async fn an_infer_task_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::an_infer_task_is_never_run");
    let report = rehearse_in(TEST, &sales_world(), INFER).await;
    assert!(refused_before_any_room(&report, "provider"), "{report:?}");
}

#[tokio::test]
async fn an_agent_task_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::an_agent_task_is_never_run");
    let report = rehearse_in(TEST, &sales_world(), AGENT).await;
    assert!(refused_before_any_room(&report, "provider"), "{report:?}");
}

#[tokio::test]
async fn an_exec_task_never_spawns() {
    const TEST: &str = concat!(module_path!(), "::an_exec_task_never_spawns");
    let world = sales_world();
    let sentinel = world.base().join("spawned");
    let report = rehearse_in(TEST, &world, &exec_touching(&sentinel)).await;
    assert!(refused_before_any_room(&report, "exec"), "{report:?}");
    assert!(!sentinel.exists(), "the command never ran");
}

#[tokio::test]
async fn an_mcp_tool_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::an_mcp_tool_is_never_run");
    let report = rehearse_in(TEST, &sales_world(), MCP).await;
    assert!(refused_before_any_room(&report, "mcp"), "{report:?}");
}

#[tokio::test]
async fn a_notification_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::a_notification_is_never_run");
    let report = rehearse_in(TEST, &sales_world(), NOTIFY).await;
    assert!(
        refused_before_any_room(&report, "beyond files"),
        "{report:?}"
    );
}

#[tokio::test]
async fn image_generation_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::image_generation_is_never_run");
    let report = rehearse_in(TEST, &sales_world(), &generate("nika:image_generate")).await;
    assert!(refused_before_any_room(&report, "provider"), "{report:?}");
}

#[tokio::test]
async fn speech_generation_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::speech_generation_is_never_run");
    let report = rehearse_in(TEST, &sales_world(), &generate("nika:tts_generate")).await;
    assert!(refused_before_any_room(&report, "provider"), "{report:?}");
}

#[tokio::test]
async fn a_prompt_gate_ends_the_rehearsal_unanswered() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_prompt_gate_ends_the_rehearsal_unanswered"
    );
    let world = sales_world();
    let before = world.files();
    let report = rehearse_in(TEST, &world, &prompt_gate(&world)).await;
    assert!(refused_before_any_room(&report, "gate"), "{report:?}");
    assert_eq!(world.files(), before, "no answer, no consent, no file");
}

#[tokio::test]
async fn a_secret_reference_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::a_secret_reference_is_never_run");
    let report = rehearse_in(TEST, &sales_world(), SECRET).await;
    assert!(refused_before_any_room(&report, "secret"), "{report:?}");
}

#[tokio::test]
async fn a_nested_workflow_is_never_run() {
    const TEST: &str = concat!(module_path!(), "::a_nested_workflow_is_never_run");
    let world = sales_world();
    let sentinel = world.base().join("child-ran");
    world.put("child.nika", &exec_touching(&sentinel));
    let report = rehearse_in(TEST, &world, &nested_parent(&world)).await;
    assert!(
        refused_before_any_room(&report, "nested workflow"),
        "{report:?}"
    );
    assert!(!sentinel.exists());
}

#[tokio::test]
async fn an_unknown_tool_refuses_before_dispatch() {
    const TEST: &str = concat!(module_path!(), "::an_unknown_tool_refuses_before_dispatch");
    let report = rehearse_in(TEST, &sales_world(), UNKNOWN_TOOL).await;
    assert!(refused_before_any_room(&report, "unknown"), "{report:?}");
}

#[tokio::test]
async fn a_tool_named_by_a_template_refuses_before_dispatch() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_tool_named_by_a_template_refuses_before_dispatch"
    );
    // The parser requires a literal tool reference: the candidate is refused as it is read,
    // before any room, so the tool screen is never reached.
    let report = rehearse_in(TEST, &sales_world(), TEMPLATED_TOOL).await;
    assert!(
        refused_before_any_room(&report, "invalid tool reference"),
        "{report:?}"
    );
    assert_eq!(
        report.observation.refusal,
        Some(Refusal::Admission),
        "{report:?}"
    );
}

#[tokio::test]
async fn a_jq_expression_outside_the_bounded_subset_is_never_evaluated() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_jq_expression_outside_the_bounded_subset_is_never_evaluated"
    );
    let world = sales_world();
    let report = rehearse_in(TEST, &world, &ranking(&world, OUTSIDE_THE_JQ_SUBSET)).await;
    assert!(
        refused_before_any_room(&report, "bounded subset"),
        "{report:?}"
    );
}

#[tokio::test]
async fn a_jq_or_convert_step_is_never_run_whatever_its_expression() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_jq_or_convert_step_is_never_run_whatever_its_expression"
    );
    // The ranking's jq expression is inside the subset an earlier design bounded; no bound is
    // established for jq or convert, so it is refused like any other.
    let world = sales_world();
    let report = rehearse_in(TEST, &world, &ranking(&world, HIGHEST_FIRST)).await;
    assert!(
        refused_before_any_room(&report, "bounded subset"),
        "{report:?}"
    );
}

#[tokio::test]
async fn a_templated_jq_expression_is_never_evaluated() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_templated_jq_expression_is_never_evaluated"
    );
    let world = sales_world();
    let report = rehearse_in(TEST, &world, &templated_jq(&world)).await;
    assert!(
        refused_before_any_room(&report, "bounded subset"),
        "{report:?}"
    );
}

async fn branch_is_screened(test: &str, branch: &str) {
    let (world, listener) = (sales_world(), Listener::new());
    let report = rehearse_in(test, &world, &with_branch(&world, &listener.url(), branch)).await;
    assert!(
        refused_before_any_room(&report, "network"),
        "{branch}: {report:?}"
    );
    assert!(!listener.contacted(), "{branch}");
}

#[tokio::test]
async fn an_effect_on_an_unwind_branch_is_screened() {
    const TEST: &str = concat!(
        module_path!(),
        "::an_effect_on_an_unwind_branch_is_screened"
    );
    branch_is_screened(TEST, "    after: { a: unwind }\n").await;
}

#[tokio::test]
async fn an_effect_on_a_failure_branch_is_screened() {
    const TEST: &str = concat!(
        module_path!(),
        "::an_effect_on_a_failure_branch_is_screened"
    );
    branch_is_screened(TEST, "    after: { a: failure }\n").await;
}

#[tokio::test]
async fn an_effect_behind_a_false_condition_is_screened() {
    const TEST: &str = concat!(
        module_path!(),
        "::an_effect_behind_a_false_condition_is_screened"
    );
    branch_is_screened(TEST, "    when: \"${{ false }}\"\n").await;
}

#[tokio::test]
async fn an_effect_in_a_fan_out_body_is_screened() {
    const TEST: &str = concat!(module_path!(), "::an_effect_in_a_fan_out_body_is_screened");
    branch_is_screened(TEST, "    for_each: { items: [1, 2] }\n").await;
}

#[tokio::test]
async fn a_run_past_the_bound_is_stopped_joined_and_leaves_nothing() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_run_past_the_bound_is_stopped_joined_and_leaves_nothing"
    );
    // A cooperative wait: this proves the stopped outcome and cleanup on the wait path only.
    // The join of an in-flight blocking operation is proved by the ledger's own seam test.
    let world = World::new(&[]);
    let decoy = world.decoy(&[("keep.txt", "decoy")]);
    let (before, decoy_before) = (world.files(), decoy.files());
    let started = Instant::now();
    let report = rehearsed(
        TEST,
        "only",
        &world.room().with_bound(Duration::from_millis(150)),
        &slow(&world, "2s"),
        &[],
        &[],
    )
    .await;
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "{:?}",
        started.elapsed()
    );
    assert!(not_run_because(&report, "time bound"), "{report:?}");
    assert!(
        matches!(report.attempt, Attempt::Stopped { elapsed_ms } if elapsed_ms >= 150),
        "a run began, then stopped: {report:?}"
    );
    assert!(report.room.prepared && report.room.cleaned, "{report:?}");
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(world.files(), before, "no late.txt, no room left behind");
    assert_eq!(decoy.files(), decoy_before);
}

#[tokio::test]
async fn a_wait_under_the_bound_completes_with_its_elapsed_time() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_wait_under_the_bound_completes_with_its_elapsed_time"
    );
    let world = World::new(&[]);
    let report = rehearsed(TEST, "only", &world.room(), &slow(&world, "50ms"), &[], &[]).await;
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs.len() == 1 && outputs[0].text == "late" && outputs[0].written),
        "{report:?}"
    );
    assert!(
        matches!(report.attempt, Attempt::Completed { elapsed_ms } if elapsed_ms >= 50),
        "{report:?}"
    );
    assert!(completed(&report) && report.effects.is_none());
}
