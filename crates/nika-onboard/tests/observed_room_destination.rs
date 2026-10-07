// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A stated path the candidate only writes is its destination, never an input to copy: a reader
//! that took « save ./out/r.json as a list … » for a source must not leave a correct candidate
//! untried (`nika compile`, Serve and the Session share this room).
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;
use nika_onboard::compile::room::ObservedRoom;
use room_support::{completed, final_text, rehearsed};

const CANDIDATE: &str = "nika: copy-list
permits:
  fs:
    read:
    - ./in.json
    write:
    - ./out/r.json
  tools:
  - nika:read
  - nika:write
tasks:
  read_in:
    invoke:
      tool: nika:read
      args:
        path: ./in.json
  save:
    with:
      text: ${{ tasks.read_in.output }}
    invoke:
      tool: nika:write
      args:
        path: ./out/r.json
        content: ${{ with.text }}
";

#[tokio::test]
async fn a_stated_destination_the_candidate_only_writes_is_not_an_input() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("in.json"), "[1,2]").unwrap();
    let room = ObservedRoom::new(project.path().canonicalize().unwrap());
    let inputs = ["./in.json".to_owned(), "./out/r.json".to_owned()];
    let targets = ["./out/r.json".to_owned()];
    let report = rehearsed(
        "destination",
        "only-written",
        &room,
        CANDIDATE,
        &inputs,
        &targets,
    )
    .await;
    assert!(completed(&report), "{:?}", report.attempt);
    assert_eq!(final_text(&report, "./out/r.json"), Some("[1,2]"));
    assert!(
        !project.path().join("out").exists(),
        "the original is untouched"
    );
}
