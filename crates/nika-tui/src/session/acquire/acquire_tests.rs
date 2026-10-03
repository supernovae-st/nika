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
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let name = format!("nika-tui-acquire-{tag}-{}-{nanos}", std::process::id());
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
