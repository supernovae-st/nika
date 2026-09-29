// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Descriptor custody is independent of mutable directory-entry names.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use super::*;

fn fixture() -> (tempfile::TempDir, OwnedDir, Lease, Writer) {
    let root = tempfile::tempdir().unwrap();
    let project = OwnedDir::open(root.path()).unwrap();
    let nika = project.create_below(&[".nika"]).unwrap();
    let writer = Writer::this_process();
    let Taken::Held(lease) = take_at(&project, &nika, &writer).unwrap() else {
        panic!("fresh project is free");
    };
    (root, nika, lease, writer)
}

#[test]
fn fold_derives_once_on_the_leased_inode_after_its_name_is_replaced() {
    let (root, nika, lease, writer) = fixture();
    let row = serde_json::json!({"schema":"nika/run-cost-observation@1", "invocation":"killed",
        "phase":"prepared", "lease": writer.json(),
        "observation":{"attempts":[], "billed_nano_usd":null, "known_subtotal_nano_usd":"0",
            "limit_nano_usd":null, "overridden_defaults":[null,null], "refusal":null,
            "schema":"nika/inference-cost-observation@1", "state":"Open",
            "unknown_attempts":[], "unknown_calls":0, "unknown_cost":null}})
    .to_string();
    lease.append_row(&row).unwrap();
    let original = root.path().join(".nika/old-journal");
    let current = root.path().join(".nika").join(JOURNAL);
    std::fs::rename(&current, &original).unwrap();
    std::fs::write(&current, b"replacement\n").unwrap();
    let first = lease.fold_as(&nika, &writer, "next").unwrap();
    assert_eq!(first.runs.len(), 1);
    assert!(matches!(first.runs[0].exposure, Exposure::Unknown { .. }));
    let after = lease.read().unwrap();
    assert!(after.starts_with(row.as_bytes()));
    assert_eq!(std::str::from_utf8(&after).unwrap().lines().count(), 2);
    assert_eq!(lease.fold_as(&nika, &writer, "another").unwrap(), first);
    assert_eq!(lease.read().unwrap(), after);
    assert_eq!(std::fs::read(&original).unwrap(), after);
    assert_eq!(std::fs::read(&current).unwrap(), b"replacement\n");
}

#[test]
fn concurrent_held_appends_keep_whole_rows_and_the_torn_prefix() {
    let (root, _, lease, _) = fixture();
    std::fs::write(root.path().join(".nika").join(JOURNAL), b"torn\xe2").unwrap();
    std::thread::scope(|scope| {
        for writer in 0..4 {
            let lease = &lease;
            scope.spawn(move || {
                for row in 0..25 {
                    lease.append_row(&format!("{writer}:{row}")).unwrap();
                }
            });
        }
    });
    let bytes = lease.read().unwrap();
    assert!(bytes.starts_with(b"torn\xe2\n"));
    let lines: Vec<_> = bytes.split(|b| *b == b'\n').collect();
    assert_eq!(lines.len(), 102);
    let actual: std::collections::BTreeSet<_> = lines[1..101]
        .iter()
        .map(|line| std::str::from_utf8(line).unwrap().to_owned())
        .collect();
    let expected = (0..4)
        .flat_map(|writer| (0..25).map(move |row| format!("{writer}:{row}")))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn a_new_hard_link_refuses_append_without_modifying_either_name() {
    let (root, _, lease, _) = fixture();
    lease.append_row("preserved").unwrap();
    let current = root.path().join(".nika").join(JOURNAL);
    let alias = root.path().join("alias");
    std::fs::hard_link(&current, &alias).unwrap();
    let before = lease.read().unwrap();
    assert!(lease.append_row("refused").is_err());
    assert_eq!(std::fs::read(current).unwrap(), before);
    assert_eq!(std::fs::read(alias).unwrap(), before);
}

#[test]
fn leased_reads_keep_the_existing_size_bound() {
    let (root, _, lease, _) = fixture();
    std::fs::write(
        root.path().join(".nika").join(JOURNAL),
        vec![b'x'; 1_048_577],
    )
    .unwrap();
    assert!(lease.read().unwrap_err().to_string().contains("read bound"));
}
