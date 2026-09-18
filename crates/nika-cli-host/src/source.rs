// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow bytes plus the provenance that governs `--fix` / clean-gate.
//!
//! Disk program acquisition is suffix-gated (`*.nika`). Stdin (`-`) and
//! already-acquired bytes stay filename-independent.

use std::io::Read as _;

use nika_display::check_render::RepairTarget;
use nika_source::{SourceNameKind, classify_path, path_file_name, retired_rename_hint};

use crate::output::VerbOutput;
use crate::repair::repair_target_for_path;

#[derive(Clone)]
pub struct RunSource {
    logical_path: std::sync::Arc<str>,
    source: std::sync::Arc<str>,
    repair_target: RepairTarget,
}

impl RunSource {
    /// # Errors
    /// Unreadable path is an environment refusal; non-UTF-8 is a parse refusal.
    pub fn capture(path: &str) -> Result<Self, VerbOutput> {
        Self::capture_with_repair_target(path, repair_target_for_path(path))
    }

    /// # Errors
    /// Unreadable path is an environment refusal; non-UTF-8 is a parse refusal.
    pub fn capture_with_repair_target(
        path: &str,
        repair_target: RepairTarget,
    ) -> Result<Self, VerbOutput> {
        let bytes = if path == "-" {
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| VerbOutput::env(format!("cannot read stdin: {e}")))?;
            buf
        } else {
            if let Some(out) = refuse_disk_program(path) {
                return Err(out);
            }
            read_regular_program(path)?
        };
        Self::from_bytes_with_repair_target(path, bytes, repair_target).map_err(|_| {
            VerbOutput::file(format!(
                "PARSE ✗  {}",
                nika_schema::SchemaError::YamlSyntax {
                    message: "workflow source is not valid UTF-8".to_owned(),
                    span: None,
                }
                .diagnostic()
            ))
        })
    }

    /// # Errors
    /// Fails when `bytes` are not valid UTF-8.
    pub fn from_bytes(logical_path: impl Into<String>, bytes: Vec<u8>) -> std::io::Result<Self> {
        Self::from_bytes_with_repair_target(logical_path, bytes, RepairTarget::WorkspaceFile)
    }

    fn from_bytes_with_repair_target(
        logical_path: impl Into<String>,
        bytes: Vec<u8>,
        repair_target: RepairTarget,
    ) -> std::io::Result<Self> {
        let source = String::from_utf8(bytes).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, error.utf8_error())
        })?;
        Ok(Self {
            logical_path: std::sync::Arc::from(logical_path.into()),
            source: std::sync::Arc::from(source),
            repair_target,
        })
    }

    #[must_use]
    pub fn logical_path(&self) -> &str {
        &self.logical_path
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn repair_target(&self) -> RepairTarget {
        self.repair_target
    }
}

/// Live on-disk program entry: canonical `*.nika` only. Stdin is not a
/// disk path. `nika.yaml` stays a project file, never a workflow.
/// Trailing separators are directory shapes, not program files.
/// Regular-file proof is on the opened descriptor, not a metadata race.
fn refuse_disk_program(path: &str) -> Option<VerbOutput> {
    let name = path_file_name(path).unwrap_or(path);
    match classify_path(path) {
        SourceNameKind::CanonicalProgram => None,
        SourceNameKind::RetiredProgram => {
            Some(VerbOutput::file(retired_rename_hint(name).unwrap_or_else(
                || format!("`{path}` uses a retired Nika program suffix; rename to `*.nika`"),
            )))
        }
        SourceNameKind::ProjectFile => Some(VerbOutput::file(format!(
            "`{path}` is the project file, not a program — `nika check nika.yaml` audits the project; name a `*.nika` program to check or run as a workflow"
        ))),
        _ => Some(VerbOutput::file(format!(
            "`{path}` is not a Nika program; live program files use the `*.nika` suffix"
        ))),
    }
}

/// Open then prove the descriptor is a regular file before reading, so a
/// FIFO or swapped special file cannot block or race past the suffix gate.
/// Symlinks to regular files follow the existing CLI policy (no `O_NOFOLLOW`).
/// Unix opens `O_NONBLOCK` first, matching `nika-fs` owned-dir acquisition,
/// then fstats that fd. `O_NONBLOCK` does not change regular-file I/O.
fn read_regular_program(path: &str) -> Result<Vec<u8>, VerbOutput> {
    let mut file = open_program_file(path)?;
    let meta = file
        .metadata()
        .map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))?;
    if !meta.is_file() {
        return Err(VerbOutput::env(format!(
            "`{path}` is not a regular Nika program file"
        )));
    }
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))?;
    Ok(buf)
}

fn open_program_file(path: &str) -> Result<std::fs::File, VerbOutput> {
    #[cfg(unix)]
    {
        use nix::fcntl::{OFlag, open};
        use nix::sys::stat::Mode;
        open(
            path,
            OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .map(std::fs::File::from)
        .map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))
    }
    #[cfg(not(unix))]
    {
        std::fs::File::open(path).map_err(|e| VerbOutput::env(format!("cannot read {path}: {e}")))
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::manual_let_else)]
mod tests {
    use super::*;
    use crate::output::exit;

    #[test]
    fn capture_accepts_canonical_program_and_rejects_retired_and_project() {
        let dir = tempfile::tempdir().expect("tmp");
        let program = dir.path().join("support.nika");
        std::fs::write(&program, "nika: support\ntasks: {}\n").expect("write");
        let captured = RunSource::capture(program.to_str().expect("utf8"))
            .unwrap_or_else(|e| panic!("canonical: {}", e.text));
        assert!(captured.source().contains("nika: support"));

        let retired = dir.path().join("support.nika.yaml");
        std::fs::write(&retired, "nika: support\ntasks: {}\n").expect("write retired");
        let err = match RunSource::capture(retired.to_str().expect("utf8")) {
            Ok(_) => panic!("retired"),
            Err(err) => err,
        };
        assert_eq!(err.code, exit::FILE);
        assert!(err.text.contains("retired"), "{}", err.text);
        assert!(err.text.contains("support.nika"), "{}", err.text);

        let project = dir.path().join("nika.yaml");
        std::fs::write(&project, "nika: proj\n").expect("write project");
        let err = match RunSource::capture(project.to_str().expect("utf8")) {
            Ok(_) => panic!("project"),
            Err(err) => err,
        };
        assert_eq!(err.code, exit::FILE);
        assert!(err.text.contains("project file"), "{}", err.text);

        let yaml = dir.path().join("notes.yaml");
        std::fs::write(&yaml, "nika: notes\ntasks: {}\n").expect("write yaml");
        let err = match RunSource::capture(yaml.to_str().expect("utf8")) {
            Ok(_) => panic!("plain yaml"),
            Err(err) => err,
        };
        assert_eq!(err.code, exit::FILE);
        assert!(err.text.contains("not a Nika program"), "{}", err.text);

        let nested = dir.path().join("support.v2.nika");
        std::fs::write(&nested, "nika: v2\ntasks: {}\n").expect("write v2");
        RunSource::capture(nested.to_str().expect("utf8")).unwrap_or_else(|e| panic!("{}", e.text));

        let mixed = dir.path().join("foo.NIKA");
        std::fs::write(&mixed, "nika: foo\ntasks: {}\n").expect("write mixed");
        let err = match RunSource::capture(mixed.to_str().expect("utf8")) {
            Ok(_) => panic!("mixed case"),
            Err(err) => err,
        };
        assert_eq!(err.code, exit::FILE);

        let dir_named = dir.path().join("something.nika");
        std::fs::create_dir(&dir_named).expect("dir");
        let err = match RunSource::capture(dir_named.to_str().expect("utf8")) {
            Ok(_) => panic!("directory"),
            Err(err) => err,
        };
        assert_eq!(err.code, exit::ENV);
        assert!(err.text.contains("not a regular"), "{}", err.text);

        let slash = format!("{}/", program.display());
        let err = match RunSource::capture(&slash) {
            Ok(_) => panic!("trailing slash"),
            Err(err) => err,
        };
        assert_eq!(err.code, exit::FILE);

        #[cfg(unix)]
        {
            let alias = dir.path().join("alias.nika");
            std::os::unix::fs::symlink(&program, &alias).expect("symlink to file");
            RunSource::capture(alias.to_str().expect("utf8"))
                .unwrap_or_else(|e| panic!("{}", e.text));

            let realdir = dir.path().join("realdir");
            std::fs::create_dir(&realdir).expect("dir");
            let dir_link = dir.path().join("dirlink.nika");
            std::os::unix::fs::symlink(&realdir, &dir_link).expect("symlink to dir");
            let err = match RunSource::capture(dir_link.to_str().expect("utf8")) {
                Ok(_) => panic!("dir link"),
                Err(err) => err,
            };
            assert_eq!(err.code, exit::ENV);

            let fifo = dir.path().join("pipe.nika");
            nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRUSR).expect("fifo");
            let err = match RunSource::capture(fifo.to_str().expect("utf8")) {
                Ok(_) => panic!("fifo"),
                Err(err) => err,
            };
            assert_eq!(err.code, exit::ENV);
            assert!(err.text.contains("not a regular"), "{}", err.text);
        }
    }

    #[test]
    fn from_bytes_stays_filename_independent() {
        let source =
            RunSource::from_bytes("-", b"nika: stdin\ntasks: {}\n".to_vec()).expect("utf8");
        assert_eq!(source.logical_path(), "-");
        let retired_name =
            RunSource::from_bytes("historical.nika.yaml", b"nika: hist\ntasks: {}\n".to_vec())
                .expect("bytes");
        assert_eq!(retired_name.logical_path(), "historical.nika.yaml");
    }
}
