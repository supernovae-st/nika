// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI adapter: write a ready Compile candidate as a `.nika` file.
//!
//! The candidate payload stays path-free source text. This adapter does
//! not run, grant, or silently overwrite.

use std::fs::OpenOptions;
use std::io::{self, ErrorKind, Write as _};
use std::path::Path;

use super::types::{CompileOutcome, CompileStatus};

/// Refusal while writing a ready candidate to disk.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MaterializeError {
    /// Incomplete or failed authoring is not written.
    #[error("compile candidate is not ready to write")]
    NotReady,
    /// Destination is not a canonical `*.nika` program path.
    #[error("`{0}` is not a canonical `.nika` program path")]
    NotAProgramPath(String),
    /// Existing files require an explicit overwrite.
    #[error("`{0}` exists — pass overwrite to replace it")]
    AlreadyExists(String),
    /// Filesystem write failed.
    #[error("cannot write `{path}`: {source}")]
    Io {
        /// Destination path.
        path: String,
        /// Operating-system error.
        source: std::io::Error,
    },
}

/// Write `outcome.candidate` to `dest` when status is Ready.
///
/// # Errors
/// Not-ready outcomes, non-canonical destinations, existing files without
/// overwrite, and I/O failures.
pub fn materialize_ready(
    outcome: &CompileOutcome,
    dest: &Path,
    overwrite: bool,
) -> Result<(), MaterializeError> {
    if outcome.status != CompileStatus::Ready {
        return Err(MaterializeError::NotReady);
    }
    let Some(source) = outcome.candidate.as_deref() else {
        return Err(MaterializeError::NotReady);
    };
    let raw = dest.to_str().unwrap_or("");
    if raw.ends_with('/') || raw.ends_with('\\') || !nika_source::is_canonical_program_path(raw) {
        return Err(MaterializeError::NotAProgramPath(
            dest.display().to_string(),
        ));
    }
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| MaterializeError::Io {
            path: dest.display().to_string(),
            source,
        })?;
    }
    if overwrite {
        replace_regular(dest, source)
    } else {
        create_new(dest, source)
    }
}

fn io_err(dest: &Path, source: io::Error) -> MaterializeError {
    MaterializeError::Io {
        path: dest.display().to_string(),
        source,
    }
}

fn create_new(dest: &Path, source: &str) -> Result<(), MaterializeError> {
    match OpenOptions::new().write(true).create_new(true).open(dest) {
        Ok(mut file) => file
            .write_all(source.as_bytes())
            .map_err(|source| io_err(dest, source)),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            Err(MaterializeError::AlreadyExists(dest.display().to_string()))
        }
        Err(error) => Err(io_err(dest, error)),
    }
}

fn replace_regular(dest: &Path, source: &str) -> Result<(), MaterializeError> {
    match dest.symlink_metadata() {
        Err(error) if error.kind() == ErrorKind::NotFound => return create_new(dest, source),
        Err(error) => return Err(io_err(dest, error)),
        Ok(meta) if meta.is_file() || meta.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(io_err(
                dest,
                io::Error::new(ErrorKind::InvalidInput, "destination is not a regular file"),
            ));
        }
    }
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|error| io_err(dest, error))?;
    tmp.write_all(source.as_bytes())
        .map_err(|error| io_err(dest, error))?;
    tmp.persist(dest)
        .map_err(|error| io_err(dest, error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{CompileRequest, compile};

    fn ready() -> CompileOutcome {
        let created = compile(
            &CompileRequest::create("classify-and-route")
                .answer("const.request", r#""An outage affects our customers.""#),
        )
        .expect("create");
        assert_eq!(created.status, CompileStatus::Ready);
        created
    }

    #[test]
    fn ready_candidate_writes_nika_and_refuses_overwrite_and_retired() {
        let created = ready();
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("website-brief.nika");
        materialize_ready(&created, &dest, false).expect("write");
        assert_eq!(
            std::fs::read_to_string(&dest).expect("read"),
            created.candidate.as_deref().expect("source")
        );
        let err = materialize_ready(&created, &dest, false).expect_err("no overwrite");
        assert!(matches!(err, MaterializeError::AlreadyExists(_)));
        assert_eq!(
            std::fs::read_to_string(&dest).expect("unchanged"),
            created.candidate.as_deref().expect("source")
        );
        materialize_ready(&created, &dest, true).expect("overwrite");

        let retired = dir.path().join("website-brief.nika.yaml");
        let err = materialize_ready(&created, &retired, true).expect_err("retired dest");
        assert!(matches!(err, MaterializeError::NotAProgramPath(_)));
        assert!(!retired.exists());
    }

    #[test]
    fn dangling_symlink_is_already_exists_and_is_not_followed() {
        let created = ready();
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("website-brief.nika");
        let missing = dir.path().join("missing-target");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&missing, &dest).expect("dangling");
        #[cfg(not(unix))]
        {
            let _ = (created, dest, missing);
            return;
        }
        let err = materialize_ready(&created, &dest, false).expect_err("no follow");
        assert!(matches!(err, MaterializeError::AlreadyExists(_)));
        assert!(
            dest.symlink_metadata()
                .expect("link")
                .file_type()
                .is_symlink()
        );
        assert!(!missing.exists());
    }

    #[test]
    fn trailing_separator_is_not_a_program_and_creates_no_parent() {
        let created = ready();
        let dir = tempfile::tempdir().expect("tmp");
        let ghost = dir.path().join("ghost");
        let dest = format!("{}/website-brief.nika/", ghost.display());
        let err = materialize_ready(&created, Path::new(&dest), false).expect_err("slash");
        assert!(matches!(err, MaterializeError::NotAProgramPath(_)));
        assert!(!ghost.exists());
        let dest = format!("{}\\website-brief.nika\\", ghost.display());
        let err = materialize_ready(&created, Path::new(&dest), false).expect_err("backslash");
        assert!(matches!(err, MaterializeError::NotAProgramPath(_)));
        assert!(!ghost.exists());
    }

    #[test]
    fn existing_regular_output_is_unchanged_without_overwrite() {
        let created = ready();
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("website-brief.nika");
        std::fs::write(&dest, "KEEP\n").expect("seed");
        let err = materialize_ready(&created, &dest, false).expect_err("exists");
        assert!(matches!(err, MaterializeError::AlreadyExists(_)));
        assert_eq!(std::fs::read_to_string(&dest).expect("read"), "KEEP\n");
    }

    #[test]
    fn overwrite_replaces_a_symlink_without_touching_its_target() {
        let created = ready();
        let dir = tempfile::tempdir().expect("tmp");
        let target = dir.path().join("real-target");
        std::fs::write(&target, "TARGET\n").expect("target");
        let dest = dir.path().join("website-brief.nika");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &dest).expect("symlink");
        #[cfg(not(unix))]
        {
            let _ = (created, dest, target);
            return;
        }
        materialize_ready(&created, &dest, true).expect("replace symlink");
        assert_eq!(
            std::fs::read_to_string(&dest).expect("new file"),
            created.candidate.as_deref().expect("source")
        );
        assert_eq!(std::fs::read_to_string(&target).expect("kept"), "TARGET\n");
        assert!(
            !dest
                .symlink_metadata()
                .expect("meta")
                .file_type()
                .is_symlink()
        );
    }

    #[cfg(unix)]
    #[test]
    fn overwrite_refuses_a_fifo() {
        use std::os::unix::fs::FileTypeExt as _;
        let created = ready();
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("website-brief.nika");
        nix::unistd::mkfifo(&dest, nix::sys::stat::Mode::S_IRUSR).expect("fifo");
        let err = materialize_ready(&created, &dest, true).expect_err("fifo");
        assert!(matches!(err, MaterializeError::Io { .. }));
        assert!(dest.symlink_metadata().expect("fifo").file_type().is_fifo());
    }
}
