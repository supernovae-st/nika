// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `permits:` → [`SandboxSpec`](nika_kernel::process::SandboxSpec)
//! derivation (spec 01 §permits · ADR-095
//! Layer 6 · descended from `nika-runtime::dispatch` at the 15k wall,
//! ADR-110 · #889 — the spawn-side vocabulary rides its exec-shell home):
//! once a workflow declares a capability boundary, every `exec`
//! child is jailed to it — the OS sandbox enforces at spawn what the
//! builtin/fs gates enforce at their own seams, so an exec that the
//! blocklist passes can still not TOUCH what the boundary denies.
//!
//! Three doctrine rules, kept in lockstep with the builtin `FsBoundary` so
//! check≡run ≡jail cannot drift:
//!
//! - **Relative globs anchor at the run's launch cwd** (the same root the
//!   fs boundary canonicalizes against). The sandbox launchers speak
//!   absolute subpaths only (`grant_subpath` fail-closes on a relative
//!   grant), so the derivation absolutizes HERE — a task-level `cwd:`
//!   does not re-anchor the boundary, exactly like the builtin gate.
//!   Home-anchored grants (`~/` · `$HOME/`) expand against the operator
//!   `HOME` at the same moment, so a tracked workflow can name
//!   `~/.gitconfig` without baking one machine's `/Users/…` path. `~user`
//!   is left intact and stays the launchers' fail-closed refusal.
//! - **Network is a tri-state** ([`NetPolicy`](nika_kernel::process::NetPolicy)
//!   · the Anthropic
//!   sandbox-runtime model): `net.http` non-empty maps to `Allowlist`
//!   (exactly the declared set, via the loopback egress proxy); the bare
//!   `*` entry maps to `Allow` (the explicit escape hatch, in-file only);
//!   an absent `net:` block or empty `net.http` maps to `Deny` (the
//!   default, fail-closed). Until the proxy lands the allowlist arm
//!   confines exactly as the pre-tri-state grant did — zero regression.
//! - **A path grant names an effective path identity** (NEP-0009 ·
//!   LAW-AUTH-0330): the mount-projection arm follows a symlinked bind
//!   SOURCE at mount(2) time (the CVE-2024-42472 class) where the kernel
//!   path-walk arm refuses at open — so before any projection the grant's
//!   literal prefix is re-judged as its EFFECTIVE identity (longest
//!   existing ancestor canonicalized, the not-yet-existing tail carried
//!   lexically — the `FsBoundary::resolve_effective` algorithm, sync). An
//!   identity that escapes the declared set is REFUSED before spawn and
//!   attested (`fs.path_mismatch`), never silently rewritten or mounted:
//!   the receipt does not lie — judged = mounted. A parallel task swapping
//!   the link between this re-gate and the mount is the DECLARED residual
//!   window (NEP-0009 law 6); the fd-pin projection (`--bind-fd` · bwrap ≥
//!   0.10.0) closes it as a named follow-on.

use std::path::{Component, Path, PathBuf};

use nika_cap::expand_home_grant;
use nika_kernel::process::{EgressAllowlist, NetPolicy, SandboxSpec};
use nika_schema::types::Permits;

/// One grant whose effective identity escaped the declared set (NEP-0009
/// law 2): the judged prefix, the resolved target, and the access class —
/// the caller refuses the dispatch and attests both paths.
#[derive(Debug)]
pub struct PathMismatch {
    /// The judged (declared · absolutized) literal prefix.
    pub grant: String,
    /// The effective identity the prefix resolves to on the live fs.
    pub resolved: String,
    /// `read` | `write` — which declared set was escaped.
    pub access: &'static str,
}

/// Derive the OS-confinement spec from the declared boundary, anchored at
/// `root` (the run's launch cwd — see the module doc). Fallible since
/// NEP-0009: every fs grant's literal prefix must keep its judged identity
/// on the live filesystem — an escape refuses the whole derivation (the
/// task never spawns) and is never rewritten to the resolved form.
/// Home-anchored grants need [`spec_of_with_home`]: this entry does not
/// read `HOME`.
///
/// # Errors
///
/// `Err(Box<PathMismatch>)` when a grant's EFFECTIVE identity escapes the
/// declared set (NEP-0009 law 1+2) — the caller refuses the dispatch and
/// attests the judged prefix + the resolved target.
pub fn spec_of(permits: &Permits, root: &Path) -> Result<SandboxSpec, Box<PathMismatch>> {
    spec_of_with_home(permits, root, None)
}

/// [`spec_of`] with an injected operator home so `~/` and `$HOME/` fs
/// grants become live absolute paths on the jail. Production dispatch
/// reads `HOME` at the runtime seam and passes it here — this crate does
/// not touch the environment. `None` leaves those grants unexpanded, and
/// the launchers then fail-close on a non-absolute leftover.
///
/// # Errors
///
/// Same as [`spec_of`].
pub fn spec_of_with_home(
    permits: &Permits,
    root: &Path,
    home: Option<&str>,
) -> Result<SandboxSpec, Box<PathMismatch>> {
    let (read, write) = permits
        .fs
        .as_ref()
        .map(|fs| (fs.read.as_slice(), fs.write.as_slice()))
        .unwrap_or_default();
    let mut spec = SandboxSpec::new();
    spec.fs_read = read.iter().map(|g| absolutize(root, g, home)).collect();
    spec.fs_write = write.iter().map(|g| absolutize(root, g, home)).collect();
    for (grant, access) in spec
        .fs_read
        .iter()
        .map(|g| (g, "read"))
        .chain(spec.fs_write.iter().map(|g| (g, "write")))
    {
        judge_identity(grant, access)?;
    }
    spec.net = net_policy_of(permits);
    Ok(spec)
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

/// NEP-0009 law 1+2 — re-judge ONE grant's literal prefix: the mount point
/// bwrap binds must be the identity the author lexically named. The test
/// compares the FULLY resolved prefix against its parent-resolved form —
/// the parent (existing ancestors · the run root · a `/tmp`→`/private/tmp`
/// system link) canonicalized so a legitimately-symlinked ANCESTOR is
/// tolerated, and the FINAL component held lexical so a planted symlink AT
/// the mount point (the CVE-2024-42472 pivot) cannot masquerade. Equal →
/// the identity holds; different → the final component redirects outside
/// the judged path · REFUSE, never rewrite (NEP-0009 backwards-compat: the
/// author declares the effective path). A not-yet-existing tail (the legal
/// new-write · law 5) folds identically on both sides → admitted. Non-
/// absolute leftover (`~user` · a preserved escape) pass through untouched —
/// the launchers' `grant_subpath` owns their fail-closed refusal, as before.
/// `~/` and `$HOME/` are expanded against the operator home before this
/// gate sees them.
fn judge_identity(grant: &str, access: &'static str) -> Result<(), Box<PathMismatch>> {
    if !grant.starts_with('/') {
        return Ok(());
    }
    let lit = literal_root(grant);
    let lit_path = Path::new(&lit);
    let effective = resolve_effective(lit_path);
    let expected = match (lit_path.parent(), lit_path.file_name()) {
        // Parent resolved (ancestor symlinks absorbed), final name lexical.
        (Some(parent), Some(name)) if parent != lit_path => resolve_effective(parent).join(name),
        // A bare root (`/` · `/**`) has no final component to redirect.
        _ => effective.clone(),
    };
    if effective == expected {
        return Ok(());
    }
    Err(Box::new(PathMismatch {
        grant: lit,
        resolved: effective.display().to_string(),
        access,
    }))
}

/// The glob's longest literal directory prefix — every component up to
/// (not including) the first one containing `*` (the builtin
/// `FsBoundary`'s exact split, restated for the sync arm).
fn literal_root(glob: &str) -> String {
    if !glob.contains('*') {
        return glob.to_owned();
    }
    let mut root = PathBuf::new();
    for comp in Path::new(glob).components() {
        if comp.as_os_str().to_string_lossy().contains('*') {
            break;
        }
        root.push(comp.as_os_str());
    }
    root.to_string_lossy().into_owned()
}

/// Resolve a literal prefix to its EFFECTIVE absolute identity (symlinks +
/// `..` against the live fs) — the sync twin of the builtin
/// `FsBoundary::resolve_effective`: an existing path canonicalizes
/// directly; a not-yet-existing tail (the legal new-write case · NEP-0009
/// law 5) canonicalizes its longest existing ancestor — resolving every
/// symlink up to that point — then folds the trailing components lexically
/// (they don't exist, so none can be a symlink, making the fold sound).
fn resolve_effective(path: &Path) -> PathBuf {
    // The security gate must see the REAL filesystem the jail will mount —
    // a mocked seam here would blind the re-gate, so the bypass is the
    // deliberate, reviewable exception (the nika:uuid entropy precedent).
    let whole = std::fs::canonicalize(path); // seam-bypass-ok: judge the LIVE fs
    if let Ok(canon) = whole {
        return canon;
    }
    let mut trailing: Vec<Component<'_>> = Vec::new();
    let mut cur = path;
    loop {
        let ancestor = std::fs::canonicalize(cur); // seam-bypass-ok: judge the LIVE fs
        if let Ok(base) = ancestor {
            return fold_trailing(base, trailing.iter().rev());
        }
        match cur.parent() {
            Some(parent) if parent != cur => {
                // Keep the LAST component (file_name OR a `..`/`.`) to fold.
                if let Some(last) = cur.components().next_back() {
                    trailing.push(last);
                }
                cur = parent;
            }
            // No existing ancestor: fold lexically — nothing on this path
            // could be canonicalized anyway, so the lexical form IS the
            // judged identity.
            _ => return PathBuf::from(lexically_normalize(path)),
        }
    }
}

/// Fold trailing path components onto an already-canonical base.
fn fold_trailing<'a>(base: PathBuf, comps: impl Iterator<Item = &'a Component<'a>>) -> PathBuf {
    let mut out = base;
    for comp in comps {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Absolutize one declared glob against the run root: a relative glob
/// joins the root and folds lexically (`.`/`..` resolved textually — the
/// `lexically_normalize` semantics the fit checker already pins, so a
/// `data/../out/**` grant reads identically on both seams). An absolute
/// glob passes through unchanged — including the shapes the launchers
/// refuse (`~user` · a preserved leading `..` · a bare system root): their
/// fail-closed `Profile` refusal is the honest verdict, named to the
/// operator, never a silent widening. `~/` and `$HOME/` expand first so
/// the jail lists the live absolute path.
fn absolutize(root: &Path, glob: &str, home: Option<&str>) -> String {
    let glob = match home {
        Some(h) => expand_home_grant(glob, h),
        None => glob.to_owned(),
    };
    if glob.starts_with('/') {
        return glob;
    }
    if glob.starts_with('~') {
        return glob;
    }
    lexically_normalize(&root.join(glob))
}

/// Textual `.`/`..` fold (the `nika-cap` `fit::lexically_normalize`
/// semantics, restated here so the runtime stays a leaf consumer — no
/// symlink walk: the EFFECTIVE-identity walk lives in
/// [`resolve_effective`], which owns the existing part).
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
#[allow(clippy::expect_used, clippy::panic)]
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

    /// A per-test scratch tree, CANONICALIZED so the declared grants name
    /// their effective identity (NEP-0009's own law — `temp_dir` on macOS
    /// lives under the `/var` → `/private/var` symlink).
    fn scratch(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("nika-h2-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("scratch dir");
        std::fs::canonicalize(&base).expect("canonical scratch root")
    }

    // ── check ≡ run on the exec fs bound (the 2026-08-20 D2 harvest) ──

    /// The fs twin of `blocklist::tests::check_and_run_agree_on_the_argv_floor`.
    ///
    /// MEASURED on 0.111.0 before this arm existed · `["bash","leg.sh"]`
    /// under `exec: [bash]` and no `fs.read` audited ✔ on all fourteen
    /// lanes, then ran as `✔ leg` with rc 0 — while the leg had exited
    /// **126**, empty stdout, because `spec_of` below copies `fs.read`
    /// into the jail and bash could never open its own script. Granting
    /// `fs.read: ["leg.sh"]` and changing nothing else returned exit 0.
    ///
    /// Both big pipelines are pinned to ONE reference semantics —
    /// `Permits::jail_admits_read(script)` — exactly as
    /// `nika-runtime`'s `boundary_differential` pins the exec CATEGORY to
    /// `allows_program`:
    ///
    /// * **static leg** · `nika_check::check` raises an `fs` escape on the
    ///   task ⟺ the reference REFUSES the script.
    /// * **derivation leg** · the jail `spec_of` hands the OS admits the
    ///   script ⟺ the reference ADMITS it. This is the leg that earns its
    ///   keep: `spec_of` re-anchors every grant against the run root, so a
    ///   fold that mangled a glob would open a hole no static lane can see.
    ///
    /// Composed · check-flags ⟺ reference-refuses ⟺ jail-refuses.
    #[test]
    fn check_and_run_agree_on_the_exec_script_read_bound() {
        // (grants, script, the reference verdict — admitted?)
        let cases: &[(&[&str], &str, bool)] = &[
            (&[], "leg.sh", false),                         // the measured repro
            (&["leg.sh"], "leg.sh", true),                  // the one grant that fixes it
            (&["scripts/**"], "scripts/deploy.sh", true),   // a globbed subtree
            (&["scripts/**"], "other/deploy.sh", false),    // a NEIGHBOURING tree
            (&["sub"], "sub/leg.sh", true),                 // a BARE dir binds its subtree
            (&["sub"], "other/leg.sh", false),              // and only its own
            (&["./scripts/**"], "scripts/deploy.sh", true), // `./` folds on both sides
            (&["data/**"], "leg.sh", false),                // granted, but not this
        ];
        let root = Path::new("/repo");
        for (grants, script, admitted) in cases {
            let p = permits(grants, &[], &[]);

            // the reference
            assert_eq!(
                p.jail_admits_read(script),
                *admitted,
                "reference verdict for {script:?} under {grants:?}"
            );

            // static leg · the `nika check` finding
            let list = grants
                .iter()
                .map(|g| format!("\"{g}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let yaml = format!(
                "nika: w\npermits:\n  exec: [\"bash\"]\n  fs: {{ read: [{list}] }}\n\
                 tasks:\n  t:\n    exec: {{ command: [\"bash\", \"{script}\"] }}\n"
            );
            let wf = nika_schema::parser::parse(
                &yaml,
                nika_schema::source::FileId::new(0),
                nika_schema::parser::ParseMode::Strict,
            )
            .expect("fixture parses");
            let report = nika_check::check(&wf);
            let static_flags = report
                .capability_escapes
                .iter()
                .any(|e| e.category == "fs" && e.task == "t");
            assert_eq!(
                static_flags, !*admitted,
                "static check verdict for {script:?} under {grants:?}"
            );

            // derivation leg · what the jail would admit
            let spec = spec_of(&p, root).expect("no symlink on the judged prefixes");
            let mut jail = Permits::new();
            jail.fs = Some(nika_schema::types::FsPermits::new(spec.fs_read, vec![]));
            let jail_admits = jail.jail_admits_read(&absolutize(root, script, None));
            assert_eq!(
                jail_admits, *admitted,
                "the jail must admit exactly what the reference admits — \
                 {script:?} under {grants:?}"
            );

            // the law
            assert_eq!(
                static_flags, !jail_admits,
                "check ≡ run on the exec fs bound — {script:?} under {grants:?}"
            );
        }
    }

    #[test]
    fn relative_globs_anchor_at_the_run_root() {
        let spec = spec_of(
            &permits(&["./data/**"], &["./out/**"], &[]),
            Path::new("/repo"),
        )
        .expect("no symlink on the judged prefixes");
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
        )
        .expect("no symlink on the judged prefixes");
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
        let spec = spec_of(&permits(&["data/../out/**"], &[], &[]), Path::new("/repo"))
            .expect("no symlink on the judged prefix");
        assert_eq!(spec.fs_read, vec!["/repo/out/**".to_owned()]);
    }

    #[test]
    fn no_fs_block_means_no_extra_grants() {
        let spec =
            spec_of(&Permits::new(), Path::new("/repo")).expect("an empty boundary judges nothing");
        assert!(spec.fs_read.is_empty() && spec.fs_write.is_empty());
        assert_eq!(spec.net, NetPolicy::Deny, "no net block = the deny holds");
    }

    /// NEP-0009 law 1+2 — the planted-pivot shape: a grant prefix that IS
    /// a symlink pointing outside the declared set is refused, carrying
    /// the judged prefix and the resolved target (never rewritten).
    #[cfg(unix)]
    #[test]
    fn a_symlinked_prefix_escaping_the_declared_set_is_refused() {
        let base = scratch("escape");
        let outside = base.join("outside");
        let ws = base.join("ws");
        std::fs::create_dir_all(&outside).expect("outside tree");
        std::fs::create_dir_all(&ws).expect("judged tree");
        let data = ws.join("data");
        std::os::unix::fs::symlink(&outside, &data).expect("the plant");

        let grant = format!("{}/**", data.display());
        let err = spec_of(&permits(&[&grant], &[], &[]), &base)
            .expect_err("the escaped identity refuses the derivation");
        assert_eq!(err.grant, data.display().to_string(), "the judged prefix");
        assert_eq!(
            err.resolved,
            outside.display().to_string(),
            "the resolved target is named, never mounted"
        );
        assert_eq!(err.access, "read");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// NEP-0009 law 5 — a write grant whose target does not exist yet is
    /// judged on its longest existing ancestor: creation under a healthy
    /// ancestor is admitted.
    #[test]
    fn a_not_yet_existing_write_tree_under_a_healthy_ancestor_stays_legal() {
        let base = scratch("newwrite");
        let grant = format!("{}/fresh/out/**", base.display());
        let spec = spec_of(&permits(&[], &[&grant], &[]), &base)
            .expect("the not-yet-existing tail folds onto the healthy ancestor");
        assert_eq!(spec.fs_write, vec![grant]);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The run-root tolerance (the correctness core): a grant sitting under
    /// a legitimately-symlinked ANCESTOR (the `/tmp`→`/private/tmp` class,
    /// or any symlinked checkout root) is ADMITTED — the parent is resolved
    /// on both sides, so the ancestor link is absorbed, not mistaken for the
    /// planted-pivot. Without this the effective (canonical) form would
    /// never match a lexical declared root and every macOS temp run would
    /// false-refuse.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_ancestor_is_tolerated_not_mistaken_for_the_pivot() {
        let base = scratch("ancestor");
        let real_root = base.join("real-root");
        let data = real_root.join("data");
        std::fs::create_dir_all(&data).expect("the real tree");
        // The declared grant is reached THROUGH a symlinked ancestor.
        let link_root = base.join("link-root");
        std::os::unix::fs::symlink(&real_root, &link_root).expect("ancestor link");

        let grant = format!("{}/data/**", link_root.display());
        let spec = spec_of(&permits(&[&grant], &[], &[]), &base)
            .expect("a symlinked ANCESTOR is absorbed on both sides · admitted");
        assert_eq!(spec.fs_read, vec![grant]);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The identity is what the author NAMED: a final-component symlink is
    /// refused even when its target is a sibling under the SAME base — the
    /// receipt would otherwise say `link` while binding `real` (NEP-0009
    /// backwards-compat · declare the effective path).
    #[cfg(unix)]
    #[test]
    fn a_final_component_symlink_is_refused_even_to_a_sibling() {
        let base = scratch("sibling");
        let real = base.join("real");
        std::fs::create_dir_all(&real).expect("sibling tree");
        let link = base.join("link");
        std::os::unix::fs::symlink(&real, &link).expect("redirecting link");

        let grant = format!("{}/**", link.display());
        let err = spec_of(&permits(&[&grant], &[], &[]), &base)
            .expect_err("the final component redirects · refused");
        assert_eq!(err.resolved, real.display().to_string());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Leftover `~user` (not this operator) and a missing home stay the
    /// launchers' fail-closed refusal — `~/` with an injected home is
    /// expanded, not refused.
    #[test]
    fn leftover_tilde_user_stays_the_launchers_refusal() {
        let spec = spec_of_with_home(
            &permits(&["~other/.gitconfig"], &[], &[]),
            Path::new("/repo"),
            Some("/tmp/nika-op-home"),
        )
        .expect("`~user` is the launcher's refusal, not this gate's");
        assert_eq!(spec.fs_read, vec!["~other/.gitconfig".to_owned()]);
        let spec = spec_of_with_home(&permits(&["~/x/**"], &[], &[]), Path::new("/repo"), None)
            .expect("no home · the `~/` leftover is the launcher's");
        assert_eq!(spec.fs_read, vec!["~/x/**".to_owned()]);
    }

    /// #1025 — a portable `~/` grant becomes this operator's absolute
    /// path on the jail spec, not another tree, and never `/**`.
    #[test]
    fn a_tilde_grant_expands_to_the_operator_home_on_the_jail() {
        let home = "/tmp/nika-op-home";
        let spec = spec_of_with_home(
            &permits(&["~/.gitconfig", "$HOME/.config/git/**"], &[], &[]),
            Path::new("/repo"),
            Some(home),
        )
        .expect("home grants expand to absolute paths, then judge");
        assert_eq!(
            spec.fs_read,
            vec![
                format!("{home}/.gitconfig"),
                format!("{home}/.config/git/**"),
            ]
        );
        let mut jail = Permits::new();
        jail.fs = Some(nika_schema::types::FsPermits::new(
            spec.fs_read.clone(),
            vec![],
        ));
        assert!(jail.jail_admits_read(&format!("{home}/.gitconfig")));
        assert!(jail.jail_admits_read(&format!("{home}/.config/git/config")));
        assert!(
            !jail.jail_admits_read("/tmp/evil/.gitconfig"),
            "another tree"
        );
        assert!(!jail.jail_admits_read("/Users/other/.gitconfig"));
        assert!(
            !jail.jail_admits_read("/etc/passwd"),
            "~ is not /** · /etc/passwd still needs an absolute grant"
        );
    }
}
