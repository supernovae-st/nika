// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The regular-file removal the host and room backends share: the final name
//! is classified without following it (`fstatat(AT_SYMLINK_NOFOLLOW)`), then
//! unlinked relative to the same held parent descriptor (`unlinkat`, never a
//! directory). The file is never opened or read, nothing is created, nothing
//! is retried, and no ledger state is touched here.
//!
//! The classification and the unlink are two steps, not an atomic
//! compare-and-remove of one inode: a name substituted between them may be
//! removed in its place (a substituted symlink is unlinked, never followed).
//! The held parent pins which directory is touched, not where that directory
//! currently sits in the tree.

use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use nika_kernel::fs::FsError;
use nix::fcntl::AtFlags;
use nix::sys::stat::{SFlag, fstatat};
use nix::unistd::{UnlinkatFlags, unlinkat};

/// Refuse a path whose raw spelling names no final file, before any
/// decomposition that would normalize it: empty, ending in a separator, or
/// ending in `.` or `..` (so `file/` and `file/.` never become `file`).
pub(crate) fn final_name(path: &Path) -> Result<(), FsError> {
    let bytes = path.as_os_str().as_bytes();
    let leaf = bytes
        .rsplit(|byte| *byte == b'/')
        .next()
        .unwrap_or_default();
    if leaf.is_empty() || leaf == b"." || leaf == b".." {
        return Err(FsError::InvalidData {
            path: path.display().to_string(),
            reason: "the path names no final file".to_owned(),
        });
    }
    Ok(())
}

/// Remove the regular file `name` names in the held directory `parent`;
/// `shown` is the caller's path, for errors only. `name` must be one
/// contained leaf (not empty, `.`, `..` or holding a separator). Absent is
/// `NotFound`, a symlink `SymlinkRefused`, a directory or a special node
/// `InvalidData`; any other failure keeps its errno's kind.
pub(crate) fn remove_regular_at(parent: &File, name: &OsStr, shown: &Path) -> Result<(), FsError> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        return Err(FsError::InvalidData {
            path: shown.display().to_string(),
            reason: "the final name is not one contained leaf".to_owned(),
        });
    }
    let stat = fstatat(parent, name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .map_err(|errno| FsError::from_io(&io::Error::from(errno), shown))?;
    let kind = SFlag::from_bits_truncate(stat.st_mode) & SFlag::S_IFMT;
    if kind == SFlag::S_IFLNK {
        return Err(FsError::SymlinkRefused {
            path: shown.display().to_string(),
        });
    }
    if kind != SFlag::S_IFREG {
        let what = if kind == SFlag::S_IFDIR {
            "a directory is not a regular file"
        } else {
            "a special node is not a regular file"
        };
        return Err(FsError::InvalidData {
            path: shown.display().to_string(),
            reason: what.to_owned(),
        });
    }
    unlinkat(parent, name, UnlinkatFlags::NoRemoveDir)
        .map_err(|errno| FsError::from_io(&io::Error::from(errno), shown))
}
