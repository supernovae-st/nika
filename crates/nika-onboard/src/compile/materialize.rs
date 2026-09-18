// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI adapter: write a ready Compile candidate as a `.nika` file.
//!
//! The candidate payload stays path-free source text. This adapter does
//! not run, grant, or silently overwrite.

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
    let name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if !nika_source::is_canonical_program_file_name(name) {
        return Err(MaterializeError::NotAProgramPath(
            dest.display().to_string(),
        ));
    }
    if dest.exists() && !overwrite {
        return Err(MaterializeError::AlreadyExists(dest.display().to_string()));
    }
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|source| MaterializeError::Io {
            path: dest.display().to_string(),
            source,
        })?;
    }
    std::fs::write(dest, source).map_err(|source| MaterializeError::Io {
        path: dest.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::{CompileRequest, compile};

    #[test]
    fn ready_candidate_writes_nika_and_refuses_overwrite_and_retired() {
        let created = compile(
            &CompileRequest::create("classify-and-route")
                .answer("const.request", r#""An outage affects our customers.""#),
        )
        .expect("create");
        assert_eq!(created.status, CompileStatus::Ready);
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("website-brief.nika");
        materialize_ready(&created, &dest, false).expect("write");
        assert_eq!(
            std::fs::read_to_string(&dest).expect("read"),
            created.candidate.as_deref().expect("source")
        );
        let err = materialize_ready(&created, &dest, false).expect_err("no overwrite");
        assert!(matches!(err, MaterializeError::AlreadyExists(_)));
        materialize_ready(&created, &dest, true).expect("overwrite");

        let retired = dir.path().join("website-brief.nika.yaml");
        let err = materialize_ready(&created, &retired, true).expect_err("retired dest");
        assert!(matches!(err, MaterializeError::NotAProgramPath(_)));
        assert!(!retired.exists());
    }
}
