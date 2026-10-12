// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The quarantine's file operations, held by descriptors. A recorded path
//! is judged by name before any filesystem call ([`inside`]); the project
//! root is then opened once, every directory below it is opened one
//! component at a time without following a symbolic link (a parent replaced
//! by a link is refused, never walked), and each final name is examined,
//! renamed or linked relative to its parent's descriptor, never re-resolved
//! from a path string. These are the POSIX `*at` calls on macOS and Linux
//! (this crate builds on unix only, through its `nix` dependency).

use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::os::unix::fs::MetadataExt as _;
use std::path::{Component, Path};

use nika_kernel::fs::FileIdentity;
use nika_runtime::CreatedFile;
use nix::errno::Errno;
use nix::fcntl::{AtFlags, OFlag, open, openat, renameat};
use nix::sys::stat::{Mode, SFlag, fstatat, mkdirat};
use nix::unistd::{UnlinkatFlags, linkat, unlinkat};

/// The directories the quarantine creates belong to the person running it.
const DIR_MODE: Mode = Mode::from_bits_truncate(0o700);
/// The manifest, likewise.
const FILE_MODE: Mode = Mode::from_bits_truncate(0o600);
/// The largest manifest a restore reads.
const MANIFEST_MAX: u64 = 8 * 1024 * 1024;

/// A recorded path that stays inside the project: its parent directories
/// and its final name, each one plain component.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Inside<'p> {
    /// The directories from the root down to the file.
    pub(super) parents: Vec<&'p str>,
    /// The file's own name.
    pub(super) name: &'p str,
}

/// Judge `path` by name alone, before any filesystem call: it must be
/// relative, every component plain (a `.` is skipped), and it must name a
/// file; `..`, a root, a drive prefix or an empty path are refused.
pub(super) fn inside(path: &str) -> Result<Inside<'_>, &'static str> {
    let mut parts = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_str().ok_or("the path is not UTF-8")?),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("the path leaves the project");
            }
        }
    }
    let name = parts.pop().ok_or("the path names no file")?;
    Ok(Inside {
        parents: parts,
        name,
    })
}

/// The project root, opened once; every operation below holds it.
pub(super) struct Root(File);

impl Root {
    /// Open the project at `root` (`""` is the working directory). The root
    /// itself is the person's own choice and may be reached through a link;
    /// nothing below it is.
    pub(super) fn open(root: &Path) -> io::Result<Self> {
        let at = if root.as_os_str().is_empty() {
            Path::new(".")
        } else {
            root
        };
        let fd = open(
            at,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )?;
        Ok(Self(File::from(fd)))
    }

    /// The directory at `parts` below the root, each component opened
    /// without following a link; a missing one is created first when
    /// `create`.
    pub(super) fn dir(&self, parts: &[&str], create: bool) -> io::Result<File> {
        let mut dir = self.0.try_clone()?;
        for part in parts {
            if create {
                match mkdirat(&dir, *part, DIR_MODE) {
                    Ok(()) | Err(Errno::EEXIST) => {}
                    Err(errno) => return Err(errno.into()),
                }
            }
            let fd = openat(
                &dir,
                *part,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|errno| match errno {
                Errno::ELOOP | Errno::ENOTDIR => io::Error::other(
                    "a directory on the path is a symbolic link or not a directory",
                ),
                other => other.into(),
            })?;
            dir = File::from(fd);
        }
        Ok(dir)
    }
}

/// The regular file at `name` in `dir`, opened without following it;
/// `None` when nothing stands there. A link, a directory or any other kind
/// of entry is refused with what it is.
pub(super) fn open_file(dir: &File, name: &str) -> io::Result<Option<File>> {
    let stat = match fstatat(dir, name, AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::ENOENT) => return Ok(None),
        Err(errno) => return Err(errno.into()),
    };
    let kind = SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT;
    if kind == SFlag::S_IFLNK {
        return Err(io::Error::other("a symbolic link stands at the path"));
    }
    if kind == SFlag::S_IFDIR {
        return Err(io::Error::other("a directory stands at the path"));
    }
    if kind != SFlag::S_IFREG {
        return Err(io::Error::other(
            "what stands at the path is not a regular file",
        ));
    }
    let fd = openat(
        dir,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;
    let file = File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(io::Error::other(
            "what stands at the path is not a regular file",
        ));
    }
    Ok(Some(file))
}

/// Which file `file` is, and its size.
fn identity(file: &File) -> io::Result<(FileIdentity, u64)> {
    let meta = file.metadata()?;
    Ok((
        FileIdentity::new(meta.dev(), meta.ino(), meta.nlink()),
        meta.len(),
    ))
}

/// Why `file` is not the one a run created (`made`): it has more than one
/// name, it is another file, or it changed since the run wrote it.
fn not_made(file: &File, made: &CreatedFile) -> io::Result<Option<&'static str>> {
    let (found, len) = identity(file)?;
    Ok(if found.links != 1 {
        Some("the file has more than one name")
    } else if (found.device, found.inode) != (made.identity.device, made.identity.inode) {
        Some("another file stands at the path")
    } else if len != made.len {
        Some("the file changed since the run wrote it")
    } else {
        None
    })
}

/// Move `name` in `from` to `target` in `to`, then check that what moved
/// is `held`: when the name changed hands in between, what moved goes
/// back without replacing anything, and the move is refused.
fn rename_held(
    from: &File,
    name: &str,
    to: &File,
    target: &str,
    held: &File,
) -> Result<(), String> {
    renameat(from, name, to, target).map_err(|errno| io::Error::from(errno).to_string())?;
    let expected = identity(held).map_err(|e| e.to_string())?.0;
    let moved = open_file(to, target)
        .ok()
        .flatten()
        .and_then(|file| identity(&file).ok())
        .map(|(found, _)| (found.device, found.inode));
    if moved == Some((expected.device, expected.inode)) {
        return Ok(());
    }
    let back = linkat(to, target, from, name, AtFlags::empty())
        .and_then(|()| unlinkat(to, target, UnlinkatFlags::NoRemoveDir));
    Err(match back {
        Ok(()) => "the path changed while it moved; what moved went back".to_owned(),
        Err(_) => format!("the path changed while it moved; what moved stays as {target}"),
    })
}

/// Move the file a run created at `from` to `target` in `to`, only while
/// it is still that very file (`made`).
pub(super) fn move_made(
    root: &Root,
    from: &Inside<'_>,
    made: &CreatedFile,
    to: &File,
    target: &str,
) -> Result<(), String> {
    let parent = root.dir(&from.parents, false).map_err(|e| e.to_string())?;
    let file = open_file(&parent, from.name)
        .map_err(|e| e.to_string())?
        .ok_or("nothing stands at the path")?;
    if let Some(why) = not_made(&file, made).map_err(|e| e.to_string())? {
        return Err(why.to_owned());
    }
    rename_held(&parent, from.name, to, target, &file)
}

/// Move a staged original (a regular file, never a link) to `target` in
/// `to`.
pub(super) fn move_staged(
    root: &Root,
    staged: &Inside<'_>,
    to: &File,
    target: &str,
) -> Result<(), String> {
    let parent = root
        .dir(&staged.parents, false)
        .map_err(|e| e.to_string())?;
    let file = open_file(&parent, staged.name)
        .map_err(|e| e.to_string())?
        .ok_or("the staged original is gone")?;
    rename_held(&parent, staged.name, to, target, &file)
}

/// Write `bytes` as a new file `name` in `dir`: never through a link, never
/// over something that already stands there.
pub(super) fn write_new(dir: &File, name: &str, bytes: &[u8]) -> io::Result<()> {
    let fd = openat(
        dir,
        name,
        OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        FILE_MODE,
    )?;
    File::from(fd).write_all(bytes)
}

/// Read the regular file `name` in `dir` (never through a link), up to the
/// manifest bound; `None` when nothing stands there.
pub(super) fn read_bounded(dir: &File, name: &str) -> io::Result<Option<Vec<u8>>> {
    let Some(file) = open_file(dir, name)? else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    file.take(MANIFEST_MAX + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MANIFEST_MAX {
        return Err(io::Error::other("the manifest is over its bound"));
    }
    Ok(Some(bytes))
}

/// What became of one quarantined file a restore tried to put back.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PutBack {
    /// It is back at its path.
    Restored,
    /// It is no longer in the quarantine (an earlier resume put it back).
    AlreadyBack,
    /// Something stands at its path: both stay, the person's file untouched.
    Conflict,
    /// It stays in quarantine, for this reason.
    Refused(String),
}

/// Put the quarantined file `moved` (in `quarantine`) back at `dest`, never
/// over a file that stands there: the file is linked at its path, which
/// fails when the name is taken, and only then unlinked from the quarantine.
pub(super) fn put_back(root: &Root, quarantine: &File, moved: &str, dest: &Inside<'_>) -> PutBack {
    match open_file(quarantine, moved) {
        Ok(Some(_)) => {}
        Ok(None) => return PutBack::AlreadyBack,
        Err(e) => return PutBack::Refused(e.to_string()),
    }
    let parent = match root.dir(&dest.parents, true) {
        Ok(parent) => parent,
        Err(e) => return PutBack::Refused(e.to_string()),
    };
    match linkat(quarantine, moved, &parent, dest.name, AtFlags::empty()) {
        Ok(()) => {}
        Err(Errno::EEXIST) => return PutBack::Conflict,
        Err(errno) => return PutBack::Refused(io::Error::from(errno).to_string()),
    }
    match unlinkat(quarantine, moved, UnlinkatFlags::NoRemoveDir) {
        Ok(()) => PutBack::Restored,
        Err(errno) => PutBack::Refused(format!(
            "back at its path, but its quarantine name stays: {}",
            io::Error::from(errno)
        )),
    }
}
