// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The standing ledger counts a rule's Runs per window across restarts and
//! fails closed when it cannot be read; the archive moves a cleared journal's
//! exact bytes aside before it empties it, and never touches a journal that
//! is within its threshold or that still blocks.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use super::super::super::JOURNAL;
use super::super::{Blocked, RunAccount, RunJournal, clear};
use super::*;
use nika_fs::OwnedDir;
use std::path::Path;
use std::sync::Mutex;

const ROUTE: &str = "deepseek/standing-fixture";
const DAY: Duration = Duration::from_secs(86_400);

fn at(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn cleared(root: &Path, observer: &str) -> Cleared {
    let dir = OwnedDir::open(root).unwrap();
    clear(&dir, observer).unwrap().expect("the project clears")
}

fn ledger(root: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(root.join(".nika").join(STANDING_LEDGER)).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// An account that sent `0` requests it could not price, closed or not.
#[derive(Default)]
struct Account(Mutex<(u64, bool)>);

impl RunAccount for std::sync::Arc<Account> {
    fn observation(&self) -> std::io::Result<serde_json::Value> {
        let (sent, closed) = *self.0.lock().unwrap();
        let state = match (closed, sent) {
            (false, _) => "Open",
            (true, 0) => "Closed",
            (true, _) => "Uncertain",
        };
        let attempts: Vec<_> = (0..sent)
            .map(|id| serde_json::json!({"id": id, "sent": true, "estimated_nano_usd": null}))
            .collect();
        Ok(
            serde_json::json!({"schema": "nika/inference-cost-observation@1",
            "known_subtotal_nano_usd": "0", "unknown_calls": sent, "unknown_attempts": attempts,
            "attempts": [], "state": state}),
        )
    }
    fn close(&self, _why: &str) -> std::io::Result<()> {
        self.0.lock().unwrap().1 = true;
        Ok(())
    }
}

/// One Run of `invocation` in the project: prepared, `sent` requests left
/// without a usable answer, settled.
fn run(root: &Path, invocation: &str, sent: u64) {
    let account = std::sync::Arc::new(Account::default());
    let journal = RunJournal::new(
        cleared(root, invocation),
        invocation.into(),
        Box::new(account.clone()),
    );
    journal.observe("prepared").unwrap();
    account.0.lock().unwrap().0 = sent;
    journal.settle().unwrap();
}

fn journal_bytes(root: &Path) -> Vec<u8> {
    std::fs::read(root.join(".nika").join(JOURNAL)).unwrap()
}

/// A rule's window admits exactly `max_runs`: each admission is recorded
/// before its Run starts, the next one is refused with the count and the
/// instant the oldest leaves the window, and a refusal records nothing.
#[test]
fn a_window_admits_its_runs_then_refuses_with_the_count_and_the_reopening() {
    let root = tempfile::tempdir().unwrap();
    let dir = OwnedDir::open(root.path()).unwrap();
    assert_eq!(standing_runs(&dir, ROUTE, DAY, at(1_000)).unwrap(), 0);
    assert!(
        !root.path().join(".nika").exists(),
        "reading counts nothing and writes nothing, not even `.nika/`"
    );
    let held = cleared(root.path(), "run-1");
    assert_eq!(standing_runs(&dir, ROUTE, DAY, at(1_000)).unwrap(), 0);
    assert_eq!(
        held.admit_standing_run(ROUTE, 2, DAY, at(1_000)).unwrap(),
        Ok(1)
    );
    assert_eq!(
        held.admit_standing_run(ROUTE, 2, DAY, at(1_500)).unwrap(),
        Ok(2)
    );
    let full = held
        .admit_standing_run(ROUTE, 2, DAY, at(2_000))
        .unwrap()
        .expect_err("the window is full");
    assert_eq!(full.admitted, 2);
    assert_eq!(full.reopens_at, 1_000 + 86_400);
    assert_eq!(
        standing_runs(&dir, ROUTE, DAY, at(2_000)).unwrap(),
        2,
        "read without the lease its admissions were counted under"
    );
    assert_eq!(
        ledger(root.path()),
        serde_json::json!({"schema": "nika/standing-cost-runs@1",
            "runs": {ROUTE: [1_000, 1_500]}}),
        "a refusal records nothing"
    );
}

/// The window slides: a Run leaves it exactly one window after its admission
/// (never a second earlier), and the ledger then forgets it.
#[test]
fn the_window_slides_and_forgets_what_left_it() {
    let root = tempfile::tempdir().unwrap();
    let held = cleared(root.path(), "run-1");
    let dir = OwnedDir::open(root.path()).unwrap();
    assert_eq!(
        held.admit_standing_run(ROUTE, 1, DAY, at(1_000)).unwrap(),
        Ok(1)
    );
    let last_second = at(1_000 + 86_399);
    assert_eq!(standing_runs(&dir, ROUTE, DAY, last_second).unwrap(), 1);
    assert!(
        held.admit_standing_run(ROUTE, 1, DAY, last_second)
            .unwrap()
            .is_err(),
        "still inside the window"
    );
    let reopened = at(1_000 + 86_400);
    assert_eq!(standing_runs(&dir, ROUTE, DAY, reopened).unwrap(), 0);
    assert_eq!(
        held.admit_standing_run(ROUTE, 1, DAY, reopened).unwrap(),
        Ok(1)
    );
    assert_eq!(
        ledger(root.path())["runs"][ROUTE],
        serde_json::json!([87_400]),
        "the Run that left the window is forgotten"
    );
}

/// The count is the route's and survives its host: another route has its own
/// window, a later lease holder reads what an earlier one recorded, and a
/// clock that moved back never frees a window.
#[test]
fn the_count_is_per_route_durable_and_never_freed_by_a_clock_moved_back() {
    let root = tempfile::tempdir().unwrap();
    let first = cleared(root.path(), "run-1");
    assert_eq!(
        first.admit_standing_run(ROUTE, 1, DAY, at(5_000)).unwrap(),
        Ok(1)
    );
    assert_eq!(
        first
            .admit_standing_run("deepseek/other", 1, DAY, at(5_000))
            .unwrap(),
        Ok(1),
        "another route counts apart"
    );
    drop(first);
    let restarted = cleared(root.path(), "run-2");
    let dir = OwnedDir::open(root.path()).unwrap();
    assert_eq!(standing_runs(&dir, ROUTE, DAY, at(5_001)).unwrap(), 1);
    let earlier = at(4_000);
    assert_eq!(standing_runs(&dir, ROUTE, DAY, earlier).unwrap(), 1);
    assert!(
        restarted
            .admit_standing_run(ROUTE, 1, DAY, earlier)
            .unwrap()
            .is_err(),
        "a Run admitted in the clock's future still counts"
    );
}

/// A ledger this engine did not write is never read as empty: its count is
/// unknown, so reading and admitting both fail and nothing is rewritten.
#[test]
fn an_unreadable_ledger_fails_closed() {
    for body in [
        "not json",
        r#"{"schema":"nika/standing-cost-runs@2","runs":{}}"#,
        r#"{"schema":"nika/standing-cost-runs@1","runs":{},"extra":1}"#,
        r#"{"schema":"nika/standing-cost-runs@1","runs":{"a/b":["soon"]}}"#,
    ] {
        let root = tempfile::tempdir().unwrap();
        let held = cleared(root.path(), "run-1");
        let path = root.path().join(".nika").join(STANDING_LEDGER);
        std::fs::write(&path, body).unwrap();
        let dir = OwnedDir::open(root.path()).unwrap();
        let read = standing_runs(&dir, ROUTE, DAY, at(1_000)).unwrap_err();
        assert_eq!(read.kind(), std::io::ErrorKind::InvalidData, "{body}");
        assert_eq!(
            read.to_string(),
            "the standing ledger is unreadable: its Runs cannot be counted"
        );
        assert!(
            held.admit_standing_run(ROUTE, 5, DAY, at(1_000)).is_err(),
            "{body}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body, "untouched");
    }
}

/// A ledger is never written past its read bound. The Run that would carry it
/// there is refused uncounted, and the ledger stays as it was: whole, readable
/// and still counting for every route.
#[test]
fn a_ledger_is_never_written_past_its_read_bound() {
    let root = tempfile::tempdir().unwrap();
    let held = cleared(root.path(), "run-1");
    let dir = OwnedDir::open(root.path()).unwrap();
    // Another route's Runs fill the ledger to within one entry of its bound.
    let prefix = format!(r#"{{"schema":"{SCHEMA}","runs":{{"other/route":["#);
    let bound = usize::try_from(LEDGER_BOUND).unwrap();
    let entries = (bound - prefix.len() - "4000]}}".len()) / "4000,".len();
    let body = format!("{prefix}{}4000]}}}}", "4000,".repeat(entries));
    assert!(body.len() <= bound && body.len() + "4000,".len() > bound);
    let path = root.path().join(".nika").join(STANDING_LEDGER);
    std::fs::write(&path, &body).unwrap();
    let others = u32::try_from(entries + 1).unwrap();
    assert_eq!(
        standing_runs(&dir, "other/route", DAY, at(5_000)).unwrap(),
        others
    );
    let full = held
        .admit_standing_run(ROUTE, 5, DAY, at(5_000))
        .unwrap_err();
    assert_eq!(full.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(
        full.to_string(),
        "the standing ledger is full: no further Run can be counted until earlier ones leave their windows"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), body, "untouched");
    assert_eq!(standing_runs(&dir, ROUTE, DAY, at(5_000)).unwrap(), 0);
    // The other route's Runs leave their window: its next admission forgets
    // them, and the ledger counts again.
    let later = at(4_000 + 86_400);
    assert_eq!(
        held.admit_standing_run("other/route", 5, DAY, later)
            .unwrap(),
        Ok(1)
    );
    assert_eq!(
        held.admit_standing_run(ROUTE, 5, DAY, later).unwrap(),
        Ok(1)
    );
}

/// Room is counted against the journal's read bound: a journal that leaves
/// the reserve stays where it is, one byte less room archives it, and an
/// empty journal is never archived, whatever the reserve.
#[test]
fn room_is_counted_against_the_journal_read_bound() {
    let root = tempfile::tempdir().unwrap();
    let empty = cleared(root.path(), "run-0");
    assert_eq!(empty.make_room(u64::MAX).unwrap(), None);
    drop(empty);
    run(root.path(), "run-1", 0);
    let before = journal_bytes(root.path());
    let held = cleared(root.path(), "run-2");
    let spare = READ_BOUND - u64::try_from(before.len()).unwrap();
    assert_eq!(held.make_room(0).unwrap(), None);
    assert_eq!(
        held.make_room(spare).unwrap(),
        None,
        "the reserve fits exactly"
    );
    assert_eq!(journal_bytes(root.path()), before, "untouched");
    let name = held
        .make_room(spare + 1)
        .unwrap()
        .expect("one byte short of the reserve");
    assert_eq!(
        std::fs::read(root.path().join(".nika").join(&name)).unwrap(),
        before,
        "the archive holds the journal's exact bytes"
    );
    assert_eq!(journal_bytes(root.path()), b"");
}

/// A journal within the threshold stays where it is; one that outgrew it is
/// copied whole to a file named by its digest and only then emptied, and the
/// next Run starts a journal that clears.
#[test]
fn a_cleared_journal_over_its_threshold_is_archived_whole_then_emptied() {
    let root = tempfile::tempdir().unwrap();
    run(root.path(), "run-1", 0);
    run(root.path(), "run-2", 0);
    let before = journal_bytes(root.path());
    assert!(!before.is_empty());
    let held = cleared(root.path(), "run-3");
    let exactly = u64::try_from(before.len()).unwrap();
    assert_eq!(
        held.archive_over(exactly).unwrap(),
        None,
        "at the threshold"
    );
    assert_eq!(journal_bytes(root.path()), before, "untouched");
    let name = held
        .archive_over(exactly - 1)
        .unwrap()
        .expect("one byte over the threshold");
    let digest = nika_event::source_id::sha256_hex(&before);
    assert_eq!(
        name,
        format!("inference-cost-observations.{}.ndjson", &digest[..16])
    );
    assert_eq!(
        std::fs::read(root.path().join(".nika").join(&name)).unwrap(),
        before,
        "the archive holds the journal's exact bytes"
    );
    assert_eq!(
        journal_bytes(root.path()),
        b"",
        "then the journal is emptied"
    );
    assert_eq!(held.journal().unwrap().length, 0);
    assert_eq!(
        held.archive_over(0).unwrap(),
        None,
        "nothing left to archive"
    );
    // The Run that archived writes its rows to the emptied journal, and the
    // next review clears it.
    let account = std::sync::Arc::new(Account::default());
    let journal = RunJournal::new(held, "run-3".into(), Box::new(account));
    journal.observe("prepared").unwrap();
    journal.settle().unwrap();
    drop(journal);
    let rows: Vec<serde_json::Value> = String::from_utf8(journal_bytes(root.path()))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["invocation"], "run-3");
    drop(cleared(root.path(), "run-4"));
}

/// An archive interrupted after its copy and before the journal was emptied
/// is finished by the next one: the same bytes name the same file, which is
/// written once, and the journal is emptied then.
#[test]
fn an_interrupted_archive_is_finished_by_the_next_one() {
    let root = tempfile::tempdir().unwrap();
    run(root.path(), "run-1", 0);
    let before = journal_bytes(root.path());
    let digest = nika_event::source_id::sha256_hex(&before);
    let name = format!("inference-cost-observations.{}.ndjson", &digest[..16]);
    let nika = root.path().join(".nika");
    std::fs::write(nika.join(&name), &before).unwrap();
    let held = cleared(root.path(), "run-2");
    assert_eq!(held.archive_over(0).unwrap(), Some(name.clone()));
    assert_eq!(std::fs::read(nika.join(&name)).unwrap(), before);
    assert_eq!(journal_bytes(root.path()), b"");
    let archives = std::fs::read_dir(&nika)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("inference-cost-observations.") && name.ends_with(".ndjson")
        })
        .count();
    assert_eq!(archives, 2, "the journal and its one archive");
}

/// A journal that has another link is never emptied: its rows could be read,
/// or keep growing, under the other name. Its bytes stay whole and the
/// refusal names the link; once the other name is gone the same call finishes.
#[test]
fn a_journal_with_another_link_is_never_emptied() {
    let root = tempfile::tempdir().unwrap();
    run(root.path(), "run-1", 0);
    let before = journal_bytes(root.path());
    let nika = root.path().join(".nika");
    let held = cleared(root.path(), "run-2");
    let other = nika.join("another-name.ndjson");
    std::fs::hard_link(nika.join(JOURNAL), &other).unwrap();
    let refused = held.make_room(u64::MAX).unwrap_err();
    assert_eq!(refused.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(
        refused.to_string(),
        "cost journal no longer has one owned link"
    );
    assert_eq!(journal_bytes(root.path()), before, "left whole");
    std::fs::remove_file(&other).unwrap();
    let name = held.make_room(u64::MAX).unwrap().expect("archived");
    assert_eq!(std::fs::read(nika.join(&name)).unwrap(), before);
    assert_eq!(journal_bytes(root.path()), b"");
}

/// A journal that still blocks is never archived: no `Cleared` exists for it,
/// so an earlier Run whose charge is unknown keeps blocking, row for row.
#[test]
fn a_journal_that_still_blocks_is_never_archived() {
    let root = tempfile::tempdir().unwrap();
    run(root.path(), "run-1", 1);
    let before = journal_bytes(root.path());
    let dir = OwnedDir::open(root.path()).unwrap();
    let blocked = clear(&dir, "run-2").unwrap().expect_err("still exposed");
    assert!(matches!(blocked, Blocked::Exposed(_)), "{blocked}");
    assert_eq!(journal_bytes(root.path()), before);
    let nika = root.path().join(".nika");
    let names: Vec<String> = std::fs::read_dir(&nika)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("observations.") && name != JOURNAL)
        .filter(|name| Path::new(name).extension().is_none_or(|ext| ext != "lock"))
        .collect();
    assert!(names.is_empty(), "{names:?}");
}
