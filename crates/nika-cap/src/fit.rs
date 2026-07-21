// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pure "fits" predicate over a declared [`Permits`] boundary.
//!
//! Moved from the schema crate's check module (where these were private
//! `fn`s exercised only through `scan_escapes(parse(yaml))` — the module is
//! `nika_check::permits_fit` since the 2026-07-21 judgment split). Here they are
//! first-class + unit-tested in isolation. This is the STATIC, lexical half:
//! `..`/`.` are folded before comparison so a traversal that climbs OUT of a
//! glob's literal prefix no longer string-matches it. Symlink escapes still
//! need the runtime canonicalize-then-confine check (`NIKA-SEC-004`) — a
//! static pass cannot resolve a link.

use crate::{Permits, permits::glob_matches};

impl Permits {
    /// Whether `host` matches the declared `permits.net.http` allowlist.
    /// Default-deny: an omitted `net` block forbids all hosts. Uses the same
    /// matcher `nika-http` enforces at runtime, so check and run can't drift.
    #[must_use]
    pub fn allows_host(&self, host: &str) -> bool {
        self.net.as_ref().is_some_and(|n| {
            n.http
                .iter()
                .any(|g| nika_types::net::host_glob_matches(g, host))
        })
    }

    /// Whether `path` matches the declared `permits.fs` allowlist for the
    /// direction (`write` selects `fs.write`, else `fs.read`). Default-deny:
    /// an omitted `fs` block forbids all paths. Traversal-safe (see module doc).
    #[must_use]
    pub fn allows_path(&self, path: &str, write: bool) -> bool {
        self.fs.as_ref().is_some_and(|fs| {
            let globs = if write { &fs.write } else { &fs.read };
            globs.iter().any(|g| path_glob_matches(g, path))
        })
    }
}

/// Gitignore-style path glob match · supports a trailing `/**` (any
/// descendant) and a single `*` (any tail within a segment). Conservative:
/// when in doubt it does NOT match (default-deny favours flagging). The
/// `path` is lexically normalized FIRST so a `..` traversal out of the
/// glob's prefix no longer string-matches it.
#[must_use]
pub(crate) fn path_glob_matches(glob: &str, path: &str) -> bool {
    let path = lexically_normalize(path);
    let path = path.as_str();
    if let Some(prefix) = glob.strip_suffix("/**") {
        let prefix = lexically_normalize(prefix);
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    if glob.contains('*') {
        return glob_matches(glob, path);
    }
    lexically_normalize(glob) == path
}

/// Fold `.`/`..` segments textually, preserving a leading `/` or `./`. Purely
/// lexical — symlinks are the runtime's job.
///
/// A `..` cancels the preceding real segment. When there is none to cancel it
/// climbs *above* the root: for an **absolute** path that is a filesystem no-op
/// (`/..` == `/`), so it is dropped; for a **relative** path the escape MUST be
/// preserved (`./../data` stays out-of-boundary) — dropping it would collapse an
/// escaping path onto an in-boundary one, a false ACCEPT in the permit check.
#[must_use]
pub(crate) fn lexically_normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
    let dot_rooted = path.starts_with("./");
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                // Cancel a preceding real segment; a leading `..` (nothing real
                // to cancel) is preserved for relative paths, dropped for absolute.
                if matches!(out.last(), Some(&last) if last != "..") {
                    out.pop();
                } else if !absolute {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    let joined = out.join("/");
    if absolute {
        format!("/{joined}")
    } else if dot_rooted && !joined.starts_with("..") {
        // Keep the `./` root only when the result stays at-or-below it; an escaping
        // result (`../…`) is returned bare so it matches a `../**` glob, never `./**`.
        format!("./{joined}")
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FsPermits, NetPermits};

    #[test]
    fn host_default_deny_then_glob() {
        let mut p = Permits::new();
        assert!(!p.allows_host("api.github.com"), "omitted net = no hosts");
        p.net = Some(NetPermits {
            http: vec!["*.github.com".into()],
        });
        assert!(p.allows_host("api.github.com"));
        assert!(p.allows_host("github.com"), "*.x matches the bare apex");
        assert!(!p.allows_host("evil.com"));
    }

    #[test]
    fn path_direction_and_traversal() {
        let mut p = Permits::new();
        assert!(!p.allows_path("./out/x", false), "omitted fs = no paths");
        p.fs = Some(FsPermits {
            read: vec!["./data/**".into()],
            write: vec!["./out/report.json".into()],
        });
        assert!(p.allows_path("./data/a/b.txt", false));
        assert!(
            !p.allows_path("./data/a/b.txt", true),
            "read glob does not grant write"
        );
        assert!(p.allows_path("./out/report.json", true));
        // traversal OUT of the granted prefix must not match
        assert!(!p.allows_path("./data/../secret", false));
    }

    #[test]
    fn above_root_escape_is_not_a_false_accept() {
        // M1 regression (Gate-11 rust-pro review): a leading `..` that climbs
        // above the relative root must NOT collapse onto an in-boundary path.
        // `./../data/x` resolves OUTSIDE `./data/**` and must be rejected.
        let mut p = Permits::new();
        p.fs = Some(FsPermits {
            read: vec!["./data/**".into()],
            write: vec![],
        });
        assert!(
            p.allows_path("./data/x", false),
            "in-boundary still granted"
        );
        assert!(
            !p.allows_path("./../data/x", false),
            "above-root climb must not match the ./data/** glob"
        );
        assert!(
            !p.allows_path("../data/x", false),
            "bare ../ escape must not match either"
        );
    }

    #[test]
    fn normalize_preserves_relative_escape_drops_absolute() {
        // relative: the escape survives (M1)
        assert_eq!(lexically_normalize("./../data/x"), "../data/x");
        assert_eq!(lexically_normalize("../a/b"), "../a/b");
        // interior cancel still folds
        assert_eq!(lexically_normalize("./data/../secret"), "./secret");
        assert_eq!(lexically_normalize("./data/sub/../x"), "./data/x");
        // absolute: a root-level `..` is a filesystem no-op, dropped
        assert_eq!(lexically_normalize("/../etc"), "/etc");
        assert_eq!(lexically_normalize("/a/../b"), "/b");
    }

    // M3 (Gate-11): fuzz the path shape the algebra alphabet omits (`.`/`..`).
    use proptest::prelude::*;

    fn rel_path() -> impl Strategy<Value = String> {
        prop::collection::vec(prop_oneof!["a", "b", "..", "."], 1..6)
            .prop_map(|segs| format!("./{}", segs.join("/")))
    }

    proptest! {
        #[test]
        fn in_tree_glob_never_matches_an_escaping_path(p in rel_path()) {
            // The property that would have caught M1: an in-tree glob ("./a/**")
            // must never match a path normalizing to an above-root escape, and —
            // the contrapositive — any path it DOES match stays in-tree.
            let n = lexically_normalize(&p);
            if n.starts_with("..") {
                prop_assert!(
                    !path_glob_matches("./a/**", &p),
                    "escape {:?} → {:?} must not match ./a/**", p, n
                );
            }
            if path_glob_matches("./a/**", &p) {
                prop_assert!(
                    !n.starts_with(".."),
                    "a path matching ./a/** must not normalize to an escape: {:?} → {:?}", p, n
                );
            }
        }
    }
}
