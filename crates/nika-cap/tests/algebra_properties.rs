// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Property tests for the capability set-algebra (Gate 6).
//!
//! The lattice laws, at the strength that actually holds:
//! * **union is EXACT** — `union(a,b).allows(x) == a.allows(x) || b.allows(x)`.
//! * **intersect is a SOUND under-approximation** — `intersect(a,b).allows(x)`
//!   implies `a.allows(x) && b.allows(x)` (one direction: two different globs
//!   can both admit an atom yet share no pattern, so the string-intersection
//!   meet omits it — safe for a ceiling, never grants what either denies).
//! * **bottom** — the empty boundary denies everything.

use nika_cap::{ExecPermit, FsPermits, NetPermits, Permits};
use proptest::prelude::*;

fn tok() -> impl Strategy<Value = String> {
    prop_oneof!["a", "b", "ab", "abc", "x", "nika:read", "nika:write"].prop_map(String::from)
}

fn glob() -> impl Strategy<Value = String> {
    (tok(), any::<bool>()).prop_map(|(t, star)| if star { format!("{t}*") } else { t })
}

fn glob_list() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(glob(), 0..4)
}

/// Path segments that include the traversal tokens the algebra alphabet omits
/// (`.` and `..`) — the exact structure that hid M1 until this generator existed.
fn rel_path() -> impl Strategy<Value = String> {
    prop::collection::vec(prop_oneof!["a", "b", "..", "."], 1..6)
        .prop_map(|segs| format!("./{}", segs.join("/")))
}

fn arb_exec() -> impl Strategy<Value = Option<ExecPermit>> {
    prop_oneof![
        Just(None),
        Just(Some(ExecPermit::No)),
        Just(Some(ExecPermit::Any)),
        glob_list().prop_map(|l| Some(ExecPermit::Programs(l))),
    ]
}

prop_compose! {
    fn arb_permits()(
        fs in prop::option::of((glob_list(), glob_list())),
        net in prop::option::of(glob_list()),
        exec in arb_exec(),
        tools in prop::option::of(glob_list()),
    ) -> Permits {
        let mut p = Permits::new();
        p.fs = fs.map(|(read, write)| FsPermits::new(read, write));
        p.net = net.map(NetPermits::new);
        p.exec = exec;
        p.tools = tools;
        p
    }
}

proptest! {
    #[test]
    fn union_tool_is_or(a in arb_permits(), b in arb_permits(), t in tok()) {
        prop_assert_eq!(a.union(&b).allows_tool(&t), a.allows_tool(&t) || b.allows_tool(&t));
    }
    #[test]
    fn union_host_is_or(a in arb_permits(), b in arb_permits(), h in tok()) {
        prop_assert_eq!(a.union(&b).allows_host(&h), a.allows_host(&h) || b.allows_host(&h));
    }
    #[test]
    fn union_path_read_is_or(a in arb_permits(), b in arb_permits(), p in tok()) {
        prop_assert_eq!(a.union(&b).allows_path(&p, false), a.allows_path(&p, false) || b.allows_path(&p, false));
    }
    #[test]
    fn union_path_write_is_or(a in arb_permits(), b in arb_permits(), p in tok()) {
        prop_assert_eq!(a.union(&b).allows_path(&p, true), a.allows_path(&p, true) || b.allows_path(&p, true));
    }
    #[test]
    fn union_program_is_or(a in arb_permits(), b in arb_permits(), pr in tok()) {
        prop_assert_eq!(a.union(&b).allows_program(&pr), a.allows_program(&pr) || b.allows_program(&pr));
    }
    #[test]
    fn intersect_is_sound(a in arb_permits(), b in arb_permits(), x in tok()) {
        let i = a.intersect(&b);
        if i.allows_tool(&x)       { prop_assert!(a.allows_tool(&x) && b.allows_tool(&x)); }
        if i.allows_host(&x)       { prop_assert!(a.allows_host(&x) && b.allows_host(&x)); }
        if i.allows_path(&x, false){ prop_assert!(a.allows_path(&x, false) && b.allows_path(&x, false)); }
        if i.allows_path(&x, true) { prop_assert!(a.allows_path(&x, true) && b.allows_path(&x, true)); }
        if i.allows_program(&x)    { prop_assert!(a.allows_program(&x) && b.allows_program(&x)); }
    }
    #[test]
    fn empty_denies_everything(x in tok()) {
        let e = Permits::new();
        prop_assert!(!e.allows_tool(&x) && !e.allows_host(&x));
        prop_assert!(!e.allows_path(&x, false) && !e.allows_path(&x, true));
        prop_assert!(!e.allows_program(&x) && !e.allows_exec());
    }
    #[test]
    fn union_with_empty_is_identity(a in arb_permits(), x in tok()) {
        let u = a.union(&Permits::new());
        prop_assert_eq!(u.allows_tool(&x), a.allows_tool(&x));
        prop_assert_eq!(u.allows_host(&x), a.allows_host(&x));
        prop_assert_eq!(u.allows_program(&x), a.allows_program(&x));
    }
    #[test]
    fn union_commutes(a in arb_permits(), b in arb_permits(), x in tok()) {
        prop_assert_eq!(a.union(&b).allows_tool(&x), b.union(&a).allows_tool(&x));
        prop_assert_eq!(a.union(&b).allows_host(&x), b.union(&a).allows_host(&x));
    }
    #[test]
    fn any_exec_allows_all_programs(pr in tok()) {
        let mut p = Permits::new();
        p.exec = Some(ExecPermit::Any);
        prop_assert!(p.allows_program(&pr) && p.allows_exec());
    }
    #[test]
    fn union_path_is_or_over_real_paths(a in arb_permits(), b in arb_permits(), p in rel_path()) {
        // exercises union + normalize over genuinely path-shaped inputs
        prop_assert_eq!(
            a.union(&b).allows_path(&p, false),
            a.allows_path(&p, false) || b.allows_path(&p, false)
        );
    }
}
