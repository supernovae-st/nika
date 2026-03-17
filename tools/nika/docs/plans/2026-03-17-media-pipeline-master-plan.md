# Multimodal Media Pipeline -- Master Plan (v3.1)

> **3 PRs** to give Nika first-class binary media support.
> Feature branches on `main`, merge + clean after each PR.

**Schema**: `@0.12` (no schema bump -- additive changes only)
**Source of truth**: [23-multimodal-media-pipeline-design.md](../brainstorm/nika-evolution/23-multimodal-media-pipeline-design.md)
**rmcp**: Stays on `0.16.0` (rig-core 0.32 pins `^0.16`, upgrade to 1.x blocked)

---

## The Problem (2 lines of code)

```
rmcp_adapter.rs:347  ->  c.as_text()       <- only extracts text, drops Image/Audio/Resource/ResourceLink
verbs.rs:918         ->  tool_result.text() <- joins only text blocks, ignores data/mime_type
```

Every MCP tool that returns binary content (images, audio, PDFs) is **silently dropped**.
40+ MCP servers in the ecosystem return binary data. Nika ignores all of it.

**rmcp 0.16.0 has 5 content variants**: `Text`, `Image`, `Audio`, `Resource` (EmbeddedResource), `ResourceLink`.
Only `Text` is extracted today. Note: `as_audio()` does NOT exist — must pattern match on `RawContent` directly.

---

## Architecture: B+ Design

```
                    +---------------------------------------------+
                    |          MCP Server (rmcp v0.16.0)          |
                    |  Returns: Text|Image|Audio|Resource|ResLink  |
                    +--------------------+------------------------+
                                         |
                    +--------------------v------------------------+
                    |   Layer 1: EXTRACTION (rmcp_adapter.rs)     |
        PR1         |   Match ALL ContentBlock variants            |
                    |   Image -> base64 + mime_type                |
                    |   Resource -> uri + blob                     |
                    |   + MediaExtracted event (telemetry)         |
                    +--------------------+------------------------+
                                         |
                    +--------------------v------------------------+
                    |   Layer 2: DETECTION (media/detect.rs)      |
                    |   infer 0.19 magic bytes -> MIME             |
                    |   mime_guess fallback -> extension            |
                    |   Cross-validate: server MIME vs detected    |
                    +--------------------+------------------------+
                                         |
                    +--------------------v------------------------+
        PR2         |   Layer 3: PROCESSING (media/processor.rs)  |
                    |   blake3 hash -> ArrayString<64>             |
                    |   CAS store: .nika/media/{h[0..2]}/{rest}   |
                    |   io::atomic::write_atomic() for safety     |
                    |   READ-BACK verification (hash match)       |
                    |   + MediaProcessed + MediaStored events      |
                    +--------------------+------------------------+
                                         |
                    +--------------------v------------------------+
                    |   Layer 4: REFERENCE (media/types.rs)       |
                    |   MediaRef { hash, mime, size, path }       |
                    |   Stored in TaskResult.media alongside output|
                    +--------------------+------------------------+
                                         |
                    +--------------------v------------------------+
        PR3         |   Layer 5: ARTIFACTS (ast/artifact.rs)      |
                    |   ArtifactFormat::Binary variant             |
                    |   ArtifactWriter.write_binary() method       |
                    |   with: bindings for {{with.media_path}}      |
                    |   `nika media clean` CLI command             |
                    |   E2E integrity check at workflow end        |
                    +---------------------------------------------+
```

---

## Decision Matrix (Brainstorm Results)

| # | Decision | Choice | Rationale |
|:-:|----------|--------|-----------|
| Q1 | PR Granularity | **3 PRs** | Extraction -> Processor -> Artifacts |
| Q2 | Branch Strategy | **Feature branches on main** | merge + clean after each |
| Q3 | Error Codes | **Grouped by layer** | 250-252 detect, 253-255 store |
| Q4 | Testing | **TDD + proptest** | ~40-50 tests, binary edge cases |
| Q5 | YAML Schema | **Hybrid** | Auto-capture now, media: block later |
| Q6 | CAS Location | **Hierarchical** | Default .nika/media/ + config override |

---

## CAS Location: Hierarchical Config (Option C)

Priority: **workflow YAML > workspace config > global config > defaults**

```
Global (~/.config/nika/config.toml):
  [media]
  store_dir = "~/.nika/media/store"    # Shared CAS across projects
  max_store_size = "1GB"               # Global limit
  cleanup_after = "30d"                # Auto-cleanup

Workspace (.nika/config.toml):
  [media]
  store_dir = ".nika/media/store"      # Project-local (DEFAULT)
  max_store_size = "500MB"

DEFAULT (no config needed):
  store_dir = ".nika/media/store"      # Workspace-local, zero config
```

This follows the existing NikaConfig pattern (ENV > config file > defaults).
Config parsing is deferred -- PR2 uses the default, PR3 adds config override.

---

## Dependency Map

```
New crates (VERIFIED via crates.io 2026-03-17):
  blake3     = "1.8"    # Latest: 1.8.3. CAS hashing -- Hash::to_hex() → ArrayString<64>, zero alloc
                        # New: Hash::as_slice() → &[u8; 32]. MSRV 1.85. 107M downloads.
  infer      = "0.19"   # Latest: 0.19.0. Magic byte MIME detection -- no_std, custom matchers. 77M dl.
  mime_guess = "2.0"    # Latest: 2.0.5. Extension ↔ MIME fallback -- from_ext(), get_mime_extensions_str()
  base64     = "0.22"   # Latest: 0.22.1. Engine API (STANDARD, URL_SAFE_NO_PAD). 995M dl.

Already in tree:
  tempfile   = "3.14"   # Latest: 3.27.0. Already direct dep. NamedTempFile::persist() for CAS.
  uuid       = "..."    # Already used by io/atomic.rs

Pinned (rig-core blocker):
  rmcp       = "0.16"   # CANNOT upgrade to 1.x: rig-core 0.32/0.33 pins ^0.16
                        # Content API identical between 0.16 and 1.2.0 -- no impact
                        # PR #1479 on rig repo (rmcp v1 upgrade) open but unreviewed

Optional upgrade (orthogonal to media pipeline):
  rig-core   = "0.33"   # Released 2026-03-17, adds McpClientHandler for auto tool updates
                        # Same rmcp ^0.16 -- safe upgrade from 0.32

Phase 4 candidates (research findings -- evaluate after PRs merge):
  file-format = "0.28"  # Deep MIME: format-specific readers for ZIP/MP4 containers. 1M dl.
  img-parts   = "0.4"   # Structural validation: JPEG segments / PNG chunks without full decode. 9.1M dl.
  atomic-write-file = "0.3"  # Crash-tested atomic writes for non-CAS files (config, metadata). 4.4M dl.

NOT needed (YAGNI -- confirmed by research):
  cacache (SHA-based, npm layout, heavy deps), iroh-blobs (p2p stack),
  tree_magic_mini (freedesktop.org dep), atomicwrites (older API),
  rsraw, imagehash, sled
```

---

## Error Code Allocation

```
NIKA-251  MimeDetectionFailed    # infer returns None + mime_guess returns None
NIKA-252  UnsupportedMediaType   # Recognized but not supported (e.g., video/*)
NIKA-253  MediaNotFound          # CAS hash lookup found no file

NIKA-254  HashMismatch           # blake3 verification failed (integrity)
NIKA-255  MediaStoreWrite        # CAS write failed (disk full, permissions)
NIKA-256  Base64DecodeFailed     # Invalid base64 from MCP server

NIKA-257..259  RESERVED          # Future media errors
```

**NOTE**: NIKA-250 is taken by `ContextLoadError`. Media errors start at 251.
Range 251-259 is free (between NIKA-250 and NIKA-280-289 artifacts).

---

## New Module Structure

```
src/
+-- media/               # NEW MODULE (pub mod media; in lib.rs after tools)
|   +-- mod.rs           # Module root, re-exports
|   +-- types.rs         # MediaRef, MediaBlock, MediaType enum
|   +-- detect.rs        # MIME detection (infer + mime_guess)
|   +-- processor.rs     # MediaProcessor: decode -> hash -> store
|   +-- store.rs         # CAS store operations (read, write, exists, clean)
|   +-- config.rs        # MediaConfig for hierarchical config (PR3)
|   +-- error.rs         # NIKA-251..256 error variants
+-- ...existing...
```

---

## Event System Additions (Telemetry)

5 new EventKind variants across PRs (new MEDIA EVENTS category).
Current EventKind has **32 variants** across 10 categories. Media adds a new 11th category.

**IMPORTANT**: Use exhaustive `match` on `RawContent` (5 variants), NOT `if let` chains.
`as_audio()` does NOT exist in rmcp 0.16 — pattern match on raw enum via `Deref`.

```rust
// PR1 - Extraction telemetry
MediaExtracted {
    task_id: Arc<str>,
    block_count: u32,
    content_types: Vec<String>,  // ["image", "audio", "resource", "resource_link"]
},

// PR2 - Processing telemetry
MediaProcessed {
    task_id: Arc<str>,
    hash: String,
    mime_type: String,
    media_type: String,          // "image"|"audio"|"document"
    size_bytes: u64,
    server_mime: Option<String>, // MCP server's claim vs detected
},
MediaStored {
    task_id: Arc<str>,
    hash: String,
    path: String,
    size_bytes: u64,
    verified: bool,              // read-back hash verification
    deduplicated: bool,          // file already existed in CAS
},
MediaStoreFailed {
    task_id: Arc<str>,
    hash: String,
    reason: String,
},

// PR3 - Cleanup telemetry
MediaCleanup {
    removed_count: u32,
    freed_bytes: u64,
    policy: String,              // "all"|"older_than_7d"|"manual"
},
```

Each variant MUST be added to `EventKind::task_id()` match arm.

---

## Defense-in-Depth: Silent Error Detection

5 verification layers prevent silent data loss:

| Layer | PR | What | How |
|-------|:--:|------|-----|
| 1. Extraction | 1 | Count extracted blocks | Log + event if count < rmcp result count |
| 2. Decode | 2 | Verify base64 output | Check decoded.len() > 0 when input.len() > 0 |
| 3. Storage | 2 | Read-back verification | Re-hash stored file, compare to expected hash |
| 4. Artifact copy | 3 | Verify destination | Check size matches, file exists after copy |
| 5. Workflow end | 3 | E2E integrity check | Verify all MediaRefs still point to existing CAS files |

---

## PR Sequence

| PR | Branch | Scope | Tests | Commits |
|----|--------|-------|-------|---------|
| [PR1](./2026-03-17-media-pipeline-pr1-extraction.md) | `feat/media-extraction` | Fix rmcp_adapter (5 variants) + verbs.rs + telemetry + count test | ~17-20 | ~5-6 |
| [PR2](./2026-03-17-media-pipeline-pr2-processor.md) | `feat/media-processor` | MediaProcessor + CAS store + MediaRef + detection + resolve_path | ~22-28 | ~8-10 |
| [PR3](./2026-03-17-media-pipeline-pr3-artifacts.md) | `feat/media-artifacts` | ArtifactFormat::Binary + write_binary + with: templates + CLI + e2e | ~12-18 | ~6-8 |

| Phase 4 | (post-merge) | Dep upgrades (rig-core 0.33) + research + code review + perf audit | — | ~1-2 |

**Execution**: TDD subagent-driven, each commit = 1 logical change.
**Phase 4**: Research-driven optimization after all 3 PRs merged.

---

## Backward Compatibility

| Component | Impact | Migration |
|-----------|--------|-----------|
| Existing workflows | **Zero** | Text-only tools continue working identically |
| ToolCallResult | **Additive** | New methods (`has_media`, `images`, `media_blocks`) |
| TaskResult | **Additive** | New `media: Vec<MediaRef>` field (default empty) |
| ArtifactFormat | **Additive** | New `Binary` variant, existing Text/Json/Yaml unchanged |
| ArtifactWriter | **Additive** | New `write_binary()` method, existing `write()` unchanged |
| OutputFormat | **Additive** | New `Binary` variant in both `ArtifactFormat` + `OutputFormat` |
| Event stream | **Additive** | 5 new variants, existing 32 unchanged |
| YAML schema | **None** | Auto-capture via `with:` bindings, zero new YAML syntax |
| NikaConfig | **Additive** | Optional `[media]` section, ignored if absent |

---

## File Impact Summary

| File | PR | Change Type |
|------|:--:|-------------|
| `src/lib.rs` | 2 | Add `pub mod media;` |
| `src/error.rs` | 2 | Add NIKA-251..256 variants + `From<MediaError>` |
| `src/mcp/rmcp_adapter.rs` | 1 | Fix `call_tool()`: exhaustive match on 5 `RawContent` variants |
| `src/mcp/types.rs` | 1 | Add `audio()` constructor, `is_audio()`, `has_media()`, `images()`, `media_blocks()` |
| `src/runtime/executor/verbs.rs` | 1,2 | PR1: `__media` bridge. PR2: replace with MediaProcessor |
| `src/store/run_context.rs` | 2 | Add `media: Vec<MediaRef>` to TaskResult + extend `resolve_path()` for "media" |
| `src/event/log.rs` | 1,2,3 | PR1: MediaExtracted + count test. PR2: +3 events. PR3: MediaCleanup |
| `src/ast/artifact.rs` | 3 | Add `Binary` variant to ArtifactFormat |
| `src/ast/output.rs` | 3 | Add `Binary` variant to OutputFormat |
| `src/io/writer.rs` | 3 | Add `write_binary()` method |
| `src/media/*.rs` | 2 | NEW: entire media module (6 files) |
| `Cargo.toml` | 2 | Add blake3, infer, mime_guess, base64 |

---

## Testing Strategy

| Layer | Tool | What |
|-------|------|------|
| Unit | `#[tokio::test]` | Every public function in media/ |
| Property | `proptest` | Binary edge cases (truncated base64, zero-len, invalid MIME) |
| Snapshot | `insta` | MediaRef serialization, CAS path generation |
| Integration | Mock MCP | Full flow: tool returns image -> CAS stored -> ref in output |
| Verification | Custom | Read-back hash check, dedup check, size consistency |
| E2E | Full workflow | image gen workflow -> CAS stored -> artifact copied -> integrity verified |
| Guard | `count_all_variants` | Enforces EventKind variant count (32→33→36→37) |
| Regression | `cargo test` | All 6,610 existing tests must pass |

---

## Code Review Checkpoints

Each PR must pass before merging:

1. **cargo test** -- all tests green (existing + new)
2. **cargo clippy -- -D warnings** -- zero warnings
3. **Telemetry trace review** -- grep NDJSON for media events, verify breadcrumb chain
4. **Silent error test** -- introduce deliberate failures (bad base64, disk full), verify they surface
5. **Regression** -- run 3 existing workflows, verify unchanged output

---

## Phase 4: Dependency Upgrades + Optimization (Post-PR3)

After all 3 media PRs are merged, a final optimization pass:

### 4.1 Dependency Upgrades

| Crate | Current | Target | Reason |
|-------|---------|--------|--------|
| rig-core | 0.32 | **0.33** | McpClientHandler for auto tool updates (released 2026-03-17) |
| rmcp | 0.16.0 | 0.16.0 (stay) | Blocked by rig-core. Monitor PR #1479 on rig repo |

**Commit**: `chore(deps): upgrade rig-core 0.32 → 0.33`

### 4.2 Research & Improvement Pass (COMPLETED 2026-03-17)

Research completed across 3 parallel agents. Full report: `nika/docs/research/media-pipeline-crate-audit.md`

**Key findings:**

1. **Crate versions**: All planned versions confirmed latest. blake3 1.8 → 1.8.3 (semver covers it). tempfile 3.14 → 3.27 in tree.
2. **CAS alternatives**: cacache (npm-port, SHA-based) rejected — wrong hash algo, heavy deps. iroh-blobs rejected — p2p stack. **Build our own** (~100 lines with blake3 + tempfile).
3. **Atomic writes**: `atomic-write-file` 0.3.0 is crash-tested, but `tempfile::persist_noclobber()` is optimal for CAS (race-safe, dedup-native). Keep existing `io::atomic::write_atomic()` for PR2, evaluate `persist_noclobber` pattern as improvement.
4. **MIME detection**: `infer` best for primary. `file-format` 0.28 best for secondary (ZIP/MP4 container disambiguation). `tree_magic_mini` rejected (freedesktop.org dep).
5. **MCP ecosystem**: TS/Python SDKs pass base64 ImageContent directly to LLM APIs. Data URI format (`data:{mime};base64,{data}`) required when forwarding. MCP 2025-06-18 adds `annotations` (audience + priority) — useful for future TUI filtering.
6. **Performance**: blake3 4-10x faster than SHA-256. Use `rayon` feature for >1MB files. Store `Hash` as `[u8; 32]` (Copy, stack-allocated) not String.
7. **Image validation**: `img-parts` 0.4 for structural validation (JPEG segments, PNG chunks) without full decode. Layer: infer (magic) → img-parts (structure) → image crate (full decode, on-demand).
8. **CAS crash safety**: Add dir fsync after rename for maximum durability (`File::open(parent)?.sync_all()?`). Current `write_atomic` doesn't fsync parent dir.

### 4.3 Code Review (Full Pipeline)

Use `code-reviewer` agent to review the complete media pipeline implementation:

```
Scope: ALL files changed across PR1 + PR2 + PR3
Focus:
  - Security: path traversal in CAS store, base64 injection
  - Performance: unnecessary allocations, async bottlenecks
  - Error handling: silent failures, missing error propagation
  - Thread safety: MediaRef in DashMap, Arc<Value> interactions
  - API design: public surface area, backward compat guarantees
```

### 4.4 Rust-Pro Audit

Use `rust-pro` / `rust-perf` / `rust-security` agents:

1. **rust-security**: Audit CAS store paths (directory traversal?), base64 decode (DoS on huge input?)
2. **rust-perf**: Profile CAS store operations, blake3 hashing, base64 decode for large media
3. **rust-pro**: Review all `unsafe` (should be none), error handling patterns, idiomatic Rust

### 4.5 Full Test Suite Verification

```bash
# Run ALL tests (existing + new media pipeline tests)
cargo test

# Zero warnings policy
cargo clippy -- -D warnings

# Coverage check (media module should be >80%)
cargo tarpaulin --out Html -- --lib

# Property-based test run (proptest may need extended time)
cargo test --release -- proptest

# Integration test with mock MCP server returning binary
cargo test --features integration
```

### 4.6 Propose Improvements (Informed by Research)

Based on research findings, propose improvements as separate PRs:

- **PR5** (recommended): Add `file-format` 0.28 for deep MIME detection on ambiguous containers (ZIP-based, ISOBMFF)
- **PR6** (recommended): Add `img-parts` 0.4 for structural image validation (catches truncated/corrupted files that pass magic bytes)
- **PR7** (optional): CAS write upgrade — `tempfile::persist_noclobber()` for race-safe dedup + dir fsync for crash durability
- **PR8** (optional): Media preview in TUI (thumbnail rendering)
- **PR9** (optional): MCP annotations support (`audience`/`priority` from 2025-06-18 spec) for TUI filtering

### 4.7 Documentation

- Update `CLAUDE.md` with media module conventions
- Add media pipeline architecture diagram to `docs/`
- Update error code table with NIKA-251..256

---

## Links

- **Master Plan**: this document
- **PR1**: [media-pipeline-pr1-extraction.md](./2026-03-17-media-pipeline-pr1-extraction.md)
- **PR2**: [media-pipeline-pr2-processor.md](./2026-03-17-media-pipeline-pr2-processor.md)
- **PR3**: [media-pipeline-pr3-artifacts.md](./2026-03-17-media-pipeline-pr3-artifacts.md)
- **Design Doc**: [23-multimodal-media-pipeline-design.md](../brainstorm/nika-evolution/23-multimodal-media-pipeline-design.md)
