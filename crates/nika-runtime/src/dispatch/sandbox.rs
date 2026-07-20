// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `permits:` → [`SandboxSpec`] derivation (spec 01 §permits · ADR-095
//! Layer 6): once a workflow declares a capability boundary, every `exec`
//! child is jailed to it — the OS sandbox enforces at spawn what the
//! builtin/fs gates enforce at their own seams, so an exec that the
//! blocklist passes can still not TOUCH what the boundary denies.
//!
//! Two doctrine rules, kept in lockstep with the builtin `FsBoundary` so
//! check≡run ≡jail cannot drift:
//!
//! - **Relative globs anchor at the run's launch cwd** (the same root the
//!   fs boundary canonicalizes against). The sandbox launchers speak
//!   absolute subpaths only (`grant_subpath` fail-closes on a relative
//!   grant), so the derivation absolutizes HERE — a task-level `cwd:`
//!   does not re-anchor the boundary, exactly like the builtin gate.
//! - **Network is a tri-state** ([`NetPolicy`] · the Anthropic
//!   sandbox-runtime model): `net.http` non-empty maps to `Allowlist`
//!   (exactly the declared set, via the loopback egress proxy); the bare
//!   `*` entry maps to `Allow` (the explicit escape hatch, in-file only);
//!   an absent `net:` block or empty `net.http` maps to `Deny` (the
//!   default, fail-closed). Until the proxy lands the allowlist arm
//!   confines exactly as the pre-tri-state grant did — zero regression.

use std::path::{Component, Path};

use nika_kernel::process::{EgressAllowlist, NetPolicy, SandboxSpec};
use nika_schema::types::Permits;

/// Derive the OS-confinement spec from the declared boundary, anchored at
/// `root` (the run's launch cwd — see the module doc).
pub(super) fn spec_of(permits: &Permits, root: &Path) -> SandboxSpec {
    let (read, write) = permits
        .fs
        .as_ref()
        .map(|fs| (fs.read.as_slice(), fs.write.as_slice()))
        .unwrap_or_default();
    let mut spec = SandboxSpec::new();
    spec.fs_read = read.iter().map(|g| absolutize(root, g)).collect();
    spec.fs_write = write.iter().map(|g| absolutize(root, g)).collect();
    spec.net = net_policy_of(permits);
    spec
}

/// The network arm (see the module doc): declared host list → `Allowlist`
/// (that exact set) · a bare `*` entry anywhere in the list → `Allow` (the
/// explicit escape hatch — it subsumes every other entry) · absent/empty →
/// `Deny` (fail-closed).
fn net_policy_of(permits: &Permits) -> NetPolicy {
    let Some(net) = permits.net.as_ref() else {
        return NetPolicy::Deny;
    };
    if net.http.iter().any(|g| g == "*") {
        return NetPolicy::Allow;
    }
    if net.http.is_empty() {
        return NetPolicy::Deny;
    }
    NetPolicy::Allowlist(EgressAllowlist::new(net.http.clone()))
}

/// Absolutize one declared glob against the run root: a relative glob
/// joins the root and folds lexically (`.`/`..` resolved textually — the
/// `lexically_normalize` semantics the fit checker already pins, so a
/// `data/../out/**` grant reads identically on both seams). An absolute
/// glob passes through unchanged — including the shapes the launchers
/// refuse (`~` · a preserved leading `..` · a bare system root): their
/// fail-closed `Profile` refusal is the honest verdict, named to the
/// operator, never a silent widening.
fn absolutize(root: &Path, glob: &str) -> String {
    if glob.starts_with('/') || glob.starts_with('~') {
        return glob.to_owned();
    }
    lexically_normalize(&root.join(glob))
}

/// Textual `.`/`..` fold (the `nika-cap` `fit::lexically_normalize`
/// semantics, restated here so the runtime stays a leaf consumer — no
/// symlink walk: a not-yet-existing write tree has no symlinks to
/// resolve, and the jail's own canonicalization owns the existing part).
fn lexically_normalize(path: &Path) -> String {
    let mut out = std::path::PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use nika_schema::types::{FsPermits, NetPermits, Permits};

    use super::*;

    fn permits(read: &[&str], write: &[&str], http: &[&str]) -> Permits {
        let mut p = Permits::new();
        p.fs = Some(FsPermits::new(
            read.iter().map(ToString::to_string).collect(),
            write.iter().map(ToString::to_string).collect(),
        ));
        p.net = Some(NetPermits::new(
            http.iter().map(ToString::to_string).collect(),
        ));
        p
    }

    #[test]
    fn relative_globs_anchor_at_the_run_root() {
        let spec = spec_of(
            &permits(&["./data/**"], &["./out/**"], &[]),
            Path::new("/repo"),
        );
        assert_eq!(spec.fs_read, vec!["/repo/data/**".to_owned()]);
        assert_eq!(spec.fs_write, vec!["/repo/out/**".to_owned()]);
        assert_eq!(
            spec.net,
            NetPolicy::Deny,
            "an empty net.http keeps the network deny"
        );
    }

    #[test]
    fn absolute_globs_pass_through_and_a_host_list_maps_to_allowlist() {
        let spec = spec_of(
            &permits(&["/data/in/**"], &["/data/out/**"], &["api.example.com"]),
            Path::new("/repo"),
        );
        assert_eq!(spec.fs_read, vec!["/data/in/**".to_owned()]);
        assert_eq!(spec.fs_write, vec!["/data/out/**".to_owned()]);
        assert_eq!(
            spec.net,
            NetPolicy::Allowlist(EgressAllowlist::new(vec!["api.example.com".to_owned()])),
            "a declared net.http set maps to the allowlist arm, verbatim"
        );
    }

    /// The tri-state derivation matrix: absent `net:` → Deny · empty
    /// `net.http` → Deny · declared hosts → Allowlist(exactly that set) ·
    /// the bare `*` entry → Allow (the explicit escape hatch — alone or
    /// mixed into a list, where it subsumes the rest).
    #[test]
    fn the_tri_state_derivation_matrix() {
        // absent net: → Deny (the default, fail-closed)
        let mut p = Permits::new();
        assert_eq!(net_policy_of(&p), NetPolicy::Deny);
        // declared-but-empty net.http → Deny
        p.net = Some(NetPermits::new(vec![]));
        assert_eq!(net_policy_of(&p), NetPolicy::Deny);
        // declared hosts → Allowlist, the set verbatim
        p.net = Some(NetPermits::new(vec![
            "api.example.com".to_owned(),
            "*.github.com".to_owned(),
        ]));
        assert_eq!(
            net_policy_of(&p),
            NetPolicy::Allowlist(EgressAllowlist::new(vec![
                "api.example.com".to_owned(),
                "*.github.com".to_owned(),
            ]))
        );
        // the bare `*` escape hatch → Allow (never by default — the author
        // must type it), alone or subsuming a mixed list
        p.net = Some(NetPermits::new(vec!["*".to_owned()]));
        assert_eq!(net_policy_of(&p), NetPolicy::Allow);
        p.net = Some(NetPermits::new(vec![
            "api.example.com".to_owned(),
            "*".to_owned(),
        ]));
        assert_eq!(
            net_policy_of(&p),
            NetPolicy::Allow,
            "a bare `*` anywhere in the list subsumes the rest"
        );
        // a leading-`*.` glob is NOT the hatch — it stays an allowlist entry
        p.net = Some(NetPermits::new(vec!["*.example.com".to_owned()]));
        assert!(matches!(net_policy_of(&p), NetPolicy::Allowlist(_)));
    }

    #[test]
    fn dot_dot_folds_the_same_way_as_the_fit_checker() {
        let spec = spec_of(&permits(&["data/../out/**"], &[], &[]), Path::new("/repo"));
        assert_eq!(spec.fs_read, vec!["/repo/out/**".to_owned()]);
    }

    #[test]
    fn no_fs_block_means_no_extra_grants() {
        let spec = spec_of(&Permits::new(), Path::new("/repo"));
        assert!(spec.fs_read.is_empty() && spec.fs_write.is_empty());
        assert_eq!(spec.net, NetPolicy::Deny, "no net block = the deny holds");
    }
}
