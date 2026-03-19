# Post-PR3a Testing + PR3b Plan

> PR3a merged (v0.33.0). 28/28 multi-provider tests pass. This plan covers deep media testing then PR3b implementation.

---

## Phase H1: Deep Media Tool E2E (2h)

### H1.1: Create real test images programmatically

Generate PNG, JPEG, SVG via exec + nika:write in workflows.
Use Python PIL or pure-byte construction for test fixtures.

### H1.2: Store in CAS + invoke every media tool

For each generated image:
1. Store in CAS via workflow ingest
2. nika:dimensions — verify width/height
3. nika:thumbhash — verify base64 output, 3-28 bytes
4. nika:dominant_color — verify hex colors match
5. nika:thumbnail — verify output smaller than input
6. nika:optimize — verify PNG size reduced
7. nika:convert (png-jpeg, jpeg-png, png-webp) — verify format
8. nika:strip — verify metadata removed
9. nika:svg_render — verify PNG output from SVG
10. nika:metadata — verify EXIF/dimensions extracted

### H1.3: Verify artifacts

- Write binary artifacts to disk from media tool outputs
- Verify files exist and are valid images
- Check file sizes match expected ranges

### H1.4: Verify telemetry

- Parse NDJSON traces for every workflow
- Assert: mcp_invoke + mcp_response for every tool call
- Assert: duration_ms reasonable
- Assert: no error events for successful workflows

### Success criteria
- 50+ workflows with real images
- Every media tool tested with real image data
- Artifacts written to disk and verified
- Telemetry complete and correct

---

## Phase H2: Cross-tool Pipeline Workflows (1h)

### Complex pipelines to test chaining

1. PNG - thumbnail(256) - optimize(level 4) - artifact
2. PNG - convert(jpeg) - strip - dimensions - log
3. SVG - svg_render(512x512) - thumbnail(128) - optimize - artifact
4. exec(generate image) - infer(describe) - log
5. fetch(download image URL) - store - process with media tools

### Verify
- CAS deduplication when same image processed twice
- Budget tracking across pipeline steps
- Template bindings chain correctly between steps

---

## Phase H3: Error Recovery + Edge Cases (1h)

### Error scenarios
1. Invalid CAS hash - NIKA-253
2. Corrupt image data - NIKA-290
3. Wrong format for tool (audio to dimensions) - NIKA-291
4. Budget exhaustion - NIKA-259
5. Cancelled workflow - check_cancelled works

### Edge cases
1. 1x1 pixel image through all tools
2. Very large image (5000x5000) through thumbnail
3. Transparent PNG to JPEG (verify white background)
4. Empty SVG to svg_render
5. SVG with text (fontless environment behavior)

---

## Phase H4: Multi-Provider Media Workflows (1h)

### OpenAI + Gemini + xAI
- Same prompt on all 3 providers, compare outputs
- infer + builtin tool chains on each provider
- Structured JSON output on each provider
- Error handling differences between providers

---

## Phase PR3b: Superpowers (v0.34.0)

### PR3b Scope (12 commits)

| # | Tool | Crate | Feature |
|---|------|-------|---------|
| C1 | nika:ocr | ocrs + rten | media-ocr |
| C2 | nika:chart | charts-rs | media-chart |
| C3 | nika:qr_validate | qrcode-ai-scanner-core | media-qr |
| C4 | nika:phash | image_hasher | media-phash |
| C5 | nika:compare | image_hasher + pixel diff | media-phash |
| C6 | nika:provenance | c2pa | media-provenance |
| C7 | nika:pdf_extract | pdf-extract + lopdf | media-pdf |
| C8 | nika:pipeline | orchestrator | always on |
| C9 | CAS compression | zstd | media-compression |
| C10 | nika media tools CLI | -- | -- |
| C11 | Tests + E2E + fuzz | -- | ~142 tests |
| C12 | Bump to v0.34.0 | -- | -- |

### PR3b Architecture Additions
- PipelineCapable trait for in-memory chaining
- MultiBinary variant in MediaOpResult
- decode_semaphore (max 4 concurrent decodes)
- extract_pdf_safe() in safety.rs (dedicated thread, 4MB stack)
- Rig agent loop media tool registration
- nika media tools CLI subcommand

### PR3b Dependencies
- ocrs 0.12, rten 0.24 (OCR)
- charts-rs 0.3 (chart generation)
- c2pa 0.78 (content provenance)
- image_hasher 3.1 (perceptual hashing)
- pdf-extract 0.10 + lopdf 0.39 (PDF)
- zstd 0.13 (CAS compression)

### PR3b Success Criteria
- 12 additional tools implemented
- nika:pipeline chains tools in-memory
- nika media tools CLI lists available tools
- OCR on real images produces text
- QR validation scores artistic QR codes
- C2PA provenance signs artifacts
- PDF text extraction works
- ~142 new tests, all passing
- Binary size delta tracked

---

## Timeline

| Phase | Duration | Status |
|-------|----------|--------|
| PR3a implementation | Done | Merged (18 commits) |
| PR3a audit (42 agents) | Done | 21 bugs fixed |
| PR3a live tests | Done | 28/28 pass, 3 providers |
| H1: Deep media E2E | 2h | Next |
| H2: Cross-tool pipelines | 1h | Queued |
| H3: Error recovery | 1h | Queued |
| H4: Multi-provider media | 1h | Queued |
| PR3b implementation | 5h | Queued |
| PR3b audit | 2h | Queued |
| PR3b live tests | 1h | Queued |
