// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Contract tests for `nika-blob` — the production `DiskBlobStore`.
//!
//! CAS semantics, sharded layout, atomic writes, the sidecar mime
//! round-trip, dedup, and the size cap — all against real tempdirs.

use bytes::Bytes;
use nika_blob::DiskBlobStore;
use nika_kernel::BlobError;
use nika_kernel::blob::BlobStoreDyn;
use tempfile::TempDir;

fn store() -> (TempDir, DiskBlobStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = DiskBlobStore::new(dir.path());
    (dir, store)
}

// ─── type-level guarantees ───────────────────────────────────────────

#[test]
fn diskblobstore_satisfies_blanket_and_dyn() {
    fn accepts_store<T: nika_kernel::BlobStore>(_: &T) {}
    fn accepts_dyn<T: BlobStoreDyn>(_: &T) {}
    let (_d, s) = store();
    accepts_store(&s);
    accepts_dyn(&s);
}

#[test]
fn diskblobstore_is_clone() {
    fn assert_clone<T: Clone>() {}
    assert_clone::<DiskBlobStore>();
}

// ─── put · hashing + metadata ────────────────────────────────────────

#[tokio::test]
async fn put_returns_blake3_prefixed_hash_and_size() {
    let (_d, s) = store();
    let meta = s
        .put(Bytes::from_static(b"hello"), "text/plain")
        .await
        .unwrap();
    assert!(
        meta.hash.starts_with("blake3:"),
        "hash must be prefixed: {}",
        meta.hash
    );
    assert_eq!(meta.size, 5);
    assert_eq!(meta.mime_type, "text/plain");
    // blake3("hello") is a stable known vector.
    assert_eq!(
        meta.hash,
        "blake3:ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f"
    );
}

#[tokio::test]
async fn put_is_content_addressed_same_bytes_same_hash() {
    let (_d, s) = store();
    let a = s
        .put(Bytes::from_static(b"identical"), "text/plain")
        .await
        .unwrap();
    let b = s
        .put(Bytes::from_static(b"identical"), "application/octet-stream")
        .await
        .unwrap();
    assert_eq!(a.hash, b.hash, "identical bytes → identical hash (CAS)");
}

#[tokio::test]
async fn put_different_bytes_different_hash() {
    let (_d, s) = store();
    let a = s
        .put(Bytes::from_static(b"alpha"), "text/plain")
        .await
        .unwrap();
    let b = s
        .put(Bytes::from_static(b"beta"), "text/plain")
        .await
        .unwrap();
    assert_ne!(a.hash, b.hash);
}

#[tokio::test]
async fn put_empty_blob_rejected() {
    let (_d, s) = store();
    let err = s.put(Bytes::new(), "text/plain").await.unwrap_err();
    assert!(matches!(err, BlobError::Io { .. }), "got {err:?}");
}

#[tokio::test]
async fn put_dedup_is_idempotent() {
    let (_d, s) = store();
    let first = s
        .put(Bytes::from_static(b"dup"), "text/plain")
        .await
        .unwrap();
    // Second put of identical content must succeed (dedup) and return
    // the same hash + size.
    let second = s
        .put(Bytes::from_static(b"dup"), "text/plain")
        .await
        .unwrap();
    assert_eq!(first.hash, second.hash);
    assert_eq!(first.size, second.size);
    assert!(s.exists(&first.hash).await);
}

#[tokio::test]
async fn put_leaves_no_temp_residue() {
    let (dir, s) = store();
    s.put(Bytes::from_static(b"clean"), "text/plain")
        .await
        .unwrap();
    // Walk the store: only the 2-char shard dirs + blob/sidecar, never
    // a *.nika-tmp file.
    let mut stack = vec![dir.path().to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&d).await.unwrap();
        while let Some(e) = rd.next_entry().await.unwrap() {
            let p = e.path();
            assert!(
                !p.to_string_lossy().contains("nika-tmp"),
                "atomic put must leave no temp residue: {p:?}"
            );
            if e.file_type().await.unwrap().is_dir() {
                stack.push(p);
            }
        }
    }
}

// ─── get ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_roundtrips_bytes() {
    let (_d, s) = store();
    let data = Bytes::from_static(&[0u8, 1, 2, 250, 255]);
    let meta = s
        .put(data.clone(), "application/octet-stream")
        .await
        .unwrap();
    let back = s.get(&meta.hash).await.unwrap();
    assert_eq!(back, data);
}

#[tokio::test]
async fn get_missing_is_not_found() {
    let (_d, s) = store();
    let err = s.get("blake3:deadbeef").await.unwrap_err();
    assert!(matches!(err, BlobError::NotFound { .. }), "got {err:?}");
}

#[tokio::test]
async fn get_accepts_hash_with_or_without_prefix() {
    let (_d, s) = store();
    let meta = s
        .put(Bytes::from_static(b"prefixless"), "text/plain")
        .await
        .unwrap();
    let raw = meta.hash.strip_prefix("blake3:").unwrap();
    // Both spellings address the same blob.
    assert_eq!(s.get(&meta.hash).await.unwrap(), s.get(raw).await.unwrap());
}

// ─── exists ──────────────────────────────────────────────────────────

#[tokio::test]
async fn exists_true_after_put_false_otherwise() {
    let (_d, s) = store();
    assert!(!s.exists("blake3:nope").await);
    let meta = s.put(Bytes::from_static(b"x"), "text/plain").await.unwrap();
    assert!(s.exists(&meta.hash).await);
}

// ─── stat · the sidecar mime round-trip (the Diamond fix) ────────────

#[tokio::test]
async fn stat_returns_the_declared_mime_not_a_guess() {
    let (_d, s) = store();
    // A text/plain blob with no magic bytes — brouillon's content-sniff
    // stat would have said application/octet-stream. The sidecar makes
    // put/stat agree.
    let meta = s
        .put(Bytes::from_static(b"plain words"), "text/markdown")
        .await
        .unwrap();
    let stat = s.stat(&meta.hash).await.unwrap();
    assert_eq!(
        stat.mime_type, "text/markdown",
        "stat must echo the declared mime"
    );
    assert_eq!(stat.size, 11);
    assert_eq!(stat.hash, meta.hash);
}

#[tokio::test]
async fn stat_accepts_prefixless_and_uppercase_hash() {
    let (_d, s) = store();
    let meta = s
        .put(Bytes::from_static(b"casecheck"), "text/markdown")
        .await
        .unwrap();
    let raw = meta.hash.strip_prefix("blake3:").unwrap();
    // prefixless raw hex resolves + returns the canonical prefixed hash.
    let by_raw = s.stat(raw).await.unwrap();
    assert_eq!(by_raw.hash, meta.hash);
    assert_eq!(by_raw.mime_type, "text/markdown");
    // UPPERCASE hex resolves too (canonical_raw lowercases) — was a
    // silent NotFound footgun before the validation refactor.
    let by_upper = s.stat(&meta.hash.to_uppercase()).await.unwrap();
    assert_eq!(by_upper.hash, meta.hash);
}

#[tokio::test]
async fn get_accepts_uppercase_hash() {
    let (_d, s) = store();
    let meta = s
        .put(Bytes::from_static(b"UP"), "text/plain")
        .await
        .unwrap();
    let upper = meta.hash.to_uppercase();
    assert_eq!(s.get(&upper).await.unwrap(), Bytes::from_static(b"UP"));
    assert!(s.exists(&upper).await);
}

#[tokio::test]
async fn put_rejects_blank_mime() {
    let (_d, s) = store();
    let err = s.put(Bytes::from_static(b"x"), "   ").await.unwrap_err();
    assert!(
        matches!(err, BlobError::Io { .. }),
        "blank mime rejected, got {err:?}"
    );
}

#[tokio::test]
async fn malformed_hash_is_not_found_not_panic() {
    // The attacker-supplied-hash panic vector (non-ASCII / wrong length)
    // must surface as NotFound / false, never a slice panic.
    let (_d, s) = store();
    for bad in ["aé00", "tooshort", "", "blake3:nothex", &"z".repeat(64)] {
        assert!(
            matches!(s.get(bad).await.unwrap_err(), BlobError::NotFound { .. }),
            "get {bad:?}"
        );
        assert!(
            matches!(s.stat(bad).await.unwrap_err(), BlobError::NotFound { .. }),
            "stat {bad:?}"
        );
        assert!(
            matches!(s.delete(bad).await.unwrap_err(), BlobError::NotFound { .. }),
            "delete {bad:?}"
        );
        assert!(!s.exists(bad).await, "exists {bad:?}");
    }
}

#[tokio::test]
async fn stat_missing_is_not_found() {
    let (_d, s) = store();
    let err = s.stat("blake3:ghost").await.unwrap_err();
    assert!(matches!(err, BlobError::NotFound { .. }), "got {err:?}");
}

#[tokio::test]
async fn stat_falls_back_when_sidecar_absent() {
    // A blob written by an older/foreign writer (bytes only, no sidecar)
    // must still stat — degrading to application/octet-stream, not erroring.
    let (dir, s) = store();
    let meta = s
        .put(Bytes::from_static(b"sidecarless"), "text/plain")
        .await
        .unwrap();
    // Delete just the sidecar, keep the bytes.
    let raw = meta.hash.strip_prefix("blake3:").unwrap();
    let sidecar = dir
        .path()
        .join(&raw[..2])
        .join(format!("{}.mime", &raw[2..]));
    tokio::fs::remove_file(&sidecar).await.unwrap();
    let stat = s.stat(&meta.hash).await.unwrap();
    assert_eq!(stat.mime_type, "application/octet-stream");
    assert_eq!(stat.size, 11);
}

#[tokio::test]
async fn stat_empty_sidecar_falls_back_to_default() {
    // A sidecar that EXISTS but is blank (interrupted write / foreign
    // writer that stored an empty mime) must degrade to octet-stream,
    // not surface mime_type == "". Pins the `!s.trim().is_empty()` guard.
    let (dir, s) = store();
    let meta = s
        .put(Bytes::from_static(b"blankcar"), "text/plain")
        .await
        .unwrap();
    let raw = meta.hash.strip_prefix("blake3:").unwrap();
    let sidecar = dir
        .path()
        .join(&raw[..2])
        .join(format!("{}.mime", &raw[2..]));
    tokio::fs::write(&sidecar, b"   ").await.unwrap(); // whitespace-only
    let stat = s.stat(&meta.hash).await.unwrap();
    assert_eq!(stat.mime_type, "application/octet-stream");
}

#[cfg(unix)]
#[tokio::test]
async fn stat_surfaces_non_notfound_sidecar_error() {
    // A non-NotFound read error on the sidecar (here: it's a DIRECTORY,
    // not a file) must be SURFACED as Io, not masked into octet-stream.
    // Pins the `e.kind() == NotFound` narrowing guard (the nika-reviewer
    // P2 — a too-broad Err(_) would have swallowed this).
    let (dir, s) = store();
    let meta = s
        .put(Bytes::from_static(b"dircar"), "text/plain")
        .await
        .unwrap();
    let raw = meta.hash.strip_prefix("blake3:").unwrap();
    let sidecar = dir
        .path()
        .join(&raw[..2])
        .join(format!("{}.mime", &raw[2..]));
    tokio::fs::remove_file(&sidecar).await.unwrap();
    tokio::fs::create_dir(&sidecar).await.unwrap(); // read_to_string(dir) → non-NotFound err
    let err = s.stat(&meta.hash).await.unwrap_err();
    assert!(
        matches!(err, BlobError::Io { .. }),
        "non-NotFound sidecar error must surface, got {err:?}"
    );
}

// ─── delete ──────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_removes_blob_and_sidecar() {
    let (dir, s) = store();
    let meta = s
        .put(Bytes::from_static(b"doomed"), "text/plain")
        .await
        .unwrap();
    assert!(s.exists(&meta.hash).await);
    s.delete(&meta.hash).await.unwrap();
    assert!(!s.exists(&meta.hash).await);
    // Sidecar gone too.
    let raw = meta.hash.strip_prefix("blake3:").unwrap();
    let sidecar = dir
        .path()
        .join(&raw[..2])
        .join(format!("{}.mime", &raw[2..]));
    assert!(!tokio::fs::try_exists(&sidecar).await.unwrap());
}

#[tokio::test]
async fn delete_missing_is_not_found() {
    let (_d, s) = store();
    let err = s.delete("blake3:absent").await.unwrap_err();
    assert!(matches!(err, BlobError::NotFound { .. }), "got {err:?}");
}

#[tokio::test]
async fn delete_then_get_is_not_found() {
    let (_d, s) = store();
    let meta = s
        .put(Bytes::from_static(b"transient"), "text/plain")
        .await
        .unwrap();
    s.delete(&meta.hash).await.unwrap();
    assert!(matches!(
        s.get(&meta.hash).await.unwrap_err(),
        BlobError::NotFound { .. }
    ));
}

// ─── size cap ────────────────────────────────────────────────────────

#[tokio::test]
async fn put_over_cap_is_too_large() {
    let (dir, _s) = store();
    // 64-byte cap for the test.
    let s = DiskBlobStore::with_max_size(dir.path(), 64);
    let err = s
        .put(Bytes::from(vec![7u8; 128]), "application/octet-stream")
        .await
        .unwrap_err();
    match err {
        BlobError::TooLarge { size, max } => {
            assert_eq!(size, 128);
            assert_eq!(max, 64);
        }
        other => panic!("expected TooLarge, got {other:?}"),
    }
}

#[tokio::test]
async fn put_exactly_at_cap_succeeds() {
    let (dir, _s) = store();
    let s = DiskBlobStore::with_max_size(dir.path(), 64);
    let meta = s
        .put(Bytes::from(vec![1u8; 64]), "application/octet-stream")
        .await
        .unwrap();
    assert_eq!(meta.size, 64);
}

// ─── property: roundtrip + hash determinism ──────────────────────────

mod properties {
    use super::*;
    use proptest::prelude::*;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt")
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// Any non-empty payload survives put→get and the hash is stable
        /// across two stores (content-addressed, deterministic).
        #[test]
        fn put_get_roundtrip_and_stable_hash(data in proptest::collection::vec(any::<u8>(), 1..2048)) {
            let bytes = Bytes::from(data.clone());
            let (h1, back) = rt().block_on(async {
                let (_d, s) = store();
                let m = s.put(bytes.clone(), "application/octet-stream").await.expect("put");
                let b = s.get(&m.hash).await.expect("get");
                (m.hash, b)
            });
            prop_assert_eq!(&back[..], &data[..]);
            // Independent store, same bytes → same hash.
            let h2 = rt().block_on(async {
                let (_d, s) = store();
                s.put(bytes, "application/octet-stream").await.expect("put").hash
            });
            prop_assert_eq!(h1, h2);
        }
    }
}
