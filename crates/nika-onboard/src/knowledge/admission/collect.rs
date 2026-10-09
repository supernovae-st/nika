// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! B · the collection (the r1 contract §3, unchanged in r2 but for its layout and byte bound).
//! It runs only once a trusted identity is named.
//!
//! **The closed collection layout.** A payload holds root-level files, and files directly inside
//! the directories its profile names ([`Layout`]): `knowledge/`, `blocks/` and `LICENSES/` in
//! r1; `examples/`, `counterexamples/` and `skills/` too in r2. Any other directory, even an
//! empty one, is refused.
//!
//! **The memory form** judges, in this order:
//! 1. each path in byte order: safe, then in the layout;
//! 2. the bounds, the manifest included.
//!
//! **The disk form** holds the root and every layout directory by descriptor:
//! - The root is opened once, without following its final component.
//! - Every entry is inspected and opened relative to its held parent, never by a path from the
//!   root.
//! - Links, types, the layout and the bounds decide before any byte is read.
//! - A file is opened without following and without blocking, checked to be the entry inspected,
//!   read once within its bound, and checked again after the read.
//!
//! These checks catch the inconsistencies they observe. They do not detect every concurrent write
//! of the same length; the trusted identity and the pins judge the bytes kept.

use std::collections::BTreeMap;
use std::path::Path;

use nika_compile_seats::foundry::release::r2;

use super::profile::safe_relative;
use super::{
    Checked, MANIFEST_PATH, MAX_BYTES, MAX_FILES, MAX_MANIFEST_BYTES, RefusalCode, refuse,
};

/// The collected files by their `/`-separated relative paths.
pub(super) type Files = BTreeMap<String, Vec<u8>>;

/// What one profile's collection admits: the directories directly under the root, and the bytes
/// of the whole payload. The other bounds are the same in r1 and r2.
#[derive(Clone, Copy, Debug)]
pub(super) struct Layout {
    pub(super) dirs: &'static [&'static str],
    pub(super) max_bytes: u64,
}

/// Profile r1's collection.
pub(super) const R1: Layout = Layout {
    dirs: &["knowledge", "blocks", "LICENSES"],
    max_bytes: MAX_BYTES,
};

/// Profile r2's collection.
pub(super) const R2: Layout = Layout {
    dirs: &r2::DIRECTORIES,
    max_bytes: r2::MAX_BYTES,
};

/// A step of the disk walk a test barrier may stand between; a door's probe does nothing.
/// No step can occur on platforms without the descriptor walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    /// A layout directory was inspected and is about to be opened.
    #[cfg(unix)]
    DirInspected,
    /// A layout directory is open and held.
    #[cfg(unix)]
    DirHeld,
    /// A regular file was inspected and is about to be opened.
    #[cfg(unix)]
    FileInspected,
    /// A file was read and is about to be checked again.
    #[cfg(unix)]
    FileRead,
}

/// A path in the closed collection layout: a root-level file, or a file directly inside one of
/// the layout's directories.
pub(super) fn in_layout(path: &str, layout: Layout) -> bool {
    match path.split_once('/') {
        None => true,
        Some((dir, rest)) => layout.dirs.contains(&dir) && !rest.contains('/'),
    }
}

/// The memory form (§3.1): each path in byte order, safe and in the layout; then the bounds,
/// the manifest included, before anything is hashed or parsed.
pub(super) fn check_memory(files: &Files, layout: Layout) -> Checked<()> {
    for path in files.keys() {
        if !safe_relative(path) {
            return refuse("B4", RefusalCode::UnsafePath, path.clone());
        }
        if !in_layout(path, layout) {
            return refuse(
                "B4",
                RefusalCode::UnexpectedFile,
                format!("{path}: outside the collection layout"),
            );
        }
    }
    let size = |bytes: &Vec<u8>| u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if files.len() > MAX_FILES {
        return refuse(
            "B5",
            RefusalCode::TooLarge,
            format!("more than {MAX_FILES} files"),
        );
    }
    if files
        .get(MANIFEST_PATH)
        .is_some_and(|manifest| size(manifest) > MAX_MANIFEST_BYTES)
    {
        return refuse(
            "B5",
            RefusalCode::TooLarge,
            format!("a manifest of more than {MAX_MANIFEST_BYTES} bytes"),
        );
    }
    let total = files.values().map(size).fold(0_u64, u64::saturating_add);
    if total > layout.max_bytes {
        return refuse(
            "B5",
            RefusalCode::TooLarge,
            format!("more than {} bytes", layout.max_bytes),
        );
    }
    Ok(())
}

/// The disk form on a platform without the descriptor calls the walk needs: refused, never a
/// weaker walk (the memory form is the same everywhere).
#[cfg(not(unix))]
pub(super) fn read_root(
    root: &Path,
    _layout: Layout,
    _probe: &mut dyn FnMut(Stage, &str),
) -> Checked<Files> {
    if !root.is_absolute() {
        return refuse(
            "B1",
            RefusalCode::PathNotAbsolute,
            format!("{}: not an absolute path", root.display()),
        );
    }
    refuse(
        "B2",
        RefusalCode::Io,
        "the anchored collection needs Unix descriptors: admit the release in memory",
    )
}

/// The disk form (§3.2): B1 to B6 on held descriptors.
#[cfg(unix)]
pub(super) fn read_root(
    root: &Path,
    layout: Layout,
    probe: &mut dyn FnMut(Stage, &str),
) -> Checked<Files> {
    unix::read_root(root, layout, probe)
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsStr;
    use std::io::Read as _;
    use std::os::fd::{AsFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::Path;

    use rustix::fs::{AtFlags, CWD, Dir, FileType, Mode, OFlags, Stat, fstat, openat, statat};

    use super::super::profile::safe_relative;
    use super::super::{
        Checked, MANIFEST_PATH, MAX_ENTRIES, MAX_FILES, MAX_MANIFEST_BYTES, Refusal, RefusalCode,
        Step, refuse,
    };
    use super::{Files, Layout, Stage};

    /// A directory opened to be held: read-only, never through a link.
    const DIRECTORY: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);
    /// A file opened to be read: never through a link, never blocking on a special file.
    const FILE: OFlags = OFlags::RDONLY
        .union(OFlags::NOFOLLOW)
        .union(OFlags::NONBLOCK)
        .union(OFlags::CLOEXEC)
        .union(OFlags::NOCTTY);

    /// What an entry could not do at `step`, as `PAYLOAD_IO` naming it.
    fn io(step: Step, what: &str) -> impl Fn(rustix::io::Errno) -> Refusal + '_ {
        move |error| Refusal(step, RefusalCode::Io, format!("{what}: {error}"))
    }

    /// The root as text (§3.2): its trailing slashes removed. `None` when nothing is left, or
    /// when its final component is `.` or `..`. Nothing is canonicalized or reopened: a trailing
    /// slash never makes a final link followed.
    fn final_root(root: &Path) -> Option<&Path> {
        let mut bytes = root.as_os_str().as_bytes();
        while let Some(rest) = bytes.strip_suffix(b"/") {
            bytes = rest;
        }
        let last = bytes.rsplit(|byte| *byte == b'/').next()?;
        let named = !bytes.is_empty() && last != b"." && last != b"..";
        named.then(|| Path::new(OsStr::from_bytes(bytes)))
    }

    /// The root held once, then the layout directories it holds, each listing bounded first.
    pub(super) fn read_root(
        root: &Path,
        layout: Layout,
        probe: &mut dyn FnMut(Stage, &str),
    ) -> Checked<Files> {
        if !root.is_absolute() {
            return refuse(
                "B1",
                RefusalCode::PathNotAbsolute,
                format!("{}: not an absolute path", root.display()),
            );
        }
        let Some(root) = final_root(root) else {
            return refuse(
                "B2",
                RefusalCode::RootInvalid,
                format!("{}: no final component, or a final . or ..", root.display()),
            );
        };
        let inspected = statat(CWD, root, AtFlags::SYMLINK_NOFOLLOW).map_err(|error| {
            Refusal("B2", RefusalCode::RootInvalid, format!("the root: {error}"))
        })?;
        match FileType::from_raw_mode(inspected.st_mode) {
            FileType::Symlink => {
                return refuse("B2", RefusalCode::Symlink, "the root is a symbolic link");
            }
            FileType::Directory => {}
            _ => {
                return refuse(
                    "B2",
                    RefusalCode::RootInvalid,
                    "the root is not a directory",
                );
            }
        }
        let held = openat(CWD, root, DIRECTORY, Mode::empty()).map_err(io("B2", "the root"))?;
        same_entry("B2", &held, &inspected, false, "the root")?;
        let mut walk = Walk {
            files: Files::new(),
            total: 0,
            layout,
        };
        let mut held_dirs = Vec::new();
        walk.directory("", &held, &mut held_dirs, probe)?;
        for (path, dir) in &held_dirs {
            walk.directory(path, dir, &mut Vec::new(), probe)?;
        }
        Ok(walk.files)
    }

    /// What the walk has kept so far, under the layout it admits.
    struct Walk {
        files: Files,
        total: u64,
        layout: Layout,
    }

    impl Walk {
        /// One held directory: its listing bounded before any sort, then each entry in byte
        /// order of its name. Layout directories it holds are pushed onto `layout`.
        fn directory(
            &mut self,
            rel: &str,
            dir: &OwnedFd,
            layout: &mut Vec<(String, OwnedFd)>,
            probe: &mut dyn FnMut(Stage, &str),
        ) -> Checked<()> {
            let here = if rel.is_empty() { "the root" } else { rel };
            let mut names = Vec::new();
            for entry in Dir::read_from(dir).map_err(io("B3", here))? {
                let entry = entry.map_err(io("B3", here))?;
                let name = entry.file_name().to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                if names.len() == MAX_ENTRIES {
                    return refuse(
                        "B3",
                        RefusalCode::TooLarge,
                        format!("{here}: more than {MAX_ENTRIES} entries"),
                    );
                }
                names.push(name.to_vec());
            }
            names.sort();
            for raw in names {
                let Ok(name) = String::from_utf8(raw) else {
                    return refuse(
                        "B4",
                        RefusalCode::NotRegular,
                        format!("{here}: a name that is not UTF-8"),
                    );
                };
                let path = if rel.is_empty() {
                    name.clone()
                } else {
                    format!("{rel}/{name}")
                };
                self.entry(rel, dir, &name, path, layout, probe)?;
            }
            Ok(())
        }

        /// One entry of a held directory, inspected without following before anything of it is
        /// opened or read.
        fn entry(
            &mut self,
            rel: &str,
            dir: &OwnedFd,
            name: &str,
            path: String,
            layout: &mut Vec<(String, OwnedFd)>,
            probe: &mut dyn FnMut(Stage, &str),
        ) -> Checked<()> {
            if !safe_relative(&path) {
                return refuse("B4", RefusalCode::UnsafePath, path);
            }
            let stat = statat(dir, name, AtFlags::SYMLINK_NOFOLLOW).map_err(io("B6", &path))?;
            match FileType::from_raw_mode(stat.st_mode) {
                FileType::Symlink => refuse("B4", RefusalCode::Symlink, path),
                FileType::Directory if rel.is_empty() && self.layout.dirs.contains(&name) => {
                    probe(Stage::DirInspected, &path);
                    let child =
                        openat(dir, name, DIRECTORY, Mode::empty()).map_err(io("B6", &path))?;
                    same_entry("B6", &child, &stat, false, &path)?;
                    probe(Stage::DirHeld, &path);
                    layout.push((path, child));
                    Ok(())
                }
                FileType::Directory => refuse(
                    "B4",
                    RefusalCode::UnexpectedFile,
                    format!("{path}/: outside the collection layout"),
                ),
                FileType::RegularFile => {
                    self.bounded(&path, &stat)?;
                    probe(Stage::FileInspected, &path);
                    let bytes = read_file(dir, name, &stat, &path, probe)?;
                    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                    self.total = self.total.saturating_add(size);
                    self.files.insert(path, bytes);
                    Ok(())
                }
                _ => refuse("B4", RefusalCode::NotRegular, path),
            }
        }

        /// B5 before the read: the next file within the file count, the manifest within its
        /// bound, the file within what remains of the total.
        fn bounded(&self, path: &str, stat: &Stat) -> Checked<()> {
            let size = u64::try_from(stat.st_size).unwrap_or(u64::MAX);
            if self.files.len() >= MAX_FILES {
                return refuse(
                    "B5",
                    RefusalCode::TooLarge,
                    format!("more than {MAX_FILES} files"),
                );
            }
            if path == MANIFEST_PATH && size > MAX_MANIFEST_BYTES {
                return refuse(
                    "B5",
                    RefusalCode::TooLarge,
                    format!("a manifest of more than {MAX_MANIFEST_BYTES} bytes"),
                );
            }
            let max_bytes = self.layout.max_bytes;
            if size > max_bytes.saturating_sub(self.total) {
                return refuse(
                    "B5",
                    RefusalCode::TooLarge,
                    format!("more than {max_bytes} bytes"),
                );
            }
            Ok(())
        }
    }

    /// B6: the file opened relative to its held parent, without following or blocking, checked
    /// to be the entry inspected, read once within its length, and checked again after.
    fn read_file(
        dir: &OwnedFd,
        name: &str,
        inspected: &Stat,
        path: &str,
        probe: &mut dyn FnMut(Stage, &str),
    ) -> Checked<Vec<u8>> {
        let fd = openat(dir, name, FILE, Mode::empty()).map_err(io("B6", path))?;
        same_entry("B6", &fd, inspected, true, path)?;
        let size = u64::try_from(inspected.st_size).unwrap_or(u64::MAX);
        let mut file = std::fs::File::from(fd);
        let mut bytes = Vec::new();
        (&mut file)
            .take(size.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| Refusal("B6", RefusalCode::Io, format!("{path}: {error}")))?;
        probe(Stage::FileRead, path);
        let after = fstat(&file).map_err(io("B6", path))?;
        if u64::try_from(bytes.len()).ok() != Some(size) || after.st_size != inspected.st_size {
            return refuse(
                "B6",
                RefusalCode::Io,
                format!("{path}: changed while it was read"),
            );
        }
        Ok(bytes)
    }

    /// An opened descriptor is the entry inspected: the same type, device and inode, and for a
    /// file the same length.
    fn same_entry(
        step: Step,
        fd: &impl AsFd,
        inspected: &Stat,
        file: bool,
        what: &str,
    ) -> Checked<()> {
        let opened = fstat(fd).map_err(io(step, what))?;
        let same = FileType::from_raw_mode(opened.st_mode)
            == FileType::from_raw_mode(inspected.st_mode)
            && opened.st_dev == inspected.st_dev
            && opened.st_ino == inspected.st_ino
            && (!file || opened.st_size == inspected.st_size);
        if same {
            Ok(())
        } else {
            refuse(
                step,
                RefusalCode::Io,
                format!("{what}: not the entry inspected"),
            )
        }
    }
}
