// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use nika_kernel::fs::FsError;

/// A complete, privately named file awaiting exclusive publication.
pub(super) struct StagedFile {
    path: PathBuf,
}

impl StagedFile {
    pub(super) fn create(destination: &Path, contents: &[u8]) -> Result<Self, FsError> {
        if destination.file_name().is_none() {
            return Err(FsError::InvalidData {
                path: destination.display().to_string(),
                reason: "write path has no file name".to_owned(),
            });
        }
        let (parent, path) = super::tmp_sibling(destination);
        if let Some(parent) = parent {
            std::fs::create_dir_all(parent).map_err(|error| FsError::from_io(&error, parent))?;
        }
        // Construct the cleanup owner only after exclusive creation succeeds:
        // a collision must never remove somebody else's temporary file.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    FsError::Io {
                        reason: format!(
                            "exclusive staging file already exists: {}",
                            path.display()
                        ),
                    }
                } else {
                    FsError::from_io(&error, &path)
                }
            })?;
        let staged = Self { path };
        let written = file
            .write_all(contents)
            .map_err(|error| FsError::from_io(&error, &staged.path));
        drop(file);
        written?;
        Ok(staged)
    }

    pub(super) fn publish(&self, destination: &Path) -> Result<(), FsError> {
        // The name is claimed atomically; existing files, directories and
        // symlinks survive. Unsupported filesystems fail without a rename fallback.
        std::fs::hard_link(&self.path, destination)
            .map_err(|error| FsError::from_io(&error, destination))
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        // Publication has already committed when cleanup follows a successful
        // link. A cleanup failure cannot undo it or turn it into a failed write.
        let _ = std::fs::remove_file(&self.path);
    }
}
