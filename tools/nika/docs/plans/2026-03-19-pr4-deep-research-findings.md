# PR4 Vision Master Plan -- Deep Research Findings

> **Date:** 2026-03-19
> **Researcher:** Claude Opus 4.6 (14 parallel Perplexity queries, 28 sources analyzed)
> **Purpose:** Enrich the PR4 Vision master plan with latest intelligence

---

## 1. Agentic AI Multimodal Workflows (2025-2026)

### Key Finding: No framework combines CAS + Vision + Provenance

**How production AI agents handle vision today:**

| Framework | Vision Implementation | CAS Support | Provenance |
|-----------|----------------------|-------------|------------|
| **LangChain/LangGraph** | `HumanMessage(content=[{"type":"image_url","image_url":{"url":"data:image/jpeg;base64,..."}}])` via ChatOpenAI/ChatGoogleGenerativeAI. Also integrates Google Vision API via Eden AI. | None | None |
| **AutoGen** | `MultimediaAgent` shares image URLs or base64 data; agents invoke vision tools and pass text summaries to collaborators | None | None |
| **CrewAI** | Limited -- relies on custom tools wrapping GPT-4o/Claude Vision. No built-in image passing in core workflows | None | None |
| **Dify** | Visual workflow builder with multimodal nodes accepting uploads/URLs/base64, routed to vision LLMs | None | None |
| **Google ADK** | Native multimodal (text, image, video, audio) via workflow agents (sequential/parallel/loop). Dev UI for debugging | None | None |

**The gap Nika fills:** No framework integrates CAS-backed media storage with vision capabilities and C2PA provenance. Google ADK comes closest with native multimodal but lacks CAS and provenance. This is Nika's unique moat.

**Agentic patterns emerging in 2026:**
- Gartner predicts 40% of enterprise apps will have AI agents by end of 2026
- The $199B agentic AI market favors proactive, autonomous, multi-step workflows
- Anthropic's "Agent Skills" standard is gaining traction in B2B
- Menlo Security highlights the need for governance of AI agent actions (security angle)

**Source URLs:**
- https://intuitionlabs.ai/articles/ai-agents-b2b-productivity-anthropic
- https://www.jeffbullas.com/agentic-ai-revolution/
- https://airbyte.com/agentic-data/best-ai-agent-frameworks-2026
- https://47billion.com/blog/ai-agents-in-production-frameworks-protocols-and-what-actually-works-in-2026/

**Implication for PR4:** Nika's `import -> CAS -> content: -> infer:` pipeline is genuinely novel. No competitor has this. The vision PR should emphasize this unique chain in documentation and marketing.

---

## 2. EU AI Act Article 50 Content Labeling Requirements

### Key Finding: Deadline is August 2, 2026 -- C2PA is the de facto standard

**What Article 50 requires:**
1. **Machine-readable marking** of AI-generated/manipulated content (images, audio, video, text)
2. Solutions must be "effective, interoperable, robust, reliable, and technically feasible"
3. **Deepfake disclosure** -- deployers must reveal AI generation/manipulation
4. Clear disclosure at first interaction for chatbots/emotion recognition systems
5. Five distinct transparency obligations covering providers and deployers

**Technical standard mandated:** None specifically. The Act says "generally acknowledged state of the art, as may be reflected in relevant technical standards." However:
- **C2PA is the de facto standard** -- 500+ member companies, ISO standardization underway
- The EU AI Office facilitates voluntary **Codes of Practice** (first draft published Feb 2026)
- The Code of Practice specifically addresses marking/labeling for generative AI

**Compliance deadline:** **August 2, 2026** (24 months after entry into force on August 1, 2024)

**Penalties:** Up to 35M EUR or 7% of global annual turnover for prohibited/high-risk violations; lower fines expected for transparency non-compliance (limited-risk category)

**What competitors use:**
| Company | Approach |
|---------|----------|
| **Adobe** | C2PA Content Credentials embedded in Firefly, Photoshop, Lightroom, Premiere Pro. Enterprise API for signing. Co-founder of CAI and C2PA. |
| **Google** | SynthID (invisible watermarks in pixels/tokens) + C2PA metadata. Pixel 10 achieved top C2PA Conformance tier. Open-sourced via Responsible GenAI Toolkit. |
| **Microsoft** | C2PA co-founder. Integrates Content Integrity tools across ecosystem. |

**SynthID vs C2PA comparison:**
| Aspect | SynthID | C2PA |
|--------|---------|------|
| Method | Invisible signals in pixels/tokens | Metadata-based digital signatures |
| Resilience | Survives compression/cropping | Vulnerable if metadata stripped |
| Provenance | Detects AI origin only | Full chain-of-custody |
| Compatibility | Complements C2PA | Complements SynthID |

**Source URLs:**
- https://www.euaiact.com/key-issue/5
- https://digital-strategy.ec.europa.eu/en/faqs/guidelines-and-code-practice-transparent-ai-systems
- https://www.wilmerhale.com/en/insights/blogs/wilmerhale-privacy-and-cybersecurity-law/20240528-limited-risk-ai-a-deep-dive-into-article-50-of-the-european-unions-ai-act
- https://www.kirkland.com/publications/kirkland-alert/2026/02/illuminating-ai-the-eus-first-draft-code-of-practice-on-transparency-for-ai
- https://digital-strategy.ec.europa.eu/en/policies/code-practice-ai-generated-content
- https://contentauthenticity.org/how-it-works

**Implication for PR4:** Nika already has `nika:provenance` (C2PA signing). Adding `nika:verify` (C2PA verification) in PR4 Phase 3 makes Nika **compliance-ready for August 2026**. This is a massive selling point. Consider adding:
- A `compliance: eu-ai-act` flag in workflow metadata
- Automatic C2PA signing for all AI-generated outputs (opt-in)
- A `nika doctor --compliance` check that verifies C2PA integration

---

## 3. rig-core Latest State

### Key Finding: v0.32.0 is the latest on crates.io (50 versions total)

**Current state:**
- **Latest published version:** 0.32.0 on crates.io (as of March 2026)
- **Nika's Cargo.toml:** `rig-core = { version = "0.32", features = ["rmcp"] }` -- this is current
- **The PR4 plan mentions 0.33.0** as "published 2026-03-17" but crates.io shows 0.32.0 as latest with 50 versions

**Multimodal support confirmed:**
- `image_generation` module (behind `image` feature flag)
- `ImageGenerationModel` trait for compatible providers
- `UserContent::image_base64()`, `image_url()` exist in the message types
- Supports: Anthropic, OpenAI, Gemini, Groq, Mistral, Ollama, Perplexity, xAI, Cohere

**Provider ecosystem:**
- Cloud providers via companion crates (rig-mongodb, rig-qdrant, etc.)
- Streaming, evals (experimental), extractors, pipelines
- RMCP integration (already used by Nika)

**Source URLs:**
- https://crates.io/crates/rig-core
- https://docs.rs/rig-core

**Implication for PR4:**
- The PR4 plan should reference `rig-core 0.32` (not 0.33) unless a newer version is published before implementation
- The `UserContent::image_base64()` API is confirmed available -- the vision message builder design in the plan is correct
- No breaking changes to worry about from 0.32

---

## 4. YAML Workflow DSL Design Patterns for Binary Content

### Key Finding: All major YAML DSLs use references, never inline binary

**Universal pattern across all platforms:**

| Platform | Binary Reference Mechanism | Example |
|----------|---------------------------|---------|
| **GitHub Actions** | Artifacts (upload/download between jobs), filesystem paths | `actions/upload-artifact: path: ./images/` |
| **Argo Workflows** | Artifacts (S3/HTTP/PVC), declared as inputs/outputs | `inputs: artifacts: - name: video path: /data/image.jpg` |
| **Tekton** | PersistentVolumeClaims, Workspaces | `workspace: name: audio-data` |
| **Temporal** | Data converters, external storage URLs | `data: {url: "gs://bucket/file.mp4"}` |
| **Dagger** | Files, HTTP sources, Caches | `ctr.withMountedFile('/input.mp4', src)` |

**Best practices identified:**
1. **Always use external references** (URLs, artifact names, paths, hashes) -- never embed base64 in YAML
2. **Leverage artifact stores** for persistence across runs (S3, MinIO, PVCs)
3. **Avoid base64 encoding** in YAML except for very small data (<1KB)
4. **Normalize paths** with absolute paths or volumes
5. **Pin versions/SHAs** for security on downloaded artifacts
6. **Producer-Consumer chains** -- one step produces binary, next consumes via reference

**Design patterns:**
- **Fan-Out/Fan-In** -- parallel multimodal processing branches, aggregated via shared storage
- **Sidecar/Init Processing** -- dedicated containers for binary transforms
- **Event-Driven References** -- trigger on binary uploads, reference via metadata URLs
- **Opaque Blob Handling** -- treat binaries as paths, avoid YAML parsing

**Emerging DSL: duckflux** -- a new minimal YAML workflow DSL (v0.2, Go CLI) with loops, conditionals, parallelism, and events. Worth watching as a design reference.

**Source URLs:**
- https://arxiv.org/html/2601.14455v2
- https://dev.to/ggondim/duckflux-a-declarative-workflow-dsl-born-from-the-multi-agent-orchestration-gap-4n28

**Implication for PR4:** Nika's `content:` syntax with CAS hash references (`{{with.photo.media[0].hash}}`) follows exactly the right pattern. The CAS hash IS the reference. This is more elegant than filesystem paths because:
- Content-addressed = immutable = reproducible
- Hash IS the dedup key
- No path resolution issues
- Works identically in local and distributed contexts

---

## 5. Media Pipeline Optimizations

### 5a. Compression Benchmarks: zstd wins for CAS

**Detailed benchmarks (aggregated from multiple sources):**

| Algorithm | Compression Speed | Decompression Speed | Ratio (text/JSON) | Ratio (images) | Best For |
|-----------|------------------|--------------------|--------------------|-----------------|----------|
| **zstd lvl 3** | ~1 GB/s | ~1 GB/s | 3.5x | ~1x (passthrough) | **CAS default** -- best speed/ratio balance |
| **zstd lvl 9** | ~200 MB/s | ~1 GB/s | 4.0x | ~1x | Cold storage |
| **lz4** | ~3.5 GB/s | ~3.5 GB/s | 2.0x | ~1x | Hot caches, streaming |
| **brotli lvl 6** | ~100 MB/s | ~400 MB/s | 4.3x | ~1x | Static web assets |
| **brotli lvl 9** | ~10 MB/s | ~400 MB/s | 5.0x | ~1x | Archive only |

**Key findings for CAS:**
- **zstd level 3 is optimal** for CAS: 3.5x ratio on JSON/text/SVG with ~1 GB/s throughput
- Images/audio/video should **skip compression** (already entropy-coded, <2% gain)
- SVG (XML-like) compresses excellently with zstd: 56-80% ratio
- JSON: zstd default achieves 48-80% compression depending on redundancy
- **Multi-threading boosts zstd** at higher levels (not available for brotli)
- Decompression is fastest for zstd/lz4 (~0.01s for small files, 40% faster than brotli)

**Concrete benchmark data (307KB web content):**
- zstd: 0.01s compress, 158KB output (48.5% reduction)
- brotli: 0.82s compress, 121KB output (60.5% reduction) -- 82x slower for 12% better ratio
- At 349MB scale: zstd 0.83s vs brotli 577s -- 700x speed difference

**Source URLs:**
- https://peazip.github.io/fast-compression-benchmark-brotli-zstandard.html
- https://ntorga.com/gzip-bzip2-xz-zstd-7z-brotli-or-lz4/
- https://speedvitals.com/blog/zstd-vs-brotli-vs-gzip/
- https://manishrjain.com/compression-algo-moving-data
- https://dev.to/konstantinas_mamonas/which-compression-saves-the-most-storage-gzip-snappy-lz4-zstd-1898

**Implication for PR4:** The plan's choice of zstd level 3 with MIME-based skip for images is perfectly validated. Recommended refinements:
- Use zstd magic bytes (0x28B52FFD) for transparent decompression -- confirmed standard approach
- Consider zstd dictionary training on common JSON schemas for 10-30% extra compression
- Skip compression for: image/*, audio/*, video/*, application/octet-stream
- Compress: text/*, application/json, application/xml, image/svg+xml, application/yaml

### 5b. Lazy Evaluation Patterns in Rust

**Key patterns identified:**
- Rust's `Iterator` lazy chains (`pixels.iter().filter(...).map(...)`) are the foundation
- Java's Stream Gatherers (2025) show intermediate filtering without breaking flow -- Rust already has this via `Iterator`
- **Tokio async streams** for I/O-bound media processing
- Message queue patterns (RabbitMQ/Kafka style) decouple slow video tasks from user responses

**Implication for PR4:** Consider lazy CAS reads -- don't decode images until `content:` actually needs them for vision. The `MediaRef` type could hold a CAS hash and only resolve to bytes when `.data()` is called.

### 5c. Image Quality Assessment Crates

**butteraugli crate:**
- **Latest version:** v0.7.0 (pure Rust implementation from libjxl)
- Psychovisual difference scoring (lower = more similar)
- `precompute` module for efficient repeated comparisons
- Related: `butteraugli-cli` v0.1.3 (Feb 2026), `jxl-encoder-cli` uses it for JXL distance

**dssim crate:**
- **Latest version:** v3.4.0
- Multi-scale DSSIM metric (derived from SSIM)
- Supports RGBA/RGB u8 slices, LAB color space conversion
- `dssim-core` provides abstract interface for custom pixel types + SIMD
- Well-maintained, actively used

**zensim crate (NEW -- worth considering):**
- Multi-scale SSIM with 4-scale pyramid, O(1) box blur with SIMD, 228 trained weights
- Bidirectional mapping to DSSIM/SSIM2/butteraugli/libjpeg quality/zenjpeg
- Diff image generation and comparison montages
- **Most comprehensive** Rust IQA crate as of March 2026

**Source URLs:**
- https://crates.io/crates/butteraugli
- https://crates.io/crates/dssim
- https://lib.rs/crates/zensim

**Implication for PR4:** Consider using `zensim` instead of (or alongside) `butteraugli` for Task 3.1. It provides SSIM, maps to butteraugli scores, and generates visual diffs. Alternatively, use `butteraugli` v0.7.0 for the metric and `dssim` v3.4.0 for SSIM. The plan's `butteraugli 0.1` version reference should be updated to `0.7.0`.

---

## 6. JPEG XL Browser Support Status

### Key Finding: Chrome re-added JXL in January 2026, Interop 2026 accepted

**Current browser support (March 2026):**

| Browser | Status | Notes |
|---------|--------|-------|
| **Chrome/Chromium** | Behind flag in Canary, not in stable yet | Re-added Jan 2026 via jxl-rs (Rust decoder). `chrome://flags/#enable-jxl-image-format` |
| **Safari** | Partial support | Lacks animation and progressive decoding |
| **Firefox** | No support confirmed | Experimented in 2021, dropped after Chrome's 2022 removal |
| **Edge/Brave/Opera** | Will follow Chrome | Chromium-based |
| **Windows 11 24H2** | Supported | Since March 2025 |

**Key developments:**
- Google reversed its 2022 "obsolete" declaration for JXL
- Chromium uses **jxl-rs** (memory-safe Rust decoder) -- validates Nika's choice of `jxl-oxide`
- **JXL accepted into Interop 2026** -- cross-browser interoperability testing program
- Features being tested: high bit-depth (8/10/12/16), wide gamut (P3/Rec.2020), progressive decode

**jxl-oxide Rust crate:**
- Forms the basis of Chromium's decoder
- Supports: HDR, lossless recompression (20% smaller JPEGs), progressive decoding
- Superior compression vs WebP/AVIF in many benchmarks
- `jxl-encoder-cli` v0.1.3 (Feb 2026) uses butteraugli for quality targeting

**Source URLs:**
- https://coywolf.com/news/web-development/jpeg-xl-jxl-is-coming-back-to-chrome/
- https://www.theregister.com/2026/01/14/google_rekindles_relationship_with_jilted/
- https://www.devclass.com/development/2025/11/24/googles-chromium-team-decides-it-will-add-jpeg-xl-support-reverses-obsolete-declaration/
- https://caniuse.com/jpegxl

**Implication for PR4:** JXL support in `nika:convert` is well-timed. The plan's `jxl-oxide 0.11` reference should be verified against crates.io for latest version. Chrome's adoption of the same Rust decoder validates the approach. JXL is on a clear trajectory to become the successor to JPEG.

---

## 7. QR Code Scanning: rxing Crate

### Key Finding: rxing is the most comprehensive Rust barcode library

**rxing crate:**
- Rust port of ZXing (industry standard barcode library)
- Integrated ZXing-Cpp QRCode library for "large enhancements" in detection/decoding
- Supports: QR codes + multiple barcode formats via `MultiFormatReader`
- Bindings: Node.js (`@rxing/rxing`), WASM (`rxing-wasm`), Python (`pyrxing`)
- Input: `PlanarYUVLuminanceSource` (camera), `RGBLuminanceSource` (files), crop support
- Decoding hints for speed/accuracy optimization

**Source URLs:**
- https://docs.rs/rxing
- https://crates.io/crates/rxing

**Implication for PR4:** The plan's choice of rxing is validated. The ZXing-Cpp integration is particularly relevant for QR Code AI use case. Consider using `default-features = false, features = ["qrcode"]` to minimize binary size.

---

## 8. Differentiating Features Synthesis

### What no competitor has (Nika's unique chain):

```
import file -> CAS store (blake3 hash) -> C2PA sign -> vision analyze -> quality score -> verify provenance
```

### Feature differentiation matrix (updated):

| Feature | Nika (PR4) | LangChain | CrewAI | AutoGen | Dify | Google ADK |
|---------|:----------:|:---------:|:------:|:-------:|:----:|:----------:|
| CAS media storage | YES | no | no | no | no | no |
| C2PA provenance sign | YES | no | no | no | no | no |
| C2PA provenance verify | **PR4** | no | no | no | no | no |
| Pipeline composition | YES | partial | no | no | partial | yes |
| Vision in YAML workflows | **PR4** | yes* | no | yes* | yes | yes |
| CAS-to-Vision bridge | **PR4** | no | no | no | no | no |
| Image quality scoring | **PR4** | no | no | no | no | no |
| QR validation | **PR4** | no | no | no | no | no |
| CAS compression | **PR4** | no | no | no | no | no |
| EU AI Act compliance | **PR4** | no | no | no | no | partial** |
| JPEG XL support | **PR4** | no | no | no | no | no |

*via raw API calls, not declarative YAML
**Google uses SynthID + C2PA but not in workflow DSL

### Production teams' actual needs (from Deloitte/ByteBytego research):
1. **Trustworthy scaling** -- robust data infrastructure, not just generation
2. **Provenance tracking** -- especially for EU compliance (August 2026 deadline)
3. **Quality assurance** -- automated IQA in pipelines, not manual review
4. **Hyper-personalization** -- dozens of variants with consistent quality (40% revenue uplift)
5. **Content differentiation** -- metadata, discovery, governance over volume
6. **Cost efficiency** -- CAS deduplication saves storage on repeated variants

---

## 9. Recommended Plan Adjustments

Based on this research, I recommend these changes to the PR4 plan:

### Version Updates
| Dependency | Plan Says | Research Shows | Action |
|------------|-----------|----------------|--------|
| rig-core | 0.32 (0.33 mentioned) | 0.32.0 is latest on crates.io | Keep 0.32, monitor for 0.33 |
| butteraugli | 0.1 | 0.7.0 | **Update to 0.7.0** |
| jxl-oxide | 0.11 | Verify on crates.io | Verify latest version |
| rxing | 0.8 | Latest on crates.io (ZXing-Cpp integrated) | Verify latest version |

### New Considerations
1. **zensim crate** -- Consider for Task 3.1 (IQA) as a more comprehensive alternative to butteraugli alone
2. **EU AI Act compliance mode** -- Add `compliance:` workflow metadata and `nika doctor --compliance` (Phase 3 bonus)
3. **Lazy CAS reads** -- Defer image decoding until vision actually needs bytes
4. **zstd dictionary training** -- For common JSON schemas in CAS (10-30% extra compression)
5. **dssim v3.4.0** -- Add SSIM scoring alongside butteraugli for `nika:quality`

### Marketing Angles from Research
- "The only AI workflow engine with CAS-backed multimodal vision" (no competitor has this)
- "EU AI Act ready" (C2PA sign + verify, August 2026 deadline)
- "Content-addressed, not path-addressed" (more robust than all YAML DSL competitors)
- "Sign, process, verify -- full provenance chain in a single pipeline"

---

## Methodology

- **Tools used:** Perplexity API (sonar-pro model), 14 parallel queries
- **Sources analyzed:** 28 unique URLs across legal, technical, and industry sources
- **Time period covered:** 2024-2026, with focus on Q1 2026
- **Cost:** ~$0.20 in API calls
- **Confidence level:** HIGH for EU AI Act, compression benchmarks, JPEG XL status. MEDIUM for rig-core (crates.io confirms 0.32 but GitHub changelog not scraped). HIGH for competitive analysis.
