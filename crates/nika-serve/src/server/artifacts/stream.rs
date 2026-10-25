// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One artifact's bytes, verified twice against the sha256 its journal event records: once
//! whole before the response starts ([`verify_whole`], a mismatch is the route's 409), then
//! again while they stream ([`VerifiedBody`]). The stream holds its last chunk back until
//! every byte it sent hashed to the record; a file changed in between ends the stream short
//! of its `Content-Length`, so no client receives a complete body the journal never recorded.

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

/// One blocking read: bounded memory per stream and a few hops per mebibyte.
const CHUNK: u64 = 256 * 1024;

/// Whether `file` holds exactly `size` bytes hashing to `expected`, read from its start; the
/// cursor is back at the start on return. A shorter or longer file is not the recorded one.
///
/// # Errors
/// A seek or read failure.
pub(super) fn verify_whole(file: &mut File, size: u64, expected: &[u8; 32]) -> io::Result<bool> {
    file.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut remaining = size;
    while remaining > 0 {
        let chunk = read_chunk(file, remaining.min(CHUNK))?;
        if chunk.is_empty() {
            return Ok(false);
        }
        hasher.update(&chunk);
        remaining = remaining.saturating_sub(chunk.len() as u64);
    }
    let longer = !read_chunk(file, 1)?.is_empty();
    file.seek(SeekFrom::Start(0))?;
    Ok(!longer && hasher.finalize().as_slice() == expected)
}

/// At most `want` bytes from the cursor; empty only at the end of the file.
fn read_chunk(file: &mut File, want: u64) -> io::Result<Vec<u8>> {
    let mut chunk = Vec::new();
    loop {
        match file.by_ref().take(want).read_to_end(&mut chunk) {
            Ok(_) => return Ok(chunk),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

/// The response body of one verified artifact: `size` bytes from `file`'s start, each chunk
/// read off the runtime threads, the last one sent only once the whole hashed to `expected`.
pub(super) struct VerifiedBody {
    state: State,
    remaining: u64,
    hasher: Sha256,
    expected: [u8; 32],
}

enum State {
    /// Ready for the next read.
    Idle(File),
    /// One blocking read in flight; dropping the body drops its result.
    Reading(JoinHandle<io::Result<(File, Vec<u8>)>>),
    /// Complete, or aborted short of the record.
    Done,
}

impl VerifiedBody {
    /// Stream `size` bytes of `file`, positioned at its start.
    pub(super) fn new(file: File, size: u64, expected: [u8; 32]) -> Self {
        let state = if size == 0 {
            State::Done
        } else {
            State::Idle(file)
        };
        Self {
            state,
            remaining: size,
            hasher: Sha256::new(),
            expected,
        }
    }

    /// Fold one read into the stream: the chunk to send, or `None` to end the body here — a
    /// file shorter than its record, or a whole that does not hash to it (its last chunk
    /// withheld). `Some` with the file means more remains.
    fn accept(&mut self, file: File, chunk: Vec<u8>) -> Option<(Bytes, Option<File>)> {
        let len = chunk.len() as u64;
        if chunk.is_empty() || len > self.remaining {
            return None;
        }
        self.hasher.update(&chunk);
        self.remaining -= len;
        if self.remaining > 0 {
            return Some((Bytes::from(chunk), Some(file)));
        }
        let whole = std::mem::take(&mut self.hasher).finalize();
        (whole.as_slice() == self.expected).then(|| (Bytes::from(chunk), None))
    }
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
                State::Idle(mut file) => {
                    let want = this.remaining.min(CHUNK);
                    this.state = State::Reading(tokio::task::spawn_blocking(move || {
                        read_chunk(&mut file, want).map(|chunk| (file, chunk))
                    }));
                }
                State::Reading(mut read) => match Pin::new(&mut read).poll(cx) {
                    Poll::Pending => {
                        this.state = State::Reading(read);
                        return Poll::Pending;
                    }
                    Poll::Ready(Ok(Ok((file, chunk)))) => {
                        let Some((bytes, next)) = this.accept(file, chunk) else {
                            return Poll::Ready(None);
                        };
                        if let Some(file) = next {
                            this.state = State::Idle(file);
                        }
                        return Poll::Ready(Some(Ok(Frame::data(bytes))));
                    }
                    // A read error or a lost worker: end short of the record.
                    Poll::Ready(_) => return Poll::Ready(None),
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
