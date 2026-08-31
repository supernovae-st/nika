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
//!
//! Home-anchored grants (`~/` · `$HOME/` · `${HOME}/`) stay authored as-is
//! so a tracked workflow is portable. [`expand_home_grant`] rewrites them
//! against the operator home at match time; `~user` is left intact.

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
    ///
    /// Home-anchored grants stay literal here — the check has no operator
    /// `HOME`. The run-time twin is [`Self::allows_path_in`].
    #[must_use]
    pub fn allows_path(&self, path: &str, write: bool) -> bool {
        self.allows_path_in(path, write, None)
    }

    /// [`Self::allows_path`], expanding `~/` and `$HOME/` grants against
    /// `home` first (the operator home at run). A missing or non-absolute
    /// home leaves those grants inert — they do not become `/**`.
    #[must_use]
    pub fn allows_path_in(&self, path: &str, write: bool, home: Option<&str>) -> bool {
        self.fs.as_ref().is_some_and(|fs| {
            let globs = if write { &fs.write } else { &fs.read };
            globs.iter().any(|g| match home {
                Some(h) => path_glob_matches(&expand_home_grant(g, h), path),
                None => path_glob_matches(g, path),
            })
        })
    }

    /// Whether the OS jail derived from this boundary would let a
    /// SUBPROCESS open `path` for reading.
    ///
    /// This is NOT [`Self::allows_path`]. The jail does not glob: the
    /// launchers take each grant's LITERAL PREFIX — everything before the
    /// first `*`/`?`/`[`, trimmed back to a directory boundary — and bind
    /// that subtree (`nika-sandbox-*::grant_subpath`). So the jail admits
    /// strictly MORE than the lexical walk, and the difference is
    /// measured, not assumed (2026-08-20, seatbelt, `nika 0.111.0`):
    ///
    /// ```text
    /// permits.fs.read      script            run    allows_path
    /// ───────────────      ──────            ───    ───────────
    /// []                   leg.sh            126    no
    /// ["leg.sh"]           leg.sh              0    yes
    /// ["data/**"]          leg.sh            126    no
    /// ["sub"]              sub/leg.sh          0    NO   ← binds the subtree
    /// ["sub/inn*"]         sub/leg.sh          0    NO   ← prefix is `sub`
    /// ```
    ///
    /// The looser reading is the one an exec claim must be judged against:
    /// a checker using the stricter one would redden files the run
    /// executes happily. The builtin fs gate keeps `allows_path` — its own
    /// runtime seam enforces that stricter reading, and check refuses
    /// before a run exists — so the two readings of one grant are a live
    /// divergence in the authored vocabulary. Naming both here is what
    /// keeps it from being rediscovered a fourth time.
    ///
    /// A grant with no literal prefix (`*` · `**/x`) grants nothing, the
    /// same `Ok(None)` skip the launchers make. A prefix the launcher
    /// REFUSES outright (a bare system root) is admitted here: that spawn
    /// fails loudly with a profile error, which is not the silent class
    /// this predicate exists to catch.
    #[must_use]
    pub fn jail_admits_read(&self, path: &str) -> bool {
        self.jail_admits_read_in(path, None)
    }

    /// [`Self::jail_admits_read`], expanding `~/` and `$HOME/` grants
    /// against `home` first — the same rewrite the exec jail applies
    /// before it binds a subpath.
    #[must_use]
    pub fn jail_admits_read_in(&self, path: &str, home: Option<&str>) -> bool {
        self.fs.as_ref().is_some_and(|fs| {
            fs.read.iter().any(|g| match home {
                Some(h) => bound_subtree_admits(&expand_home_grant(g, h), path),
                None => bound_subtree_admits(g, path),
            })
        })
    }
}

/// Expand a `~/` or `$HOME/` fs grant against the operator `home`.
///
/// The authored spelling stays portable (`~/.gitconfig`); this rewrite
/// is what makes it match the live absolute path. `~user` and `$HOMEDIR`
/// are left intact — they are not this operator's home. An empty or
/// non-absolute `home` is a no-op (the grant stays inert rather than
/// becoming `/…` or `/**`).
#[must_use]
pub fn expand_home_grant(glob: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    if home.is_empty() || !home.starts_with('/') {
        return glob.to_owned();
    }
    let rest = if let Some(rest) = glob.strip_prefix("~/") {
        Some(rest)
    } else if glob == "~" {
        Some("")
    } else if let Some(rest) = glob.strip_prefix("$HOME/") {
        Some(rest)
    } else if glob == "$HOME" {
        Some("")
    } else if let Some(rest) = glob.strip_prefix("${HOME}/") {
        Some(rest)
    } else if glob == "${HOME}" {
        Some("")
    } else {
        None
    };
    match rest {
        None => glob.to_owned(),
        Some("") => home.to_owned(),
        Some(rest) => format!("{home}/{rest}"),
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

/// Does the subtree the JAIL binds for `glob` contain `path`?
///
/// The launchers do not glob — they bind `glob`'s literal prefix and let
/// the kernel admit everything under it (`grant_subpath` · `literal_prefix`
/// in `nika-sandbox-landlock` and its seatbelt sibling). This models that,
/// lexically, both sides folded. An empty prefix grants nothing, matching
/// the launchers' `Ok(None)` skip.
///
/// NOTE · this is a THIRD copy of the prefix rule (the two launchers carry
/// byte-identical siblings, so the unification is a named follow-on, not a
/// regression introduced here). The rule is four lines and pinned by this
/// module's tests against the launchers' own examples.
fn bound_subtree_admits(glob: &str, path: &str) -> bool {
    let cut = glob.find(['*', '?', '[']).unwrap_or(glob.len());
    let head = &glob[..cut];
    let prefix = match head.rfind('/') {
        Some(slash) if cut < glob.len() => &head[..slash],
        _ => head,
    };
    if prefix.is_empty() {
        return false; // no literal prefix — the launchers skip the grant
    }
    let path = lexically_normalize(path);
    let prefix = lexically_normalize(prefix);
    if prefix.is_empty() {
        return false;
    }
    // An escaping path must never be admitted by an in-tree grant — the
    // same guard `path_glob_matches` states, for the same reason.
    if path.starts_with("..") && !prefix.starts_with("..") {
        return false;
    }
    // Same tree-identity as `glob_admits`: a relative jail prefix is the
    // workspace, an absolute path is the host (B04 / issue 1294).
    if path.starts_with('/') != prefix.starts_with('/') {
        return false;
    }
    path == prefix
        || path
            .strip_prefix(&prefix)
            .is_some_and(|tail| tail.starts_with('/'))
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
    // Absolute vs relative is a different tree. Segment walking drops the
    // leading `/` and `.`, so without this guard a workspace glob (`./**`)
    // admits every host path (`/etc/passwd`) — B04 / B28, issue 1294:
    // check printed PERMITS-clean while run still refused NIKA-SEC-004.
    if glob.starts_with('/') != path.starts_with('/') {
        return false;
    }
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

    /// The jail binds a grant's LITERAL PREFIX and lets the kernel admit
    /// the subtree — it never globs. Pinned against the launchers' own
    /// documented examples (`nika-sandbox-landlock::literal_prefix`:
    /// `./output/**` → `./output` · `/data/lo*` → `/data` · `/data/x.txt`
    /// → itself · `**/y` → empty), so a drift on either side shows here.
    ///
    /// MEASURED 2026-08-20 on seatbelt · `permits.fs.read: ["sub"]` with
    /// `["bash","sub/leg.sh"]` exits 0 and prints the script's output.
    #[test]
    fn the_jail_binds_a_literal_prefix_where_the_lexical_walk_globs() {
        let grant = |g: &str| {
            let mut p = Permits::new();
            p.fs = Some(FsPermits {
                read: vec![g.into()],
                write: vec![],
            });
            p
        };

        // a bare directory · the row that reddened three studio workflows
        let p = grant("sub");
        assert!(
            p.jail_admits_read("sub/leg.sh"),
            "the jail binds the subtree"
        );
        assert!(
            !p.allows_path("sub/leg.sh", false),
            "the lexical walk still refuses — the divergence is the point"
        );
        assert!(p.jail_admits_read("sub"));
        assert!(!p.jail_admits_read("other/leg.sh"));
        assert!(
            !p.jail_admits_read("subterfuge/leg.sh"),
            "prefix, not segment"
        );

        // a MID-PATH glob · the prefix is `data`, so the whole tree is bound
        let p = grant("data/lo*");
        assert!(
            p.jail_admits_read("data/other.txt"),
            "the launcher binds `data`, not the pattern"
        );
        assert!(!p.jail_admits_read("elsewhere/lo.txt"));

        // `**` and an exact file behave as the launcher documents
        assert!(grant("./output/**").jail_admits_read("output/a/b.txt"));
        assert!(grant("data/x.txt").jail_admits_read("data/x.txt"));
        assert!(!grant("data/x.txt").jail_admits_read("data/y.txt"));

        // no literal prefix · the launchers skip the grant, so it grants
        // nothing here either
        assert!(!grant("*").jail_admits_read("anything"));
        assert!(!grant("**/y").jail_admits_read("a/y"));
    }

    /// A grant is not a licence to climb out of it, on either reading.
    #[test]
    fn the_jail_reading_still_refuses_an_escape() {
        let mut p = Permits::new();
        p.fs = Some(FsPermits {
            read: vec!["sub".into(), "data/**".into()],
            write: vec![],
        });
        assert!(!p.jail_admits_read("sub/../secret"));
        assert!(!p.jail_admits_read("../secret"));
        assert!(!p.jail_admits_read("leg.sh"), "granted something, not this");
        assert!(p.jail_admits_read("data/a/b.txt"), "the subtree is bound");
    }

    /// An omitted `fs:` block is zero authority on this reading too — the
    /// jail with no grants opens nothing.
    #[test]
    fn no_fs_block_admits_no_read_in_the_jail() {
        assert!(!Permits::new().jail_admits_read("anything"));
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

    fn home_grant(read: &str) -> Permits {
        let mut p = Permits::new();
        p.fs = Some(FsPermits {
            read: vec![read.into()],
            write: vec![],
        });
        p
    }

    /// #1025 — a portable `~/` grant matches this operator's home after
    /// expansion, not another tree, and never becomes `/**`.
    #[test]
    fn a_tilde_grant_matches_the_operator_home_after_expansion() {
        let home = "/tmp/nika-op-home";
        let p = home_grant("~/.gitconfig");
        assert!(
            !p.allows_path(&format!("{home}/.gitconfig"), false),
            "without a home, ~ is a literal spelling — the bug this closes"
        );
        assert!(
            p.allows_path_in(&format!("{home}/.gitconfig"), false, Some(home)),
            "the operator home after expansion"
        );
        assert!(
            p.jail_admits_read_in(&format!("{home}/.gitconfig"), Some(home)),
            "the jail binds the expanded exact file"
        );
        assert!(
            !p.allows_path_in("/tmp/evil/.gitconfig", false, Some(home)),
            "another tree"
        );
        assert!(!p.allows_path_in("/Users/other/.gitconfig", false, Some(home)));
        assert!(
            !p.allows_path_in("/etc/passwd", false, Some(home)),
            "~ is not /**"
        );
        assert!(!p.jail_admits_read_in("/etc/passwd", Some(home)));
        assert!(
            !p.allows_path_in(&format!("{home}/.ssh/id_rsa"), false, Some(home)),
            "a sibling under home is not granted"
        );
    }

    #[test]
    fn a_dollar_home_grant_expands_the_same_way() {
        let home = "/tmp/nika-op-home";
        for grant in ["$HOME/.gitconfig", "${HOME}/.gitconfig"] {
            let p = home_grant(grant);
            assert!(p.allows_path_in(&format!("{home}/.gitconfig"), false, Some(home)));
            assert!(!p.allows_path_in("/tmp/evil/.gitconfig", false, Some(home)));
        }
    }

    #[test]
    fn literal_etc_passwd_still_requires_an_absolute_grant() {
        let home = "/tmp/nika-op-home";
        assert!(!home_grant("~/.gitconfig").allows_path_in("/etc/passwd", false, Some(home)));
        assert!(home_grant("/etc/passwd").allows_path("/etc/passwd", false));
        assert!(
            !home_grant("/etc/passwd").allows_path_in(
                &format!("{home}/.gitconfig"),
                false,
                Some(home)
            ),
            "an absolute grant is not a home grant"
        );
    }

    /// B04 / B28 · issue 1294 — a workspace glob is the TREE, not the
    /// host. Segment walking used to drop the leading `/` and `.`, so
    /// `./**` (and a bare `*` / `**`) admitted `/etc/passwd` at check
    /// while run still refused `NIKA-SEC-004`. Relative vs absolute is
    /// a different tree; an in-tree sibling stays granted.
    #[test]
    fn a_workspace_glob_never_admits_a_host_passwd_path() {
        let mut p = Permits::new();
        p.fs = Some(FsPermits {
            read: vec!["./**".into()],
            write: vec![],
        });
        assert!(
            !p.allows_path("/etc/passwd", false),
            "./** is the workspace, not the host"
        );
        assert!(
            !p.allows_path("../etc/passwd", false),
            "a relative climb is the same class"
        );
        assert!(
            p.allows_path("./notes.md", false),
            "an in-tree sibling stays granted"
        );
        assert!(
            !p.jail_admits_read("/etc/passwd"),
            "the jail reading must not widen the hole"
        );
        // The matcher itself, not just the Permits wrapper — `./**` and
        // a bare `**` both folded to the same walk that admitted the host.
        assert!(!glob_admits("./**", "/etc/passwd"));
        assert!(!glob_admits("**", "/etc/passwd"));
        assert!(!glob_admits("*", "/etc"));
        assert!(glob_admits("./**", "./notes.md"));
        // An explicit absolute grant is still the operator's named intent.
        assert!(home_grant("/etc/passwd").allows_path("/etc/passwd", false));
        assert!(!home_grant("/etc/passwd").allows_path("./notes.md", false));
    }

    #[test]
    fn exec_git_does_not_auto_grant_the_homedir() {
        let mut p = Permits::new();
        p.exec = Some(crate::ExecPermit::Programs(vec!["git".into()]));
        assert!(p.allows_program("git"));
        assert!(
            !p.allows_path_in(
                "/tmp/nika-op-home/.gitconfig",
                false,
                Some("/tmp/nika-op-home")
            ),
            "permits.exec: [git] is not a home grant — the author still names ~/.gitconfig"
        );
    }

    #[test]
    fn expand_home_grant_is_prefix_only() {
        let home = "/tmp/nika-op-home";
        assert_eq!(
            expand_home_grant("~/.config/git/**", home),
            "/tmp/nika-op-home/.config/git/**"
        );
        assert_eq!(expand_home_grant("~", home), home);
        assert_eq!(
            expand_home_grant("~other/.gitconfig", home),
            "~other/.gitconfig",
            "~user is not this operator"
        );
        // A longer variable name that begins with the same four letters
        // is not the operator home variable and must stay literal.
        assert_eq!(
            expand_home_grant("$HOMEDIR/.gitconfig", home),
            "$HOMEDIR/.gitconfig"
        );
        assert_eq!(
            expand_home_grant("~/.gitconfig", "relative"),
            "~/.gitconfig",
            "a non-absolute home is a no-op"
        );
        assert_eq!(expand_home_grant("~/.gitconfig", ""), "~/.gitconfig");
        assert_eq!(
            expand_home_grant("./data/**", home),
            "./data/**",
            "in-tree grants are untouched"
        );
    }
}
