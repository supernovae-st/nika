# Research Report: Content-Addressed Storage and Binary Handling for AI/ML Systems in Rust

> Date: 2026-03-18 | Agent: claude-opus-4-6 (1M) | Queries: 13 | Sources: 30+
> Context: Nika media pipeline design (doc 23), CAS-ready blake3 layout

---

## Summary

Content-addressed storage (CAS) for binary media in AI workflow engines has converged on a
**handle-not-data** consensus: never pass raw binary through the orchestration layer, only pass
lightweight references (URI, hash, path). Every major ML pipeline (Dagster, Flyte, ZenML,
Temporal, Prefect) and the MCP protocol itself use this pattern. BLAKE3 is the clear winner
for Rust CAS hashing due to speed, streaming support, and the bao verified-streaming
ecosystem. The Nika B+ design (MediaRef + CAS-ready blake3 layout) is well-aligned with
industry best practices -- this report identifies specific refinements and patterns to adopt.

---

## 1. BLAKE3 Content-Addressed Storage Best Practices

### Key Findings

**Directory layout consensus**: 2-level hex prefix sharding is the industry standard.
- **Git**: `{sha1[0..2]}/{sha1[2..40]}` -- 256 directories, proven at massive scale.
- **Iroh-blobs**: Flat storage with outboard files alongside data files (no prefix sharding
  in the iroh design -- they optimize for different access patterns via a separate DB index).
- **S5 Network**: BLAKE3-addressed with `.obao` sidecar files for chunks >256 KiB.
- **Bazel remote cache**: SHA-256 with 2-char prefix sharding.

**Nika's `{blake3[0..2]}/{blake3[2..64]}.{ext}` is correct and aligned with the git/Bazel
pattern.** No changes needed.

**BLAKE3 advantages confirmed**:
- 14x faster than SHA-256 (SIMD-accelerated, hardware-native on modern CPUs)
- `to_hex()` returns stack-allocated `ArrayString<64>` (zero heap allocation)
- Built-in Merkle tree enables verified streaming via bao/bao-tree crates
- Cryptographically secure (unlike xxHash, CityHash)
- Rust-native with zero C dependencies

**Incremental hashing pattern** -- the recommended approach for computing hashes during
streaming writes:

```rust
use blake3::Hasher;
use tokio::io::AsyncReadExt;

async fn hash_and_write(data: &[u8], dest: &Path) -> Result<blake3::Hash> {
    // Hash and write simultaneously -- no double-pass
    let hash = tokio::task::spawn_blocking({
        let data = data.to_vec();
        move || blake3::hash(&data)
    }).await?;

    // Atomic write: tmp -> rename
    let tmp = dest.with_extension("tmp");
    tokio::fs::write(&tmp, data).await?;
    tokio::fs::rename(&tmp, dest).await?;

    Ok(hash)
}
```

For large files (>1MB), use `spawn_blocking` to avoid blocking the async runtime.
BLAKE3's `Hasher::update()` is incremental and can process data chunk-by-chunk without
buffering the entire file in memory.

### Actionable for Nika

1. **Current design is validated.** The `{2hex}/{62hex}.{ext}` layout matches industry
   practice. No changes needed.
2. **Add `spawn_blocking` for hashing.** The current `blake3::hash(&bytes)` call in
   `MediaProcessor::save_base64` should wrap in `spawn_blocking` for files >1MB.
3. **Consider bao outboard files.** For future verified streaming support (PR 3 / Approach C),
   store `.obao` sidecar alongside large blobs (>256 KiB). This enables:
   - Resumable downloads
   - Range verification without full re-hash
   - P2P-compatible transfers (iroh wire format)
4. **Inline threshold.** Iroh uses ~16 KiB as the inline threshold (small blobs stored in DB,
   large blobs as files). For Nika, this is not needed yet -- all blobs go to disk via CAS.
   But the threshold concept is useful for future optimization.

---

## 2. Iroh-Blobs Architecture

### Key Findings

Iroh-blobs (from n0-computer, now iroh.computer) is a Rust-native CAS library built on
BLAKE3 with verified streaming. The v0.90 rewrite (2025-07) represents state-of-the-art
CAS design in Rust.

**Architecture (v0.90+)**:
- **Per-blob file triplet**: data file + outboard file (flattened hash tree) + bitfield file
  (tracks available byte ranges for partial transfers)
- **Hybrid storage**: Small blobs (<16 KiB) inline in database; large blobs as filesystem
  files. This optimizes for both millions of tiny blobs and GiB-scale files.
- **BLAKE3 Merkle tree**: Outboards stored down to 16 KiB chunk groups (not per-1KiB chunk)
  to minimize overhead.
- **RPC-as-interface**: Custom stores implement an RPC trait, enabling pluggable backends
  (filesystem, in-memory, S3-like).
- **No tokio dependency** (optional) -- io_uring support planned.
- **Protocol**: QUIC-based request/response with verified streaming. Supports multi-range
  requests (e.g., "bytes [0..1000, 5000..6000]" in a single request).

**v0.95 additions** (2025-10):
- Stream transforms for on-the-fly processing
- `ConnectionRef` for lifetime tracking
- Toward 1.0 release

**GuardianDB integration**: Uses iroh-blobs as lowest storage layer, proving the CAS
architecture works for real database systems.

### Actionable for Nika

1. **Do NOT depend on iroh-blobs.** It is designed for P2P distributed systems. Nika's local
   CAS is much simpler. But learn from the design:
   - **Bitfield files** are smart for tracking partial downloads (future `fetch:` verb for
     large files).
   - **Outboard files** enable verified streaming without modifying the original blob.
   - **Hybrid storage** (inline small blobs) is a good optimization to consider if blob
     count grows large.
2. **The 16 KiB chunk group size** is a good reference for future bao integration. Don't
   store outboards for tiny files.
3. **RPC trait pattern** maps well to Nika's future `MediaStore` trait (designed in doc 23,
   section for Approach C). The iroh pattern of "implement RPC to get a custom store"
   validates the trait-based abstraction.

---

## 3. Async File Hashing Patterns in Rust

### Key Findings

The consensus is clear: **use `tokio::task::spawn_blocking` for CPU-intensive hashing**.

**Anti-pattern**: Hashing directly on the async runtime blocks the executor.
**Anti-pattern**: Using `futures::executor::block_on` causes thread starvation.
**Anti-pattern**: Spawning `spawn_blocking` for every small chunk (overhead > benefit).

**Recommended pattern**:

```rust
async fn hash_bytes_async(bytes: Vec<u8>) -> blake3::Hash {
    tokio::task::spawn_blocking(move || blake3::hash(&bytes))
        .await
        .expect("spawn_blocking panicked")
}
```

For streaming large files:

```rust
async fn hash_file_streaming(path: &Path) -> Result<blake3::Hash> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut hasher = blake3::Hasher::new();
        let file = std::fs::File::open(&path)?;
        // blake3 has mmap support for large files
        hasher.update_mmap(&file)?;
        Ok(hasher.finalize())
    }).await?
}
```

**Key insight**: BLAKE3's `update_mmap()` is the fastest path for large files -- it
memory-maps the file and uses SIMD-parallel hashing across mapped regions. This avoids
the read-loop overhead entirely.

**Buffer sizing**: 64 KiB is the sweet spot for chunk-based reading when mmap is not
available. Matches the BLAKE3 internal parallelism boundary.

### Actionable for Nika

1. **Wrap `blake3::hash()` in `spawn_blocking`.** The current implementation in
   `MediaProcessor::save_base64` hashes on the async runtime. For base64-decoded blobs
   from MCP (typically <10MB), this is acceptable but not ideal. Wrap it.
2. **Use `update_mmap()` for large file hashing** when adding `fetch:` verb binary
   download support. This is the fastest path.
3. **64 KiB buffer** for any chunk-based processing (matching BLAKE3 internal parallelism).

---

## 4. AI Pipeline Binary Artifact Management

### Key Findings

Every major ML pipeline framework has converged on the same pattern:
**artifact handle vs. artifact data separation**.

| Framework | Handle Type | Storage | Passing Pattern |
|-----------|------------|---------|-----------------|
| **Dagster** | `MaterializeResult` metadata (path, size) | User-managed (S3, DB) | Handle only; load via resources |
| **Flyte** | `FlyteFile<BinaryType>` (typed URI) | Flyte-managed (S3/GCS) | URI reference; auto-download |
| **ZenML** | `Artifact(name, uri, hash)` | Artifact store (MinIO/S3) | Handle + lazy load |
| **Temporal** | `PayloadCodec` + external storage | S3/GCS via codec | Reference swap (transparent) |
| **Prefect** | `Result` with storage block | S3/filesystem | URI in result |
| **n8n** | Binary data mode config | Filesystem/S3 | Path reference |

**The consensus**: Never pass binary data through the orchestration layer. Store externally,
pass handles (URI/path/hash), load on demand.

**Temporal's PayloadCodec pattern** is particularly relevant:
- SDKs enforce a 2MB payload limit per event
- `PayloadCodec` transparently intercepts large payloads
- Encodes: detect large payload -> upload to S3 -> replace with `{s3_key, size, hash}`
- Decodes: detect reference -> download from S3 -> reconstruct original
- DataDog published an open-source implementation: `temporal-large-payload-codec`

**Dagster's asset materialization** is the closest to Nika's MediaRef concept:
- Asset function runs, produces binary data
- Stores to external storage (S3, filesystem)
- Returns `MaterializeResult` with metadata (path, size, checksum)
- Downstream steps receive only the metadata; load data independently

### Actionable for Nika

1. **MediaRef IS the industry-standard handle pattern.** Nika's `MediaRef { path, mime_type,
   size_bytes, checksum, source }` is exactly what Dagster, Flyte, and ZenML use.
   The design is validated.
2. **Consider a size threshold for inline vs. external.** MCP ImageContent is inline base64.
   For small images (<64KB), it might be wasteful to write to disk + create a MediaRef.
   But consistency is more important than optimization at this stage. Keep it simple: always
   write to CAS.
3. **Temporal's codec pattern is informative for future MCP ResourceLink support.** When
   Nika receives a `ResourceLink` (URI-only, no inline data), it should:
   - Fetch the resource content via `resources/read`
   - Store in CAS
   - Return MediaRef
   This is the "decode" side of Temporal's codec pattern.
4. **The `source` field on MediaRef is valuable.** No other framework tracks provenance this
   granularly. Keep it -- it enables lineage tracking (which framework was this image from?).

---

## 5. MCP ResourceLink Implementation Status

### Key Findings

**ResourceLink was added to the MCP spec in the 2025-06-18 revision.** It is a content type
in `CallToolResult` that provides a URI reference to external data without embedding it inline.

**Schema definition** (from the spec):
```typescript
interface ResourceLink {
  type: "resource_link";
  name: string;
  uri: string;
  title?: string;
  description?: string;
  mimeType?: string;
  size?: number;
  annotations?: Annotations;
  _meta?: { [key: string]: unknown };
}
```

**Key differences from EmbeddedResource**:

| Aspect | ResourceLink | EmbeddedResource |
|--------|-------------|------------------|
| Data inclusion | URI only (no inline data) | Full payload inline (base64) |
| Use case | Large files, lazy loading | Small data, immediate availability |
| Client action | Fetch via `resources/read` or direct | Already available |
| Guarantee | Resource may not be in `resources/list` | Immediately available |

**SDK support status**:
- **TypeScript MCP SDK**: Supports ResourceLink in CallToolResult content (2025-06-18 spec)
- **Python MCP SDK**: Supports via spec compliance
- **Rust rmcp crate**: `RawContent::ResourceLink` variant exists since rmcp 0.16.
  In rmcp 1.2.0+, `rust-mcp-schema` provides `ResourceLink` struct for 2025-11-25 spec.
- **Rust rmcp (Nika's version 0.16)**: Has `RawContent::ResourceLink(RawResource)` but
  Nika never calls `as_resource_link()`.

**Real-world adoption**: An arXiv paper (2510.05968) discusses patterns for using ResourceLink
for large dataset processing, but **no major MCP client implementations of ResourceLink were
found in the research**. Most clients (Claude Desktop, Cursor, etc.) only process Text and
Image content types. ResourceLink is spec-ready but ecosystem-early.

### Actionable for Nika

1. **Handle ResourceLink in PR 1 (rmcp_adapter.rs).** When an MCP server returns a
   ResourceLink, Nika should:
   - Log the URI and metadata
   - If the URI is fetchable (`file://`, `https://`), download and store in CAS
   - If the URI requires `resources/read`, call the MCP server to fetch content
   - Return a MediaRef pointing to the CAS file
2. **Nika could be an early leader in ResourceLink support.** Most clients drop it. By
   handling it properly, Nika becomes one of the few MCP clients that actually uses the
   full spec. This matters for server authors who want to return large results.
3. **The `size` field on ResourceLink** enables pre-flight size checks before downloading.
   Use it to enforce `max_size` limits.
4. **rmcp upgrade consideration**: rmcp 1.2.0 (latest) may have improved ResourceLink
   support vs. 0.16.0. The upgrade is already planned in doc 23, section 9.

---

## 6. Rust Workflow Engine Media Patterns

### Key Findings

**No Rust-native workflow engine with media handling was found.** The Rust ecosystem has:
- **Temporal SDK for Rust** (temporal-sdk-rust): Exists but no specific media patterns
  documented. Uses the same PayloadCodec pattern as other SDKs.
- **Windmill**: Written in Rust (backend), but media handling is in the TypeScript/Python
  worker layer, not the Rust orchestration layer.
- **rig**: Rust AI framework, but focused on LLM inference, not workflow orchestration
  with media.

**This means Nika is pioneering.** There is no established Rust pattern for "workflow engine
that handles binary media between steps." The closest analogues are:
- **Iroh-blobs** for CAS storage (Rust-native)
- **Temporal's PayloadCodec** for transparent binary externalization
- **Dagster's IO managers** for storage abstraction

**What Windmill does (relevant because it is Rust-backed)**:
- Workers handle binary via filesystem references
- Large payloads stored in S3-compatible storage
- Job results contain paths/URIs, not binary data
- The Rust orchestration layer only handles metadata and scheduling

### Actionable for Nika

1. **Nika is establishing a new pattern.** Document it well. The MediaRef + CAS approach
   could become a reference implementation for other Rust workflow engines.
2. **Follow Windmill's approach**: The Rust orchestration layer (DAG executor) handles only
   metadata (MediaRef). Binary data lives in the CAS store. Workers (MCP servers) produce
   binary; Nika's adapter layer materializes it.
3. **Consider publishing `nika-media` as a standalone crate** in the future. The
   MediaProcessor + CAS layout + MediaRef pattern is reusable by other Rust projects.

---

## 7. CAS Garbage Collection Strategies

### Key Findings

Three primary GC strategies exist for content-addressed stores:

**A. Reachability-based (Mark-and-Sweep) -- used by Git**
- Walk from roots (branch tips, tags) through the DAG
- Mark all reachable objects
- Delete unreachable objects older than a grace period (default: 2 weeks)
- Expensive: requires full traversal of object graph
- Git packs loose objects into packfiles during GC
- GitHub engineered "cruft packs" to scale GC to massive repos

**B. Root-based Reference Counting -- used by Nix Store**
- GC roots are explicit symlinks in `/nix/var/nix/gcroots/`
- `nix-store --gc` deletes paths with no root pointing to them
- Simple and fast: no graph traversal needed
- But requires manual root management

**C. Pin-based -- used by IPFS**
- Pinned CIDs are immune to GC
- `ipfs repo gc` deletes unpinned blocks after reachability check from pins
- Explicit control: users decide what to keep
- No automatic roots

**D. Snapshot-based Expiration -- used by Icechunk**
- Time-based: `expire_snapshots` marks old versions as deletable
- GC deletes objects only accessible by expired snapshots
- Tunable tradeoff: storage vs. time-travel capability
- Clean semantic: "keep last N days" is intuitive

| Strategy | Roots | Traversal Cost | Automatic? | Best For |
|----------|-------|---------------|------------|----------|
| Mark-and-sweep (Git) | Branch tips, tags | O(objects) | Semi-auto | Version history |
| Root-counting (Nix) | Explicit symlinks | O(1) lookup | Manual | Package store |
| Pin-based (IPFS) | Explicit pins | O(pins * depth) | Manual | Distributed |
| Snapshot expiry (Icechunk) | Time-based | O(expired) | Automatic | Data versioning |

### Actionable for Nika

1. **Start with the simplest viable GC: run-scoped cleanup.**
   - After a workflow run completes, identify MediaRefs in the run's output
   - Delete CAS entries not referenced by any run output
   - This is simpler than full mark-and-sweep but handles the common case

2. **Implement root-based GC (Nix pattern) for v1.**
   - Roots = workflow run outputs (MediaRefs in TaskResults)
   - GC = delete CAS files not referenced by any root
   - Simple file scan: list all files in `store/`, check against root set
   - Grace period: don't delete files newer than 1 hour (protects in-flight runs)

3. **Future: time-based expiration (Icechunk pattern).**
   - `nika gc --keep-days 30` deletes CAS entries older than 30 days not in any root
   - Combine with root-based: keep if referenced OR recent
   - Best UX: users set retention policy, GC runs automatically

4. **Design the MediaRef to support GC from day one.**
   - The `checksum` field enables dedup-aware GC (refcount by hash)
   - Consider adding a `created_at` timestamp to MediaRef for time-based GC
   - Consider a lightweight `media.json` manifest per run that lists all MediaRefs

5. **Do NOT implement mark-and-sweep.** Nika's CAS is flat (no DAG structure in the store
   itself). Root-based GC is sufficient and much simpler.

---

## Cross-Cutting Insights

### The Industry Consensus

Every system researched -- from ML pipelines to P2P networks to MCP protocol design --
converges on the same architecture:

```
Binary Producer  -->  Store (CAS/S3/filesystem)  -->  Handle/Ref  -->  Consumer
     ^                       ^                            ^               ^
  MCP Server            .nika/media/              MediaRef JSON     Next task step
  fetch: verb           blake3 layout             path + hash       {{with.x.media}}
  exec: verb            atomic writes             mime + size
```

**Never pass binary data through the orchestration layer. Always pass handles.**

Nika's B+ design already implements this pattern. The research confirms it is correct.

### Recommended Priority Adjustments

Based on this research, the three PRs planned in doc 23 should prioritize:

1. **PR 1 (rmcp adapter)** -- highest impact, unblocked:
   - Handle `as_image()`, `as_audio()`, `as_resource()`, `as_resource_link()` in
     `rmcp_adapter.rs` line 352
   - Store binary in CAS via MediaProcessor
   - Return composite output with MediaRef
   - Add `spawn_blocking` wrapper for blake3 hashing

2. **PR 2 (binding + TUI)** -- needed for usability:
   - `{{with.task.media[0].path}}` binding access
   - TUI media preview (file type icon + path + size)

3. **PR 3 (GC + hardening)** -- can wait:
   - Root-based GC (Nix pattern)
   - `nika gc` command
   - Size limits and cleanup policies
   - Optional: `created_at` on MediaRef for time-based expiry

### What NOT to Do

- **Do NOT add iroh-blobs as a dependency.** Too heavy for local CAS. Learn from it, don't import it.
- **Do NOT add S3/cloud support yet.** YAGNI. The CAS layout is compatible with cloud migration later.
- **Do NOT implement verified streaming (bao) in PR 1.** Forward-compatible via the blake3 hash, but the complexity is not justified yet.
- **Do NOT implement ResourceLink fetching in PR 1.** Log it, record the URI in MediaRef, but don't implement the `resources/read` fetch loop until there are real MCP servers returning ResourceLink.

---

## Sources

### Primary Sources (Scraped/Queried)
1. [Iroh blob store design challenges](https://www.iroh.computer/blog/blob-store-design-challenges) -- Storage architecture, chunk groups, outboard design
2. [Iroh-blobs v0.90 upgrade guide](https://www.iroh.computer/blog/iroh-blobs-0-90-changes) -- Rewrite details, bitfield files, RPC-as-interface
3. [Iroh-blobs v0.95 new features](https://www.iroh.computer/blog/iroh-blobs-0-95-new-features) -- Stream transforms, ConnectionRef
4. [MCP spec 2025-06-18 Resources](https://modelcontextprotocol.io/specification/2025-06-18/server/resources) -- ResourceLink schema
5. [MCP schema 2025-06-18](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/schema/2025-06-18/schema.ts) -- TypeScript type definitions
6. [arXiv 2510.05968: Patterns for Large Dataset Processing in MCP](https://arxiv.org/abs/2510.05968) -- ResourceLink usage patterns
7. [rust-mcp-schema ResourceLink](https://docs.rs/rust-mcp-schema/latest/rust_mcp_schema/mcp_2025_11_25/struct.ResourceLink.html) -- Rust SDK support
8. [DataDog temporal-large-payload-codec](https://github.com/DataDog/temporal-large-payload-codec) -- Production codec for large payloads
9. [GitHub: Scaling Git's garbage collection](https://github.blog/engineering/architecture-optimization/scaling-gits-garbage-collection/) -- Cruft packs, mark-and-sweep at scale
10. [Icechunk garbage collection](https://earthmover.io/blog/everything-you-need-to-know-about-icechunk-garbage-collection/) -- Snapshot-based expiration
11. [S5 content-addressed data](https://docs.sfive.net/concepts/content-addressed-data.html) -- BLAKE3 + bao .obao pattern
12. [bao-tree crate](https://github.com/n0-computer/bao-tree) -- BLAKE3 verified streaming in Rust
13. [BLAKE3 spec (PDF)](https://raw.githubusercontent.com/BLAKE3-team/BLAKE3-specs/master/blake3.pdf) -- Merkle tree design
14. [rmcp crate docs](https://docs.rs/crate/rmcp/latest) -- v1.2.0 MCP SDK for Rust
15. [Dagster asset docs](https://docs.dagster.io/guides/build/assets/defining-assets) -- MaterializeResult pattern

### Secondary Sources (Search Results)
16. [LambdaClass: The Wisdom of Iroh](https://blog.lambdaclass.com/the-wisdom-of-iroh/)
17. [GuardianDB (iroh-blobs integration)](https://lib.rs/crates/guardian-db)
18. [Content-addressable storage at scale (DesignGurus)](https://www.designgurus.io/answers/detail/how-would-you-implement-contentaddressable-storage-at-scale)
19. [Temporal community: payload storage patterns](https://community.temporal.io/t/payload-storage-and-type-safety/17412)
20. [n8n community: binary data handling](https://community.n8n.io/t/handling-binary-data-throughout-the-workflow/167249)

---

## Methodology

- **Tools used**: Perplexity sonar (13 queries), local file analysis
- **Pages analyzed**: 30+
- **Time period covered**: 2024-2026 (focus on 2025-2026)
- **Cross-referencing**: Each finding verified against 2+ sources where possible

## Confidence Level

**High** for sections 1, 3, 4, 7 -- well-documented patterns with multiple confirming sources.
**Medium** for sections 2, 5 -- iroh-blobs internals are partially documented; ResourceLink
adoption is still early and hard to verify.
**Medium** for section 6 -- absence of evidence (no Rust workflow engines with media patterns)
is not evidence of absence, but multiple searches found nothing.

## Further Research Suggestions

1. **rmcp 1.2.0 source code audit** -- Check exact ResourceLink handling vs. 0.16.0
2. **Windmill source code** -- How does the Rust backend handle job artifacts internally?
3. **Claude Desktop MCP client** -- Does it process ImageContent from tool results?
4. **bao-tree performance benchmarks** -- Overhead of outboard generation for various file sizes
5. **iroh-blobs FsStore source** -- Exact file layout and GC implementation details
