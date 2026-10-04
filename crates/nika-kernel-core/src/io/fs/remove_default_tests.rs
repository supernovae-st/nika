// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The provided `remove_regular_file` of a backend that predates it: it
//! refuses at once, without awaiting and without calling any other operation
//! (so it never approximates the check with a raw `remove_file`), for a base
//! implementation, a `Send` implementation and the base trait that
//! `trait_variant` derives from the latter.

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
struct PriorBaseFs(Calls);

// Deliberately only the three required members: no removal override.
impl FsWrite for PriorBaseFs {
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
struct PriorSendFs(Calls);

// This also obtains the base trait through trait_variant's blanket impl.
impl FsWriteDyn for PriorSendFs {
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
fn a_prior_base_backend_refuses_regular_removal_without_any_io() -> std::io::Result<()> {
    let fs = PriorBaseFs::default();
    let path = Path::new("not-created/nested/victim.bin");
    let refused = complete_now(send_future(FsWrite::remove_regular_file(&fs, path)))
        .map_err(std::io::Error::other)?;
    assert!(matches!(refused, Err(FsError::Io { .. })), "{refused:?}");
    assert_eq!(fs.0.snapshot(), [0, 0, 0], "no write, mkdir or raw unlink");

    // The counters are live and the three required operations remain callable.
    complete_now(FsWrite::write(&fs, path, b"prior"))
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
fn a_prior_send_backend_and_its_base_blanket_refuse_regular_removal_without_any_io()
-> std::io::Result<()> {
    let fs = PriorSendFs::default();
    let path = Path::new("not-created/nested/victim.bin");
    let refused = complete_now(send_future(FsWriteDyn::remove_regular_file(&fs, path)))
        .map_err(std::io::Error::other)?;
    assert!(matches!(refused, Err(FsError::Io { .. })), "{refused:?}");
    assert_eq!(fs.0.snapshot(), [0, 0, 0]);

    let refused =
        complete_now(FsWrite::remove_regular_file(&fs, path)).map_err(std::io::Error::other)?;
    assert!(matches!(refused, Err(FsError::Io { .. })), "{refused:?}");
    assert_eq!(fs.0.snapshot(), [0, 0, 0]);

    complete_now(FsWriteDyn::write(&fs, path, b"prior"))
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
