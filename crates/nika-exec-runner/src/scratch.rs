// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The per-spawn private scratch (issue 754 · the seatbelt arm).
//!
//! Split out of `egress.rs` at the 1,500-LOC file wall, and the wall
//! only forced a boundary that was already there: the proxy next door
//! fences the NETWORK, this fences the child's TEMP. They meet in one
//! line of `apply_sandbox` and nowhere else.
//!
//! ## Why a private scratch exists at all
//!
//! The seatbelt profile used to grant `/private/tmp`,
//! `/private/var/tmp` and `/private/var/folders` to every confined
//! child, unconditionally — so a task under the tightest declared
//! `permits.fs` could still persist files in world-known locations,
//! read the same user's temp caches, and hand data to a sibling task
//! across the per-task boundary. Those three lines left the preamble;
//! each spawn now gets its OWN directory instead, granted like any
//! other declared prefix and removed when the spawn settles.
//!
//! ## Why the CREATE is the security property
//!
//! What this module returns is handed straight to `spec.fs_write` — it
//! becomes an OS-enforced grant. It runs under a temp root every
//! same-uid process can write. So the path it mints is a path someone
//! else can reach first, and the only defence that matters is refusing
//! to ADOPT what is already there. See [`claim_dir`].

/// Claim ONE path exclusively as a directory we own, then resolve it.
///
/// `create_dir` refuses an existing entry of ANY kind
/// (`AlreadyExists`) — a plain directory, a file, and crucially a
/// SYMLINK, which `create_dir_all` would have adopted by following it.
/// Only once the entry is provably ours does `canonicalize` run, so it
/// can resolve nothing but our own directory's real path (the macOS
/// `/var/folders` → `/private/var/folders` fold the sandbox profile
/// needs to match), never a stranger's target.
///
/// Measured 2026-08-03, on the adversarial review of the commit that
/// introduced this: `create_dir_all` treats a pre-planted
/// symlink-to-directory as "already there" — Ok, no error, because
/// `is_dir()` follows links — and the `canonicalize` after it hands
/// back the link's TARGET. A co-resident process planting
/// `nika-scratch-<pid>-0 -> $HOME` would have had the sandbox grant its
/// own confined child read+write across the entire home directory,
/// through the mechanism that exists to CLOSE an ambient-tmp hole. The
/// system-root guard would not have caught it either: it refuses the
/// bare `/Users`, never `/Users/<someone>`.
fn claim_dir(dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir(dir)?;
    std::fs::canonicalize(dir)
}

/// Mint one per-spawn scratch directory under the runner's temp root.
///
/// The name is widened past `pid + seq` with nanos because a
/// co-resident process predicts that pair exactly — but that is
/// defence in depth only. [`claim_dir`]'s exclusive create is what
/// makes a GUESSED name harmless, and it is the property the tests
/// pin.
pub(crate) fn mint_scratch_dir() -> Result<std::path::PathBuf, nika_kernel::ShellError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SCRATCH_SEQ: AtomicU64 = AtomicU64::new(0);
    // A bounded ladder: each rung is a fresh name, and an occupied one is
    // never adopted. Sixteen collisions in a row is not a race lost, it is
    // a temp root someone is farming — refuse rather than reach further.
    const RUNGS: u32 = 16;
    let root = std::env::temp_dir();
    let mut last: Option<(std::path::PathBuf, std::io::Error)> = None;
    for _ in 0..RUNGS {
        let seq = SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let dir = root.join(format!(
            "nika-scratch-{}-{seq}-{nonce:08x}",
            std::process::id()
        ));
        match claim_dir(&dir) {
            Ok(claimed) => return Ok(claimed),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some((dir, e)),
            Err(e) => {
                return Err(nika_kernel::ShellError::Other {
                    reason: format!(
                        "could not mint the per-spawn sandbox scratch {}: {e}",
                        dir.display()
                    ),
                });
            }
        }
    }
    let detail = last.map_or_else(
        || "no candidate was tried".to_owned(),
        |(dir, e)| format!("last {} · {e}", dir.display()),
    );
    Err(nika_kernel::ShellError::Other {
        reason: format!(
            "could not mint a per-spawn sandbox scratch after {RUNGS} attempts ({detail}) — \
             refusing to adopt an existing path as the sandbox's writable grant"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE plant (adversarial review 2026-08-03). What the mint returns
    /// becomes an OS-enforced `fs_write` GRANT, under a temp root every
    /// same-uid process can write — so a pre-planted entry must never be
    /// ADOPTED.
    ///
    /// Both arms are pinned, because only the pair proves it: the claim
    /// REFUSES a plant (a symlink AND a plain directory), and it still
    /// succeeds on a free name. A test that only walked the happy path
    /// is exactly what let the first draft ship.
    #[test]
    fn claiming_refuses_a_planted_symlink_and_takes_a_free_name() {
        let target = std::env::temp_dir().join(format!("nika-decoy-tgt-{}", std::process::id()));
        std::fs::create_dir_all(&target).expect("decoy target");
        let planted = std::env::temp_dir().join(format!("nika-decoy-{}", std::process::id()));
        let _ = std::fs::remove_file(&planted);
        let _ = std::fs::remove_dir_all(&planted);
        std::os::unix::fs::symlink(&target, &planted).expect("plant the symlink");

        // The plant is refused — never resolved to its target.
        let refused = claim_dir(&planted);
        assert!(
            refused
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists),
            "a planted symlink must be REFUSED, got {refused:?}"
        );

        // A plain pre-existing directory is equally not ours to adopt.
        let occupied = std::env::temp_dir().join(format!("nika-occupied-{}", std::process::id()));
        std::fs::create_dir_all(&occupied).expect("occupied");
        assert!(
            claim_dir(&occupied)
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::AlreadyExists),
            "an existing directory is not ours either"
        );

        // …and a free name still works, resolved to a REAL directory.
        let free = std::env::temp_dir().join(format!("nika-free-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&free);
        let claimed = claim_dir(&free).expect("a free name claims");
        assert!(
            std::fs::symlink_metadata(&claimed)
                .expect("claimed exists")
                .is_dir(),
            "the claim must own a real directory, never a link: {claimed:?}"
        );

        let _ = std::fs::remove_dir_all(&claimed);
        let _ = std::fs::remove_file(&planted);
        let _ = std::fs::remove_dir_all(&occupied);
        let _ = std::fs::remove_dir_all(&target);
    }

    /// Two spawns never share a scratch, and each owns a real directory.
    #[test]
    fn the_mint_survives_an_occupied_rung() {
        let first = mint_scratch_dir().expect("mints");
        let second = mint_scratch_dir().expect("mints again");
        assert_ne!(first, second, "two spawns never share a scratch");
        for d in [&first, &second] {
            assert!(std::fs::symlink_metadata(d).expect("exists").is_dir());
            let _ = std::fs::remove_dir_all(d);
        }
    }
}
