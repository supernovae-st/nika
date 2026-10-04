// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shared writer below its adapters: policy bounds, metadata admission, scope and closing
//! records, container safety and local status. The real CLI positives (returned Text, repair,
//! cancellation) live in `nika-cli/tests/compile_cli/capture.rs`; these complement them. Saved
//! bytes are parsed here independently of the writer's counters.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nika_fs::OwnedDir;
use serde_json::Value;

use super::{
    Capture, CaptureContext, CapturePolicy, CaptureReport, CaptureState, CaptureStatus,
    TextAdmission,
};

fn admit_all() -> TextAdmission {
    Arc::new(|_| true)
}

fn policy(withheld: &[&str]) -> CapturePolicy {
    CapturePolicy::admitted(
        withheld.iter().map(|value| (*value).to_owned()).collect(),
        admit_all(),
    )
}

/// A private project with its capture container created the way the CLI does.
fn project() -> (tempfile::TempDir, OwnedDir) {
    let root = tempfile::tempdir().unwrap();
    let held = OwnedDir::open(root.path()).unwrap();
    let capture = CaptureContext::directory(&held, &[".nika", "compile", "capture"]).unwrap();
    (root, capture)
}

fn capture_dir(root: &Path) -> PathBuf {
    root.join(".nika/compile/capture")
}

/// The one reserved file of a container, with its name.
fn artifact(root: &Path) -> (String, Vec<u8>) {
    let mut logs: Vec<_> = fs::read_dir(capture_dir(root))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".capture"))
        .collect();
    assert_eq!(logs.len(), 1, "{logs:?}");
    let name = logs.remove(0);
    let bytes = fs::read(capture_dir(root).join(&name)).unwrap();
    (name, bytes)
}

/// NDJSON records followed only by space padding, parsed without the writer.
fn records(bytes: &[u8]) -> Vec<Value> {
    let text = std::str::from_utf8(bytes).unwrap();
    let body = text.trim_end_matches(' ');
    assert!(body.ends_with('\n'), "every record ends with its line feed");
    body.lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn withheld_values_are_bounded_before_they_are_encoded_or_kept() {
    assert!(policy(&["sk-test"]).ready());
    assert!(policy(&[]).with_withheld("sk-test").ready());
    // One value over 32 KiB disables capture; authoring itself is never reduced.
    let large = "k".repeat(32 * 1024 + 1);
    assert!(!policy(&[]).with_withheld(&large).ready());
    assert!(policy(&[]).with_withheld(&"k".repeat(32 * 1024)).ready());
    // At most 256 retained needles (128 raw/escaped pairs).
    let mut many = policy(&[]);
    for i in 0..128 {
        many = many.with_withheld(&format!("value-{i}"));
    }
    assert!(many.ready());
    assert!(!many.with_withheld("one-more").ready());
    // The raw plus escaped aggregate stays within 512 KiB.
    let mut total = policy(&[]);
    let quotes = "\"".repeat(32 * 1024 - 2);
    for _ in 0..5 {
        total = total.with_withheld(&quotes);
    }
    assert!(total.ready(), "5 x (32 KiB raw + 64 KiB escaped) = 480 KiB");
    assert!(
        !total.with_withheld(&quotes).ready(),
        "a sixth crosses 512 KiB"
    );
    // The constructor's own bounds are kept.
    let values: Vec<String> = (0..129).map(|i| format!("v{i}")).collect();
    assert!(!CapturePolicy::admitted(values, admit_all()).ready());
}

fn admitted(values: Vec<String>) -> CapturePolicy {
    CapturePolicy::admitted(values, admit_all())
}

/// The constructor and `with_withheld` keep one law: at most 128 values, 32 KiB raw each, 256
/// retained needles, 512 KiB of raw plus escaped bytes; refused work stops at the refusal.
#[test]
fn the_constructor_keeps_the_ratified_bounds_and_stops_at_a_refusal() {
    let values = |n: usize| (0..n).map(|i| format!("v{i}")).collect::<Vec<_>>();
    let ok = admitted(values(128));
    assert!(ok.ready());
    assert_eq!(ok.needle_count(), 256);
    let refused = admitted(values(129));
    assert!(!refused.ready());
    assert_eq!(
        refused.needle_count(),
        0,
        "a refused count encodes and keeps nothing"
    );
    assert!(admitted(vec!["k".repeat(32 * 1024)]).ready());
    assert!(!admitted(vec!["k".repeat(32 * 1024 + 1)]).ready());
    // Escaping alone crosses the aggregate: 5 x (32 KiB - 2 quotes, escaped twice) fits in 512
    // KiB, the sixth value does not.
    let quotes = "\"".repeat(32 * 1024 - 2);
    assert!(admitted(vec![quotes.clone(); 5]).ready(), "480 KiB fits");
    assert!(
        !admitted(vec![quotes.clone(); 6]).ready(),
        "the sixth crosses 512 KiB"
    );
    // Nothing after a refused value is encoded or kept.
    let stopped = admitted(vec![
        "a".to_owned(),
        "k".repeat(32 * 1024 + 1),
        "b".to_owned(),
    ]);
    assert!(!stopped.ready());
    assert_eq!(
        stopped.needle_count(),
        2,
        "only the value before the refusal"
    );
    assert!(!stopped.clone().with_withheld("c").ready());
    assert_eq!(stopped.with_withheld("c").needle_count(), 2);
    // Known keys stay refused raw and escaped.
    let keys = admitted(vec!["sk-test".to_owned(), "quo\"te".to_owned()]);
    assert!(!keys.admits("x sk-test y") && !keys.admits("x quo\\\"te"));
    assert!(keys.admits("x y"));
}

#[test]
fn selected_metadata_is_admitted_raw_and_escaped_within_512_bytes() {
    let policy = policy(&["sk-secret", "quo\"te"]);
    let raw = "m".repeat(512);
    assert_eq!(policy.metadata(Some(&raw)), (Some(raw.clone()), false));
    assert_eq!(policy.metadata(Some(&"m".repeat(513))), (None, true));
    // 256 quotes escape to exactly 512 bytes; 257 escape to 514.
    let escaped_512 = "\"".repeat(256);
    assert_eq!(
        policy.metadata(Some(&escaped_512)),
        (Some(escaped_512.clone()), false)
    );
    assert_eq!(policy.metadata(Some(&"\"".repeat(257))), (None, true));
    assert_eq!(policy.metadata(Some("vllm/sk-secret")), (None, true));
    // The known value's escaped spelling is withheld as well as its raw spelling.
    assert_eq!(policy.metadata(Some("x quo\\\"te")), (None, true));
    assert_eq!(policy.metadata(Some("x quo\"te")), (None, true));
    assert_eq!(policy.metadata(None), (None, false));
}

#[test]
fn a_scope_without_observations_records_its_scope_and_an_unknown_close() {
    let (root, directory) = project();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let listened = Arc::clone(&seen);
    let report = CaptureReport::with_listener(Arc::new(move |status: CaptureStatus| {
        listened.lock().unwrap().push(status.state);
    }));
    let capture = Capture::start(
        Some(&directory),
        policy(&["sk-secret"]),
        report.clone(),
        Some("vllm/served"),
        512,
        2000,
    );
    let opened = report.status();
    assert_eq!(opened.state, CaptureState::Open);
    let identity = opened.identity.expect("an opaque identity");
    // A scope that is built but never polled observes nothing.
    let unpolled = Capture::observe(Some(&capture), async { 7 });
    drop(unpolled);
    drop(capture);
    let status = report.status();
    assert_eq!(status.state, CaptureState::Saved);
    assert!(status.finished && status.close_saved && !status.outcome_returned);
    assert_eq!((status.received, status.saved), (0, 0));
    assert_eq!(
        *seen.lock().unwrap(),
        [CaptureState::Open, CaptureState::Saved]
    );

    let (name, bytes) = artifact(root.path());
    assert_eq!(name, format!("{}-{}.capture", identity[0], identity[1]));
    assert_eq!(bytes.len(), 1024 * 1024, "the whole reservation is kept");
    let records = records(&bytes);
    assert_eq!(records.len(), 2);
    let scope = &records[0];
    assert_eq!(scope["kind"], "scope");
    assert_eq!(scope["identity"], serde_json::json!(identity));
    assert_eq!(scope["requested_model"], "vllm/served");
    assert_eq!(scope["requested_model_withheld"], false);
    assert_eq!(scope["physical_send"], "unknown");
    assert!(scope["scope_opened_unix_ms"].as_u64().is_some());
    let close = &records[1];
    assert_eq!(close["kind"], "close");
    assert_eq!(close["outcome_returned"], false);
    assert_eq!(close["received"], 0);
    assert_eq!(close["unreceived_or_unsynchronized_work"], "unknown");
    assert!(close.get("prompt_role_block_bytes").is_none());
    let file = fs::metadata(capture_dir(root.path()).join(&name)).unwrap();
    assert_eq!(file.mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(capture_dir(root.path())).unwrap().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::read(capture_dir(root.path()).join(".gitignore")).unwrap(),
        b"*\n"
    );
}

#[test]
fn a_secret_requested_model_is_withheld_and_never_reaches_disk() {
    let (root, directory) = project();
    let report = CaptureReport::default();
    let capture = Capture::start(
        Some(&directory),
        policy(&["sk-secret"]),
        report.clone(),
        Some("vllm/sk-secret"),
        16,
        10,
    );
    capture.finish(None);
    let (_, bytes) = artifact(root.path());
    let records = records(&bytes);
    assert!(records[0]["requested_model"].is_null());
    assert_eq!(records[0]["requested_model_withheld"], true);
    assert!(!String::from_utf8_lossy(&bytes).contains("sk-secret"));
    let summary = report.status().summary();
    assert!(!summary.contains("sk-secret") && !summary.contains("vllm"));
    assert!(!summary.contains(&root.path().display().to_string()));
}

#[test]
fn finish_happens_once_and_drop_after_finish_writes_nothing_more() {
    let (root, directory) = project();
    let report = CaptureReport::default();
    let capture = Capture::start(Some(&directory), policy(&[]), report.clone(), None, 1, 1);
    capture.finish(None);
    let (_, first) = artifact(root.path());
    capture.finish(None);
    drop(capture);
    let (_, second) = artifact(root.path());
    assert_eq!(first, second);
    assert_eq!(records(&second).len(), 2);
}

#[test]
fn without_a_safe_container_capture_is_unavailable_and_writes_nothing() {
    // No held directory at all.
    let report = CaptureReport::default();
    let capture = Capture::start(None, policy(&[]), report.clone(), None, 1, 1);
    assert_eq!(report.status().state, CaptureState::Unavailable);
    drop(capture);
    assert!(!report.status().close_saved);

    // A container other users can read.
    let (root, directory) = project();
    fs::set_permissions(capture_dir(root.path()), fs::Permissions::from_mode(0o755)).unwrap();
    let report = CaptureReport::default();
    drop(Capture::start(
        Some(&directory),
        policy(&[]),
        report.clone(),
        None,
        1,
        1,
    ));
    assert_eq!(report.status().state, CaptureState::Unavailable);
    let names: Vec<_> = fs::read_dir(capture_dir(root.path())).unwrap().collect();
    assert!(
        names.is_empty(),
        "nothing is created in an unsafe container"
    );

    // A policy whose bounds failed never reserves.
    let (root, directory) = project();
    let disabled = policy(&[]).with_withheld(&"k".repeat(40 * 1024));
    let report = CaptureReport::default();
    drop(Capture::start(
        Some(&directory),
        disabled,
        report.clone(),
        None,
        1,
        1,
    ));
    assert_eq!(report.status().state, CaptureState::Unavailable);
    assert_eq!(fs::read_dir(capture_dir(root.path())).unwrap().count(), 0);
}

/// Every name in the capture container, sorted.
fn names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(capture_dir(root))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

/// The ignore marker is exact before a round is reserved. A wrong marker, an empty one (a first
/// round that created it and has not written yet) or an incomplete one refuses the round with
/// nothing created; the marker is never rewritten. An exact marker permits the round.
#[test]
fn a_refused_ignore_marker_reserves_nothing_and_an_exact_one_permits_the_round() {
    for marker in [&b"x\n"[..], b"", b"*", b"*\n\n"] {
        let (root, directory) = project();
        let path = capture_dir(root.path()).join(".gitignore");
        fs::write(&path, marker).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let report = CaptureReport::default();
        drop(Capture::start(
            Some(&directory),
            policy(&[]),
            report.clone(),
            None,
            1,
            1,
        ));
        assert_eq!(
            report.status().state,
            CaptureState::Unavailable,
            "{marker:?}"
        );
        assert_eq!(
            names(root.path()),
            [".gitignore"],
            "{marker:?}: nothing reserved"
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            marker,
            "{marker:?}: marker untouched"
        );
    }
    let (root, directory) = project();
    let path = capture_dir(root.path()).join(".gitignore");
    fs::write(&path, "*\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let report = CaptureReport::default();
    drop(Capture::start(
        Some(&directory),
        policy(&[]),
        report.clone(),
        None,
        1,
        1,
    ));
    assert_eq!(report.status().state, CaptureState::Saved);
    let (_, bytes) = artifact(root.path());
    assert_eq!(records(&bytes).len(), 2);
}

#[test]
fn an_exhausted_container_refuses_a_new_round_without_removing_anything() {
    let (root, directory) = project();
    let filler = capture_dir(root.path()).join("old.capture");
    let file = fs::File::create(&filler).unwrap();
    fs::set_permissions(&filler, fs::Permissions::from_mode(0o600)).unwrap();
    file.set_len(15 * 1024 * 1024 + 1).unwrap();
    let report = CaptureReport::default();
    drop(Capture::start(
        Some(&directory),
        policy(&[]),
        report.clone(),
        None,
        1,
        1,
    ));
    assert_eq!(report.status().state, CaptureState::Unavailable);
    assert_eq!(fs::metadata(&filler).unwrap().len(), 15 * 1024 * 1024 + 1);
    let logs = fs::read_dir(capture_dir(root.path()))
        .unwrap()
        .filter(|entry| entry.as_ref().unwrap().file_name() != "old.capture")
        .filter(|entry| {
            let name = entry.as_ref().unwrap().file_name();
            name.to_string_lossy().ends_with(".capture")
        })
        .count();
    assert_eq!(logs, 0);
}

#[test]
fn the_container_is_created_private_below_the_held_root() {
    let root = tempfile::tempdir().unwrap();
    let held = OwnedDir::open(root.path()).unwrap();
    let directory = CaptureContext::directory(&held, &[".nika", "compile", "capture"]);
    assert!(directory.is_some());
    for component in [".nika", ".nika/compile", ".nika/compile/capture"] {
        let mode = fs::metadata(root.path().join(component)).unwrap().mode() & 0o777;
        assert_eq!(mode & 0o077, 0, "{component} is private: {mode:o}");
    }
    assert!(CaptureContext::directory(&held, &[]).is_none());
}
