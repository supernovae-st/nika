// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The worlds a closed copy is rehearsed on, fixed before the first run and in this order:
//! - **the user's own world**: the project as it is, its target an input when a file is there
//!   (observed bounded and never through a symlink: anything else refuses);
//! - **CRLF, a non-ASCII letter and a literal that looks like a template**, over a stale
//!   target;
//! - **an empty source** over a stale target.
//!
//! A discriminating world lives in a fixture root this door makes. Each root is made exclusively
//! under the scratch parent, never over an existing name, guarded the moment it exists, owner-only,
//! and removed with its absence verified. No folder this door did not create is ever removed.

use std::io;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::Closed;
use super::witness::{self, Seen};

/// The user's own world.
pub(super) const OBSERVED: &str = "observed";

/// The discriminating worlds: a name, the source's text, and the stale target's.
const DISCRIMINATING: [(&str, &str, &str); 2] = [
    (
        "crlf-unicode-template",
        "beta\r\ncafé ${{ const.not_code }}\n",
        "alpha\n",
    ),
    ("empty-over-stale", "", "stale"),
];

/// How many fresh names a fixture root is tried under before the door gives up.
const ATTEMPTS: u32 = 16;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// One world of the frozen list.
pub(super) struct World {
    pub(super) name: &'static str,
    /// The paths the room copies, as the request names them.
    pub(super) inputs: Vec<String>,
    /// What the user's target held before the run: `None` for a discriminating world.
    pub(super) before: Option<Seen>,
    origin: Origin,
}

/// Where a world's files come from.
enum Origin {
    /// The user's project, as it is.
    User(PathBuf),
    /// The files a fixture root is made of, each path as the request names it.
    Fixture(Vec<(String, Vec<u8>)>),
}

impl World {
    /// Whether this is the user's own world.
    pub(super) fn observed(&self) -> bool {
        matches!(self.origin, Origin::User(_))
    }

    /// The files this door writes for the world: none for the user's own.
    pub(super) fn files(&self) -> &[(String, Vec<u8>)] {
        match &self.origin {
            Origin::Fixture(files) => files,
            Origin::User(_) => &[],
        }
    }

    /// The root the room copies this world from: the user's project as it is, or a fixture root
    /// made now under `scratch` and holding exactly the world's files.
    pub(super) fn make(&self, scratch: &Path) -> Result<Made, String> {
        match &self.origin {
            Origin::User(root) => Ok(Made::User(root.clone())),
            Origin::Fixture(files) => {
                let fixture = FixtureRoot::create(scratch)
                    .map_err(|error| format!("no fixture root for {}: {error}", self.name))?;
                for (path, bytes) in files {
                    fixture.put(path, bytes).map_err(|error| {
                        format!("the world {} could not be made: {error}", self.name)
                    })?;
                }
                Ok(Made::Fixture(fixture))
            }
        }
    }
}

/// The frozen list for `closed` over the project at `root`: the user's world first, then the
/// discriminating ones. The user's target is an input when a regular file is there, no input when
/// nothing is, and refuses the list otherwise.
pub(super) async fn frozen(closed: &Closed, root: &Path) -> Result<Vec<World>, String> {
    let target = std::slice::from_ref(&closed.target);
    let before = match witness::observe(root, target).await?.pop() {
        Some(Ok(seen)) => seen,
        Some(Err(why)) => {
            return Err(format!(
                "the target {} cannot be observed: it {why}",
                closed.target
            ));
        }
        None => return Err(format!("the target {} was not observed", closed.target)),
    };
    let mut inputs = vec![closed.source.clone()];
    if matches!(before, Seen::File(_)) {
        inputs.push(closed.target.clone());
    }
    let mut worlds = vec![World {
        name: OBSERVED,
        inputs,
        before: Some(before),
        origin: Origin::User(root.to_path_buf()),
    }];
    for (name, source, stale) in DISCRIMINATING {
        worlds.push(World {
            name,
            inputs: vec![closed.source.clone(), closed.target.clone()],
            before: None,
            origin: Origin::Fixture(vec![
                (closed.source.clone(), source.as_bytes().to_vec()),
                (closed.target.clone(), stale.as_bytes().to_vec()),
            ]),
        });
    }
    Ok(worlds)
}

/// A world's root while it is rehearsed.
pub(super) enum Made {
    /// The user's project, as it is: never removed.
    User(PathBuf),
    /// A fixture root this door made, removed once the rehearsal is over.
    Fixture(FixtureRoot),
}

impl Made {
    /// Where the room copies from.
    pub(super) fn root(&self) -> &Path {
        match self {
            Self::User(root) => root,
            Self::Fixture(fixture) => &fixture.top,
        }
    }

    /// Remove a fixture root, then verify that nothing is left at its name; the user's project
    /// is left as it is.
    pub(super) fn remove(self) -> bool {
        match self {
            Self::User(_) => true,
            Self::Fixture(fixture) => fixture.remove(),
        }
    }
}

/// A fixture root made exclusively and owner-only, removed when dropped if nothing removed it.
pub(super) struct FixtureRoot {
    top: PathBuf,
    removed: bool,
}

impl FixtureRoot {
    /// A fresh root under `scratch`: a name nobody holds, made by this call or not at all.
    fn create(scratch: &Path) -> io::Result<Self> {
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        for _ in 0..ATTEMPTS {
            let top = scratch.join(format!(
                "nika-copy-world-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match builder.create(&top) {
                // Guarded at once: from here on, only this root is ever removed.
                Ok(()) => {
                    return Ok(Self {
                        top,
                        removed: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no fresh name for a fixture root",
        ))
    }

    /// Write `bytes` at `path` inside the root, its directories made owner-only.
    fn put(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let relative = witness::relative(path).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "not a path inside the root")
        })?;
        let at = self.top.join(relative);
        if let Some(parent) = at.parent() {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true).mode(0o700);
            builder.create(parent)?;
        }
        std::fs::write(at, bytes)
    }

    /// Remove the root, then verify that nothing is left at its name.
    fn remove(mut self) -> bool {
        self.removed = true;
        let _ = std::fs::remove_dir_all(&self.top);
        let left = std::fs::symlink_metadata(&self.top);
        matches!(left, Err(error) if error.kind() == io::ErrorKind::NotFound)
    }
}

impl Drop for FixtureRoot {
    fn drop(&mut self) {
        if !self.removed {
            let _ = std::fs::remove_dir_all(&self.top);
        }
    }
}
