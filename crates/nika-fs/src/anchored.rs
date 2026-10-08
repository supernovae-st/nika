// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! [`AnchoredFs`] · the host backend with relative paths anchored at a
//! selected root rather than at the process working directory.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use nika_kernel::fs::{FileMetadata, FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};

use crate::TokioFs;

/// [`TokioFs`] with its relative paths anchored at a root the caller selected.
///
/// A relative path in a run (`./notes/brief.md`) names a path under the root
/// the run's host selected: the launch directory of a local run, the admitted
/// project of a served one. The process working directory cannot carry that
/// root for a long-lived service, which shares one directory between every
/// project it runs and cannot change it for one run without changing it for
/// all. `AnchoredFs` holds the root as data: an operation on a relative,
/// non-empty path runs on `root.join(path)` through [`TokioFs`], and an
/// absolute or empty path runs verbatim.
///
/// The adapter changes coordinates, never policy. It grants nothing and
/// confines nothing: `..` and symlinks resolve exactly as they would from a
/// working directory at the root, and an absolute path keeps its meaning. The
/// `permits.fs` boundary resolves the path it judges through the same backend
/// that then performs the effect, so the judgment and the effect name one
/// file. The final-component pin, the atomic and exclusive writes and the
/// regular-only removal are [`TokioFs`]'s own, each reached explicitly rather
/// than through a refusing trait default.
///
/// A caller reads back its own spelling. A path the host reports for an
/// anchored operation, such as a [`list_dir`](FsListDyn::list_dir) or
/// [`glob`](FsListDyn::glob) entry or the path an error names, is given back
/// relative: listing `./notes` yields `./notes/a.md`, as it would from a
/// working directory at the root. [`canonicalize`](FsReadDyn::canonicalize)
/// keeps returning the physical absolute identity. A relative root is itself
/// resolved against the process working directory; an empty root leaves every
/// path verbatim, which is [`TokioFs`] exactly.
#[derive(Debug, Clone)]
pub struct AnchoredFs {
    root: PathBuf,
}

impl AnchoredFs {
    /// Anchor relative paths at `root`, the directory the caller selected.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The directory relative paths are anchored at.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the host operates for `path`: under the root for a relative,
    /// non-empty path; `None` runs `path` verbatim.
    fn host(&self, path: &Path) -> Option<PathBuf> {
        let anchored =
            path.is_relative() && !path.as_os_str().is_empty() && !self.root.as_os_str().is_empty();
        anchored.then(|| self.root.join(path))
    }

    /// The caller's spelling of `reported`, a path the host named while
    /// operating at `host` for the caller's `path`. The host path and each of
    /// its ancestors inside the root stand for `path` and its ancestors; the
    /// longest one `reported` descends from is spelled back, so an entry of
    /// `./notes` reads `./notes/a.md` and the staging sibling of a bare
    /// `a.md` reads `./.nika-tmp…`. Any other path is returned as reported.
    fn spelled(&self, reported: &Path, host: &Path, path: &Path) -> PathBuf {
        for (at, own) in host.ancestors().zip(path.ancestors()) {
            if !at.starts_with(&self.root) {
                break;
            }
            if let Ok(rest) = reported.strip_prefix(at) {
                // A bare name's empty parent is the working directory itself.
                let own = if own.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    own
                };
                return if rest.as_os_str().is_empty() {
                    own.to_path_buf()
                } else {
                    own.join(rest)
                };
            }
        }
        reported.to_path_buf()
    }

    /// `error` naming its path in the caller's spelling ([`Self::spelled`]).
    /// A reason text is kept verbatim.
    fn spelled_error(&self, error: FsError, host: &Path, path: &Path) -> FsError {
        let spell = |reported: String| {
            self.spelled(Path::new(&reported), host, path)
                .display()
                .to_string()
        };
        match error {
            FsError::NotFound { path } => FsError::NotFound { path: spell(path) },
            FsError::PermissionDenied { path } => FsError::PermissionDenied { path: spell(path) },
            FsError::AlreadyExists { path } => FsError::AlreadyExists { path: spell(path) },
            FsError::InvalidData { path, reason } => FsError::InvalidData {
                path: spell(path),
                reason,
            },
            FsError::SymlinkRefused { path } => FsError::SymlinkRefused { path: spell(path) },
            FsError::PinUnavailable { path } => FsError::PinUnavailable { path: spell(path) },
            other => other,
        }
    }

    /// A host listing made at `host`, each entry and any error in the
    /// caller's spelling. The order is the host's: every entry shares one
    /// prefix, so swapping that prefix keeps the sort.
    fn spelled_listing(
        &self,
        listed: Result<Vec<PathBuf>, FsError>,
        host: &Path,
        path: &Path,
    ) -> Result<Vec<PathBuf>, FsError> {
        match listed {
            Ok(entries) => Ok(entries
                .iter()
                .map(|entry| self.spelled(entry, host, path))
                .collect()),
            Err(error) => Err(self.spelled_error(error, host, path)),
        }
    }
}

impl FsReadDyn for AnchoredFs {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        match self.host(path) {
            None => TokioFs.read(path).await,
            Some(host) => TokioFs
                .read(&host)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }

    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        match self.host(path) {
            None => TokioFs.read_to_string(path).await,
            Some(host) => TokioFs
                .read_to_string(&host)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }

    async fn exists(&self, path: &Path) -> bool {
        match self.host(path) {
            None => TokioFs.exists(path).await,
            Some(host) => TokioFs.exists(&host).await,
        }
    }

    /// The physical absolute identity, as [`TokioFs`] resolves it from a
    /// working directory at the root (`.` is the root's own identity).
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        match self.host(path) {
            None => TokioFs.canonicalize(path).await,
            Some(host) => TokioFs
                .canonicalize(&host)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }

    /// [`TokioFs`]'s pin on the anchored path: the final component is opened
    /// `O_NOFOLLOW`, never the refusing default.
    async fn read_pinned(&self, path: &Path) -> Result<Bytes, FsError> {
        match self.host(path) {
            None => TokioFs.read_pinned(path).await,
            Some(host) => TokioFs
                .read_pinned(&host)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }
}

impl FsWriteDyn for AnchoredFs {
    /// [`TokioFs`]'s atomic temp-and-rename write, its parent created beside
    /// the anchored destination.
    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        match self.host(path) {
            None => TokioFs.write(path, contents).await,
            Some(host) => TokioFs
                .write(&host, contents)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }

    /// [`TokioFs`]'s exclusive publication, never the refusing default.
    async fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        match self.host(path) {
            None => TokioFs.write_new(path, contents).await,
            Some(host) => TokioFs
                .write_new(&host, contents)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }

    /// [`TokioFs`]'s regular-only removal, never the refusing default. The raw
    /// spelling is judged as joined, so a path naming no final file (`.`, `..`,
    /// a trailing separator) is still refused before any effect.
    async fn remove_regular_file(&self, path: &Path) -> Result<(), FsError> {
        match self.host(path) {
            None => TokioFs.remove_regular_file(path).await,
            Some(host) => TokioFs
                .remove_regular_file(&host)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }

    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        match self.host(path) {
            None => TokioFs.create_dir_all(path).await,
            Some(host) => TokioFs
                .create_dir_all(&host)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }

    async fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        match self.host(path) {
            None => TokioFs.remove_file(path).await,
            Some(host) => TokioFs
                .remove_file(&host)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }
}

impl FsMetaDyn for AnchoredFs {
    async fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError> {
        match self.host(path) {
            None => TokioFs.metadata(path).await,
            Some(host) => TokioFs
                .metadata(&host)
                .await
                .map_err(|e| self.spelled_error(e, &host, path)),
        }
    }
}

impl FsListDyn for AnchoredFs {
    /// The entries of the anchored directory, sorted, in the caller's
    /// spelling (`./notes` lists `./notes/a.md`).
    async fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        match self.host(path) {
            None => TokioFs.list_dir(path).await,
            Some(host) => self.spelled_listing(TokioFs.list_dir(&host).await, &host, path),
        }
    }

    /// [`TokioFs`]'s walk from the anchored root: the same matching, hidden
    /// directories and leaf symlinks, each hit in the caller's spelling.
    async fn glob(&self, root: &Path, pattern: &str) -> Result<Vec<PathBuf>, FsError> {
        match self.host(root) {
            None => TokioFs.glob(root, pattern).await,
            Some(host) => self.spelled_listing(TokioFs.glob(&host, pattern).await, &host, root),
        }
    }
}
