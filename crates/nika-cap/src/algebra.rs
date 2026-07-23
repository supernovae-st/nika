// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Set-algebra over two capability boundaries — the lattice a future
//! `nika-policy` (L2, design-locked) composes with: workflow declares X,
//! operator policy allows Y, effective = X ∩ Y (`intersect`); the dual
//! `union` combines declared boundaries across included sub-workflows.
//!
//! Shipped now, unwired (FCI-001 "traits upfront, impls deferred" spirit):
//! pure value algebra, zero callers today, exactly the reservation this
//! codebase already carries for `InferRequest`/`CatalogEntry`. `None` in any
//! category is the empty set (default-deny), so union keeps the present side
//! and intersect collapses to `None`.

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

    /// A CONSERVATIVE meet — admits only the glob patterns present in BOTH
    /// boundaries (string-equal). This is a SOUND under-approximation of the
    /// true set-intersection: `intersect(a,b).allows(x)` ⟹ `a.allows(x) &&
    /// b.allows(x)`, but NOT the converse (two DIFFERENT globs — `a*` and
    /// `ab*` — can both admit `abc` yet share no pattern, so the meet omits
    /// it). Safe for ceiling composition (nika-policy): it never grants what
    /// either side denies · at worst it denies something both would allow.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            fs: match (self.fs.as_ref(), other.fs.as_ref()) {
                (Some(x), Some(y)) => Some(FsPermits {
                    read: list_intersect(&x.read, &y.read),
                    write: list_intersect(&x.write, &y.write),
                }),
                _ => None,
            },
            net: match (self.net.as_ref(), other.net.as_ref()) {
                (Some(x), Some(y)) => Some(NetPermits {
                    http: list_intersect(&x.http, &y.http),
                }),
                _ => None,
            },
            exec: exec_intersect(self.exec.as_ref(), other.exec.as_ref()),
            tools: match (self.tools.as_deref(), other.tools.as_deref()) {
                (Some(x), Some(y)) => Some(list_intersect(x, y)),
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
