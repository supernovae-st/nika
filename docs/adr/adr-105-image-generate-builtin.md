---
id: ADR-105
title: "nika:image_generate — the Media builtin rides a dedicated image plane"
status: accepted
date: 2026-07-05
phase: "post-0.93 · announce ladder"
deciders: ["@ThibautMelen"]
tags: ["nika-builtin", "media", "image-generation", "openai", "gemini", "mock", "image-plane", "provenance", "permits"]
affects_crates: ["nika-builtin", "nika-catalog", "nika-schema", "nika-cli", "nika-pack", "nika-verb-agent"]
affects_layers: ["L0", "L1.5", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-093", "ADR-095", "ADR-096"]
requires: []
enables: []
amends: []
fci: []
inv: ["INV-027"]
shadow_zones: []
nika_codes: ["NIKA-BUILTIN-IMAGE_GENERATE-001", "NIKA-BUILTIN-IMAGE_GENERATE-002", "NIKA-BUILTIN-IMAGE_GENERATE-003", "NIKA-BUILTIN-IMAGE_GENERATE-004", "NIKA-BUILTIN-IMAGE_GENERATE-005", "NIKA-BUILTIN-IMAGE_GENERATE-006", "NIKA-BUILTIN-IMAGE_GENERATE-007"]
timeline: "shipped 2026-07-05 · one arc"
follow_ups: ["mode: edit + reference_images (media roadmap)", "in-memory/artifact mode (save: false)", "RunContext seam → run/workflow ids in the manifest", "streaming/partial images", "EventKind::Extension wiring for image_generation.* events"]
---

# ADR-105: nika:image_generate — the Media builtin rides a dedicated image plane

## Context

Workflows increasingly need image ASSETS (OG covers, QR landing heroes, SEO
batches) as a step in a larger pipeline, not as a chat artifact. The stdlib
deferred the whole media class at the 42→22 consolidation; the spec kept a
`§Media · DEFERRED` placeholder. Meanwhile the two providers that matter
stabilized their image APIs (OpenAI `gpt-image-2` · Gemini
`gemini-3.1-flash-image`, primary-source verified 2026-07-05), and the
engine already owns every primitive the pipeline needs: the kernel http
seam, the atomic-write fs seam, the `permits.fs` runtime boundary, base64 +
sha256 helpers, and a builtin dispatcher with injected effect seams. Two
transport facts force a design decision: the FETCH http client carries a 30s
idle-read guard that kills 60–120s image renders (the F1 timeout class), and
image endpoints are engine-fixed constants — never workflow data.

## Decision

Ship **one official builtin, `nika:image_generate`**, as a **module family
inside `nika-builtin`** (`src/image/` · collapse-default rule — no new crate,
no 12-gate ceremony; feature-level gates still apply) and graduate the spec's
deferred Media class (23 → 24 canonical builtins).

- **The image plane** · provider calls ride a DEDICATED dispatcher seam —
  `image_http: Option<Arc<H>>` + `with_image_plane(http, keys)`, wired at the
  composition root with the PROVIDER-plane client (SSRF `Disabled`
  sanctioned: endpoints are const studio-fixed strings, same reasoning as
  `infer:` · 600s transport ceiling). Unwired ⇒ real providers fail with the
  precise `-002`; the mock provider works without it. `permits.net.http`
  does NOT govern the plane (like `infer:`); `permits.tools` + `permits.fs`
  DO.
- **Keys** · `ImageKeys { openai, gemini }` (zeroizing `Secret`s) resolved
  ONLY at the composition root's sanctioned env boundary
  (`NIKA_OPENAI_API_KEY → OPENAI_API_KEY` · `NIKA_GEMINI_API_KEY →
  GEMINI_API_KEY`). The builtin never reads env, never logs a credential.
- **Assets, not blobs** · V1 requires `save` (default true) — images land
  under `output_dir:` (boundary-gated per FINAL path before any I/O ·
  `NIKA-SEC-004`), outputs and the `manifest_version: 1` provenance manifest
  carry paths + dimensions + sha256. Base64 NEVER rides outputs/logs/traces;
  `debug: true` echoes a SANITIZED provider response (payloads stripped).
- **Validation is header-only** · magic bytes are the authority (PNG IHDR ·
  JPEG SOF · WebP VP8/VP8L/VP8X) — no pixel decode ⇒ no decompression-bomb
  surface; dimension/byte bounds refuse absurd payloads (`-007`).
- **Deterministic mock** · a hand-rolled stored-deflate PNG encoder
  (byte-stable forever · validated by the independent `png` dev-dep) makes
  the whole pipeline offline-testable, golden-grade.
- **Static surface** · catalog row (21-arg vocabulary · required
  `prompt`+`output_dir`) · model-facing ToolDef (drift-guarded against the
  catalog) · `builtin_shape` ladder (V1 reservations `mode: edit` /
  `reference_images` / `save: false` refused loudly · closed enums · ranges ·
  the transparent×jpeg conflict) · `BuiltinEffect::Fs { path_arg:
  "output_dir", writes, recursive }` so escape-check AND `--infer-permits`
  understand the recursive write (`<dir>/**`).
- **V1 boundaries stated, not faked** · `mode: edit`, `reference_images:`,
  `save: false`, streaming, Imagen `:predict` are RESERVED with pointed
  errors + roadmap notes.

## Consequences

- The stdlib counts 24 (catalog · defs · schema enum · spec canon.yaml ·
  conformance fixtures 004/005 · showcase `t1-og-images` — cascade complete,
  pack re-vendored).
- `n:` on gemini = n sequential calls (no verified `candidateCount` for
  images) — documented, usage accumulates across calls.
- The 64 MiB provider-response cap bounds `n × png` batches — a clean `-003`
  hints jpeg/webp+compression.
- `image_generation.*` observability events ride the Emitter seam (stderr
  today; the run-stream integration follows the documented
  `EventKind::Extension` path — FCI-009, unchanged here).
- Run/workflow ids are honestly ABSENT from the manifest (no RunContext
  seam yet — roadmap, never empty-stringed).
- **The 24th builtin crossed the agent router's activation threshold**
  (`RouterConfig::default().min_universe` was 24): a plain
  `tools: ["nika:*"]` agent would have silently flipped from
  deterministic full-catalog passthrough to BM25 narrowing. The default
  is raised to 32 (routing pays for itself on MCP-heavy universes, never
  on the stdlib alone) and a composition-root test pins
  `tool_defs().len() < min_universe` so the next builtin addition fails
  loudly instead of silently re-crossing.
- **`provider_text` is a caption, not a payload channel**: gemini's
  interleaved text is clamped (2 000 chars · stable
  `provider_text_clamped:` warning) and the `debug:` echo clamps ANY
  oversized non-payload string — a multimodal response cannot carry
  megabytes of text into `ToolResult.content`/the agent's next turn/the
  manifest (review-swarm refutation, 2026-07-05).
- **The image plane shares the dispatcher's `H` type parameter**
  (`Option<Arc<H>>`) rather than a `dyn` seam: `HttpPostDyn` is a
  `trait_variant` async trait (not dyn-compatible), and the house
  composes generics everywhere — both planes are `nika-http` clients
  with different configs. Accepted trade-off; a distinct provider-plane
  TYPE would need a boxing adapter (additive, if ever needed).
