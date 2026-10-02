// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `JudgedFs` · the builtin arm's fd-pin, closing the enforce→open race
//! window NEP-0009 law 6 left declared (the builtin-side follow-on).
//!
//! The dispatch guard enforces the `permits.fs` boundary and THEN the op
//! opens the path. Between the two, a parallel task of the same run can
//! swap the judged file for a symlink: a plain `open(2)` follows it and
//! serves the swapped target through the in-process builtin no sandbox
//! sees (the parallel sibling of the sequenced 2026-08-08 pivot).
//! `JudgedFs` wraps the production fs for every builtin READ:
//!
//! 1. the named path is re-enforced at open time · the same coded
//!    `NIKA-SEC-004` judgment as the guard, so the window shrinks to the
//!    stretch between that judgment and the syscall;
//! 2. the open is the BACKEND's own pin ([`FsReadDyn::read_pinned`]) · the
//!    host backend opens the final component `O_NOFOLLOW` (the kernel
//!    refuses a symlinked name inside the open syscall itself, atomically),
//!    a rooted room refuses a symlink at any component, and a backend with
//!    no pin answers its typed `PinUnavailable` · no host path of the same
//!    name is ever opened behind the backend;
//! 3. on `SymlinkRefused` the path is canonicalized and the RESOLVED TARGET
//!    is re-judged, then read through the pin again · a pre-existing
//!    INSIDE-pointing symlink admitted under a glob grant is still served
//!    (the no-regression rule), a swapped one refuses coded. The loop is
//!    hard-bounded ([`MAX_HOPS`]): a symlink storm refuses instead of
//!    hanging, and an unresolvable path (a ↔ b) is the coded refusal.
//!
//! The WRITE lane needs no pin: the production write is temp-file +
//! rename, and `rename(2)` REPLACES a symlinked destination (it never
//! follows it) · the executed write is always the judged path's own inode
//! exchange, and nothing the destination NAME pointed to is ever opened.
//!
//! Declared gaps. What a pin covers is the backend's statement: the host
//! backend's `O_NOFOLLOW` pins the FINAL component only · a swapped
//! ANCESTOR directory mid-path is still followed by the kernel; that
//! residual belongs to the exec arm's `--bind-fd` mount follow-on class.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use nika_kernel::io::fs::{FileMetadata, FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};

use crate::BuiltinFailure;
use crate::permits::{FsAccess, FsBoundary, SEC_DENIED};

/// The hard bound on the re-judge loop · a path that keeps resolving to a
/// symlink past this many hops is a storm: refuse, never hang.
const MAX_HOPS: u8 = 8;

/// The judged fs view one guarded op runs against: `inner` does the I/O,
/// `boundary` owns the judgment (always `permits.fs.read` · the pin guards
/// the READ lane; writes delegate to the atomic lane untouched).
pub(crate) struct JudgedFs<'a, F> {
    inner: &'a F,
    boundary: &'a FsBoundary,
}

impl<'a, F> JudgedFs<'a, F> {
    /// Wrap `inner` so every read is boundary-judged at open time and
    /// fd-pinned against the enforce→open swap (NEP-0009 law 6).
    pub(crate) fn new(inner: &'a F, boundary: &'a FsBoundary) -> Self {
        Self { inner, boundary }
    }
}

impl<F: FsReadDyn> JudgedFs<'_, F> {
    /// The judged read. An UNDECLARED boundary judges nothing, so nothing
    /// is pinned either: delegate verbatim (the pre-permits floor · this
    /// is also what keeps a virtual mock fs byte-identical, its paths
    /// having no real disk to open).
    async fn read_bytes(&self, path: &Path) -> Result<Bytes, FsError> {
        if !self.boundary.is_declared() {
            return self.inner.read(path).await;
        }
        self.read_pinned(path).await.map_err(PinError::into_fs)
    }

    /// The judged string read · same delegation law, then the pinned
    /// bytes decoded (non-UTF-8 stays the `InvalidData` class the ops
    /// already map, e.g. `NIKA-BUILTIN-READ-003`).
    async fn read_string(&self, path: &Path) -> Result<String, FsError> {
        if !self.boundary.is_declared() {
            return self.inner.read_to_string(path).await;
        }
        let bytes = self.read_bytes(path).await?;
        String::from_utf8(bytes.to_vec()).map_err(|e| FsError::InvalidData {
            path: path.display().to_string(),
            reason: e.to_string(),
        })
    }

    /// The pinned read: re-judge at open time, then the backend's own pin
    /// with the bounded resolve-and-re-judge loop.
    async fn read_pinned(&self, path: &Path) -> Result<Bytes, PinError> {
        // 1 · the at-open re-judgment · the same helper, the same coded
        // refusal as the dispatch guard — UNWITNESSED: the guard already
        // journaled this op's decision (one fs frame per op).
        self.boundary
            .enforce_unwitnessed(self.inner, &path.to_string_lossy(), FsAccess::Read)
            .await
            .map_err(PinError::Denied)?;
        let mut current = path.to_path_buf();
        let mut hops = 0u8;
        loop {
            match self.inner.read_pinned(&current).await {
                Ok(bytes) => return Ok(bytes),
                Err(FsError::SymlinkRefused { .. }) => {
                    if hops == MAX_HOPS {
                        return Err(PinError::Denied(storm_refusal(path)));
                    }
                    hops += 1;
                    // 3 · the final component is a symlink AT OPEN TIME:
                    // resolve it and re-judge the TARGET, never the name.
                    let resolved =
                        self.inner
                            .canonicalize(&current)
                            .await
                            .map_err(|ce| match ce {
                                // A dangling link keeps today's file-not-found
                                // verdict · nothing is ever opened on this arm.
                                FsError::NotFound { .. } => PinError::Fs(ce),
                                // A loop (a ↔ b) cannot canonicalize: fail
                                // closed, coded.
                                other => PinError::Denied(loop_refusal(&current, &other)),
                            })?;
                    self.boundary
                        .enforce_unwitnessed(
                            self.inner,
                            &resolved.to_string_lossy(),
                            FsAccess::Read,
                        )
                        .await
                        .map_err(PinError::Denied)?;
                    current = resolved;
                }
                Err(e) => return Err(PinError::Fs(e)),
            }
        }
    }
}

/// The pin's own coded refusal for the path that cannot be resolved at
/// open time (a ↔ b loop · an unreadable component): the exec arm's
/// `fs.path_mismatch` voice (NEP-0009 law 3), the `NIKA-SEC-004` class
/// exactly as `permits.rs` · fail-closed, the cause carried verbatim.
fn loop_refusal(named: &Path, cause: &FsError) -> BuiltinFailure {
    BuiltinFailure::new(
        SEC_DENIED,
        format!(
            "fs.path_mismatch · `{}` cannot be resolved at open time ({cause}) · \
             the open keeps meeting a symlink and the resolve failed · refused \
             (NEP-0009 law 6)",
            named.display()
        ),
    )
}

/// The coded refusal for the hop-bound storm: the path kept redirecting
/// past [`MAX_HOPS`] re-opens.
fn storm_refusal(named: &Path) -> BuiltinFailure {
    BuiltinFailure::new(
        SEC_DENIED,
        format!(
            "fs.path_mismatch · `{}` keeps redirecting past {MAX_HOPS} hops · \
             a symlink storm at open time · refused (NEP-0009 law 6)",
            named.display()
        ),
    )
}

/// The pin's two failure planes. The fs-trait channel carries only
/// [`FsError`], and the op downstream owns the mapping of the ORDINARY
/// kinds (`NotFound` → its `NIKA-BUILTIN-READ-001` &c) · so the CODED
/// boundary refusal flattens into [`FsError::Io`] with its full
/// `<code> · <message>` voice preserved in the reason text. The
/// attestation survives (law 3); the bare-code wire shape stays with the
/// guard's early refusal, which is exactly where a non-raced swap lands.
enum PinError {
    /// The coded `NIKA-SEC-004` refusal (what `enforce` or the pin made).
    Denied(BuiltinFailure),
    /// An ordinary fs error · the op's own code mapping owns it.
    Fs(FsError),
}

impl PinError {
    fn into_fs(self) -> FsError {
        match self {
            Self::Denied(f) => FsError::Io {
                reason: format!("{} · {}", f.code, f.message),
            },
            Self::Fs(e) => e,
        }
    }
}

impl<F: FsReadDyn> FsReadDyn for JudgedFs<'_, F> {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        self.read_bytes(path).await
    }

    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        self.read_string(path).await
    }

    async fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path).await
    }

    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        self.inner.canonicalize(path).await
    }
}

impl<F: FsWriteDyn> FsWriteDyn for JudgedFs<'_, F> {
    /// Writes delegate VERBATIM · the atomic temp+rename lane replaces
    /// (never follows) a symlinked destination, so no pin is needed.
    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        self.inner.write(path, contents).await
    }

    async fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        self.inner.write_new(path, contents).await
    }

    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        self.inner.create_dir_all(path).await
    }

    async fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        self.inner.remove_file(path).await
    }
}

impl<F: FsMetaDyn> FsMetaDyn for JudgedFs<'_, F> {
    async fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError> {
        self.inner.metadata(path).await
    }
}

impl<F: FsListDyn> FsListDyn for JudgedFs<'_, F> {
    async fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        self.inner.list_dir(path).await
    }

    async fn glob(&self, root: &Path, pattern: &str) -> Result<Vec<PathBuf>, FsError> {
        self.inner.glob(root, pattern).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use bytes::Bytes;
    use nika_fs::{EffectLedger, OwnedDir, RoomLimits, RootedFs, TokioFs};
    use nika_kernel::io::fs::{FsError, FsReadDyn};
    use nika_kernel_mock::MockFs;

    use super::*;
    use crate::permits::SEC_DENIED;

    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// A unique scratch dir (parallel-safe) holding an `allowed/` subtree
    /// the boundary admits and an `oob/` sibling with the secret · the
    /// permits.rs `fs_security_tests` fixture shape, with the outside tree
    /// named for what it is.
    struct Scratch {
        root: PathBuf,
    }

    impl Scratch {
        fn new() -> Self {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("nika-judged-{}-{n}", std::process::id()));
            // Created exclusively and owned from here: only a root this test made is removed.
            std::fs::create_dir(&root).expect("HARNESS_INVALID: the scratch root is not ours");
            let scratch = Self { root };
            std::fs::create_dir_all(scratch.root.join("allowed")).unwrap();
            std::fs::create_dir_all(scratch.root.join("oob")).unwrap();
            std::fs::write(scratch.root.join("allowed/real.txt"), b"inside-bytes").unwrap();
            std::fs::write(scratch.root.join("oob/secret.txt"), b"secret-bytes").unwrap();
            scratch
        }

        /// `<root>/allowed/**` · the declared READ boundary glob.
        fn boundary(&self) -> FsBoundary {
            FsBoundary::declared(vec![format!("{}/allowed/**", self.root.display())], vec![])
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
    async fn an_honest_regular_file_read_is_served() {
        // The control: a plain file under the declared boundary reads,
        // byte-exact, both read forms.
        let s = Scratch::new();
        let fs = TokioFs;
        let boundary = s.boundary();
        let judged = JudgedFs::new(&fs, &boundary);
        let bytes = judged.read(&s.root.join("allowed/real.txt")).await.unwrap();
        assert_eq!(&bytes[..], b"inside-bytes");
        let text = judged
            .read_to_string(&s.root.join("allowed/real.txt"))
            .await
            .unwrap();
        assert_eq!(text, "inside-bytes");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_backend_pin_never_serves_a_symlink_target() {
        // THE primitive, now the backend's: the host pin refuses a symlinked
        // final component typed, inside the open · the target's bytes never
        // come back.
        let s = Scratch::new();
        let link = s.root.join("allowed/link.txt");
        std::os::unix::fs::symlink(s.root.join("allowed/real.txt"), &link).unwrap();
        // Control: an honest file reads whole through the same pin.
        let honest = TokioFs
            .read_pinned(&s.root.join("allowed/real.txt"))
            .await
            .unwrap();
        assert_eq!(&honest[..], b"inside-bytes");
        let err = TokioFs
            .read_pinned(&link)
            .await
            .expect_err("a symlinked final component must never be followed");
        assert!(
            matches!(err, FsError::SymlinkRefused { .. }),
            "the typed refusal: {err:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_inside_pointing_symlink_under_a_glob_grant_is_served() {
        // The no-regression rule: a PRE-EXISTING symlink whose target
        // stays inside the boundary is admitted under the glob grant ·
        // the ELOOP arm resolves it, re-judges the TARGET (inside), and
        // serves the resolved path pinned.
        let s = Scratch::new();
        let link = s.root.join("allowed/link.txt");
        std::os::unix::fs::symlink(s.root.join("allowed/real.txt"), &link).unwrap();
        let fs = TokioFs;
        let boundary = s.boundary();
        let judged = JudgedFs::new(&fs, &boundary);
        let bytes = judged.read(&link).await.unwrap();
        assert_eq!(&bytes[..], b"inside-bytes", "the inside link is served");
        let text = judged.read_to_string(&link).await.unwrap();
        assert_eq!(text, "inside-bytes");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_outside_pointing_symlink_is_refused_coded() {
        // `./allowed/leak.txt` → `./oob/secret.txt`: the at-open judgment
        // refuses with the `NIKA-SEC-004` class · zero secret bytes.
        let s = Scratch::new();
        let link = s.root.join("allowed/leak.txt");
        std::os::unix::fs::symlink(s.root.join("oob/secret.txt"), &link).unwrap();
        let fs = TokioFs;
        let boundary = s.boundary();
        let judged = JudgedFs::new(&fs, &boundary);
        let err = judged.read(&link).await.expect_err("the escape refuses");
        let text = err.to_string();
        assert!(text.contains(SEC_DENIED), "the coded class: {text}");
        assert!(
            !text.contains("secret-bytes"),
            "zero secret bytes surface: {text}"
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn a_symlink_loop_is_a_bounded_coded_refusal() {
        // a ↔ b: the path never resolves · the refusal is coded and the
        // call RETURNS (the test completing is the no-hang proof).
        let s = Scratch::new();
        let a = s.root.join("allowed/a");
        let b = s.root.join("allowed/b");
        std::os::unix::fs::symlink(&b, &a).unwrap();
        std::os::unix::fs::symlink(&a, &b).unwrap();
        let fs = TokioFs;
        let boundary = s.boundary();
        let judged = JudgedFs::new(&fs, &boundary);
        let err = judged.read(&a).await.expect_err("a loop refuses");
        let text = err.to_string();
        assert!(
            text.contains(SEC_DENIED) && text.contains("fs.path_mismatch"),
            "the bounded coded refusal: {text}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_exact_grant_swapped_to_a_symlink_before_the_read_is_refused_coded() {
        // The 2026-08-08 pivot shape at the WRAPPER plane: the grant
        // `./allowed.txt` admitted while a real file, then swapped to a
        // symlink to the outside secret before the read · the at-open
        // re-judgment holds the grant's final component lexical, so the
        // divergence refuses coded.
        let s = Scratch::new();
        let grant = s.root.join("allowed.txt");
        std::fs::write(&grant, b"honest").unwrap();
        let boundary = FsBoundary::declared(vec![s.path("allowed.txt")], vec![]);
        let fs = TokioFs;
        // Instrument qualification: the honest grant reads.
        let honest = JudgedFs::new(&fs, &boundary).read(&grant).await.unwrap();
        assert_eq!(&honest[..], b"honest");
        // The pivot.
        std::fs::remove_file(&grant).unwrap();
        std::os::unix::fs::symlink(s.root.join("oob/secret.txt"), &grant).unwrap();
        let err = JudgedFs::new(&fs, &boundary)
            .read(&grant)
            .await
            .expect_err("the swapped grant refuses");
        let text = err.to_string();
        assert!(
            text.contains(SEC_DENIED) && text.contains("fs.path_mismatch"),
            "the coded mismatch refusal: {text}"
        );
        assert!(
            !text.contains("secret-bytes"),
            "zero secret bytes surface: {text}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_undeclared_boundary_delegates_and_follows_the_link() {
        // The pre-permits floor: no boundary declared · nothing is judged,
        // nothing is pinned, the read follows (today's exact behavior, and
        // what the unbounded grep test asserts one plane up).
        let s = Scratch::new();
        let link = s.root.join("allowed/leak.txt");
        std::os::unix::fs::symlink(s.root.join("oob/secret.txt"), &link).unwrap();
        let fs = TokioFs;
        let floor = FsBoundary::unbounded();
        let judged = JudgedFs::new(&fs, &floor);
        let bytes = judged.read(&link).await.unwrap();
        assert_eq!(&bytes[..], b"secret-bytes", "the floor follows the link");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_dangling_symlink_keeps_the_file_not_found_verdict() {
        // A link whose target does not exist: nothing is opened after the
        // failed resolve, and the verdict stays today's file-not-found
        // (never a coded refusal for a benign dangling link).
        let s = Scratch::new();
        let link = s.root.join("allowed/dangling.txt");
        std::os::unix::fs::symlink(s.root.join("allowed/gone.txt"), &link).unwrap();
        let fs = TokioFs;
        let boundary = s.boundary();
        let judged = JudgedFs::new(&fs, &boundary);
        let err = judged.read(&link).await.expect_err("dangling is not found");
        assert!(
            matches!(err, FsError::NotFound { .. }),
            "the pre-pin verdict preserved: {err:?}"
        );
    }

    #[tokio::test]
    async fn an_undeclared_boundary_serves_a_virtual_fs_verbatim() {
        // The contract the mock-backed dispatch tests rely on: with no
        // boundary in force the wrapper is a pure delegate, so a virtual
        // fs (no real disk behind its paths) reads byte-identically.
        let fs = MockFs::new().with_file("virtual.txt", "mock-bytes");
        let floor = FsBoundary::unbounded();
        let judged = JudgedFs::new(&fs, &floor);
        let bytes = judged.read(Path::new("virtual.txt")).await.unwrap();
        assert_eq!(&bytes[..], b"mock-bytes");
        let text = judged
            .read_to_string(Path::new("virtual.txt"))
            .await
            .unwrap();
        assert_eq!(text, "mock-bytes");
    }

    /// A unique disposable directory holding `files`, removed on drop.
    struct Disposable(PathBuf);

    impl Disposable {
        fn new(files: &[(&str, &str)]) -> Self {
            let n = SEQ.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("nika-pin-{}-{n}", std::process::id()));
            // Created exclusively and owned from here: only a root this test made is removed.
            std::fs::create_dir(&root).expect("HARNESS_INVALID: the disposable root is not ours");
            let disposable = Self(root);
            for (rel, body) in files {
                let path = disposable.0.join(rel);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, body).unwrap();
            }
            disposable
        }

        fn join(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
        }

        fn room(&self) -> RootedFs {
            RootedFs::new(
                OwnedDir::open(&self.0).unwrap(),
                EffectLedger::new(RoomLimits::new(1 << 20, 64)),
            )
        }
    }

    impl Drop for Disposable {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A virtual backend: no real disk behind its paths, and no pin.
    struct VirtualFs;

    impl FsReadDyn for VirtualFs {
        async fn read(&self, _: &Path) -> Result<Bytes, FsError> {
            Ok(Bytes::from_static(b"virtual-bytes"))
        }

        async fn read_to_string(&self, _: &Path) -> Result<String, FsError> {
            Ok("virtual-bytes".to_owned())
        }

        async fn exists(&self, _: &Path) -> bool {
            true
        }

        async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
            Ok(path.to_path_buf())
        }
    }

    #[tokio::test]
    async fn a_backend_without_a_pin_refuses_a_declared_read_typed() {
        let fs = VirtualFs;
        let boundary = FsBoundary::declared(vec!["/nika-virtual-room/**".to_owned()], vec![]);
        let refused = JudgedFs::new(&fs, &boundary)
            .read(Path::new("/nika-virtual-room/data.txt"))
            .await;
        assert!(
            matches!(refused, Err(FsError::PinUnavailable { .. })),
            "the judge never opens a host path behind the backend: {refused:?}"
        );
    }

    #[tokio::test]
    async fn a_declared_read_over_the_room_serves_the_room_bytes() {
        let scratch = Disposable::new(&[("data/sales.csv", "a,1\n")]);
        let room = scratch.room();
        let boundary = FsBoundary::declared(vec!["./data/**".to_owned()], vec![]);
        let bytes = JudgedFs::new(&room, &boundary)
            .read(Path::new("./data/sales.csv"))
            .await;
        assert!(
            matches!(&bytes, Ok(served) if &served[..] == b"a,1\n"),
            "{bytes:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_symlinked_component_inside_the_room_is_a_coded_refusal() {
        let scratch = Disposable::new(&[("data/real.txt", "inside")]);
        let outside = Disposable::new(&[("secret.txt", "secret-bytes")]);
        std::os::unix::fs::symlink(&outside.0, scratch.join("data/linkdir")).unwrap();
        let room = scratch.room();
        let boundary = FsBoundary::declared(vec!["./data/**".to_owned()], vec![]);
        let refused = JudgedFs::new(&room, &boundary)
            .read(Path::new("./data/linkdir/secret.txt"))
            .await;
        let text = format!("{refused:?}");
        assert!(refused.is_err() && text.contains(SEC_DENIED), "{text}");
        assert!(!text.contains("secret-bytes"), "{text}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_host_pin_still_follows_an_inside_ancestor_symlink_as_stated() {
        let scratch = Disposable::new(&[("allowed/real/a.txt", "through-the-ancestor")]);
        std::os::unix::fs::symlink(
            scratch.join("allowed/real"),
            scratch.join("allowed/linkdir"),
        )
        .unwrap();
        let boundary =
            FsBoundary::declared(vec![format!("{}/allowed/**", scratch.0.display())], vec![]);
        let bytes = JudgedFs::new(&TokioFs, &boundary)
            .read(&scratch.join("allowed/linkdir/a.txt"))
            .await
            .unwrap();
        assert_eq!(&bytes[..], b"through-the-ancestor");
    }
}
