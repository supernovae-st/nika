# Research Report: Rust Crates for Media Processing Pipeline

**Date**: 2026-03-17
**Methodology**: crates.io API, GitHub release notes, README analysis
**Confidence**: High (all data from primary sources)

---

## Summary

Most planned versions are already at or very near latest. The biggest wins for pipeline robustness come from: (1) upgrading blake3 to 1.8.3 for the new `Hash::as_slice` API, (2) pairing `infer` with `file-format` for defense-in-depth MIME detection, (3) using `atomic-write-file` instead of hand-rolling atomic writes, and (4) considering `cacache` if we need a full CAS rather than building one from scratch.

---

## 1. blake3 -- Hashing for Content-Addressable Storage

| Metric | Value |
|--------|-------|
| **Planned** | 1.8 |
| **Latest** | **1.8.3** (2026-01-08) |
| **Total downloads** | 107M |
| **Recent downloads** | 20.5M/quarter |

### Changes since 1.8.0

- **1.8.3**: Added `Hash::as_slice()` -- returns `&[u8; 32]` directly, avoids `.as_bytes()` copy. MSRV bumped to 1.85 (2024 Edition). Fixed Miri failures in intrinsics (wrapping_add vs add). CPU feature detection on x86 no longer requires `std` feature.
- **1.8.2**: CMake build fixes (TBB).
- **1.8.1**: CMake transitive dependency fix.
- **1.8.0**: New `blake3::hazmat` module replacing deprecated `blake3::guts`. For advanced use cases (Bao, Iroh-style chunk-level operations).

### What 1.7.0 added (since your 1.8 plan was probably based on 1.7)

- WASM SIMD backend (`wasm32_simd` feature) -- 6x perf improvement.
- C implementation gained TBB-based multithreading.

### Alternatives considered

| Crate | Use case | Verdict |
|-------|----------|---------|
| xxhash-rust / twox-hash | Faster non-crypto hash | **Not suitable** -- not collision-resistant enough for CAS |
| seahash | Portable fast hash | **Not suitable** -- same concern |
| SHA-256 (ring/sha2) | Traditional CAS hash | **Slower** -- blake3 is ~5x faster on modern CPUs |

### Recommendation

**Use blake3 1.8.3.** It is the gold standard for CAS hashing in Rust. No better alternative exists for this use case. The new `Hash::as_slice()` is a minor ergonomic win. The `hazmat` module is useful if we ever need incremental/streaming chunk verification.

### Performance tips

- Enable the `rayon` feature for large files (multi-threaded hashing).
- For small files (<1MB), single-threaded is faster due to Rayon overhead.
- Use `blake3::Hasher::update_reader()` for streaming from `Read` implementors.
- The `Hash` type is `Copy` and 32 bytes -- store it directly, not as String.

---

## 2. infer -- MIME Type Detection by Magic Bytes

| Metric | Value |
|--------|-------|
| **Planned** | 0.19 |
| **Latest** | **0.19.0** (2025-02-02) |
| **Total downloads** | 77M |
| **Recent downloads** | 12.9M/quarter |

### Status

You are already on the latest version. The 0.16 -> 0.19 jump happened on the same day (2025-02-02), suggesting a batch release with breaking changes across versions.

### Key features of infer

- `no_std` / `no_alloc` support -- minimal footprint
- Custom matchers -- register your own magic byte patterns
- Top-level functions (`infer::get()`, `infer::get_from_path()`)
- Class-based discovery (image, video, audio, document)
- No external database dependency (no `/etc/magic`)

### Alternatives comparison

| Crate | Latest | Downloads | Approach | Accuracy |
|-------|--------|-----------|----------|----------|
| **infer** | 0.19.0 | 77M | Magic bytes, in-memory DB | Good for common formats |
| **tree_magic_mini** | 3.2.2 | 6.7M | freedesktop.org shared-mime-info tree | Better for Linux system types |
| **file-format** | 0.28.0 | 1M | Signature + format-specific readers | Best for deep detection |
| **mime_guess** | 2.0.5 | 172M | Extension-based only | N/A (no content inspection) |

### Accuracy analysis

- **infer**: Good for standard image/video/audio. Checks first few bytes. May miss container subtypes (e.g., HEIF vs AVIF in the same ISOBMFF container).
- **tree_magic_mini**: Uses freedesktop hierarchy, so `image/svg+xml` is correctly identified as both XML and image. Better for Linux-centric workflows. Heavier dependency.
- **file-format**: Most thorough -- uses format-specific readers (ZIP internals, XML parsing, MP4 atoms) when `reader-*` features are enabled. Can distinguish DOCX from XLSX (both ZIP-based). Newer, less battle-tested.

### Recommendation

**Keep infer 0.19.0 as primary** (lightweight, well-tested, huge adoption). Consider adding **file-format 0.28.0 as secondary validation** for cases where infer returns ambiguous results (especially ZIP-based and ISOBMFF-based containers). Use `file-format` with specific reader features rather than the blanket `reader` feature to keep binary size down.

```toml
infer = "0.19"
file-format = { version = "0.28", features = ["reader-mp4", "reader-zip"] }
```

---

## 3. mime_guess -- Extension-Based MIME Lookup

| Metric | Value |
|--------|-------|
| **Planned** | 2.0 |
| **Latest** | **2.0.5** (2024-06-29) |
| **Total downloads** | 172M |
| **Recent downloads** | 26.6M/quarter |

### Status

Mature and stable. Last meaningful update was 2024-06-29. This crate is extension-based only -- it does NOT inspect file contents.

### Alternatives

| Crate | Latest | Approach | Notes |
|-------|--------|----------|-------|
| **mime_guess** | 2.0.5 | File extension lookup | 172M downloads, industry standard |
| **mime** | 0.3.17 | MIME type parsing/representation | Not a lookup crate -- represents parsed MIME types. Used BY mime_guess internally |
| **new_mime_guess** | 4.0.5 | Fork of mime_guess | Updated DB, but fork maintenance uncertain |

### Important distinction

- `mime_guess` = "given filename `photo.jpg`, return `image/jpeg`" (extension-based)
- `infer` = "given bytes `[0xFF, 0xD8, ...]`, return `image/jpeg`" (content-based)
- `mime` = "parse the string `image/jpeg` into a typed `Mime` struct"

### Recommendation

**Use mime_guess 2.0.5** for extension-based lookups. It is the de facto standard. The `mime` crate (0.3.17) is complementary, not a replacement -- use it if you need to parse/compare MIME types as typed values rather than strings. For a media pipeline, you likely want both:

```toml
mime_guess = "2.0"    # extension -> MIME string
mime = "0.3"          # typed MIME comparisons
```

---

## 4. base64 -- Encoding/Decoding

| Metric | Value |
|--------|-------|
| **Planned** | 0.22 |
| **Latest** | **0.22.1** (2024-04-30) |
| **Total downloads** | 995M |
| **Recent downloads** | 162M/quarter |

### Changes in 0.22.x

- **0.22.0** (2024-03-02): Major API overhaul with the **Engine** abstraction. This was the breaking change from 0.21.x.
- **0.22.1** (2024-04-30): Bug fixes and polish.

### Engine API (introduced in 0.22)

The 0.22 API uses `Engine` trait for configurable encoding:

```rust
use base64::{Engine as _, engine::general_purpose};

// Standard encoding
let encoded = general_purpose::STANDARD.encode(b"hello");
let decoded = general_purpose::STANDARD.decode(&encoded)?;

// URL-safe, no padding
let encoded = general_purpose::URL_SAFE_NO_PAD.encode(b"hello");
```

Key engines:
- `general_purpose::STANDARD` -- RFC 4648 standard
- `general_purpose::STANDARD_NO_PAD` -- Standard without `=` padding
- `general_purpose::URL_SAFE` -- URL-safe alphabet
- `general_purpose::URL_SAFE_NO_PAD` -- URL-safe without padding (best for CAS file names)

### Recommendation

**Use base64 0.22.1.** You are already on the right track. The Engine API is the stable interface going forward. For CAS path encoding, use `URL_SAFE_NO_PAD` to avoid filesystem issues with `+`, `/`, and `=` characters. However, for hex-encoded blake3 hashes (which are already filesystem-safe), you may not even need base64.

---

## 5. Content-Addressable Storage in Rust

### Existing CAS crates

| Crate | Version | Downloads | Description | Verdict |
|-------|---------|-----------|-------------|---------|
| **cacache** | 13.1.0 | 2.7M | npm-style CAS cache (port of Node.js cacache) | **Best general-purpose CAS** |
| **iroh-blobs** | 0.99.0 | 119K | Content-addressed blobs for p2p (iroh project) | Overkill -- brings p2p networking |
| **ssri** | 9.2.0 | 3.0M | Subresource Integrity hashes (used by cacache) | Useful standalone for integrity verification |
| fcdb-cas | 0.1.0 | 646 | Immature | Skip |
| cassadilia | 0.4.7 | 7.1K | Optimized for large blobs | Niche, low adoption |
| casq_core | 0.12.0 | 222 | Minimal BLAKE3-based CAS | Too immature |

### cacache deep dive

cacache is a Rust port of npm's cache module. It provides:
- Content-addressable storage with integrity verification (SHA-256/SHA-512/SHA-1 via ssri)
- Atomic writes (uses tempfile + rename internally)
- Automatic deduplication
- Key-value metadata on top of content-addressed blobs
- Async (tokio or async-std) and sync APIs
- Lockless concurrent access
- mmap support for reads
- Hard link support (new in v13)
- reflink-copy support
- xxhash support (v11.6+)

### Build vs Buy analysis

| Factor | Build your own | Use cacache |
|--------|---------------|-------------|
| Hash algorithm | BLAKE3 (your choice) | SHA-256/SHA-512 (ssri-based), xxhash optional |
| Storage layout | Custom (e.g., `ab/cdef...`) | npm-compatible content-store |
| Metadata | Custom | Built-in key-value metadata |
| Dependencies | blake3 + tempfile | cacache pulls in ssri, miette, serde, tempfile, tokio/async-std |
| Atomic writes | Must implement | Built-in |
| Control | Full | Must work within cacache's model |
| Deduplication | Must implement | Built-in |

### Recommendation

**Build your own minimal CAS** for these reasons:
1. You want BLAKE3, not SHA-256 -- cacache is built around ssri (Subresource Integrity) which uses SHA family hashes.
2. cacache brings significant transitive dependencies (miette, serde_json, async runtime).
3. A media pipeline CAS is simple: `hash(content) -> store at hash-derived path`. This is ~100 lines of code with blake3 + tempfile.
4. cacache's npm-compatible layout is irrelevant for your use case.

However, **study cacache's patterns**: its atomic write strategy (tempfile in same directory + rename), its content path derivation, and its integrity verification on read are all worth replicating.

Minimal CAS recipe:
```toml
blake3 = "1.8"
tempfile = "3.27"
```

---

## 6. Atomic File Writes in Rust

### Crate comparison

| Crate | Version | Downloads | Last updated | Approach |
|-------|---------|-----------|-------------|----------|
| **tempfile** | 3.27.0 | 488M | Active | Provides `NamedTempFile` with `persist()` (rename) |
| **atomicwrites** | 0.4.4 | 14.1M | 2024-09 | Wraps tempfile + rename with clean API |
| **atomic-write-file** | 0.3.0 | 4.4M | 2025-09 | `std::fs::File`-compatible API, explicit `commit()`, crash-tested |

### Analysis

**tempfile + rename pattern** (the "hand-rolling" approach):
```rust
use tempfile::NamedTempFile;
let mut tmp = NamedTempFile::new_in(target_dir)?;
tmp.write_all(data)?;
tmp.persist(final_path)?;
```
This is the standard pattern. `persist()` calls `rename(2)` on Unix, which is atomic on POSIX filesystems when src and dst are on the same filesystem.

**atomic-write-file** (the ergonomic choice):
```rust
use atomic_write_file::AtomicWriteFile;
let mut file = AtomicWriteFile::options().open("config.json")?;
file.write_all(data)?;
file.commit()?;
```
- Mirrors `std::fs::File` API
- Cross-platform (Unix, Windows, WASI)
- Explicit `commit()` -- file is NOT renamed if dropped without commit
- **Crash-tested** with simulated kernel panics on Linux CI
- 0.3.0 released 2025-09-11

**atomicwrites** (the veteran):
- Older, stable, but less ergonomic
- Callback-based API rather than `File`-like API
- Still actively downloaded (14M total)

### Recommendation

**Use `atomic-write-file` 0.3.0** for dedicated atomic write operations. It has the best API design (`File`-compatible), is crash-tested, and cross-platform. For CAS specifically, `tempfile::NamedTempFile::persist()` is equally valid since you are writing to a temp file and renaming in one operation anyway.

For a media pipeline, the pattern is:
1. Write to `NamedTempFile` in the same directory as the CAS store
2. Compute blake3 hash while writing (streaming)
3. `persist()` to the hash-derived final path
4. If the final path already exists, the content is already stored (dedup for free)

This means `tempfile` alone is sufficient for CAS writes, but `atomic-write-file` is better for non-CAS writes (config files, metadata, indexes).

```toml
tempfile = "3.27"                             # For CAS atomic writes
atomic-write-file = "0.3"                     # For metadata/config atomic writes
```

---

## 7. Image Integrity Validation in Rust

### The spectrum of validation

| Level | What it checks | Crate | Weight |
|-------|---------------|-------|--------|
| 1. Magic bytes | First N bytes match known format | `infer` | Tiny |
| 2. Header parsing | Format header is structurally valid | `imageinfo`, `img-parts` | Small |
| 3. Full decode | Entire file decodes without error | `image`, `png`, `jpeg-decoder` | Heavy |

### Crate details

| Crate | Version | Downloads | What it does |
|-------|---------|-----------|-------------|
| **image** | 0.25.10 | 107M | Full image processing. `image::ImageReader::open(path)?.with_guessed_format()?.decode()?` validates the entire image. |
| **img-parts** | 0.4.0 | 9.1M | Reads/writes JPEG, PNG, RIFF containers at the chunk/segment level without full decode. Can validate structure. |
| **imageinfo** | 0.7.27 | 50K | Gets image dimensions and format from headers only. Very lightweight. |
| **png** | 0.18.1 | 112M | PNG-specific decoder. Can validate PNG structure without full pixel decode using `png::Decoder`. |
| **jpeg-decoder** | 0.3.2 | 67M | JPEG-specific decoder. Validates JPEG structure. |
| **kamadak-exif** | 0.6.1 | 8.4M | EXIF metadata parsing. Validates EXIF blocks. |
| **zune-image** | 0.5.0 | 46K | Alternative image library, focused on performance. |

### Recommended approach for a media pipeline

For "more than magic bytes, less than full decode", use **img-parts**:

```rust
use img_parts::{jpeg::Jpeg, png::Png, ImageEXIF};

// Validate JPEG structure (parses all segments/markers)
let jpeg = Jpeg::from_bytes(data.into())?;  // Fails if not valid JPEG structure

// Validate PNG structure (parses all chunks)
let png = Png::from_bytes(data.into())?;    // Fails if not valid PNG structure
```

This catches:
- Truncated files
- Corrupted headers
- Invalid chunk/segment structure
- Files that have correct magic bytes but are otherwise broken

Without the cost of:
- Full pixel decoding
- Color space conversion
- Memory allocation for the full image bitmap

### For full validation (when needed)

```rust
use image::ImageReader;
let reader = ImageReader::open(path)?
    .with_guessed_format()?;
let _img = reader.decode()?;  // Full validation -- expensive but thorough
```

### Recommendation

**Layer the validation**:

```toml
infer = "0.19"              # Level 1: magic bytes (fast, always)
img-parts = "0.4"           # Level 2: structural validation (medium, on ingest)
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }
                            # Level 3: full decode (expensive, on-demand/async)
```

Only enable the `image` format features you actually need. The full `image` crate with all features is heavy.

---

## Dependency Summary

### Recommended Cargo.toml additions

```toml
[dependencies]
# Hashing (CAS)
blake3 = "1.8"              # 1.8.3 -- CAS hashing, Hash::as_slice() is new

# MIME detection (layered)
infer = "0.19"              # Magic byte detection
file-format = { version = "0.28", default-features = false, features = ["reader-mp4", "reader-zip"] }
                            # Deep format detection for ambiguous types

# MIME utilities
mime_guess = "2.0"          # Extension-based MIME lookup
mime = "0.3"                # Typed MIME representation

# Encoding
base64 = "0.22"             # URL_SAFE_NO_PAD for any base64 needs

# Atomic I/O
tempfile = "3.27"           # CAS atomic writes (NamedTempFile::persist)
atomic-write-file = "0.3"   # Non-CAS atomic writes (metadata, config)

# Image validation
img-parts = "0.4"           # Structural validation without full decode
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }
                            # Full decode validation (on-demand)
```

### What NOT to use

| Crate | Why not |
|-------|---------|
| cacache | Overkill -- pulls async runtime, uses SHA not BLAKE3, npm-specific layout |
| iroh-blobs | Brings entire p2p networking stack |
| tree_magic_mini | Depends on freedesktop.org DB, heavier than infer |
| atomicwrites | Older API design, atomic-write-file is better |
| sled | Embedded DB -- wrong abstraction level for a CAS |

---

## Sources

1. [blake3 on crates.io](https://crates.io/crates/blake3) -- version 1.8.3, 107M downloads
2. [BLAKE3 GitHub Releases](https://github.com/BLAKE3-team/BLAKE3/releases) -- changelog for 1.7.0-1.8.3
3. [infer on crates.io](https://crates.io/crates/infer) -- version 0.19.0, 77M downloads
4. [file-format on crates.io](https://crates.io/crates/file-format) -- version 0.28.0, 1M downloads
5. [mime_guess on crates.io](https://crates.io/crates/mime_guess) -- version 2.0.5, 172M downloads
6. [base64 on crates.io](https://crates.io/crates/base64) -- version 0.22.1, 995M downloads
7. [cacache on crates.io](https://crates.io/crates/cacache) -- version 13.1.0, 2.7M downloads
8. [cacache README](https://github.com/zkat/cacache-rs) -- feature list and API
9. [atomic-write-file on crates.io](https://crates.io/crates/atomic-write-file) -- version 0.3.0, 4.4M downloads
10. [atomic-write-file README](https://github.com/andreacorbellini/rust-atomic-write-file) -- crash-test details
11. [image on crates.io](https://crates.io/crates/image) -- version 0.25.10, 107M downloads
12. [img-parts on crates.io](https://crates.io/crates/img-parts) -- version 0.4.0, 9.1M downloads
13. [tempfile on crates.io](https://crates.io/crates/tempfile) -- version 3.27.0, 488M downloads
