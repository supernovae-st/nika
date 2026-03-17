# Content-Addressed Storage (CAS) with blake3 in Rust

Research date: 2026-03-17

## Summary

This document covers blake3 API usage in Rust, CAS directory layout patterns from
production systems (Git, OCI, Nix, iroh), deduplication strategies, and file naming
conventions for content-addressed stores. Concrete Rust code examples are included
throughout.

---

## 1. blake3 Crate API (v1.8.3)

**Crate**: `blake3` -- Latest version 1.8.3
**License**: CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception
**Source**: https://github.com/BLAKE3-team/BLAKE3
**Docs**: https://docs.rs/blake3

### 1.1 Core Constants

```rust
blake3::OUT_LEN   // 32 bytes (256 bits) -- default output size
blake3::KEY_LEN   // 32 bytes -- key size for keyed hashing
blake3::BLOCK_LEN // 64 bytes -- internal block size
blake3::CHUNK_LEN // 1024 bytes -- chunk size for the Merkle tree
```

### 1.2 Hash Bytes (One-Shot)

```rust
// Simplest: hash all at once
let hash = blake3::hash(b"foobarbaz");
println!("{}", hash);  // prints 64-char lowercase hex string
```

### 1.3 Hash Bytes (Incremental / Streaming)

```rust
let mut hasher = blake3::Hasher::new();
hasher.update(b"foo");
hasher.update(b"bar");
hasher.update(b"baz");
let hash = hasher.finalize();
assert_eq!(blake3::hash(b"foobarbaz"), hash);

// Hasher also implements std::io::Write
use std::io::Write;
let mut hasher = blake3::Hasher::new();
write!(hasher, "hello {}", "world").unwrap();
let hash = hasher.finalize();
```

### 1.4 Hash a File

**Via reader (std feature, enabled by default)**:
```rust
use std::fs::File;

let mut hasher = blake3::Hasher::new();
let file = File::open("data.bin")?;
hasher.update_reader(file)?;  // uses internal 64 KiB buffer
let hash = hasher.finalize();
```

**Via memory map (requires `mmap` feature)**:
```rust
// Single-threaded mmap -- ~10% faster than reader for large cached files
let mut hasher = blake3::Hasher::new();
hasher.update_mmap("data.bin")?;  // auto-falls back to reader for small files (<16 KiB)
let hash = hasher.finalize();
```

**Via mmap + rayon (requires `mmap` + `rayon` features)**:
```rust
// Multi-threaded mmap -- this is what b3sum uses by default
// Can be 10x-20x faster than single-threaded for large cached files
let mut hasher = blake3::Hasher::new();
hasher.update_mmap_rayon("big_file.dat")?;
let hash = hasher.finalize();
```

**Cargo.toml features**:
```toml
[dependencies]
blake3 = { version = "1.8", features = ["mmap", "rayon"] }
```

**Important**: `update_mmap` takes a `Path`, not a `File`, because mmap ignores
seek position. This avoids subtle bugs where `update_reader` and `update_mmap`
would give different results on the same `File` handle.

**Mmap heuristic** (from source `io.rs`):
- Returns `None` (falls back to reader) if: not a regular file, or file size < 16 KiB
- Uses `memmap2::Mmap` under the hood
- The mmap is considered "safe enough" because worst case is hashing wrong bytes
  or SIGBUS, never memory corruption in safe Rust

### 1.5 Output Format

The `blake3::Hash` struct wraps `[u8; 32]`:

```rust
let hash = blake3::hash(b"data");

// Raw bytes
let bytes: &[u8; 32] = hash.as_bytes();
let bytes: [u8; 32] = hash.into();  // consuming

// Hex string (no allocation -- returns ArrayString<64>)
let hex: arrayvec::ArrayString<64> = hash.to_hex();
println!("{}", hex);  // 64 lowercase hex chars

// Display trait also outputs hex
println!("{}", hash);  // same as to_hex()

// From hex
let hash = blake3::Hash::from_hex("af1349b9...")?;
let hash: blake3::Hash = "af1349b9...".parse()?;

// From bytes
let hash = blake3::Hash::from_bytes([0u8; 32]);
let hash = blake3::Hash::from_slice(&bytes_slice)?;  // errors if not exactly 32 bytes

// Constant-time equality (security feature)
// Hash implements PartialEq with constant_time_eq
assert_eq!(hash1, hash2);  // constant-time comparison
```

### 1.6 Keyed Hashing (MAC)

```rust
let key = [42u8; 32];  // must be exactly 32 bytes

// One-shot
let mac = blake3::keyed_hash(&key, b"message");

// Incremental
let mut hasher = blake3::Hasher::new_keyed(&key);
hasher.update(b"message");
let mac = hasher.finalize();
```

Use cases for CAS: keyed hashing lets you create domain-separated hashes.
For example, hashing workflow artifacts with a key derived from the workflow ID
ensures hashes are unique per-workflow even for identical content.

### 1.7 Key Derivation (KDF)

```rust
const CONTEXT: &str = "nika 2026-03-17 artifact content hash";
let input_key = b"some high-entropy key material";
let derived: [u8; 32] = blake3::derive_key(CONTEXT, input_key);

// Incremental
let mut hasher = blake3::Hasher::new_derive_key(CONTEXT);
hasher.update(input_key);
let derived_hash = hasher.finalize();
```

### 1.8 Extended Output (XOF)

BLAKE3 can produce output of any length (shorter outputs are prefixes of longer ones):

```rust
let mut hasher = blake3::Hasher::new();
hasher.update(b"data");
let mut output_reader = hasher.finalize_xof();

// OutputReader implements Read + Seek
let mut output = [0u8; 1000];
output_reader.fill(&mut output);

// The first 32 bytes match the standard hash
assert_eq!(&output[..32], hasher.finalize().as_bytes());
```

### 1.9 Serde Support

```toml
blake3 = { version = "1.8", features = ["serde"] }
```

```rust
#[derive(serde::Serialize, serde::Deserialize)]
struct Artifact {
    hash: blake3::Hash,
    // ...
}
```

### 1.10 Performance Characteristics

- BLAKE3 is a Merkle tree internally (chunks of 1024 bytes)
- SIMD: SSE2, SSE4.1, AVX2, AVX-512, NEON, WASM SIMD (auto-detected at runtime on x86)
- Multi-threading via rayon is beneficial only for inputs > ~128 KiB
- For CAS: hashing is almost never the bottleneck; disk I/O dominates

---

## 2. CAS Directory Layout Patterns

### 2.1 Git Object Storage

```
.git/objects/
  ab/cdef1234567890...   # first 2 hex chars as directory
  pack/
    pack-abc123.idx
    pack-abc123.pack
  info/
```

**Layout**: `objects/{first_2_hex}/{remaining_38_hex}`
**Hash**: SHA-1 (40 hex chars, 20 bytes) -- migrating to SHA-256
**Compression**: zlib-compressed content
**Fan-out**: 256 subdirectories (00-ff)
**Rationale**: Prevents any single directory from having too many entries. Most
filesystems degrade with >10K entries in a single directory (ext4 uses htree but
still has overhead). With uniform hash distribution, 1M objects = ~3900 per bucket.

**Pack files**: Git also packs loose objects into `.pack` files with `.idx` indexes
for better performance with many objects. This is a compaction/GC strategy.

### 2.2 OCI / Docker Image Layer Storage

```
/var/lib/docker/image/overlay2/
  distribution/
    v2metadata-by-diffid/
      sha256/
        ab12cd34...   # full hash as filename
  imagedb/
    content/
      sha256/
        ab12cd34...   # full hash as filename
  layerdb/
    sha256/
      ab12cd34.../
        cache-id
        diff
        size
        tar-split.json.gz

/var/lib/docker/overlay2/
  {cache-id}/
    diff/     # actual layer content
    link      # shortened symlink name
    lower     # parent layer reference
```

**Layout**: `{algorithm}/{full_hash}` -- flat within algorithm directory
**Hash**: SHA-256 (64 hex chars)
**No fan-out**: Docker uses full hash as filename directly (flat directory)
**Content descriptor**: `{mediaType, digest: "sha256:abc...", size: N}`

OCI Image Spec uses the digest format `algorithm:hex` (e.g., `sha256:abc123...`).
The algorithm prefix enables future migration.

### 2.3 Nix Store Paths

```
/nix/store/
  wadmyilr558sjhjifrz7x3w8vj8yzh5z-hello-2.12/
  ├── bin/
  │   └── hello
  └── share/
      └── ...
```

**Layout**: `/nix/store/{hash}-{name}/`
**Hash**: Truncated to 160 bits, encoded in base32 (Nix-specific: `0123456789abcdfghijklmnpqrsvwxyz`)
**Rationale**: The name suffix is for human readability. The hash prefix ensures uniqueness.
Truncation to 160 bits is a deliberate tradeoff: birthday attack resistance of 2^80,
which Nix considers sufficient for a local package store.

As of 2024, Nix is migrating from SHA-256 to BLAKE3 for content hashing
(see https://github.com/NixOS/nix/pull/12379).

### 2.4 Bazel / Remote Build Execution (CAS)

```
cas/
  {hash_function}/
    {full_hex_hash}
```

Bazel's Remote Execution API uses a Content Addressable Storage with the layout:
`{instance_name}/blobs/{hash}/{size_bytes}` for the gRPC API. Locally, it uses
a flat structure keyed by hash.

### 2.5 iroh-blobs (n0 team)

iroh-blobs uses BLAKE3 for content addressing:

```
data/
  blobs.db         # redb metadata database
  data/            # actual blob data files
    {uuid}.temp    # temporary files during import
    {hash}.data    # complete blob data
    {hash}.obao    # outboard Bao tree (for verified streaming)
```

**Key design decisions from iroh-blobs source**:
- Uses `redb` (an embedded key-value database) for metadata, not filesystem layout
- Blob data is stored in flat files named by full BLAKE3 hash
- Supports both "inline" storage (small blobs in the DB) and file-based storage
- Outboard Bao trees enable verified streaming without re-hashing
- Tags system for GC roots (similar to Git refs)
- Temporary files use UUID naming during import, renamed to hash on completion

### 2.6 Summary: Layout Comparison

| System    | Hash    | Fan-out           | Name format             | Extension |
|-----------|---------|-------------------|-------------------------|-----------|
| Git       | SHA-1   | 2-char prefix dir | `{2hex}/{38hex}`        | none      |
| OCI       | SHA-256 | algorithm dir     | `sha256/{64hex}`        | none      |
| Nix       | SHA-256 | flat              | `{base32hash}-{name}/`  | none      |
| Bazel     | SHA-256 | flat              | `{64hex}`               | none      |
| iroh      | BLAKE3  | flat + DB         | `{64hex}.data`          | `.data`   |

### 2.7 Recommended Layout for Nika

Based on the research, a pragmatic layout for a Nika artifact store:

```
artifacts/
  store/
    ab/             # 2-char prefix (Git-style fan-out)
      abcdef1234... # remaining 62 hex chars (BLAKE3 = 64 hex total)
  tmp/
    {uuid}.partial  # in-progress writes
  metadata.db       # optional: redb or sqlite for tags, refs, sizes
```

**Why 2-char fan-out**: Proven at scale by Git. 256 buckets. Works well up to
millions of objects. Avoids filesystem directory entry limits.

**Why full hash, no extension**: The content type should be tracked in metadata,
not in the filename. Content-addressed files are opaque blobs.

---

## 3. Rust CAS Implementations

### 3.1 iroh-blobs (n0-computer/iroh-blobs)

The most mature Rust CAS implementation using BLAKE3.

**Key crates**:
- `iroh-blobs` -- blob store with verified streaming
- `bao-tree` -- BLAKE3 Merkle tree operations
- `iroh-base` -- `Hash` type (wraps `blake3::Hash`)
- `redb` -- embedded database for metadata

**Architecture**:
- Actor-based: main actor handles commands, DB actor handles metadata
- Entity manager pattern for concurrent access to the same hash
- Import flow: write to temp file -> hash -> rename to content-addressed name
- Supports partial downloads via Bao tree chunk ranges
- GC via tag-based reachability (similar to Git refs)

**Source**: https://github.com/n0-computer/iroh-blobs

### 3.2 bao / bao-tree (Verified Streaming with BLAKE3)

Bao implements BLAKE3 verified streaming as described in Section 6.4 of the
BLAKE3 spec. Since BLAKE3 is a Merkle tree internally, you can verify any
slice of the output without re-hashing the entire input.

**Two formats**:
- **Combined**: content bytes + tree hash bytes interleaved
- **Outboard**: tree hash bytes only (separate from content)

**Use cases for CAS**:
- Verify integrity of partial downloads
- Random-access reads with integrity checking
- Prove storage of a file without downloading it (challenge-response)

```rust
// Outboard encoding (used by iroh-blobs)
use bao_tree::{BaoTree, ChunkRanges};
use bao_tree::io::outboard::PreOrderOutboard;

// The outboard data is small relative to the content
// (~62 KB overhead per 1 MB of content)
```

**Source**: https://github.com/oconnor663/bao

### 3.3 Other Notable Rust CAS-Adjacent Crates

| Crate           | Purpose                                          |
|-----------------|--------------------------------------------------|
| `redb`          | Embedded KV store (used by iroh-blobs for meta)  |
| `sled`          | Embedded KV store (alternative to redb)          |
| `cacache`       | Content-addressable, key-value, file-based cache (Node.js port) |
| `ssri`          | Subresource Integrity hash parsing               |
| `git2`/`libgit2`| Git object database access                       |

---

## 4. Concrete Rust CAS Implementation

### 4.1 Minimal CAS Store

```rust
use blake3;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// A content-addressed store using BLAKE3 with Git-style 2-char fan-out.
pub struct ContentStore {
    root: PathBuf,
}

impl ContentStore {
    pub fn new(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("store"))?;
        fs::create_dir_all(root.join("tmp"))?;
        Ok(Self { root })
    }

    /// Compute the storage path for a given hash.
    /// Layout: store/{first_2_hex}/{remaining_62_hex}
    fn blob_path(&self, hash: &blake3::Hash) -> PathBuf {
        let hex = hash.to_hex();
        let hex = hex.as_str();
        self.root
            .join("store")
            .join(&hex[..2])
            .join(&hex[2..])
    }

    /// Check if content with this hash already exists.
    pub fn contains(&self, hash: &blake3::Hash) -> bool {
        self.blob_path(hash).exists()
    }

    /// Store bytes, returning the hash. Deduplicates automatically.
    pub fn put(&self, data: &[u8]) -> io::Result<blake3::Hash> {
        let hash = blake3::hash(data);

        // Fast path: content already exists
        if self.contains(&hash) {
            return Ok(hash);
        }

        // Write to temp file first, then atomic rename
        let tmp_path = self.root.join("tmp").join(format!(
            "{}.partial",
            uuid::Uuid::new_v4()  // or use a simple counter
        ));
        fs::write(&tmp_path, data)?;

        // Ensure fan-out directory exists
        let blob_path = self.blob_path(&hash);
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Atomic rename (same filesystem)
        fs::rename(&tmp_path, &blob_path)?;

        Ok(hash)
    }

    /// Store from a reader (streaming). Hash and write simultaneously.
    pub fn put_reader(&self, mut reader: impl Read) -> io::Result<blake3::Hash> {
        let tmp_path = self.root.join("tmp").join(format!(
            "{}.partial",
            std::process::id()  // simple unique-enough temp name
        ));

        let mut hasher = blake3::Hasher::new();
        let mut tmp_file = fs::File::create(&tmp_path)?;
        let mut buf = [0u8; 65536];  // 64 KiB buffer (matches blake3 internal)

        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    hasher.update(&buf[..n]);
                    tmp_file.write_all(&buf[..n])?;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    let _ = fs::remove_file(&tmp_path);
                    return Err(e);
                }
            }
        }
        tmp_file.flush()?;
        drop(tmp_file);

        let hash = hasher.finalize();

        // Dedup check after hashing
        if self.contains(&hash) {
            let _ = fs::remove_file(&tmp_path);
            return Ok(hash);
        }

        let blob_path = self.blob_path(&hash);
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&tmp_path, &blob_path)?;

        Ok(hash)
    }

    /// Retrieve content by hash.
    pub fn get(&self, hash: &blake3::Hash) -> io::Result<Vec<u8>> {
        fs::read(self.blob_path(hash))
    }

    /// Get a path to the blob (for zero-copy access / mmap).
    pub fn get_path(&self, hash: &blake3::Hash) -> Option<PathBuf> {
        let path = self.blob_path(hash);
        if path.exists() { Some(path) } else { None }
    }

    /// Delete a blob by hash. Returns true if it existed.
    pub fn delete(&self, hash: &blake3::Hash) -> io::Result<bool> {
        let path = self.blob_path(hash);
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}
```

### 4.2 File Hashing with mmap (Production Pattern)

```rust
/// Hash a file using the fastest available method.
/// Requires blake3 features: mmap, rayon
pub fn hash_file(path: &Path) -> io::Result<blake3::Hash> {
    let mut hasher = blake3::Hasher::new();

    // update_mmap_rayon is the fastest for large files:
    // - Automatically falls back to reader for files < 16 KiB
    // - Uses mmap for zero-copy access
    // - Uses rayon for multi-threaded hashing
    #[cfg(all(feature = "mmap", feature = "rayon"))]
    hasher.update_mmap_rayon(path)?;

    #[cfg(all(feature = "mmap", not(feature = "rayon")))]
    hasher.update_mmap(path)?;

    #[cfg(not(feature = "mmap"))]
    {
        let file = std::fs::File::open(path)?;
        hasher.update_reader(file)?;
    }

    Ok(hasher.finalize())
}
```

### 4.3 Verify Content After Read

```rust
/// Read content and verify its hash matches expectations.
pub fn get_verified(store: &ContentStore, expected: &blake3::Hash) -> io::Result<Vec<u8>> {
    let data = store.get(expected)?;
    let actual = blake3::hash(&data);
    if actual != *expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "hash mismatch: expected {}, got {}",
                expected.to_hex(),
                actual.to_hex()
            ),
        ));
    }
    Ok(data)
}
```

### 4.4 Atomic Write Pattern (Production-Grade)

```rust
use std::os::unix::fs::MetadataExt;

/// Write content to CAS with proper atomic semantics.
/// Handles:
/// - Concurrent writers for the same content
/// - Crash safety (no partial writes visible)
/// - Filesystem-level deduplication
pub fn atomic_put(
    root: &Path,
    data: &[u8],
) -> io::Result<blake3::Hash> {
    let hash = blake3::hash(data);
    let hex = hash.to_hex();
    let hex = hex.as_str();

    let dir = root.join("store").join(&hex[..2]);
    let final_path = dir.join(&hex[2..]);

    // Fast path: already exists
    if final_path.exists() {
        return Ok(hash);
    }

    // Write to temp in same filesystem (required for atomic rename)
    let tmp_dir = root.join("tmp");
    fs::create_dir_all(&tmp_dir)?;
    fs::create_dir_all(&dir)?;

    let tmp_path = tmp_dir.join(format!("{}.{}", hex, std::process::id()));

    // Write and sync
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(data)?;
        f.sync_all()?;  // fsync for crash safety
    }

    // Atomic rename. On Unix, rename() is atomic.
    // If another process wrote the same hash concurrently, this is fine:
    // rename just replaces the identical file.
    match fs::rename(&tmp_path, &final_path) {
        Ok(()) => Ok(hash),
        Err(e) => {
            // Clean up temp file on error
            let _ = fs::remove_file(&tmp_path);
            Err(e)
        }
    }
}
```

---

## 5. Deduplication Patterns

### 5.1 Hash-First-Then-Check

The standard CAS deduplication pattern:

```
1. Hash the content  ->  H
2. Check if store/H exists
3a. If yes: skip write, return H (dedup!)
3b. If no: write to tmp, rename to store/H, return H
```

This is inherently race-safe on Unix because `rename()` is atomic. Two processes
writing the same content concurrently will both succeed, one will just overwrite
the other with identical content.

### 5.2 Hard Links vs Symlinks vs Copy

| Method     | Pros                           | Cons                                    |
|------------|--------------------------------|-----------------------------------------|
| **Hard links** | Zero space, instant, no dangling | Same filesystem only, can't link dirs, refcount limit |
| **Symlinks**   | Cross-filesystem, link dirs   | Dangling if target deleted, extra indirection |
| **Copy**       | Independent, cross-filesystem | Uses space, slow for large files        |
| **Reflink**    | CoW, instant, independent     | Filesystem-specific (btrfs, APFS, XFS)  |

**Recommendation for CAS**: Use hard links for references to CAS content when the
reference must be on the same filesystem. Use reflinks (`ficlone` ioctl) when available
(macOS APFS, Linux btrfs/XFS). Fall back to copy.

```rust
/// Create a reference to content in the store.
/// Tries reflink -> hard link -> copy, in that order.
pub fn link_out(
    store_path: &Path,  // path in CAS
    dest: &Path,        // where user wants the file
) -> io::Result<()> {
    // Try reflink (copy-on-write clone) first
    #[cfg(target_os = "macos")]
    {
        // APFS supports clonefile()
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let src = CString::new(store_path.as_os_str().as_bytes())?;
        let dst = CString::new(dest.as_os_str().as_bytes())?;
        unsafe {
            if libc::clonefile(src.as_ptr(), dst.as_ptr(), 0) == 0 {
                return Ok(());
            }
        }
    }

    // Try hard link
    match fs::hard_link(store_path, dest) {
        Ok(()) => return Ok(()),
        Err(_) => {}  // Fall through to copy
    }

    // Fall back to copy
    fs::copy(store_path, dest)?;
    Ok(())
}
```

### 5.3 Garbage Collection

CAS stores need GC because content is shared. Deleting a "reference" should not
delete the content if other references exist. Common approaches:

**A. Reference Counting** (simple, fragile):
```rust
// Metadata DB tracks refcount per hash
// put() increments, unlink() decrements
// GC deletes entries with refcount == 0
// Problem: crashes can leak refcounts
```

**B. Mark-and-Sweep** (Git's approach, robust):
```rust
/// Garbage collect unreferenced blobs.
/// 1. Mark: walk all roots (tags, refs) and mark reachable hashes
/// 2. Sweep: delete any blob not marked
pub fn gc(store_root: &Path, roots: &HashSet<blake3::Hash>) -> io::Result<GcStats> {
    let mut stats = GcStats::default();
    let store_dir = store_root.join("store");

    for prefix_entry in fs::read_dir(&store_dir)? {
        let prefix_entry = prefix_entry?;
        if !prefix_entry.file_type()?.is_dir() { continue; }

        for blob_entry in fs::read_dir(prefix_entry.path())? {
            let blob_entry = blob_entry?;
            let name = blob_entry.file_name();
            let name = name.to_string_lossy();

            // Reconstruct full hex hash
            let prefix = prefix_entry.file_name();
            let prefix = prefix.to_string_lossy();
            let full_hex = format!("{}{}", prefix, name);

            if let Ok(hash) = blake3::Hash::from_hex(&full_hex) {
                if !roots.contains(&hash) {
                    fs::remove_file(blob_entry.path())?;
                    stats.deleted += 1;
                } else {
                    stats.kept += 1;
                }
            }
        }
    }
    Ok(stats)
}

#[derive(Default)]
struct GcStats {
    kept: usize,
    deleted: usize,
}
```

**C. Tag-Based** (iroh-blobs approach):
- Named tags point to hashes (like Git refs)
- Temp tags for in-flight operations (auto-expire)
- GC only deletes blobs with no tags pointing to them
- Protected set during GC to prevent race conditions

---

## 6. File Naming from Hash

### 6.1 Full Hash vs Truncated

| Approach          | Chars | Collision resistance | Use case                |
|-------------------|-------|---------------------|-------------------------|
| Full BLAKE3       | 64    | 2^128 (collision)   | Production CAS          |
| 32 chars (128-bit)| 32    | 2^64 (collision)    | Large-scale but bounded |
| 16 chars (64-bit) | 16    | 2^32 (collision)    | Dev/testing only        |
| 8 chars (32-bit)  | 8     | 2^16 (collision)    | Never for CAS           |
| 7 chars (Git short)| 7    | ~2^14               | Display only            |

**Birthday problem collision probabilities** (approximate):

| Truncation | Items for 50% collision | Items for 10^-6 collision |
|------------|------------------------|--------------------------|
| 32 bits    | ~77,000                | ~93                       |
| 64 bits    | ~5 billion             | ~6 million                |
| 128 bits   | ~1.8 * 10^19           | ~2.6 * 10^13             |
| 256 bits   | ~3.4 * 10^38           | ~4.8 * 10^32             |

**Recommendation**: Always use the full 64-char hex hash for storage. Use truncated
hashes only for display/logging. BLAKE3's 256-bit output is already compact.

### 6.2 With or Without Extension?

**Without extension** (Git, OCI, Nix, most CAS systems):
- Content type belongs in metadata, not filename
- A CAS blob is opaque: the same hash always means the same bytes
- Avoids the question of "what extension?" for arbitrary content

**With extension** (iroh-blobs uses `.data` and `.obao`):
- Distinguishes blob data from metadata/outboard data
- Makes `ls` output more readable for debugging
- Extension describes the *storage format*, not the *content type*

**Recommendation**: No extension for the content blob itself. If you need companion
files (outboard hashes, metadata), use distinct extensions:
```
store/ab/cdef1234...           # content blob (no extension)
store/ab/cdef1234....obao      # outboard Bao tree (optional)
```

### 6.3 Encoding: Hex vs Base32 vs Base64

| Encoding | Chars for 256-bit | Case-sensitive | URL-safe |
|----------|-------------------|----------------|----------|
| Hex      | 64                | No             | Yes      |
| Base32   | 52                | No             | Yes      |
| Base64   | 43                | Yes            | With +/= |
| Base58   | 44                | Yes            | Yes      |

**Hex** is the standard choice (Git, OCI, blake3 crate default). It is simple,
debuggable, and case-insensitive on case-folding filesystems (macOS HFS+/APFS).
The 64 vs 52 character difference is negligible for filenames.

---

## 7. Production Considerations

### 7.1 Concurrent Access

```rust
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Rate-limit concurrent writes to prevent overwhelming the filesystem.
pub struct ConcurrentStore {
    inner: ContentStore,
    write_semaphore: Arc<Semaphore>,
}

impl ConcurrentStore {
    pub fn new(root: impl Into<PathBuf>, max_concurrent_writes: usize) -> io::Result<Self> {
        Ok(Self {
            inner: ContentStore::new(root)?,
            write_semaphore: Arc::new(Semaphore::new(max_concurrent_writes)),
        })
    }

    pub async fn put(&self, data: Vec<u8>) -> io::Result<blake3::Hash> {
        let _permit = self.write_semaphore.acquire().await.unwrap();
        let store = &self.inner;
        tokio::task::spawn_blocking(move || store.put(&data)).await.unwrap()
    }
}
```

### 7.2 Cleanup of Temp Files

```rust
/// Clean up orphaned temp files (from crashes).
pub fn cleanup_tmp(root: &Path, max_age: std::time::Duration) -> io::Result<usize> {
    let tmp_dir = root.join("tmp");
    let mut cleaned = 0;
    let now = std::time::SystemTime::now();

    if let Ok(entries) = fs::read_dir(&tmp_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > max_age {
                            let _ = fs::remove_file(entry.path());
                            cleaned += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(cleaned)
}
```

### 7.3 Integrity Verification (Scrub)

```rust
/// Verify all blobs in the store match their expected hashes.
pub fn scrub(root: &Path) -> io::Result<Vec<CorruptedBlob>> {
    let mut corrupted = vec![];
    let store_dir = root.join("store");

    for prefix_entry in fs::read_dir(&store_dir)?.flatten() {
        if !prefix_entry.file_type()?.is_dir() { continue; }

        for blob_entry in fs::read_dir(prefix_entry.path())?.flatten() {
            let name = blob_entry.file_name();
            let name = name.to_string_lossy();
            let prefix = prefix_entry.file_name();
            let prefix = prefix.to_string_lossy();
            let full_hex = format!("{}{}", prefix, name);

            if let Ok(expected) = blake3::Hash::from_hex(&full_hex) {
                let mut hasher = blake3::Hasher::new();
                let file = fs::File::open(blob_entry.path())?;
                hasher.update_reader(file)?;
                let actual = hasher.finalize();

                if actual != expected {
                    corrupted.push(CorruptedBlob {
                        expected,
                        actual,
                        path: blob_entry.path(),
                    });
                }
            }
        }
    }
    Ok(corrupted)
}

struct CorruptedBlob {
    expected: blake3::Hash,
    actual: blake3::Hash,
    path: PathBuf,
}
```

---

## 8. blake3 Feature Matrix

| Feature            | Default | What it enables                              |
|--------------------|---------|----------------------------------------------|
| `std`              | Yes     | `Write` impl, `update_reader`, `Read`/`Seek` for `OutputReader` |
| `rayon`            | No      | `update_rayon`, multi-threaded hashing        |
| `mmap`             | No      | `update_mmap`, `update_mmap_rayon`            |
| `serde`            | No      | `Serialize`/`Deserialize` for `Hash`          |
| `zeroize`          | No      | `Zeroize` impl for `Hash` and `Hasher`        |
| `traits-preview`   | No      | RustCrypto `digest` trait impls (unstable)    |
| `neon`             | Auto    | NEON SIMD (auto on AArch64, opt-in on ARM)    |
| `wasm32_simd`      | No      | WASM SIMD instructions                        |

**Recommended for CAS**:
```toml
[dependencies]
blake3 = { version = "1.8", features = ["mmap", "rayon", "serde"] }
```

---

## Sources

1. [blake3 crate source (lib.rs)](https://github.com/BLAKE3-team/BLAKE3/blob/master/src/lib.rs) -- Full API with doc comments
2. [blake3 README](https://github.com/BLAKE3-team/BLAKE3/blob/master/README.md) -- Usage examples, benchmarks, adoption list
3. [blake3 io.rs](https://github.com/BLAKE3-team/BLAKE3/blob/master/src/io.rs) -- mmap implementation details
4. [b3sum main.rs](https://github.com/BLAKE3-team/BLAKE3/blob/master/b3sum/src/main.rs) -- Production file hashing patterns
5. [iroh-blobs store/fs.rs](https://github.com/n0-computer/iroh-blobs/blob/main/src/store/fs.rs) -- Production Rust CAS using BLAKE3
6. [iroh README](https://github.com/n0-computer/iroh/blob/main/README.md) -- iroh architecture and protocol composition
7. [Bao README](https://github.com/oconnor663/bao/blob/master/README.md) -- Verified streaming with BLAKE3
8. [BLAKE3 spec](https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf) -- Formal specification
9. [Nix BLAKE3 migration PR](https://github.com/NixOS/nix/pull/12379) -- Nix adopting BLAKE3

## Confidence Level

**High** -- blake3 API documentation is sourced directly from the crate source code
(lib.rs, io.rs). CAS patterns are from production systems (Git, OCI, iroh-blobs)
with verified source code. Directory layout comparisons are based on documented
specifications and observed filesystem structures.

## Further Research Suggestions

- **bao-tree crate** deep dive for verified streaming integration with CAS
- **redb vs sqlite** comparison for CAS metadata storage in Rust
- **reflink support** across filesystems (APFS clonefile, btrfs FICLONE, XFS)
- **Content chunking** (Rabin fingerprinting, FastCDC) for sub-file deduplication
- **BLAKE3 hazmat module** for building custom tree structures over CAS content
