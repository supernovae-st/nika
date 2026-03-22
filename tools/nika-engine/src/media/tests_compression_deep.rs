//! Deep integration tests for CAS compression (media-compression feature).
//!
//! Categories:
//! 1. CAS API Contract — store/read roundtrip for every data type
//! 2. Framing Byte Correctness — CAS marker vs raw zstd detection
//! 3. Media Tool Integration — decode_image_safe, serde_json, provenance
//! 4. Edge Cases — threshold boundaries, corrupt files, races
//!
//! Run:
//!   cargo test --lib --features media-compression -q -- tests_compression_deep
//!   cargo test --lib --features "media-compression,media-provenance" -q -- tests_compression_deep

#![cfg(feature = "media-compression")]

use crate::media::CasStore;
use std::sync::Arc;

/// CAS-internal compression marker byte (must match store.rs CAS_ZSTD_MARKER).
const CAS_ZSTD_MARKER: u8 = 0x01;

/// Zstd magic bytes (must match store.rs ZSTD_MAGIC).
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// Hash prefix used by CAS store.
const HASH_PREFIX: &str = "blake3:";

/// Strip the blake3: prefix from a hash, returning the raw hex.
fn strip_prefix(hash: &str) -> &str {
    hash.strip_prefix(HASH_PREFIX).unwrap_or(hash)
}

/// Derive the on-disk path for a CAS hash within a tempdir root.
fn cas_path(root: &std::path::Path, hash: &str) -> std::path::PathBuf {
    let raw = strip_prefix(hash);
    root.join(&raw[..2]).join(&raw[2..])
}

// ═══════════════════════════════════════════════════════════════════
// 1. CAS API Contract Tests (10 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn contract_json_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let json = br#"{"name":"test","items":[1,2,3,4,5],"nested":{"a":"b","c":"d","e":"f","g":"h","i":"j"}}"#;
    let result = store.store(json.as_slice()).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();

    assert_eq!(
        read_back.as_slice(),
        json.as_slice(),
        "store(json) -> read(hash) must return the EXACT original json bytes"
    );
    assert_eq!(
        result.size,
        json.len() as u64,
        "StoreResult.size must be the original data length, not compressed"
    );
}

#[tokio::test]
async fn contract_png_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Minimal PNG: magic bytes + padding (already compressed format)
    let mut png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend_from_slice(&[0u8; 100]);

    let result = store.store(&png).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();

    assert_eq!(
        read_back, png,
        "PNG (already-compressed) must round-trip unchanged"
    );
}

#[tokio::test]
async fn contract_svg_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // SVG is text-based and highly compressible
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="red"/></svg>"#;
    let result = store.store(svg.as_slice()).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();

    assert_eq!(
        read_back.as_slice(),
        svg.as_slice(),
        "SVG must round-trip unchanged (transparent decompression)"
    );

    // Verify it was actually compressed on disk
    let path = cas_path(dir.path(), &result.hash);
    let on_disk = tokio::fs::read(&path).await.unwrap();
    assert_eq!(
        on_disk[0], CAS_ZSTD_MARKER,
        "SVG should be compressed on disk (CAS marker prefix)"
    );
    assert!(
        on_disk.len() < svg.len(),
        "compressed SVG on disk ({}) should be smaller than original ({})",
        on_disk.len(),
        svg.len()
    );
}

#[tokio::test]
async fn contract_yaml_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let yaml = b"name: nika\nversion: 0.33.0\ntasks:\n  - id: gen_img\n    infer:\n      model: gpt-4o\n      prompt: generate a cat image\n";
    let result = store.store(yaml.as_slice()).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();

    assert_eq!(
        read_back.as_slice(),
        yaml.as_slice(),
        "YAML must round-trip unchanged"
    );
}

#[tokio::test]
async fn contract_incompressible_data() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Deterministic pseudo-random data that won't compress well
    let mut data = Vec::with_capacity(200);
    let mut state: u32 = 0xDEAD_BEEF;
    for _ in 0..200 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        data.push((state >> 16) as u8);
    }

    let result = store.store(&data).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();

    assert_eq!(
        read_back, data,
        "incompressible data must round-trip unchanged"
    );
    assert_eq!(result.size, data.len() as u64);
}

#[tokio::test]
async fn contract_large_text_compressed_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // 50KB+ of highly compressible text
    let text: Vec<u8> = "the quick brown fox jumps over the lazy dog. "
        .repeat(1200)
        .into_bytes();
    assert!(
        text.len() > 50_000,
        "fixture must be > 50KB, got {}",
        text.len()
    );

    let result = store.store(&text).await.unwrap();

    // Verify round-trip
    let read_back = store.read(&result.hash).await.unwrap();
    assert_eq!(read_back, text);

    // Verify on-disk is actually compressed
    let path = cas_path(dir.path(), &result.hash);
    let on_disk_meta = tokio::fs::metadata(&path).await.unwrap();
    assert!(
        on_disk_meta.len() < text.len() as u64,
        "on-disk size ({}) must be < original size ({}) for compressible data",
        on_disk_meta.len(),
        text.len()
    );
}

#[tokio::test]
async fn contract_raw_zstd_magic_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Pre-compress data so it has raw zstd magic bytes [0x28, 0xB5, 0x2F, 0xFD]
    let original =
        b"this data is pre-compressed by the user, not CAS, and must round-trip exactly!!!!!";
    let raw_zstd = zstd::encode_all(std::io::Cursor::new(original.as_slice()), 3).unwrap();
    assert!(
        raw_zstd.len() >= 4 && raw_zstd[..4] == ZSTD_MAGIC,
        "fixture must start with zstd magic bytes"
    );

    let result = store.store(&raw_zstd).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();

    // CRITICAL: raw zstd data must come back EXACTLY as stored,
    // NOT decompressed. CAS only decompresses data with its own marker.
    assert_eq!(
        read_back, raw_zstd,
        "raw zstd data must round-trip EXACTLY (not decompressed)"
    );
}

#[tokio::test]
async fn contract_list_reports_on_disk_size() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Store compressible text
    let text: Vec<u8> = "repeated content for size check ".repeat(100).into_bytes();
    let result = store.store(&text).await.unwrap();

    // StoreResult.size must be original
    assert_eq!(
        result.size,
        text.len() as u64,
        "StoreResult.size must be original size"
    );

    // CasEntry.size in list() reports on-disk size (compressed).
    // This is expected behavior -- list() uses metadata().len() which
    // reflects the physical file size. This documents the contract.
    let entries = store.list();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.hash, result.hash);

    let path = cas_path(dir.path(), &result.hash);
    let on_disk_len = tokio::fs::metadata(&path).await.unwrap().len();
    assert_eq!(
        entry.size, on_disk_len,
        "CasEntry.size should be on-disk size (physical)"
    );
}

#[tokio::test]
async fn contract_dedup_with_compression() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let data: Vec<u8> = "dedup test data with compression enabled "
        .repeat(20)
        .into_bytes();
    let r1 = store.store(&data).await.unwrap();
    let r2 = store.store(&data).await.unwrap();

    assert_eq!(r1.hash, r2.hash, "same data must produce same hash");
    assert!(!r1.deduplicated, "first store must not be dedup");
    assert!(r2.deduplicated, "second store must detect dedup hit");

    // Read still works after dedup
    let read_back = store.read(&r1.hash).await.unwrap();
    assert_eq!(read_back, data);
}

#[tokio::test]
async fn contract_concurrent_compressible_stores() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(CasStore::new(dir.path()));

    let data: Vec<u8> = "concurrent compression contract test "
        .repeat(50)
        .into_bytes();

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let s = Arc::clone(&store);
            let d = data.clone();
            tokio::spawn(async move { s.store(&d).await })
        })
        .collect();

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|h| h.unwrap().unwrap())
        .collect();

    // All hashes must match
    let hash = &results[0].hash;
    assert!(
        results.iter().all(|r| &r.hash == hash),
        "all concurrent stores of same data must produce same hash"
    );

    // Exactly one non-dedup
    let non_dedup = results.iter().filter(|r| !r.deduplicated).count();
    assert_eq!(non_dedup, 1, "exactly one writer should be non-dedup");

    // Every reader gets correct content
    for _ in 0..5 {
        let read_back = store.read(hash).await.unwrap();
        assert_eq!(read_back, data, "concurrent read must return original data");
    }
}

// ═══════════════════════════════════════════════════════════════════
// 2. Framing Byte Correctness (6 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn framing_cas_compressed_starts_with_marker_then_zstd() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Highly compressible text
    let text: Vec<u8> = "framing byte test ".repeat(100).into_bytes();
    let result = store.store(&text).await.unwrap();

    let path = cas_path(dir.path(), &result.hash);
    let on_disk = tokio::fs::read(&path).await.unwrap();

    // CAS-compressed: [0x01][0x28][0xB5][0x2F][0xFD]...
    assert!(
        on_disk.len() >= 5,
        "compressed data must be at least 5 bytes"
    );
    assert_eq!(on_disk[0], 0x01, "CAS marker byte must be 0x01");
    assert_eq!(
        &on_disk[1..5],
        &[0x28, 0xB5, 0x2F, 0xFD],
        "bytes 1..5 must be zstd magic: [0x28, 0xB5, 0x2F, 0xFD]"
    );
}

/// CAS-internal no-compression marker (must match store.rs CAS_NO_COMPRESSION_MARKER).
const CAS_NO_COMPRESSION_MARKER: u8 = 0x00;

#[tokio::test]
async fn framing_raw_zstd_has_no_compression_marker() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Pre-compressed zstd data (raw, should NOT be CAS-compressed)
    let original = b"raw zstd framing test data that exceeds the sixty-four byte threshold for compression!!!!!!!!";
    let raw_zstd = zstd::encode_all(std::io::Cursor::new(original.as_slice()), 3).unwrap();

    let result = store.store(&raw_zstd).await.unwrap();

    // On disk: [0x00][zstd-data] (no-compression marker + original zstd bytes)
    // should_compress returns false for data starting with zstd magic,
    // so store() uses frame_uncompressed() which prepends 0x00.
    let path = cas_path(dir.path(), &result.hash);
    let on_disk = tokio::fs::read(&path).await.unwrap();

    assert!(on_disk.len() >= 5);
    assert_eq!(
        on_disk[0], CAS_NO_COMPRESSION_MARKER,
        "raw zstd on disk must start with no-compression marker 0x00"
    );
    assert_eq!(
        &on_disk[1..5],
        &ZSTD_MAGIC,
        "raw zstd data follows after the no-compression marker"
    );
    assert_ne!(
        on_disk[0], CAS_ZSTD_MARKER,
        "raw zstd on disk must NOT start with CAS compression marker 0x01"
    );
}

#[tokio::test]
async fn framing_raw_zstd_not_decompressed_on_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let original = b"this content was pre-compressed outside of CAS and should be returned as raw zstd bytes, not decompressed";
    let raw_zstd = zstd::encode_all(std::io::Cursor::new(original.as_slice()), 3).unwrap();

    let result = store.store(&raw_zstd).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();

    // Must get back the raw zstd bytes, NOT the decompressed original
    assert_eq!(
        read_back, raw_zstd,
        "read() must return raw zstd bytes, NOT decompressed data"
    );
    assert_ne!(
        read_back.as_slice(),
        original.as_slice(),
        "read() must NOT return the decompressed content"
    );
}

#[tokio::test]
async fn framing_compressed_text_has_cas_marker_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let text: Vec<u8> = "marker verification test ".repeat(50).into_bytes();
    let result = store.store(&text).await.unwrap();

    let path = cas_path(dir.path(), &result.hash);
    let on_disk = tokio::fs::read(&path).await.unwrap();

    assert_eq!(
        on_disk[0], 0x01,
        "CAS-compressed text must start with 0x01 marker on disk"
    );
}

#[tokio::test]
async fn framing_data_starts_with_0x01_but_not_zstd() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Data that starts with 0x01 but is NOT followed by zstd magic.
    // transparent_decompress checks data[0]==0x01 AND data[1..5]==ZSTD_MAGIC,
    // so this data will pass through unchanged.
    let mut data = vec![0x01u8, 0x00, 0x00, 0x00, 0x00];
    data.extend_from_slice(&[0xAB; 100]);

    let result = store.store(&data).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();

    assert_eq!(
        read_back, data,
        "data starting with 0x01 but without zstd magic must round-trip as-is"
    );
}

#[tokio::test]
async fn framing_data_mimicking_cas_header_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Data that starts with [0x01][0x28][0xB5][0x2F][0xFD] (CAS marker + zstd magic)
    // but the rest is garbage. should_compress sees data[0..4] = [0x01, 0x28, 0xB5, 0x2F]
    // which is NOT the zstd magic (that would be [0x28, 0xB5, 0x2F, 0xFD]).
    // So it will attempt compression. With enough repeated padding, compression IS
    // beneficial, so on disk: [0x01][zstd-of-our-data]. On read, transparent_decompress
    // decompresses back to our original data.
    let mut data = vec![0x01, 0x28, 0xB5, 0x2F, 0xFD];
    data.extend_from_slice(&[0xAB; 200]); // compressible padding

    let result = store.store(&data).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();

    assert_eq!(
        read_back, data,
        "data mimicking CAS marker+zstd header must round-trip correctly"
    );
}

// ═══════════════════════════════════════════════════════════════════
// 3. Media Tool Integration with Compression (3-5 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn integration_png_via_context_read() {
    use crate::runtime::builtin::media::context::MediaToolContext;

    let dir = tempfile::tempdir().unwrap();
    let ctx = MediaToolContext::new(CasStore::new(dir.path()));

    let png = minimal_png();

    let sr = ctx.store_media(&png, "test").await.unwrap();
    let read_back = ctx.read_media(&sr.hash).await.unwrap();

    assert_eq!(
        &read_back[..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "read_media must return valid PNG magic bytes"
    );
    assert_eq!(
        read_back, png,
        "read_media must return the exact original PNG"
    );
}

#[cfg(any(
    feature = "media-thumbnail",
    feature = "media-svg",
    feature = "media-phash",
    feature = "media-qr",
    feature = "media-iqa",
))]
#[tokio::test]
async fn integration_png_decode_image_safe() {
    use crate::runtime::builtin::media::safety::decode_image_safe;

    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let png = minimal_png();
    let sr = store.store(&png).await.unwrap();
    let read_back = store.read(&sr.hash).await.unwrap();

    let img =
        decode_image_safe(&read_back).expect("decode_image_safe must succeed on CAS-read PNG");
    assert!(img.width() > 0 && img.height() > 0);
}

#[tokio::test]
async fn integration_json_serde_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let json_data = serde_json::json!({
        "name": "nika",
        "version": "0.33.0",
        "features": ["media-compression", "media-thumbnail"],
        "config": {"max_size": 100, "enabled": true}
    });
    let json_bytes = serde_json::to_vec(&json_data).unwrap();

    let sr = store.store(&json_bytes).await.unwrap();
    let read_back = store.read(&sr.hash).await.unwrap();

    let parsed: serde_json::Value = serde_json::from_slice(&read_back)
        .expect("serde_json::from_slice must work on CAS-read JSON");
    assert_eq!(
        parsed, json_data,
        "JSON parsed from CAS read must match original"
    );
}

#[cfg(feature = "media-provenance")]
#[tokio::test]
async fn integration_jpeg_provenance_sign_verify() {
    use crate::runtime::builtin::media::context::MediaToolContext;
    use crate::runtime::builtin::media::provenance::ProvenanceOp;
    use crate::runtime::builtin::media::{MediaOp, MediaOpResult};

    let dir = tempfile::tempdir().unwrap();
    let ctx = MediaToolContext::new(CasStore::new(dir.path()));

    let jpeg = fixture_jpeg();
    let sr = ctx.store_media(&jpeg, "test").await.unwrap();

    let sign_result = ProvenanceOp
        .execute(
            serde_json::json!({
                "hash": sr.hash,
                "assertion": "ai.generated"
            }),
            &ctx,
        )
        .await
        .expect("provenance signing should succeed with compression enabled");

    match sign_result {
        MediaOpResult::Binary {
            data, mime_type, ..
        } => {
            assert!(!data.is_empty(), "signed data must not be empty");
            assert!(
                mime_type.contains("jpeg") || mime_type.contains("jpg"),
                "signed output must be JPEG, got: {}",
                mime_type
            );
            assert!(
                data.len() > jpeg.len(),
                "signed JPEG should be larger than original due to C2PA manifest"
            );

            // Store the signed data and verify it round-trips through CAS
            let sr2 = ctx.store_media(&data, "test_signed").await.unwrap();
            let read_signed = ctx.read_media(&sr2.hash).await.unwrap();
            assert_eq!(
                read_signed, data,
                "signed JPEG must round-trip through CAS correctly"
            );
        }
        other => panic!("expected Binary result from provenance, got: {:?}", other),
    }
}

#[cfg(feature = "media-provenance")]
#[tokio::test]
async fn integration_png_provenance_sign_with_compression() {
    use crate::runtime::builtin::media::context::MediaToolContext;
    use crate::runtime::builtin::media::provenance::ProvenanceOp;
    use crate::runtime::builtin::media::{MediaOp, MediaOpResult};

    let dir = tempfile::tempdir().unwrap();
    let ctx = MediaToolContext::new(CasStore::new(dir.path()));

    let png = fixture_png();
    let sr = ctx.store_media(&png, "test_png").await.unwrap();

    let sign_result = ProvenanceOp
        .execute(
            serde_json::json!({
                "hash": sr.hash,
                "assertion": "human.created"
            }),
            &ctx,
        )
        .await
        .expect("provenance signing PNG should succeed with compression enabled");

    match sign_result {
        MediaOpResult::Binary {
            data, mime_type, ..
        } => {
            assert!(!data.is_empty(), "signed PNG data must not be empty");
            assert!(
                mime_type.contains("png"),
                "signed output must be PNG, got: {}",
                mime_type
            );

            let sr2 = ctx.store_media(&data, "test_signed_png").await.unwrap();
            let read_back = ctx.read_media(&sr2.hash).await.unwrap();
            assert_eq!(
                read_back, data,
                "signed PNG must round-trip through CAS with compression"
            );
        }
        other => panic!("expected Binary result from provenance, got: {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════════
// 4. Edge Cases (7 tests)
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn edge_below_compression_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // 63 bytes: below the 64-byte threshold
    let data = vec![0x41u8; 63];

    let result = store.store(&data).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();
    assert_eq!(read_back, data, "data below threshold must round-trip");

    // Should NOT be compressed (too small), but gets no-compression framing marker
    let path = cas_path(dir.path(), &result.hash);
    let on_disk = tokio::fs::read(&path).await.unwrap();
    assert_eq!(
        on_disk[0], CAS_NO_COMPRESSION_MARKER,
        "data below 64-byte threshold must have no-compression marker"
    );
    assert_ne!(
        on_disk[0], CAS_ZSTD_MARKER,
        "data below 64-byte threshold must NOT be compressed"
    );
    assert_eq!(
        on_disk.len(),
        64,
        "on-disk size must be original + 1 marker byte for uncompressed data"
    );
    assert_eq!(
        &on_disk[1..],
        &data[..],
        "on-disk data after marker must be original bytes"
    );
}

#[tokio::test]
async fn edge_exactly_at_compression_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // 64 bytes: exactly at the threshold (should_compress returns true if >= 64)
    let data = vec![0x41u8; 64];

    let result = store.store(&data).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();
    assert_eq!(
        read_back, data,
        "data at threshold must round-trip regardless of compression decision"
    );
}

#[tokio::test]
async fn edge_compression_makes_data_larger() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // High-entropy data where zstd compression makes it larger.
    // compress_if_beneficial detects this and stores with no-compression marker.
    let mut data = Vec::with_capacity(64);
    let mut state: u32 = 0xCAFE_BABE;
    for _ in 0..64 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        data.push((state >> 16) as u8);
    }

    let result = store.store(&data).await.unwrap();
    let read_back = store.read(&result.hash).await.unwrap();
    assert_eq!(
        read_back, data,
        "data where compression inflates must still round-trip correctly"
    );

    // When compression makes data larger, compress_if_beneficial stores with
    // no-compression marker [0x00][raw-data]. The on-disk file is 1 byte
    // larger than the original.
    let path = cas_path(dir.path(), &result.hash);
    let on_disk = tokio::fs::read(&path).await.unwrap();
    assert_eq!(
        on_disk[0], CAS_NO_COMPRESSION_MARKER,
        "when compression inflates, data should have no-compression marker"
    );
    assert_eq!(
        &on_disk[1..],
        &data[..],
        "original data follows the no-compression marker"
    );
}

#[tokio::test]
async fn edge_corrupt_cas_file_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Store valid data first to get a real hash/path
    let data = b"valid data for corruption test";
    let result = store.store(data.as_slice()).await.unwrap();

    // Corrupt the on-disk file: write bytes that look like CAS-compressed
    // but contain invalid zstd payload
    let path = cas_path(dir.path(), &result.hash);
    let garbage = vec![
        CAS_ZSTD_MARKER, 0x28, 0xB5, 0x2F, 0xFD,
        0xFF, 0xFF, 0xFF, 0xFF, 0x00,
    ];
    tokio::fs::write(&path, &garbage).await.unwrap();

    // Read should return an error (corrupt zstd decompression), not panic
    let read_result = store.read(&result.hash).await;
    assert!(
        read_result.is_err(),
        "reading corrupt CAS file must return error, not panic"
    );
}

#[tokio::test]
async fn edge_store_then_immediate_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    // Sequential store + immediate read
    let data: Vec<u8> = "immediate read test ".repeat(50).into_bytes();
    let sr = store.store(&data).await.unwrap();
    let read_back = store.read(&sr.hash).await.unwrap();
    assert_eq!(
        read_back, data,
        "immediate read after store must return correct data"
    );
}

#[tokio::test]
async fn edge_multiple_data_types_same_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let json: &[u8] = br#"{"key": "value", "number": 42, "array": [1,2,3,4,5,6,7,8,9,10]}"#;
    let yaml: &[u8] =
        b"key: value\nnumber: 42\narray:\n  - 1\n  - 2\n  - 3\n  - 4\n  - 5\n  - 6\n  - 7\n";
    let svg: &[u8] =
        br#"<svg xmlns="http://www.w3.org/2000/svg"><circle cx="50" cy="50" r="40" fill="blue"/></svg>"#;
    let mut png_vec = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    png_vec.extend_from_slice(&[0u8; 100]);
    let binary: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE];

    let datasets: Vec<(&[u8], &str)> = vec![
        (json, "JSON"),
        (yaml, "YAML"),
        (svg, "SVG"),
        (&png_vec, "PNG"),
        (binary, "binary"),
    ];

    let mut hashes = Vec::new();
    for &(data, _label) in &datasets {
        let sr = store.store(data).await.unwrap();
        hashes.push(sr.hash.clone());
    }

    for (hash, &(data, label)) in hashes.iter().zip(datasets.iter()) {
        let read_back = store.read(hash).await.unwrap();
        assert_eq!(
            read_back.as_slice(),
            data,
            "{label} data did not round-trip correctly through CAS with compression"
        );
    }
}

#[tokio::test]
async fn edge_corrupt_marker_no_payload() {
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let data = b"some valid data here";
    let result = store.store(data.as_slice()).await.unwrap();

    // Overwrite with just the marker+magic but no actual zstd frame data
    let path = cas_path(dir.path(), &result.hash);
    let fake = vec![CAS_ZSTD_MARKER, 0x28, 0xB5, 0x2F, 0xFD];
    tokio::fs::write(&path, &fake).await.unwrap();

    let read_result = store.read(&result.hash).await;
    assert!(
        read_result.is_err(),
        "CAS marker + zstd magic with no payload must error on read, not panic"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 21 FIXED: CasStore::read_raw decompresses before artifact copy
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cas_read_raw_decompresses_for_artifact_copy() {
    // Verifies Bug 21 fix: CasStore::read_raw() returns original data,
    // not the on-disk framed/compressed bytes. This is what the artifact
    // writer now uses instead of a raw file copy.
    let dir = tempfile::tempdir().unwrap();
    let store = CasStore::new(dir.path());

    let text: Vec<u8> = "read_raw decompression test "
        .repeat(100)
        .into_bytes();
    let result = store.store(&text).await.unwrap();

    // Read via CAS API -- decompresses transparently
    let via_api = store.read(&result.hash).await.unwrap();
    assert_eq!(via_api, text, "CAS API read must return original data");

    // Read via read_raw (used by artifact writer) -- also decompresses
    let path = cas_path(dir.path(), &result.hash);
    let via_read_raw = CasStore::read_raw(&path).await.unwrap();
    assert_eq!(
        via_read_raw, text,
        "read_raw must return original data (not compressed framing bytes)"
    );

    // Raw on-disk bytes ARE different (compressed + framed)
    let raw_on_disk = tokio::fs::read(&path).await.unwrap();
    assert_ne!(
        raw_on_disk, text,
        "on-disk bytes should be framed/compressed"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

/// Create a minimal valid 1x1 white PNG (in-memory, no `image` crate needed).
fn minimal_png() -> Vec<u8> {
    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    let ihdr_data = [
        0x00, 0x00, 0x00, 0x01, // width=1
        0x00, 0x00, 0x00, 0x01, // height=1
        0x08, // bit depth = 8
        0x02, // color type = RGB
        0x00, // compression = deflate
        0x00, // filter = adaptive
        0x00, // interlace = none
    ];
    write_png_chunk(&mut png, b"IHDR", &ihdr_data);
    let raw_scanline = [0x00, 0xFF, 0xFF, 0xFF]; // filter byte + RGB
    let mut deflated = Vec::new();
    {
        use std::io::Write;
        let mut encoder =
            flate2::write::ZlibEncoder::new(&mut deflated, flate2::Compression::default());
        encoder.write_all(&raw_scanline).unwrap();
        encoder.finish().unwrap();
    }
    write_png_chunk(&mut png, b"IDAT", &deflated);
    write_png_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_png_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(chunk_type);
    buf.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(chunk_type);
    crc_input.extend_from_slice(data);
    let crc = crc32_png(&crc_input);
    buf.extend_from_slice(&crc.to_be_bytes());
}

fn crc32_png(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(feature = "media-provenance")]
fn fixture_jpeg() -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let img = ImageBuffer::from_pixel(4, 4, Rgb([255u8, 0, 0]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
    buf.into_inner()
}

#[cfg(feature = "media-provenance")]
fn fixture_png() -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let img = ImageBuffer::from_pixel(8, 8, Rgb([0u8, 128, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}
