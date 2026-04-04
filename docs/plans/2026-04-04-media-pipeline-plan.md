# Media Pipeline Enhancement Plan — v0.68.x

> **Status**: Plan written, Sprint A ready to execute
> **Author**: Claude + Thibaut
> **Date**: 2026-04-04
> **Context**: v0.68.0 feature freeze. This plan adds 1 missing builtin tool
>   (`nika:decode`) that unblocks a documented but unusable capability.
>   Treated as a bug fix, not a new feature.

---

## Problem Statement

Nika's media pipeline has 24 builtin tools, a CAS store, and binary fetch support.
But **APIs that return base64-encoded images inline** (Gemini, Replicate, fal.ai/Flux,
Stability AI) cannot feed into the pipeline because there's no `base64 string → CAS`
bridge. The `| base64_decode` transform exists but produces a string, not a CAS blob.

**Impact**: Users must use `exec: python3` workarounds for the #1 AI media use case
(image generation). This undermines "One file. Any AI." positioning.

## Current State (v0.68.0)

### What Works

```
fetch URL (response: binary) → CAS hash → nika:thumbnail → artifact
nika:import local_file.png  → CAS hash → nika:convert → artifact
nika:pipeline [thumbnail, optimize, convert] → single CAS read/write
```

### What's Blocked

```
fetch API → JSON { base64: "..." } → ??? → CAS hash → nika:thumbnail
                                      ^^^
                              MISSING: nika:decode
```

### APIs Affected

| API | Returns | Works Today? |
|-----|---------|--------------|
| DALL-E (`response_format: url`) | URL to download | YES (fetch binary) |
| DALL-E (`response_format: b64_json`) | Inline base64 | NO |
| Gemini (`inlineData`) | Inline base64 | NO |
| Replicate / fal.ai / Flux | Base64 or URL | Partial |
| Stability AI | Base64 | NO |

## Solution: `nika:decode` Builtin Tool

### Design

```yaml
- id: decode_image
  invoke:
    tool: nika:decode
    params:
      data: "{{with.base64_string}}"    # base64-encoded bytes
      mime_type: "image/png"             # required MIME type
```

**Returns**: `{ hash: "blake3:...", mime_type: "image/png", size_bytes: 42000 }`

### Implementation Notes

- **Reuse**: `MediaProcessor::process_base64()` already does 90% of this
  (decode base64 → validate MIME → store CAS → return hash)
- **New code**: ~60 LOC for the tool wrapper + parameter validation
- **Always-on**: No feature flag (like `nika:import`)
- **Limits**: Same as import — 100 MB max, MIME validation via magic bytes
- **Security**: No file paths involved, no path traversal risk.
  Base64 bomb protection via existing `MediaBudget` (500 MB per run)

### Test Plan

| Test | What it validates |
|------|-------------------|
| `decode_valid_png` | Valid base64 PNG → CAS hash returned, correct size |
| `decode_valid_jpeg` | Valid base64 JPEG → correct mime_type |
| `decode_invalid_base64` | Garbage string → clean error, not panic |
| `decode_mime_mismatch` | base64 is PNG but mime says JPEG → magic bytes win |
| `decode_empty_data` | Empty string → error |
| `decode_oversized` | >100MB decoded → budget rejection |
| `decode_dedup` | Same data twice → `deduplicated: true` |

---

## Sprint A: Implement nika:decode (1h)

### Files to Create

1. **`tools/nika-media/src/tools/decode.rs`** (~60 LOC)
   - `DecodeParams { data: String, mime_type: String }`
   - Decode base64 via `base64::engine::general_purpose::STANDARD`
   - Validate MIME via `detect_mime()` (magic bytes, hint = params.mime_type)
   - Store via `CasStore::put()`
   - Return `{ hash, mime_type, size_bytes, deduplicated }`

### Files to Modify

2. **`tools/nika-media/src/tools/mod.rs`**
   - Add `pub mod decode;` 
   - Register in `create_all_media_ops()` as always-on (Tier 1)

3. **`tools/nika-engine/src/runtime/builtin/media/mod.rs`**
   - Wire `DecodeTool` into the router (if not auto-registered via MediaOp)

### TDD Sequence

```
RED:    write decode_valid_png test → fails (no decode.rs)
GREEN:  implement decode.rs → test passes
RED:    write decode_invalid_base64 → fails  
GREEN:  add error handling → passes
RED:    write decode_empty, decode_oversized, decode_dedup
GREEN:  implement all edge cases
REFACTOR: clean up, check clippy
```

### Commit

```
feat(media): nika:decode — base64 string → CAS store

Unblocks image generation APIs that return inline base64 (Gemini,
Replicate, fal.ai, Stability AI). 7 tests. Always-on (Tier 1).
```

---

## Sprint B: Security + Code Review (30min)

Run two agents in parallel:
- `rust-security` on nika-media (CAS, import, pipeline, fetch binary)
- `rust-pro` code review on all 24 media tools

Fix any CRITICAL or HIGH findings. Document MEDIUM as known issues.

### Commit

```
fix(media): address security review findings
```

---

## Sprint C: Showcase Workflows (30min)

### 1. Image Generation via URL (DALL-E pattern)

**`examples/showcase/media/image-generation-url.nika.yaml`**

```yaml
schema: "nika/workflow@0.12"
workflow: image-generation-url
description: "Generate image via DALL-E URL → thumbnail → artifact"
provider: mock

tasks:
  - id: describe
    infer: "Describe a serene mountain landscape at golden hour in one sentence"

  - id: generate
    with:
      prompt: $describe
    fetch:
      url: "https://api.openai.com/v1/images/generations"
      method: POST
      headers:
        Authorization: "Bearer $env.OPENAI_API_KEY"
      json:
        model: dall-e-3
        prompt: "{{with.prompt | trim}}"
        size: "1024x1024"
        response_format: url
      extract: jsonpath
      selector: "$.data[0].url"

  - id: download
    with:
      url: $generate
    fetch:
      url: "{{with.url | trim}}"
      response: binary
    artifact:
      path: generated.png
      format: binary

  - id: thumbnail
    with:
      img: $download
    invoke:
      tool: nika:thumbnail
      params:
        hash: "{{with.img.hash}}"
        width: 256
        format: webp
    artifact:
      path: thumb.webp
      format: binary
```

### 2. Base64 Decode Pattern (Gemini/Replicate)

**`examples/showcase/media/image-decode-base64.nika.yaml`**

```yaml
schema: "nika/workflow@0.12"
workflow: image-decode-base64
description: "Decode base64 image from API response → CAS → thumbnail"
provider: mock

tasks:
  - id: call_api
    # Simulates an API returning base64 image in JSON
    infer: "Return a JSON with field 'image_b64' containing a valid base64 PNG"
    structured:
      schema:
        type: object
        properties:
          image_b64: { type: string }
        required: [image_b64]

  - id: decode
    with:
      b64: $call_api
    invoke:
      tool: nika:decode
      params:
        data: "{{with.b64.image_b64}}"
        mime_type: "image/png"

  - id: dimensions
    with:
      img: $decode
    invoke:
      tool: nika:dimensions
      params:
        hash: "{{with.img.hash}}"

  - id: thumbnail
    with:
      img: $decode
    invoke:
      tool: nika:thumbnail
      params:
        hash: "{{with.img.hash}}"
        width: 128
        format: webp
    artifact:
      path: preview.webp
      format: binary
```

### Commit

```
docs(showcase): add image generation + base64 decode workflow examples
```

---

## Sprint D: DX Update (15min)

1. AGENTS.md: 62 builtins (was 61), add `nika:decode` to Tier 1
2. nika-bugs-and-patterns.md: add base64→CAS pattern
3. nika.md: add nika:decode to tool table (if not auto-regenerated)

### Commit

```
docs(dx): update tool count to 62, add nika:decode patterns
```

---

## Methodology

### For nika:decode implementation

1. **Read first**: Study `import.rs` and `processor.rs` — reuse patterns
2. **TDD**: Write failing test → implement → green → next test
3. **Minimal**: ~60 LOC, no new deps (base64 crate already in workspace)
4. **Security**: MIME validation via magic bytes, not user hint. Budget check.
5. **Review**: Run rust-security agent after implementation

### For showcase workflows

1. Write YAML using only existing patterns from nika.md
2. `nika check` to validate
3. `nika run --provider mock` to verify execution (no API keys needed)

### Verification Checklist

- [ ] `cargo test --workspace --lib --exclude nika-py` — 0 failures
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` — 0 warnings  
- [ ] `nika check examples/showcase/media/*.nika.yaml` — all valid
- [ ] New tool appears in `nika invoke --list` output
- [ ] AGENTS.md says 62 builtins
- [ ] All showcase workflows pass `nika check`

---

## What NOT To Do

- No new verbs (5 are sacred)
- No image generation verb — use fetch + nika:decode
- No video processing tools
- No animated GIF support
- No new feature flags
- No changes to CAS store format
- No provider changes

---

## Handoff Notes for Next Session

### Context

v0.68.0 is FEATURE FROZEN (May 5 launch). This plan adds 1 builtin tool
(`nika:decode`) as a bug fix — it completes an existing but broken capability.

### What's Done

- 3 review agents already launched (security, code review, research)
- Plan written and committed to docs/plans/
- All 450 showcase workflows validated
- Working tree clean, tests green (9,823+)

### What Needs Doing

1. Read agent results (check /tmp output or re-run)
2. Execute Sprint A (nika:decode) — TDD, ~60 LOC
3. Fix any security/review findings from agents
4. Create showcase workflows
5. Update DX files
6. Push (no new tag needed — patch version)

### Key Files

| File | Purpose |
|------|---------|
| `nika-media/src/tools/import.rs` | Reference impl (closest to decode) |
| `nika-media/src/processor.rs` | Has `process_base64()` — might reuse |
| `nika-media/src/tools/mod.rs` | Tool registration |
| `nika-media/src/store.rs` | CAS store API |
| `nika-engine/src/runtime/builtin/media/mod.rs` | Engine wiring |

### How to Verify

```bash
cd tools
cargo test --workspace --lib --exclude nika-py  # Must pass
cargo clippy --all-targets --all-features -- -D warnings  # Must be clean
# After implementation:
cargo test -p nika-media --lib -- decode  # New tests
nika check examples/showcase/media/*.nika.yaml  # Showcases valid
```

### Risks

- `process_base64()` in processor.rs might be tightly coupled to MCP ContentBlock
  format. If so, extract the base64→CAS core into a reusable function.
- Base64 padding variants (standard vs URL-safe) — test both.
- Feature freeze optics: this is a 1-tool addition, not a feature. Frame as bug fix.
