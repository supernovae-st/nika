// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Set-algebra over two capability boundaries — the lattice a future
//! `nika-policy` (L2, design-locked) composes with: workflow declares X,
//! operator policy allows Y, effective = X ∩ Y (`intersect`); the dual
//! `union` combines declared boundaries across included sub-workflows.
//!
//! The two halves ship at different maturities. `intersect` is WIRED and
//! load-bearing on BOTH surfaces of the composition laws (spec 14 laws 3/4):
//! `nika-check`'s `composition.rs` meets parent ∩ child-declared to judge a
//! child's boundary statically, and `nika-service-execution`'s child runner
//! (`effective_permits`) computes the same meet, child ∩ parent, to cap a
//! child's file and fetch boundaries at run. `union` is the half still unwired
//! (tests only) — the FCI-001 "traits upfront, impls deferred" reservation
//! this codebase already carries for `InferRequest`/`CatalogEntry`.
//!
//! `None` in any category is the empty set (default-deny), so union keeps
//! the present side and intersect collapses to `None`.

use crate::subsume::{host_glob_contains, path_glob_contains, tool_glob_contains};
use crate::{ExecPermit, FsPermits, NetPermits, Permits};

/// List union preserving order, de-duplicated.
fn list_union(a: &[String], b: &[String]) -> Vec<String> {
    let mut out = a.to_vec();
    for x in b {
        if !out.contains(x) {
            out.push(x.clone());
        }
    }
    out
}

/// List intersection preserving `a`'s order.
fn list_intersect(a: &[String], b: &[String]) -> Vec<String> {
    a.iter().filter(|x| b.contains(x)).cloned().collect()
}

/// The meet of two glob lists: every pattern of either side that a pattern
/// of the other side CONTAINS (`contains(wide, narrow)` · `subsume.rs`),
/// kept at its own narrower spelling, de-duplicated. The same set whichever
/// side comes first, so parent ∩ child and child ∩ parent admit alike.
fn glob_meet(a: &[String], b: &[String], contains: fn(&str, &str) -> bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (own, other) in [(a, b), (b, a)] {
        for narrow in own {
            if !out.contains(narrow) && other.iter().any(|wide| contains(wide, narrow)) {
                out.push(narrow.clone());
            }
        }
    }
    out
}

/// `exec` join — the more-permissive tri-state wins (`No < Programs < Any`).
fn exec_union(a: Option<&ExecPermit>, b: Option<&ExecPermit>) -> Option<ExecPermit> {
    use ExecPermit::{Any, No, Programs};
    match (a, b) {
        (None, None) => None,
        (Some(Any), _) | (_, Some(Any)) => Some(Any),
        (Some(Programs(x)), Some(Programs(y))) => Some(Programs(list_union(x, y))),
        (Some(Programs(x)), _) | (_, Some(Programs(x))) => Some(Programs(x.clone())),
        _ => Some(No),
    }
}

/// `exec` meet — the less-permissive tri-state wins (`No` absorbs · `None`
/// is the empty set, so it collapses the meet to `None`).
fn exec_intersect(a: Option<&ExecPermit>, b: Option<&ExecPermit>) -> Option<ExecPermit> {
    use ExecPermit::{Any, No, Programs};
    match (a, b) {
        (None, _) | (_, None) => None,
        (Some(No), _) | (_, Some(No)) => Some(No),
        (Some(Any), Some(other)) | (Some(other), Some(Any)) => Some(other.clone()),
        (Some(Programs(x)), Some(Programs(y))) => Some(Programs(list_intersect(x, y))),
    }
}

impl Permits {
    /// The loosest boundary that admits everything EITHER operand admits (join).
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            fs: match (self.fs.as_ref(), other.fs.as_ref()) {
                (None, None) => None,
                (Some(x), None) => Some(x.clone()),
                (None, Some(y)) => Some(y.clone()),
                (Some(x), Some(y)) => Some(FsPermits {
                    read: list_union(&x.read, &y.read),
                    write: list_union(&x.write, &y.write),
                }),
            },
            net: match (self.net.as_ref(), other.net.as_ref()) {
                (None, None) => None,
                (Some(x), None) => Some(x.clone()),
                (None, Some(y)) => Some(y.clone()),
                (Some(x), Some(y)) => Some(NetPermits {
                    http: list_union(&x.http, &y.http),
                }),
            },
            exec: exec_union(self.exec.as_ref(), other.exec.as_ref()),
            tools: match (self.tools.as_deref(), other.tools.as_deref()) {
                (None, None) => None,
                (Some(x), None) => Some(x.to_vec()),
                (None, Some(y)) => Some(y.to_vec()),
                (Some(x), Some(y)) => Some(list_union(x, y)),
            },
            env: match (self.env.as_deref(), other.env.as_deref()) {
                (None, None) => None,
                (Some(x), None) => Some(x.to_vec()),
                (None, Some(y)) => Some(y.to_vec()),
                (Some(x), Some(y)) => Some(list_union(x, y)),
            },
        }
    }

    /// A SUBSUMPTION-AWARE meet. For each glob family (`fs.read` ·
    /// `fs.write` · `net.http` · `tools`) it keeps every pattern of either
    /// side that a pattern of the other side CONTAINS, at the narrower
    /// spelling: `./references/**` ∩ `./references/x.html` is
    /// `./references/x.html`, `*.example.com` ∩ `api.example.com` is
    /// `api.example.com`, `nika:*` ∩ `nika:read` is `nika:read`. Containment
    /// is decided in each family's own matcher reading (`subsume.rs`), so
    /// this stays a SOUND under-approximation of the true set-intersection:
    /// `intersect(a,b).allows(x)` ⟹ `a.allows(x) && b.allows(x)`, but NOT
    /// the converse (two incomparable globs — `*.csv` and `report*` — can both
    /// admit `report.csv` while neither contains the other, so the meet omits
    /// it). Safe for ceiling composition (nika-policy): it never grants what
    /// either side denies · at worst it denies something both would allow.
    /// A path grant that leaves the workspace (absolute · `~/` · climbing) or
    /// names `..` meets by its exact spelling only, and the exact-name planes
    /// (`exec` programs · `env`) keep the exact meet. The result admits the
    /// same values whichever operand comes first.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            fs: match (self.fs.as_ref(), other.fs.as_ref()) {
                (Some(x), Some(y)) => Some(FsPermits {
                    read: glob_meet(&x.read, &y.read, path_glob_contains),
                    write: glob_meet(&x.write, &y.write, path_glob_contains),
                }),
                _ => None,
            },
            net: match (self.net.as_ref(), other.net.as_ref()) {
                (Some(x), Some(y)) => Some(NetPermits {
                    http: glob_meet(&x.http, &y.http, host_glob_contains),
                }),
                _ => None,
            },
            exec: exec_intersect(self.exec.as_ref(), other.exec.as_ref()),
            tools: match (self.tools.as_deref(), other.tools.as_deref()) {
                (Some(x), Some(y)) => Some(glob_meet(x, y, tool_glob_contains)),
                _ => None,
            },
            // `env:` names are exact literals (NEP-0005 · no globs), so the
            // conservative string-equal meet IS the true set intersection.
            env: match (self.env.as_deref(), other.env.as_deref()) {
                (Some(x), Some(y)) => Some(list_intersect(x, y)),
                _ => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(list: &[&str]) -> Permits {
        Permits {
            tools: Some(list.iter().map(|s| (*s).to_owned()).collect()),
            ..Permits::new()
        }
    }

    #[test]
    fn union_is_the_join_for_tools() {
        let u = tools(&["nika:read"]).union(&tools(&["nika:write"]));
        assert!(u.allows_tool("nika:read") && u.allows_tool("nika:write"));
    }

    #[test]
    fn intersect_is_the_meet_for_tools() {
        let i = tools(&["nika:read", "nika:write"]).intersect(&tools(&["nika:read"]));
        assert!(i.allows_tool("nika:read") && !i.allows_tool("nika:write"));
    }

    fn env(list: &[&str]) -> Permits {
        Permits {
            env: Some(list.iter().map(|s| (*s).to_owned()).collect()),
            ..Permits::new()
        }
    }

    #[test]
    fn env_meet_is_the_exact_name_intersection() {
        // NEP-0005 law 5 · child ∩ parent on exact names — and since env
        // bounds are literals (no globs), the conservative meet IS the
        // true set intersection here.
        let i = env(&["CI_COMMIT_SHA", "CI_JOB_ID"]).intersect(&env(&["CI_COMMIT_SHA"]));
        assert!(i.allows_env_key("CI_COMMIT_SHA"));
        assert!(!i.allows_env_key("CI_JOB_ID"));
        // an absent side means zero: no child inherits a passthrough its
        // parent could not grant.
        let z = env(&["CI_COMMIT_SHA"]).intersect(&Permits::new());
        assert!(!z.allows_env_key("CI_COMMIT_SHA"));
    }

    #[test]
    fn env_union_is_the_join() {
        let u = env(&["A_ONE"]).union(&env(&["B_TWO"]));
        assert!(u.allows_env_key("A_ONE") && u.allows_env_key("B_TWO"));
    }

    #[test]
    fn union_with_empty_keeps_the_present_side() {
        let u = tools(&["nika:read"]).union(&Permits::new());
        assert!(u.allows_tool("nika:read"), "union with ⊥ is identity");
    }

    #[test]
    fn intersect_preserves_shared_fs_and_net_drops_the_rest() {
        // Kills the intersect fs/net (Some,Some) arms + the whole-fn->default
        // mutant: a deny-all intersect would fail these positive assertions.
        let a = Permits {
            fs: Some(FsPermits::new(
                vec!["./data/**".into(), "./only-a".into()],
                vec!["./out/**".into()],
            )),
            net: Some(NetPermits::new(vec!["a.com".into(), "b.com".into()])),
            ..Permits::new()
        };
        let b = Permits {
            fs: Some(FsPermits::new(
                vec!["./data/**".into()],
                vec!["./out/**".into()],
            )),
            net: Some(NetPermits::new(vec!["a.com".into()])),
            ..Permits::new()
        };
        let i = a.intersect(&b);
        assert!(
            i.allows_path("./data/y.txt", false),
            "shared read glob kept"
        );
        assert!(
            i.allows_path("./out/r.json", true),
            "shared write glob kept"
        );
        assert!(
            !i.allows_path("./only-a", false),
            "a-only read dropped by the meet"
        );
        assert!(i.allows_host("a.com"), "shared host kept");
        assert!(!i.allows_host("b.com"), "a-only host dropped by the meet");
    }

    #[test]
    fn union_none_none_is_none_not_some_no() {
        // Kills the exec_union (None,None) arm: falling through to `_ => Some(No)`
        // would still deny exec, but the FIELD must be None, not Some(No).
        let u = Permits::new().union(&Permits::new());
        assert_eq!(u.exec, None, "None union None = None (empty), not Some(No)");
        assert_eq!(u.fs, None);
        assert_eq!(u.net, None);
        assert_eq!(u.tools, None);
    }

    #[test]
    fn union_any_wins_both_orders() {
        // Kills the exec_union Some(Any) arm (both alternation branches).
        let no = Permits {
            exec: Some(ExecPermit::No),
            ..Permits::new()
        };
        let any = Permits {
            exec: Some(ExecPermit::Any),
            ..Permits::new()
        };
        assert_eq!(
            no.union(&any).exec,
            Some(ExecPermit::Any),
            "(No, Any) -> Any"
        );
        assert_eq!(
            any.union(&no).exec,
            Some(ExecPermit::Any),
            "(Any, No) -> Any"
        );
    }

    #[test]
    fn intersect_keeps_shared_tool_drops_the_rest() {
        // Kills the intersect tools (Some,Some) arm explicitly.
        let i = tools(&["nika:read", "nika:write"]).intersect(&tools(&["nika:read", "nika:log"]));
        assert!(i.allows_tool("nika:read"), "shared tool kept");
        assert!(
            !i.allows_tool("nika:write") && !i.allows_tool("nika:log"),
            "non-shared dropped"
        );
        assert!(
            i.tools.is_some(),
            "the tools field is present, not defaulted away"
        );
    }

    #[test]
    fn exec_lattice_extremes() {
        let no = Permits {
            exec: Some(ExecPermit::No),
            ..Permits::new()
        };
        let any = Permits {
            exec: Some(ExecPermit::Any),
            ..Permits::new()
        };
        assert!(no.union(&any).allows_program("rm"), "union takes the top");
        assert!(
            !no.intersect(&any).allows_exec(),
            "intersect takes the bottom"
        );
    }

    #[test]
    fn exec_union_programs_survive_and_merge() {
        // Kills exec_union Programs arm delete: (Programs, No) → Programs, not No.
        let git = Permits {
            exec: Some(ExecPermit::Programs(vec!["git".into()])),
            ..Permits::new()
        };
        let no = Permits {
            exec: Some(ExecPermit::No),
            ..Permits::new()
        };
        assert!(
            git.union(&no).allows_program("git"),
            "(Programs, No) keeps programs"
        );
        assert!(
            no.union(&git).allows_program("git"),
            "(No, Programs) keeps programs"
        );
        let cargo = Permits {
            exec: Some(ExecPermit::Programs(vec!["cargo".into()])),
            ..Permits::new()
        };
        let both = git.union(&cargo);
        assert!(
            both.allows_program("git") && both.allows_program("cargo"),
            "two program lists merge under union"
        );
    }

    fn fs_read(list: &[&str]) -> Permits {
        Permits {
            fs: Some(FsPermits::new(
                list.iter().map(|s| (*s).to_owned()).collect(),
                Vec::new(),
            )),
            ..Permits::new()
        }
    }

    /// Both call orders: the static check meets parent ∩ child, the runtime
    /// (`nika-service-execution::effective_permits`) child ∩ parent.
    fn both_orders(parent: &Permits, child: &Permits) -> [Permits; 2] {
        [parent.intersect(child), child.intersect(parent)]
    }

    /// The reported journey: the parent grants `./references/**`,
    /// the child declares exactly the one file it reads. The string-equal
    /// meet was empty; the meet now keeps the child's narrower grant.
    #[test]
    fn a_literal_inside_the_other_sides_glob_survives_the_meet() {
        let parent = fs_read(&["./references/**"]);
        let child = fs_read(&["./references/preview-shell.html"]);
        for meet in both_orders(&parent, &child) {
            assert_eq!(
                meet.fs.map(|fs| fs.read),
                Some(vec!["./references/preview-shell.html".to_owned()]),
                "the narrower spelling, never the parent's glob"
            );
        }
        let outside = fs_read(&["./other/x.html"]);
        for meet in both_orders(&parent, &outside) {
            assert!(!meet.allows_path("./other/x.html", false));
            assert!(!meet.allows_path("./references/preview-shell.html", false));
        }
    }

    #[test]
    fn a_narrower_glob_survives_and_a_wider_one_is_cut_to_the_other_side() {
        let parent = fs_read(&["./references/**"]);
        for meet in both_orders(&parent, &fs_read(&["./references/html/**"])) {
            assert!(meet.allows_path("./references/html/a.html", false));
            assert!(!meet.allows_path("./references/b.html", false));
        }
        for meet in both_orders(&parent, &fs_read(&["./**"])) {
            assert!(meet.allows_path("./references/b.html", false));
            assert!(
                !meet.allows_path("./other/x.html", false),
                "the child's ./** never widens the parent's grant"
            );
        }
    }

    #[test]
    fn the_write_plane_meets_by_containment_too() {
        let grant = |w: &str| Permits {
            fs: Some(FsPermits::new(Vec::new(), vec![w.to_owned()])),
            ..Permits::new()
        };
        for meet in both_orders(&grant("./out/**"), &grant("./out/report.json")) {
            assert!(meet.allows_path("./out/report.json", true));
            assert!(!meet.allows_path("./out/other.json", true));
            assert!(
                !meet.allows_path("./out/report.json", false),
                "a write grant never meets into a read"
            );
        }
    }

    #[test]
    fn hosts_and_tools_meet_by_containment() {
        let host = |h: &str| Permits {
            net: Some(NetPermits::new(vec![h.to_owned()])),
            ..Permits::new()
        };
        for meet in both_orders(&host("*.example.com"), &host("api.example.com")) {
            assert!(meet.allows_host("api.example.com"));
            assert!(!meet.allows_host("www.example.com"));
        }
        for meet in both_orders(&tools(&["nika:*"]), &tools(&["nika:read"])) {
            assert!(meet.allows_tool("nika:read"));
            assert!(!meet.allows_tool("nika:write"));
        }
    }

    /// The meet stays an under-approximation: two globs that overlap without
    /// either containing the other still meet to nothing.
    #[test]
    fn incomparable_globs_still_meet_to_nothing() {
        let (a, b) = (fs_read(&["./data/*.csv"]), fs_read(&["./data/report*"]));
        assert!(a.allows_path("./data/report.csv", false));
        assert!(b.allows_path("./data/report.csv", false));
        for meet in both_orders(&a, &b) {
            assert!(!meet.allows_path("./data/report.csv", false));
        }
    }

    /// A grant the run resolves against the host, the operator home or the
    /// filesystem (absolute · `~/` · any `..`) keeps the string-equal meet:
    /// `..` escapes and absolute paths keep their refusals, even where the
    /// lexical matcher alone would call one grant inside the other.
    #[test]
    fn leaving_or_climbing_grants_meet_by_exact_spelling_only() {
        for (parent, child, need) in [
            ("./**", "../secret/x", "../secret/x"),
            ("./**", "/etc/hosts", "/etc/hosts"),
            ("/srv/app/**", "/srv/app/x.txt", "/srv/app/x.txt"),
            (
                "~/.config/**",
                "~/.config/git/config",
                "~/.config/git/config",
            ),
            ("~/**", "./~/x", "./~/x"),
            (
                "./references/**",
                "./references/../references/x.html",
                "./references/x.html",
            ),
        ] {
            for meet in both_orders(&fs_read(&[parent]), &fs_read(&[child])) {
                assert!(!meet.allows_path(need, false), "{parent} ∩ {child}");
            }
            for meet in both_orders(&fs_read(&[child]), &fs_read(&[child])) {
                assert!(meet.allows_path(child, false), "the same spelling: {child}");
            }
        }
    }

    #[test]
    fn exec_intersect_meets_to_shared_programs() {
        // Kills exec_intersect→None: the meet must keep the shared program.
        let ab = Permits {
            exec: Some(ExecPermit::Programs(vec!["git".into(), "cargo".into()])),
            ..Permits::new()
        };
        let a = Permits {
            exec: Some(ExecPermit::Programs(vec!["git".into()])),
            ..Permits::new()
        };
        let i = ab.intersect(&a);
        assert!(i.allows_program("git"), "shared program survives the meet");
        assert!(
            !i.allows_program("cargo"),
            "non-shared program dropped by the meet"
        );
        // Any ∩ Programs = the narrower Programs (never None, never Any).
        let any = Permits {
            exec: Some(ExecPermit::Any),
            ..Permits::new()
        };
        let m = any.intersect(&a);
        assert!(
            m.allows_program("git") && !m.allows_program("rm"),
            "Any ∩ Programs collapses to the program list"
        );
    }
}
