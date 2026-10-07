// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reserved log, observed from the outside: file bytes, lengths, modes and
//! directory entries are read independently of the handle's own results.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::{FileExt as _, MetadataExt as _, PermissionsExt as _};
use std::path::Path;
use std::sync::{Arc, Barrier};

use super::super::OwnedDir;
use super::{LOCK, ReservedLog};

#[derive(Clone, Copy)]
enum Fault {
    /// Half the record reaches the file, then the write fails.
    Short,
    /// The whole record reaches the file, then synchronization fails.
    Sync,
}

thread_local! {
    static FAULT: Cell<Option<Fault>> = const { Cell::new(None) };
    /// Fail the encoder sink's nth write (0-based) with a non-capacity error.
    static SINK_FAULT: Cell<Option<u32>> = const { Cell::new(None) };
}

/// The test seam of the encoder sink: one injected non-capacity failure.
pub(super) fn sink_fault() -> io::Result<()> {
    SINK_FAULT.with(|cell| match cell.get() {
        Some(0) => {
            cell.set(None);
            Err(io::Error::other("injected sink failure"))
        }
        Some(n) => {
            cell.set(Some(n - 1));
            Ok(())
        }
        None => Ok(()),
    })
}

/// The test seam of `store`: one injected failure, consumed by the next record.
pub(super) fn inject(file: &File, bytes: &[u8], at: u64) -> io::Result<()> {
    match FAULT.with(Cell::take) {
        None => Ok(()),
        Some(Fault::Short) => {
            file.write_all_at(&bytes[..bytes.len() / 2], at)?;
            Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "injected short write",
            ))
        }
        Some(Fault::Sync) => {
            file.write_all_at(bytes, at)?;
            Err(io::Error::other("injected sync failure"))
        }
    }
}

/// A private container below a fresh temporary root.
fn container() -> (tempfile::TempDir, OwnedDir) {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let owned = OwnedDir::open(root.path()).unwrap();
    (root, owned)
}

/// The error of a refused reservation (the handle itself has no Debug).
fn refused(result: io::Result<ReservedLog>) -> io::Error {
    match result {
        Ok(_) => panic!("the reservation must be refused"),
        Err(error) => error,
    }
}

fn line(text: &str) -> impl FnOnce(&mut dyn Write) -> io::Result<()> + '_ {
    move |out| out.write_all(text.as_bytes())
}

fn spaces(n: usize) -> Vec<u8> {
    vec![b' '; n]
}

/// Every name in the directory, sorted, read without the handle.
fn listing(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn records_are_synchronized_in_order_and_the_reservation_is_kept_whole() {
    let (root, dir) = container();
    let mut log = dir
        .reserve_private_log("a.log", 4096, 65536, 512, 8)
        .unwrap();
    assert_eq!(log.append_encoded(line("{\"n\":1}")).unwrap(), Some(8));
    assert_eq!(log.append_encoded(line("é\"x")).unwrap(), Some(5));
    assert_eq!(
        log.finish_encoded(line("{\"close\":true}")).unwrap(),
        Some(15)
    );
    let bytes = fs::read(root.path().join("a.log")).unwrap();
    let mut expected = b"{\"n\":1}\n\xc3\xa9\"x\n{\"close\":true}\n".to_vec();
    expected.extend(spaces(4096 - expected.len()));
    assert_eq!(
        bytes, expected,
        "ordered records, then the untouched space padding"
    );
    let file = fs::metadata(root.path().join("a.log")).unwrap();
    assert_eq!(
        file.len(),
        4096,
        "finish keeps the full logical reservation"
    );
    assert_eq!(file.mode() & 0o777, 0o600);
    assert_eq!(file.nlink(), 1);
    assert_eq!(listing(root.path()), [LOCK, "a.log"]);
    assert_eq!(
        fs::metadata(root.path().join(LOCK)).unwrap().mode() & 0o777,
        0o600
    );
}

#[test]
fn an_overflowing_record_writes_nothing_even_when_the_encoder_swallows_it() {
    let (root, dir) = container();
    let mut log = dir.reserve_private_log("a.log", 64, 1024, 16, 8).unwrap();
    let before = fs::read(root.path().join("a.log")).unwrap();
    // 48 ordinary bytes: 47 of payload plus the line feed.
    let swallowed = log
        .append_encoded(|out| {
            let _ = out.write_all(&[b'x'; 48]);
            Ok(())
        })
        .unwrap();
    assert_eq!(swallowed, None);
    assert_eq!(fs::read(root.path().join("a.log")).unwrap(), before);
    assert_eq!(
        log.append_encoded(|out| out.write_all(&[b'y'; 47]))
            .unwrap(),
        Some(48)
    );
    // The ordinary room is used up; the closing room is still whole.
    assert_eq!(log.append_encoded(line("z")).unwrap(), None);
    assert_eq!(
        log.finish_encoded(|out| out.write_all(&[b'c'; 16]))
            .unwrap(),
        None
    );
    assert_eq!(
        log.finish_encoded(|out| out.write_all(&[b'c'; 15]))
            .unwrap(),
        Some(16)
    );
    let mut expected = vec![b'y'; 47];
    expected.push(b'\n');
    expected.extend([b'c'; 15]);
    expected.push(b'\n');
    assert_eq!(fs::read(root.path().join("a.log")).unwrap(), expected);
}

#[test]
fn an_encoder_error_fails_the_log_and_no_encoder_is_called_again() {
    let (root, dir) = container();
    let mut log = dir.reserve_private_log("a.log", 256, 1024, 32, 8).unwrap();
    let error = log
        .append_encoded(|out| {
            out.write_all(b"partial")?;
            Err(io::Error::other("encoder failed"))
        })
        .unwrap_err();
    assert_eq!(error.to_string(), "encoder failed");
    assert_eq!(fs::read(root.path().join("a.log")).unwrap(), spaces(256));
    let called = Cell::new(false);
    assert!(
        log.append_encoded(|_| {
            called.set(true);
            Ok(())
        })
        .is_err()
    );
    assert!(
        log.finish_encoded(|_| {
            called.set(true);
            Ok(())
        })
        .is_err()
    );
    assert!(!called.get(), "a failed log never calls an encoder");
}

#[test]
fn a_closed_log_refuses_without_calling_the_encoder() {
    let (_root, dir) = container();
    let mut log = dir.reserve_private_log("a.log", 256, 1024, 32, 8).unwrap();
    assert_eq!(log.finish_encoded(line("end")).unwrap(), Some(4));
    let called = Cell::new(false);
    for finish in [false, true] {
        let encode = |_: &mut dyn Write| {
            called.set(true);
            Ok(())
        };
        let refused = if finish {
            log.finish_encoded(encode)
        } else {
            log.append_encoded(encode)
        };
        assert!(refused.is_err());
    }
    assert!(!called.get());
}

#[test]
fn short_writes_and_sync_failures_fail_the_log_without_retry() {
    for (fault, written) in [
        (Fault::Short, &b"abcd"[..]),
        (Fault::Sync, &b"abcdefgh\n"[..]),
    ] {
        let (root, dir) = container();
        let mut log = dir.reserve_private_log("a.log", 128, 1024, 16, 8).unwrap();
        FAULT.with(|cell| cell.set(Some(fault)));
        assert!(log.append_encoded(line("abcdefgh")).is_err());
        // The uncertain effect stays as it is: no rewrite, no second attempt.
        let bytes = fs::read(root.path().join("a.log")).unwrap();
        assert_eq!(&bytes[..written.len()], written);
        assert!(bytes[written.len()..].iter().all(|b| *b == b' '));
        assert_eq!(bytes.len(), 128);
        let called = Cell::new(false);
        assert!(
            log.append_encoded(|_| {
                called.set(true);
                Ok(())
            })
            .is_err()
        );
        assert!(!called.get());
    }
}

#[test]
fn quota_counts_every_regular_length_including_failed_and_control_files() {
    let (root, dir) = container();
    fs::write(root.path().join(".gitignore"), "*\n").unwrap();
    fs::set_permissions(
        root.path().join(".gitignore"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let mut first = dir.reserve_private_log("a.log", 1000, 2002, 10, 8).unwrap();
    FAULT.with(|cell| cell.set(Some(Fault::Sync)));
    assert!(first.append_encoded(line("x")).is_err());
    // 2 (.gitignore) + 1000 (failed, still charged) + 1000 fits exactly; one byte more does not.
    let full = refused(dir.reserve_private_log("b.log", 1001, 2002, 10, 8));
    assert_eq!(full.kind(), io::ErrorKind::StorageFull);
    assert_eq!(listing(root.path()), [".gitignore", LOCK, "a.log"]);
    dir.reserve_private_log("b.log", 1000, 2002, 10, 8).unwrap();
    let after = refused(dir.reserve_private_log("c.log", 2, 2002, 1, 8));
    assert_eq!(after.kind(), io::ErrorKind::StorageFull);
    assert_eq!(listing(root.path()), [".gitignore", LOCK, "a.log", "b.log"]);
    assert_eq!(fs::metadata(root.path().join("a.log")).unwrap().len(), 1000);
}

#[test]
fn the_entry_bound_includes_the_lock_and_the_new_file() {
    let (root, dir) = container();
    dir.reserve_private_log("a.log", 16, 1024, 1, 3).unwrap();
    dir.reserve_private_log("b.log", 16, 1024, 1, 4).unwrap();
    let full = refused(dir.reserve_private_log("c.log", 16, 1024, 1, 3));
    assert_eq!(full.kind(), io::ErrorKind::StorageFull);
    assert_eq!(listing(root.path()), [LOCK, "a.log", "b.log"]);
}

#[test]
fn an_existing_name_is_refused_and_left_untouched() {
    let (root, dir) = container();
    let mut log = dir.reserve_private_log("a.log", 32, 1024, 4, 8).unwrap();
    assert_eq!(log.append_encoded(line("kept")).unwrap(), Some(5));
    let before = fs::read(root.path().join("a.log")).unwrap();
    let collision = refused(dir.reserve_private_log("a.log", 32, 1024, 4, 8));
    assert_eq!(collision.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(root.path().join("a.log")).unwrap(), before);
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the cross-writer race needs real OS threads at one deterministic barrier"
)]
fn a_held_lock_refuses_at_once_and_concurrent_writers_never_over_reserve() {
    let (root, dir) = container();
    let held = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.path().join(LOCK))
        .unwrap();
    fs::set_permissions(root.path().join(LOCK), fs::Permissions::from_mode(0o600)).unwrap();
    held.lock().unwrap();
    let busy = refused(dir.reserve_private_log("a.log", 16, 1024, 1, 8));
    assert_eq!(busy.kind(), io::ErrorKind::WouldBlock);
    assert_eq!(listing(root.path()), [LOCK]);
    held.unlock().unwrap();

    // Eight writers race for a container that admits three reservations.
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let dir = dir.try_clone().unwrap();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                dir.reserve_private_log(&format!("w{i}.log"), 100, 300, 10, 64)
                    .map(|_| ())
                    .map_err(|error| error.kind())
            })
        })
        .collect();
    for handle in handles {
        if let Err(kind) = handle.join().unwrap() {
            assert!(matches!(
                kind,
                io::ErrorKind::WouldBlock | io::ErrorKind::StorageFull
            ));
        }
    }
    let total: u64 = listing(root.path())
        .iter()
        .map(|name| fs::metadata(root.path().join(name)).unwrap().len())
        .sum();
    assert!(
        total <= 300,
        "logical lengths never exceed the container: {total}"
    );
    let logs = listing(root.path()).len() - 1;
    assert!((1..=3).contains(&logs), "{logs} reservations");
}

#[test]
fn unsafe_entries_and_containers_are_refused_without_writing() {
    use std::os::unix::fs::symlink;

    type Plant = fn(&Path);
    let cases: [(&str, Plant); 5] = [
        ("symlink", |p| {
            symlink("/etc/hosts", p.join("link")).unwrap();
        }),
        ("fifo", |p| {
            nix::unistd::mkfifo(
                &p.join("pipe"),
                nix::sys::stat::Mode::from_bits_truncate(0o600),
            )
            .unwrap();
        }),
        ("directory", |p| fs::create_dir(p.join("sub")).unwrap()),
        ("hard link", |p| {
            fs::write(p.join("one"), "x").unwrap();
            fs::set_permissions(p.join("one"), fs::Permissions::from_mode(0o600)).unwrap();
            fs::hard_link(p.join("one"), p.join("two")).unwrap();
        }),
        ("group readable", |p| {
            fs::write(p.join("open"), "x").unwrap();
            fs::set_permissions(p.join("open"), fs::Permissions::from_mode(0o640)).unwrap();
        }),
    ];
    for (label, plant) in cases {
        let (root, dir) = container();
        plant(root.path());
        let before = listing(root.path());
        let refused = dir.reserve_private_log("a.log", 16, 1024, 1, 8);
        assert!(refused.is_err(), "{label} must be refused");
        let after: Vec<String> = listing(root.path())
            .into_iter()
            .filter(|name| name != LOCK)
            .collect();
        assert_eq!(after, before, "{label}: nothing written");
    }
    let (root, dir) = container();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let open = refused(dir.reserve_private_log("a.log", 16, 1024, 1, 8));
    assert_eq!(open.kind(), io::ErrorKind::PermissionDenied);
    assert!(
        listing(root.path()).is_empty(),
        "a non-private container gains nothing"
    );
}

#[test]
fn invalid_bounds_and_names_are_refused_before_any_entry() {
    let (root, dir) = container();
    for (name, file, container, closing, entries) in [
        ("a.log", 16, 1024, 16, 8),
        ("a.log", 16, 1024, 0, 8),
        ("a.log", 2048, 1024, 1, 8),
        ("a.log", 16, 1024, 1, 1),
        (LOCK, 16, 1024, 1, 8),
        ("a/b", 16, 1024, 1, 8),
        ("..", 16, 1024, 1, 8),
    ] {
        let refused = dir.reserve_private_log(name, file, container, closing, entries);
        assert!(
            refused.is_err(),
            "{name} {file} {container} {closing} {entries}"
        );
    }
    assert!(listing(root.path()).is_empty());
}

#[test]
fn a_held_directory_keeps_its_authority_after_its_ancestor_moves() {
    let root = tempfile::tempdir().unwrap();
    let before = root.path().join("before");
    fs::create_dir(&before).unwrap();
    fs::create_dir(before.join("cap")).unwrap();
    fs::set_permissions(before.join("cap"), fs::Permissions::from_mode(0o700)).unwrap();
    let dir = OwnedDir::open(&before.join("cap")).unwrap();
    fs::rename(&before, root.path().join("after")).unwrap();
    fs::create_dir_all(before.join("cap")).unwrap();
    let mut log = dir.reserve_private_log("a.log", 32, 1024, 4, 8).unwrap();
    assert_eq!(log.append_encoded(line("held")).unwrap(), Some(5));
    assert!(
        listing(&before.join("cap")).is_empty(),
        "the visible new path is untouched"
    );
    let moved = fs::read(root.path().join("after/cap/a.log")).unwrap();
    assert_eq!(&moved[..5], b"held\n");
}

#[test]
fn dropping_a_log_performs_no_io() {
    let (root, dir) = container();
    let mut log: ReservedLog = dir.reserve_private_log("a.log", 64, 1024, 8, 8).unwrap();
    assert_eq!(log.append_encoded(line("one")).unwrap(), Some(4));
    let before = fs::read(root.path().join("a.log")).unwrap();
    let modified = fs::metadata(root.path().join("a.log"))
        .unwrap()
        .modified()
        .unwrap();
    drop(log);
    assert_eq!(fs::read(root.path().join("a.log")).unwrap(), before);
    let after = fs::metadata(root.path().join("a.log")).unwrap();
    assert_eq!(after.modified().unwrap(), modified);
    assert_eq!(after.len(), 64);
}

#[test]
fn ownership_is_the_effective_uid_of_the_process() {
    use nix::unistd::Uid;

    let (root, _dir) = container();
    fs::write(root.path().join("mine"), "x").unwrap();
    fs::set_permissions(root.path().join("mine"), fs::Permissions::from_mode(0o600)).unwrap();
    let metadata = fs::metadata(root.path().join("mine")).unwrap();
    let me = Uid::effective().as_raw();
    assert_eq!(metadata.uid(), me);
    assert!(super::private(&metadata, me).is_ok());
    // The same private single-link file is refused for any other owner. A real foreign-owned
    // entry needs a privileged chown, which this unprivileged test does not attempt.
    let other = me.wrapping_add(1);
    let refused = super::private(&metadata, other).unwrap_err();
    assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn a_swallowed_sink_failure_fails_the_log_and_never_stores_a_prefix() {
    let (root, dir) = container();
    let mut log = dir.reserve_private_log("a.log", 256, 1024, 32, 8).unwrap();
    let before = fs::read(root.path().join("a.log")).unwrap();
    SINK_FAULT.with(|cell| cell.set(Some(1)));
    let swallowed = log.append_encoded(|out| {
        out.write_all(b"prefix")?;
        let _ = out.write_all(b"-rest");
        Ok(())
    });
    assert!(
        swallowed.is_err(),
        "a non-capacity sink failure is never a stored record"
    );
    assert_eq!(fs::read(root.path().join("a.log")).unwrap(), before);
    let called = Cell::new(false);
    assert!(
        log.append_encoded(|_| {
            called.set(true);
            Ok(())
        })
        .is_err()
    );
    assert!(
        log.finish_encoded(|_| {
            called.set(true);
            Ok(())
        })
        .is_err()
    );
    assert!(!called.get(), "a failed log never calls an encoder");
}

#[test]
fn a_sink_failure_is_never_masked_as_retryable_overflow() {
    let (root, dir) = container();
    let mut log = dir.reserve_private_log("a.log", 64, 1024, 16, 8).unwrap();
    let before = fs::read(root.path().join("a.log")).unwrap();
    SINK_FAULT.with(|cell| cell.set(Some(0)));
    let both = log.append_encoded(|out| {
        let _ = out.write_all(b"x");
        let _ = out.write_all(&[b'y'; 100]);
        Ok(())
    });
    assert!(
        both.is_err(),
        "failure then overflow is a failure, not Ok(None)"
    );
    assert_eq!(fs::read(root.path().join("a.log")).unwrap(), before);
    assert!(log.append_encoded(|out| out.write_all(b"later")).is_err());
}
