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
//! pre-permits behaviour (« absent `permits:` = today's behaviour, the
//! engine floor is the only gate » · spec §permits).

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
/// retryable, never fed back to an `agent:` model · independent of `permits:`).
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
    /// boundary is admitted while a `..` that climbs out is refused.
    pub(crate) async fn enforce<F: FsReadDyn>(
        &self,
        fs: &F,
        path: &str,
        access: FsAccess,
    ) -> Result<(), BuiltinFailure> {
        if !self.declared {
            return Ok(()); // no boundary in force · the engine floor only
        }
        let effective = resolve_effective(fs, Path::new(path)).await;
        let globs = self.globs(access);
        for glob in globs {
            if confines(fs, glob, &effective).await {
                return Ok(());
            }
        }
        Err(BuiltinFailure::new(
            SEC_DENIED,
            format!(
                "`{path}` resolves outside the declared {} boundary",
                access.category()
            ),
        ))
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
            // No existing ancestor (or hit the root): fold lexically from a
            // best-effort base (the leading absolute/relative components).
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

/// Whether `effective` (already canonical) is confined to `glob`'s declared
/// root. The root is the glob's longest LITERAL directory prefix (the part
/// before any `*`); it is canonicalized so a symlinked boundary compares by
/// its real location too. Confinement = `effective` IS the root or a
/// descendant of it — the descendancy test a literal-prefix string match
/// fails to make.
async fn confines<F: FsReadDyn>(fs: &F, glob: &str, effective: &Path) -> bool {
    let (root_lit, has_wildcard) = literal_root(glob);
    // Resolve the permit root the SAME way the effective path was resolved
    // ([`resolve_effective`]: longest existing ancestor canonicalized —
    // symlinks resolved, the security core — then the trailing non-existent
    // components folded lexically). The literal prefix is a directory for a
    // `**`/`*` glob; for an exact-path glob it IS the target. A raw
    // `canonicalize` failed CLOSED on a not-yet-existing target — the common
    // `nika:write` of a new file (and exactly what `nika check
    // --infer-permits` emits): `nika check` passed, `nika run` then died on
    // NIKA-SEC-004. Resolving the root like the write path admits the
    // declared new file/dir while a fake root still resolves to a path no
    // real (canonicalized) effective path can match — fail-closed preserved.
    // (A `..` inside a permit literal — e.g. `out/../secret/**` — resolves
    // here too, WIDENING the declared boundary; that is author responsibility,
    // not an escape: the boundary only ever ADMITS on a match · authors should
    // write canonical `permits.fs` paths.)
    let canon_root = resolve_effective(fs, Path::new(&root_lit)).await;
    if has_wildcard {
        // `<root>/**` · `<root>/*` etc. — any descendant of the real root
        // (and the root itself, matching the static `/**`-includes-root rule).
        effective == canon_root || effective.starts_with(&canon_root)
    } else {
        // An exact path (no wildcard) — the resolved target must BE it.
        effective == canon_root
    }
}

/// Split a gitignore-style permit glob into (literal directory prefix, has
/// a wildcard). `/data/in/**` → (`/data/in`, true) · `/etc/x` → (`/etc/x`,
/// false) · `/var/*.log` → (`/var`, true). The literal prefix is every
/// component up to (not including) the first one containing `*`.
fn literal_root(glob: &str) -> (String, bool) {
    if !glob.contains('*') {
        return (glob.to_owned(), false);
    }
    let mut root = PathBuf::new();
    for comp in Path::new(glob).components() {
        let part = comp.as_os_str().to_string_lossy();
        if part.contains('*') {
            break; // the first wildcard segment ends the literal prefix
        }
        root.push(comp.as_os_str());
    }
    (root.to_string_lossy().into_owned(), true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn literal_root_splits_at_the_first_wildcard() {
        assert_eq!(
            literal_root("/data/in/**"),
            ("/data/in".to_owned(), true),
            "trailing /** → the dir prefix"
        );
        assert_eq!(
            literal_root("/var/log/*.log"),
            ("/var/log".to_owned(), true),
            "a *.ext segment ends the literal prefix"
        );
        assert_eq!(
            literal_root("/etc/cron.d/x"),
            ("/etc/cron.d/x".to_owned(), false),
            "no wildcard → the whole path is literal"
        );
        assert_eq!(
            literal_root("/srv/*/cache/**"),
            ("/srv".to_owned(), true),
            "a mid-path wildcard segment ends the prefix"
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
