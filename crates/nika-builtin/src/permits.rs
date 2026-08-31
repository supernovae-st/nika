// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Runtime `permits.fs` enforcement — the filesystem capability boundary
//! (spec `01-envelope.md` §permits · « enforced statically + at runtime ·
//! `NIKA-SEC-004` »).
//!
//! The static `nika check` surface (`nika-schema` · `permits_fit`) flags
//! the *literal* escapes; this is the runtime half the spec mandates — and
//! the ONLY surface that can confine a `${{ }}`-built or `..`/symlink path,
//! because those resolve to their real target only at run time.
//!
//! # Why canonicalization is load-bearing
//!
//! A boundary glob like `/data/in/**` matched by *literal* prefix accepts
//! `/data/in/../../etc/passwd` (the string starts with `/data/in/`) and a
//! `symlink → /` inside the boundary. Both ESCAPE. The fix: resolve the
//! EFFECTIVE path (`..` + symlinks) and the boundary root to their real
//! locations, then test descendancy — `/data/in/../../etc/passwd` resolves
//! to `/etc/passwd`, which is not under the resolved `/data/in`, so it is
//! refused with the boundary's [`BuiltinFailure`] before any I/O.
//!
//! An UNSET boundary ([`FsBoundary::unbounded`]) enforces nothing — the
//! pre-F-O8 behaviour. Post-F-O8 « absent = zero authority » the compose
//! layer never DERIVES it from a workflow (an absent `permits:` block
//! maps to the empty declared boundary); it remains only as an explicit
//! embedder construction.

use std::path::{Component, Path, PathBuf};

use nika_kernel::io::fs::FsReadDyn;

use crate::BuiltinFailure;

/// The spec-canonical code for an effect outside the declared `permits:`
/// boundary (`05-errors.md` · `security_error` · never retryable, never
/// fed back to an `agent:` model).
pub(crate) const SEC_DENIED: &str = "NIKA-SEC-004";

/// The spec-canonical code for an SSRF block — a `nika:fetch`/`nika:notify`
/// URL resolving to a loopback/private/link-local/metadata target (the
/// always-on engine floor · `05-errors.md` · `security_error` · never
/// retryable, never fed back to an `agent:` model · independent of
/// `permits:` with ONE carve-out: an exact loopback literal in
/// `permits.net.http` declassifies the floor for that host only · #395).
pub(crate) const SEC_SSRF: &str = "NIKA-SEC-005";

/// Which direction of `permits.fs` an op needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FsAccess {
    /// A read (or the read phase of `edit`/`grep`).
    Read,
    /// A write (or the write phase of `edit`).
    Write,
}

impl FsAccess {
    const fn category(self) -> &'static str {
        match self {
            Self::Read => "permits.fs.read",
            Self::Write => "permits.fs.write",
        }
    }
}

/// The declared filesystem boundary the fs builtins enforce at run time.
///
/// Holds the `permits.fs` glob lists verbatim (gitignore-style · the spec
/// surface). `None` lists = a category that grants nothing once a boundary
/// is present; [`FsBoundary::unbounded`] = no boundary declared at all
/// (enforce nothing · the pre-permits floor).
#[derive(Debug, Clone, Default)]
pub struct FsBoundary {
    /// `Some` once a `permits:` block is declared (then every category is
    /// default-deny unless listed) · `None` = no boundary (enforce nothing).
    declared: bool,
    read: Vec<String>,
    write: Vec<String>,
}

impl FsBoundary {
    /// No boundary declared — the fs builtins enforce nothing (today's
    /// floor). The dispatcher's default.
    #[must_use]
    pub fn unbounded() -> Self {
        Self::default()
    }

    /// A declared boundary from the workflow's `permits.fs` glob lists.
    /// Once declared, every direction is default-deny unless its glob list
    /// admits the (canonicalized) path.
    #[must_use]
    pub fn declared(read: Vec<String>, write: Vec<String>) -> Self {
        Self {
            declared: true,
            read,
            write,
        }
    }

    /// Whether a boundary is in force (a `permits:` block was declared).
    #[must_use]
    pub fn is_declared(&self) -> bool {
        self.declared
    }

    fn globs(&self, access: FsAccess) -> &[String] {
        match access {
            FsAccess::Read => &self.read,
            FsAccess::Write => &self.write,
        }
    }

    /// Enforce the boundary for one fs op on `path`.
    ///
    /// `Ok(())` when no boundary is declared OR the EFFECTIVE (canonical)
    /// path is confined to a declared glob root for `access`. `Err` (the
    /// `NIKA-SEC-004` [`BuiltinFailure`]) when the resolved path escapes —
    /// the caller returns it INSTEAD of touching the filesystem.
    ///
    /// Resolution (the security core): symlinks + `..` are resolved against
    /// the real filesystem via the kernel `canonicalize` seam — for a not-
    /// yet-existing write target, the longest existing ancestor is
    /// canonicalized and the trailing (non-existent · so symlink-free)
    /// components are folded lexically, so a legitimate NEW file inside the
    /// boundary is admitted while a `..` that climbs out is refused. The
    /// GRANT is judged by its effective path identity (NEP-0009 law 1 —
    /// [`grant_identity`]): the ancestor canonicalized, the FINAL component
    /// held lexical, so a symlink planted AT the grant literal between
    /// check and run cannot redirect the judgment (resolved-vs-resolved
    /// followed it — the measured 2026-08-08 hole, refused as
    /// `fs.path_mismatch`, never rewritten).
    pub(crate) async fn enforce<F: FsReadDyn>(
        &self,
        fs: &F,
        path: &str,
        access: FsAccess,
    ) -> Result<(), BuiltinFailure> {
        self.enforce_judged(fs, path, access, true).await
    }

    /// The at-open RE-judgment ([`JudgedFs`](crate::judged_fs::JudgedFs))
    /// — the same op's second enforcement, SILENT: the dispatch guard
    /// already witnessed the op's decision (one fs frame per op · a
    /// confirming allow recorded twice would charge one decision twice),
    /// and a refusal here stays the coded `NIKA-SEC-004` it always was.
    pub(crate) async fn enforce_unwitnessed<F: FsReadDyn>(
        &self,
        fs: &F,
        path: &str,
        access: FsAccess,
    ) -> Result<(), BuiltinFailure> {
        self.enforce_judged(fs, path, access, false).await
    }

    async fn enforce_judged<F: FsReadDyn>(
        &self,
        fs: &F,
        path: &str,
        access: FsAccess,
        witness: bool,
    ) -> Result<(), BuiltinFailure> {
        if !self.declared {
            return Ok(()); // no boundary in force · no decision to witness
        }
        let effective = resolve_effective(fs, Path::new(path)).await;
        let verdict = self.judge(fs, path, access, &effective).await;
        if witness {
            crate::witness::record_decision(
                "fs",
                format!("{} {path}", access.category()),
                verdict.decision(),
                verdict.why(&effective, access),
            );
        }
        verdict.into_result(path, &effective, access)
    }

    /// The glob walk, pure of side effect: the first glob that admits
    /// wins, else the first exact grant whose identity diverged (NEP-0009
    /// law 2), else outside.
    async fn judge<F: FsReadDyn>(
        &self,
        fs: &F,
        path: &str,
        access: FsAccess,
        effective: &Path,
    ) -> FsVerdict {
        let mut mismatch = None; // the first exact grant whose identity diverged
        for glob in self.globs(access) {
            match confines(fs, glob, path, effective).await {
                ConfineVerdict::Admits => return FsVerdict::Admitted,
                ConfineVerdict::Outside => {}
                ConfineVerdict::IdentityMismatch(judged) => {
                    mismatch = mismatch.or(Some(judged));
                }
            }
        }
        mismatch.map_or(FsVerdict::Outside, |judged| FsVerdict::Mismatch { judged })
    }

    /// #433 option C · create `path`'s parent directory WHEN the boundary
    /// declares this write tree — a `permits.fs.write` glob is the author's
    /// standing declaration that the tree is theirs to make, so the two
    /// artifact writers (`nika:write` · `nika:chart`) agree in the case that
    /// matters: declared-intent auto-creates, un-declared keeps each
    /// writer's own safety default. MUST be called only AFTER a passing
    /// [`enforce`](Self::enforce) for [`FsAccess::Write`] on this path — the
    /// confinement is proven there, so a declared boundary reaching here
    /// means the target is inside a write glob. No-op when no boundary is in
    /// force (best-effort · a real fs error surfaces on the write op itself).
    pub(crate) async fn ensure_write_parent<F: nika_kernel::io::fs::FsWriteDyn>(
        &self,
        fs: &F,
        path: &str,
    ) {
        if !self.declared {
            return;
        }
        if let Some(parent) = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            let _ = fs.create_dir_all(parent).await;
        }
    }
}

/// Resolve a path to its EFFECTIVE absolute location (symlinks + `..`).
///
/// Existing paths canonicalize directly. A path whose tail does not exist
/// yet (the common `nika:write` of a new file) canonicalizes its longest
/// existing ancestor — which resolves every symlink up to that point — then
/// folds the trailing components lexically (they don't exist, so none can
/// be a symlink, making lexical `..`/`.` resolution sound). A `..` in the
/// trailing part pops the resolved base (it can climb OUT of the boundary —
/// exactly what the descendancy test below must catch).
async fn resolve_effective<F: FsReadDyn>(fs: &F, path: &Path) -> PathBuf {
    if let Ok(canon) = fs.canonicalize(path).await {
        return canon;
    }
    // Walk up to the longest existing ancestor, collecting the trailing
    // (non-existent) components to fold back lexically.
    let mut trailing: Vec<Component<'_>> = Vec::new();
    let mut cur = path;
    loop {
        if let Ok(base) = fs.canonicalize(cur).await {
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
            // A relative path whose first component does not exist has an
            // implicit existing ancestor: the current directory. Anchor it
            // there before folding. Returning the relative spelling here
            // made identity depend on scheduling: a concurrent writer could
            // create the first component between target and grant resolution,
            // turning only the latter absolute and causing a false denial.
            // If the fs seam cannot resolve `.` (e.g. a deliberately minimal
            // mock), keep the old lexical fail-closed fallback.
            _ if path.is_relative() => {
                if let Ok(cwd) = fs.canonicalize(Path::new(".")).await {
                    let components: Vec<_> = path.components().collect();
                    return fold_trailing(cwd, components.iter());
                }
                return lexical_only(path);
            }
            // No existing ancestor on an absolute path (or an fs seam that
            // does not model its root): preserve the lexical identity.
            _ => return lexical_only(path),
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

/// Last-resort lexical normalization when NO ancestor exists on disk
/// (e.g. a relative path under a cwd the mock fs doesn't model). Resolves
/// `.`/`..` purely textually — sound here only because nothing on this path
/// could be canonicalized anyway.
fn lexical_only(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
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

/// The confinement verdict for one grant against an effective path — the
/// bool it replaces could not say WHY a refusal happened, and the
/// attestation needs the class (NEP-0009 law 3).
enum ConfineVerdict {
    /// The effective path IS the granted identity or descends from it.
    Admits,
    /// No part of the grant covers the effective path.
    Outside,
    /// The task named an EXACT grant verbatim yet the resolve landed
    /// elsewhere — the grant's final component is a planted symlink
    /// (NEP-0009 law 2: refuse, never rewrite). Carries the judged
    /// identity the attestation names.
    IdentityMismatch(PathBuf),
}

/// The boundary's judgment on one op, computed ONCE from the glob set —
/// the record AND the coded refusal derive from the same value, so the
/// attested `why` and the `NIKA-SEC-004` message can never drift apart
/// (NEP-0009 law 3 · one verdict on every enforcement arm, law 4).
enum FsVerdict {
    /// The effective identity stays inside a declared glob.
    Admitted,
    /// The resolved target is outside every declared glob for the access.
    Outside,
    /// An exact grant matched the path yet the resolve landed elsewhere
    /// (NEP-0009 law 2 · carries the judged identity the attestation names).
    Mismatch { judged: PathBuf },
}

impl FsVerdict {
    /// The witness record's decision word.
    fn decision(&self) -> &'static str {
        match self {
            Self::Admitted => "allow",
            Self::Outside | Self::Mismatch { .. } => "deny",
        }
    }

    /// The witness record's one-line attestation.
    fn why(&self, effective: &Path, access: FsAccess) -> String {
        match self {
            Self::Admitted => "the effective identity stays inside the declared set".to_owned(),
            Self::Outside => format!(
                "the resolved target `{}` is outside the declared {} set",
                effective.display(),
                access.category()
            ),
            Self::Mismatch { judged } => format!(
                "the resolved target `{}` diverges from the judged grant `{judged}`",
                effective.display(),
                judged = judged.display(),
            ),
        }
    }

    /// The enforcement result: `Ok(())` on an admission, the coded
    /// `NIKA-SEC-004` refusal otherwise (the exact voice the guard has
    /// always spoken).
    fn into_result(
        self,
        path: &str,
        effective: &Path,
        access: FsAccess,
    ) -> Result<(), BuiltinFailure> {
        match self {
            Self::Admitted => Ok(()),
            Self::Outside => Err(BuiltinFailure::new(
                SEC_DENIED,
                format!(
                    "`{path}` resolves to `{}` — outside the declared {} boundary",
                    effective.display(),
                    access.category()
                ),
            )),
            Self::Mismatch { judged } => Err(BuiltinFailure::new(
                SEC_DENIED,
                format!(
                    "fs.path_mismatch · `{path}` resolves to `{}` · the judged grant is \
                     `{judged}` · outside the declared {} set (NEP-0009)",
                    effective.display(),
                    access.category(),
                    judged = judged.display(),
                ),
            )),
        }
    }
}

/// A grant's EFFECTIVE PATH IDENTITY (NEP-0009 law 1 — the exec arm's
/// `judge_identity` comparison, restated on this seam): when the literal
/// EXISTS, its identity is the parent canonicalized (a legitimately-
/// symlinked ANCESTOR is absorbed — tolerated: `/tmp`→`/private/tmp`, a
/// nix-store link) with the FINAL component carried LEXICALLY, so a
/// symlink planted AT the grant literal (the measured 2026-08-08 pivot)
/// makes the identity diverge from the fully-resolved form instead of
/// redirecting it — resolved-vs-resolved followed the plant on both
/// sides and compared equal. A not-yet-existing literal (law 5 · the
/// legal new write) cannot contain a redirecting symlink, so the plain
/// `resolve_effective` fold IS the identity. The result rides the
/// lexical fold so a no-op `canonicalize` (the mock seam) reduces this
/// to the literal itself — the identity judgment is a no-op exactly
/// where nothing can be a symlink.
async fn grant_identity<F: FsReadDyn>(fs: &F, root_lit: &str) -> PathBuf {
    let lit = Path::new(root_lit);
    let resolved = resolve_effective(fs, lit).await;
    let Some(name) = lit.file_name() else {
        return resolved; // a bare root · a `..` tail: no final component to redirect
    };
    if fs.canonicalize(lit).await.is_err() {
        // Not-yet-existing (or a mocked no-op that never redirects) —
        // the folded resolve is already the identity.
        return resolved;
    }
    // The literal exists, so its parent exists: the parent walk always
    // canonicalizes here (a bare name's empty parent is the cwd, the
    // same anchor `canonicalize` resolves against).
    let parent = lit
        .parent()
        .filter(|p| *p != lit && !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    lexical_only(&resolve_effective(fs, parent).await.join(name))
}

/// Whether `effective` (already canonical) is confined to `glob`'s declared
/// root, judging the grant by its effective path identity ([`grant_identity`]
/// — NEP-0009). `named` is the path the task asked for, used only to tell
/// a planted-symlink-at-the-grant (the task named the grant verbatim) from
/// an ordinary out-of-boundary miss. The root is the glob's longest LITERAL
/// directory prefix (the part before any `*`). Confinement = `effective` IS
/// the root identity or a descendant of it — the descendancy test a
/// literal-prefix string match fails to make.
async fn confines<F: FsReadDyn>(
    fs: &F,
    glob: &str,
    named: &str,
    effective: &Path,
) -> ConfineVerdict {
    let (root_lit, tail) = literal_root(glob);
    // Both sides compare in the lexically-folded form: a canonical
    // (real-fs) path is already folded, so this is a no-op there, and a
    // mocked/relative path keeps its literal identity (`./x` ≡ `x`) —
    // the identity judgment never forks on spelling.
    let effective = &lexical_only(effective);
    // The permit root is judged by its IDENTITY (ancestor canonicalized,
    // final component lexical — never the fully-resolved form that
    // follows a planted final symlink). A raw `canonicalize` also failed
    // CLOSED on a not-yet-existing target — the common `nika:write` of a
    // new file (and exactly what `nika check --infer-permits` emits):
    // `nika check` passed, `nika run` then died on NIKA-SEC-004. The
    // identity fold admits the declared new file/dir while a fake root
    // still resolves to a path no real effective path can match —
    // fail-closed preserved. (A `..` inside a permit literal — e.g.
    // `out/../secret/**` — resolves here too, WIDENING the declared
    // boundary; that is author responsibility, not an escape: the
    // boundary only ever ADMITS on a match · authors should write
    // canonical `permits.fs` paths.)
    let canon_root = lexical_only(&grant_identity(fs, &root_lit).await);
    let Some(tail) = tail else {
        // An exact path (no wildcard) — the resolved target must BE the
        // grant's identity. The measured hole: the grant literal swapped
        // for a symlink BETWEEN check and run — resolved-vs-resolved
        // followed the plant on both sides and compared equal, serving
        // the out-of-bounds target through the in-process builtin the
        // sandbox never sees.
        if *effective == canon_root {
            return ConfineVerdict::Admits;
        }
        return if Path::new(named) == Path::new(&root_lit) {
            ConfineVerdict::IdentityMismatch(canon_root)
        } else {
            ConfineVerdict::Outside
        };
    };
    // Descendancy is NECESSARY but not SUFFICIENT. The wildcard half of the
    // permit is then re-applied to whatever the path has beyond the root.
    //
    // It used to be discarded. `literal_root` returned a bare `true` and this
    // arm admitted any descendant of the literal prefix, so `data/*.csv` meant
    // `data/**` and the extension was decoration. Measured on the published
    // 0.106.1, no attacker, no symlink, no traversal:
    //
    //     read:  ["data/*.csv"]  →  read  data/sub/deeper/private.key
    //     write: ["out/*.md"]    →  wrote out/sub/pwned.sh
    //
    // Both green at check with 0 hints, and the read's content landed in the
    // signed trace.
    //
    // The differential proptest that exists to catch exactly this drift was
    // scoped away from mid-pattern globs, on the grounds that "glob-pattern ⊆
    // permits-glob inclusion is not soundly decidable". That theorem is true
    // and it is about a different question: deciding whether one PATTERN is
    // contained in another. Here a CONCRETE resolved path is matched against
    // one pattern, which is ordinary glob matching and entirely decidable.
    // A correct theorem justified a shortcut on a problem it does not govern.
    let Ok(rest) = effective.strip_prefix(&canon_root) else {
        return ConfineVerdict::Outside;
    };
    // An empty remainder is the root itself: `<root>/**` admits it (zero
    // segments), `<root>/*` does not (it wants exactly one).
    if nika_cap::glob_admits(&tail, &rest.to_string_lossy()) {
        ConfineVerdict::Admits
    } else {
        ConfineVerdict::Outside
    }
}

/// Split a gitignore-style permit glob into its literal directory prefix and
/// the wildcard pattern that follows it.
///
/// `/data/in/**` → (`/data/in`, Some(`**`)) · `/var/*.log` → (`/var`,
/// Some(`*.log`)) · `/srv/*/cache/**` → (`/srv`, Some(`*/cache/**`)) ·
/// `/etc/x` → (`/etc/x`, None).
///
/// The prefix is resolved (symlinks and all) so the boundary is checked
/// against real filesystem identity; the TAIL is then matched lexically
/// against what remains. Returning the tail is the whole point: dropping it
/// — which the previous `(String, bool)` shape forced — turned every
/// wildcard permit into "any descendant of the literal prefix".
fn literal_root(glob: &str) -> (String, Option<String>) {
    if !glob.contains('*') {
        return (glob.to_owned(), None);
    }
    let mut root = PathBuf::new();
    let mut rest: Vec<String> = Vec::new();
    for comp in Path::new(glob).components() {
        let part = comp.as_os_str().to_string_lossy();
        if !rest.is_empty() || part.contains('*') {
            rest.push(part.into_owned());
        } else {
            root.push(comp.as_os_str());
        }
    }
    (root.to_string_lossy().into_owned(), Some(rest.join("/")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn literal_root_splits_at_the_first_wildcard() {
        assert_eq!(
            literal_root("/data/in/**"),
            ("/data/in".to_owned(), Some("**".to_owned())),
            "trailing /** → the dir prefix, and the ** is KEPT"
        );
        // This one used to assert `true` for the second field, which is how a
        // test came to pin the fail-open: `*.log` was dropped, so the permit
        // admitted `/var/log/secrets/id_rsa`. The pattern must survive the
        // split or there is nothing left to enforce.
        assert_eq!(
            literal_root("/var/log/*.log"),
            ("/var/log".to_owned(), Some("*.log".to_owned())),
            "a *.ext segment ends the prefix and BECOMES the tail"
        );
        assert_eq!(
            literal_root("/etc/cron.d/x"),
            ("/etc/cron.d/x".to_owned(), None),
            "no wildcard → the whole path is literal"
        );
        assert_eq!(
            literal_root("/srv/*/cache/**"),
            ("/srv".to_owned(), Some("*/cache/**".to_owned())),
            "a mid-path wildcard keeps every following segment"
        );
    }

    #[test]
    fn lexical_only_resolves_dot_and_dotdot() {
        assert_eq!(lexical_only(Path::new("/a/b/../c")), PathBuf::from("/a/c"));
        assert_eq!(lexical_only(Path::new("/a/./b")), PathBuf::from("/a/b"));
        assert_eq!(
            lexical_only(Path::new("/a/b/../../x")),
            PathBuf::from("/x"),
            "two `..` climb two levels"
        );
    }

    #[test]
    fn fold_trailing_applies_parentdir_and_curdir() {
        let base = PathBuf::from("/real/base");
        let comps = [
            Component::Normal("sub".as_ref()),
            Component::Normal("f".as_ref()),
        ];
        assert_eq!(
            fold_trailing(base.clone(), comps.iter()),
            PathBuf::from("/real/base/sub/f")
        );
        let up = [Component::ParentDir, Component::Normal("out".as_ref())];
        assert_eq!(
            fold_trailing(base, up.iter()),
            PathBuf::from("/real/out"),
            "a trailing `..` pops the canonical base"
        );
    }

    #[test]
    fn unbounded_is_not_declared() {
        assert!(!FsBoundary::unbounded().is_declared());
        assert!(FsBoundary::declared(vec![], vec![]).is_declared());
    }
}

/// The SECURITY tests — proven against the REAL filesystem (`TokioFs`),
/// because the escape vectors (`..` + symlinks) only resolve through a true
/// `canonicalize`; `MockFs::canonicalize` is a no-op. Each test owns a
/// unique `/tmp` scratch dir (parallel-safe) and tears it down.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod fs_security_tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    use nika_fs::TokioFs;

    use super::{FsAccess, FsBoundary};

    static SCRATCH_SEQ: AtomicU64 = AtomicU64::new(0);

    /// A unique scratch dir under the system temp root, with an `allowed/`
    /// subtree that the boundary will admit and a sibling SECRET outside it.
    struct Scratch {
        root: std::path::PathBuf,
    }

    /// Best-effort cleanup for a test path that must start nonexistent.
    struct Cleanup(std::path::PathBuf);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl Scratch {
        fn new() -> Self {
            let n = SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("nika-permits-test-{}-{n}", std::process::id()));
            std::fs::create_dir_all(root.join("allowed/sub")).unwrap();
            std::fs::write(root.join("allowed/in.txt"), b"inside").unwrap();
            std::fs::write(root.join("allowed/sub/deep.txt"), b"deep").unwrap();
            std::fs::write(root.join("secret.txt"), b"outside the boundary").unwrap();
            Self { root }
        }

        /// `<root>/allowed/**` — the declared boundary glob (both directions).
        fn boundary(&self) -> FsBoundary {
            let glob = format!("{}/allowed/**", self.root.display());
            FsBoundary::declared(vec![glob.clone()], vec![glob])
        }

        fn path(&self, rel: &str) -> String {
            self.root.join(rel).to_string_lossy().into_owned()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[tokio::test]
    async fn declared_write_tree_creates_the_parent() {
        // #433 option C · a `permits.fs.write` glob for `allowed/**` is the
        // author's declaration that the tree is theirs to make, so a new
        // sub-directory under it is created (both writers inherit this via
        // the guarded seam · uniform declared-intent behavior).
        let s = Scratch::new();
        let fs = TokioFs;
        let target = s.path("allowed/newdir/x.txt");
        let parent = s.path("allowed/newdir");
        assert!(!std::path::Path::new(&parent).exists(), "precondition");
        s.boundary().ensure_write_parent(&fs, &target).await;
        assert!(
            std::path::Path::new(&parent).exists(),
            "the declared write tree's parent is created"
        );
    }

    #[tokio::test]
    async fn concurrent_parent_creation_keeps_relative_write_identity_stable() {
        // The image-fx batch race measured on Linux CI: one fan-out item
        // resolved its target while `out/` did not exist, then its sibling
        // created `out/darkroom/` before the same judgment resolved the grant.
        // The target stayed relative while the grant became absolute, so two
        // spellings of the SAME path compared unequal and NIKA-SEC-004 fired.
        let n = SCRATCH_SEQ.fetch_add(1, Ordering::Relaxed);
        let root =
            std::path::PathBuf::from(format!(".nika-permits-race-{}-{n}", std::process::id()));
        let _cleanup = Cleanup(root.clone());
        let target = root.join("out/darkroom/sunset.png");
        let named = target.to_string_lossy().into_owned();
        let boundary =
            FsBoundary::declared(vec![], vec![format!("{}/out/darkroom/*", root.display())]);
        let fs = TokioFs;

        // First half of `enforce_judged`: no ancestor exists yet.
        let effective = super::resolve_effective(&fs, &target).await;
        // The sibling task wins the scheduler here and creates the grant root.
        std::fs::create_dir_all(root.join("out/darkroom")).unwrap();
        // Second half: resolving the grant after that creation must still
        // compare equal to the already-resolved target.
        let verdict = boundary
            .judge(&fs, &named, FsAccess::Write, &effective)
            .await;
        assert!(
            matches!(verdict, super::FsVerdict::Admitted),
            "parent creation cannot change path identity: effective={}",
            effective.display()
        );
    }

    #[tokio::test]
    async fn undeclared_boundary_creates_no_parent() {
        // No boundary in force (engine floor) → the helper is a no-op; each
        // writer keeps its own default (write gates on `create_dirs`, chart
        // uses the atomic seam · the un-declared corner, unchanged).
        let s = Scratch::new();
        let fs = TokioFs;
        let target = s.path("allowed/nope/y.txt");
        FsBoundary::unbounded()
            .ensure_write_parent(&fs, &target)
            .await;
        assert!(
            !std::path::Path::new(&s.path("allowed/nope")).exists(),
            "no boundary declared → nothing created"
        );
    }

    #[tokio::test]
    async fn traversal_out_of_boundary_is_blocked() {
        // `<root>/allowed/../secret.txt` resolves to `<root>/secret.txt`,
        // OUTSIDE the `allowed/**` boundary → NIKA-SEC-004 (both directions).
        let s = Scratch::new();
        let fs = TokioFs;
        let p = s.path("allowed/../secret.txt");
        let read = s.boundary().enforce(&fs, &p, FsAccess::Read).await;
        assert!(
            matches!(read, Err(ref f) if f.code == super::SEC_DENIED),
            "{read:?}"
        );
        // a WRITE traversal to a new file outside is blocked too.
        let pw = s.path("allowed/../TRAVERSED.txt");
        let write = s.boundary().enforce(&fs, &pw, FsAccess::Write).await;
        assert!(
            matches!(write, Err(ref f) if f.code == super::SEC_DENIED),
            "{write:?}"
        );
    }

    #[tokio::test]
    async fn the_denial_names_the_access_category() {
        // The boundary error names the ACCESS CATEGORY (read/write · the
        // `FsAccess::category` projection) so the author knows WHICH permit class
        // refused — not a generic « outside the boundary ».
        let s = Scratch::new();
        let fs = TokioFs;
        let read = s
            .boundary()
            .enforce(&fs, &s.path("allowed/../secret.txt"), FsAccess::Read)
            .await;
        assert!(
            matches!(&read, Err(f) if f.message.contains("read")),
            "the read denial names « read »: {read:?}"
        );
        let write = s
            .boundary()
            .enforce(&fs, &s.path("allowed/../X.txt"), FsAccess::Write)
            .await;
        assert!(
            matches!(&write, Err(f) if f.message.contains("write")),
            "the write denial names « write »: {write:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_escape_is_blocked() {
        // A symlink INSIDE the boundary pointing at the parent — reading
        // THROUGH it must resolve to the real (outside) target and refuse.
        let s = Scratch::new();
        let link = s.root.join("allowed/link");
        std::os::unix::fs::symlink(&s.root, &link).unwrap();
        let fs = TokioFs;
        let through = format!("{}/allowed/link/secret.txt", s.root.display());
        let got = s.boundary().enforce(&fs, &through, FsAccess::Read).await;
        assert!(
            matches!(got, Err(ref f) if f.code == super::SEC_DENIED),
            "a symlink that escapes the boundary must be refused: {got:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_exact_grant_swapped_to_a_symlink_is_refused() {
        // The measured 2026-08-08 hole (NEP-0009 on the builtin arm): the
        // grant's OWN literal is replaced by a symlink BETWEEN check and
        // run. The floor re-judged the path the task NAMED — and both
        // sides of the resolved-vs-resolved comparison followed the same
        // plant, so the swapped grant compared equal and was admitted.
        // The identity judgment holds the grant's FINAL component
        // lexical: the judged form cannot be redirected.
        let s = Scratch::new();
        let grant = s.path("allowed/swapped.txt");
        std::fs::write(&grant, b"honest").unwrap();
        let boundary = FsBoundary::declared(vec![grant.clone()], vec![]);
        let fs = TokioFs;
        // The honest tree admits — the instrument is qualified (the
        // grant works before the pivot).
        assert!(
            boundary.enforce(&fs, &grant, FsAccess::Read).await.is_ok(),
            "the honest grant admits"
        );
        // The pivot: the grant literal becomes a symlink OUT of the
        // judged tree.
        std::fs::remove_file(&grant).unwrap();
        std::os::unix::fs::symlink(s.root.join("secret.txt"), &grant).unwrap();
        let got = boundary.enforce(&fs, &grant, FsAccess::Read).await;
        assert!(
            matches!(got, Err(ref f) if f.code == super::SEC_DENIED),
            "a swapped exact grant refuses, never follows: {got:?}"
        );
        // NEP-0009 law 3 — the refusal names the judged prefix and the
        // resolved target, in the exec arm's `fs.path_mismatch` voice.
        let Err(f) = got else { panic!("refused above") };
        assert!(f.message.contains("fs.path_mismatch"), "{f:?}");
        assert!(f.message.contains("secret.txt"), "{f:?}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_exact_grant_symlink_is_refused_even_pointing_inside() {
        // NEP-0009 backwards-compat: a grant whose final component is a
        // symlink changes verdict EVEN when the target stays inside the
        // tree — divergence, not escape, is the trigger (the exec arm's
        // parity: « final-component symlink to a sibling refused »). The
        // author declares the effective path instead.
        let s = Scratch::new();
        let link = s.path("allowed/alias.txt");
        std::os::unix::fs::symlink(s.root.join("allowed/in.txt"), &link).unwrap();
        let boundary = FsBoundary::declared(vec![link.clone()], vec![]);
        let fs = TokioFs;
        let got = boundary.enforce(&fs, &link, FsAccess::Read).await;
        assert!(
            matches!(got, Err(ref f) if f.code == super::SEC_DENIED),
            "a symlinked grant literal refuses even pointing inside: {got:?}"
        );
        // …while the SAME target declared by its effective path admits.
        let direct = FsBoundary::declared(vec![s.path("allowed/in.txt")], vec![]);
        assert!(
            direct
                .enforce(&fs, &s.path("allowed/in.txt"), FsAccess::Read)
                .await
                .is_ok(),
            "the effective path declared directly admits"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_glob_root_swapped_to_a_symlink_is_refused() {
        // Platform parity (NEP-0009 law 4) on the GLOB arm: the exec
        // arm's `judge_identity` refuses a symlink at the glob's literal
        // ROOT (bwrap would mount the target); the builtin arm judged
        // the root fully-resolved and followed the plant. The identity
        // judgment holds the root's final component lexical, so the
        // swapped tree no longer confines anything.
        let s = Scratch::new();
        let fs = TokioFs;
        // The honest tree admits (instrument qualification).
        assert!(
            s.boundary()
                .enforce(&fs, &s.path("allowed/in.txt"), FsAccess::Read)
                .await
                .is_ok(),
            "the honest glob root admits"
        );
        // The pivot: the glob's root dir becomes a symlink to the parent
        // (which holds the out-of-boundary secret).
        std::fs::remove_dir_all(s.root.join("allowed")).unwrap();
        std::os::unix::fs::symlink(&s.root, s.root.join("allowed")).unwrap();
        let through = s.path("allowed/secret.txt");
        let got = s.boundary().enforce(&fs, &through, FsAccess::Read).await;
        assert!(
            matches!(got, Err(ref f) if f.code == super::SEC_DENIED),
            "a swapped glob root refuses: {got:?}"
        );
    }

    #[tokio::test]
    async fn legit_read_inside_boundary_is_allowed() {
        let s = Scratch::new();
        let fs = TokioFs;
        let p = s.path("allowed/in.txt");
        assert!(
            s.boundary().enforce(&fs, &p, FsAccess::Read).await.is_ok(),
            "an existing file inside the boundary reads"
        );
    }

    #[tokio::test]
    async fn legit_new_file_write_inside_boundary_is_allowed() {
        // The not-yet-existing write target must be ADMITTED when it lands
        // inside the boundary (the longest-existing-ancestor resolution).
        let s = Scratch::new();
        let fs = TokioFs;
        let p = s.path("allowed/sub/brand-new.txt");
        assert!(
            s.boundary().enforce(&fs, &p, FsAccess::Write).await.is_ok(),
            "a new file inside the boundary is writable"
        );
        // even one whose PARENT dir does not exist yet (create_dirs path).
        let deep = s.path("allowed/fresh-dir/deeper/new.txt");
        assert!(
            s.boundary()
                .enforce(&fs, &deep, FsAccess::Write)
                .await
                .is_ok(),
            "a new file under a not-yet-created dir inside the boundary is writable"
        );
    }

    #[tokio::test]
    async fn exact_file_write_permit_admits_its_own_not_yet_existing_file() {
        // `permits.fs.write: ["<root>/allowed/report.json"]` — an EXACT path
        // (no wildcard) for a file that does NOT exist yet (the common
        // nika:write-a-new-file case · and EXACTLY what `nika check
        // --infer-permits` emits). It MUST admit a write to that file.
        // Regression: the permit root was canonicalized whole, so a
        // not-yet-existing target failed closed → NIKA-SEC-004 — `nika check`
        // passed but `nika run` died on the tool's own inferred permit.
        let s = Scratch::new();
        let fs = TokioFs;
        let target = s.path("allowed/report.json"); // inside allowed/ · uncreated
        let boundary = FsBoundary::declared(vec![], vec![target.clone()]);
        assert!(
            boundary
                .enforce(&fs, &target, FsAccess::Write)
                .await
                .is_ok(),
            "an exact-file write permit must admit writing exactly that new file"
        );
        // The exact match is PRECISE — a sibling under allowed/ is NOT admitted
        // by a single-file permit (no accidental directory widening).
        let other = s.path("allowed/other.json");
        assert!(
            matches!(
                boundary.enforce(&fs, &other, FsAccess::Write).await,
                Err(ref f) if f.code == super::SEC_DENIED
            ),
            "an exact-file permit admits ONLY that file, not a sibling"
        );
    }

    #[tokio::test]
    async fn dotdot_that_stays_inside_is_allowed() {
        // `<root>/allowed/sub/../in.txt` resolves back to `<root>/allowed/
        // in.txt` — INSIDE the boundary, so it is admitted (a `..` is not
        // an escape unless it actually climbs out).
        let s = Scratch::new();
        let fs = TokioFs;
        let p = s.path("allowed/sub/../in.txt");
        assert!(
            s.boundary().enforce(&fs, &p, FsAccess::Read).await.is_ok(),
            "a `..` that stays inside the boundary is fine"
        );
    }

    #[tokio::test]
    async fn an_undeclared_boundary_enforces_nothing() {
        // The pre-permits floor: no boundary declared → any path passes
        // (the engine floor is the only gate · spec §permits).
        let s = Scratch::new();
        let fs = TokioFs;
        let outside = s.path("secret.txt");
        assert!(
            FsBoundary::unbounded()
                .enforce(&fs, &outside, FsAccess::Read)
                .await
                .is_ok(),
            "no boundary → nothing to enforce"
        );
    }

    #[tokio::test]
    async fn a_declared_boundary_with_empty_lists_denies_by_default() {
        // `permits: {}` (declared · no fs lists) → default-deny: even an
        // in-tree path is refused because nothing is granted.
        let s = Scratch::new();
        let fs = TokioFs;
        let inside = s.path("allowed/in.txt");
        let denied = FsBoundary::declared(vec![], vec![])
            .enforce(&fs, &inside, FsAccess::Read)
            .await;
        assert!(
            matches!(denied, Err(ref f) if f.code == super::SEC_DENIED),
            "{denied:?}"
        );
    }

    #[tokio::test]
    async fn a_nonexistent_boundary_root_grants_nothing() {
        // A glob whose literal root does not exist cannot confine anything
        // (fail-closed) — a path is refused rather than wrongly admitted.
        let s = Scratch::new();
        let fs = TokioFs;
        let glob = format!("{}/does-not-exist/**", s.root.display());
        let boundary = FsBoundary::declared(vec![glob], vec![]);
        let p = s.path("allowed/in.txt");
        let denied = boundary.enforce(&fs, &p, FsAccess::Read).await;
        assert!(
            matches!(denied, Err(ref f) if f.code == super::SEC_DENIED),
            "{denied:?}"
        );
    }

    #[tokio::test]
    async fn the_helper_resolves_existing_paths_through_canonicalize() {
        // Path::canonicalize on macOS resolves /tmp → /private/tmp: the
        // boundary root is canonicalized on BOTH sides, so the symlinked
        // temp root compares by its real location (no false escape).
        let s = Scratch::new();
        let fs = TokioFs;
        let resolved = super::resolve_effective(&fs, Path::new(&s.path("allowed/in.txt"))).await;
        assert!(
            resolved.is_absolute() && resolved.ends_with("allowed/in.txt"),
            "resolved to a real absolute path: {resolved:?}"
        );
    }
}

/// The fs boundary DIFFERENTIAL — the static checker and the runtime enforcer
/// decide the `permits.fs` boundary the same way on the CANONICAL glob forms.
///
/// `nika_cap::Permits::allows_path` (what `nika check`/`permits_fit` consults)
/// and this crate's `FsBoundary::enforce` (what refuses at effect time) once
/// read: « two independent implementations · they share no code, so a common
/// bug would have to be born twice ». They drifted instead, in the permissive
/// direction, and the differential below was scoped away from exactly the
/// forms that broke — so it ran green while `data/*.csv` granted `data/**` on
/// the published binary. Both now decide through the ONE predicate
/// `nika_cap::glob_admits`, the arrangement hosts have always had via
/// `nika_types::net::host_glob_matches`.
///
/// What this proptest guards now is the part that legitimately still differs:
/// the runtime resolves the permit root against the real filesystem before
/// matching. On a symlink-free `MockFs` (canonicalize is a no-op) that
/// resolution is the identity, so the two verdicts must agree exactly — over
/// every tail form, mid-pattern included.
///
/// The old scoping cited `crate::effect`'s « glob-pattern ⊆ permits-glob
/// inclusion is not soundly decidable ». That is a true statement about
/// containment between two PATTERNS. Both sides here match a CONCRETE path
/// against ONE pattern, which is ordinary glob matching and entirely
/// decidable — a correct theorem waived a proof obligation it did not govern,
/// and the fail-open lived in the gap.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod fs_boundary_differential {
    use nika_cap::{FsPermits, Permits};
    use nika_kernel_mock::MockFs;
    use proptest::prelude::*;

    use super::{FsAccess, FsBoundary};

    fn seg() -> impl Strategy<Value = String> {
        prop_oneof!["a", "b", "sub", "x"].prop_map(String::from)
    }

    /// A CANONICAL fs permit glob · `<segs>/**` (recursive) or a literal
    /// `<segs>` · never a mid-pattern `*` (the known non-decidable form).
    /// The glob TAIL. The generator used to emit `**` or nothing, which is
    /// why the fail-open survived it: `*`, `*.csv` and `*/x` were never
    /// generated, and those are the forms where the runtime discarded the
    /// pattern and admitted the whole subtree.
    fn tail() -> impl Strategy<Value = Option<String>> {
        prop_oneof![
            Just(None),                     // a literal path · no wildcard
            Just(Some("**".to_owned())),    // any descendant, any depth
            Just(Some("*".to_owned())),     // exactly one segment
            Just(Some("*.csv".to_owned())), // one segment, by extension
            Just(Some("*/x".to_owned())),   // MID-PATTERN · the excluded class
            Just(Some("*/**".to_owned())),  // one segment, then any depth
        ]
    }

    fn canon_glob() -> impl Strategy<Value = String> {
        (prop::collection::vec(seg(), 1..3), tail()).prop_map(|(segs, tail)| {
            let base = segs.join("/");
            tail.map_or(base.clone(), |t| format!("{base}/{t}"))
        })
    }

    fn glob_list() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec(canon_glob(), 0..3)
    }

    fn path() -> impl Strategy<Value = String> {
        prop::collection::vec(seg(), 1..4).prop_map(|s| s.join("/"))
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

        #[test]
        fn static_and_runtime_agree_on_canonical_fs_globs(
            read in glob_list(),
            write in glob_list(),
            p in path(),
            is_write in any::<bool>(),
        ) {
            // static side · nika-cap · the predicate `nika check` consults
            let mut permits = Permits::new();
            permits.fs = Some(FsPermits::new(read.clone(), write.clone()));
            let static_allows = permits.allows_path(&p, is_write);

            // runtime side · this crate · what refuses at effect time. MockFs
            // canonicalize is a no-op, so enforce folds lexically (symlink-free).
            let boundary = FsBoundary::declared(read, write);
            let access = if is_write { FsAccess::Write } else { FsAccess::Read };
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let runtime_allows = rt.block_on(boundary.enforce(&MockFs::new(), &p, access)).is_ok();

            prop_assert_eq!(static_allows, runtime_allows);
        }
    }
}
