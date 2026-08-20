// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Descriptor-rooted filesystem ownership for one arm sidecar.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read as _, Seek as _, Write as _};
use std::path::{Component, Path, PathBuf};

use nix::dir::Dir;
use nix::fcntl::{AtFlags, OFlag, openat, renameat};
use nix::sys::stat::{Mode, mkdirat};
use nix::unistd::{UnlinkatFlags, linkat, unlinkat};

const DIR_MODE: Mode = Mode::from_bits_truncate(0o700);
const FILE_MODE: Mode = Mode::from_bits_truncate(0o600);

/// An opened sidecar directory. Every child access is relative to this held
/// descriptor, so renaming or replacing the visible path cannot redirect I/O.
#[derive(Debug)]
pub(super) struct SafeDir {
    fd: File,
    display: PathBuf,
}

impl SafeDir {
    pub(super) fn open(project: &Path, label: &str) -> io::Result<Self> {
        validate_component(label)?;
        let display = project.join(".nika/arm").join(label);
        let project_fd = File::open(project)?;
        let nika = open_or_create_dir(&project_fd, ".nika")?;
        let arm = open_or_create_dir(&nika, "arm")?;
        let fd = open_or_create_dir(&arm, label)?;
        Ok(Self { fd, display })
    }

    pub(super) fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            fd: self.fd.try_clone()?,
            display: self.display.clone(),
        })
    }

    pub(super) fn open_lock(&self, name: &str) -> io::Result<File> {
        self.open_file(
            name,
            OFlag::O_CREAT
                | OFlag::O_RDWR
                | OFlag::O_NOFOLLOW
                | OFlag::O_NONBLOCK
                | OFlag::O_CLOEXEC,
        )
    }

    pub(super) fn read_optional(&self, name: &str) -> io::Result<Option<String>> {
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

    pub(super) fn read(&self, name: &str) -> io::Result<String> {
        self.read_optional(name)?
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    pub(super) fn append_line(&self, name: &str, line: &str) -> io::Result<()> {
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

    pub(super) fn write_atomic(&self, name: &str, body: &str) -> io::Result<()> {
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

    pub(super) fn names(&self) -> io::Result<Vec<String>> {
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
            if name == "." || name == ".." {
                continue;
            }
            let name = name.into_owned();
            names.push(name);
        }
        Ok(names)
    }

    pub(super) fn exists(&self, name: &str) -> io::Result<bool> {
        self.read_optional(name).map(|value| value.is_some())
    }

    pub(super) fn hard_link(&self, from: &str, to: &str) -> io::Result<()> {
        linkat(&self.fd, from, &self.fd, to, AtFlags::empty()).map_err(io_error)?;
        self.fd.sync_all()
    }

    pub(super) fn remove(&self, name: &str) -> io::Result<()> {
        match unlinkat(&self.fd, name, UnlinkatFlags::NoRemoveDir) {
            Ok(()) => self.fd.sync_all(),
            Err(nix::errno::Errno::ENOENT) => Ok(()),
            Err(error) => Err(io_error(error)),
        }
    }

    pub(super) fn read_lock_owner(file: &mut File) -> io::Result<String> {
        file.seek(std::io::SeekFrom::Start(0))?;
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        Ok(text)
    }

    fn open_file(&self, name: &str, flags: OFlag) -> io::Result<File> {
        validate_component(name)?;
        let fd = openat(&self.fd, name, flags, FILE_MODE).map_err(io_error)?;
        let file = File::from(fd);
        if !file.metadata()?.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "arm sidecar: {} is not a regular file",
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
            "arm sidecar: path component is not contained",
        )),
    }
}

fn io_error(error: nix::errno::Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}
