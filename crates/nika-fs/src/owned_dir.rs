// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Descriptor-rooted synchronous filesystem ownership.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read as _, Write as _};
use std::path::{Component, Path, PathBuf};

use nix::dir::Dir;
use nix::fcntl::{AtFlags, OFlag, openat, renameat};
use nix::sys::stat::{Mode, mkdirat};
use nix::unistd::{UnlinkatFlags, linkat, unlinkat};

const DIR_MODE: Mode = Mode::from_bits_truncate(0o700);
const FILE_MODE: Mode = Mode::from_bits_truncate(0o600);

/// An opened directory whose child I/O stays relative to a held descriptor.
///
/// Replacing any visible ancestor after construction cannot redirect later
/// operations. Every component and child name is one contained normal path
/// component; directory and file opens refuse symlinks.
#[derive(Debug)]
pub struct OwnedDir {
    fd: File,
    display: PathBuf,
}

impl OwnedDir {
    /// Open the root and create/open each contained directory component below it.
    ///
    /// # Errors
    /// The root is inaccessible, a component is invalid, or a component is not
    /// a real directory owned beneath the previously opened descriptor.
    pub fn create(root: &Path, components: &[&str]) -> io::Result<Self> {
        let mut fd = File::open(root)?;
        let mut display = root.to_path_buf();
        for component in components {
            validate_component(component)?;
            fd = open_or_create_dir(&fd, component)?;
            display.push(component);
        }
        Ok(Self { fd, display })
    }

    /// Duplicate the held directory capability.
    ///
    /// # Errors
    /// Returns an error when the directory descriptor cannot be duplicated.
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            fd: self.fd.try_clone()?,
            display: self.display.clone(),
        })
    }

    /// Open or create a regular lock file without following a symlink.
    ///
    /// # Errors
    /// Returns an error for an invalid name or when the child cannot be opened safely.
    pub fn open_lock(&self, name: &str) -> io::Result<File> {
        self.open_file(
            name,
            OFlag::O_CREAT
                | OFlag::O_RDWR
                | OFlag::O_NOFOLLOW
                | OFlag::O_NONBLOCK
                | OFlag::O_CLOEXEC,
        )
    }

    /// Read an optional UTF-8 regular file without following a symlink.
    ///
    /// # Errors
    /// Returns an error for an invalid name, unsafe child, or invalid UTF-8.
    pub fn read_optional(&self, name: &str) -> io::Result<Option<String>> {
        match self.open_file(name, OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC) {
            Ok(mut file) => {
                let mut text = String::new();
                file.read_to_string(&mut text)?;
                Ok(Some(text))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Read a required UTF-8 regular file without following a symlink.
    ///
    /// # Errors
    /// Returns an error when the child is absent or cannot be read safely.
    pub fn read(&self, name: &str) -> io::Result<String> {
        self.read_optional(name)?
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    /// Durably append one line to a regular file.
    ///
    /// # Errors
    /// Returns an error when the child cannot be opened, written, or synchronized safely.
    pub fn append_line(&self, name: &str, line: &str) -> io::Result<()> {
        let mut file = self.open_file(
            name,
            OFlag::O_CREAT
                | OFlag::O_APPEND
                | OFlag::O_WRONLY
                | OFlag::O_NOFOLLOW
                | OFlag::O_CLOEXEC,
        )?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()
    }

    /// Durably replace a regular file through a descriptor-relative rename.
    ///
    /// # Errors
    /// Returns an error when the replacement cannot be written, renamed, or synchronized.
    pub fn write_atomic(&self, name: &str, body: &str) -> io::Result<()> {
        validate_component(name)?;
        if self.read_optional(name)?.as_deref() == Some(body) {
            return Ok(());
        }
        let tmp = format!(".{name}.tmp-{}", std::process::id());
        let mut file = self.open_file(
            &tmp,
            OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        )?;
        let result = (|| {
            file.write_all(body.as_bytes())?;
            file.sync_all()?;
            renameat(&self.fd, tmp.as_str(), &self.fd, name).map_err(io_error)?;
            self.fd.sync_all()
        })();
        if result.is_err() {
            let _ = self.remove(&tmp);
        }
        result
    }

    /// List child names relative to the held directory.
    ///
    /// # Errors
    /// Returns an error when the held directory cannot be read.
    pub fn names(&self) -> io::Result<Vec<String>> {
        let mut dir = Dir::openat(
            &self.fd,
            ".",
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(io_error)?;
        let mut names = Vec::new();
        for entry in dir.iter() {
            let entry = entry.map_err(io_error)?;
            let name = entry.file_name().to_string_lossy();
            if name != "." && name != ".." {
                names.push(name.into_owned());
            }
        }
        Ok(names)
    }

    /// Test whether a regular child file exists, refusing other node kinds.
    ///
    /// # Errors
    /// Returns an error for an invalid name or unsafe child node.
    pub fn exists(&self, name: &str) -> io::Result<bool> {
        self.read_optional(name).map(|value| value.is_some())
    }

    /// Create and sync a descriptor-relative hard link.
    ///
    /// # Errors
    /// Returns an error for invalid names or when the link cannot be created or synchronized.
    pub fn hard_link(&self, from: &str, to: &str) -> io::Result<()> {
        validate_component(from)?;
        validate_component(to)?;
        linkat(&self.fd, from, &self.fd, to, AtFlags::empty()).map_err(io_error)?;
        self.fd.sync_all()
    }

    /// Remove a non-directory child and sync the held directory.
    ///
    /// # Errors
    /// Returns an error for an invalid name or when removal or synchronization fails.
    pub fn remove(&self, name: &str) -> io::Result<()> {
        validate_component(name)?;
        match unlinkat(&self.fd, name, UnlinkatFlags::NoRemoveDir) {
            Ok(()) => self.fd.sync_all(),
            Err(nix::errno::Errno::ENOENT) => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }

    fn open_file(&self, name: &str, flags: OFlag) -> io::Result<File> {
        validate_component(name)?;
        let fd = openat(&self.fd, name, flags, FILE_MODE).map_err(io_error)?;
        let file = File::from(fd);
        if !file.metadata()?.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} is not a regular file",
                    self.display.join(name).display()
                ),
            ));
        }
        Ok(file)
    }
}

fn open_or_create_dir(parent: &File, name: &str) -> io::Result<File> {
    match mkdirat(parent, name, DIR_MODE) {
        Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
        Err(error) => return Err(io_error(error)),
    }
    let fd = openat(
        parent,
        name,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(io_error)?;
    Ok(File::from(fd))
}

fn validate_component(value: &str) -> io::Result<()> {
    let mut components = Path::new(value).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(name)), None) if name != OsStr::new(".") => Ok(()),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "owned directory: path component is not contained",
        )),
    }
}

fn io_error(error: nix::errno::Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn owned_directory_operations_round_trip() {
        use nix::fcntl::{FcntlArg, FdFlag, fcntl};

        let root = tempfile::tempdir().expect("root");
        let dir = OwnedDir::create(root.path(), &["arm", "daily"]).expect("owned");
        let clone = dir.try_clone().expect("clone");

        assert_eq!(dir.read_optional("history.ndjson").expect("missing"), None);
        assert!(!dir.exists("history.ndjson").expect("missing existence"));

        dir.append_line("history.ndjson", "claim").expect("claim");
        clone
            .append_line("history.ndjson", "receipt")
            .expect("receipt");
        assert_eq!(
            dir.read("history.ndjson").expect("history"),
            "claim\nreceipt\n"
        );
        assert!(dir.exists("history.ndjson").expect("history existence"));

        dir.write_atomic("last.json", "first")
            .expect("first projection");
        dir.write_atomic("last.json", "first")
            .expect("stable projection");
        dir.write_atomic("last.json", "second")
            .expect("next projection");
        assert_eq!(dir.read("last.json").expect("projection"), "second");

        dir.hard_link("history.ndjson", "archive.ndjson")
            .expect("archive link");
        assert_eq!(
            dir.read("archive.ndjson").expect("archive"),
            "claim\nreceipt\n"
        );
        let mut names = dir.names().expect("names");
        names.sort();
        assert_eq!(names, ["archive.ndjson", "history.ndjson", "last.json"]);

        dir.remove("archive.ndjson").expect("remove archive");
        dir.remove("archive.ndjson").expect("idempotent remove");
        assert!(!dir.exists("archive.ndjson").expect("archive existence"));

        let lock = dir.open_lock("ledger.lock").expect("lock");
        assert!(lock.metadata().expect("lock metadata").is_file());
        let descriptor_flags = FdFlag::from_bits_truncate(
            fcntl(&lock, FcntlArg::F_GETFD).expect("lock descriptor flags"),
        );
        assert!(descriptor_flags.contains(FdFlag::FD_CLOEXEC));
        let status_flags =
            OFlag::from_bits_truncate(fcntl(&lock, FcntlArg::F_GETFL).expect("lock status flags"));
        assert!(status_flags.contains(OFlag::O_NONBLOCK));
        assert!(dir.exists("ledger.lock").expect("lock existence"));

        std::fs::create_dir(root.path().join("arm/daily/nested")).expect("nested child");
        assert!(dir.read("nested").is_err());
    }

    #[test]
    fn components_and_child_names_stay_contained() {
        let root = tempfile::tempdir().expect("root");
        for component in ["", ".", "..", "../escape", "nested/escape"] {
            assert!(OwnedDir::create(root.path(), &[component]).is_err());
        }
        let dir = OwnedDir::create(root.path(), &["held"]).expect("owned");
        assert!(dir.write_atomic("../escape", "no").is_err());
        assert!(!root.path().join("escape").exists());

        std::fs::write(root.path().join("not-a-directory"), "file").expect("regular file");
        assert!(OwnedDir::create(root.path(), &["not-a-directory"]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn file_and_directory_symlinks_fail_closed() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("root");
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).expect("outside");
        symlink(&outside, root.path().join("redirect")).expect("directory symlink");
        assert!(OwnedDir::create(root.path(), &["redirect"]).is_err());

        let dir = OwnedDir::create(root.path(), &["held"]).expect("owned");
        let target = root.path().join("outside.txt");
        std::fs::write(&target, "sentinel").expect("target");
        symlink(&target, root.path().join("held/history.ndjson")).expect("file symlink");
        assert!(dir.append_line("history.ndjson", "escaped").is_err());
        assert!(dir.read_optional("history.ndjson").is_err());

        symlink(&target, root.path().join("held/ledger.lock")).expect("lock symlink");
        assert!(dir.open_lock("ledger.lock").is_err());
        assert_eq!(std::fs::read_to_string(target).expect("target"), "sentinel");
    }

    #[cfg(unix)]
    #[test]
    fn visible_path_replacement_cannot_redirect_a_held_descriptor() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("root");
        let dir = OwnedDir::create(root.path(), &["held"]).expect("owned");
        dir.append_line("history.ndjson", "claim").expect("claim");
        std::fs::rename(root.path().join("held"), root.path().join("original")).expect("rename");
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).expect("outside");
        symlink(&outside, root.path().join("held")).expect("replacement");
        dir.append_line("history.ndjson", "receipt")
            .expect("receipt");
        assert_eq!(
            std::fs::read_to_string(root.path().join("original/history.ndjson")).expect("history"),
            "claim\nreceipt\n"
        );
        assert!(!outside.join("history.ndjson").exists());
    }
}
