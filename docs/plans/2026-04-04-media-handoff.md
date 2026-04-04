# Handoff: Media Pipeline Enhancement — nika:decode

> **For**: Next Claude session
> **Date**: 2026-04-04
> **State**: Plan written, research complete, code skeleton ready
> **Pre-req**: v0.68.0 tagged and pushed. Feature freeze. 9,823+ tests green.

---

## What Happened This Session

1. **Deep audit** of 24 media tools via code-explorer agent (743+ tests, 3 tiers)
2. **Security review** via rust-security agent — findings below
3. **Code review** via rust-pro agent — still running (check output)
4. **Research** via Explore agent — complete code skeleton for nika:decode
5. **Plan** written at `docs/plans/2026-04-04-media-pipeline-plan.md`

---

## Security Review Findings

| Severity | Finding | File:Line | Fix |
|----------|---------|-----------|-----|
| **HIGH** | nika:import reads ANY user file outside system dirs. ~/.ssh/id_rsa, ~/.aws/credentials accessible. Blocklist approach is inherently incomplete. | `nika-media/src/tools/import.rs:24-35` | Validate canonical path within project working dir |
| **MEDIUM** | Pipeline has no step count limit. Thousands of steps = CPU exhaustion. | `nika-media/src/tools/pipeline.rs:69-76` | Add `MAX_PIPELINE_STEPS = 50` |
| **MEDIUM** | Pipeline returns Binary result without budget check. Budget accounting bypassed. | `nika-media/src/tools/pipeline.rs:272-282` | Call `ctx.budget.check_and_add()` before return |
| **LOW** | CAS symlink-at-target treated as dedup hit. Attacker with fs access could redirect reads. | `nika-media/src/store.rs:1407-1428` | Low risk, document only |

### Recommendation

Fix HIGH first (import path validation), then the two MEDIUMs (pipeline step limit + budget). LOW is acceptable risk.

---

## nika:decode Implementation — Ready to Code

### Architecture Decision

**Don't reuse `MediaProcessor::process_base64()`** — it's coupled to MCP ContentBlock format and does auto-enrichment (dimensions, thumbhash) we don't need. Instead, implement standalone decode with the same base64 decode + CAS store pattern.

### Files to Create

**`tools/nika-media/src/tools/decode.rs`** — Complete skeleton provided by research agent:
- Struct: `DecodeOp` implementing `MediaOp` trait
- Params: `{ data: String, mime_type: String }`
- Flow: validate → strip whitespace → base64 decode → CAS store → return metadata
- Guards: empty check, size check (100MB), MIME validation, cancelled check
- Returns: `MediaOpResult::Metadata(json!({ hash, mime_type, size_bytes, deduplicated }))`

### Files to Modify

1. **`tools/nika-media/src/tools/mod.rs`**:
   - Add `pub mod decode;` after the import module declaration
   - Add `ops.push(Box::new(decode::DecodeOp));` in `create_all_media_ops()` Tier 1 section

2. **No engine changes needed** — `create_media_tool_adapters()` in `nika-engine/src/runtime/builtin/media/mod.rs` auto-discovers all tools from `create_all_media_ops()`.

### Key APIs

```rust
// CAS store (budget-checked)
ctx.store_media(&decoded_bytes, "decode").await?
// Returns StoreResult { hash, path, size, deduplicated, verified, pipeline_ms }

// Error constructors (from tools/error.rs)
invalid_args("decode", "reason")  // NIKA-294
tool_error("decode", "reason")    // NIKA-290

// Base64 decode
use base64::Engine;
base64::engine::general_purpose::STANDARD.decode(&clean_b64)?
```

### 13 Tests Provided

| Test | What |
|------|------|
| `decode_valid_png_base64` | Happy path PNG |
| `decode_with_whitespace_in_base64` | PEM-style newlines in base64 |
| `decode_rejects_empty_base64` | Empty string → NIKA-294 |
| `decode_rejects_invalid_base64` | Garbage → NIKA-290 |
| `decode_rejects_empty_mime_type` | Empty MIME → NIKA-294 |
| `decode_rejects_octet_stream_mime` | Generic MIME → NIKA-294 |
| `decode_missing_data_param` | Missing param → NIKA-294 |
| `decode_missing_mime_type_param` | Missing param → NIKA-294 |
| `decode_deduplicates` | Same content → deduplicated: true |
| `decode_roundtrip` | Encode → decode → CAS → read back → identical |
| `decode_oversized_rejected` | >100MB → rejected |
| `decode_various_mimes` | JPEG, MP3, PDF, MP4 all accepted |
| `decode_cancelled_workflow` | Cancellation token respected |

### TDD Order

```
1. Create decode.rs with struct + trait impl (empty execute)
2. Write decode_valid_png_base64 → RED
3. Implement base64 decode + CAS store → GREEN
4. Write error tests (empty, invalid, missing params) → RED
5. Add parameter validation → GREEN
6. Write edge cases (dedup, roundtrip, oversized, whitespace) → RED/GREEN
7. cargo clippy + fmt
8. Commit
```

---

## Execution Order for Next Session

```
Step 1 (5 min)  — Check rust-pro code review results (agent may have completed)
Step 2 (15 min) — Fix HIGH security finding (import.rs path validation)
Step 3 (15 min) — Fix 2 MEDIUM findings (pipeline step limit + budget)
Step 4 (45 min) — Implement nika:decode (TDD, skeleton provided)
Step 5 (15 min) — Create 2 showcase workflows (URL pattern + base64 pattern)
Step 6 (10 min) — Update DX files (62 builtins, decode pattern)
Step 7 (5 min)  — Full test suite + clippy + push
```

**Total: ~1h50**

---

## Verification Commands

```bash
cd /Users/thibaut/dev/supernovae/nika/tools

# After each step:
cargo test --workspace --lib --exclude nika-py
cargo clippy --all-targets --all-features -- -D warnings

# After decode implementation:
cargo test -p nika-media --lib -- decode

# After showcases:
../target/debug/nika check ../examples/showcase/media/*.nika.yaml
```

---

## What NOT to Do

- No new verbs
- No new feature flags
- No changes to CAS store format
- No provider changes
- No video processing
- No animated GIF support
- Don't reuse MediaProcessor::process_base64() — implement standalone

---

## Context for the Session Prompt

```
Lis docs/plans/2026-04-04-media-handoff.md — c'est le plan complet.

Exécute dans l'ordre :
1. Fix HIGH security (import.rs path validation)
2. Fix 2 MEDIUM (pipeline step limit + budget)
3. Implement nika:decode (TDD, skeleton dans le handoff)
4. Showcases + DX update
5. Tests + push

Le code skeleton pour decode.rs est dans le handoff.
Les findings security sont déjà triés par priorité.
```
