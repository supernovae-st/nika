# Research Report: Rust CAS Crates & Dependencies (March 2026)

## Summary

Comprehensive research into the latest versions, APIs, and best practices for Rust crates
relevant to building a Content-Addressed Storage (CAS) system. Covers blake3, infer, base64,
tokio::fs, tempfile, iroh-blobs, MIME handling, and atomic file write patterns. All data
sourced from crates.io, GitHub, and docs.rs as of 2026-03-17.

---

## 1. blake3

**Latest version:** `1.8.3` (2026-01-08)

### Version History (since 1.5)

| Version | Date       | Key Changes |
|---------|------------|-------------|
| 1.8.3   | 2026-01-08 | `Hash::as_slice`, 2024 Edition, MSRV 1.85, Miri fix for OOB pointer |
| 1.8.2   | 2025-04-21 | CMake TBB fixes |
| 1.8.0   | 2025-03-31 | **`blake3::hazmat` module** replacing deprecated `blake3::guts` |
| 1.7.0   | 2025-03-18 | WASM SIMD backend (6x faster), C multithreading via oneTBB |
| 1.6.1   | 2025-02-27 | Removed accidental `mmap` default feature |
| 1.6.0   | 2025-02-24 | `mmap` feature, memory-mapped file hashing |

### New APIs Since 1.5

- **`blake3::hazmat` module (1.8.0):** First-class replacement for undocumented `blake3::guts`.
  Enables advanced use cases like Bao verified streaming and iroh-blobs, where you manipulate
  chunk/subtree chaining values directly. This is the official API for CAS implementors.
- **`Hash::as_slice` (1.8.3):** Returns `&[u8; 32]` directly, avoiding the copy from
  `Hash::as_bytes()` in hot paths. Important for CAS key lookups.
- **WASM SIMD backend (1.7.0):** Feature-gated via `wasm32_simd`. 6x improvement for large
  inputs under Wasmtime.
- **`mmap` feature (1.6.0):** Memory-mapped hashing for large files, avoids manual chunked reads.

### Benchmarks vs SHA-256/SHA-3

From official BLAKE3 paper benchmarks on Cascade Lake-SP (16 KiB inputs, single-threaded):

| Algorithm | Speed (GB/s) | Relative |
|-----------|-------------|----------|
| BLAKE3    | ~6.8        | 1x       |
| SHA-256   | ~0.5        | ~14x slower |
| SHA-3-256 | ~0.4        | ~17x slower |
| SHA-1     | ~1.0        | ~7x slower  |
| BLAKE2b   | ~1.0        | ~7x slower  |

With AVX-512 + multithreading (Rayon), BLAKE3 scales to **100+ GB/s** on modern CPUs.

**Recommendation:** Use blake3 1.8.3. For CAS, the `hazmat` module is the official way to build
verified streaming. Use `Hash::as_slice` for zero-copy key comparisons.

---

## 2. infer

**Latest version:** `0.19.0` (2025-02-02)

### Version History

| Version | Date       | Key Changes |
|---------|------------|-------------|
| 0.19.0  | 2025-02-02 | **Par2** archive support |
| 0.18.0  | 2025-02-02 | **LZ4** and **bzip3** archive support |
| 0.17.0  | 2025-02-02 | **HEIC/HEIX** (iOS 18) image support |
| 0.16.0  | 2024-06-01 | **DjVu** image format support |
| 0.15.0  | 2023-07-05 | Java class file detection |
| 0.14.0  | 2023-06-22 | Zstd skippable frames support |

### New File Types Since 0.16

- **0.17.0:** HEIX (iOS 18 HEIC variant)
- **0.18.0:** LZ4, bzip3 archives
- **0.19.0:** Par2 (parity archive)

### API Notes

The API has remained stable -- still uses magic number signature matching. No `std` dependency
by default (uses `core` where possible). Good for `no_std` environments.

**Recommendation:** Use infer 0.19.0. It covers the most common file types and is lightweight.
For CAS, use it during import to detect MIME type from file content (magic bytes), not
file extension.

---

## 3. base64

**Latest version:** `0.22.1` (2024-04-30)

### Breaking Changes from 0.21 to 0.22

| Change | Impact |
|--------|--------|
| `DecodeSliceError::OutputSliceTooSmall` is now conservative, not precise | **Positive:** `Engine::decode_slice` now works with exactly-sized output slices |
| `DecodeError::InvalidLength` now refers to valid symbol count | Semantic change in error reporting |
| `Engine::internal_decode` returns `DecodeSliceError` instead of `DecodeError` | Only affects custom engine implementors |
| 5-10% faster decoding | Free perf improvement |
| `BIN_HEX` alphabet symbols corrected (0.22.1) | Bug fix |

### Migration from 0.21 to 0.22

The migration is minimal. The main breaking change is in `internal_decode` return type, which
only affects users implementing custom `Engine` traits. For standard usage
(`STANDARD.encode()`, `STANDARD.decode()`), the API is unchanged.

```rust
// Both 0.21 and 0.22 -- identical usage:
use base64::prelude::*;

let encoded = BASE64_STANDARD.encode(b"hello world");
let decoded = BASE64_STANDARD.decode(&encoded)?;
```

**Recommendation:** Upgrade to 0.22.1. The migration is painless for standard usage. The
decode performance improvement is free.

---

## 4. tokio::fs::rename() Atomicity

**Latest tokio:** `1.50.0` (2026-03-03)

### How It Works

`tokio::fs::rename()` is a thin async wrapper around `std::fs::rename()`, which calls:

- **Linux:** `renameat2(2)` or `rename(2)` -- **atomic on the same filesystem** (POSIX guarantee)
- **macOS:** `renameatx_np(2)` or `rename(2)` -- **atomic on the same filesystem** (POSIX guarantee)
- **Cross-filesystem:** Returns `EXDEV` error; not supported (you must copy + delete)

### Known Issues and Caveats

1. **Same filesystem requirement:** The temp file and target MUST be on the same filesystem/mount
   point. If not, you get `ErrorKind::CrossesDevices`. This is why `tempfile::NamedTempFile`
   created in the same directory as the target is the safe pattern.

2. **No RENAME_NOREPLACE on all platforms:** `std::fs::rename` uses plain `rename()` which
   overwrites the target atomically. If you need no-clobber behavior, you need platform-specific
   code (`renameat2` with `RENAME_NOREPLACE` on Linux 3.15+, `renameatx_np` with
   `RENAME_EXCL` on macOS 10.12+). The `tempfile` crate's `persist_noclobber` handles this.

3. **NFS/network filesystems:** Atomicity is NOT guaranteed on NFS. The `rename()` may not be
   atomic across NFS clients. For CAS on network storage, use `fsync` + `rename` + `fsync`
   on parent directory.

4. **No tokio-specific bugs found:** No open issues in tokio related to `rename` atomicity.
   The implementation is a straightforward `spawn_blocking(std::fs::rename)`.

**Recommendation:** `tokio::fs::rename()` is safe for atomic file placement on local
filesystems. Always create temp files in the same directory as the target. Use
`tempfile::NamedTempFile::persist()` or `persist_noclobber()` for the full atomic pattern.

---

## 5. tempfile

**Latest version:** `3.27.0` (2026-03-11)

### Recent Highlights

| Version | Date       | Key Feature |
|---------|------------|-------------|
| 3.27.0  | 2026-03-11 | `TempPath::try_from_path` (safe relative path handling) |
| 3.26.0  | 2026-02-24 | `persist` on RedoxOS |
| 3.25.0  | 2026-02-09 | `getrandom` 0.4.x support |
| 3.20.0  | 2025-05-11 | `disable_cleanup()` API, `SpooledTempFile::into_file`, `new_in` |
| 3.18.0  | 2025-02-18 | **`persist_noclobber` atomic on Apple** (macOS + Linux + Windows) |
| 3.15.0  | 2024-11-08 | Per-thread RNG reseed from system randomness (DoS fix) |
| 3.13.0  | 2024-08-15 | `with_suffix` constructors |

### Should You Use tempfile Instead of Manual Tmp Files?

**Yes, absolutely.** Here is why:

1. **Atomic persist:** `NamedTempFile::persist(target)` does `rename()` to the target path,
   guaranteeing atomicity on the same filesystem. `persist_noclobber()` uses platform-specific
   no-clobber rename.

2. **Automatic cleanup:** If you crash or panic before `persist()`, the temp file is
   automatically deleted on drop. Manual temp files leak on panic.

3. **Same-directory creation:** `NamedTempFile::new_in(dir)` creates the temp file in the
   same directory as the target, guaranteeing same-filesystem for atomic rename.

4. **Security:** Proper random name generation with system entropy reseeding. Handles TOCTOU
   races. Much safer than `format!("{}.tmp", uuid)`.

5. **Cross-platform:** Works correctly on Linux, macOS, Windows, WASI, Redox.

6. **Well-maintained:** Active development (3.27.0 is 6 days old as of this writing). 33M+
   downloads for `gix-tempfile` (gitoxide uses it), widely battle-tested.

### Pattern for CAS Atomic Writes

```rust
use tempfile::NamedTempFile;
use std::io::Write;

async fn cas_write(dir: &Path, hash: &str, data: &[u8]) -> std::io::Result<()> {
    let target = dir.join(hash);

    // Skip if already exists (CAS is idempotent)
    if target.exists() {
        return Ok(());
    }

    // Create temp file in same directory (same filesystem)
    let mut tmp = NamedTempFile::new_in(dir)?;
    tmp.write_all(data)?;
    tmp.as_file().sync_all()?; // fsync before rename

    // Atomic rename (no-clobber: safe for concurrent CAS writes)
    match tmp.persist_noclobber(&target) {
        Ok(_) => Ok(()),
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()), // race: ok
        Err(e) => Err(e.error),
    }
}
```

**Recommendation:** Use `tempfile` 3.27.0. Never roll your own temp file handling for
production code.

---

## 6. iroh-blobs and Content Addressing with BLAKE3

**Latest version:** `0.98.0` (2026-01-28) -- NOTE: versions on crates.io jumped from 0.35
to 0.90+ series, tracking iroh ecosystem versions.

### How iroh-blobs Works

iroh-blobs is the reference implementation for BLAKE3-based content addressing in Rust:

1. **BLAKE3 as the hash function:** Every blob is identified by its BLAKE3 hash (32 bytes).
2. **Bao verified streaming:** Uses BLAKE3's Merkle tree structure (via `bao-tree` crate)
   for verified streaming -- you can verify chunks independently without downloading the
   entire blob.
3. **Block size:** 16 KiB chunks (`BlockSize::from_chunk_log(4)`).
4. **Store backends:**
   - `MemStore` -- in-memory, for tests and small data
   - `readonly_mem` -- static data sharing
   - `FsStore` -- file-based, for production use with redb metadata database
5. **Entity manager pattern:** The fs store uses an actor-based architecture with a main actor
   for user commands and a database actor for metadata. Tasks handle import/export operations.
6. **Tags:** Named references to hashes, similar to git tags. Used for GC roots.
7. **GC:** Garbage collection with configurable protect callbacks.

### Key CAS Patterns from iroh-blobs

- **Hash is the address:** `Hash` is a newtype around `[u8; 32]` (BLAKE3 output).
- **Streaming import:** Data is hashed and stored incrementally, not buffered entirely in memory.
- **Outboard storage:** Bao tree outboard data (for verified streaming) stored alongside content.
- **Deduplication:** Same content = same hash = stored once.
- **No filename metadata:** Blobs are pure content; metadata is separate (tags, collections).

### Architecture for a Custom CAS

Based on iroh-blobs patterns, a simpler CAS for Nika would look like:

```
store/
  blobs/
    <first-2-hex>/<remaining-hex>   # content files, named by blake3 hash
  tmp/                               # atomic write staging
```

**Recommendation:** iroh-blobs is excellent as a reference, but it brings heavy dependencies
(iroh networking, QUIC, redb). For a local-only CAS (no networking), build a simpler store
inspired by its patterns, using blake3 + tempfile + redb/sqlite for metadata.

---

## 7. MIME Type Handling: Alternatives to mime_guess v2

### Comparison Table

| Crate | Version | Downloads | Detection Method | MIME from Content | MIME from Extension | `no_std` |
|-------|---------|-----------|------------------|-------------------|---------------------|----------|
| `mime_guess` | 2.0.5 | 172M | Extension only | No | Yes | No |
| `infer` | 0.19.0 | 63M | Magic bytes | Yes | No | Yes |
| `file-format` | 0.28.0 | 1M | Magic bytes + readers | Yes | Yes | No |
| `tree_magic_mini` | 3.2.2 | 6.7M | freedesktop.org MIME DB | Yes | Yes | No |
| `mime` | 0.3.17 | 458M | Type definitions only | No | No | No |
| `mime_guess2` | 2.3.1 | 6.7M | Extension (fork) | No | Yes | No |

### Recommendations

**Best overall for CAS:** Use `infer` + `mime_guess` together:
- `infer` for content-based detection during import (reads magic bytes from blob content)
- `mime_guess` as fallback for extension-based detection when filename is known

**Best single crate:** `file-format` 0.28.0 is the most comprehensive:
- 500+ file formats recognized
- Both magic bytes AND readers for complex formats (ZIP, XML, CFB, MP4, PDF...)
- Provides `.media_type()` (MIME), `.extension()`, `.kind()` (Image/Video/Document/etc.)
- Feature-gated readers: enable only what you need
- Actively maintained (0.28.0 from 2025-08-07)
- MSRV 1.85

```rust
use file_format::FileFormat;

let fmt = FileFormat::from_bytes(&data);
let mime = fmt.media_type();   // "image/png"
let ext = fmt.extension();     // "png"
let kind = fmt.kind();         // Kind::Image
```

**Avoid:** `mime_guess` alone for CAS (extension-based only -- unreliable for content-addressed
blobs which may not have filenames). `tree_magic_mini` works but requires the freedesktop
MIME database, adding system-dependency complexity.

---

## 8. Atomic File Write Patterns in Production Rust

### The Canonical Pattern

```rust
use std::io::Write;
use std::fs;
use tempfile::NamedTempFile;

fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));

    // 1. Create temp file in same directory (same filesystem)
    let mut tmp = NamedTempFile::new_in(dir)?;

    // 2. Write all data
    tmp.write_all(data)?;

    // 3. fsync to ensure data is on disk
    tmp.as_file().sync_all()?;

    // 4. Atomic rename
    tmp.persist(path)?;

    // 5. (Optional) fsync parent directory for crash consistency
    //    Required on ext4 for new file visibility after crash
    if let Ok(dir_fd) = fs::File::open(dir) {
        let _ = dir_fd.sync_all();
    }

    Ok(())
}
```

### Available Crates

| Crate | Version | Status | Notes |
|-------|---------|--------|-------|
| `tempfile` | 3.27.0 | **Actively maintained** | Best choice. `persist()` + `persist_noclobber()` |
| `atomicwrites` | 0.4.4 | Maintained (last commit 2024-10) | Simple API, good for basic use |
| `gix-tempfile` | 21.0.1 | Active (gitoxide project) | Global registry for cleanup, signal handling |

### Key Rules for Production

1. **Same filesystem:** Temp file MUST be on same mount as target (use `new_in(target_dir)`).
2. **fsync before rename:** Call `sync_all()` on the file before `persist()`. Without this,
   a power failure after rename could leave a zero-length or corrupt file.
3. **fsync parent directory:** On Linux ext4, `rename()` is metadata-only. The directory entry
   may not survive a crash without `fsync` on the parent directory.
4. **No-clobber for CAS:** Use `persist_noclobber()` for CAS writes where concurrent writers
   might race to store the same hash. The loser gets `AlreadyExists` -- just discard.
5. **Cleanup on panic:** `NamedTempFile` deletes on drop. Manual tmp files leak. Always prefer
   `NamedTempFile`.

### Async Pattern (tokio)

```rust
use tokio::io::AsyncWriteExt;
use tempfile::NamedTempFile;

async fn atomic_write_async(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap();

    // tempfile creation is sync but fast (no I/O beyond open())
    let tmp = NamedTempFile::new_in(dir)?;
    let tmp_path = tmp.into_temp_path();

    // Use tokio for actual I/O
    let mut file = tokio::fs::File::create(tmp_path.to_path_buf()).await?;
    file.write_all(data).await?;
    file.sync_all().await?;
    drop(file);

    // Rename is async-wrapped but effectively instant
    tokio::fs::rename(&tmp_path, path).await?;

    // Prevent TempPath from deleting the file (it was already renamed)
    tmp_path.keep()?;

    Ok(())
}
```

Or more simply, use `spawn_blocking` with the sync pattern:

```rust
async fn atomic_write_async(path: PathBuf, data: Vec<u8>) -> std::io::Result<()> {
    tokio::task::spawn_blocking(move || atomic_write(&path, &data)).await?
}
```

---

## 9. Best Practices for Rust CAS Implementations (2026)

### Storage Layout

```
<root>/
  blobs/
    <hex[0..2]>/<hex[2..]>        # 2-char prefix for directory fan-out
  tmp/                             # staging directory for atomic writes
  meta.db                          # redb or SQLite for metadata/tags/refs
```

Fan-out with 2 hex chars = 256 subdirectories, keeping directory entry counts manageable
up to millions of blobs.

### Hash Selection

**BLAKE3** is the consensus choice for new CAS implementations in 2026:
- 14-17x faster than SHA-256/SHA-3 single-threaded
- Scales with SIMD and threads (Rayon)
- Merkle tree structure enables verified streaming
- Used by iroh, Bao, and growing ecosystem
- `hazmat` module (1.8.0+) provides official API for advanced CAS use

### Recommended Dependency Stack

```toml
[dependencies]
blake3 = "1.8"           # Hashing
tempfile = "3.27"        # Atomic writes
infer = "0.19"           # Content-type detection (magic bytes)
base64 = "0.22"          # Hash display (URL-safe encoding)
file-format = "0.28"     # Rich MIME + file kind detection (optional, heavier)
tokio = { version = "1.50", features = ["fs"] }  # Async I/O

# For metadata storage (pick one):
redb = "2.x"             # Embedded key-value (iroh uses this)
# OR
rusqlite = "0.32"        # SQLite (if you need SQL queries)
```

### Deduplication Strategy

CAS is inherently deduplicated: same content = same hash = one storage location.
For the write path:

1. Hash the content with BLAKE3
2. Check if `blobs/<prefix>/<hash>` exists
3. If yes: skip write (or verify with a read-back)
4. If no: atomic write via tempfile (see Section 8)

### Integrity Verification

- On write: hash content, store at hash-named path
- On read: re-hash and compare (optional, depends on threat model)
- Periodic: background GC scan that re-hashes and removes corrupt entries

---

## Sources

1. [blake3 crate on crates.io](https://crates.io/crates/blake3) -- v1.8.3, 2026-01-08
2. [BLAKE3 GitHub releases](https://github.com/BLAKE3-team/BLAKE3/releases) -- Full changelog
3. [BLAKE3 README](https://github.com/BLAKE3-team/BLAKE3/blob/master/README.md) -- Benchmarks
4. [infer crate on crates.io](https://crates.io/crates/infer) -- v0.19.0, 2025-02-02
5. [infer GitHub releases](https://github.com/bojand/infer/releases) -- Changelog
6. [base64 RELEASE-NOTES.md](https://github.com/marshallpierce/rust-base64/blob/master/RELEASE-NOTES.md) -- v0.22.1
7. [tempfile CHANGELOG.md](https://github.com/Stebalien/tempfile/blob/master/CHANGELOG.md) -- v3.27.0, 2026-03-11
8. [iroh-blobs README](https://github.com/n0-computer/iroh-blobs/blob/main/README.md) -- CAS architecture
9. [iroh-blobs CHANGELOG](https://github.com/n0-computer/iroh-blobs/blob/main/CHANGELOG.md) -- v0.98.0
10. [iroh-blobs src/store/fs.rs](https://github.com/n0-computer/iroh-blobs/blob/main/src/store/fs.rs) -- FS store design
11. [cacache-rs](https://github.com/zkat/cacache-rs) -- v13.1.0, CAS cache reference
12. [file-format crate](https://crates.io/crates/file-format) -- v0.28.0, 500+ formats
13. [atomicwrites crate](https://crates.io/crates/atomicwrites) -- v0.4.4, simple atomic writes
14. [tokio docs: fs::rename](https://docs.rs/tokio/latest/tokio/fs/fn.rename.html) -- v1.50.0

## Methodology

- **Tools used:** crates.io API, GitHub API, GitHub raw file access
- **Pages analyzed:** 25+
- **Data freshness:** All version numbers verified against crates.io on 2026-03-17

## Confidence Level

**High** -- All version numbers and changelogs are sourced directly from crates.io and GitHub.
Benchmark numbers are from the official BLAKE3 paper. Atomic write patterns are well-established
POSIX semantics. The iroh-blobs architecture analysis is based on current source code.

## Further Research Suggestions

- Benchmark blake3 `mmap` feature vs manual chunked reads for Nika's typical blob sizes
- Evaluate `redb` vs `rusqlite` for CAS metadata storage (tags, refs, MIME types)
- Investigate `bao-tree` crate directly for verified streaming without full iroh-blobs dependency
- Consider `content_inspector` crate for binary vs text detection in CAS imports
- Research `opendal` (Apache OpenDAL) for multi-backend CAS storage (local + S3 + ...)
