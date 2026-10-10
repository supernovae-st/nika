// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pattern containment — the decision behind the subsumption-aware meet
//! ([`crate::Permits::intersect`]).
//!
//! `*_contains(wide, narrow)` answers « does every value `narrow` admits also
//! fit `wide`? » for the three glob-scoped permit families, each in the
//! reading of the ONE matcher that family is enforced with — never a
//! parallel grammar:
//!
//! - **fs paths** — [`crate::glob_admits`] over [`crate::lexically_normalize`]
//!   (`./` and `//` spelling folded, absolute vs relative tree, byte case);
//! - **net hosts** — [`nika_types::net::host_glob_matches`] (ASCII case
//!   folded, a leading `*.` subdomain wildcard, the bare `*`);
//! - **tool ids** — [`crate::glob_matches`] (a trailing `*` that stops at a
//!   `:` or `/` boundary).
//!
//! Every `true` is a proof for all values, so a meet built from it can only
//! keep what both sides admit. When the reading is uncertain the answer is
//! `false`, which leaves the meet exactly as conservative as the string-equal
//! one it extends. A path pattern that leaves the workspace
//! ([`crate::path_leaves_workspace`]: absolute · home-anchored · climbing
//! past the root) or names any `..` segment is therefore compared by its
//! exact spelling only, as before: the run resolves those against the host,
//! the operator home or the filesystem (a `..` inside a grant follows a
//! symlink), none of which a lexical comparison can see.

use nika_types::net::host_glob_matches;

use crate::fit::{lexically_normalize, segment_matches, segments};
use crate::{glob_matches, path_leaves_workspace};

/// Whether every path the `permits.fs` glob `narrow` admits is admitted by
/// `wide`, in [`crate::Permits::allows_path`]'s reading. Only two
/// workspace-relative patterns free of `..` are compared structurally.
pub(crate) fn path_glob_contains(wide: &str, narrow: &str) -> bool {
    if wide == narrow {
        return true;
    }
    let exact_only =
        |glob: &str| path_leaves_workspace(glob) || glob.split(['/', '\\']).any(|seg| seg == "..");
    if exact_only(wide) || exact_only(narrow) {
        return false;
    }
    segments_contain(
        &pattern(&lexically_normalize(wide)),
        &pattern(&lexically_normalize(narrow)),
    )
}

/// A normalized glob's segments with `**` runs collapsed — the shape
/// [`crate::glob_admits`] walks.
fn pattern(glob: &str) -> Vec<&str> {
    let mut segs = segments(glob);
    segs.dedup_by(|a, b| *a == "**" && *b == "**");
    segs
}

/// Whether every segment sequence `narrow` walks is walked by `wide`.
///
/// `cover[i][j]` decides the suffixes `wide[i..]` and `narrow[j..]`, filled
/// from the end so each pair is judged once — an authored `**/a/**/a/…`
/// cannot make this backtrack. A wide `**` takes zero segments or one more
/// of narrow's (its own `**` included); any other wide segment takes
/// exactly one, which never covers narrow's unbounded `**`.
fn segments_contain(wide: &[&str], narrow: &[&str]) -> bool {
    let (w, n) = (wide.len(), narrow.len());
    let mut cover = vec![vec![false; n + 1]; w + 1];
    cover[w][n] = true;
    for i in (0..w).rev() {
        for j in (0..=n).rev() {
            cover[i][j] = if wide[i] == "**" {
                cover[i + 1][j] || (j < n && cover[i][j + 1])
            } else {
                j < n
                    && narrow[j] != "**"
                    && segment_contains(wide[i], narrow[j])
                    && cover[i + 1][j + 1]
            };
        }
    }
    cover[0][0]
}

/// Whether every segment `narrow` matches is matched by `wide`, in
/// [`segment_matches`]'s reading (the first `*` is the wildcard, a later
/// one is literal). `pre*post` covers `pre'*post'` exactly when the narrow
/// fixed halves extend the wide ones: every value keeps them, and its length
/// already leaves room for both.
fn segment_contains(wide: &str, narrow: &str) -> bool {
    match (wide.split_once('*'), narrow.split_once('*')) {
        (_, None) => segment_matches(wide, narrow),
        (Some((w_pre, w_post)), Some((n_pre, n_post))) => {
            n_pre.starts_with(w_pre) && n_post.ends_with(w_post)
        }
        (None, Some(_)) => false,
    }
}

/// Whether every host the `permits.net.http` glob `narrow` admits is
/// admitted by `wide`, in [`host_glob_matches`]'s reading.
pub(crate) fn host_glob_contains(wide: &str, narrow: &str) -> bool {
    let (wide, narrow) = (wide.to_ascii_lowercase(), narrow.to_ascii_lowercase());
    if wide == narrow || wide == "*" {
        return true;
    }
    match narrow.strip_prefix("*.") {
        // One host (an embedded `*` is literal); the bare `*` is everything.
        None => narrow != "*" && host_glob_matches(&wide, &narrow),
        // `*.apex` admits the apex and all of its subdomains: a wide `*.`
        // suffix covers them all exactly when it admits the apex.
        Some(apex) => wide.starts_with("*.") && host_glob_matches(&wide, apex),
    }
}

/// Whether every tool id the `permits.tools` glob `narrow` admits is
/// admitted by `wide`, in [`glob_matches`]'s reading. A starless wide id
/// admits only itself. A starred one covers a narrow `prefix*` family
/// exactly when it admits `prefix` itself: the separator rule then holds
/// for every extension of that prefix as well.
pub(crate) fn tool_glob_contains(wide: &str, narrow: &str) -> bool {
    wide == narrow
        || (wide.ends_with('*') && glob_matches(wide, narrow.strip_suffix('*').unwrap_or(narrow)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn a_literal_or_a_narrower_glob_under_a_wider_path_glob_is_contained() {
        let wide = "./references/**";
        for narrow in [
            "./references/preview-shell.html",
            "references/preview-shell.html",
            "./references//preview-shell.html",
            "./references/html/**",
            "./references/*.html",
            "./references",
        ] {
            assert!(path_glob_contains(wide, narrow), "{narrow} ⊆ {wide}");
        }
        assert!(path_glob_contains("./data/*.csv", "./data/q*.csv"));
        assert!(path_glob_contains("./data/**/*.csv", "./data/a/b.csv"));
        assert!(path_glob_contains("./**", "./references/**"));
        assert!(path_glob_contains("**", "./references/x.html"));
        // a workspace directory that happens to be named `~` is a workspace path
        assert!(path_glob_contains("./~/**", "./~/x"));
    }

    #[test]
    fn a_wider_or_foreign_path_pattern_is_not_contained() {
        let wide = "./references/**";
        for narrow in [
            "./**",
            "./other/x.html",
            "./References/x.html",
            "./references-old/x.html",
            "**/x.html",
            "/references/x.html",
        ] {
            assert!(!path_glob_contains(wide, narrow), "{narrow} ⊄ {wide}");
        }
        // one-level and suffix globs stay inside their own shape
        assert!(!path_glob_contains("./data/*", "./data/a/b"));
        assert!(!path_glob_contains("./data/*.csv", "./data/*"));
        assert!(!path_glob_contains("./data/x.csv", "./data/*.csv"));
        // two globs that overlap without containment stay incomparable
        assert!(!path_glob_contains("./data/*.csv", "./data/report*"));
        assert!(!path_glob_contains("./data/report*", "./data/*.csv"));
    }

    /// A pattern that leaves the workspace or names `..` is resolved at run
    /// (host · operator home · symlinks), which lexical folding cannot see:
    /// only the identical spelling is contained — the string-equal meet.
    #[test]
    fn leaving_or_climbing_patterns_keep_their_exact_spelling() {
        for (wide, narrow) in [
            ("./**", "../secret/x"),
            ("./references/**", "./references/../../etc/x"),
            ("./references/**", "./references/../references/x.html"),
            ("../shared/**", "../shared/x.txt"),
            ("./x/../references/**", "./references/x.html"),
            ("/srv/app/**", "/srv/app/cache/x.txt"),
            ("/**", "/etc/passwd"),
            ("./**", "/etc/passwd"),
            ("/**", "./notes.md"),
            ("~/.config/**", "~/.config/git/config"),
            ("~/**", "./~/x"),
            ("./~/**", "~/x"),
            ("$HOME/**", "$HOME/x"),
            ("./**", "a\\..\\..\\x"),
        ] {
            assert!(!path_glob_contains(wide, narrow), "{narrow} ⊄ {wide}");
        }
        for same in ["../shared/**", "/srv/app/**", "~/x"] {
            assert!(path_glob_contains(same, same), "{same}");
        }
    }

    #[test]
    fn host_containment_follows_the_shared_host_matcher() {
        assert!(host_glob_contains("*.example.com", "api.example.com"));
        assert!(host_glob_contains("*.example.com", "example.com"));
        assert!(host_glob_contains("*.example.com", "*.api.example.com"));
        assert!(host_glob_contains("*.Example.COM", "API.example.com"));
        assert!(host_glob_contains("*", "*.example.com"));
        assert!(!host_glob_contains("api.example.com", "*.example.com"));
        assert!(!host_glob_contains("*.example.com", "*.com"));
        assert!(!host_glob_contains("*.example.com", "evil-example.com"));
        assert!(!host_glob_contains("*.example.com", "*"));
        assert!(!host_glob_contains("foo*.com", "foobar.com"));
    }

    #[test]
    fn tool_containment_follows_the_separator_rule() {
        assert!(tool_glob_contains("nika:*", "nika:read"));
        assert!(tool_glob_contains("nika:*", "nika:connectome/*"));
        assert!(tool_glob_contains("mcp:browser*", "mcp:browser/*"));
        assert!(tool_glob_contains("*", "mcp:anything/*"));
        assert!(!tool_glob_contains("nika:read", "nika:*"));
        assert!(!tool_glob_contains("mcp:brow*", "mcp:browser*"));
        assert!(!tool_glob_contains("mcp:browser*", "mcp:browser-evil"));
        assert!(!tool_glob_contains("nika:*", "*"));
    }

    // ── soundness: a `true` must hold for every value narrow admits ────

    fn path_seg() -> impl Strategy<Value = String> {
        prop::sample::select(vec![
            "a", "b", "ab", "x.csv", "*", "a*", "*.csv", "**", ".", "..",
        ])
        .prop_map(String::from)
    }

    fn path_glob() -> impl Strategy<Value = String> {
        (
            prop::sample::select(vec!["", "./", "/", "~/"]),
            prop::collection::vec(path_seg(), 0..5),
        )
            .prop_map(|(root, segs)| format!("{root}{}", segs.join("/")))
    }

    /// A concrete path drawn FROM `glob`'s own shape: each `**` becomes zero
    /// to two segments and each starred segment a filled value, so the
    /// probe lands in the narrow language the containment speaks about.
    fn instance(glob: &str, fill: &[u8]) -> String {
        let mut out: Vec<String> = Vec::new();
        for (k, seg) in glob.split('/').enumerate() {
            let pick = fill.get(k % fill.len().max(1)).copied().unwrap_or(0);
            if seg == "**" {
                out.extend((0..pick % 3).map(|d| format!("d{d}")));
            } else if let Some((pre, post)) = seg.split_once('*') {
                let mid = ["", "z", "a", "q.csv"][usize::from(pick % 4)];
                out.push(format!("{pre}{mid}{post}"));
            } else {
                out.push(seg.to_owned());
            }
        }
        out.join("/")
    }

    proptest! {
        #[test]
        fn path_containment_is_sound(
            wide in path_glob(),
            narrow in path_glob(),
            fill in prop::collection::vec(any::<u8>(), 1..6),
            home in prop::sample::select(vec!["/home/op", "/tmp/nika-op"]),
        ) {
            if path_glob_contains(&wide, &narrow) {
                let admits = |glob: &str, probe: &str, h: Option<&str>| {
                    let mut p = crate::Permits::new();
                    p.fs = Some(crate::FsPermits::new(vec![glob.to_owned()], Vec::new()));
                    p.allows_path_in(probe, false, h)
                };
                let raw = instance(&narrow, &fill);
                // the run-time spelling of a home-anchored probe as well
                for probe in [crate::expand_home_grant(&raw, home), raw] {
                    for h in [None, Some(home)] {
                        if admits(&narrow, &probe, h) {
                            prop_assert!(
                                admits(&wide, &probe, h),
                                "{narrow:?} ⊆ {wide:?} but {probe:?} escapes (home {h:?})"
                            );
                        }
                    }
                }
            }
        }

        #[test]
        fn host_containment_is_sound(
            wide in prop::sample::select(vec!["*", "*.example.com", "*.api.example.com", "example.com", "API.example.com", "*.com"]),
            narrow in prop::sample::select(vec!["*", "*.example.com", "*.api.example.com", "example.com", "api.example.com", "*.com", "x.api.example.com"]),
            probe in prop::sample::select(vec!["example.com", "api.example.com", "x.api.example.com", "evil.com", "com", "api.example.com.evil.com"]),
        ) {
            if host_glob_contains(wide, narrow) && host_glob_matches(narrow, probe) {
                prop_assert!(host_glob_matches(wide, probe), "{narrow} ⊆ {wide} but {probe} escapes");
            }
        }

        #[test]
        fn tool_containment_is_sound(
            wide in prop::sample::select(vec!["*", "nika:*", "nika:read", "mcp:browser*", "mcp:browser/*", "mcp:brow*", "nika:connectome/*"]),
            narrow in prop::sample::select(vec!["*", "nika:*", "nika:read", "mcp:browser*", "mcp:browser/*", "mcp:browser/navigate", "mcp:browser-evil", "nika:connectome/*"]),
            probe in prop::sample::select(vec!["nika:read", "nika:", "mcp:browser", "mcp:browser/navigate", "mcp:browser-evil", "mcp:browser-evil/x", "nika:connectome/recall", ""]),
        ) {
            if tool_glob_contains(wide, narrow) && glob_matches(narrow, probe) {
                prop_assert!(glob_matches(wide, probe), "{narrow} ⊆ {wide} but {probe} escapes");
            }
        }
    }
}
