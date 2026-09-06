// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

use nika_kernel::fs::{FsError, FsWriteDyn};

use crate::TokioFs;
use crate::write_new::StagedFile;

type TestResult = std::io::Result<()>;

#[test]
fn temporary_names_stay_distinct_under_ascii_case_folding() {
    for name in ["file", ".nika-tmp.1.0", ".NIKA-TMP.1.0", "..nika-tmp.1.0"] {
        let destination = Path::new("parent").join(name);
        let (_, temporary) = crate::tmp_sibling(&destination);
        let temporary_name = temporary.file_name().map(std::ffi::OsStr::as_encoded_bytes);
        assert!(
            temporary_name
                .is_some_and(|value| { !value[..2].eq_ignore_ascii_case(&name.as_bytes()[..2]) })
        );
        assert_eq!(temporary.parent(), destination.parent());
    }
}

#[test]
fn staging_name_cannot_be_the_requested_destination() -> TestResult {
    use std::sync::atomic::Ordering;

    let directory = tempfile::tempdir()?;
    let next = crate::TMP_COUNTER.load(Ordering::Relaxed);
    let destination = directory
        .path()
        .join(format!(".nika-tmp.{}.{}", std::process::id(), next));
    let staged = StagedFile::create(&destination, b"complete").map_err(std::io::Error::other)?;
    assert!(
        matches!(
            std::fs::symlink_metadata(&destination),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ),
        "staging must use a distinct name"
    );
    staged
        .publish(&destination)
        .map_err(std::io::Error::other)?;
    drop(staged);
    assert_eq!(std::fs::read(&destination)?, b"complete");
    let names = std::fs::read_dir(directory.path())?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    assert_eq!(
        names,
        vec![
            destination
                .file_name()
                .ok_or_else(|| std::io::Error::other("missing name"))?
                .to_os_string()
        ]
    );
    Ok(())
}

fn assert_names(directory: &Path, expected: &[&str]) -> std::io::Result<()> {
    let mut actual = std::fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    actual.sort();
    let mut expected: Vec<_> = expected.iter().map(|name| OsString::from(*name)).collect();
    expected.sort();
    assert_eq!(
        actual, expected,
        "all directory entries, including temporaries"
    );
    Ok(())
}

#[allow(
    clippy::disallowed_methods,
    reason = "two real filesystem writers start at one barrier on independent OS threads"
)]
fn start_writer(
    runtime: tokio::runtime::Runtime,
    destination: PathBuf,
    contents: Vec<u8>,
    barrier: Arc<Barrier>,
) -> std::thread::JoinHandle<Result<(), FsError>> {
    std::thread::spawn(move || {
        barrier.wait();
        runtime.block_on(TokioFs.write_new(&destination, &contents))
    })
}

#[test]
fn write_new_has_one_real_filesystem_winner() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("winner.bin");
    let first_bytes = vec![0x31; 16_384];
    let second_bytes = vec![0xb2; 16_384];
    // Build before either writer waits: a runtime error must not strand a peer.
    let first_runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let second_runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let barrier = Arc::new(Barrier::new(3));
    let first = start_writer(
        first_runtime,
        destination.clone(),
        first_bytes.clone(),
        Arc::clone(&barrier),
    );
    let second = start_writer(
        second_runtime,
        destination.clone(),
        second_bytes.clone(),
        Arc::clone(&barrier),
    );
    barrier.wait();
    let first = first
        .join()
        .map_err(|_| std::io::Error::other("first filesystem writer panicked"))?;
    let second = second
        .join()
        .map_err(|_| std::io::Error::other("second filesystem writer panicked"))?;

    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    let (winner, loser) = if first.is_ok() {
        (&first_bytes, &second)
    } else {
        (&second_bytes, &first)
    };
    assert!(
        matches!(loser, Err(FsError::AlreadyExists { .. })),
        "{loser:?}"
    );
    assert_eq!(std::fs::read(&destination)?, *winner);
    assert!(std::fs::symlink_metadata(&destination)?.is_file());
    assert_names(directory.path(), &["winner.bin"])?;
    Ok(())
}

#[test]
fn staging_keeps_the_destination_absent_until_complete_publication() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("published.bin");
    let contents = [0x00, 0xff, 0x80, b'\n', 0x7f];
    let staged = StagedFile::create(&destination, &contents).map_err(std::io::Error::other)?;

    assert!(matches!(
        std::fs::symlink_metadata(&destination),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ));
    let entries = std::fs::read_dir(directory.path())?.collect::<std::io::Result<Vec<_>>>()?;
    let [temporary] = entries.as_slice() else {
        return Err(std::io::Error::other(
            "staging must create exactly one private file",
        ));
    };
    assert_eq!(std::fs::read(temporary.path())?, contents);

    staged
        .publish(&destination)
        .map_err(std::io::Error::other)?;
    assert_eq!(std::fs::read(&destination)?, contents);
    assert_eq!(std::fs::read(temporary.path())?, contents);
    drop(staged);
    assert_eq!(std::fs::read(&destination)?, contents);
    assert_names(directory.path(), &["published.bin"])?;
    Ok(())
}

#[test]
fn a_peer_created_after_staging_survives_refusal_and_stage_cleanup() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("peer.bin");
    let staged = StagedFile::create(&destination, b"our bytes").map_err(std::io::Error::other)?;
    std::fs::write(&destination, b"peer bytes")?;

    let refused = staged.publish(&destination);
    assert!(
        matches!(refused, Err(FsError::AlreadyExists { .. })),
        "{refused:?}"
    );
    assert_eq!(std::fs::read(&destination)?, b"peer bytes");
    drop(staged);
    assert_eq!(std::fs::read(&destination)?, b"peer bytes");
    assert_names(directory.path(), &["peer.bin"])?;
    Ok(())
}

#[tokio::test]
async fn write_new_preserves_binary_content_and_refuses_an_existing_file() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("bytes.bin");
    let bytes = [0x00, 0xff, 0x7f, 0x80, b'\n', 0xfe];
    TokioFs
        .write_new(&destination, &bytes)
        .await
        .map_err(std::io::Error::other)?;
    assert_eq!(std::fs::read(&destination)?, bytes);

    let refused = TokioFs.write_new(&destination, b"replacement").await;
    assert!(
        matches!(refused, Err(FsError::AlreadyExists { .. })),
        "{refused:?}"
    );
    assert_eq!(std::fs::read(&destination)?, bytes);
    assert_names(directory.path(), &["bytes.bin"])?;
    Ok(())
}

#[tokio::test]
async fn write_new_refuses_an_existing_empty_file() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("empty");
    std::fs::write(&destination, [])?;

    let refused = TokioFs.write_new(&destination, b"not empty").await;
    assert!(
        matches!(refused, Err(FsError::AlreadyExists { .. })),
        "{refused:?}"
    );
    assert!(std::fs::read(&destination)?.is_empty());
    assert!(std::fs::symlink_metadata(&destination)?.is_file());
    assert_names(directory.path(), &["empty"])?;
    Ok(())
}

#[tokio::test]
async fn write_new_preserves_an_existing_directory_and_its_child() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("occupied");
    std::fs::create_dir(&destination)?;
    std::fs::write(destination.join("sentinel"), b"peer bytes")?;

    let refused = TokioFs.write_new(&destination, b"replacement").await;
    assert!(
        matches!(refused, Err(FsError::AlreadyExists { .. })),
        "{refused:?}"
    );
    assert!(std::fs::symlink_metadata(&destination)?.is_dir());
    assert_eq!(std::fs::read(destination.join("sentinel"))?, b"peer bytes");
    assert_names(directory.path(), &["occupied"])?;
    assert_names(&destination, &["sentinel"])?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn write_new_preserves_a_symlink_and_its_existing_target() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("link");
    let target = directory.path().join("target");
    std::fs::write(&target, b"peer bytes")?;
    std::os::unix::fs::symlink("target", &destination)?;

    let refused = TokioFs.write_new(&destination, b"replacement").await;
    assert!(
        matches!(refused, Err(FsError::AlreadyExists { .. })),
        "{refused:?}"
    );
    assert!(
        std::fs::symlink_metadata(&destination)?
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&destination)?, Path::new("target"));
    assert_eq!(std::fs::read(&target)?, b"peer bytes");
    assert_names(directory.path(), &["link", "target"])?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn write_new_preserves_a_dangling_symlink_without_creating_its_target() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("link");
    let target = directory.path().join("missing");
    std::os::unix::fs::symlink("missing", &destination)?;

    let refused = TokioFs.write_new(&destination, b"replacement").await;
    assert!(
        matches!(refused, Err(FsError::AlreadyExists { .. })),
        "{refused:?}"
    );
    assert!(
        std::fs::symlink_metadata(&destination)?
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&destination)?, Path::new("missing"));
    assert!(matches!(
        std::fs::symlink_metadata(&target),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ));
    assert_names(directory.path(), &["link"])?;
    Ok(())
}

#[tokio::test]
async fn write_still_replaces_a_file_created_exclusively() -> TestResult {
    let directory = tempfile::tempdir()?;
    let destination = directory.path().join("replace.bin");
    TokioFs
        .write_new(&destination, b"first")
        .await
        .map_err(std::io::Error::other)?;

    TokioFs
        .write(&destination, b"replacement")
        .await
        .map_err(std::io::Error::other)?;
    assert_eq!(std::fs::read(&destination)?, b"replacement");
    assert!(std::fs::symlink_metadata(&destination)?.is_file());
    assert_names(directory.path(), &["replace.bin"])?;
    Ok(())
}
