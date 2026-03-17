# Research Report: Rust Binary Data Handling in Workflow/Pipeline Architectures

## Summary

This report covers the Rust ecosystem for handling binary data in async workflow engines: zero-copy buffer types, async file I/O, base64 codec patterns, content-addressed storage, MIME detection, rmcp SDK binary types, JSON/binary interop, and large file streaming. All findings are drawn from crate documentation (docs.rs), source code inspection (rmcp 0.16.0 local source), and crates.io metadata.

---

## 1. bytes Crate Patterns: Bytes, BytesMut, and Tradeoffs

**Crate**: `bytes` (tokio project)
**Source**: [docs.rs/bytes](https://docs.rs/bytes/latest/bytes/)

### Core Types

| Type | Ownership | Clone Cost | Mutability | Use Case |
|------|-----------|------------|------------|----------|
| `Vec<u8>` | Owned, unique | O(n) deep copy | Mutable | Building buffers, small data |
| `Arc<[u8]>` | Shared, ref-counted | O(1) pointer bump | Immutable | Shared immutable blobs, cache values |
| `Bytes` | Shared, vtable-dispatch | O(1) ref-count inc | Immutable | Zero-copy networking, passing around |
| `BytesMut` | Unique, growable | N/A (not Clone) | Mutable | Building buffers, then freeze |

### Bytes Internals

`Bytes` is 4 `usize` fields internally: a vtable pointer, a data pointer, a length, and custom state. It uses dynamic dispatch through the vtable to support multiple backing stores:

- **From `Vec<u8>`**: Stores the Vec's allocation, ref-counted via Arc internally
- **From `&'static [u8]`** (`Bytes::from_static`): Zero-cost clone, no ref counting
- **From `BytesMut::freeze()`**: Converts unique buffer to shared

### Key Pattern: Build then Freeze

```rust
use bytes::{BytesMut, BufMut, Bytes};

// Phase 1: Build (mutable, unique)
let mut buf = BytesMut::with_capacity(64);
buf.put_u8(b'H');
buf.put(&b"ello"[..]);

// Phase 2: Freeze (immutable, shareable, zero-copy clone)
let shared: Bytes = buf.freeze();
let clone = shared.clone(); // O(1), same backing memory
```

### Key Pattern: Zero-Copy Slicing

```rust
let data = Bytes::from(vec![0u8; 1024]);
let header = data.slice(0..16);   // no copy, same allocation
let body = data.slice(16..);      // no copy
// header, body, data all point to same underlying memory
```

### Recommendation for Nika

For a workflow engine handling binary artifacts:
- **`Bytes`** for passing binary data between pipeline steps (zero-copy cloning)
- **`BytesMut`** for constructing binary output (base64 decode target, HTTP response accumulation)
- **`Arc<[u8]>`** for long-lived cache entries where you want simpler semantics than Bytes
- **`Vec<u8>`** only when you need ownership AND mutation AND won't share

---

## 2. tokio::fs vs std::fs: Async File I/O

**Source**: [docs.rs/tokio/latest/tokio/fs](https://docs.rs/tokio/latest/tokio/fs/)

### Key Finding: tokio::fs::write is spawn_blocking Underneath

From the docs: _"This operation is implemented by running the equivalent blocking operation on a separate thread pool using `spawn_blocking`."_

This means `tokio::fs::write` does NOT use io_uring or true async I/O -- it just moves the blocking call to a thread pool. This has implications:

### When to Use What

| Scenario | Recommendation | Why |
|----------|---------------|-----|
| Single file write (< 10MB) | `tokio::fs::write()` | Convenient, non-blocking to caller |
| Large file write (> 10MB) | `tokio::io::AsyncWriteExt` with `tokio::fs::File` | Streaming, bounded memory |
| Write from sync context | `std::fs::write()` | No runtime overhead |
| Inside `spawn_blocking` | `std::fs::write()` | Already on blocking thread, double-spawn is wasteful |
| Many small writes in batch | `tokio::task::spawn_blocking` + `std::fs` | Amortize thread-switch overhead |

### Streaming Write Pattern

```rust
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

let mut file = File::create("output.bin").await?;

// Write in chunks, never loading entire file into memory
while let Some(chunk) = stream.next().await {
    file.write_all(&chunk).await?;
}
file.flush().await?;
```

### The spawn_blocking Nuance

```rust
// BAD: Double spawn_blocking overhead
tokio::fs::write("a.txt", data_a).await?; // spawns blocking task
tokio::fs::write("b.txt", data_b).await?; // spawns another blocking task

// GOOD: Batch blocking work
tokio::task::spawn_blocking(move || {
    std::fs::write("a.txt", &data_a)?;
    std::fs::write("b.txt", &data_b)?;
    Ok::<_, std::io::Error>(())
}).await??;
```

---

## 3. Base64 Decoding in Rust

**Crate**: `base64` v0.22.1
**Source**: [docs.rs/base64/0.22.1](https://docs.rs/base64/0.22.1/base64/)

### API Overview

```rust
use base64::prelude::*;

// One-shot decode (allocates Vec<u8>)
let bytes: Vec<u8> = BASE64_STANDARD.decode(b"SGVsbG8=")?;

// One-shot encode
let encoded: String = BASE64_STANDARD.encode(b"Hello");

// Decode into pre-allocated buffer (avoids allocation)
let mut buf = vec![0u8; 1024];
let len = BASE64_STANDARD.decode_slice(input, &mut buf)?;
```

### Streaming Decode (base64::read::DecoderReader)

```rust
use base64::read::DecoderReader;
use std::io::Read;

// Wraps any Read, decodes base64 on the fly
let mut decoder = DecoderReader::new(
    input_reader,  // impl Read (e.g., Cursor, File)
    &BASE64_STANDARD
);

let mut decoded = Vec::new();
decoder.read_to_end(&mut decoded)?;

// Or read in chunks:
let mut chunk = [0u8; 8192];
loop {
    let n = decoder.read(&mut chunk)?;
    if n == 0 { break; }
    output.write_all(&chunk[..n])?;
}
```

### Streaming Encode (base64::write::EncoderWriter)

```rust
use base64::write::EncoderWriter;
use std::io::Write;

let mut encoder = EncoderWriter::new(
    output_writer,  // impl Write
    &BASE64_STANDARD
);
encoder.write_all(binary_data)?;
encoder.finish()?; // Must call to flush padding
```

### Performance Notes

- `base64` v0.22 uses SIMD-optimized paths on supported platforms
- One-shot decode of a 1MB base64 string: ~microseconds (fast)
- Streaming decode advantage: bounded memory usage, not speed
- `data-encoding` v2.10.0 is the alternative: slightly different API, comparable speed, more encoding formats (base32, hex), but no streaming Read/Write wrappers

### Recommendation for Nika

For MCP tool results returning base64 images:
```rust
// MCP returns base64 as String in RawImageContent.data
// Decode into Bytes for pipeline use:
let decoded_bytes: Vec<u8> = BASE64_STANDARD.decode(&image_content.data)?;
let artifact = Bytes::from(decoded_bytes);
```

For large base64 blobs (>10MB), use streaming decode to avoid 2x memory:
```rust
use std::io::Cursor;
let reader = Cursor::new(base64_string.as_bytes());
let mut decoder = DecoderReader::new(reader, &BASE64_STANDARD);
// Stream to file or hasher without full allocation
```

---

## 4. Content-Addressed Storage in Rust

### The Pattern

Content-addressed storage (CAS) uses a cryptographic hash of content as its key. Like git objects: `hash(content) -> filename`, guaranteeing deduplication and immutability.

### Crate Landscape

| Crate | Version | Downloads | Description |
|-------|---------|-----------|-------------|
| `blake3` | 1.8.3 | millions | The hash function of choice (14x faster than SHA-256) |
| `sha2` | 0.10.x | millions | SHA-256 (already in Nika's deps) |
| `cassadilia` | 0.4.7 | 7,127 | Full CAS with WAL, transactions, blob indexing |
| `casq_core` | 0.12.0 | 222 | Minimal CAS using BLAKE3 |
| `guts-storage` | 0.1.0 | 314 | Git-style object storage |

### blake3 for CAS (Recommended)

```rust
use blake3;

// Hash content to get address
let hash = blake3::hash(b"file contents here");
let hex = hash.to_hex();  // "af1349b9f5f9a1a6a0404d..."

// Incremental hashing for large files (streaming)
let mut hasher = blake3::Hasher::new();
hasher.update(b"chunk 1");
hasher.update(b"chunk 2");
let hash = hasher.finalize();

// With rayon feature: parallel hashing of large files
// hasher.update_rayon(large_slice);

// With mmap feature: memory-mapped file hashing
// hasher.update_mmap(path)?;
```

### blake3 vs SHA-256

| Property | blake3 | SHA-256 |
|----------|--------|---------|
| Speed | ~14x faster | Baseline |
| Output size | 32 bytes (extensible) | 32 bytes |
| Parallel hashing | Built-in (rayon feature) | No |
| Streaming | Yes (Hasher::update) | Yes |
| SIMD | Auto-detected | Depends on impl |
| Cryptographic | Yes | Yes |
| Ecosystem | Growing fast | Ubiquitous |

### DIY CAS Pattern (Git-style, ~50 lines)

```rust
use blake3;
use std::path::{Path, PathBuf};

pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Store content, return its hash key
    pub async fn put(&self, data: &[u8]) -> std::io::Result<String> {
        let hash = blake3::hash(data);
        let hex = hash.to_hex().to_string();

        // Git-style 2-char prefix sharding: ab/cdef1234...
        let dir = self.root.join(&hex[..2]);
        tokio::fs::create_dir_all(&dir).await?;

        let path = dir.join(&hex[2..]);
        if !path.exists() {  // Dedup: skip if already stored
            tokio::fs::write(&path, data).await?;
        }
        Ok(hex)
    }

    /// Retrieve content by hash
    pub async fn get(&self, hash: &str) -> std::io::Result<Vec<u8>> {
        let path = self.root.join(&hash[..2]).join(&hash[2..]);
        tokio::fs::read(&path).await
    }

    /// Check if content exists
    pub fn exists(&self, hash: &str) -> bool {
        self.root.join(&hash[..2]).join(&hash[2..]).exists()
    }

    /// Path to stored artifact (for zero-copy reference)
    pub fn path(&self, hash: &str) -> PathBuf {
        self.root.join(&hash[..2]).join(&hash[2..])
    }
}
```

### Why Not Use Existing CAS Crates?

- `cassadilia`: Full-featured but heavy (WAL, transactions) -- overkill for artifact caching
- `casq_core`: Only 222 downloads, immature
- The pattern is simple enough (hash + fs::write) that a bespoke implementation (~50 lines) avoids a dependency and gives full control over the storage layout

---

## 5. MIME Type Handling in Rust

### Crate Ecosystem

| Crate | Version | Approach | Best For |
|-------|---------|----------|----------|
| `mime` | 0.3.17 | Strongly-typed MIME type struct | Parsing/comparing MIME strings |
| `mime_guess` | 2.0.5 | Extension-to-MIME lookup | Guessing from file paths |
| `infer` | 0.19.0 | Magic bytes detection | Detecting from file content |

### mime crate

```rust
use mime::Mime;

let png: Mime = "image/png".parse().unwrap();
assert_eq!(png.type_(), mime::IMAGE);
assert_eq!(png.subtype(), "png");

// Constants available
assert_eq!(mime::IMAGE_PNG, "image/png".parse::<Mime>().unwrap());
assert_eq!(mime::APPLICATION_JSON, "application/json".parse::<Mime>().unwrap());
```

### mime_guess crate

```rust
use mime_guess;

// Extension -> MIME
let mime = mime_guess::from_ext("png").first_or_octet_stream();
assert_eq!(mime, mime::IMAGE_PNG);

// Path -> MIME
let mime = mime_guess::from_path("image.jpg").first_or_octet_stream();
assert_eq!(mime, mime::IMAGE_JPEG);

// MIME -> Extensions (reverse lookup)
let exts = mime_guess::get_mime_extensions_str("image/png");
// Returns Some(&["png"])
```

### infer crate (Magic Bytes Detection)

```rust
// From buffer (first few bytes are enough)
let buf = [0xFF, 0xD8, 0xFF, 0xAA]; // JPEG magic
let kind = infer::get(&buf).expect("file type is known");
assert_eq!(kind.mime_type(), "image/jpeg");
assert_eq!(kind.extension(), "jpg");

// From file path
let kind = infer::get_from_path("photo.jpg")?.expect("known type");

// Type class checks
assert!(infer::is_image(&buf));
assert!(!infer::is_video(&buf));

// Custom matcher
let mut info = infer::Infer::new();
info.add("custom/foo", "foo", |buf| {
    buf.len() >= 3 && buf[0] == 0x10
});
```

### Combined Pattern for Nika Artifacts

```rust
/// Detect MIME type using multiple strategies (magic > extension > default)
fn detect_mime(data: &[u8], filename: Option<&str>) -> String {
    // Strategy 1: Magic bytes (most reliable)
    if let Some(kind) = infer::get(data) {
        return kind.mime_type().to_string();
    }

    // Strategy 2: File extension
    if let Some(name) = filename {
        let mime = mime_guess::from_path(name).first_or_octet_stream();
        return mime.to_string();
    }

    // Strategy 3: Default
    "application/octet-stream".to_string()
}

/// Get file extension for a MIME type
fn mime_to_extension(mime_type: &str) -> &str {
    match mime_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "audio/wav" => "wav",
        "audio/mpeg" => "mp3",
        "video/mp4" => "mp4",
        "application/pdf" => "pdf",
        _ => mime_guess::get_mime_extensions_str(mime_type)
            .and_then(|exts| exts.first())
            .copied()
            .unwrap_or("bin"),
    }
}
```

---

## 6. rmcp SDK Types: Binary Content Representation

**Crate**: `rmcp` v0.16.0
**Source**: Local cargo registry at `~/.cargo/registry/src/.../rmcp-0.16.0/`

### Content Type Hierarchy

```
Content = Annotated<RawContent>

RawContent (tagged enum, "type" field)
├── Text(RawTextContent)           { text: String }
├── Image(RawImageContent)         { data: String, mime_type: String }
├── Audio(RawAudioContent)         { data: String, mime_type: String }
├── Resource(RawEmbeddedResource)  { resource: ResourceContents }
└── ResourceLink(RawResource)      { uri: String, name: String, ... }
```

### Critical Observation: ALL Binary Data is base64 String

In rmcp 0.16.0:
- **`RawImageContent.data`**: `String` (base64-encoded)
- **`RawAudioContent.data`**: `String` (base64-encoded)
- **`ResourceContents::BlobResourceContents.blob`**: `String` (base64-encoded)

There is NO `Bytes`, `Vec<u8>`, or binary buffer type anywhere in the rmcp content model. All binary is transported as base64 `String` per the MCP JSON-RPC protocol.

### ResourceContents (for EmbeddedResource)

```rust
pub enum ResourceContents {
    TextResourceContents {
        uri: String,
        mime_type: Option<String>,
        text: String,
    },
    BlobResourceContents {
        uri: String,
        mime_type: Option<String>,
        blob: String,  // base64-encoded binary
    },
}
```

### RawResource / ResourceLink (MCP 2025-11 addition)

```rust
pub struct RawResource {
    pub uri: String,           // "file:///path/to/file" or custom URI
    pub name: String,          // Display name
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<u32>,     // Raw byte size (before base64)
    pub icons: Option<Vec<Icon>>,
}
```

**ResourceLink** is the key new type -- it provides a *reference* to a resource without embedding the content. This is the escape hatch for large binary data.

### CallToolResult (what tool_call returns)

```rust
pub struct CallToolResult {
    pub content: Vec<Content>,                // Text, Image, Audio, Resource, ResourceLink
    pub structured_content: Option<Value>,    // JSON structured output
    pub is_error: Option<bool>,
}
```

A tool result can contain mixed content: text description + image + resource link.

### SamplingMessageContent (what goes to LLMs)

```rust
pub enum SamplingMessageContent {
    Text(RawTextContent),
    Image(RawImageContent),     // base64 data inline
    Audio(RawAudioContent),     // base64 data inline
    ToolUse(ToolUseContent),
    ToolResult(ToolResultContent),
}
```

**Important**: `Resource` and `ResourceLink` variants are NOT supported in sampling messages. The `TryFrom<Content> for SamplingMessageContent` explicitly rejects them:
```rust
RawContent::Resource(_) => Err("Resource content is not supported in sampling messages")
RawContent::ResourceLink(_) => Err("ResourceLink content is not supported in sampling messages")
```

### Implications for Nika

1. MCP tools return binary as base64 `String` in `Content::Image` or `Content::Resource(BlobResourceContents)`
2. To use binary in a workflow, Nika must decode the base64 String into bytes
3. ResourceLink provides a URI reference pattern -- the content is NOT embedded, just pointed to
4. When sending to LLMs, only Image/Audio (with inline base64) are supported -- no file references
5. For large binaries, the pattern should be: store to CAS, pass URI reference between steps, only base64-encode when sending to LLM

---

## 7. serde_json::Value and Binary: JSON/Binary Interop

### The Problem

JSON has no binary type. `serde_json::Value` can represent:
- `Value::String` (for base64-encoded binary)
- `Value::Array` of `Value::Number` (byte array -- wasteful)

There is no `Value::Binary` variant.

### Pattern 1: Base64 String in JSON (Standard MCP Approach)

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Artifact {
    #[serde(with = "base64_serde")]
    data: Vec<u8>,  // Serialized as base64 string in JSON
    mime_type: String,
}

// Custom serde module
mod base64_serde {
    use base64::prelude::*;
    use serde::{Serializer, Deserializer, Deserialize};

    pub fn serialize<S>(data: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(&BASE64_STANDARD.encode(data))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where D: Deserializer<'de> {
        let s = String::deserialize(deserializer)?;
        BASE64_STANDARD.decode(&s).map_err(serde::de::Error::custom)
    }
}
```

### Pattern 2: File Handle Reference in JSON (CAS Approach)

```rust
#[derive(Serialize, Deserialize)]
struct ArtifactRef {
    /// Content-addressed hash pointing to stored file
    hash: String,          // "blake3:af1349b9f5..."
    mime_type: String,
    size: u64,
    /// Local file path (resolved at runtime, not serialized over wire)
    #[serde(skip)]
    local_path: Option<PathBuf>,
}
```

### Pattern 3: Hybrid Value Type (What Nika Could Use)

```rust
/// A workflow value that can be text, JSON, or a binary artifact reference
#[derive(Clone, Debug)]
pub enum TaskOutput {
    /// Plain text
    Text(String),
    /// Structured JSON
    Json(serde_json::Value),
    /// Binary artifact stored in CAS
    Artifact {
        hash: String,
        mime_type: String,
        size: u64,
    },
    /// Multiple content blocks (from MCP tool results)
    Mixed(Vec<TaskOutput>),
}
```

This avoids putting binary data in `serde_json::Value` entirely -- binary lives on disk in CAS, and only the reference (hash) travels through the JSON pipeline.

---

## 8. Streaming Large Files (>100MB)

### The Challenge

A 100MB video as base64 becomes ~133MB of String. Loading it all into memory doubles to ~233MB (String + decoded bytes). For a pipeline with multiple steps, this compounds.

### Pattern 1: AsyncRead/AsyncWrite Chaining

```rust
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::fs::File;

async fn stream_copy(
    mut reader: impl AsyncRead + Unpin,
    mut writer: impl tokio::io::AsyncWrite + Unpin,
) -> std::io::Result<u64> {
    tokio::io::copy(&mut reader, &mut writer).await
}

// Example: HTTP response -> file
let response = reqwest::get(url).await?;
let mut stream = response.bytes_stream();
let mut file = File::create("video.mp4").await?;

while let Some(chunk) = stream.try_next().await? {
    file.write_all(&chunk).await?;
}
```

### Pattern 2: tokio_util::io Bridges

```rust
use tokio_util::io::{ReaderStream, StreamReader};

// AsyncRead -> Stream<Bytes>
let file = File::open("large.mp4").await?;
let stream = ReaderStream::new(file); // yields Bytes chunks

// Stream<Bytes> -> AsyncRead
let reader = StreamReader::new(stream);
```

### Pattern 3: Chunked Hashing While Streaming

```rust
use blake3;
use tokio::io::AsyncReadExt;

async fn hash_and_store(
    mut reader: impl AsyncRead + Unpin,
    store_path: &Path,
) -> std::io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut file = File::create(store_path).await?;
    let mut buf = vec![0u8; 64 * 1024]; // 64KB chunks

    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).await?;
    }

    Ok(hasher.finalize().to_hex().to_string())
}
```

### Pattern 4: Memory-Mapped I/O (for read-heavy workloads)

```rust
// blake3 has built-in mmap support
let mut hasher = blake3::Hasher::new();
hasher.update_mmap("large_file.bin")?; // mmap feature required
let hash = hasher.finalize();
```

### Pattern 5: Bounded Channel for Pipeline Streaming

```rust
use tokio::sync::mpsc;
use bytes::Bytes;

// Producer (e.g., HTTP download)
let (tx, mut rx) = mpsc::channel::<Bytes>(16); // bounded backpressure

tokio::spawn(async move {
    while let Some(chunk) = http_stream.next().await {
        tx.send(chunk?).await.ok();
    }
});

// Consumer (e.g., file write + hash)
let mut hasher = blake3::Hasher::new();
let mut file = File::create("output.bin").await?;
while let Some(chunk) = rx.recv().await {
    hasher.update(&chunk);
    file.write_all(&chunk).await?;
}
```

### Large File Strategy Summary

| File Size | Strategy |
|-----------|----------|
| < 1MB | Load into memory as `Bytes`, pass through pipeline |
| 1-10MB | Load into memory, store to CAS, pass hash reference |
| 10-100MB | Stream to CAS file while hashing, pass reference |
| > 100MB | Stream with bounded channel, never fully in memory |

---

## Sources

1. [docs.rs/bytes](https://docs.rs/bytes/latest/bytes/) - Bytes/BytesMut documentation
2. [docs.rs/tokio/fs](https://docs.rs/tokio/latest/tokio/fs/) - tokio file I/O (spawn_blocking note)
3. [docs.rs/base64/0.22.1](https://docs.rs/base64/0.22.1/base64/) - Base64 encode/decode with streaming modules
4. [docs.rs/blake3/1.8.3](https://docs.rs/blake3/latest/blake3/) - BLAKE3 hash function with mmap/rayon features
5. [docs.rs/infer/0.19.0](https://docs.rs/infer/0.19.0/infer/) - Magic bytes MIME detection
6. [docs.rs/mime_guess/2.0.5](https://docs.rs/mime_guess/latest/mime_guess/) - Extension-to-MIME lookup
7. [docs.rs/mime/0.3.17](https://docs.rs/mime/latest/mime/) - Strongly-typed MIME struct
8. `rmcp` v0.16.0 source code - Local cargo registry inspection
9. [docs.rs/tokio-util/io](https://docs.rs/tokio-util/latest/tokio_util/io/) - ReaderStream/StreamReader bridges
10. [crates.io search: content-addressed storage](https://crates.io/search?q=content-addressed+storage) - CAS crate landscape

## Methodology

- Tools used: docs.rs HTML scraping, crates.io API, local cargo registry source inspection
- Files analyzed: rmcp 0.16.0 source (content.rs, resource.rs, tool.rs, model.rs, annotated.rs)
- All rmcp types verified against actual Rust source code, not documentation

## Confidence Level

**High** - All rmcp findings verified from source code. Crate API patterns verified from docs.rs. Performance claims based on published benchmarks (blake3 team, base64 crate authors). CAS pattern recommendations are conservative (DIY over immature crates).

## Key Takeaways for Nika Architecture

1. **rmcp represents ALL binary as base64 `String`** -- there is no zero-copy binary path in MCP JSON-RPC
2. **The decode boundary is Nika's responsibility** -- rmcp gives you Strings, Nika must decode to bytes
3. **ResourceLink is the escape hatch** -- URI references instead of inline base64 for large content
4. **blake3 + simple filesystem = sufficient CAS** -- no need for a CAS crate dependency
5. **Bytes is the right internal type** -- zero-copy cloning between pipeline steps after base64 decode
6. **Streaming decode matters at scale** -- base64::read::DecoderReader for >10MB blobs
7. **infer crate for magic bytes** -- don't trust the MIME type from MCP; verify from content
