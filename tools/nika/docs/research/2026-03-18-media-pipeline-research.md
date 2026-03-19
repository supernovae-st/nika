# Research Report: Media Pipeline — MCP Binary Content, CAS, and Telemetry

**Date**: 2026-03-18
**Researcher**: Claude Opus 4.6 (1M context)
**Topics**: MCP binary servers, MCP spec content types, CAS best practices, Blake3 CAS, media telemetry

---

## Summary

This report consolidates research across five domains critical to Nika's media pipeline design: (1) which MCP servers produce binary output, (2) how the MCP spec defines binary content types, (3) how production workflow engines handle binary data, (4) Blake3-based content-addressable storage implementations, and (5) telemetry best practices for media processing. The findings directly inform the design of Nika's CAS store, media event system, and `ContentBlock` handling.

---

## 1. MCP Servers That Return Binary Content

### Confirmed Servers with Binary Output

| Server | Binary Type | Content Type Used | MIME Examples |
|--------|-------------|-------------------|---------------|
| **Stability AI** | Generated images | `ImageContent` | `image/png`, `image/jpeg` |
| **ElevenLabs** | TTS audio | `AudioContent` | `audio/mpeg`, `audio/wav` |
| **MiniMax MCP** | Images, TTS audio, video | `ImageContent`, `AudioContent` | `image/png`, `audio/mpeg`, `video/mp4` |
| **OpenAI GPT Image** (`SureScaleAI/openai-gpt-image-mcp`) | Generated/edited images | `ImageContent` | `image/png` |
| **Playwright MCP** | Screenshots, page captures | `ImageContent` | `image/png` |
| **Puppeteer MCP** | Screenshots | `ImageContent` | `image/png` |
| **Firecrawl** (screenshot feature) | Website screenshots | `ImageContent` | `image/png` |
| **EverArt** | AI-generated images | `ImageContent` | `image/png` |
| **AllVoiceLab** | Speech synthesis, video translation | `AudioContent` | `audio/mpeg` |
| **Audioscrape** | Audio retrieval | `AudioContent` | `audio/mpeg`, `audio/wav` |
| **Blender MCP** | Renders, 3D exports | `EmbeddedResource` | `image/png`, `model/gltf-binary` |
| **Manim MCP** (`abhiemj/manim-mcp-server`) | Mathematical animations | `ImageContent` | `image/png`, `video/mp4` |

### Servers That Use ResourceLink (External References)

| Server | Content | Notes |
|--------|---------|-------|
| **AWS S3 MCP** | Any binary from S3 buckets | Returns `ResourceLink` with S3 URIs |
| **Replicate MCP** | Model outputs | Typically returns URLs, not inline base64 |
| **Figma Context MCP** | Design assets | Layout info, sometimes screenshot links |

### Key Observations

- **Most image generation servers** return `ImageContent` with base64-encoded PNG data inline.
- **Audio servers** (ElevenLabs, AllVoiceLab) return `AudioContent` with base64 MP3/WAV.
- **Large binary outputs** (video, 3D models) tend to use `EmbeddedResource` or `ResourceLink` to avoid payload bloat.
- **No dedicated PDF generation MCP servers** were identified; PDF handling typically goes through `EmbeddedResource` with `application/pdf` MIME.
- **No Midjourney or Suno official MCP servers** exist yet (as of March 2026).

### Implications for Nika

Nika's media pipeline must handle ALL five content block types from MCP tool results. The most common real-world pattern is:
1. `ImageContent` from image generation tools (Stability AI, DALL-E, screenshots)
2. `AudioContent` from TTS tools (ElevenLabs)
3. `EmbeddedResource` for arbitrary binary (PDFs, 3D models)
4. `ResourceLink` for large outputs that servers host externally

Sources:
- https://mcpservers.org
- https://www.pulsemcp.com/servers (11,150+ servers indexed)
- https://github.com/punkpeye/awesome-mcp-servers
- https://github.com/modelcontextprotocol/servers

---

## 2. MCP Protocol Spec: Binary Content in Tool Results

### Spec Versions and Evolution

| Version | Date | Binary Content Changes |
|---------|------|----------------------|
| **2025-03-26** | Initial | `TextContent` only; no native binary support |
| **2025-06-18** | June 2025 | Added `ImageContent`, `EmbeddedResource`; base64 encoding for images |
| **2025-11-25** | November 2025 | Added `AudioContent`, `ResourceLink`; refined base64 rules |

### Content Type Union (2025-11-25 spec)

Tool results contain a `content: ContentBlock[]` array where each block is one of:

```typescript
type ContentBlock = TextContent | ImageContent | AudioContent | EmbeddedResource | ResourceLink;
```

### Exact Schemas

#### TextContent
```json
{
  "type": "text",
  "text": "string"
}
```
- Required fields: `type`, `text`
- No binary data; plain text or markdown

#### ImageContent
```json
{
  "type": "image",
  "mimeType": "image/png",
  "data": "<base64-encoded bytes>"
}
```
- Required fields: `type`, `mimeType`, `data`
- Supported MIME: `image/png`, `image/jpeg`, `image/gif`, `image/webp`, `image/svg+xml`
- `data` is standard base64 (RFC 4648)

#### AudioContent
```json
{
  "type": "audio",
  "mimeType": "audio/mpeg",
  "data": "<base64-encoded bytes>"
}
```
- Required fields: `type`, `mimeType`, `data`
- Supported MIME: `audio/mpeg`, `audio/wav`, `audio/ogg`, `audio/mp4`, `audio/flac`
- Added in 2025-11-25 spec revision

#### EmbeddedResource
```json
{
  "type": "resource",
  "resource": {
    "uri": "file:///path/to/file",
    "mimeType": "application/pdf",
    "text": "optional text content",
    "blob": "<base64-encoded binary>"
  }
}
```
- Required fields: `type`, `resource.uri`
- Optional: `resource.mimeType`, `resource.text`, `resource.blob`
- Generic container for ANY binary type (PDF, video, 3D models, etc.)
- Either `text` or `blob` should be present (not both typically)

#### ResourceLink
```json
{
  "type": "resource_link",
  "uri": "https://example.com/file.pdf",
  "mimeType": "application/pdf",
  "title": "Optional title",
  "description": "Optional description"
}
```
- Required fields: `type`, `uri`
- Optional: `mimeType`, `title`, `description`
- **No inline content** -- client must fetch via URI
- Added in 2025-11-25 spec for lazy-loading large resources

### Base64 Encoding Requirements

- Standard RFC 4648 base64 encoding
- Size limits negotiated via MCP capabilities (default max ~10MB per block)
- Data URIs (`data:{mimeType};base64,{data}`) are optional for inline rendering

### Nika's Current ContentBlock (aligned)

Nika's `ContentBlock` enum in `src/mcp/types.rs` already covers all five types:
- `Text { text }` -- matches `TextContent`
- `Image { data, mime_type }` -- matches `ImageContent`
- `Audio { data, mime_type }` -- matches `AudioContent`
- `Resource(ResourceContent)` -- matches `EmbeddedResource`
- `ResourceLink { uri, name, mime_type }` -- matches `ResourceLink`

Sources:
- https://modelcontextprotocol.io/specification/2025-06-18
- https://modelcontextprotocol.io/specification/2025-11-25/changelog
- https://github.com/modelcontextprotocol/modelcontextprotocol (schema.ts)

---

## 3. CAS Best Practices in Workflow Engines

### 3.1 Temporal.io — Binary Payload Handling

**Architecture**: Temporal stores workflow state in a durable execution log. Binary payloads go through a `DataConverter` -> `PayloadConverter` -> `PayloadCodec` pipeline.

**Key design decisions**:
- **2MB hard limit per payload** -- enforced by Temporal Platform
- **50MB total workflow history limit** -- multiple 1.5MB payloads risk termination
- **Recommended pattern**: Store large blobs externally (S3) and pass lightweight references (keys/URIs) in payloads
- **Codec server** can intercept payloads for compression/encryption, but impacts replay performance
- **Custom PayloadConverter** handles non-JSON types (binary blobs), but must be deterministic for workflow replay

**Media processing pattern** (from Temporal's blog):
```
Activity 1: Download media → store in S3 → return S3 key
Activity 2: Receive S3 key → fetch → transcode → store result → return new key
Activity 3: Receive key → post-process → return final URI
```

**Heartbeats**: Long-running media activities (transcoding) use periodic heartbeats to:
- Report progress (e.g., 45% encoded)
- Extend activity timeout
- Enable graceful restart on failure (resume from last heartbeat checkpoint)

**Retry policies**: Exponential backoff with max retries; retries inherit last heartbeat state.

**Relevance to Nika**: Nika should similarly avoid storing base64 blobs inline in event logs. Pass CAS keys (Blake3 hashes) as references between tasks.

Source: https://temporal.io/blog/media-processing-workflows

### 3.2 n8n — Binary Data Between Nodes

**Architecture**: n8n uses a `$binary` object attached to workflow items. Binary data is stored alongside JSON metadata.

**Key design decisions**:
- **IBinaryData interface**: Stores `fileName`, `mimeType`, `fileSize`, and the actual data
- **Storage backends**: Configurable -- filesystem (default), S3, Google Cloud Storage
- **Reference-based passing**: In production, binary data is uploaded to external storage; nodes pass references (IDs/URLs) not raw buffers
- **Selective forwarding**: Not all nodes pass binary data forward -- some require explicit Merge nodes to re-attach binary from earlier nodes
- **Branching**: Binary data is available in both branches after a fork
- **Max 10 files** per workflow item in Dify (n8n has no hard limit but recommends external storage for large files)

**Dedicated binary nodes**:
- `Convert to File` -- serialize data to binary
- `Extract From File` -- parse binary to structured data
- `Read/Write Files from Disk` -- filesystem I/O (self-hosted only)
- `Compression` -- zip/unzip
- `Edit Image` -- image manipulation

**Relevance to Nika**: The n8n pattern of "upload binary, pass reference" maps well to CAS. Nika's `with:` bindings could reference CAS hashes instead of raw data.

### 3.3 Dify — Media File Handling

**Architecture**: Dify uses a `file` variable type representing uploaded files, passed as arrays between workflow nodes.

**Key design decisions**:
- **File references, not base64**: Dify stores files in backend storage and passes metadata/URLs via variables
- **Type-based routing**: `type in image` or `type in doc` filters for node routing
- **Doc Extractor node**: Converts documents, audio, and video to text for LLM consumption
- **Vision LLM nodes**: Accept image file variables directly
- **Max 10 files** per workflow
- **Storage backends**: Built-in storage + custom integrations (Fast.io, S3-compatible)

**Data flow pattern**:
```
User Upload → Start Node → Filter by Type
  ├── Images → Vision LLM (direct)
  └── Docs/Audio/Video → Doc Extractor → text → LLM/Tool
```

**Relevance to Nika**: Dify's type-based routing is similar to what Nika needs for `ContentBlock` variants. The "extract text from media" pattern maps to Nika's potential media processors.

### 3.4 General CAS Best Practices

| Practice | Description |
|----------|-------------|
| **Content-addressed keys** | Use cryptographic hash of content as storage key (deduplication is automatic) |
| **External blob storage** | Never inline large binaries in workflow state/history |
| **Reference passing** | Tasks exchange hash/URI references, not raw bytes |
| **Garbage collection** | Implement retention policies; track reference counts; archive old data |
| **Federated access** | Support multiple storage backends (local, S3, OCI registries) |
| **Metadata tagging** | Store MIME type, size, creation time alongside content |
| **Immutable writes** | Once stored, content never changes (content-addressed = immutable) |
| **Streaming ingestion** | Hash content as it streams in; avoid buffering entire blob in memory |
| **Version history** | Track which workflow runs produced/consumed each blob |

Sources:
- https://chainloop.dev/blog/announcing-federated-content-addressable-storage-cas/
- https://temporal.io/blog/media-processing-workflows

---

## 4. Blake3 CAS Implementations

### 4.1 Blake3 Rust Crate

The official `blake3` crate (80M+ downloads) is the foundation:

```rust
// Hash content to get CAS key
let hash = blake3::hash(content_bytes);
let cas_key = hash.to_hex(); // 64-char hex string

// Incremental hashing for streaming
let mut hasher = blake3::Hasher::new();
hasher.update(chunk1);
hasher.update(chunk2);
let hash = hasher.finalize();

// Parallel hashing with rayon
hasher.update_rayon(large_content); // Automatic SIMD + multithreading
```

**Features**:
- SIMD optimized (SSE2, AVX2, AVX-512, NEON)
- Tree hashing mode for parallelism
- Keyed hashing for domain separation
- 32-byte output (256-bit security)

### 4.2 BAO — Blake3 Authenticated Output

The `bao` crate enables streaming verification of CAS content:

```rust
// Encode file with BAO for streaming verification
let (encoded, hash) = bao::encode::encode(content);

// Verify arbitrary byte ranges without full re-hash
let slice = bao::decode::decode_slice(&encoded, start..end, &hash)?;
```

**Use case**: Verify integrity of partial downloads from CAS. Fetch a 10KB range from a 1GB video and verify it matches the root hash.

Source: https://github.com/oconnor663/bao

### 4.3 XET Protocol — Blake3 CAS at Scale (IETF Draft)

The **XET protocol** (IETF draft-denis-xet-03, December 2025) is the most complete Blake3 CAS specification:

**Algorithm suite**: `XET-BLAKE3-GEARHASH-LZ4`
- **Blake3** for all cryptographic hashing (keyed mode for domain separation)
- **Gearhash** for content-defined chunking (deterministic boundaries)
- **LZ4** for chunk compression

**Key concepts**:
- **Chunks**: Max 128 KiB uncompressed, split at content-defined boundaries
- **Xorbs**: Serialized containers of compressed chunks (storage units)
- **Shards**: Groups of xorbs for batch operations
- **Terms**: Ordered references to chunks/xorbs for file reconstruction

**Deduplication**: Chunk-level via Blake3 hash identity. Identical chunks across files stored once.

**Merkle trees**: Xorb hash = Blake3 Merkle root of chunk tree. Enables partial verification.

**Wire protocol**: HTTP with pre-signed URLs; reconstruction queries return terms + fetch info.

**Relationship to Hugging Face**: XET appears related to HF's internal storage system for large model files, though the IETF draft is an independent submission.

Source: https://www.ietf.org/archive/id/draft-denis-xet-02.html

### 4.4 Existing Rust CAS Crates

| Crate | Hash Algorithm | Notes |
|-------|---------------|-------|
| `lago-store` | SHA-256 | Content-addressed blob storage with zstd compression |
| `focuson_cas` | Configurable | Filesystem-based CAS with `ContentAddressableStorage` trait |
| `cacache` | SHA-256 (sri) | npm-style content-addressable cache, mature |
| `iroh` | BLAKE3 | IPFS reimplementation by n0; uses Blake3 for content addressing |
| `bao` | BLAKE3 | Streaming verification, not full CAS |

**`iroh` is notable**: It is a full IPFS reimplementation in Rust using Blake3 instead of SHA-256. However, it is a complex networking stack, not a simple embedded CAS.

### 4.5 Blake3 vs SHA-256 for CAS

| Aspect | Blake3 | SHA-256 |
|--------|--------|---------|
| **Speed** | ~10x faster on modern CPUs | Baseline |
| **Parallelism** | Native tree hashing, multithreaded with rayon | Sequential only |
| **SIMD** | SSE2, AVX2, AVX-512, NEON | Limited SIMD in some implementations |
| **Output size** | 32 bytes (256-bit) | 32 bytes (256-bit) |
| **Security** | Equivalent collision resistance | Well-proven standard |
| **Streaming verify** | BAO for Merkle proofs | No standard equivalent |
| **CAS fit** | Superior for large blobs, chunked storage | Adequate, widely deployed |

**Recommendation**: Blake3 is the clear choice for a new CAS implementation. It is faster, supports parallelism, and has BAO for streaming verification. The `blake3` Rust crate is the de facto standard.

---

## 5. Media Pipeline Telemetry Best Practices

### 5.1 Core Metrics to Track

#### Throughput Metrics
| Metric | Type | Description |
|--------|------|-------------|
| `media.bytes_processed` | Counter | Total bytes ingested/processed |
| `media.blobs_stored` | Counter | Number of blobs written to CAS |
| `media.blobs_deduplicated` | Counter | Number of blobs that were already in CAS (dedup hits) |
| `media.frames_processed` | Counter | For video: frames processed |
| `media.tasks_completed` | Counter | Media-related tasks completed |

#### Latency Metrics
| Metric | Type | Description |
|--------|------|-------------|
| `media.processing_latency_ms` | Histogram | End-to-end media processing time |
| `media.cas_write_latency_ms` | Histogram | Time to write blob to CAS store |
| `media.cas_read_latency_ms` | Histogram | Time to read blob from CAS store |
| `media.base64_decode_latency_ms` | Histogram | Time to decode MCP base64 payloads |
| `media.hash_latency_ms` | Histogram | Time to compute Blake3 hash |

#### Storage Metrics
| Metric | Type | Description |
|--------|------|-------------|
| `media.blob_size_bytes` | Histogram | Distribution of blob sizes |
| `media.cas_hit_rate` | Gauge | CAS cache hit ratio (0.0-1.0) |
| `media.dedup_ratio` | Gauge | Deduplication ratio (unique/total) |
| `media.store_size_bytes` | Gauge | Total CAS store size on disk |
| `media.memory_pressure_bytes` | Gauge | Memory used for in-flight media blobs |

#### Error Metrics
| Metric | Type | Description |
|--------|------|-------------|
| `media.errors_by_type` | Counter (labeled) | Errors grouped by `{media_type, error_kind}` |
| `media.decode_failures` | Counter | Base64 decode failures |
| `media.hash_mismatches` | Counter | CAS integrity verification failures |
| `media.oversized_blobs` | Counter | Blobs exceeding size limit |

### 5.2 Recommended Tracing Spans

```
workflow_run
  └── task_execute (task_id, verb)
        └── mcp_tool_call (server, tool_name)
              └── media_extract (content_type, mime_type)
                    ├── base64_decode (bytes_decoded)
                    ├── blake3_hash (hash, elapsed_ms)
                    ├── cas_lookup (hash, hit: bool)
                    └── cas_write (hash, bytes_written)
```

### 5.3 Rust Tracing Integration Pattern

Using `tracing` crate with OpenTelemetry export:

```rust
use tracing::{instrument, event, Level, Span};

#[instrument(skip(data), fields(
    media.mime_type = %mime_type,
    media.bytes_in = data.len(),
    media.hash,
    media.cas_hit
))]
async fn store_media_blob(data: &[u8], mime_type: &str) -> Result<String, Error> {
    // Compute hash
    let hash = blake3::hash(data);
    let hash_hex = hash.to_hex().to_string();
    Span::current().record("media.hash", &hash_hex.as_str());

    // Check CAS
    if cas_store.exists(&hash_hex).await? {
        Span::current().record("media.cas_hit", true);
        event!(Level::DEBUG, cas.operation = "hit");
        return Ok(hash_hex);
    }

    Span::current().record("media.cas_hit", false);
    event!(Level::DEBUG, cas.operation = "miss");

    // Write to CAS
    cas_store.write(&hash_hex, data).await?;
    event!(Level::INFO, cas.operation = "write", bytes = data.len());

    Ok(hash_hex)
}
```

### 5.4 Event Recording for Nika's EventKind

Recommended new `EventKind` variants for media telemetry:

```
MediaStored { hash, mime_type, size_bytes, was_dedup }
MediaResolved { hash, mime_type, size_bytes }
MediaError { hash, error_code, error_message }
```

These complement the existing event system and enable:
- Trace replay showing media flow through the DAG
- Deduplication statistics per workflow run
- Error diagnosis by media type

### 5.5 Priority Order

Start with reliability metrics (error rates, decode failures), then storage metrics (CAS hits, dedup ratio), then performance metrics (latency histograms, throughput).

Sources:
- https://temporal.io/blog/media-processing-workflows
- https://latenode.com/blog/workflow-automation-business-processes/automation-roi-metrics/top-metrics-for-workflow-performance-monitoring
- OpenTelemetry semantic conventions for media processing

---

## Confidence Level

**High** -- Findings are cross-referenced across multiple authoritative sources (MCP spec, Temporal docs, IETF drafts, crates.io). The MCP content type schemas are well-documented. Blake3 performance claims are verified by the official BLAKE3 team benchmarks.

**Medium** for MCP server inventory -- The MCP ecosystem is growing rapidly (11,000+ servers on PulseMCP). Some servers listed may have changed their output format. No Midjourney or Suno MCP servers were confirmed.

---

## Methodology

- **Tools used**: Perplexity AI (sonar-pro model), source code analysis
- **Queries executed**: 9 Perplexity searches across all 5 topics
- **Sources analyzed**: 40+ URLs including spec docs, GitHub repos, blog posts, IETF drafts
- **Cross-reference**: MCP spec changes verified against changelog; Blake3 benchmarks from official repo
- **Nika codebase**: Verified `ContentBlock` enum alignment with MCP spec

---

## Further Research Suggestions

1. **Scrape rmcp 0.16 docs.rs** for exact Rust type mappings to confirm `Content` / `RawContent` enum structure
2. **Benchmark Blake3 vs SHA-256** for Nika's expected blob sizes (1KB-10MB from MCP tools)
3. **Evaluate iroh's CAS layer** for potential reuse (n0-computer/iroh) -- may be too heavy
4. **Survey ElevenLabs MCP server** source code for exact AudioContent format
5. **Design CAS garbage collection** strategy -- reference counting vs. TTL vs. workflow-scoped
6. **Investigate BAO integration** for streaming verification of large media blobs
7. **Profile memory pressure** of base64 decoding MCP payloads (base64 is ~33% larger than raw)
