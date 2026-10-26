// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One artifact's bytes, checked against the sha256 its journal event records before any of
//! them leaves. [`verify_whole`] reads the whole file once and hashes it against the record (a
//! mismatch is the route's 409), keeping the SHA-256 of each fixed-size chunk on the way. The
//! stream ([`VerifiedBody`]) reads the same chunks again and sends one only when it hashes to
//! the digest kept for it. Every byte that leaves is then a byte of the recorded file. A file
//! changed in between ends the stream at the first chunk that differs, short of its
//! `Content-Length`, so no client receives a complete body the journal did not record.

use std::convert::Infallible;
use std::fs::File;
use std::future::Future as _;
use std::io::{self, Read as _, Seek as _, SeekFrom};
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use hyper::body::{Frame, SizeHint};
use sha2::{Digest as _, Sha256};
use tokio::task::JoinHandle;

/// One chunk: one blocking read, one digest kept, bounded memory per stream.
const CHUNK: u64 = 256 * 1024;

/// The SHA-256 of each `CHUNK` of `file`, read from its start, when the file holds exactly
/// `size` bytes hashing to `expected`; `None` for a shorter, longer or different file. The
/// cursor is back at the start on return.
///
/// # Errors
/// A seek or read failure.
pub(super) fn verify_whole(
    file: &mut File,
    size: u64,
    expected: &[u8; 32],
) -> io::Result<Option<Vec<[u8; 32]>>> {
    file.seek(SeekFrom::Start(0))?;
    let mut whole = Sha256::new();
    let mut chunks = Vec::new();
    let mut remaining = size;
    while remaining > 0 {
        let chunk = read_chunk(file, remaining.min(CHUNK))?;
        if chunk.is_empty() {
            return Ok(None);
        }
        whole.update(&chunk);
        chunks.push(Sha256::digest(&chunk).into());
        remaining = remaining.saturating_sub(chunk.len() as u64);
    }
    let longer = !read_chunk(file, 1)?.is_empty();
    file.seek(SeekFrom::Start(0))?;
    Ok((!longer && whole.finalize().as_slice() == expected).then_some(chunks))
}

/// At most `want` bytes from the cursor; fewer only at the end of the file.
fn read_chunk(file: &mut File, want: u64) -> io::Result<Vec<u8>> {
    let mut chunk = Vec::with_capacity(usize::try_from(want).unwrap_or(0));
    file.by_ref().take(want).read_to_end(&mut chunk)?;
    Ok(chunk)
}

/// The response body of one verified artifact: `size` bytes from `file`'s start, each chunk
/// read and hashed off the runtime threads and sent only when it is the chunk [`verify_whole`]
/// kept the digest of.
pub(super) struct VerifiedBody {
    state: State,
    remaining: u64,
    chunks: std::vec::IntoIter<[u8; 32]>,
}

enum State {
    /// Ready for the next read.
    Idle(File),
    /// One blocking read in flight, `None` once its chunk differs; dropping the body drops it.
    Reading(JoinHandle<io::Result<(File, Option<Vec<u8>>)>>),
    /// Complete, or ended short of the record.
    Done,
}

impl VerifiedBody {
    /// Stream `size` bytes of `file`, positioned at its start, against `chunks`.
    pub(super) fn new(file: File, size: u64, chunks: Vec<[u8; 32]>) -> Self {
        let state = if size == 0 {
            State::Done
        } else {
            State::Idle(file)
        };
        Self {
            state,
            remaining: size,
            chunks: chunks.into_iter(),
        }
    }

    /// End the body here, short of its `Content-Length`: hyper then aborts the response.
    fn cut(&mut self) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        self.state = State::Done;
        self.remaining = 0;
        Poll::Ready(None)
    }
}

/// Read the next `want` bytes and keep them only when they hash to `digest`.
fn checked_read(
    mut file: File,
    want: u64,
    digest: [u8; 32],
) -> io::Result<(File, Option<Vec<u8>>)> {
    let chunk = read_chunk(&mut file, want)?;
    let same = chunk.len() as u64 == want && Sha256::digest(&chunk).as_slice() == digest;
    Ok((file, same.then_some(chunk)))
}

impl hyper::body::Body for VerifiedBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        loop {
            match std::mem::replace(&mut this.state, State::Done) {
                State::Done => return Poll::Ready(None),
                State::Idle(file) => {
                    let Some(digest) = this.chunks.next() else {
                        return this.cut();
                    };
                    let want = this.remaining.min(CHUNK);
                    this.state = State::Reading(tokio::task::spawn_blocking(move || {
                        checked_read(file, want, digest)
                    }));
                }
                State::Reading(mut read) => match Pin::new(&mut read).poll(cx) {
                    Poll::Pending => {
                        this.state = State::Reading(read);
                        return Poll::Pending;
                    }
                    Poll::Ready(Ok(Ok((file, Some(chunk))))) => {
                        this.remaining = this.remaining.saturating_sub(chunk.len() as u64);
                        if this.remaining > 0 {
                            this.state = State::Idle(file);
                        }
                        return Poll::Ready(Some(Ok(Frame::data(Bytes::from(chunk)))));
                    }
                    // A chunk that differs, a short read, a read error or a lost worker.
                    Poll::Ready(_) => return this.cut(),
                },
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        matches!(self.state, State::Done)
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.remaining)
    }
}
