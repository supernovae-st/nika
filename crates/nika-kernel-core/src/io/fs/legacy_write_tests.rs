// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

use super::{FsError, FsWrite, FsWriteDyn};

#[derive(Default)]
struct Calls {
    write: AtomicUsize,
    create_dir_all: AtomicUsize,
    remove_file: AtomicUsize,
}

impl Calls {
    fn snapshot(&self) -> [usize; 3] {
        [
            self.write.load(Ordering::Relaxed),
            self.create_dir_all.load(Ordering::Relaxed),
            self.remove_file.load(Ordering::Relaxed),
        ]
    }
}

#[derive(Default)]
struct LegacyBaseFs(Calls);

// Deliberately only the three pre-existing members: no write_new override.
impl FsWrite for LegacyBaseFs {
    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn write(&self, _: &Path, _: &[u8]) -> Result<(), FsError> {
        self.0.write.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn create_dir_all(&self, _: &Path) -> Result<(), FsError> {
        self.0.create_dir_all.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn remove_file(&self, _: &Path) -> Result<(), FsError> {
        self.0.remove_file.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Default)]
struct LegacySendFs(Calls);

// This also obtains the base trait through trait_variant's blanket impl.
impl FsWriteDyn for LegacySendFs {
    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn write(&self, _: &Path, _: &[u8]) -> Result<(), FsError> {
        self.0.write.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn create_dir_all(&self, _: &Path) -> Result<(), FsError> {
        self.0.create_dir_all.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn remove_file(&self, _: &Path) -> Result<(), FsError> {
        self.0.remove_file.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

fn complete_now<F: Future>(future: F) -> Result<F::Output, &'static str> {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => Ok(output),
        Poll::Pending => Err("the unsupported default must refuse without awaiting IO"),
    }
}

fn send_future<F: Future + Send>(future: F) -> F {
    future
}

#[test]
fn legacy_base_backend_refuses_exclusive_publication_without_any_io() -> std::io::Result<()> {
    let fs = LegacyBaseFs::default();
    let path = Path::new("not-created/nested/output.bin");
    let refused = complete_now(FsWrite::write_new(&fs, path, &[0x00, 0xff]))
        .map_err(std::io::Error::other)?;
    assert!(matches!(refused, Err(FsError::Io { .. })), "{refused:?}");
    assert_eq!(fs.0.snapshot(), [0, 0, 0]);

    // The counters are live and the original three operations remain callable.
    complete_now(FsWrite::write(&fs, path, b"legacy"))
        .map_err(std::io::Error::other)?
        .map_err(std::io::Error::other)?;
    complete_now(FsWrite::create_dir_all(&fs, path))
        .map_err(std::io::Error::other)?
        .map_err(std::io::Error::other)?;
    complete_now(FsWrite::remove_file(&fs, path))
        .map_err(std::io::Error::other)?
        .map_err(std::io::Error::other)?;
    assert_eq!(fs.0.snapshot(), [1, 1, 1]);
    Ok(())
}

#[test]
fn legacy_send_backend_and_its_base_blanket_refuse_without_any_io() -> std::io::Result<()> {
    let fs = LegacySendFs::default();
    let path = Path::new("not-created/nested/output.bin");
    let refused = complete_now(send_future(FsWriteDyn::write_new(&fs, path, b"send")))
        .map_err(std::io::Error::other)?;
    assert!(matches!(refused, Err(FsError::Io { .. })), "{refused:?}");
    assert_eq!(fs.0.snapshot(), [0, 0, 0]);

    let refused =
        complete_now(FsWrite::write_new(&fs, path, b"base")).map_err(std::io::Error::other)?;
    assert!(matches!(refused, Err(FsError::Io { .. })), "{refused:?}");
    assert_eq!(fs.0.snapshot(), [0, 0, 0]);

    complete_now(FsWriteDyn::write(&fs, path, b"legacy"))
        .map_err(std::io::Error::other)?
        .map_err(std::io::Error::other)?;
    complete_now(FsWriteDyn::create_dir_all(&fs, path))
        .map_err(std::io::Error::other)?
        .map_err(std::io::Error::other)?;
    complete_now(FsWriteDyn::remove_file(&fs, path))
        .map_err(std::io::Error::other)?
        .map_err(std::io::Error::other)?;
    assert_eq!(fs.0.snapshot(), [1, 1, 1]);
    Ok(())
}
