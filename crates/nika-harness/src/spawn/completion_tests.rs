// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A spawned completion opened inside a call's deadline, against keyless scripted adapters: an
//! expired deadline starts no process, a row whose identity is its handshake starts its adapter
//! once (the session's own `initialize` is the identity answer), and the transport is bounded by
//! the call's own allowance, never by the fixed generic bound.
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

/// The first frame of a session, ended or errored, within a generous outer bound.
async fn first_frame(
    stream: &mut HarnessEventStream,
) -> Option<Result<nika_kernel::ai::harness::HarnessEvent, HarnessError>> {
    tokio::time::timeout(
        Duration::from_secs(20),
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut *stream).poll_next(cx)),
    )
    .await
    .expect("ended long before the generic 300 s bound")
}

/// A row whose identity is its handshake starts its adapter ONCE: the session's own
/// `initialize` answer is the identity, judged before `session/new`, so no probe process runs
/// beside the session. The scripted adapter answers `initialize` and nothing else, so the
/// admitted session then ends at the call's allowance.
#[tokio::test]
async fn a_handshake_row_starts_its_adapter_once() {
    let dir = tempfile::tempdir().expect("dir");
    let (seat, log) = adapter(dir.path(), "0", true);
    let opened = seat
        .open(
            HarnessRequest::new("p", dir.path()),
            Deadline::start(Duration::from_millis(1500)),
            Progress::default(),
        )
        .await
        .expect("the adapter opens");
    let mut stream = opened.stream;
    let first = first_frame(&mut stream).await;
    assert!(
        matches!(&first, Some(Err(HarnessError::Session { reason })) if reason.contains("idle deadline")),
        "admitted at initialize, then silent on session/new: {first:?}"
    );
    assert_eq!(
        starts(&log),
        1,
        "the session is the handshake: no probe beside it"
    );
}

/// The identity answer spends the call's own deadline: an adapter that answers `initialize`
/// after it ends the session within the call's allowance, as the session death it is, and the
/// one process that was started is the only one.
#[tokio::test]
async fn a_start_slower_than_the_deadline_ends_within_the_allowance() {
    let dir = tempfile::tempdir().expect("dir");
    let (seat, log) = adapter(dir.path(), "0.6", true);
    let started = tokio::time::Instant::now();
    let opened = seat
        .open(
            HarnessRequest::new("p", dir.path()),
            Deadline::start(Duration::from_millis(400)),
            Progress::default(),
        )
        .await
        .expect("the adapter starts inside the deadline");
    let mut stream = opened.stream;
    let first = first_frame(&mut stream).await;
    assert!(
        matches!(&first, Some(Err(error @ HarnessError::Session { reason }))
            if reason.contains("idle deadline") && error.is_transient()),
        "{first:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    drop(stream);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(starts(&log), 1, "one adapter, nothing started after it");
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
    let first = first_frame(&mut stream).await;
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

/// What a seat sends with `session/new` follows its adapter and role: a Claude Code one-shot its
/// audited profile, its thinking shown; a Claude Code agent session its thinking shown and the
/// bypass mode withheld (NIK-266), its own loop untouched; a Codex seat, one-shot or agent,
/// nothing.
#[test]
fn a_seat_sends_its_profile_or_its_agent_options() {
    let seat = |id: &str| SpawnedHarness::new(HarnessAdapter::new(id, "python3").expect("id"));
    let claude = seat("claude-code");
    assert_eq!(
        claude.session_meta(),
        crate::authoring::acp::agent_meta("claude-code")
    );
    let shown = serde_json::json!({"type": "adaptive", "display": "summarized"});
    let agent = claude
        .session_meta()
        .expect("a Claude Code agent session sends options");
    assert_eq!(
        agent,
        serde_json::json!({"claudeCode": {"options": {
            "allowDangerouslySkipPermissions": false, "thinking": shown}}})
    );
    let one_shot = claude
        .for_completion(Completion::Authoring)
        .expect("audited");
    assert_eq!(
        one_shot.session_meta(),
        Some(crate::authoring::acp::profile())
    );
    let codex = seat("codex");
    assert_eq!(codex.session_meta(), None);
    let codex = codex.for_completion(Completion::Infer).expect("audited");
    assert_eq!(codex.session_meta(), None);
    assert_eq!(seat("gemini").session_meta(), None);
}
