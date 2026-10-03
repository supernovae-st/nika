// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A file the run named is read now, below the root only, never through a
//! symlink, within its cap, witnessed. A journal is captured once and judged
//! by the one verifier; its verdict is the run's only when the journal names
//! exactly that execution, one start naming the run's source hash, and the
//! end its receipt named.

use super::*;

const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";
const OTHER: &str = "01a0ef11-0212-70de-a8b3-99de94270000";
/// The journal chain's genesis tag (`nika_dap::chain::CHAIN_GENESIS`).
const GENESIS: &[u8] = b"nika-trace-v1";

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, b| {
            let _ = std::fmt::Write::write_fmt(&mut hex, format_args!("{b:02x}"));
            hex
        })
}

fn id(uuid: &str) -> ExecutionId {
    serde_json::from_value(serde_json::json!({ "uuid": uuid })).expect("an execution")
}

/// `(execution, kind, workflow_sha256)` frames of a journal.
type Frames<'a> = &'a [(&'a str, &'a str, Option<&'a str>)];

/// A chained journal of `(execution, kind, workflow_sha256)` frames, and its head.
fn journal(frames: &[(&str, &str, Option<&str>)]) -> (String, String) {
    let mut chain = sha256_hex(GENESIS);
    let mut out = String::new();
    for (n, (exec, kind, sha)) in frames.iter().enumerate() {
        let mut fields = vec![serde_json::json!({"key": "workflow", "value": "copy"})];
        if let Some(sha) = sha {
            fields.push(serde_json::json!({"key": "workflow_sha256", "value": sha}));
        }
        if *kind == "workflow_completed" {
            fields.push(serde_json::json!({"key": "status", "value": "succeeded"}));
        }
        let line = serde_json::json!({
            "chain": chain, "correlation": null, "execution": {"uuid": exec}, "fields": fields,
            "id": {"uuid": format!("01a0ef11-03a1-73d9-a2bc-{n:012x}")},
            "kind": kind, "run": null, "timestamp": n,
        })
        .to_string();
        chain = sha256_hex(line.as_bytes());
        out.push_str(&line);
        out.push('\n');
    }
    (out, chain)
}

/// A directory under the temp dir, removed on drop (the session tests' idiom).
struct Room(PathBuf);

impl Room {
    fn new(tag: &str) -> Self {
        // Tests run in parallel threads of one process and the clock is
        // coarser than their starts: the count keeps two rooms apart.
        static MADE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let made = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!(
            "nika-tui-acquire-{tag}-{}-{nanos}-{made}",
            std::process::id()
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&path).expect("room");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn room() -> Room {
    let room = Room::new("room");
    std::fs::create_dir_all(room.path().join("out")).expect("out");
    std::fs::create_dir_all(room.path().join(".nika/traces")).expect("traces");
    std::fs::write(room.path().join("out/copy.md"), "# Brief\n").expect("file");
    room
}

fn expect(sha: Option<&str>, head: Option<&str>, len: Option<u64>) -> Expect {
    Expect {
        execution: id(EXEC),
        workflow_sha256: sha.map(str::to_owned),
        chain_head: head.map(str::to_owned),
        chain_len: len,
    }
}

#[test]
fn a_named_file_is_read_now_and_witnessed() {
    let room = room();
    let read = fetch(room.path(), "./out/copy.md");
    assert_eq!(read.bytes(), Some(&b"# Brief\n"[..]));
    assert_eq!(read.witness(), Some(Witness::of(b"# Brief\n").0.as_str()));
    assert!(read.why().is_none() && !read.missing());
    let gone = fetch(room.path(), "out/gone.md");
    assert!(gone.missing() && gone.bytes().is_none(), "{gone:?}");
}

#[test]
fn a_path_out_of_the_root_a_symlink_or_an_oversize_file_is_refused() {
    let room = room();
    let outside = Room::new("outside");
    std::fs::write(outside.path().join("secret.md"), "secret").expect("secret");
    let link = room.path().join("out/link.md");
    std::os::unix::fs::symlink(outside.path().join("secret.md"), link).expect("link");
    for path in ["../secret.md", "/etc/hosts", "", "out/link.md"] {
        let read = fetch(room.path(), path);
        assert!(
            read.bytes().is_none() && !read.missing(),
            "{path}: {read:?}"
        );
    }
    let big = vec![b'a'; usize::try_from(FILE_CAP).expect("cap") + 1];
    std::fs::write(room.path().join("out/big.md"), big).expect("big");
    let read = fetch(room.path(), "out/big.md");
    assert!(
        read.why().is_some_and(|w| w.contains("larger than")),
        "{read:?}"
    );
}

#[test]
fn a_journal_of_exactly_the_run_binds_its_verdict() {
    let room = room();
    let frames = [
        (EXEC, "workflow_started", Some("aa")),
        (EXEC, "workflow_completed", None),
    ];
    let (raw, head) = journal(&frames);
    std::fs::write(room.path().join(".nika/traces/t.ndjson"), &raw).expect("trace");
    let proven = prove(
        room.path(),
        ".nika/traces/t.ndjson",
        &expect(Some("aa"), Some(&head), Some(2)),
    );
    assert_eq!(
        (proven.tier(), proven.terminal()),
        (Some("ok"), Some("succeeded"))
    );
    assert!(proven.unbound().is_empty(), "{:?}", proven.unbound());
    assert_eq!(
        proven.witness(),
        Some(Witness::of(raw.as_bytes()).0.as_str())
    );
}

#[test]
fn a_journal_that_is_not_exactly_the_run_never_binds() {
    let room = room();
    let cases: [(Frames<'_>, &str); 4] = [
        (
            &[(OTHER, "workflow_started", Some("aa"))],
            "another execution",
        ),
        (
            &[
                (EXEC, "workflow_started", Some("aa")),
                (OTHER, "task_started", None),
            ],
            "2 executions",
        ),
        (
            &[
                (EXEC, "workflow_started", Some("aa")),
                (EXEC, "workflow_started", Some("aa")),
            ],
            "2 starts",
        ),
        (&[(EXEC, "workflow_started", Some("bb"))], "other bytes"),
    ];
    for (frames, why) in cases {
        let (raw, head) = journal(frames);
        std::fs::write(room.path().join(".nika/traces/t.ndjson"), &raw).expect("trace");
        let len = u64::try_from(frames.len()).expect("len");
        let proven = prove(
            room.path(),
            ".nika/traces/t.ndjson",
            &expect(Some("aa"), Some(&head), Some(len)),
        );
        assert!(
            proven.unbound().iter().any(|w| w.contains(why)),
            "{why}: {:?}",
            proven.unbound()
        );
    }
    let (raw, _) = journal(&[(EXEC, "workflow_started", Some("aa"))]);
    std::fs::write(room.path().join(".nika/traces/t.ndjson"), &raw).expect("trace");
    for (receipt, why) in [
        (expect(Some("aa"), Some("ff"), Some(1)), "ends elsewhere"),
        (expect(Some("aa"), None, None), "no receipt"),
        (expect(None, None, None), "start was not observed"),
    ] {
        let proven = prove(room.path(), ".nika/traces/t.ndjson", &receipt);
        assert!(
            proven.unbound().iter().any(|w| w.contains(why)),
            "{why}: {:?}",
            proven.unbound()
        );
    }
}

#[test]
fn a_journal_out_of_the_traces_a_symlink_or_over_the_cap_is_never_judged() {
    let room = room();
    let (raw, _) = journal(&[(EXEC, "workflow_started", Some("aa"))]);
    std::fs::write(room.path().join("t.ndjson"), &raw).expect("elsewhere");
    let linked = room.path().join(".nika/traces/linked.ndjson");
    std::os::unix::fs::symlink(room.path().join("t.ndjson"), linked).expect("link");
    let big = vec![b'a'; usize::try_from(JOURNAL_CAP).expect("cap") + 1];
    std::fs::write(room.path().join(".nika/traces/big.ndjson"), big).expect("big");
    let probe = expect(Some("aa"), None, None);
    for trace in [
        "t.ndjson",
        "../t.ndjson",
        ".nika/traces/../../t.ndjson",
        ".nika/traces/linked.ndjson",
        ".nika/traces/big.ndjson",
        ".nika/traces/gone.ndjson",
    ] {
        let refused = prove(room.path(), trace, &probe);
        assert!(
            refused.verdict().is_none() && refused.why().is_some(),
            "{trace}: {refused:?}"
        );
    }
}

/// The project root the operator selected may be reached through a link:
/// it is resolved once, a file the run wrote and its journal are read below
/// it; a link INSIDE the project stays refused.
#[test]
fn a_project_root_reached_through_a_link_is_read_and_its_children_stay_unfollowed() {
    let room = room();
    let outer = Room::new("outer");
    let linked = outer.path().join("project");
    std::os::unix::fs::symlink(room.path(), &linked).expect("a linked root");
    let read = fetch(&linked, "out/copy.md");
    assert_eq!(read.bytes(), Some(&b"# Brief\n"[..]), "{read:?}");
    let (raw, head) = journal(&[(EXEC, "workflow_started", Some("aa"))]);
    std::fs::write(room.path().join(".nika/traces/t.ndjson"), &raw).expect("trace");
    let proven = prove(
        &linked,
        ".nika/traces/t.ndjson",
        &expect(Some("aa"), Some(&head), Some(1)),
    );
    assert!(
        proven.unbound().is_empty() && proven.why().is_none(),
        "{proven:?}"
    );
    std::os::unix::fs::symlink(room.path().join("out"), room.path().join("alias"))
        .expect("child link");
    let child = fetch(&linked, "alias/copy.md");
    assert!(child.bytes().is_none() && !child.missing(), "{child:?}");
}

/// A start that names no source hash is said missing, never « other bytes ».
#[test]
fn a_start_naming_no_hash_is_not_compared_never_other_bytes() {
    let room = room();
    let (raw, head) = journal(&[(EXEC, "workflow_started", None)]);
    std::fs::write(room.path().join(".nika/traces/t.ndjson"), &raw).expect("trace");
    let proven = prove(
        room.path(),
        ".nika/traces/t.ndjson",
        &expect(Some("aa"), Some(&head), Some(1)),
    );
    assert!(
        proven
            .unbound()
            .iter()
            .any(|w| w.contains("names no source hash")),
        "{proven:?}"
    );
    assert!(
        !proven.unbound().iter().any(|w| w.contains("other bytes")),
        "{proven:?}"
    );
}

/// A child journal as the local child route writes it: frames with NO
/// execution, `starts` starts naming `sha`, one task, then `end` (a
/// terminal kind, or none); its head.
fn child_journal(sha: Option<&str>, starts: usize, end: Option<&str>) -> (String, String) {
    let mut chain = sha256_hex(GENESIS);
    let mut out = String::new();
    let mut frames: Vec<(&str, Vec<serde_json::Value>)> = Vec::new();
    for _ in 0..starts {
        let mut fields = vec![serde_json::json!({"key": "workflow", "value": "child"})];
        fields.extend(sha.map(|s| serde_json::json!({"key": "workflow_sha256", "value": s})));
        frames.push(("workflow_started", fields));
    }
    frames.push((
        "task_started",
        vec![serde_json::json!({"key": "task", "value": "greet"})],
    ));
    frames.push((
        "task_completed",
        vec![serde_json::json!({"key": "task", "value": "greet"})],
    ));
    if let Some(end) = end {
        frames.push((end, Vec::new()));
    }
    for (n, (kind, fields)) in frames.into_iter().enumerate() {
        let line = serde_json::json!({
            "chain": chain, "correlation": null, "fields": fields,
            "id": {"uuid": format!("01a0ef11-03a1-73d9-a2bc-{n:012x}")},
            "kind": kind, "run": null, "timestamp": n,
        })
        .to_string();
        chain = sha256_hex(line.as_bytes());
        out.push_str(&line);
        out.push('\n');
    }
    (out, chain)
}

/// The relation a parent's settle names for `trace`, read as the fold reads it.
fn relation(trace: &str, head: Option<&str>, sha: Option<&str>, outcome: &str) -> ChildRun {
    let mut row =
        serde_json::json!({"target": "./child.nika", "trace_id": trace, "outcome": outcome});
    if let Some(head) = head {
        row["chain_head"] = head.into();
    }
    if let Some(sha) = sha {
        row["def_hash"] = sha.into();
    }
    let line = serde_json::json!({
        "correlation": null, "execution": {"uuid": EXEC},
        "fields": [{"key": "task", "value": "call"}, {"key": "child", "value": row.to_string()}],
        "id": {"uuid": "01a0ef11-03a1-73d9-a2bc-00000000ffff"},
        "kind": "task_completed", "run": null, "timestamp": 9,
    })
    .to_string();
    let Some(nika_display::run_story::RunFrame::Event(event)) =
        nika_display::run_story::RunFrame::decode(&line)
    else {
        panic!("a settle frame");
    };
    ChildRun::of(&event).expect("a relation")
}

/// The words of every engagement that does not hold.
fn unheld(read: &ChildRead) -> Vec<&str> {
    (read.compared().iter())
        .filter(|(holds, _)| !holds)
        .map(|(_, words)| words.as_str())
        .collect()
}

/// The child journal is read for real though it records no execution: the
/// verdict is the verifier's over the captured bytes; the head it computes,
/// the source its one start names and its terminal hold against what the
/// parent's frame named; its identity and length are said, never compared,
/// so the relation is not bound whole by their absence.
#[test]
fn a_child_journal_without_an_execution_is_read_and_compared() {
    let room = room();
    let (raw, head) = child_journal(Some("cd"), 1, Some("workflow_completed"));
    std::fs::write(room.path().join(".nika/traces/child.ndjson"), &raw).expect("trace");
    let named = relation("child.ndjson", Some(&head), Some("cd"), "success");
    let read = read_child(room.path(), &named);
    let proven = read.proven();
    assert!(
        proven.why().is_none() && proven.verdict().is_some(),
        "{read:?}"
    );
    let holds: Vec<&str> = (read.compared().iter())
        .filter(|(holds, _)| *holds)
        .map(|(_, w)| w.as_str())
        .collect();
    for word in ["head holds", "source holds", "outcome holds"] {
        assert!(
            holds.iter().any(|w| w.starts_with(word)),
            "{word}: {read:?}"
        );
    }
    let unheld = unheld(&read);
    assert_eq!(unheld.len(), 2, "{read:?}");
    assert!(
        unheld.iter().all(|w| w.contains("not compared")),
        "{read:?}"
    );
    assert!(
        unheld.iter().any(|w| w.contains("not recorded")),
        "{read:?}"
    );
    assert_eq!(proven.unbound(), unheld, "unbound lists what does not hold");
    assert!(!unheld.iter().any(|w| w.contains("differs")), "{read:?}");
    assert_eq!(proven.terminal(), Some("succeeded"));
    assert_eq!(
        read.rows().len(),
        1,
        "its one task, folded from the same bytes"
    );
    assert_eq!(read.rows()[0].id, "greet");
    let outer = Room::new("child-outer");
    let linked = outer.path().join("project");
    std::os::unix::fs::symlink(room.path(), &linked).expect("a linked root");
    let through = read_child(&linked, &named);
    assert_eq!(unheld_of(&through), 2, "{through:?}");
}

fn unheld_of(read: &ChildRead) -> usize {
    unheld(read).len()
}

/// The outcome the parent's frame named is compared with the journal's
/// terminal: success only with `succeeded`; failure only with a terminal
/// the producer calls a failure; a missing terminal, or a contradiction,
/// leaves the relation not bound even when head and source hold.
#[test]
fn the_named_outcome_is_compared_with_the_journal_terminal() {
    let room = room();
    for (end, file) in [
        (Some("workflow_completed"), "ok.ndjson"),
        (Some("workflow_failed"), "failed.ndjson"),
        (None, "open.ndjson"),
    ] {
        let (raw, _) = child_journal(Some("cd"), 1, end);
        std::fs::write(room.path().join(".nika/traces").join(file), &raw).expect("trace");
    }
    let head_of = |file: &str| {
        let raw = std::fs::read(room.path().join(".nika/traces").join(file)).expect("raw");
        let last = raw
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .next_back()
            .expect("line");
        sha256_hex(last)
    };
    for (file, outcome, said) in [
        ("ok.ndjson", "success", "outcome holds"),
        ("failed.ndjson", "failure", "outcome holds"),
        ("failed.ndjson", "success", "outcome differs"),
        ("ok.ndjson", "failure", "outcome differs"),
        ("open.ndjson", "success", "outcome not compared"),
    ] {
        let named = relation(file, Some(&head_of(file)), Some("cd"), outcome);
        let read = read_child(room.path(), &named);
        assert!(
            read.compared().iter().any(|(_, w)| w.starts_with(said)),
            "{file} {outcome} {said}: {read:?}"
        );
        let bound_parts = (read.compared().iter()).filter(|(holds, _)| *holds).count();
        if said != "outcome holds" {
            assert!(
                read.proven()
                    .unbound()
                    .iter()
                    .any(|w| w.starts_with("outcome")),
                "{file} {outcome}: an outcome that does not hold never binds: {read:?}"
            );
            assert_eq!(bound_parts, 2, "head and source still hold: {read:?}");
        }
    }
}

/// Another head, other bytes, several starts, several identities or a
/// corrupted journal: each says its own reason, none is bound.
#[test]
fn a_child_journal_that_is_not_the_named_one_says_why() {
    let room = room();
    let (raw, head) = child_journal(Some("cd"), 1, Some("workflow_completed"));
    std::fs::write(room.path().join(".nika/traces/child.ndjson"), &raw).expect("trace");
    let (two, two_head) = child_journal(Some("cd"), 2, Some("workflow_completed"));
    std::fs::write(room.path().join(".nika/traces/two.ndjson"), &two).expect("two");
    let (ids, ids_head) = journal(&[
        (EXEC, "workflow_started", Some("cd")),
        (OTHER, "workflow_completed", None),
    ]);
    std::fs::write(room.path().join(".nika/traces/ids.ndjson"), &ids).expect("ids");
    let mut torn = raw.clone();
    torn.insert(10, 'x');
    std::fs::write(room.path().join(".nika/traces/torn.ndjson"), &torn).expect("torn");
    for (trace, named, sha, reason) in [
        ("child.ndjson", "ff".repeat(32), "cd", "head differs"),
        ("child.ndjson", head.clone(), "ee", "source differs"),
        ("two.ndjson", two_head, "cd", "2 starts"),
        ("ids.ndjson", ids_head, "cd", "2 executions"),
    ] {
        let read = read_child(
            room.path(),
            &relation(trace, Some(&named), Some(sha), "success"),
        );
        assert!(
            read.proven().unbound().iter().any(|w| w.contains(reason)),
            "{trace} {reason}: {read:?}"
        );
    }
    let torn = read_child(
        room.path(),
        &relation("torn.ndjson", Some(&head), Some("cd"), "success"),
    );
    assert!(
        torn.proven().exit() != Some(0) || torn.compared().iter().any(|(h, _)| !h),
        "{torn:?}"
    );
    let unnamed = read_child(
        room.path(),
        &relation("child.ndjson", None, None, "success"),
    );
    for word in ["head not compared", "source not compared"] {
        assert!(
            unnamed
                .compared()
                .iter()
                .any(|(holds, w)| !holds && w.starts_with(word)),
            "{word}: {unnamed:?}"
        );
    }
}

/// The journal's name is the producer's whole file name, local, under
/// `.nika/traces`: anything else is refused before a byte is read, no link
/// below the root is followed, what is not a regular file is never opened, the
/// cap holds.
#[test]
fn a_child_journal_name_that_is_not_local_is_refused_unread() {
    let room = room();
    let (raw, _) = child_journal(Some("cd"), 1, Some("workflow_completed"));
    std::fs::write(room.path().join("elsewhere.ndjson"), &raw).expect("elsewhere");
    std::fs::create_dir_all(room.path().join(".nika/traces/sub")).expect("sub");
    std::fs::write(room.path().join(".nika/traces/sub/x.ndjson"), &raw).expect("sub");
    std::os::unix::fs::symlink(
        room.path().join("elsewhere.ndjson"),
        room.path().join(".nika/traces/linked.ndjson"),
    )
    .expect("link");
    // Not a regular file: a directory under a journal's name.
    std::fs::create_dir_all(room.path().join(".nika/traces/dir.ndjson")).expect("not a file");
    let big = vec![b'a'; usize::try_from(JOURNAL_CAP).expect("cap") + 1];
    std::fs::write(room.path().join(".nika/traces/big.ndjson"), big).expect("big");
    for name in [
        "/etc/passwd",
        "../elsewhere.ndjson",
        "../../elsewhere.ndjson",
        "sub/x.ndjson",
        ".",
        "",
        "linked.ndjson",
        "dir.ndjson",
        "big.ndjson",
        "gone.ndjson",
    ] {
        let read = read_child(room.path(), &relation(name, None, None, "success"));
        assert!(
            read.proven().verdict().is_none() && read.proven().why().is_some(),
            "{name}: {read:?}"
        );
        assert!(read.rows().is_empty(), "{name}");
    }
}
