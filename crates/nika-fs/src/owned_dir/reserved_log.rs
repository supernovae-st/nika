// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A bounded private log reserved whole below a held directory.
//!
//! [`OwnedDir::reserve_private_log`] admits one new file under a single
//! nonblocking lock shared by every cooperating writer of that directory: it
//! inventories the directory through its held descriptor (bounded while
//! iterating), charges every regular file's logical length, creates the file
//! exclusively and fills its whole reservation with spaces before returning.
//! The reservation is kept after finish: nothing is truncated, refunded,
//! renamed or deleted, by success, failure or drop, until an operator removes
//! the file. This is logical-length accounting for cooperating writers, not a
//! physical disk quota or an isolation boundary against the same user.
//!
//! Ownership is the process's effective uid: the held directory and every
//! opened entry (lock, existing files, the new log) must belong to it.

use std::fs::{File, TryLockError};
use std::io::{self, Write};
use std::os::unix::fs::{FileExt as _, MetadataExt as _};

use nix::dir::Dir;
use nix::fcntl::{OFlag, openat};
use nix::sys::stat::Mode;
use nix::unistd::Uid;

use super::{FILE_MODE, OwnedDir, io_error, validate_component};

/// The one lock every cooperating writer of a directory takes; never a log name.
const LOCK: &str = ".reserved-log.lock";
/// Initialization writes at most this many bytes per call.
const CHUNK: usize = 64 * 1024;

/// One reserved private log: an opaque handle with no path or descriptor getter.
///
/// Records are written at an internal offset inside the reservation (never at
/// end of file). Dropping it closes the descriptor and performs no other I/O.
pub struct ReservedLog {
    file: File,
    offset: u64,
    size: u64,
    closing: u64,
    state: State,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Open,
    Closed,
    Poisoned,
}

impl OwnedDir {
    /// Reserve `file_bytes` for one new private log `name` in this held directory,
    /// whose regular files may total at most `container_bytes` and hold at most
    /// `entry_limit` entries, the new file and the lock included. `closing_bytes`
    /// of the reservation are kept for [`ReservedLog::finish_encoded`].
    ///
    /// The directory must be private and owned by the process's effective uid;
    /// every existing entry must be a regular, private, single-link file of
    /// that owner. Busy, unsafe, full or
    /// colliding containers are refused without writing; a failure after the
    /// exclusive creation leaves the partial file in place, charged to later
    /// reservations, never removed.
    ///
    /// # Errors
    /// Invalid bounds or name ([`io::ErrorKind::InvalidInput`]), another writer
    /// holding the lock ([`io::ErrorKind::WouldBlock`]), an unsafe directory or
    /// entry, an exhausted quota or entry bound ([`io::ErrorKind::StorageFull`]),
    /// an existing name, or a failed write/sync.
    pub fn reserve_private_log(
        &self,
        name: &str,
        file_bytes: u64,
        container_bytes: u64,
        closing_bytes: u64,
        entry_limit: usize,
    ) -> io::Result<ReservedLog> {
        validate_component(name)?;
        if name == LOCK
            || closing_bytes == 0
            || closing_bytes >= file_bytes
            || file_bytes > container_bytes
            || entry_limit < 2
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "reserved log: invalid name or bounds",
            ));
        }
        let owner = Uid::effective().as_raw();
        let directory = self.fd.metadata()?;
        if !directory.is_dir() || directory.mode() & 0o077 != 0 || directory.uid() != owner {
            return Err(unsafe_entry("directory is not private to this user"));
        }
        let lock = self.open_file(
            LOCK,
            OFlag::O_CREAT | OFlag::O_RDWR | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        )?;
        private(&lock.metadata()?, owner)?;
        lock.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => io::Error::new(
                io::ErrorKind::WouldBlock,
                "reserved log: another writer holds the directory",
            ),
            TryLockError::Error(error) => error,
        })?;
        let used = self.inventory(owner, entry_limit)?;
        if used
            .checked_add(file_bytes)
            .is_none_or(|total| total > container_bytes)
        {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "reserved log: container quota exhausted",
            ));
        }
        let fd = openat(
            &self.fd,
            name,
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_RDWR | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            FILE_MODE,
        )
        .map_err(io_error)?;
        let file = File::from(fd);
        private(&file.metadata()?, owner)?;
        initialize(&file, file_bytes)?;
        file.sync_all()?;
        self.fd.sync_all()?;
        drop(lock);
        Ok(ReservedLog {
            file,
            offset: 0,
            size: file_bytes,
            closing: closing_bytes,
            state: State::Open,
        })
    }

    /// Sum the logical length of every entry, refusing unsafe ones, and stop
    /// while iterating once the entry bound leaves no room for one more file.
    fn inventory(&self, owner: u32, entry_limit: usize) -> io::Result<u64> {
        let mut dir = Dir::openat(
            &self.fd,
            ".",
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(io_error)?;
        let mut entries = 0usize;
        let mut used = 0u64;
        for entry in dir.iter() {
            let entry = entry.map_err(io_error)?;
            let name = entry.file_name();
            if matches!(name.to_bytes(), b"." | b"..") {
                continue;
            }
            entries = entries.saturating_add(1);
            if entries >= entry_limit {
                return Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "reserved log: container entry bound reached",
                ));
            }
            // Never follows a link and never waits for a FIFO peer.
            let fd = openat(
                &self.fd,
                name,
                OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(io_error)?;
            let metadata = File::from(fd).metadata()?;
            if !metadata.file_type().is_file() {
                return Err(unsafe_entry("container holds a non-regular entry"));
            }
            private(&metadata, owner)?;
            used = used.checked_add(metadata.len()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::StorageFull, "reserved log: length overflow")
            })?;
        }
        Ok(used)
    }
}

impl ReservedLog {
    /// Encode one record and store it, followed by a line feed, at the current
    /// offset within the ordinary part of the reservation (all but its closing
    /// bytes). `Some(n)`: all `n` bytes, the line feed included, were written and
    /// synchronized. `None`: the record did not fit, observed by this writer even
    /// if `encode` swallowed the error; nothing was written and the log stays open.
    ///
    /// # Errors
    /// A closed or failed log (without calling `encode`), an `encode` error other
    /// than overflow, or a write/sync failure. Every error leaves the log failed:
    /// no later call writes or calls an encoder again.
    pub fn append_encoded(
        &mut self,
        encode: impl FnOnce(&mut dyn Write) -> io::Result<()>,
    ) -> io::Result<Option<usize>> {
        let room = (self.size - self.closing).saturating_sub(self.offset);
        self.record(room, false, encode)
    }

    /// Store the closing record, at most the reserved closing bytes, and close the
    /// log on `Some`. The reservation keeps its full length: nothing is truncated.
    /// `None` leaves the log open with nothing written.
    ///
    /// # Errors
    /// As [`ReservedLog::append_encoded`].
    pub fn finish_encoded(
        &mut self,
        encode: impl FnOnce(&mut dyn Write) -> io::Result<()>,
    ) -> io::Result<Option<usize>> {
        let room = self.closing.min(self.size.saturating_sub(self.offset));
        self.record(room, true, encode)
    }

    fn record(
        &mut self,
        room: u64,
        finish: bool,
        encode: impl FnOnce(&mut dyn Write) -> io::Result<()>,
    ) -> io::Result<Option<usize>> {
        if self.state != State::Open {
            return Err(io::Error::other("reserved log: closed or failed"));
        }
        // The line feed is part of the record's room.
        let Some(limit) = room.checked_sub(1) else {
            return Ok(None);
        };
        let Ok(limit) = usize::try_from(limit) else {
            self.state = State::Poisoned;
            return Err(io::Error::other("reserved log: bound exceeds memory"));
        };
        let mut out = Bounded {
            bytes: Vec::new(),
            limit,
            overflow: false,
            failed: false,
        };
        let encoded = encode(&mut out);
        // A sink failure other than capacity is terminal even when the encoder swallowed it,
        // and is never reported as the retryable overflow.
        if out.failed {
            self.state = State::Poisoned;
            return Err(io::Error::other("reserved log: record sink failed"));
        }
        if out.overflow {
            return Ok(None);
        }
        let stored = encoded
            .and_then(|()| out.bytes.try_reserve(1).map_err(io::Error::other))
            .and_then(|()| {
                out.bytes.push(b'\n');
                store(&self.file, &out.bytes, self.offset)
            });
        let next = u64::try_from(out.bytes.len())
            .ok()
            .and_then(|n| self.offset.checked_add(n));
        match (stored, next) {
            (Ok(()), Some(next)) => {
                self.offset = next;
                if finish {
                    self.state = State::Closed;
                }
                Ok(Some(out.bytes.len()))
            }
            (Err(error), _) => {
                self.state = State::Poisoned;
                Err(error)
            }
            (Ok(()), None) => {
                self.state = State::Poisoned;
                Err(io::Error::other("reserved log: offset overflow"))
            }
        }
    }
}

/// An encoder sink that never grows past its limit and remembers that it was asked to, or
/// that it failed for any other reason.
struct Bounded {
    bytes: Vec<u8>,
    limit: usize,
    overflow: bool,
    failed: bool,
}

impl Write for Bounded {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.overflow || data.len() > self.limit - self.bytes.len() {
            self.overflow = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "reserved log: record exceeds its bound",
            ));
        }
        let reserved = self.bytes.try_reserve(data.len()).map_err(io::Error::other);
        #[cfg(test)]
        let reserved = reserved.and_then(|()| tests::sink_fault());
        if let Err(error) = reserved {
            self.failed = true;
            return Err(error);
        }
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// An entry of `owner` (the effective uid), private, with a single link.
fn private(metadata: &std::fs::Metadata, owner: u32) -> io::Result<()> {
    if metadata.uid() != owner || metadata.mode() & 0o077 != 0 || metadata.nlink() != 1 {
        return Err(unsafe_entry(
            "entry is not a private single-link file of its owner",
        ));
    }
    Ok(())
}

fn unsafe_entry(why: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("reserved log: {why}"),
    )
}

/// Fill the reservation with spaces in bounded positional writes (never at end of file).
fn initialize(file: &File, size: u64) -> io::Result<()> {
    let chunk = vec![b' '; CHUNK];
    let mut at = 0u64;
    while at < size {
        let n = usize::try_from(size - at).map_or(CHUNK, |left| left.min(CHUNK));
        file.write_all_at(&chunk[..n], at)?;
        at = at
            .checked_add(u64::try_from(n).map_err(io::Error::other)?)
            .ok_or_else(|| io::Error::other("reserved log: offset overflow"))?;
    }
    Ok(())
}

/// Write one whole record at `at`, then synchronize it.
fn store(file: &File, bytes: &[u8], at: u64) -> io::Result<()> {
    #[cfg(test)]
    tests::inject(file, bytes, at)?;
    file.write_all_at(bytes, at)?;
    file.sync_all()
}

#[cfg(test)]
mod tests;
