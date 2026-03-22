# Plan: Refonte Structurelle du Cours Nika

> **Date:** 2026-03-19
> **Scope:** 17 documents + INDEX (~2.2 MB, ~65k lignes)
> **Baseline:** Docs ecrits pour Nika v0.30.5 / Schema @0.12
> **Target:** Nika v0.34.0 avec media pipeline, vision/multimodal, CAS storage
> **Duree estimee:** 12 jours ouvrables
> **Audit initial:** 75 issues (25 CRITICAL, 30 IMPORTANT, 20 MINOR)

---

## Principes anti-regression

1. **Ground Truth First** — Extraire les faits du code AVANT de toucher aux docs
2. **Automated Checker** — Script de regression execute a chaque checkpoint
3. **One Phase = One Shippable State** — Chaque phase laisse les docs coherents
4. **Agent Isolation** — Chaque agent modifie 2-3 docs max, jamais le meme doc en parallele
5. **Review Gate** — Aucune phase ne commence sans validation de la precedente
6. **Diff-Only Review** — Les agents reviewers lisent les diffs, pas les docs entiers
7. **CORRECTIONS.md v2** — Le fichier est mis a jour a chaque checkpoint

---

## Architecture des Agents

```
Orchestrateur (humain + Claude)
    |
    +-- Phase 0: 2 agents paralleles
    |   +-- Agent "ground-truth-extractor"  (Explore codebase)
    |   +-- Agent "checker-builder"         (Write verification script)
    |
    +-- Phase 1: 4 agents paralleles (patches chirurgicaux)
    |   +-- Agent "patch-versions"     (docs 00-16 + INDEX: version badges)
    |   +-- Agent "patch-counts"       (docs 00,01,05,09,14,16,INDEX: counts)
    |   +-- Agent "patch-factual"      (docs 04,07,08,10,15,16: factual errors)
    |   +-- Agent "patch-prefix"       (docs 07,08: nika_ -> nika:)
    |   +-- [GATE] Review Agent + Automated Checker
    |
    +-- Phase 2: 3 agents paralleles (content creation)
    |   +-- Agent "write-media-ref"    (reference block media pipeline)
    |   +-- Agent "write-vision-ref"   (reference block vision/multimodal)
    |   +-- Agent "write-errors-ref"   (reference block 20 error codes)
    |   +-- [GATE] Review Agent coherence des 3 blocks
    |
    +-- Phase 3: 6 batches sequentiels (per-doc enrichment)
    |   +-- Batch 3A: 3 agents // (docs 00, 01, 02)  -> [GATE]
    |   +-- Batch 3B: 3 agents // (docs 03, 04, 05)  -> [GATE]
    |   +-- Batch 3C: 2 agents // (docs 06, 07)       -> [GATE] MAJOR
    |   +-- Batch 3D: 2 agents // (docs 08, 09)       -> [GATE] MAJOR
    |   +-- Batch 3E: 3 agents // (docs 10, 11, 12)   -> [GATE]
    |   +-- Batch 3F: 2 agents // (docs 13, 14)       -> [GATE] MAJOR
    |
    +-- Phase 4: 2 agents paralleles (cookbook + exercises + INDEX)
    |   +-- Agent "enrich-cookbook"     (doc 15 + doc 16)
    |   +-- Agent "enrich-index"       (INDEX.md)
    |   +-- [GATE] Cross-coherence review
    |
    +-- Phase 5: 3 agents paralleles (cross-coherence)
    |   +-- Agent "harmonize-terms"    (terminology scan all docs)
    |   +-- Agent "add-navigation"     (prev/next + summaries all docs)
    |   +-- Agent "validate-xrefs"     (cross-references + internal links)
    |   +-- [GATE] Final review
    |
    +-- Phase 6: 2 agents paralleles (final validation)
        +-- Agent "full-read-review"   (read every doc start to finish)
        +-- Agent "regression-check"   (automated checker final run)
        +-- [GATE] Ship decision
```

---

## Phase 0 : Baseline & Outillage (Jour 0)

### Objectif
Etablir la verite terrain et construire un filet de securite automatise.

### Task 0.1 — Ground Truth Extraction

**Agent:** `Explore` (very thorough)
**Input:** Codebase `/Users/thibaut/dev/supernovae/nika/tools/nika/`
**Output:** `/Users/thibaut/Desktop/nika-cours/GROUND_TRUTH.md`

Extraire et documenter avec precision:

| Donnee | Fichier source | Methode |
|--------|---------------|---------|
| Version binaire | `Cargo.toml:3` | Read line |
| Schema version | `src/ast/schema.rs` | Grep CURRENT_SCHEMA |
| EventKind count + liste | `src/event/log.rs` | Count enum variants |
| TransformOp count + liste | `src/binding/transform.rs` | Count enum variants |
| NikaError variant count | `src/error.rs` | Count enum variants |
| Error code ranges complete | `src/error.rs:26-36` | Read comment block |
| MediaError codes 251-259 | `src/media/error.rs` | Count + list |
| Media tool error codes 290-297 | `src/runtime/builtin/media/error.rs` | Count + list |
| Artifact error codes 280-285 | `src/error.rs` | Grep 280-285 |
| Builtin tools core (7) | `src/runtime/builtin/mod.rs` | List exports |
| Builtin tools file (5) | `src/runtime/builtin/file/` | List modules |
| Media tools (18) + feature gates | `src/runtime/builtin/media/mod.rs` | Parse create_media_tool_adapters() |
| Provider enum variants | `src/provider/rig.rs` | Count RigProvider |
| CLI commands | `src/main.rs` | List Command enum |
| AST types (InferParams fields) | `src/ast/action.rs` | Read struct |
| ContentPart types | `src/ast/content.rs` | Read enums |
| Feature flags | `Cargo.toml [features]` | List |
| Constants (timeouts) | `src/util/constants.rs` | Read |
| TUI views | `src/tui/views/mod.rs` | List TuiView |
| LSP completions for infer | `src/lsp/handlers/completion.rs` | Read infer block |

Format de sortie:
```markdown
## Ground Truth — Nika v0.34.0 (extracted 2026-03-19)

### Versions
- Binary: 0.34.0
- Schema: @0.12
- nika-lsp standalone: 0.30.3

### EventKind (N variants)
1. WorkflowStarted
2. WorkflowCompleted
...

### TransformOp (31 variants)
1. Upper
2. Lower
...
```

### Task 0.2 — Automated Regression Checker

**Agent:** `shell-scripting:bash-pro`
**Output:** `/Users/thibaut/Desktop/nika-cours/check-docs.sh`

Script bash qui:
1. Prend `GROUND_TRUTH.md` comme reference
2. Grep chaque doc pour les claims verifiables
3. Reporte les divergences

Checks a implementer:

```bash
# Version badge check
grep -rn "v0\\.30\\.5" *.md        # Should find 0 after Phase 1
grep -rn "v0\\.34\\.0" *.md        # Should find N (one per doc with badge)

# EventKind count check
grep -rn "EventKind" *.md | grep -oP '\d+' | sort -u
# Should only show the correct count

# TransformOp count check
grep -rn -i "transform" *.md | grep -oP '\d+ (transforms|operations|TransformOp)'
# Should only show 31

# Prefix check
grep -rn "nika_" *.md | grep -v "nika_cours" | grep -v ".nika.yaml"
# Should find 0 (no underscore prefix)

# to_yaml check (doesn't exist)
grep -rn "to_yaml" *.md             # Should find 0

# parallel: alias check (doesn't exist)
grep -rn "parallel:" *.md           # Should find 0 (or only in notes saying it doesn't exist)

# $ prefix in with: bindings
# (harder to automate — flag for manual review)

# serde_yaml check (deprecated)
grep -rn "serde_yaml" *.md          # Should find 0 or only migration notes

# Error code completeness
for code in 251 252 253 254 255 256 257 258 259 283 284 285 290 291 292 293 294 295 296 297; do
  count=$(grep -rn "NIKA-$code" *.md | wc -l)
  echo "NIKA-$code: $count occurrences"
done
```

Exit codes:
- 0 = all checks pass
- 1 = warnings (non-blocking)
- 2 = failures (blocking — cannot proceed to next phase)

### Task 0.3 — Baseline Metrics

**Agent:** simple bash
**Output:** `/Users/thibaut/Desktop/nika-cours/BASELINE_METRICS.md`

```bash
for f in /Users/thibaut/Desktop/nika-cours/*.md; do
  lines=$(wc -l < "$f")
  words=$(wc -w < "$f")
  echo "$(basename $f) | $lines lines | $words words"
done
```

### Checkpoint 0 — Gate Criteria

- [ ] GROUND_TRUTH.md exists and is complete (all 20 categories)
- [ ] check-docs.sh runs and reports current failures (baseline)
- [ ] BASELINE_METRICS.md captures pre-modification state
- [ ] All 3 files committed to nika-cours (or tracked separately)

---

## Phase 1 : Patches Chirurgicaux P0 (Jour 1)

### Objectif
Corriger tout ce qui enseigne du faux. Zero nouveau contenu, uniquement des corrections.

### Task 1.1 — Version Badges

**Agent:** `general-purpose` (search & replace)
**Scope:** ALL 17 docs + INDEX
**Rule:** Pure search-replace, no content changes

| Search | Replace | Files |
|--------|---------|-------|
| `v0.30.5` | `v0.34.0` | All badges |
| `v0.30.3` (doc 11 header) | `v0.34.0` (main) + note `nika-lsp standalone: v0.30.3` | Doc 11 |
| `"Nika 0.12"` or `"Nika v0.12"` | `"Schema @0.12"` | Doc 13 |
| `nika-v0.30.5` (badge URL) | `nika-v0.34.0` | All badges |

**Verification:** `grep -rn "v0\.30\.[35]" *.md` returns 0 results

### Task 1.2 — Count Fixes

**Agent:** `general-purpose`
**Scope:** Docs 00, 01, 05, 09, 14, 16, INDEX

| Claim | Old Value | New Value | Files + Lines |
|-------|-----------|-----------|---------------|
| EventKind variants | "34" | "(N from GROUND_TRUTH)" | Doc 00 L590, L889 |
| EventKind variants | "32" | "(N from GROUND_TRUTH)" | Doc 09 L37, L574, L576, L585, L590, L667; INDEX |
| TransformOp | "27" | "31" | Doc 00 L578, L648, L882; Doc 05 L33, L53, L113, L2000, section title; INDEX |
| TransformOp | "32" | "31" | Doc 01 L70, L1600 |
| NikaError variants | "63" | "(N from GROUND_TRUTH)" | Doc 14 section 1.2 |
| Structured output layers | "5 couches" | "4 couches (Layers 0-3, Layer 1 future)" | Doc 07 section 3.7 |
| Transform Q05.3 | "27+" | "31" | Doc 16 Q05.3 |

**IMPORTANT:** Use GROUND_TRUTH.md values, never hardcode. If ground truth says 40 EventKind, use 40.

**Verification:** Run `check-docs.sh` — count checks should all pass

### Task 1.3 — Factual Error Fixes

**Agent:** `general-purpose`
**Scope:** Docs 04, 10, 15, 16

| Error | Fix | File |
|-------|-----|------|
| `get_ready_tasks()` pseudocode has `pending.contains()` | Remove the `Si pending.contains(task.name) -> skip (in flight)` line. Add note: "In-flight guard is implicit: spawned tasks are not re-scheduled because they only appear in `get_ready_tasks()` once." | Doc 04 section 20 |
| EventKind names in TUI mapping: `TaskRegistered`, `McpCallStarted`, `McpCallCompleted`, `AgentTurnStarted` | Replace with `TaskScheduled`, `McpInvoke`, `McpResponse`, `AgentTurn`. Fix field names too: `task_type` -> `verb`, `input` -> `inputs` | Doc 10 section 6.1 |
| `\| to_yaml` in transform examples | Replace with `\| to_json` | Doc 15 sections 7.4, 8.4, 4.7 |
| `parallel: N` alias for `concurrency:` | Remove `(ou parallel: N)` — alias does not exist | Doc 16 Q15.2 |
| Lab 1 solution `haiku: generate_haiku` | Change to `haiku: $generate_haiku` | Doc 16 Lab 1 solution |
| Boot sequence Q06.4 contradicts INDEX | Align with INDEX section "Boot Sequence (7 phases)" | Doc 16 Q06.4 |
| `serde_yaml` in Annexe B | Replace with `serde-saphyr 0.0.20` + migration note | INDEX Annexe B |
| `include:` legacy syntax without note | Add `> Note: \`include:\` est la forme legacy. Depuis @0.12, utilisez \`imports:\`.` | Doc 15 section 10.1 |

### Task 1.4 — Prefix Fix (nika_ -> nika:)

**Agent:** `general-purpose`
**Scope:** Docs 07, 08

**Strategy:**
1. `grep -n "nika_" doc07.md` — list all occurrences
2. For each: if it's a tool name reference (nika_sleep, nika_read, etc.), change to `nika:sleep`, `nika:read`
3. If it's a YAML field name or comment about the prefix convention, change text to `nika:`
4. In section 6.8: "begins with `nika_`" -> "begins with `nika:`"
5. Do NOT change `.nika.yaml` file extensions or `nika_cours` directory refs

**Verification:** `grep -rn "nika_" 07*.md 08*.md | grep -v ".nika.yaml" | grep -v "nika_cours"` returns 0

### Checkpoint 1 — Gate Criteria

- [ ] `check-docs.sh` passes with 0 FAILURES (warnings OK)
- [ ] `grep -rn "v0\.30\.[35]" *.md` returns 0 results
- [ ] `grep -rn "nika_" 07*.md 08*.md | grep -v ".nika.yaml"` returns 0
- [ ] `grep -rn "to_yaml" 15*.md` returns 0
- [ ] `grep -rn "parallel:" 16*.md` returns 0
- [ ] Diff review: only corrections, no new content, no structural changes
- [ ] CORRECTIONS.md updated: issues 1.1, 1.2, 2.1, 2.2, 2.3, 2.4 marked RESOLVED

**Review Agent:** `spn-powers:code-reviewer`
- Reads: `git diff` of all Phase 1 changes
- Checks: no accidental content deletion, no new errors introduced
- Reports: PASS/FAIL with specific line references

---

## Phase 2 : Reference Blocks (Jours 2-3)

### Objectif
Creer 3 blocs de contenu de reference qui seront tisses dans les docs existants en Phase 3.
Ces blocs sont des "sources uniques de verite" pour media, vision, et error codes.

### Task 2.1 — Media Pipeline Reference Block

**Agent:** `spn-writing:writing-orchestrator` + `feature-dev:code-explorer`
**Output:** `/Users/thibaut/Desktop/nika-cours/REF_MEDIA.md`
**Size target:** ~2500 words, FR, same style as existing docs

Sections a produire:

```markdown
# Reference: Media Pipeline

## 1. Architecture 3 Tiers

### Tier 1 — Always-on (4 outils, aucun feature flag)
| Outil | Description | Input | Output |
|-------|-------------|-------|--------|
| nika:import | Import fichier dans CAS | path | { hash, path, size_bytes, mime_type } |
| nika:dimensions | Dimensions image (headers only) | hash | { width, height, format } |
| nika:thumbhash | Placeholder 25 bytes | hash | { thumbhash_base64 } |
| nika:dominant_color | Palette couleurs | hash | { colors: [...] } |

### Tier 2 — media-core default (6 outils)
[... same table format ...]

### Tier 3 — Opt-in features (8 outils)
[... same table format with feature flag column ...]

## 2. CAS (Content-Addressable Storage)
- Hash-based deduplication
- MediaRef schema: { hash, path, size_bytes, mime_type, deduplicated }
- Sharded directory structure
- Zstd compression for non-media blobs (media-compression feature)

## 3. Architecture Interne
- MediaOp trait
- MediaToolAdapter bridge
- MediaToolContext (CAS, storage, budget)
- ComputePool for CPU-bound ops
- create_media_tool_adapters() factory

## 4. Securite Media
- decode_image_safe() — JAMAIS image::load_from_memory() directement
- sanitize_svg() — AVANT tout parsing SVG
- validate_import_path() — anti path-traversal
- Pre-read size check (50 MB limit)
- 30s timeout sur toutes les operations
- Sensitive directory blocklist

## 5. CLI: nika media
| Commande | Description |
|----------|-------------|
| nika media import <file> | Import dans CAS |
| nika media list | Table HASH, SIZE, PATH |
| nika media stats | Compteurs et taille |
| nika media tools | Liste des 18 outils |
| nika media clean [--dry_run] [--older_than] | GC avec safety checks |

## 6. Diagramme Mermaid (Architecture Media)
[Flowchart: YAML invoke -> BuiltinToolRouter -> MediaToolAdapter -> MediaOp -> CAS]
```

**Source files to read:**
- `src/runtime/builtin/media/mod.rs` (architecture, tool list)
- `src/runtime/builtin/media/context.rs` (MediaToolContext)
- `src/runtime/builtin/router.rs` (with_all_tools)
- `src/media/store.rs` (CAS)
- `src/cli/media.rs` (CLI commands)
- `CLAUDE.md` (canonical tool table)

### Task 2.2 — Vision/Multimodal Reference Block

**Agent:** `spn-writing:writing-orchestrator` + `feature-dev:code-explorer`
**Output:** `/Users/thibaut/Desktop/nika-cours/REF_VISION.md`
**Size target:** ~1500 words, FR

Sections a produire:

```markdown
# Reference: Vision & Multimodal

## 1. Syntaxe YAML content:
### Trois formes valides pour infer:
- `infer: "prompt simple"` — shorthand texte
- `infer: { prompt: "...", system: "..." }` — forme complete texte
- `infer: { content: [...] }` — mode vision, prompt optionnel

### ContentPart types
| Type | Champs | Exemple |
|------|--------|---------|
| text | text | `{ type: text, text: "Decris cette image" }` |
| image | source, detail? | `{ type: image, source: "{{with.photo.hash}}", detail: high }` |
| image_url | url, detail? | `{ type: image_url, url: "https://...", detail: low }` |

### ImageDetail enum
- auto (defaut), low, high

## 2. Pipeline AST 3 Phases
### Phase 1 (Raw): RawContentPart
- Parsed from YAML with Spanned metadata
- `src/ast/content.rs`

### Phase 2 (Analyzed): AnalyzedContentPart
- Validated, spans removed
- detail string -> ImageDetail enum

### Phase 3 (Runtime): ContentPart
- Serde-enabled, used in InferParams
- InferParams.content: Option<Vec<ContentPart>>

## 3. Execution Path
1. Runner dispatches infer task
2. Executor checks: content.is_some()?
3. YES -> Vision path:
   a. Read image blobs from CAS (base64 encode)
   b. Build Vec<UserContent>
   c. Emit VisionContentResolved event
   d. Call provider.infer_vision()
   e. Skip structured output engine
4. NO -> Standard text path (streaming)

## 4. Provider Methods
| Method | Streaming | Vision |
|--------|-----------|--------|
| infer() | No | No |
| infer_stream() | Yes | No |
| infer_vision() | No | Yes |
| infer_vision_stream() | Yes | Yes |

Providers without vision: DeepSeek, Native -> VisionNotSupported error

## 5. Cancellation
- cancel_token races CAS reads
- cancel_token races infer_vision() call
- Two additional tokio::select! points vs text path

## 6. Diagramme Mermaid (Vision Pipeline)
[Flowchart: YAML content: -> Parser -> Analyzer -> Lower -> Executor -> CAS Read -> Provider.infer_vision()]
```

**Source files to read:**
- `src/ast/content.rs` (all types)
- `src/ast/raw/action.rs` (RawInferAction.content)
- `src/ast/analyzed/task.rs` (AnalyzedInferAction.content)
- `src/ast/action.rs` (InferParams.content)
- `src/runtime/executor/verbs.rs` (run_infer vision path)
- `src/provider/rig.rs` (infer_vision, infer_vision_stream)

### Task 2.3 — Error Codes Reference Block

**Agent:** `general-purpose` + `feature-dev:code-explorer`
**Output:** `/Users/thibaut/Desktop/nika-cours/REF_ERRORS.md`
**Size target:** ~1000 words, EN (consistent with doc 14)

Sections a produire:

```markdown
# Reference: Missing Error Codes

## NIKA-251 to NIKA-259 — Media Pipeline Errors
Source: `src/media/error.rs`

| Code | Variant | Description | Recoverable |
|------|---------|-------------|-------------|
| 251 | MimeDetectionFailed | MIME type cannot be detected | No |
| 252 | UnsupportedMediaType | Recognized but not processable | No |
| 253 | MediaNotFound | Hash not found in CAS | No |
| 254 | HashMismatch | CAS read-back verification failed | No |
| 255 | MediaStoreIo | I/O error during CAS read/write | Yes (retry) |
| 256 | Base64DecodeFailed | Base64 decoding failed | No |
| 257 | Base64InputTooLarge | Content exceeds size limit | No |
| 258 | EmptyMediaContent | Content block decoded to 0 bytes | No |
| 259 | RunBudgetExceeded | Per-run media budget exceeded | No |

## NIKA-283 to NIKA-285 — Media Store Errors
Source: `src/error.rs`

| Code | Variant | Description | Recoverable |
|------|---------|-------------|-------------|
| 283 | MediaIntegrityWarning | CAS file deleted/corrupted | Yes (re-import) |
| 284 | MediaCleanupError | Cleanup operation failed | Yes (retry) |
| 285 | MediaStoreLocked | Another workflow holds the lock | Yes (wait) |

## NIKA-290 to NIKA-297 — Media Tool Errors
Source: `src/runtime/builtin/media/error.rs`

| Code | Factory | Description | Maps to |
|------|---------|-------------|---------|
| 290 | tool_error() | Generic media tool error | BuiltinToolError |
| 291 | unsupported_format() | Unsupported MIME for tool | BuiltinToolError |
| 292 | dependency_missing() | Feature not enabled | BuiltinToolError |
| 293 | timeout_error() | 30s timeout exceeded | BuiltinToolError |
| 294 | invalid_args() | Invalid tool parameters | BuiltinInvalidParams |
| 295 | pipeline_step_failed() | Pipeline step error | BuiltinToolError |
| 296 | pipeline_empty() | Pipeline has no steps | BuiltinToolError |
| 297 | security_violation() | SVG injection, path traversal | BuiltinToolError |

## New EventKind Variants (for Doc 09)
Source: `src/event/log.rs`

| Variant | Category | Fields |
|---------|----------|--------|
| MediaExtracted | Media | task_id, block_count, content_types |
| MediaProcessed | Media | task_id, hash, mime_type, size_bytes |
| MediaStored | Media | task_id, hash, path, size_bytes, verified, deduplicated, pipeline_ms |
| MediaStoreFailed | Media | task_id, hash, reason |
| MediaIntegrityCheck | Media | checked, warnings (no task_id) |
| VisionContentResolved | Vision | task_id, image_count, total_bytes, resolve_ms |
| MediaCleanup | Cleanup | removed, bytes_freed, dry_run (no task_id) |

## Updated ArtifactWritten Fields (for Doc 09)
| Field | Type | Note |
|-------|------|------|
| path | String | Resolved artifact path |
| size | u64 | Size in bytes |
| format | String | "text", "json", OR "binary" (NEW) |
| checksum | Option<String> | Blake3 from CAS (NEW, binary only) |
```

**Source files to read:**
- `src/error.rs` (lines 26-36 range index, full enum)
- `src/media/error.rs` (MediaError enum)
- `src/runtime/builtin/media/error.rs` (factory functions)
- `src/event/log.rs` (new EventKind variants)

### Checkpoint 2 — Gate Criteria

- [ ] REF_MEDIA.md exists, reviewed for accuracy against source files
- [ ] REF_VISION.md exists, reviewed for accuracy against source files
- [ ] REF_ERRORS.md exists, reviewed for accuracy against source files
- [ ] All 3 blocks cross-reference each other consistently
- [ ] Diagrammes Mermaid render correctly
- [ ] No claims in ref blocks contradict GROUND_TRUTH.md

**Review Agent:** `spn-powers:code-reviewer`
- Reads: The 3 REF_*.md files
- Cross-checks: Every claim against the source files listed
- Reports: Factual accuracy + internal consistency

---

## Phase 3 : Enrichissement Par Document (Jours 4-10)

### Objectif
Tisser le contenu des reference blocks dans chaque doc existant. Ajouter les sections manquantes.

### Regle Critique
> Chaque agent recoit REF_MEDIA.md, REF_VISION.md, REF_ERRORS.md comme input.
> Il COPIE/ADAPTE le contenu depuis ces refs, ne le reinvente PAS.
> Cela garantit la coherence cross-doc.

---

### Batch 3A — Foundation Docs (Jour 4)

#### Task 3A.1 — Doc 00: Introduction & Architecture

**Agent:** `general-purpose` (worktree isolation)
**Input:** REF_MEDIA.md, REF_VISION.md, GROUND_TRUTH.md
**Read first:** `/Users/thibaut/Desktop/nika-cours/00-introduction-architecture.md`

Modifications:

1. **Section 2.2 "Architecture interne"** — APPLICATION LAYER
   - After `runtime/builtin/` entry, add:
   ```
   runtime/builtin/media/    Media pipeline (18 outils, 3 tiers)
                              MediaOp trait + MediaToolAdapter bridge
                              Tier 1: import, dimensions, thumbhash, dominant_color
                              Tier 2: thumbnail, convert, strip, metadata, optimize, svg_render
                              Tier 3: phash, compare, pdf_extract, chart, provenance, verify, qr_validate
                              + pipeline (operation chainer)
   ```
   - Update `event/` entry: "(N EventKind variants)" from GROUND_TRUTH
   - Update `binding/transform.rs`: "31 built-in transforms"

2. **Section 2.2 "Architecture interne"** — INFRASTRUCTURE LAYER
   - Add `media/store.rs` entry: "CAS (Content-Addressable Storage) for binary artifacts"
   - Add `ast/content.rs` entry: "ContentPart types for vision/multimodal"

3. **Section 2.3** — Module structure tree
   - Add `src/runtime/builtin/media/` subtree
   - Add `src/ast/content.rs`
   - Add `src/media/` subtree

4. **Section 3** — Pipeline description
   - Add brief note about vision content path
   - Reference doc 06 for details

5. **Section 6.3** — Builtin tools list
   - Add `nika:sleep`, `nika:log`
   - Add note: "Avec media-core active (defaut), 18 outils media supplementaires — voir Doc 07 Annexe B"

**Anti-regression check:**
- All existing content preserved (no deletions)
- Version badge already fixed in Phase 1
- Count values match GROUND_TRUTH.md

#### Task 3A.2 — Doc 01: YAML Syntax Reference

**Agent:** `general-purpose` (worktree isolation)
**Input:** REF_VISION.md, REF_MEDIA.md, GROUND_TRUTH.md

Modifications:

1. **Section 2.3** "Reference des champs infer:" — Add new field:
   ```markdown
   ### `content:` — Contenu multimodal (vision)

   > Depuis Nika v0.33.0. Quand `content:` est present, `prompt:` devient optionnel.

   [Copy from REF_VISION.md section 1]
   ```

2. **Section 2.9** Quick Reference Card — Add `content:` to infer card

3. **Section 6.3** — Add media tools to agent tools list
   - Reference REF_MEDIA.md Tier list

4. **New section 23.X** — Recipe: Vision + Media Workflow
   ```yaml
   # Recipe: Import image, generate thumbnail, describe with vision
   schema: "@0.12"
   workflow: vision-pipeline
   tasks:
     import_photo:
       invoke: nika:import
       args:
         path: "./photo.jpg"
     thumb:
       invoke: nika:thumbnail
       args:
         hash: "{{with.photo.hash}}"
         width: 512
       with:
         photo: $import_photo
     describe:
       infer:
         content:
           - type: text
             text: "Describe this image in detail"
           - type: image
             source: "{{with.photo.hash}}"
             detail: high
       with:
         photo: $import_photo
   ```

#### Task 3A.3 — Doc 02: AST Phase 1 Parsing

**Agent:** `general-purpose` (worktree isolation)
**Input:** REF_VISION.md, GROUND_TRUTH.md

Modifications:

1. **Section 1.3** "Fichiers source" — Add:
   ```
   +-- content.rs    <- RawContentPart, AnalyzedContentPart, ContentPart, ImageDetail
   ```

2. **Section 5.1** Class diagram — Add to RawInferAction:
   ```
   + content: Option<Spanned<Vec<RawContentPart>>>
   ```

3. **Section 5.2** Tree listing — Add after `thinking_budget`:
   ```
   +-- content: Option<Spanned<Vec<RawContentPart>>>
       +-- RawContentPart::Text { text: Spanned<String> }
       +-- RawContentPart::Image { source: Spanned<String>, detail: Option<Spanned<String>> }
       +-- RawContentPart::ImageUrl { url: Spanned<String>, detail: Option<Spanned<String>> }
   ```

4. **New section 5.X** "RawContentPart — Vision Content Parts"
   - Copy from REF_VISION.md section 2.1
   - ~200 words explaining the 3 variants

5. **Section 5.3 / 8.2** — Add note about prompt optionality:
   ```
   > Note: `prompt` is technically required on the struct but treated as optional
   > when `content:` is present (parser sets it to Spanned::dummy("")).
   ```

### Checkpoint 3A — Gate Criteria (Batch 3A)

- [ ] `check-docs.sh` still passes
- [ ] Diff review: only additions, no unintended deletions
- [ ] `content:` field documented in docs 01 and 02
- [ ] Media module mentioned in doc 00 architecture
- [ ] All numbers from GROUND_TRUTH.md

**Review Agent:** `spn-powers:code-reviewer`

---

### Batch 3B — AST & Binding Docs (Jour 5)

#### Task 3B.1 — Doc 03: AST Phase 2 Analyze

**Agent:** `general-purpose`
**Input:** REF_VISION.md

Modifications:

1. **Section 2** Source files — Add `src/ast/content.rs`
2. **Section 11** AnalyzedInferAction — Add `content: Option<Vec<AnalyzedContentPart>>`
3. **New section 11b** — "AnalyzedInferAction — Vision/Multimodal Content"
   - Copy from REF_VISION.md section 2.2
4. **Section 5** Phase Summary Table — Fix NIKA-144 placement:
   - Phase 1: `NIKA-144 (InvalidValue for duplicate prefix)`
   - Remove NIKA-144 from Phase 9a

#### Task 3B.2 — Doc 04: AST Phase 3 DAG

**Agent:** `general-purpose`

Modifications (mostly already done in Phase 1):

1. **Section 24** StableDag — Add note about `src/tui/views/monitor.rs` using it
2. Verify get_ready_tasks() fix from Phase 1 is correct

#### Task 3B.3 — Doc 05: Binding System

**Agent:** `general-purpose`
**Input:** REF_MEDIA.md

Modifications (count fix already done in Phase 1):

1. **Section title** "The 27 Transforms" -> "The 31 Transforms" (if not done in Phase 1)
2. **Section 1** Intro example — Fix `weather.summary` -> `$weather.summary`
3. **New section 10.X** "Media Tool Result Bindings"
   ```markdown
   ### Media Tool Result Bindings

   Les outils media retournent un objet JSON structure. Exemple avec `nika:thumbnail`:

   ```yaml
   tasks:
     thumb:
       invoke: nika:thumbnail
       args: { hash: "abc123", width: 256 }
     report:
       infer:
         prompt: |
           Image redimensionnee: {{with.t.hash}} ({{with.t.width}}x{{with.t.height}})
       with:
         t: $thumb
   ```

   Les champs disponibles dependent de l'outil — voir REF_MEDIA.md pour le schema
   complet de chaque outil.
   ```

### Checkpoint 3B — Gate Criteria

- [ ] `check-docs.sh` passes
- [ ] AnalyzedContentPart documented in doc 03
- [ ] NIKA-144 phase placement fixed in doc 03
- [ ] $ prefix fixed in doc 05
- [ ] Media binding example added to doc 05

---

### Batch 3C — Runtime Docs (Jour 6) — MAJOR

#### Task 3C.1 — Doc 06: Runner & Executor

**Agent:** `general-purpose` (this is the most complex single doc change)
**Input:** REF_VISION.md, REF_MEDIA.md, GROUND_TRUTH.md

Modifications:

1. **Section 2** Runner Struct — Add after existing fields:
   ```markdown
   ### LockfileGuard (RAII)
   Cree au debut de `run()`. Ecrit `.nika-run.lock`, supprime au drop.
   Empeche `nika media clean` de faire du GC pendant l'execution.
   ```

2. **Section 3.1** Boot Sequence Diagram — Fix:
   - `with_file_tools()` -> `with_all_tools(file_ctx, media_ctx)`
   - "12 nika:* tools" -> "12 core+file + N media tools"
   - Add step: "Create MediaToolContext / CasStore"

3. **Section 3** TaskExecutor Struct — Add field:
   ```
   cas: Arc<CasStore>    // Vision content resolution — reads CAS blobs for infer_vision
   ```

4. **NEW Section 3.X** "Vision Execution Path"
   ```markdown
   ## Vision Execution Path

   Quand `infer.content` est non-vide, l'executor emprunte un chemin dedie:

   1. Lire les blobs image depuis CAS (base64 encode)
   2. Construire `Vec<UserContent>` pour le provider
   3. Emettre `VisionContentResolved { image_count, total_bytes, resolve_ms }`
   4. Appeler `provider.infer_vision()` (pas de streaming)
   5. **Skip** le structured output engine

   [Copy Mermaid diagram from REF_VISION.md section 3]

   ### Cancellation Points (Vision)
   Le cancel_token est verifie a 2 points supplementaires:
   - Pendant la lecture CAS (tokio::select! sur chaque blob)
   - Pendant l'appel infer_vision() (tokio::select! sur le call)
   ```

5. **Section 8.2** Cancellation — Add vision cancel checkpoints

6. **Section 18** execute() Dispatch — Add vision branch to the dispatch description

#### Task 3C.2 — Doc 07: Les 5 Verbes

**Agent:** `general-purpose`
**Input:** REF_MEDIA.md, REF_VISION.md, GROUND_TRUTH.md

Modifications:

1. **Section 3.1** infer signature — Add `content:` field:
   ```yaml
   infer:
     prompt: "..."           # Optionnel si content: present
     content:                # Vision/multimodal (nouveau v0.33+)
       - type: text
         text: "Describe this image"
       - type: image
         source: "{{with.photo.hash}}"
         detail: high
     system: "..."
     # ... rest unchanged
   ```

2. **Section 3.7** Structured Output — Fix "5 couches" -> "4 couches (Layers 0-3)"
   - Add note: Layer 1 (rig Extractor) is future/planned
   - Remove or clearly mark Layer 4 as non-existent
   - Add note: "Le structured output n'est PAS applique aux reponses vision"

3. **Section 6.8** Builtin tools dispatch — Already fixed nika: prefix in Phase 1

4. **Section 10.2** Annexe B — Builtin tools reference — MAJOR addition:
   ```markdown
   ### Media Tools (18 outils, actives par feature flags)

   #### Tier 1 — Always-on
   [Copy table from REF_MEDIA.md section 1, Tier 1]

   #### Tier 2 — media-core (defaut)
   [Copy table from REF_MEDIA.md section 1, Tier 2]

   #### Tier 3 — Opt-in
   [Copy table from REF_MEDIA.md section 1, Tier 3]
   ```

### Checkpoint 3C — Gate Criteria (MAJOR)

This is the most critical checkpoint. These 2 docs carry the bulk of new content.

- [ ] `check-docs.sh` passes
- [ ] Vision execution path fully documented in doc 06
- [ ] content: field in infer: signature in doc 07
- [ ] All 18 media tools listed in doc 07 Annexe B
- [ ] CasStore field documented in executor struct
- [ ] LockfileGuard documented
- [ ] Structured output layers corrected
- [ ] No nika_ prefix remaining
- [ ] Diff review: additions are consistent with REF_*.md blocks

**Review Agent:** `spn-powers:code-reviewer` — THOROUGH review required

---

### Batch 3D — Provider & Event Docs (Jour 7) — MAJOR

#### Task 3D.1 — Doc 08: MCP & Providers

**Agent:** `general-purpose`
**Input:** REF_VISION.md, GROUND_TRUTH.md

Modifications:

1. **Section 13.7** "API d'inference" — Add 2 methods:
   ```markdown
   | `infer_vision()` | Content multimodal | `Vec<UserContent>` | `String` | Vision-capable uniquement |
   | `infer_vision_stream()` | Idem avec streaming | `Vec<UserContent>` + `Sender` | stream | Vision-capable uniquement |
   ```
   Add note: "DeepSeek et Native retournent `VisionNotSupported`"

2. **Section 15** / **Section 13.1** — Clarify provider count:
   ```
   RigProvider enum: 8 variants (7 cloud + 1 Native)
   KNOWN_PROVIDERS registry: 19 entries (includes aliases like "claude" -> "anthropic")
   ```

3. **Section 14** — Add CasStore to executor description:
   ```
   cas: Arc<CasStore>  // Used by vision content resolution
   ```

#### Task 3D.2 — Doc 09: Events & Security — MAJOR

**Agent:** `general-purpose`
**Input:** REF_ERRORS.md, REF_MEDIA.md, GROUND_TRUTH.md

Modifications:

1. **Title + section 9** — Update count: "Les N EventKind" (from GROUND_TRUTH)

2. **Section 9** Table — Add 7 new variants with full payload tables:
   ```markdown
   ### 9.11 Media Events (5 variants)

   #### MediaExtracted
   | Champ | Type | Description |
   |-------|------|-------------|
   | task_id | String | Task emettrice |
   | block_count | usize | Nombre de blocs media trouves |
   | content_types | Vec<String> | Types MIME detectes |

   [... same for MediaProcessed, MediaStored, MediaStoreFailed, MediaIntegrityCheck ...]

   ### 9.12 Vision Events (1 variant)

   #### VisionContentResolved
   [Copy from REF_ERRORS.md]

   ### 9.13 Media Cleanup Events (1 variant)

   #### MediaCleanup
   [Copy from REF_ERRORS.md]
   ```

3. **Section 9.9** ArtifactWritten — Add `checksum` field and `"binary"` format

4. **New section 34b** "Media Security Rules"
   - Copy from REF_MEDIA.md section 4 (Security)
   - Integrate with existing security model

5. **Section 19** / Annexe E — Add jq examples for media events:
   ```bash
   # Debit media pipeline
   jq 'select(.kind == "MediaStored") | {hash, size_bytes, pipeline_ms, deduplicated}'

   # Taux de deduplication
   jq 'select(.kind == "MediaStored" and .deduplicated == true)' | wc -l

   # Latence vision resolution
   jq 'select(.kind == "VisionContentResolved") | {image_count, total_bytes, resolve_ms}'
   ```

### Checkpoint 3D — Gate Criteria (MAJOR)

- [ ] `check-docs.sh` passes (especially error code checks)
- [ ] All 7 new EventKind variants documented with payload tables
- [ ] infer_vision methods documented in doc 08
- [ ] Provider count clarified
- [ ] Media security section added to doc 09
- [ ] jq examples added
- [ ] ArtifactWritten updated

**Review Agent:** `spn-powers:code-reviewer`

---

### Batch 3E — UI & Config Docs (Jour 8)

#### Task 3E.1 — Doc 10: TUI Architecture

**Agent:** `general-purpose`

Modifications:

1. **Section 10.1** CLI table — Add `nika media` entry
2. **Section 6.1** Event-to-TuiState mapping — Add VisionContentResolved, MediaProcessed, MediaStored
   (EventKind names already fixed in Phase 1)
3. **Section 10.5** — Add Studio aliases (`--view=editor`, `--view=explorer`, `--view=home`)
4. **New section 10.9** — "Media CLI Mode" (brief, reference doc 07/REF_MEDIA for details)

#### Task 3E.2 — Doc 11: LSP

**Agent:** `general-purpose`

Modifications:

1. **Version** — Already fixed in Phase 1, verify standalone note added
2. **Section 7.3** — Add `content:` to infer completions list
3. **Section 6.5** — Add `content:` to FIELD_DOCUMENTATION list
4. **New section** — Document `model_intel.rs` module:
   - Vision capability detection (`labels.push("vision")`)
   - Extended thinking support
   - Per-model capability tables
5. **Section 1.1** — Update MCP tool count for standalone crate

#### Task 3E.3 — Doc 12: Registry, Secrets, Init

**Agent:** `general-purpose`
**Input:** REF_MEDIA.md

Modifications:

1. **Section 10** — Add media tools section:
   ```markdown
   ### 10.8 Media Tools

   Le `BuiltinToolRouter` supporte un troisieme constructeur:

   `with_all_tools(file_ctx, media_ctx)` — enregistre les 12 outils core+file
   PLUS les outils media actives par feature flags.

   [Brief table from REF_MEDIA.md, pointing to doc 07 Annexe B for full list]
   ```
2. **Section 10.6** — Add note about NIKA-210 dual usage
3. **Section 2.2** — Clarify provider count (13 in table vs 19 in registry)

### Checkpoint 3E — Gate Criteria

- [ ] `check-docs.sh` passes
- [ ] nika media CLI documented in doc 10
- [ ] content: in LSP completions in doc 11
- [ ] model_intel.rs documented in doc 11
- [ ] Media tools section in doc 12
- [ ] No deletion of existing content

---

### Batch 3F — Reference Docs (Jour 9) — MAJOR

#### Task 3F.1 — Doc 13: Structured Output, Tools, Permissions

**Agent:** `general-purpose`
**Input:** REF_MEDIA.md

Modifications:

1. **Section 4.1** Diagram — Add media branch to BuiltinToolRouter dispatch
2. **Section 4.3** — Add `with_all_tools()` constructor documentation
3. **New section 4.6** — MediaToolAdapter dispatch:
   ```markdown
   ### 4.6 MediaToolAdapter → MediaOp Bridge

   Pour les outils media, le dispatch suit un chemin specifique:

   BuiltinToolRouter
     -> MediaToolAdapter.execute()
       -> MediaOp.execute(args, context)
         -> CAS read/write
         -> Return MediaOpResult (Metadata | Binary)
   ```
4. **Section 6** / **Section 5.9** — Add media tool security model

#### Task 3F.2 — Doc 14: Error Codes Reference — MAJOR

**Agent:** `general-purpose`
**Input:** REF_ERRORS.md, GROUND_TRUTH.md

This is the most critical single-doc update. 20 error codes need to be added.

Modifications:

1. **Section 1.2** — Update NikaError variant count diagram (from GROUND_TRUTH)

2. **Section 2** Quick Reference Table — Add 3 new rows:
   ```
   | 251-259 | Media Pipeline | 9 variants | [14] | src/media/error.rs |
   | 283-285 | Media Store | 3 variants | [14] | src/error.rs |
   | 290-297 | Media Tools | 8 variants | [14] | src/runtime/builtin/media/error.rs |
   ```

3. **Section 2.21** — Rename to "NIKA-250: Context Errors" (single code)

4. **NEW Section 2.21b** "NIKA-251-259: Media Pipeline Errors"
   - Copy full table from REF_ERRORS.md section 1

5. **Section 2.24** — Rename to "NIKA-280-285: Artifacts & Media Store"
   - Add NIKA-283, 284, 285 from REF_ERRORS.md section 2

6. **NEW Section 2.27** "NIKA-290-297: Media Tool Errors"
   - Copy full table from REF_ERRORS.md section 3

7. **Visual flowchart range map** — Add 3 new nodes for media ranges

8. **Annexe A** — Verify NIKA-160 dual usage still correct (it is per CORRECTIONS.md)

### Checkpoint 3F — Gate Criteria (MAJOR)

- [ ] `check-docs.sh` passes — ESPECIALLY error code completeness:
  ```
  for code in 251..259 283..285 290..297; do
    grep -c "NIKA-$code" 14*.md > 0
  done
  ```
- [ ] All 20 new error codes present in doc 14
- [ ] NikaError variant count matches GROUND_TRUTH
- [ ] with_all_tools() documented in doc 13
- [ ] Router diagram updated in doc 13

**Review Agent:** `spn-powers:code-reviewer` — THOROUGH

---

## Phase 4 : Cookbook, Exercises & INDEX (Jour 10)

### Task 4.1 — Doc 15: Advanced Patterns Cookbook

**Agent:** `general-purpose`
**Input:** REF_MEDIA.md, REF_VISION.md

Modifications:

1. **New Section 17** "Patterns Media Pipeline" (~1500 words):
   ```markdown
   ## 17. Patterns Media Pipeline

   ### 17.1 Pattern: Import + Dimensions + Thumbhash (Tier 1)
   [YAML workflow: nika:import -> nika:dimensions -> nika:thumbhash]
   Tous les outils Tier 1, zero feature flag supplementaire.

   ### 17.2 Pattern: Image Processing Chain (Tier 2)
   [YAML workflow: nika:import -> nika:thumbnail -> nika:convert -> nika:strip]
   Necessite media-thumbnail feature.

   ### 17.3 Pattern: Vision Multimodal
   [YAML workflow: nika:import -> infer: { content: [...] }]
   Combine media pipeline et vision inference.

   ### 17.4 Pattern: QR Generation + Validation
   [YAML workflow: generate QR -> nika:import -> nika:qr_validate]
   Necessite media-qr feature.

   ### 17.5 Pattern: PDF + Text Analysis
   [YAML workflow: nika:import -> nika:pdf_extract -> infer:]

   ### 17.6 Anti-pattern: Direct image::load_from_memory()
   [Explain why decode_image_safe() is required, reference security rules]
   ```

2. **Section 4.3** Builtin tools table — Add note pointing to new section 17
3. **Verify** all `| to_yaml` replaced (Phase 1)
4. **Verify** `include:` migration note added (Phase 1)

### Task 4.2 — Doc 16: Exercises & Labs

**Agent:** `general-purpose`
**Input:** REF_MEDIA.md

Modifications:

1. **New Lab 6** "Media Processing Pipeline" (~500 words):
   ```markdown
   ## Lab 6 — Media Processing Pipeline (Intermediaire, 30 min)

   ### Objectif
   Construire un workflow qui importe une image, genere un thumbnail,
   extrait les couleurs dominantes, et produit un rapport.

   ### Prerequis
   - Nika v0.34.0 avec media-core active (defaut)
   - Une image test (JPEG ou PNG)

   ### Etapes
   1. Creer le fichier `media-lab.nika.yaml`
   2. Task 1: `invoke: nika:import` avec path vers l'image
   3. Task 2: `invoke: nika:dimensions` avec hash du Task 1
   4. Task 3: `invoke: nika:thumbnail` avec width: 256
   5. Task 4: `invoke: nika:dominant_color`
   6. Task 5: `infer:` avec prompt utilisant les resultats

   ### Solution
   [Complete YAML workflow]

   ### Bonus
   - Ajouter `nika:thumbhash` pour generer un placeholder
   - Combiner avec `content:` pour une description vision
   ```

2. **Verify** Lab 1 $ prefix fix (Phase 1)
3. **Verify** Q05.3 transform count fix (Phase 1)
4. **Verify** Q15.2 parallel: fix (Phase 1)
5. **Verify** Q06.4 boot sequence fix (Phase 1)
6. **New QCM** section for media:
   ```
   Q17.1: Quel est le format de retour de nika:import?
   Q17.2: Quelle commande CLI liste les outils media?
   Q17.3: Quel code d'erreur indique un path traversal?
   ```

### Task 4.3 — INDEX.md

**Agent:** `general-purpose`
**Input:** REF_MEDIA.md, REF_VISION.md, REF_ERRORS.md, GROUND_TRUTH.md

Modifications:

1. **Version badge** — Already fixed in Phase 1, verify `v0.34.0`

2. **Section 3.1** Glossaire Types & Structs — Add:
   ```
   | MediaRef | Objet retour des outils media binaires: { hash, path, size_bytes, mime_type, deduplicated } | [07], [15] |
   | ContentPart | Type de contenu vision/multimodal (Text, Image, ImageUrl) | [02], [03], [06] |
   | CasStore | Store content-addressable pour les blobs media | [06], [12] |
   | MediaOp | Trait interne des outils media | [07], [13] |
   | MediaToolAdapter | Bridge MediaOp -> BuiltinTool | [07], [13] |
   | ImageDetail | Enum vision: Auto, Low, High | [01], [02] |
   ```

3. **Section 3.3** Champs YAML — Add:
   ```
   | content: | Contenu multimodal vision | @0.12 (v0.33+) | [01], [07] | infer: { content: [...] } |
   ```

4. **Section 5.1** Error code table — Add 3 new ranges (from Checkpoint 3F)

5. **Section 7** Parcours — Add:
   ```markdown
   ### Parcours F: Developpeur Media (2-3h)

   Pour les developpeurs QR Code AI et media pipeline.

   | Etape | Document | Sections | Objectif |
   |-------|----------|----------|----------|
   | 1 | 01 YAML Syntax | §2.3 content: | Syntaxe vision |
   | 2 | 07 Les 5 Verbes | §3.1, §6, Annexe B | invoke: media tools |
   | 3 | 06 Runner | §3.X Vision Path | Comprendre l'execution |
   | 4 | 15 Cookbook | §17 Media Patterns | Patterns pratiques |
   | 5 | 16 Exercices | Lab 6 | Mise en pratique |
   ```

6. **Section 8** Quick Reference Card — Add:
   ```
   # Media (invoke:)
   invoke: nika:import      # Import fichier dans CAS
   invoke: nika:thumbnail   # Redimensionner (Lanczos3)
   invoke: nika:qr_validate # Decoder + scorer QR code

   # Vision (infer: content:)
   infer:
     content:
       - type: image
         source: "{{with.img.hash}}"
   ```

7. **Section 10** Checklist prof — Fix version `v0.34.0+`

8. **Annexe B** — Replace `serde_yaml` with `serde-saphyr 0.0.20` (verify Phase 1)

9. **BuiltinToolRouter** glossaire entry — Add media tools note

### Checkpoint 4 — Gate Criteria

- [ ] `check-docs.sh` passes (full run)
- [ ] Doc 15 has section 17 with 6 media patterns
- [ ] Doc 16 has Lab 6 + media QCM
- [ ] INDEX has Parcours F, glossaire media, error code ranges, Quick Reference media
- [ ] All REF_*.md content woven in (no orphan references)

**Review Agent:** `spn-powers:code-reviewer`

---

## Phase 5 : Cross-Coherence (Jour 11)

### Task 5.1 — Terminology Harmonization

**Agent:** `general-purpose`
**Scope:** All 17 docs + INDEX

Checks and fixes:

| Term | Canonical Form | Grep for variants |
|------|---------------|-------------------|
| with: block | `with: bindings` | `grep -rn "with: block" *.md` — add "(aussi appele with: block)" |
| with block (no colon) | `with:` (always with colon) | `grep -rn "with block" *.md` |
| nika: prefix | `nika:` (colon) | `grep -rn "nika_" *.md` — should be 0 |
| imports: | `imports:` (not `include:`) | `grep -rn "^include:" *.md` — should have migration note |
| $ prefix in bindings | `$task_id` | Spot-check 5 random YAML examples per doc |
| content: | `content:` (for vision) | Should appear in docs 01,02,03,06,07,08,15 |

### Task 5.2 — Navigation + Summaries

**Agent:** `general-purpose`
**Scope:** All 17 docs

Add to EACH doc:

1. **Top of document** (after title):
   ```markdown
   > [<- Doc NN-1: Previous Title](NN-1-filename.md) | [Index](INDEX.md) | [Doc NN+1: Next Title ->](NN+1-filename.md)
   ```
   (Doc 00 has no prev, doc 16 has no next)

2. **Bottom of document** (before annexes):
   ```markdown
   ---

   ## Ce que tu as appris

   Dans ce document, tu as decouvert:
   - [3-5 bullet points summarizing key concepts]
   - [Include media/vision points where applicable]

   ---

   > [<- Doc NN-1: Previous Title](NN-1-filename.md) | [Index](INDEX.md) | [Doc NN+1: Next Title ->](NN+1-filename.md)
   ```

### Task 5.3 — Cross-Reference Validation

**Agent:** `general-purpose`
**Scope:** All docs

1. For each `[Doc N]` or `(NN-filename.md)` reference in any doc:
   - Verify target file exists
   - Verify section anchor exists (if referenced)

2. For each error code mentioned in docs 00-13, 15, 16:
   - Verify it appears in doc 14

3. For each type mentioned in structural docs (02, 03, 04):
   - Verify it appears in INDEX glossaire

### Checkpoint 5 — Gate Criteria

- [ ] `check-docs.sh` passes
- [ ] Every doc has prev/next navigation
- [ ] Every doc has "Ce que tu as appris" summary
- [ ] No broken internal links
- [ ] Terminology grep checks all pass
- [ ] All error codes cross-referenced to doc 14

**Review Agent:** `spn-powers:code-reviewer` — cross-doc focus

---

## Phase 6 : Validation Finale (Jour 12)

### Task 6.1 — Full Read-Through

**Agent:** `spn-powers:code-reviewer` (7 parallel agents, 2-3 docs each)
**Scope:** Every doc, read start to finish

Each agent checks:
1. Internal consistency (no contradictions within the doc)
2. Factual accuracy (claims match GROUND_TRUTH.md)
3. Readability and flow (new sections integrate smoothly)
4. Mermaid diagrams render correctly
5. Code examples are valid YAML
6. No TODOs or placeholders left

### Task 6.2 — Automated Regression Final Run

**Agent:** `shell-scripting:bash-pro`
**Script:** `check-docs.sh` (from Phase 0)

Must return exit code 0 (all checks pass, no warnings).

### Task 6.3 — CORRECTIONS.md v2

**Agent:** `general-purpose`
**Output:** Rewrite `/Users/thibaut/Desktop/nika-cours/CORRECTIONS.md`

Format:
```markdown
# Corrections et Erreurs — Audit v2 (2026-03-XX)

## Issues Resolues (Phase 1-5)
[List every original CORRECTIONS.md issue with RESOLVED status]

## Issues Nouvelles (if any found during Phase 6)
[Any new issues discovered]

## Metrics
- Issues resolues: N/N
- Documents modifies: 17/17
- Nouveaux contenus: ~X lignes ajoutees
- Error codes documentes: N (was N-20)
- Media tools documentes: 18 (was 0)
```

### Task 6.4 — GROUND_TRUTH v2 Refresh

Re-run Task 0.1 against the documentation to verify alignment.
This time, extract claims FROM the docs and compare TO the code.

### Final Gate — Ship Decision

- [ ] `check-docs.sh` exit code 0
- [ ] Full read-through: all 7 agents report PASS
- [ ] CORRECTIONS.md v2 shows 0 unresolved issues
- [ ] GROUND_TRUTH v2 matches documentation claims
- [ ] `diff --stat` shows expected change volume
- [ ] No reviewer flagged regressions

**Decision:**
- ALL PASS -> Ship (commit + push)
- ANY FAIL -> Fix and re-review (loop back to relevant phase)

---

## Appendix A: Agent Configuration Reference

| Agent Name | Type | Parallel? | Input | Output |
|------------|------|-----------|-------|--------|
| ground-truth-extractor | Explore (very thorough) | Phase 0 | Codebase | GROUND_TRUTH.md |
| checker-builder | shell-scripting:bash-pro | Phase 0 | GROUND_TRUTH.md | check-docs.sh |
| patch-versions | general-purpose | Phase 1 // | All docs | Version fixes |
| patch-counts | general-purpose | Phase 1 // | Targeted docs | Count fixes |
| patch-factual | general-purpose | Phase 1 // | Targeted docs | Factual fixes |
| patch-prefix | general-purpose | Phase 1 // | Docs 07, 08 | Prefix fixes |
| write-media-ref | writing-orchestrator | Phase 2 // | Codebase | REF_MEDIA.md |
| write-vision-ref | writing-orchestrator | Phase 2 // | Codebase | REF_VISION.md |
| write-errors-ref | code-explorer | Phase 2 // | Codebase | REF_ERRORS.md |
| enrich-doc-NN | general-purpose | Phase 3 (per batch) | Doc + REFs | Updated doc |
| review-batch-NX | code-reviewer | Phase 3 gates | Diffs | PASS/FAIL |
| enrich-cookbook | general-purpose | Phase 4 // | Docs 15,16 | Updated docs |
| enrich-index | general-purpose | Phase 4 // | INDEX | Updated INDEX |
| harmonize-terms | general-purpose | Phase 5 // | All docs | Term fixes |
| add-navigation | general-purpose | Phase 5 // | All docs | Nav + summaries |
| validate-xrefs | general-purpose | Phase 5 // | All docs | Link validation |
| full-read-NN | code-reviewer | Phase 6 // x7 | 2-3 docs each | PASS/FAIL |
| regression-final | bash-pro | Phase 6 | All docs | Exit code |

## Appendix B: File Inventory

| File | Phase Created | Purpose |
|------|--------------|---------|
| GROUND_TRUTH.md | Phase 0 | Authoritative facts from codebase |
| BASELINE_METRICS.md | Phase 0 | Pre-modification line/word counts |
| check-docs.sh | Phase 0 | Automated regression checker |
| REF_MEDIA.md | Phase 2 | Media pipeline reference block |
| REF_VISION.md | Phase 2 | Vision/multimodal reference block |
| REF_ERRORS.md | Phase 2 | Error codes reference block |
| CORRECTIONS.md | Phase 6 | Updated audit report |

## Appendix C: Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Agent modifies wrong section | Each agent reads full doc first, targets specific sections |
| Inconsistent numbers across docs | All agents use GROUND_TRUTH.md, never hardcode |
| Regression in existing content | check-docs.sh runs at every checkpoint |
| Two agents modify same file | Batch design ensures no parallel writes to same file |
| New content contradicts existing | REF_*.md blocks are single source of truth |
| Mermaid diagrams break | Review agent verifies rendering |
| YAML examples invalid | Review agent validates syntax |
| Phase N starts before Phase N-1 validated | Gate criteria are blocking — cannot proceed without PASS |
