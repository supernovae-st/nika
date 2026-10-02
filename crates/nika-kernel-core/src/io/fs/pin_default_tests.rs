// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pinned read's default: a backend that states no pin refuses with the
//! typed `PinUnavailable` naming the path, completes without awaiting, and
//! calls none of its own read members (no unpinned read served in its place).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

use bytes::Bytes;

use super::{FsError, FsRead, FsReadDyn};

#[derive(Default)]
struct Calls(AtomicUsize);

impl Calls {
    fn hit(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Default)]
struct UnpinnedBaseFs(Calls);

// Deliberately only the four pre-existing members: no read_pinned override.
impl FsRead for UnpinnedBaseFs {
    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn read(&self, _: &Path) -> Result<Bytes, FsError> {
        self.0.hit();
        Ok(Bytes::from_static(b"unpinned"))
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn read_to_string(&self, _: &Path) -> Result<String, FsError> {
        self.0.hit();
        Ok("unpinned".to_owned())
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn exists(&self, _: &Path) -> bool {
        self.0.hit();
        true
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        self.0.hit();
        Ok(path.to_path_buf())
    }
}

#[derive(Default)]
struct UnpinnedSendFs(Calls);

// This also obtains the base trait through trait_variant's blanket impl.
impl FsReadDyn for UnpinnedSendFs {
    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn read(&self, _: &Path) -> Result<Bytes, FsError> {
        self.0.hit();
        Ok(Bytes::from_static(b"unpinned"))
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn read_to_string(&self, _: &Path) -> Result<String, FsError> {
        self.0.hit();
        Ok("unpinned".to_owned())
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn exists(&self, _: &Path) -> bool {
        self.0.hit();
        true
    }

    /// CANCEL SAFETY: one synchronous counter increment; no await or I/O.
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        self.0.hit();
        Ok(path.to_path_buf())
    }
}

fn complete_now<F: Future>(future: F) -> Result<F::Output, &'static str> {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => Ok(output),
        Poll::Pending => Err("the default pin must refuse without awaiting IO"),
    }
}

fn send_future<F: Future + Send>(future: F) -> F {
    future
}

#[test]
fn a_base_backend_without_a_pin_refuses_typed_without_any_io() -> std::io::Result<()> {
    let fs = UnpinnedBaseFs::default();
    let path = Path::new("data/judged.txt");
    let refused = complete_now(FsRead::read_pinned(&fs, path)).map_err(std::io::Error::other)?;
    assert!(
        matches!(
            &refused,
            Err(FsError::PinUnavailable { path: named }) if named == "data/judged.txt"
        ),
        "{refused:?}"
    );
    assert_eq!(
        fs.0.count(),
        0,
        "no unpinned read and no probe in its place"
    );
    Ok(())
}

#[test]
fn a_send_backend_and_its_base_blanket_refuse_typed_without_any_io() -> std::io::Result<()> {
    let fs = UnpinnedSendFs::default();
    let path = Path::new("data/judged.txt");
    let refused = complete_now(send_future(FsReadDyn::read_pinned(&fs, path)))
        .map_err(std::io::Error::other)?;
    assert!(
        matches!(&refused, Err(FsError::PinUnavailable { .. })),
        "{refused:?}"
    );
    let refused = complete_now(FsRead::read_pinned(&fs, path)).map_err(std::io::Error::other)?;
    assert!(
        matches!(&refused, Err(FsError::PinUnavailable { .. })),
        "{refused:?}"
    );
    assert_eq!(fs.0.count(), 0);

    // The counter is live: the unpinned member stays callable and is counted.
    let bytes = complete_now(FsReadDyn::read(&fs, path))
        .map_err(std::io::Error::other)?
        .map_err(std::io::Error::other)?;
    assert_eq!(&bytes[..], b"unpinned");
    assert_eq!(fs.0.count(), 1);
    Ok(())
}
