# Media Pipeline Handoff — 2026-03-28

> Session: Media bug hunt + code review. 5 parallel agents, real workflow runs.

---

## DONE (this session)

### 3 bugs fixed, committed, tested (8629+ tests pass)

| # | Severity | Bug | Fix | File |
|---|----------|-----|-----|------|
| 1 | **CRITICAL** | `artifact: { path: output.png }` without `format: binary` writes JSON metadata instead of image bytes ("broken pixel" images) | Auto-promote to `Binary` when task has `media_refs` and no explicit format. Both `Enabled(true)` and `Single` paths covered. | `artifact_processor.rs:86-94, 210-221` |
| 2 | **MEDIUM** | `verify_media_integrity()` false-positive warnings (4-byte NK compression framing + zstd) | `#[cfg(not(feature = "media-compression"))]` to skip size check when compression enabled | `runner.rs:562` |
| 3 | **MEDIUM** | `nika provider list` triggers macOS Keychain popups for unconfigured providers | Check env vars FIRST, only query keychain when no env var set | `provider.rs:115-150` |

### Verified with real workflows

All workflows ran with `nika run --no-live`, all output files verified:

| Workflow | Verbs | Output | Status |
|----------|-------|--------|--------|
| import + thumbnail + artifact | `invoke: nika:import`, `nika:thumbnail` | `original.png`, `thumbnail.png` | Valid PNG, byte-identical |
| fetch binary | `fetch: response: binary` | `fetched.png`, `.jpg`, `.svg` | Valid JPEG/PNG/SVG |
| convert formats | `invoke: nika:convert` | `converted.jpg`, `.webp` | Valid JPEG/WebP |
| missing format (repro) | `invoke: nika:import`, no `format:` | Before: 265B JSON. After: 334B PNG | Fixed |
| text control group | `infer:` + `provider: mock` | `haiku.txt`, `data.json` | Valid text |

---

## REMAINING — Prioritized Follow-ups

### P0: Security (from session-handoff.md, NOT media-related)

Already documented in `docs/plans/2026-03-28-session-handoff.md`. Do these first:

1. **Template injection** (`binding/template.rs:498`) — LLM output containing `{{context.files.secret}}` gets resolved in Pass 2
2. **Binary fetch OOM** (`fetch.rs:450`) — `response.bytes().await` bypasses streaming size limit

### P1: Media Pipeline Gaps (from our agents + exploration)

| # | Type | Issue | File:Line | Effort |
|---|------|-------|-----------|--------|
| 1 | **Test gap** | No integration test: `for_each` + media tool + artifact export | `artifact_processor.rs` | 30min |
| 2 | **Test gap** | No test: `ArtifactSpec::Multiple` with mixed binary + text formats | `artifact_processor.rs` | 30min |
| 3 | **Test gap** | `test_write_binary_from_cas_path` uses raw file, NOT CAS-compressed file — doesn't exercise NK framing decompression path | `writer.rs:521` | 30min |
| 4 | **Test gap** | No test for workflow-level `format: json` vs auto-promote precedence with media task | `artifact_processor.rs` | 20min |
| 5 | **Fragile** | Fallback MediaRef from JSON output (`artifact_processor.rs:224-268`) — fails on arrays (for_each) and nested objects | `artifact_processor.rs:224` | 1h |
| 6 | **UX** | No hash verification on artifact output — write succeeds but no read-back blake3 check | `writer.rs:259-329` | 1h |

### P2: Documentation Updates

The auto-promote behavior is NOT documented anywhere yet. These files mention `format: binary` as if it's required:

| File | What to Update |
|------|----------------|
| `tools/nika-cli/rules/claude.md` (+ roo.md, windsurf.md, copilot.md) | Add note: `format: binary` is optional for media-producing tasks (auto-detected) |
| `tools/nika/CLAUDE.md` | Update "Binary artifact" example to show optional `format:` |
| `nika/CLAUDE.md` (rules) | Update "Common Mistakes" table — `format: binary` is no longer a mistake to omit |
| `dx/.claude/rules/nika.md` | Same — update artifact examples |

### P3: From v0.51 Plans (already documented)

These overlap with `docs/plans/2026-03-28-v051-mega-handoff-prompt.md`:

- **Task 1.5:** Remove dead `media-compression` cfg guard (related to our integrity check fix)
- **Wave 4.1:** Split `rig.rs` (3,598 LOC) into 4 focused modules
- **Bug 1:** Thinking tokens not priced separately (cost.rs)
- **Bug 2:** Structured output retry loses temperature/system
- **Bug 3:** Agent verb thinking + tools compatibility

---

## Architecture Notes for Next Session

### Binary data flow (confirmed correct)

```
File → CasStore::store() → [NK framing + optional zstd] → disk
                                    ↓
disk → CasStore::read_raw() → transparent_decompress() → original bytes
                                    ↓
original bytes → write_atomic() → artifact output file
```

No String conversion anywhere. No base64 double-encoding. The pipeline is sound.

### Auto-promote precedence (new behavior)

```
1. Task-level explicit format    → used as-is (always wins)
2. Workflow-level format         → used as-is (wins over auto)
3. Auto-promote from media_refs  → Binary (when 1+2 are unset)
4. Default                       → Text (when nothing else applies)
```

Two code paths implement this:
- `ArtifactSpec::Enabled(true)` at line 86-94 (shorthand `artifact: true`)
- `write_single_artifact()` at line 210-221 (explicit `artifact: { path: ... }`)

### Compression framing

- `media-compression` is a DEFAULT feature on `nika-media`
- 4-byte header: `[N][K][flag][version]` where flag=0x00 (raw) or 0x01 (zstd)
- Legacy files (no NK prefix) are handled transparently
- Size check skipped at runtime because on-disk size is opaque

### Keychain access pattern

- `NikaKeyring::exists()` calls `entry.get_password()` → triggers macOS Keychain prompt
- Now short-circuited by checking env var first
- `NIKA_SKIP_KEYCHAIN=1` env var also available as escape hatch

---

## Test Commands

```bash
# Media pipeline tests
cargo test -p nika-engine --lib -- artifact_processor   # 55 tests
cargo test -p nika-media --lib                           # 328 tests
cargo test -p nika-engine --lib -- writer                # io/writer tests

# Full suite (safe, no keychain)
cargo test --workspace --lib                             # 8629+ tests

# Real workflow verification
cd /tmp && mkdir -p nika-test && cd nika-test
# Create test_input.png (100x100 red PNG)
NIKA_SKIP_KEYCHAIN=1 nika run test-import-artifact.nika.yaml --no-live
NIKA_SKIP_KEYCHAIN=1 nika run test-fetch-binary.nika.yaml --no-live
file output/*.png output/*.jpg  # Should show valid image formats
```
