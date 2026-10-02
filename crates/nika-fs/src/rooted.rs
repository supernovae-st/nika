// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A room: one owned directory served through the kernel filesystem traits,
//! every blocking operation registered in the room's [`EffectLedger`].
//!
//! The backend refuses by itself any absolute path, any `..` and any name that
//! is not UTF-8. Every parent component is opened without following it, so a
//! symlink there is refused; a symlink as the final name is refused by every
//! read and by `metadata`, while a write replaces that name (a rename never
//! follows it) and a removal unlinks the name, never its target. Only regular
//! files are served, a special file without waiting for a peer. `.` answers
//! the room's identity. Every read is capped at the room's byte bound. A write
//! is staged as a private temporary file and claimed by one descriptor-relative
//! rename or link; publication, budget and cleanup are settled by the ledger.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write as _};
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use globset::GlobMatcher;
use nika_kernel::fs::{FileMetadata, FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};
use nix::dir::{Dir, Type};
use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag, openat, renameat};
use nix::sys::stat::{FileStat, Mode, SFlag, fstatat, mkdirat};
use nix::unistd::{UnlinkatFlags, linkat, unlinkat};

use crate::{EffectLedger, LedgerRefusal, OwnedDir, Phase, Reservation};

/// A room's files belong to the run alone.
const FILE_MODE: Mode = Mode::from_bits_truncate(0o600);
/// A room's directories, likewise.
const DIR_MODE: Mode = Mode::from_bits_truncate(0o700);

/// A read of at most `limit` bytes, and whether the source held more.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Capped {
    /// At most `limit` bytes, from the start.
    pub bytes: Bytes,
    /// The source held more than `limit` bytes.
    pub over: bool,
}

impl Capped {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(bytes: Bytes, over: bool) -> Self {
        Self { bytes, over }
    }
}

/// Read at most `limit + 1` bytes from `reader`: `limit` are kept and the one
/// past them only proves the source held more. A file that grows after its
/// metadata was read costs at most `limit + 1` bytes. Nothing is allocated
/// ahead of what the source actually yields.
///
/// # Errors
/// `InvalidInput`, before any byte is read, when `limit + 1` is not
/// representable as a byte count; otherwise the reader's own I/O error.
pub fn read_capped(reader: &mut impl Read, limit: u64) -> io::Result<Capped> {
    let Some(probe) = limit
        .checked_add(1)
        .filter(|probe| usize::try_from(*probe).is_ok())
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("a read bound of {limit} bytes is not representable"),
        ));
    };
    let mut bytes = Vec::new();
    reader.take(probe).read_to_end(&mut bytes)?;
    let kept = usize::try_from(limit).unwrap_or(usize::MAX);
    let over = bytes.len() > kept;
    bytes.truncate(kept);
    Ok(Capped {
        bytes: Bytes::from(bytes),
        over,
    })
}

/// One room's owned directory served through the kernel traits. Every path is
/// relative to the room and walked component by component without following a
/// symlink; only regular files are served. Every operation runs through the
/// room's [`EffectLedger`]. `.` answers the room's identity; an absent path is
/// `NotFound`, like the host `canonicalize`, so a boundary folds its tail.
#[derive(Debug)]
pub struct RootedFs {
    room: Arc<OwnedDir>,
    identity: PathBuf,
    ledger: Arc<EffectLedger>,
}

impl RootedFs {
    /// Serve `room` under `ledger`. The room's display path is its identity.
    #[must_use]
    pub fn new(room: OwnedDir, ledger: Arc<EffectLedger>) -> Self {
        let identity = room.display_path().to_path_buf();
        Self {
            room: Arc::new(room),
            identity,
            ledger,
        }
    }

    /// The room's identity: what `canonicalize(".")` answers.
    #[must_use]
    pub fn identity(&self) -> &Path {
        &self.identity
    }

    /// The room's ledger.
    #[must_use]
    pub fn ledger(&self) -> &Arc<EffectLedger> {
        &self.ledger
    }

    /// Run `work` as a registered operation (a write, in the phase `writer`
    /// names) and await its answer.
    async fn blocking<T, F>(
        &self,
        writer: Option<Phase>,
        path: &Path,
        work: F,
    ) -> Result<T, FsError>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, FsError> + Send + 'static,
    {
        let answer = self
            .ledger
            .register(writer, work)
            .map_err(|refusal| answer_refusal(refusal, path))?;
        answer.await.map_err(|_| FsError::Io {
            reason: format!(
                "the room operation on {} stopped before it answered",
                path.display()
            ),
        })?
    }

    /// The metadata of a contained path, never following a symlink.
    async fn stat(&self, path: &Path) -> Result<FileMetadata, FsError> {
        let relative = room_relative(path)?;
        let (room, shown) = (Arc::clone(&self.room), path.to_path_buf());
        self.blocking(None, path, move || stat_contained(&room, &relative, &shown))
            .await
    }

    /// Reserve, then publish `contents` at `path` through a private temporary
    /// file and one claim, inside one registered write of the reserving phase.
    async fn publish(&self, path: &Path, contents: &[u8], claim: Claim) -> Result<(), FsError> {
        let relative = room_relative(path)?;
        split_leaf(&relative, path)?;
        let reservation = self
            .ledger
            .reserve(byte_len(contents), 1)
            .map_err(|refusal| answer_refusal(refusal, path))?;
        let phase = reservation.phase();
        let publication = Publication {
            room: Arc::clone(&self.room),
            ledger: Arc::clone(&self.ledger),
            reservation,
            relative,
            path: path.to_path_buf(),
            contents: contents.to_vec(),
            claim,
        };
        self.blocking(Some(phase), path, move || publication.run())
            .await
    }

    /// The phase a write without an up-front budget belongs to.
    fn write_phase(&self, path: &Path) -> Result<Phase, FsError> {
        self.ledger
            .writable()
            .map_err(|refusal| answer_refusal(refusal, path))
    }
}

impl FsReadDyn for RootedFs {
    /// Read a regular file of the room, refused past the room's byte bound.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only); a drain waits for a dropped read.
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        let relative = room_relative(path)?;
        let (room, shown) = (Arc::clone(&self.room), path.to_path_buf());
        let bound = self.ledger.byte_bound();
        self.blocking(None, path, move || {
            read_bounded(&room, &relative, bound, &shown)
        })
        .await
    }

    /// The bounded read, as UTF-8.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        let bytes = self.read(path).await?;
        String::from_utf8(Vec::from(bytes)).map_err(|error| FsError::InvalidData {
            path: shown(path),
            reason: error.to_string(),
        })
    }

    /// Whether a contained path exists without any symlink on its way.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn exists(&self, path: &Path) -> bool {
        self.stat(path).await.is_ok()
    }

    /// `.` answers the room's identity without any I/O. Any other contained
    /// path answers the identity joined with its normalized relative form
    /// when every component exists and none is a symlink; an absent one is
    /// `NotFound`, so a boundary folds its tail like the host's.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        let relative = room_relative(path)?;
        if relative.as_os_str().is_empty() {
            return Ok(self.identity.clone());
        }
        self.stat(path).await?;
        Ok(self.identity.join(relative))
    }

    /// The room's read already refuses a symlink at every component, so the
    /// pinned read is the same read.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn read_pinned(&self, path: &Path) -> Result<Bytes, FsError> {
        self.read(path).await
    }
}

impl FsWriteDyn for RootedFs {
    /// Publish `contents` at `path`: reserved before it starts, staged as a
    /// private temporary file, then renamed over the final name in its parent
    /// directory (a symlink there is replaced, never followed). Missing parent
    /// directories are created, each one budgeted file.
    ///
    /// CANCEL SAFETY: the registered write runs to completion and a drain
    /// waits for it; a dropped future may still publish, never partially.
    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        self.publish(path, contents, Claim::Replace).await
    }

    /// The same publication, claimed by an exclusive link: an occupied name,
    /// including a symlink, answers `AlreadyExists` and is left as it was.
    ///
    /// CANCEL SAFETY: as `write`.
    async fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        self.publish(path, contents, Claim::Exclusive).await
    }

    /// Create every missing directory of `path`, each one budgeted file. A
    /// write in itself: refused outside the writing phases, even when nothing
    /// is missing.
    ///
    /// CANCEL SAFETY: partial cancel-safe; a drain waits for a dropped chain.
    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        let relative = room_relative(path)?;
        let phase = self.write_phase(path)?;
        let (room, ledger) = (Arc::clone(&self.room), Arc::clone(&self.ledger));
        let shown = path.to_path_buf();
        self.blocking(Some(phase), path, move || {
            make_dirs(&room, &ledger, &relative, &shown)?;
            Ok(())
        })
        .await
    }

    /// Unlink a contained name that is not a directory: a symlink is unlinked,
    /// never its target. A write: refused outside the writing phases. Nothing
    /// is refunded.
    ///
    /// CANCEL SAFETY: cancel-safe (one unlink).
    async fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        let relative = room_relative(path)?;
        let phase = self.write_phase(path)?;
        let (room, shown) = (Arc::clone(&self.room), path.to_path_buf());
        self.blocking(Some(phase), path, move || {
            remove_contained(&room, &relative, &shown)
        })
        .await
    }
}

impl FsMetaDyn for RootedFs {
    /// The metadata of a contained path, never following it: a symlink
    /// answers `SymlinkRefused`.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError> {
        self.stat(path).await
    }
}

impl FsListDyn for RootedFs {
    /// The entries of a contained directory, as `path` joined with each name,
    /// sorted.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        let relative = room_relative(path)?;
        let (room, shown) = (Arc::clone(&self.room), path.to_path_buf());
        self.blocking(None, path, move || {
            let dir = open_dirs(&room, &relative, &shown)?;
            let mut listed: Vec<PathBuf> = entries(&dir, &shown)?
                .into_iter()
                .map(|(name, _)| shown.join(name))
                .collect();
            listed.sort();
            Ok(listed)
        })
        .await
    }

    /// The host backend's glob semantics over the room: `literal_separator`,
    /// matched against the path relative to `root`, hidden directories not
    /// entered, no symlink followed, any error aborting the whole walk.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only directory walk).
    async fn glob(&self, root: &Path, pattern: &str) -> Result<Vec<PathBuf>, FsError> {
        let matcher = globset::GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|error| FsError::InvalidData {
                path: shown(root),
                reason: format!("invalid glob pattern '{pattern}': {error}"),
            })?
            .compile_matcher();
        let relative = room_relative(root)?;
        let (owned, shown) = (Arc::clone(&self.room), root.to_path_buf());
        self.blocking(None, root, move || {
            walk_room(&owned, &relative, &shown, &matcher)
        })
        .await
    }
}

/// How a publication claims its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    /// Rename over whatever holds the name; a symlink there is replaced.
    Replace,
    /// Link only where the name is free.
    Exclusive,
}

/// One write, staged as a private temporary file in its parent directory and
/// then claimed, settled against the ledger inside the registered operation so
/// that a drain covers the evidence and the budget.
struct Publication {
    room: Arc<OwnedDir>,
    ledger: Arc<EffectLedger>,
    reservation: Reservation,
    relative: PathBuf,
    path: PathBuf,
    contents: Vec<u8>,
    claim: Claim,
}

impl Publication {
    /// A failure before the temporary file exists returns the budget at once
    /// (the reservation drops). After it, the claim and the cleanup are both
    /// known before the ledger settles them.
    fn run(self) -> Result<(), FsError> {
        let (parents, name) = split_leaf(&self.relative, &self.path)?;
        let dir = make_dirs(&self.room, &self.ledger, parents, &self.path)?;
        let (_, temp) = crate::tmp_sibling(Path::new(name));
        let staged = openat(
            &dir,
            temp.as_path(),
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            FILE_MODE,
        )
        .map_err(|errno| errno_error(errno, &self.path))?;
        let filled = File::from(staged).write_all(&self.contents);
        let claimed = filled
            .map_err(|error| FsError::from_io(&error, &self.path))
            .and_then(|()| claim_name(&dir, &temp, name, self.claim, &self.path));
        // A replacing rename consumed the temporary name; anything else removes it.
        let cleaned = (self.claim == Claim::Replace && claimed.is_ok())
            || matches!(
                unlinkat(&dir, temp.as_path(), UnlinkatFlags::NoRemoveDir),
                Ok(()) | Err(Errno::ENOENT)
            );
        let used = byte_len(&self.contents);
        self.ledger
            .settle(self.reservation, &self.relative, used, claimed, cleaned)
    }
}

fn claim_name(
    dir: &File,
    temp: &Path,
    name: &OsStr,
    claim: Claim,
    path: &Path,
) -> Result<(), FsError> {
    let claimed = match claim {
        Claim::Replace => renameat(dir, temp, dir, name),
        Claim::Exclusive => linkat(dir, temp, dir, name, AtFlags::empty()),
    };
    claimed.map_err(|errno| errno_error(errno, path))
}

/// What a name in the room is, read without following it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    File,
    Dir,
    Symlink,
    Special,
}

impl Node {
    fn of(stat: &FileStat) -> Self {
        let kind = SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT;
        if kind == SFlag::S_IFREG {
            Self::File
        } else if kind == SFlag::S_IFDIR {
            Self::Dir
        } else if kind == SFlag::S_IFLNK {
            Self::Symlink
        } else {
            Self::Special
        }
    }
}

/// The room-relative form of `path` under the backend's own law: every
/// component a UTF-8 normal name, `.` dropped. Anything absolute, any `..`
/// and any name that is not UTF-8 is refused. The empty result is the room.
fn room_relative(path: &Path) -> Result<PathBuf, FsError> {
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) if name.to_str().is_some() => relative.push(name),
            Component::Normal(_)
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(FsError::PermissionDenied { path: shown(path) });
            }
        }
    }
    Ok(relative)
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

/// The parent directories and the final name of a room path that is not the
/// room itself.
fn split_leaf<'a>(relative: &'a Path, path: &Path) -> Result<(&'a Path, &'a OsStr), FsError> {
    relative
        .parent()
        .zip(relative.file_name())
        .ok_or_else(|| FsError::InvalidData {
            path: shown(path),
            reason: "the room itself is not a file".to_owned(),
        })
}

fn byte_len(contents: &[u8]) -> u64 {
    u64::try_from(contents.len()).unwrap_or(u64::MAX)
}

fn errno_error(errno: Errno, path: &Path) -> FsError {
    FsError::from_io(&io::Error::from(errno), path)
}

/// The coded answer a ledger refusal gets at the filesystem boundary.
fn answer_refusal(refusal: LedgerRefusal, path: &Path) -> FsError {
    match refusal {
        LedgerRefusal::Sealed { .. } | LedgerRefusal::ReadOnly { .. } => {
            FsError::PermissionDenied { path: shown(path) }
        }
        LedgerRefusal::Budget { .. }
        | LedgerRefusal::Order { .. }
        | LedgerRefusal::NoRuntime { .. } => FsError::Io {
            reason: format!("{refusal}: {}", path.display()),
        },
    }
}

fn not_regular(path: &Path) -> FsError {
    FsError::InvalidData {
        path: shown(path),
        reason: "not a regular file".to_owned(),
    }
}

/// Name what an open refused. A symlink is `SymlinkRefused` whatever errno the
/// platform chose for it (`ELOOP`, or `ENOTDIR` under `O_DIRECTORY`), a special
/// node is `InvalidData`, and anything else keeps its errno's kind.
fn open_refusal(parent: &File, name: &OsStr, errno: Errno, path: &Path) -> FsError {
    match fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW).map(|stat| Node::of(&stat)) {
        Ok(Node::Symlink) => FsError::SymlinkRefused { path: shown(path) },
        Ok(Node::Special) => not_regular(path),
        Ok(Node::File | Node::Dir) | Err(_) => errno_error(errno, path),
    }
}

fn room_root(room: &OwnedDir, path: &Path) -> Result<File, FsError> {
    room.as_file()
        .try_clone()
        .map_err(|error| FsError::from_io(&error, path))
}

fn open_child_dir(parent: &File, name: &OsStr, path: &Path) -> Result<File, FsError> {
    openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(|errno| open_refusal(parent, name, errno, path))
}

/// Open the directory `relative` names, every component a real directory
/// opened without following it.
fn open_dirs(room: &OwnedDir, relative: &Path, path: &Path) -> Result<File, FsError> {
    let mut dir = room_root(room, path)?;
    for name in relative {
        dir = open_child_dir(&dir, name, path)?;
    }
    Ok(dir)
}

/// Walk `relative` from the room, creating each missing directory as one
/// budgeted file; an existing component must be a real directory.
fn make_dirs(
    room: &OwnedDir,
    ledger: &Arc<EffectLedger>,
    relative: &Path,
    path: &Path,
) -> Result<File, FsError> {
    let mut dir = room_root(room, path)?;
    for name in relative {
        dir = match open_child_dir(&dir, name, path) {
            Err(FsError::NotFound { .. }) => {
                make_dir(&dir, name, ledger, path)?;
                open_child_dir(&dir, name, path)?
            }
            opened => opened?,
        };
    }
    Ok(dir)
}

fn make_dir(
    parent: &File,
    name: &OsStr,
    ledger: &Arc<EffectLedger>,
    path: &Path,
) -> Result<(), FsError> {
    let entry = ledger
        .reserve(0, 1)
        .map_err(|refusal| answer_refusal(refusal, path))?;
    match mkdirat(parent, name, DIR_MODE) {
        Ok(()) => entry.commit(0, 1),
        // A concurrent write made it first: nothing of this one is kept.
        Err(Errno::EEXIST) => drop(entry),
        Err(errno) => return Err(errno_error(errno, path)),
    }
    Ok(())
}

/// Open the regular file `name` in `parent` without following it, refusing a
/// special file without waiting for a peer (the open is non-blocking).
fn open_regular(parent: &File, name: &OsStr, path: &Path) -> Result<File, FsError> {
    let file = openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(|errno| open_refusal(parent, name, errno, path))?;
    let metadata = file
        .metadata()
        .map_err(|error| FsError::from_io(&error, path))?;
    if metadata.file_type().is_file() {
        Ok(file)
    } else {
        Err(not_regular(path))
    }
}

/// Read the regular file `relative` names, refused past `bound` bytes after
/// consuming at most `bound + 1`.
fn read_bounded(
    room: &OwnedDir,
    relative: &Path,
    bound: u64,
    path: &Path,
) -> Result<Bytes, FsError> {
    let (parents, name) = split_leaf(relative, path)?;
    let dir = open_dirs(room, parents, path)?;
    let mut file = open_regular(&dir, name, path)?;
    let capped = read_capped(&mut file, bound).map_err(|error| FsError::from_io(&error, path))?;
    if capped.over {
        return Err(FsError::InvalidData {
            path: shown(path),
            reason: format!("larger than the room bound of {bound} bytes"),
        });
    }
    Ok(capped.bytes)
}

fn stat_contained(room: &OwnedDir, relative: &Path, path: &Path) -> Result<FileMetadata, FsError> {
    let Some((parents, name)) = relative.parent().zip(relative.file_name()) else {
        let metadata = room
            .as_file()
            .metadata()
            .map_err(|error| FsError::from_io(&error, path))?;
        return Ok(FileMetadata::new(
            metadata.len(),
            metadata.is_file(),
            metadata.is_dir(),
        ));
    };
    let dir = open_dirs(room, parents, path)?;
    let stat = fstatat(&dir, name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .map_err(|errno| errno_error(errno, path))?;
    let node = Node::of(&stat);
    if node == Node::Symlink {
        return Err(FsError::SymlinkRefused { path: shown(path) });
    }
    let len = u64::try_from(stat.st_size).unwrap_or(0);
    Ok(FileMetadata::new(
        len,
        node == Node::File,
        node == Node::Dir,
    ))
}

fn remove_contained(room: &OwnedDir, relative: &Path, path: &Path) -> Result<(), FsError> {
    let (parents, name) = split_leaf(relative, path)?;
    let dir = open_dirs(room, parents, path)?;
    unlinkat(&dir, name, UnlinkatFlags::NoRemoveDir).map_err(|errno| errno_error(errno, path))
}

/// The entries of `dir` other than `.` and `..`, each with the type the
/// directory reports, when it reports one.
fn entries(dir: &File, path: &Path) -> Result<Vec<(OsString, Option<Type>)>, FsError> {
    let listing = Dir::openat(
        dir,
        ".",
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|errno| errno_error(errno, path))?;
    let mut found = Vec::new();
    for entry in listing {
        let entry = entry.map_err(|errno| errno_error(errno, path))?;
        let name = OsStr::from_bytes(entry.file_name().to_bytes());
        if name != "." && name != ".." {
            found.push((name.to_os_string(), entry.file_type()));
        }
    }
    Ok(found)
}

fn is_dir(dir: &File, name: &OsStr, kind: Option<Type>, path: &Path) -> Result<bool, FsError> {
    match kind {
        Some(kind) => Ok(kind == Type::Directory),
        None => fstatat(dir, name, AtFlags::AT_SYMLINK_NOFOLLOW)
            .map(|stat| Node::of(&stat) == Node::Dir)
            .map_err(|errno| errno_error(errno, path)),
    }
}

/// Every entry under `relative` that is not a directory and whose path
/// relative to it matches, as `display_base` joined with that path, sorted.
fn walk_room(
    room: &OwnedDir,
    relative: &Path,
    display_base: &Path,
    matcher: &GlobMatcher,
) -> Result<Vec<PathBuf>, FsError> {
    let mut found = Vec::new();
    let mut stack = vec![(open_dirs(room, relative, display_base)?, PathBuf::new())];
    while let Some((dir, below)) = stack.pop() {
        for (name, kind) in entries(&dir, display_base)? {
            let inner = below.join(&name);
            if is_dir(&dir, &name, kind, display_base)? {
                if name.as_bytes().first() != Some(&b'.') {
                    stack.push((open_child_dir(&dir, &name, display_base)?, inner));
                }
            } else if matcher.is_match(&inner) {
                found.push(display_base.join(&inner));
            }
        }
    }
    found.sort();
    Ok(found)
}
