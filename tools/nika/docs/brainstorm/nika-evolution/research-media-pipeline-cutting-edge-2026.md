# Research: Cutting-Edge Media Handling Patterns for AI Agent Systems in Rust

> Deep research on multimodal binary data patterns across Rust AI frameworks,
> DAG workflow engines, MCP protocol, and zero-copy async processing.
> Date: 2026-03-18 | Queries: 12 | Sources: 30+ | Agent: claude-opus-4-6

---

## Summary

The Rust AI agent ecosystem is rapidly maturing but **multimodal binary handling remains a gap**
in every Rust framework surveyed. Python frameworks (Flyte, Dagster, n8n, Dify) have converged on
a universal **reference-based two-tier architecture**: lightweight handles flow through the DAG,
binary bytes live in external storage. For Nika, this confirms the B+ design (MediaRef + CAS-ready
blake3 layout) is architecturally aligned with industry best practice. The key Rust-specific findings
are: blake3 at 6-8 GB/s single-threaded, `bytes` crate for zero-copy sharing, `HashingWriter` tee
pattern for streaming CAS writes, and ADK-Rust as the closest comparable Rust multimodal agent framework.

---

## 1. Rust AI Agent Frameworks with Multimodal Support

### Findings

Three Rust frameworks exist for AI agent workflows. None have production-grade multimodal pipelines yet:

| Framework | Crate | Multimodal Status | Key Feature |
|-----------|-------|-------------------|-------------|
| **ADK-Rust** | `adk` (lib.rs) | Early support: vision models via `adk_mistralrs`, real-time voice | Modular agent builder with `LlmAgentBuilder`, tools, memory |
| **rig-core** | `rig-core` (crates.io) | **No vision/multimodal input support**. Has `ImageGenerationModel` trait (output only) | Pipeline module for composable LLM chains |
| **GraphBit** | Hybrid Rust+Python | No explicit multimodal details | High-perf typed tool calls, deterministic workflows |

### ADK-Rust Architecture (closest comparable)

```
GitHub: https://github.com/zavora-ai/adk-rust
Crates: adk, adk-studio, adk-mistralrs

Agent Architecture:
  LlmAgentBuilder::new("agent")
      .instructions("...")
      .model(Arc::new(model))       // Supports vision models via MistralRs
      .add_tool(tool)               // Tools can process Vec<u8> media
      .build()?

Binary flow through tools:
  Agent receives multimodal prompt
      -> Calls tool with media bytes (Vec<u8> in ToolInput)
      -> Tool processes, returns results + optional media
      -> Agent synthesizes response with media references
```

### rig-core Limitations

rig-core v0.x (latest March 2026) focuses entirely on text:
- `CompletionModel` trait: text-only prompts
- No `generate_with_image()` or base64 image payload support
- `ImageGenerationModel` trait exists but is output-only (DALL-E style)
- To add vision: implement custom `CompletionModel` with provider-specific image handling
- Pipeline module could theoretically preprocess images but no built-in support

**Implication for Nika**: rig-core cannot handle multimodal inference natively. Nika's `infer:`
verb will need custom image attachment logic beyond rig's API, likely at the provider adapter level.

### Sources

- https://lib.rs/crates/adk
- https://github.com/zavora-ai/adk-rust
- https://rig.rs
- https://docs.rs/rig-core
- https://users.rust-lang.org/t/rust-for-ai-agents/136946

---

## 2. Multi-Agent Media Passing Patterns

### Findings

All major multi-agent frameworks (OpenAI Swarm, CrewAI, LangGraph) are **text-centric** with
no native binary media passing:

| Framework | Data Passed Between Agents | Media Strategy |
|-----------|---------------------------|----------------|
| **OpenAI Swarm** | Conversation messages (text) via handoff functions | None native; use tool APIs |
| **CrewAI** | Task outputs (text) via sequential controller injection | None native; file paths in context |
| **LangGraph** | Typed state dict (text/schema) via graph edges | Base64 strings in custom state channels |

### LangGraph Multimodal State Pattern

LangGraph is the only one with a documented pattern, but it is the **least scalable**:

```python
class MultimodalState(TypedDict):
    messages: Annotated[list, add_messages]
    image_data: str        # Base64 in state -- duplicated at every checkpoint
    generated_image: str   # Another base64 field

# Problems:
# - Full state serialized at each checkpoint including all base64 data
# - No streaming -- full buffer required before passing to next node
# - Memory pressure for large images (10MB image = 13.3MB base64)
```

### Universal Workaround Pattern

Every framework uses the same workaround for media between agents:

```
1. Agent A generates media -> stores to external storage (S3/filesystem)
2. Agent A returns reference (URL/path/ID) as text
3. Agent B receives reference as text input
4. Agent B fetches from storage when needed
```

**Implication for Nika**: The `with:` binding system already supports text references. Nika's
MediaRef pattern (file path or URI in task output, auto-resolved by downstream tasks) aligns
perfectly with how every multi-agent system handles media today.

### Sources

- https://ai.plainenglish.io/technical-comparison-of-autogen-crewai-langgraph-and-openai-swarm
- https://nuvi.dev/blog/ai-agent-framework-comparison-langgraph-crewai-openai-swarm
- https://www.ctipath.com/articles/ai-mlops/comparing-ai-multiagent-frameworks

---

## 3. Zero-Copy Async Media Processing in Rust

### `bytes` Crate -- The Foundation

The `bytes` crate is the standard for zero-copy binary sharing across async tasks:

```rust
use bytes::Bytes;

// O(1) clone via atomic reference counting -- same underlying buffer
let shared = Bytes::from(vec![0u8; 10_000_000]);  // 10MB image
let clone1 = shared.clone();  // Refcount increment, no memcpy
let clone2 = shared.clone();  // Same buffer, three owners

// Zero-copy slicing
let mut buf = Bytes::from(vec![0u8; 1024]);
let header = buf.split_to(64);    // header: [0..64], buf: [64..1024], zero-copy
let payload = buf.split_off(128); // Split remaining, still zero-copy
```

### Performance: `Bytes` vs `Vec<u8>`

| Operation | Bytes | Vec<u8> | Ratio |
|-----------|-------|---------|-------|
| Clone (10MB) | O(1), refcount inc | O(n), full memcpy | 10-100x faster |
| Slice | Zero-copy via split/split_to | Borrow (lifetime) or clone | Zero-copy wins |
| Memory (shared across 5 tasks) | 1x allocation | 5x allocation | 5x less memory |
| Async task sharing | Move/clone freely | Borrow checker fights | Much simpler API |

### AsyncWrite Limitation

Standard `tokio::io::AsyncWrite` **cannot do true zero-copy** because it accepts `&[u8]`:
- The `buf.copy_from(src)` call forces at least one copy
- Alternative: `futures::Sink<Bytes>` for owned-buffer passing
- For file writes, use `task::block_in_place` for blocking I/O to avoid tokio pool starvation

### Relevant Crates

| Crate | Purpose | Notes |
|-------|---------|-------|
| `bytes` 1.x | Zero-copy binary buffers | Foundation; used by tokio, hyper, tonic |
| `tokio-util` | Codec framework, framing | `BytesCodec`, `LengthDelimitedCodec` for streaming |
| `base64` 0.22 | Encode/decode base64 | Needed for MCP ImageContent/AudioContent |
| `memmap2` | Memory-mapped files | Zero-copy for same-process task communication |

### Implication for Nika

The MediaStore should accept and return `Bytes` (not `Vec<u8>`) for:
- Zero-copy sharing between concurrent tasks in `for_each:`
- Efficient slicing for header inspection (MIME sniffing)
- Reduced memory pressure when multiple tasks reference the same artifact

```rust
// Recommended Nika MediaStore signature
trait MediaStore: Send + Sync {
    async fn store(&self, data: Bytes, meta: MediaMeta) -> Result<MediaRef>;
    async fn retrieve(&self, handle: &MediaRef) -> Result<Bytes>;
    async fn stream(&self, handle: &MediaRef) -> Result<impl AsyncRead>;
}
```

### Sources

- https://github.com/Laugharne/rust_zero_copy
- https://users.rust-lang.org/t/what-does-the-bytes-crate-do/91590
- https://users.rust-lang.org/t/zero-copy-using-bytes/98143
- https://users.rust-lang.org/t/async-zero-copy-file-write-in-rust/110725
- https://oneuptime.com/blog/post/2026-01-25-infinite-data-streams-tokio-async-rust

---

## 4. DAG Binary Data Flow Patterns

### Industry Convergence

Every DAG-based workflow engine has converged on the same architecture:

```
+----------+     +----------+     +----------+
|  Task A  |     | Workflow |     |  Task B  |
| produces |---->| stores   |---->| consumes |
| binary   |     | reference|     | binary   |
+----------+     | only     |     +----------+
     |           +----------+          |
     v                                 v
+-------------------------------------------+
|        External Binary Store              |
|  (S3, filesystem, CAS, object store)      |
+-------------------------------------------+
```

### Approach Comparison

| Approach | Pros | Cons | Who Uses It |
|----------|------|------|-------------|
| **Inline base64** | Simple, no storage setup | 33% size inflation, memory hog, serialization bottleneck | LangGraph (worst practice) |
| **File references (URI)** | Low overhead, parallel-friendly, cacheable | Requires storage setup, potential consistency issues | Flyte, Dify, n8n |
| **Content-addressed storage (CAS)** | Deduplication, integrity, immutable | Hash computation cost | Temporal (DataDog codec), Bazel, Docker, Nix |
| **Shared memory** | Ultra-low latency, true zero-copy | Single-machine only, not durable | OpenVINO, Ray |

### Flyte's FlyteFile Pattern (Most Elegant)

```python
@task
def generate_image(prompt: str) -> FlyteFile:
    path = "/tmp/output.png"
    generate_to_file(prompt, path)
    return FlyteFile(path)  # Auto-uploaded to S3/GCS

@task
def analyze_image(image: FlyteFile) -> str:
    with open(image) as f:      # Auto-downloaded on access (lazy)
        return classifier.predict(f.read())
```

What flows between tasks: **~100 bytes metadata** (S3 URI, MIME type, hash), NOT the file.

### n8n's Pluggable BinaryData.Manager

```typescript
interface Manager {
    store(location, bufferOrStream, metadata): Promise<WriteResult>;
    getAsBuffer(fileId): Promise<Buffer>;
    getAsStream(fileId, chunkSize?): Promise<Readable>;
    getMetadata(fileId): Promise<Metadata>;
    copyByFileId(target, sourceId): Promise<string>;
}
// Modes: 'default' (in-memory), 'filesystem-v2', 's3'
```

Key design: `store()` accepts `Buffer | Readable` -- supports streaming without full buffering.

### Temporal's Transparent Codec (CAS)

```
Original payload:  { data: "...large binary..." }
                        |
                   PayloadCodec intercepts
                        |
After encoding:    { digest: "sha256:deadbeef", size: 1234567, key: "/blobs/sha256:deadbeef" }
```

Workflow code never sees the codec. Binary transparently offloaded when >128KB.

### Dagster's IOManager Pattern

```python
class S3BinaryIOManager(IOManager):
    def handle_output(self, context: OutputContext, obj: bytes):
        key = f"assets/{context.asset_key}/{context.run_id}.bin"
        self.s3.put_object(Bucket=self.bucket, Key=key, Body=obj)

    def load_input(self, context: InputContext) -> bytes:
        return self.s3.get_object(Key=context.metadata["s3_key"])['Body'].read()
```

### Implication for Nika

Nika's B+ design (MediaRef + CAS-ready blake3 layout) maps to **Pattern 1 (Handle/Reference) +
Pattern 2 (CAS) + Pattern 3 (Pluggable Storage)**. Specifically:

1. **MediaRef** = the lightweight handle flowing through DAG (like FlyteFile, n8n IBinaryData)
2. **blake3 hash** = content-addressed key (like Temporal's sha256 codec)
3. **MediaStore trait** = pluggable backend (like n8n's BinaryData.Manager, Dagster's IOManager)
4. **Lazy loading** = downstream tasks get MediaRef, fetch bytes only when needed (like Flyte)

### Sources

- https://docs.openvino.ai/2025/model-server/ovms_docs_dag.html
- https://dagster.io/learn/data-pipelines-examples
- https://docs-legacy.flyte.org/en/v1.13.1/concepts/data_management.html
- https://docs.dagster.io/guides/build/io-managers
- https://github.com/DataDog/temporal-large-payload-codec

---

## 5. BLAKE3 vs SHA-256 Performance for CAS

### Benchmark Summary

| Metric | BLAKE3 | SHA-256 (software) | SHA-256 (SHA-NI) | xxHash3 |
|--------|--------|---------------------|-------------------|---------|
| **Single-thread throughput (AVX2)** | 6-8 GB/s | 0.8 GB/s | ~3 GB/s | 31 GB/s |
| **Multi-threaded (16-core)** | 92 GB/s | N/A (single-thread only) | N/A | Scales well |
| **Small inputs (<4KB)** | 7-12 Gbps | 10.5 Gbps (faster!) | Higher | Fastest |
| **SIMD acceleration** | SSE2/SSE4.1/AVX2/AVX-512/NEON (auto-detect) | SHA-NI only (x86) | SHA-NI | SSE2/AVX2 |
| **Cryptographic** | Yes (256-bit) | Yes (256-bit) | Yes | **No** |
| **Rust crate** | `blake3` | `sha2` (rustcrypto) or `ring` | Same | `xxhash-rust` |

### Key Insight: Small Input Crossover

BLAKE3 is **slower than SHA-256 for inputs under ~4KB** due to Merkle tree overhead. For CAS
where payloads are always >1KB (images, audio), this crossover is irrelevant -- BLAKE3 wins
by 4-10x for typical media sizes.

### BLAKE3 Advantages for CAS

1. **Built-in parallelism**: Merkle tree construction enables `rayon` feature for multi-threaded hashing
2. **Variable output length**: 1 to 64+ bytes (can use 32-byte hash for CAS key)
3. **Streaming**: Incremental `Hasher::update()` -- perfect for hash-while-write
4. **SIMD auto-detection**: Runtime CPU detection, no compile-time flags needed
5. **Modern security**: Equivalent to SHA-256 with fewer rounds (7 vs 64)

### Recommendation for Nika

Use `blake3` crate with:
- `features = ["rayon"]` for parallel hashing of large files (>1MB)
- 32-byte output as hex string for CAS keys: `blake3-{hex}` prefix
- Streaming `Hasher` for hash-while-write pattern (see section 7)

For non-security checksums (e.g., cache invalidation), xxHash3 at 31 GB/s is an option,
but blake3 is already fast enough and provides cryptographic guarantees.

### Sources

- https://devtoolspro.org/articles/sha256-alternatives-faster-hash-functions-2025/
- https://mojoauth.com/compare-hashing-algorithms/sha-256-vs-blake3
- https://forum.solana.com/t/blake3-slower-than-sha-256-for-small-inputs/829
- https://kerkour.com/fast-secure-hash-function-sha256-sha512-sha3-blake3
- https://keypears.com/blog/2025-12-09-switching-to-sha256

---

## 6. MCP Binary Content Handling

### Protocol Content Types

The MCP spec (2025-06-18) defines a spectrum from fully inline to fully reference-based:

```
                    Inline data?    URI for re-fetch?    Size hint?
ImageContent         YES (required)  NO                   NO
AudioContent         YES (required)  NO                   NO
EmbeddedResource     YES (blob)      YES (required)       NO
ResourceLink         NO              YES (required)       YES (optional)
```

### rmcp Crate API (Rust SDK)

The official Rust MCP SDK (`rmcp`, github.com/modelcontextprotocol/rust-sdk) handles binary via:

```rust
// rmcp::model -- Content enum
pub enum RawContent {
    Text(RawTextContent),
    Image(RawImageContent),      // .data: String (base64), .mime_type: String
    Audio(RawAudioContent),      // .data: String (base64), .mime_type: String
    Resource(RawEmbeddedResource), // .resource: ResourceContents (text or blob)
    ResourceLink(RawResource),   // .uri, .name, .mime_type, .size (no inline data)
}

// Extraction methods on RawContent:
impl RawContent {
    pub fn as_text(&self) -> Option<&RawTextContent>       // <-- only one Nika calls
    pub fn as_image(&self) -> Option<&RawImageContent>     // <-- available, unused
    pub fn as_audio(&self) -> Option<&RawAudioContent>     // <-- available, unused
    pub fn as_resource(&self) -> Option<&RawEmbeddedResource> // <-- available, unused
    pub fn as_resource_link(&self) -> Option<&RawResource> // <-- available, unused
}
```

### MCP Image Generation Server Ecosystem

The MCP ecosystem has 40+ servers across 8 media categories. Common chaining patterns:

```
Pattern: Generate -> Store -> Analyze
  1. Agent calls MCP image gen tool -> server returns ImageContent (base64)
  2. Client stores to artifact store -> gets MediaRef
  3. Agent calls MCP vision tool with image reference -> gets text analysis
```

No MCP servers currently use ResourceLink for image output (too new, June 2025 spec).
Almost all return ImageContent with inline base64. This means **Nika must handle base64
decode + store as the primary ingestion path**.

### Implication for Nika

The two bottlenecks identified in doc 23 (rmcp_adapter.rs:352 and verbs.rs:918) are the
exact points where MCP binary content must be intercepted:

```
CURRENT: filter_map(|c| c.as_text()...)  -- only Text extracted
NEEDED:  match on Image/Audio/Resource/ResourceLink -> store to MediaStore -> emit MediaRef
```

### Sources

- https://modelcontextprotocol.io/specification/2025-11-25
- https://modelcontextprotocol.io/specification/2025-06-18/server/resources
- https://docs.rs/rmcp/latest/rmcp/model/index.html
- https://github.com/modelcontextprotocol/rust-sdk
- https://hackmd.io/@Hamze/S1tlKZP0kx

---

## 7. Streaming Hash-While-Write Pattern

### The HashingWriter Pattern

The core pattern for computing a blake3 hash simultaneously with writing to disk:

```rust
use tokio::io::AsyncWriteExt;
use blake3::Hasher;

pub struct HashingWriter<W: AsyncWriteExt + Unpin> {
    inner: W,
    hasher: blake3::Hasher,
    bytes_written: u64,
}

impl<W: AsyncWriteExt + Unpin> HashingWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            inner: writer,
            hasher: Hasher::new(),
            bytes_written: 0,
        }
    }

    pub async fn write_chunk(&mut self, buf: &[u8]) -> tokio::io::Result<()> {
        self.hasher.update(buf);           // CPU: ~6 GB/s (blake3)
        self.inner.write_all(buf).await?;  // IO: ~1 GB/s (SSD)
        self.bytes_written += buf.len() as u64;
        Ok(())
    }

    pub fn finalize(self) -> (blake3::Hash, u64) {
        (self.hasher.finalize(), self.bytes_written)
    }
}
```

### Streaming Download + Hash + Write

```rust
pub async fn ingest_binary(
    data: &[u8],       // Could be from base64 decode of MCP ImageContent
    store_path: &Path,
) -> Result<(blake3::Hash, u64)> {
    let file = tokio::fs::File::create(store_path).await?;
    let mut writer = HashingWriter::new(file);

    // Process in chunks for large payloads
    for chunk in data.chunks(8192) {
        writer.write_chunk(chunk).await?;
    }

    Ok(writer.finalize())
}
```

### Channel-Based Tee Pattern (for streaming sources)

For streaming from network while writing and hashing:

```rust
use tokio::sync::mpsc;

pub async fn stream_hash_write(
    mut reader: impl AsyncReadExt + Unpin,
    mut writer: impl AsyncWriteExt + Unpin,
) -> tokio::io::Result<blake3::Hash> {
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(16);  // Bounded for backpressure

    let producer = tokio::spawn(async move {
        let mut buf = vec![0; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => { tx.send(buf[..n].to_vec()).await.ok(); }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    });

    let mut hasher = blake3::Hasher::new();
    while let Some(chunk) = rx.recv().await {
        hasher.update(&chunk);
        writer.write_all(&chunk).await?;
    }

    writer.flush().await?;
    producer.await.unwrap()?;
    Ok(hasher.finalize())
}
```

### Performance Characteristics

| Aspect | Value | Notes |
|--------|-------|-------|
| Memory footprint | Constant (~8KB per chunk) | Regardless of file size |
| Hashing throughput | 6-8 GB/s (blake3 AVX2) | Far faster than disk I/O |
| Disk I/O | ~1-3 GB/s (NVMe SSD) | The actual bottleneck |
| Hash overhead | <5% of total write time | blake3 is essentially free vs disk |
| Chunk size sweet spot | 8192 bytes | Balances syscall overhead vs latency |
| Channel buffer | 10-16 items | Provides backpressure without stalling |

### Implication for Nika

The `HashingWriter` pattern is exactly what Nika's MediaStore needs for CAS:

```rust
// In media_store.rs
impl FilesystemMediaStore {
    pub async fn store(&self, data: Bytes, meta: MediaMeta) -> Result<MediaRef> {
        // 1. Write to temp file with HashingWriter
        let temp_path = self.base_path.join(format!(".tmp-{}", Uuid::new_v4()));
        let file = tokio::fs::File::create(&temp_path).await?;
        let mut writer = HashingWriter::new(file);

        for chunk in data.chunks(8192) {
            writer.write_chunk(chunk).await?;
        }

        let (hash, size) = writer.finalize();

        // 2. Rename to CAS path: .nika/media/ab/cd/abcdef1234...
        let hex = hash.to_hex();
        let cas_path = self.cas_path(&hex);
        tokio::fs::rename(&temp_path, &cas_path).await?;

        // 3. Return lightweight MediaRef
        Ok(MediaRef {
            hash: hex.to_string(),
            mime_type: meta.mime_type,
            size,
            path: cas_path,
        })
    }
}
```

### Sources

- https://leapcell.io/blog/efficiently-handling-large-files-and-long-connections-with-streaming-responses-in-rust-web-frameworks
- https://users.rust-lang.org/t/hash-files-in-parallel-using-asynchronous-filesystem-operations/76387
- https://www.youtube.com/watch?v=h-0KLCAEZgY (BLAKE3 for large file hashing in Rust)
- https://oneuptime.com/blog/post/2026-01-25-infinite-data-streams-tokio-async-rust

---

## 8. Crates Worth Evaluating

### Tier 1: Essential (already in plan)

| Crate | Version | Purpose | Status in Nika |
|-------|---------|---------|----------------|
| `blake3` | 1.x | Content-addressed hashing at 6-8 GB/s | Planned for MediaStore |
| `base64` | 0.22 | Decode MCP ImageContent/AudioContent | Already in dep tree via rmcp |
| `bytes` | 1.x | Zero-copy binary buffers across tasks | Already in dep tree via tokio |
| `rmcp` | 0.16+ | MCP binary content types (as_image, as_audio) | In use, but only as_text called |

### Tier 2: Should Evaluate

| Crate | Version | Purpose | Notes |
|-------|---------|---------|-------|
| `tokio-util` | 0.7 | `BytesCodec` for streaming frame encoding | Useful for chunked binary transfer |
| `memmap2` | 0.9 | Memory-mapped files for zero-copy same-process | Could eliminate copies for local media |
| `infer` | 0.16 | MIME type detection from magic bytes | Validate/detect MIME without trusting MCP server |
| `image` | 0.25 | Image format validation/conversion | Validate before storing; optional thumbnail gen |
| `tempfile` | 3.x | Secure temp files for atomic CAS writes | Already common pattern; ensure no partial writes |

### Tier 3: Future/Optional

| Crate | Purpose | When |
|-------|---------|------|
| `object_store` | Unified S3/GCS/Azure/local FS | When cloud storage backend needed |
| `rayon` | Parallel blake3 hashing for huge files | When handling video/large media (>100MB) |
| `xxhash-rust` | 31 GB/s non-crypto hash | Only if blake3 proves too slow (unlikely) |
| `opendal` | Unified data access layer | Alternative to object_store |

---

## 9. Architecture Synthesis: What This Means for Nika's Media Pipeline

### Validated Design Decisions

The B+ design from doc 23 is **strongly validated** by this research:

| B+ Design Decision | Industry Validation |
|--------------------|---------------------|
| MediaRef as lightweight handle | Flyte FlyteFile, n8n IBinaryData, Dify File, Temporal codec reference |
| blake3 CAS for deduplication | Temporal uses sha256 CAS; blake3 is 4-10x faster |
| Pluggable MediaStore trait | n8n BinaryData.Manager, Dagster IOManager, Dify storage backends |
| Base64 decode on ingestion | MCP ImageContent/AudioContent mandate base64; all frameworks decode immediately |
| Lazy loading (download on access) | Flyte FlyteFile.open(), MCP ResourceLink + resources/read |
| HashingWriter for streaming CAS | Standard Rust pattern; blake3 overhead <5% vs disk I/O |

### Gaps the Research Reveals

1. **rig-core has no multimodal support**: Nika must handle image attachments at the provider
   adapter level, not through rig's completion API. This affects the `infer:` verb for vision tasks.

2. **No Rust framework has solved this yet**: ADK-Rust is the closest but early-stage.
   Nika would be the first Rust workflow engine with a production CAS-based media pipeline.

3. **MCP ResourceLink is too new**: All existing MCP servers return ImageContent (inline base64),
   not ResourceLink. Nika's primary ingestion path must be base64 decode, with ResourceLink as
   a future optimization.

4. **`bytes` crate should be used throughout**: MediaRef's data should flow as `Bytes` (not
   `Vec<u8>`) for zero-copy sharing across concurrent for_each tasks.

5. **Small-input blake3 crossover**: For payloads <4KB, blake3 is slower than sha256. Since
   media payloads are always >1KB (smallest useful image is ~1KB), this is not a concern.
   However, the threshold for CAS vs inline could be set at ~4KB to avoid hashing tiny payloads.

### Recommended Implementation Priority

```
PR 1 (Capture):  Fix rmcp_adapter.rs + verbs.rs to capture binary content
                  Use existing rmcp as_image()/as_audio()/as_resource() methods
                  Decode base64 -> store to filesystem -> emit MediaRef in task output

PR 2 (Store):    Implement MediaStore trait with FilesystemMediaStore
                  HashingWriter for streaming CAS writes (blake3)
                  Content-addressed path layout: .nika/media/{prefix}/{hash}

PR 3 (Flow):     Wire MediaRef through binding system ({{with.task.media[0]}})
                  Support media: field in infer: verb for vision models
                  Artifact write integration for saving media to user-specified paths
```

---

## Methodology

- **Search tool**: Perplexity (sonar model), 12 queries across 8 topics
- **Pages analyzed**: 30+ source pages, documentation sites, and GitHub repositories
- **Existing research incorporated**: Docs 23 (media pipeline design) and 27 (binary flow patterns)
- **Cross-referenced**: Every pattern validated against 2+ independent sources
- **Time period**: 2025-2026 sources prioritized

## Confidence Level

**High** -- Core patterns (reference-based architecture, CAS, HashingWriter) are validated by
multiple production systems (n8n, Flyte, Temporal, Dagster). Blake3 benchmarks are from official
documentation and independent tests. The rmcp API is verified against docs.rs and the official
Rust SDK repository. The rig-core limitation is confirmed by both docs.rs and GitHub source.

## Further Research Suggestions

- **MCP Streamable HTTP transport**: Does the 2025-06-18 transport spec enable chunked binary
  streaming, avoiding base64 encode/decode entirely?
- **rmcp 1.2.0 changes**: What binary handling improvements exist in rmcp 1.2.0 vs 0.16.0?
  Worth upgrading before implementing the media pipeline?
- **Memory-mapped CAS**: Can `memmap2` be used for zero-copy reads from the CAS store when
  multiple tasks reference the same artifact in a single workflow run?
- **Thumbnail generation**: Should Nika auto-generate thumbnails for image artifacts? This
  would benefit the TUI preview and any future web UI.
- **Video streaming**: For large video outputs (>100MB), should Nika support chunked storage
  with parallel blake3 hashing via rayon?
