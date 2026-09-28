// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A Run's custody: rows land through the descriptor the lease was taken on
//! (a renamed or copied project never receives them), every row reads the
//! live account, and the journal witness is its exact bytes.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use super::super::JOURNAL;
use super::*;
use std::sync::Mutex;

/// An account the host would own: its sent-but-unpriced attempts and whether it closed.
#[derive(Default)]
struct Account(Mutex<(u64, bool)>);

impl Account {
    fn send(&self) {
        self.0.lock().unwrap().0 += 1;
    }
}

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

fn project() -> (tempfile::TempDir, std::path::PathBuf) {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("project");
    std::fs::create_dir(&root).unwrap();
    (base, root)
}

fn cleared(root: &Path, observer: &str) -> Cleared {
    let dir = OwnedDir::open(root).unwrap();
    clear(&dir, observer)
        .unwrap()
        .expect("a fresh project clears")
}

fn rows(root: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(root.join(".nika").join(JOURNAL))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// E21 · the held descriptor, never the path: a project moved away mid-Run
/// keeps both of its rows, and the directory now at the old path receives
/// none. The review's place test sees the replacement.
#[test]
fn rows_land_where_the_lease_was_taken_after_a_rename() {
    let (base, root) = project();
    let cleared = cleared(&root, "run-1");
    assert!(cleared.same_place(&root).unwrap());
    let account = std::sync::Arc::new(Account::default());
    let journal = RunJournal::new(cleared, "run-1".into(), Box::new(account.clone()));
    journal.observe("prepared").unwrap();
    let moved = base.path().join("moved");
    std::fs::rename(&root, &moved).unwrap();
    std::fs::create_dir_all(root.join(".nika")).unwrap();
    account.send();
    journal.settle().unwrap();
    let kept = rows(&moved);
    assert_eq!(
        kept.iter().map(|r| r["phase"].clone()).collect::<Vec<_>>(),
        ["prepared", "settled"]
    );
    assert!(rows(&root).is_empty(), "no row follows the path");
}

/// A copy of the project put back at the same path carries identical journal
/// bytes: only the held identities tell it apart.
#[test]
fn a_copied_project_at_the_same_path_is_not_the_same_place() {
    let (base, root) = project();
    let cleared = cleared(&root, "review");
    let witness = cleared.journal().unwrap();
    let copy = base.path().join("copy");
    std::fs::create_dir_all(copy.join(".nika")).unwrap();
    std::fs::rename(&root, base.path().join("original")).unwrap();
    std::fs::rename(&copy, &root).unwrap();
    assert!(!cleared.same_place(&root).unwrap());
    let other = OwnedDir::open(&root)
        .unwrap()
        .open_below(&[".nika"])
        .unwrap();
    let bytes = super::super::read(&other).unwrap();
    assert_eq!(
        nika_event::source_id::sha256_hex(&bytes),
        witness.sha256,
        "the copied journal is byte-identical: the bytes alone cannot tell"
    );
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        !cleared.same_place(&root).unwrap(),
        "a vanished path is no place"
    );
}

/// A Run dropped without `settle` settles the account's LIVE snapshot: the
/// attempt sent after `prepared` is on its final row (Uncertain), which then
/// blocks the next review by name.
#[test]
fn a_dropped_run_settles_the_live_account_never_the_prepared_snapshot() {
    let (_base, root) = project();
    let account = std::sync::Arc::new(Account::default());
    let journal = RunJournal::new(
        cleared(&root, "run-1"),
        "run-1".into(),
        Box::new(account.clone()),
    );
    journal.observe("prepared").unwrap();
    account.send();
    drop(journal);
    let rows = rows(&root);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["observation"]["state"], "Open");
    assert_eq!(rows[1]["phase"], "settled");
    assert_eq!(rows[1]["observation"]["state"], "Uncertain");
    assert_eq!(rows[1]["observation"]["unknown_calls"], 1);
    assert_eq!(rows[1]["lease"]["pid"], std::process::id());
    let dir = OwnedDir::open(&root).unwrap();
    let Err(Blocked::Exposed(exposures)) = clear(&dir, "run-2").unwrap() else {
        panic!("an uncertain settlement blocks the next review");
    };
    assert_eq!(exposures.runs[0].invocation, "run-1");
    assert_eq!(
        exposures.runs[0].exposure,
        super::super::Exposure::Uncertain
    );
}

/// The lease is exclusive while held (a second clear is Busy, in the host's
/// words), released when the Run's journal drops, and a clean settlement
/// clears the next review. The witness is the journal's exact bytes.
#[test]
fn busy_while_held_clear_after_settlement_and_the_witness_is_exact() {
    let (_base, root) = project();
    let first = cleared(&root, "run-1");
    let empty = first.journal().unwrap();
    assert_eq!(empty.length, 0);
    assert_eq!(empty.sha256, nika_event::source_id::sha256_hex(b""));
    let dir = OwnedDir::open(&root).unwrap();
    let busy = clear(&dir, "run-2").unwrap().unwrap_err();
    assert!(
        matches!(busy, Blocked::Busy { pid: Some(pid) } if pid == u64::from(std::process::id()))
    );
    assert!(
        busy.to_string()
            .contains("holds this project's cost lease: an unknown-cost Run in flight or a review waiting for its answer"),
        "{busy}"
    );
    let account = std::sync::Arc::new(Account::default());
    let journal = RunJournal::new(first, "run-1".into(), Box::new(account));
    journal.observe("prepared").unwrap();
    journal.settle().unwrap();
    drop(journal);
    let next = cleared(&root, "run-2");
    let bytes = std::fs::read(root.join(".nika").join(JOURNAL)).unwrap();
    let witness = next.journal().unwrap();
    assert_eq!(witness.length, u64::try_from(bytes.len()).unwrap());
    assert_eq!(witness.sha256, nika_event::source_id::sha256_hex(&bytes));
    assert_eq!(rows(&root).len(), 2, "a settled Run settles once");
}

/// P6 (moved with the host's file witness) · through the held root, a missing
/// output parent under `create_dirs: true` reaches the review (no bare ENOENT)
/// and nothing is created before the answer; without it the parent is named.
#[test]
fn a_write_parent_is_re_observed_never_created() {
    let (_base, root) = project();
    let cleared = cleared(&root, "review");
    let write = |path: &str, creates: bool| cleared.observe_file(Path::new(path), Some(creates));
    assert!(write("reports/q3.md", true).unwrap().is_none());
    std::fs::create_dir(root.join("out")).unwrap();
    assert!(write("out/deep/er/a.md", true).unwrap().is_none());
    assert!(!root.join("reports").exists(), "no pre-review mkdir");
    assert!(!root.join("out").join("deep").exists());
    assert!(
        write("out/a.md", false).unwrap().is_none(),
        "an existing parent is re-observed"
    );
    assert!(write("a.md", false).unwrap().is_none());
    let refused = write("reports/q3/a.md", false).unwrap_err().to_string();
    assert!(refused.contains("`reports` does not exist"), "{refused}");
    assert!(refused.contains("create_dirs: true"), "{refused}");
}

/// Whatever the write declares, an existing parent that is not a real
/// contained directory refuses: a symlink is never followed, a file never
/// becomes a directory. A read input is its bytes, bounded at 1 MiB.
#[test]
fn a_symlinked_or_file_parent_refuses_and_a_read_is_its_bounded_bytes() {
    let (_base, root) = project();
    let cleared = cleared(&root, "review");
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.join("linked")).unwrap();
    std::fs::write(root.join("plain"), "x").unwrap();
    for (path, creates) in [
        ("linked/a.md", true),
        ("linked/below/a.md", true),
        ("plain/a.md", true),
        ("plain/below/a.md", false),
    ] {
        let refused = cleared
            .observe_file(Path::new(path), Some(creates))
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("is not a contained directory"),
            "{refused}"
        );
    }
    assert!(!elsewhere.path().join("below").exists());
    std::fs::write(root.join("brief.md"), "the brief").unwrap();
    assert_eq!(
        cleared.observe_file(Path::new("brief.md"), None).unwrap(),
        Some(nika_event::source_id::sha256_hex(b"the brief"))
    );
    std::fs::write(root.join("big.md"), vec![b'x'; 1_048_577]).unwrap();
    let big = cleared.observe_file(Path::new("big.md"), None).unwrap_err();
    assert!(
        big.to_string().contains("exceeds the 1 MiB review bound"),
        "{big}"
    );
    assert!(cleared.observe_file(Path::new("absent.md"), None).is_err());
}

/// An account whose closure refuses until told otherwise, counting every call;
/// it reads `Closed` once a closure succeeded.
#[derive(Default)]
struct Refusing {
    refuse: AtomicBool,
    closes: std::sync::atomic::AtomicUsize,
    closed: AtomicBool,
}

impl RunAccount for std::sync::Arc<Refusing> {
    fn observation(&self) -> std::io::Result<serde_json::Value> {
        let state = if self.closed.load(Ordering::SeqCst) {
            "Closed"
        } else {
            "Open"
        };
        Ok(
            serde_json::json!({"schema": "nika/inference-cost-observation@1",
            "known_subtotal_nano_usd": "0", "unknown_calls": 0, "unknown_attempts": [],
            "attempts": [], "state": state}),
        )
    }
    fn close(&self, _why: &str) -> std::io::Result<()> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        if self.refuse.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("closure refused"));
        }
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn phases(root: &Path) -> Vec<serde_json::Value> {
    rows(root).iter().map(|row| row["phase"].clone()).collect()
}

/// A Run on a `Refusing` account, its `prepared` row written.
fn prepared(root: &Path, invocation: &str) -> (std::sync::Arc<Refusing>, RunJournal) {
    let account = std::sync::Arc::new(Refusing::default());
    let cleared = cleared(root, invocation);
    let journal = RunJournal::new(cleared, invocation.into(), Box::new(account.clone()));
    journal.observe("prepared").unwrap();
    (account, journal)
}

/// A settlement whose closure failed is never attempted again: not by a later
/// call once the cause is gone, not by Drop. The journal keeps its `prepared`
/// row only, which the next review records as unknown.
#[test]
fn a_failed_closure_is_never_retried_by_a_later_call_or_by_drop() {
    let (_base, root) = project();
    let (account, journal) = prepared(&root, "run-1");
    account.refuse.store(true, Ordering::SeqCst);
    assert!(journal.settle().is_err());
    account.refuse.store(false, Ordering::SeqCst);
    let again = journal
        .settle()
        .expect_err("an unproven attempt never reads as settled");
    assert!(again.to_string().contains("already attempted"), "{again}");
    drop(journal);
    assert_eq!(
        account.closes.load(Ordering::SeqCst),
        1,
        "one closure, ever"
    );
    assert_eq!(phases(&root), [serde_json::json!("prepared")]);
}

/// The same fence when the `settled` row cannot be appended (a second hard
/// link makes the leased journal ambiguous, and the append refuses): the
/// account closed once, no row landed, and once the link is gone neither a
/// later call nor Drop appends one.
#[test]
fn a_failed_settled_append_is_never_retried_by_a_later_call_or_by_drop() {
    let (_base, root) = project();
    let (account, journal) = prepared(&root, "run-1");
    let file = root.join(".nika").join(JOURNAL);
    let alias = root.join("ambiguous-journal");
    std::fs::hard_link(&file, &alias).unwrap();
    assert!(
        journal.settle().is_err(),
        "a linked journal refuses the append"
    );
    std::fs::remove_file(&alias).unwrap();
    let again = journal
        .settle()
        .expect_err("an unproven attempt never reads as settled");
    assert!(again.to_string().contains("already attempted"), "{again}");
    drop(journal);
    assert_eq!(
        account.closes.load(Ordering::SeqCst),
        1,
        "one closure, ever"
    );
    assert_eq!(phases(&root), [serde_json::json!("prepared")]);
}

/// A Run dropped before any settlement settles exactly once; after a proven
/// settlement a repeated call succeeds without acting, and Drop stays inert.
#[test]
fn an_early_drop_settles_once_and_a_settled_run_stays_settled() {
    let (_base, root) = project();
    let (dropped, journal) = prepared(&root, "run-1");
    drop(journal);
    assert_eq!(dropped.closes.load(Ordering::SeqCst), 1);
    let (settled, journal) = prepared(&root, "run-2");
    journal.settle().unwrap();
    journal.settle().unwrap();
    drop(journal);
    assert_eq!(settled.closes.load(Ordering::SeqCst), 1);
    let expected = ["prepared", "settled", "prepared", "settled"].map(serde_json::Value::from);
    assert_eq!(phases(&root), expected);
}

/// E21 on the inode: a journal renamed inside `.nika/`, or `.nika/` itself
/// renamed, with a replacement put at the old name, never receives the Run's
/// settlement. The original inode keeps `prepared` then `settled`, and the
/// replacement's bytes are untouched.
#[test]
fn settlement_stays_in_the_original_journal_after_its_name_is_replaced() {
    for replace_directory in [false, true] {
        let (_base, root) = project();
        let (_account, journal) = prepared(&root, "run-1");
        let current = root.join(".nika").join(JOURNAL);
        let original = if replace_directory {
            let old = root.join("old-nika");
            std::fs::rename(root.join(".nika"), &old).unwrap();
            std::fs::create_dir(root.join(".nika")).unwrap();
            old.join(JOURNAL)
        } else {
            let old = root.join(".nika").join("old-journal");
            std::fs::rename(&current, &old).unwrap();
            old
        };
        std::fs::write(&current, b"replacement must stay untouched\n").unwrap();
        journal.settle().unwrap();
        drop(journal);
        assert_eq!(
            std::fs::read(&current).unwrap(),
            b"replacement must stay untouched\n"
        );
        let rows: Vec<serde_json::Value> = std::fs::read_to_string(&original)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let phases: Vec<_> = rows.iter().map(|row| row["phase"].clone()).collect();
        assert_eq!(
            phases,
            ["prepared", "settled"],
            "directory={replace_directory}"
        );
    }
}
