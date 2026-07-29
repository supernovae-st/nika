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

use crate::Permits;

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
    // BOTH sides fold, then ONE walk decides. The three-armed shape this
    // replaced (`/**` prefix test · trailing-star `glob_matches` · literal
    // equality) folded the glob in one arm and not the others, so the same
    // pair got different answers depending on where the star sat.
    let path = lexically_normalize(path);
    let glob = lexically_normalize(glob);
    // An escaping path (`../x`) must never satisfy an in-tree glob. After the
    // fold an escape is the ONLY thing that keeps a leading `..`, and segment
    // walking would happily match `..` against a `*`, so the guard is here and
    // not inside the walker: a glob may only admit an escape by declaring one.
    if path.starts_with("..") && !glob.starts_with("..") {
        return false;
    }
    glob_admits(&glob, &path)
}

/// Whether one path SEGMENT matches one glob segment. `*` matches any run of
/// characters, and — the load-bearing part — never a `/`, because a segment
/// by construction contains none.
fn segment_matches(pat: &str, seg: &str) -> bool {
    match pat.split_once('*') {
        None => pat == seg,
        Some((pre, post)) => {
            seg.len() >= pre.len() + post.len() && seg.starts_with(pre) && seg.ends_with(post)
        }
    }
}

/// Walk a segmented glob against a segmented path.
///
/// `**` matches any number of segments INCLUDING zero; every other segment
/// matches exactly one. So `data/**` admits `data` itself and any descendant,
/// while `data/*` admits exactly one level and `data/*.csv` admits one level
/// whose name ends `.csv`.
fn walk(pats: &[&str], segs: &[&str]) -> bool {
    match pats.split_first() {
        None => segs.is_empty(),
        Some((&"**", rest)) => {
            // A trailing `**` takes whatever is left, at any depth.
            rest.is_empty() || (0..=segs.len()).any(|i| walk(rest, &segs[i..]))
        }
        Some((pat, rest)) => match segs.split_first() {
            Some((seg, more)) if segment_matches(pat, seg) => walk(rest, more),
            _ => false,
        },
    }
}

/// Split on `/`, dropping empties so `a//b` and `a/b` segment identically.
fn segments(s: &str) -> Vec<&str> {
    s.split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect()
}

/// Whether `path` (already segmented) is inside the fs permit `glob`.
///
/// THE single fs boundary predicate. It is `pub` for the same reason
/// `nika_types::net::host_glob_matches` is: the runtime re-gate
/// (`NIKA-SEC-004`) MUST decide with the same function the static check
/// decides with, or the two verdicts drift — and when they drift it is the
/// permissive one that is authoritative.
///
/// They did drift. Measured 2026-07-28 on the published `nika 0.106.1`, with
/// no attacker, no symlink and no traversal:
///
/// ```text
/// permits.fs.read:  ["data/*.csv"]  →  read  data/sub/deeper/private.key
/// permits.fs.write: ["out/*.md"]    →  wrote out/sub/pwned.sh
/// ```
///
/// Both printed `PERMITS body fits the declared boundary` and `0 hints`, and
/// the read's content landed in the signed trace. The runtime
/// (`nika-builtin::permits::confines`) split a glob at its first wildcard
/// component, kept the literal prefix, and admitted ANY descendant of it —
/// the wildcard half was discarded and never re-applied, so `data/*.csv`
/// meant `data/**` and the extension was decoration. Its own comment said so
/// (`<root>/**` · `<root>/*` etc. — any descendant), which is why a test
/// pinned the behaviour rather than catching it.
///
/// The static side was no better, differently: its matcher was a
/// trailing-star prefix test, so `data/*` also crossed `/`, and any glob
/// whose star was not final (`*.csv`, `data/*.md`) matched nothing at all —
/// a silently inert grant.
///
/// Both are one function now, and `*` stops at a separator on both sides.
#[must_use]
pub fn glob_admits(glob: &str, path: &str) -> bool {
    let mut pattern = segments(glob);
    // Collapse runs of `**`. Without this, `**/**/**/…` against a long path
    // backtracks exponentially, and permits come from a file we did not write.
    pattern.dedup_by(|a, b| *a == "**" && *b == "**");
    walk(&pattern, &segments(path))
}

/// Fold `.`/`..` segments textually into ONE canonical form. Purely lexical —
/// symlinks are the runtime's job.
///
/// Every relative path that stays in-tree comes back `./`-rooted whether or not
/// the caller wrote the `./`, so the two spellings of one file compare equal.
/// The tree root itself is `"."`, never the empty string.
///
/// A `..` cancels the preceding real segment. When there is none to cancel it
/// climbs *above* the root: for an **absolute** path that is a filesystem no-op
/// (`/..` == `/`), so it is dropped; for a **relative** path the escape MUST be
/// preserved (`./../data` stays out-of-boundary) — dropping it would collapse an
/// escaping path onto an in-boundary one, a false ACCEPT in the permit check.
///
/// `pub` since F-O1 PR-2: the runtime re-gate (`NIKA-SEC-004`) prints THIS
/// canonical form in its refusals — the same fold [`Permits::allows_path`]
/// matches against, so the message can never disagree with the verdict.
#[must_use]
pub fn lexically_normalize(path: &str) -> String {
    let absolute = path.starts_with('/');
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
    } else if joined.starts_with("..") {
        // An escaping result is returned bare: it must match a `../**` glob and
        // never a `./**` one. This is the load-bearing half of the marker.
        joined
    } else if joined.is_empty() {
        // The tree root itself. It MUST have a token — the empty string is not
        // a path, and `"anything".starts_with("")` is true in Rust, so encoding
        // the root as `""` makes a root-level glob admit `../secret`. Measured:
        // before this, `./**` and `./*` matched NOTHING (prefix `"."` folded to
        // `""`, and `path.starts_with("/")` is false for every relative path),
        // so the most natural way to write "everything under here" was a
        // silently inert grant with no diagnostic.
        ".".to_owned()
    } else {
        // UNCONDITIONALLY `./`-rooted, and this is the fix for F9.
        //
        // The marker was previously applied only when the AUTHOR wrote `./`
        // (`dot_rooted`), which made it a preserved spelling rather than an
        // invariant — so `./data/x` and `data/x`, the same file, normalized to
        // different strings and a boundary admitted one and refused the other.
        // Measured 2026-07-28, a perfect diagonal:
        //
        //     glob data/**    path ./data/x   REFUSED
        //     glob ./data/**  path ./data/x   admitted
        //     glob data/**    path data/x     admitted
        //     glob ./data/**  path data/x     REFUSED
        //
        // Nine shipped example workflows sat in that diagonal: every one writes
        // `const: "./data/x"` against `read: ["data/**"]`, which is the natural
        // pairing and was refused. The repair the message proposed was to add a
        // SECOND entry for the same directory.
        //
        // Applying the marker to every non-escaping relative path keeps the
        // escape property intact (an escaping path still lacks it, so a `./`
        // glob still cannot match one) while collapsing the two spellings of
        // one path onto one canonical form — which is what a normalizer is for.
        format!("./{joined}")
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
