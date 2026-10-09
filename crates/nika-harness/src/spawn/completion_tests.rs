// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A spawned completion opened inside a call's deadline, against keyless scripted adapters: the
//! deadline is spent by the probes, an expired one starts no process, and the transport is bounded
//! by the call's own allowance, never by the fixed generic bound.
use super::*;
use crate::authoring::acp::{Completion, Deadline, Door, Progress};
use futures_core::Stream as _;
use std::time::Duration;

/// The audited claude-agent-acp identity the completion profile admits.
const IDENTITY: &str = r#"{"protocolVersion":1,"agentInfo":{"name":"@agentclientprotocol/claude-agent-acp","version":"0.81.1"}}"#;

/// A scripted adapter: every start appends one line to `starts`; it answers `initialize` after
/// `delay` seconds, then reads its input until EOF and never answers anything else.
fn adapter(
    dir: &std::path::Path,
    delay: &str,
    probe: bool,
) -> (SpawnedHarness, std::path::PathBuf) {
    let starts = dir.join("starts.log");
    let script = dir.join("adapter.py");
    std::fs::write(
        &script,
        format!(
            r#"import json, sys, time
open({starts:?}, "a").write("start\n")
req = json.loads(sys.stdin.readline())
time.sleep({delay})
print(json.dumps({{"jsonrpc": "2.0", "id": req["id"], "result": json.loads({identity:?})}}))
sys.stdout.flush()
sys.stdin.read()
"#,
            starts = starts.to_string_lossy(),
            identity = IDENTITY,
        ),
    )
    .expect("script");
    let mut row = HarnessAdapter::new("claude-code", "python3")
        .expect("id is fine")
        .with_args(vec![script.to_string_lossy().into_owned()])
        .with_identities(vec!["@agentclientprotocol/claude-agent-acp".to_owned()]);
    if probe {
        row = row.with_handshake_probe();
    }
    let seat = SpawnedHarness::new(row)
        .for_completion(Completion::Authoring)
        .expect("an audited profile");
    (seat, starts)
}

fn starts(path: &std::path::Path) -> usize {
    std::fs::read_to_string(path).map_or(0, |log| log.lines().count())
}

/// An expired deadline starts nothing: no identity probe, no adapter.
#[tokio::test]
async fn an_expired_deadline_starts_no_process() {
    let dir = tempfile::tempdir().expect("dir");
    let (seat, log) = adapter(dir.path(), "0", true);
    let request = HarnessRequest::new("p", dir.path());
    let opened = seat
        .open(
            request,
            Deadline::start(Duration::ZERO),
            Progress::default(),
        )
        .await;
    assert!(
        matches!(&opened, Err(HarnessError::Session { reason }) if reason.contains("nothing was spawned")),
        "{:?}",
        opened.as_ref().err()
    );
    assert_eq!(starts(&log), 0, "no probe, no adapter");
}

/// The identity probe spends the same deadline: a probe answering after the deadline leaves
/// nothing, so the session adapter is never spawned.
#[tokio::test]
async fn a_deadline_spent_by_the_probe_spawns_no_session() {
    let dir = tempfile::tempdir().expect("dir");
    let (seat, log) = adapter(dir.path(), "0.6", true);
    let request = HarnessRequest::new("p", dir.path());
    let opened = seat
        .open(
            request,
            Deadline::start(Duration::from_millis(400)),
            Progress::default(),
        )
        .await;
    assert!(
        matches!(&opened, Err(HarnessError::Session { reason }) if reason.contains("nothing was spawned")),
        "{:?}",
        opened.as_ref().err()
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(starts(&log), 1, "the probe ran; no session adapter started");
}

/// The spawned transport is bounded by the call's own allowance: an adapter that never answers
/// `session/new` ends the session within that allowance, not after the fixed generic 300 s
/// bound, and the bound reported is the one passed.
#[tokio::test]
async fn the_spawned_transport_is_bounded_by_the_call_allowance() {
    let dir = tempfile::tempdir().expect("dir");
    let (seat, log) = adapter(dir.path(), "0", false);
    let started = tokio::time::Instant::now();
    let deadline = Deadline::start(Duration::from_millis(1500));
    let opened = seat
        .open(
            HarnessRequest::new("p", dir.path()),
            deadline,
            Progress::default(),
        )
        .await
        .expect("the adapter opens");
    assert!(
        opened.allowance > Duration::from_millis(1000)
            && opened.allowance <= Duration::from_millis(1500),
        "{:?}",
        opened.allowance
    );
    let mut stream = opened.stream;
    let first = tokio::time::timeout(
        Duration::from_secs(20),
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut stream).poll_next(cx)),
    )
    .await
    .expect("ended long before the generic 300 s bound");
    assert!(
        matches!(&first, Some(Err(HarnessError::Session { reason })) if reason.contains("idle deadline")),
        "{first:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(starts(&log), 1, "one adapter, no retry");
}
