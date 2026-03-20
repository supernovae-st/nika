# 17 -- Smart Router Pattern & Deep Multi-Modal Architecture

> **Date**: 2026-03-15
> **Status**: BRAINSTORM (validated decisions from interactive session)
> **Scope**: The Smart Router pattern, complete multi-modal tool inventory, native RAG, translation, and audio architecture for Nika v0.30
> **Dependencies**: Doc 12 (Vegapunk Naming), Doc 13 (Multi-Modal Worker Architectures), Doc 15 (Ecosystem Coherence), Doc 16 (Multi-Modal Builtin Research)
> **Research Sources**: Perplexity (8 queries), Firecrawl (2 scrapes: ComfyUI MCP servers), crates.io analysis

---

## Validated Decisions (2026-03-15 Deep Brainstorm Session)

| Decision | Status | Details |
|----------|--------|---------|
| **Smart Router Pattern** | VALIDATED | Unified dispatch: builtin > MCP > LLM fallback > error |
| **`nika:imagine`** (name) | VALIDATED | One Piece: Gear 5 / Sun God Nika = imagination becomes reality |
| **`nika:imagine`** (architecture) | VALIDATED | MCP-only, Smart Router across ComfyUI/Replicate/FAL |
| **`nika:translate`** builtin | VALIDATED | candle + NLLB-200 (600MB, 200 langues, offline) |
| **`nika:search`** native RAG | VALIDATED | nika:embed + instant-distance HNSW, in-memory, ephemeral |
| **`nika:index`** companion tool | VALIDATED | Manual + auto-index into RunContext vector index |
| **Audio input in `infer:`** | VALIDATED (future) | `audio:` field when mistral.rs supports Gemma 3n audio |
| **Tool naming** | VALIDATED | `nika:` prefix, neutral/descriptive (NOT One Piece refs for tools) |
| **`comfyui` MCP alias** | VALIDATED | Add to MCP_ALIASES (joenorton/comfyui-mcp-server, 219 stars) |
| **5 new MCP aliases** | PROPOSED | comfyui, replicate, fal, deepl, elevenlabs |
| **10 builtin tools total** | PROPOSED | vision, embed, transcribe, speak, ocr, translate, imagine, search, index + media fields |

---

## 1. The Smart Router Pattern

### 1.1 Core Concept

The Smart Router is the **fundamental architectural pattern** for all multi-modal tools in Nika v0.30. Every tool that has multiple possible backends follows the same dispatch logic:

```
Tool call (e.g., nika:translate)
    |
    v
+-- Has builtin engine? --+
|  YES                     |  NO
|  +-- Feature enabled? -+ |  |
|  | YES -> Use builtin  | |  |
|  | NO  -> Check MCP    | |  |
|  +---------------------+ |  |
+---------------------------+  |
         |                     |
         v                     v
+-- MCP servers available? ------+
|  YES -> Route to best match     |
|  NO  -> LLM fallback possible?  |
|         YES -> infer: prompt     |
|         NO  -> Error + suggestion |
+---------------------------------+
```

### 1.2 Why This Pattern

Research findings that led to this design:

1. **No existing MCP server does multi-backend routing** (confirmed via Perplexity + Firecrawl research, March 2026). Pixelle-MCP offers "mode selection" (local OR cloud) but not dynamic routing.
2. **ComfyUI MCP ecosystem is mature** for image gen (219 stars, 13+ tools) but audio/STT/TTS has ZERO MCP servers.
3. **Degradation gracieuse** is critical for developer experience: works offline in dev, scales to cloud in production.

### 1.3 Dispatch Priority Chain

```
PRIORITY (configurable per-tool in workflow YAML):

1. Builtin engine (if feature flag enabled)
   - Free, offline, no API key required
   - Lower quality than cloud alternatives
   - Immediate latency (no network)

2. MCP server (if connected)
   - Check RunContext.mcp_servers for matching capability
   - Route to first match in preference order
   - Configurable: prefer: [comfyui, replicate, fal]

3. LLM fallback (if applicable)
   - Some tools can fall back to infer: with a prompt
   - e.g., translate -> "Translate to Japanese: <text>"
   - Quality depends on LLM capability

4. Error with suggestion
   - Clear message: "No backend available"
   - Actionable: "Run: nika mcp add comfyui"
```

### 1.4 YAML Configuration

```yaml
# Workflow-level tool configuration
tools:
  nika:translate:
    prefer: [builtin, deepl, google-translate]
    fallback: true  # allow LLM fallback

  nika:imagine:
    prefer: [comfyui, replicate, fal]
    fallback: false  # no LLM fallback for image gen

  nika:speak:
    prefer: [builtin, elevenlabs]
    fallback: false  # no LLM fallback for TTS
```

### 1.5 Rust Implementation Sketch

```rust
/// Smart Router trait for tools with multiple backends
trait SmartRouter {
    /// Backend preference order
    fn backends(&self) -> Vec<Backend>;

    /// Check if a backend is available in the current RunContext
    fn is_available(&self, backend: &Backend, ctx: &RunContext) -> bool;

    /// Execute on the best available backend
    async fn dispatch(&self, input: ToolInput, ctx: &RunContext) -> Result<ToolOutput> {
        for backend in self.backends() {
            if self.is_available(&backend, ctx) {
                return self.execute_on(backend, input, ctx).await;
            }
        }
        // LLM fallback if supported
        if self.supports_llm_fallback() {
            return self.llm_fallback(input, ctx).await;
        }
        Err(NikaError::NoBackendAvailable {
            tool: self.name(),
            suggestion: self.install_suggestion(),
        })
    }
}

enum Backend {
    Builtin,                    // Feature-flagged native engine
    Mcp { server: String },     // MCP server by name
    LlmFallback,               // infer: with generated prompt
}
```

---

## 2. `nika:imagine` -- Image Generation Smart Router

### 2.1 Why MCP-Only (No Builtin)

| Factor | Assessment |
|--------|------------|
| **Memory** | SDXL requires 12GB+ VRAM. Does NOT fit alongside LLM on 16GB Mac |
| **Quality** | Cloud APIs (Midjourney-level) far exceed local quality |
| **Ecosystem** | ComfyUI MCP is mature (219 stars, 13+ tools, active March 2026) |
| **Latency** | Image gen is inherently slow (seconds). Network latency is negligible |
| **Cost** | ComfyUI on local GPU = free. Cloud = pay-per-image but high quality |

### 2.2 ComfyUI MCP Landscape (March 2026)

| Server | Stars | Language | Tools | Special Features |
|--------|-------|----------|-------|------------------|
| **joenorton/comfyui-mcp-server** | 219 | Python | 13+ | generate_image, generate_song (!), regenerate, view_image, publish_asset, PARAM_* custom workflows |
| **alecc08/comfyui-mcp** | 14 | TypeScript | 7 | text_to_image, img2img, upscale, resize, remove_background, Qwen Image models |
| **Pixelle-MCP** | -- | Python | -- | Local ComfyUI or RunningHub cloud (mode selection) |
| **Aether MCP** | -- | -- | -- | Flux/SDXL + video generation |

### 2.3 Architecture

```
nika:imagine (builtin tool, Tier 3 MCP router)
    |
    +-- Agent calls: "Generate a cyberpunk QR code hero image, 1024x1024"
    |
    v
+--- Orchestrator Router (capability match) ---+
|                                        |
|  1. Check RunContext.mcp_servers       |
|     +-- comfyui connected? --YES-->  invoke: comfyui.generate_image  |
|                                        |
|  2. Check RunContext.mcp_servers       |
|     +-- replicate connected? -YES->  invoke: replicate.run           |
|                                        |
|  3. Check RunContext.mcp_servers       |
|     +-- fal connected? ------YES-->  invoke: fal.generate            |
|                                        |
|  4. No backend available               |
|     +-- Error: "No image gen backend.  |
|         Run: nika mcp add comfyui"     |
|                                        |
+----------------------------------------+
```

### 2.4 Tool API

```json
{
  "name": "nika:imagine",
  "input": {
    "prompt": "cyberpunk QR code hero image with neon lighting",
    "width": 1024,
    "height": 1024,
    "style": "cyberpunk",
    "quality": "high",
    "negative_prompt": "blurry, low quality"
  },
  "output": {
    "image_path": "/tmp/nika/artifacts/imagine_abc123.png",
    "metadata": {
      "backend": "comfyui",
      "model": "sdxl-turbo",
      "seed": 42,
      "generation_time_ms": 3200
    }
  }
}
```

### 2.5 MCP Alias (proposed)

```rust
("comfyui", McpAlias {
    command: "uvx",
    args: &["comfyui-mcp-server"],
    env: &[("COMFYUI_URL", "http://localhost:8188")],
    description: "ComfyUI image/audio generation (13+ tools)",
})
```

### 2.6 Name Origin

**nika:imagine** -- From One Piece, Gear 5 / Sun God Nika:
> "His body gains the ability to fight however he imagines... His power is limited only by his imagination."

The tool turns imagination (prompts) into reality (images). The name is:
- Descriptive (everyone understands "imagine")
- Evocative (One Piece reference for insiders)
- Distinct from `nika:vision` (input vs output)

---

## 3. `nika:translate` -- Native Translation with Smart Router

### 3.1 Why Builtin (Candle + NLLB-200)

Research confirmed: **Candle (HuggingFace, 19.6k stars, Rust native) directly supports NLLB model inference**.

| Factor | Assessment |
|--------|------------|
| **Engine** | candle + candle-transformers (Rust native, zero Python deps) |
| **Model** | facebook/nllb-200-distilled-600M (600MB, 200 languages) |
| **Upgrade path** | facebook/nllb-200-1.3B (better quality, ~1.3GB) |
| **Memory** | 600MB for distilled model. Fits alongside everything else |
| **Quality** | Good for drafts. DeepL/Google better for final quality |
| **Offline** | 100% offline, zero API cost, zero rate limits |
| **Speed** | Fast on CPU. Faster on Metal/CUDA |
| **Limitations** | Max 512 tokens per request. Chunk + reassemble for longer texts |

### 3.2 SuperNovae Synergy

This is a killer feature for SuperNovae because:
- **NovaNet** = localization engine across 200+ locales
- **QR Code AI** = multilingual landing pages
- **Offline translation** = no API cost during development
- **Candle** = same Rust ML ecosystem as mistral.rs (shares candle backend)

### 3.3 Architecture (Smart Router)

```
nika:translate("Scannez ce code QR", source: "fr", target: "ja")
    |
    +-- 1. NLLB feature enabled? --YES--> candle NLLB inference (free, offline)
    |                               |
    |   NO                          v
    |                          "QRコードをスキャンしてメニューにアクセスしてください"
    |
    +-- 2. DeepL MCP connected? --YES--> invoke: deepl.translate (cloud, high quality)
    |
    +-- 3. Google Translate MCP? --YES--> invoke: google-translate.translate
    |
    +-- 4. LLM fallback ----------YES--> infer: "Translate to Japanese: <text>"
    |                                     (always available, quality varies)
    |
    +-- 5. Never reaches here (LLM fallback always catches)
```

### 3.4 Tool API

```json
{
  "name": "nika:translate",
  "input": {
    "text": "Scannez ce code QR pour acceder au menu",
    "source": "fr",
    "target": "ja",
    "model": "nllb-600m",
    "max_length": 512
  },
  "output": {
    "translated": "このQRコードをスキャンしてメニューにアクセスしてください",
    "backend": "builtin:nllb-600m",
    "confidence": 0.89
  }
}
```

### 3.5 Workflow Example

```yaml
# Translate content across locales with zero API cost
tasks:
  - id: context
    invoke: novanet_context
    params: { focus_key: "qr-code", locale: "fr-FR", mode: "page" }

  - id: generate
    infer: "Generate landing page content in French"
    context: $context
    use.ctx: fr_content

  - id: translate_all
    for_each: ["ja", "de", "es", "pt", "ko", "zh"]
    as: target_locale
    concurrency: 3
    agent: "Translate the French content to {{use.target_locale}}"
    tools: [nika:translate]
    context: $fr_content
    # Agent uses nika:translate internally, 200 languages, zero cost
```

### 3.6 Chunking Strategy (> 512 tokens)

NLLB is limited to 512 tokens per request. For longer texts:

```
Input text (2000 tokens)
    |
    v
+-- Sentence splitter (unicode-segmentation crate) --+
|                                                      |
|   Chunk 1 (480 tokens)  -> NLLB translate           |
|   Chunk 2 (510 tokens)  -> NLLB translate           |
|   Chunk 3 (490 tokens)  -> NLLB translate           |
|   Chunk 4 (520 tokens)  -> split further, translate  |
|                                                      |
+-- Reassemble in order --------------------------------+
    |
    v
Output text (translated, 2000 tokens)
```

Feature flag: `translate` (default OFF, requires model download: `nika model pull nllb-200-600m`)

---

## 4. `nika:search` + `nika:index` -- Native RAG in Runtime

### 4.1 Core Concept

Native RAG (Retrieval-Augmented Generation) WITHIN a single workflow run, with zero external infrastructure. No Pinecone. No Weaviate. No Qdrant. Just Nika.

### 4.2 Components

```
+-- nika:embed (mistral.rs, EmbeddingGemma) --> vectors (Tier 1, already available)
|
+-- instant-distance (HNSW index) -----------> in-memory ANN search
|   crates.io, pure Rust, zero deps
|   Simple API: build index, search neighbors
|
+-- RunContext.vector_index (DashMap) --------> storage within the run
    Ephemeral: lives and dies with the workflow run
```

### 4.3 Rust HNSW Crate Analysis

| Crate | Stars | Type | Persistence | Best For |
|-------|-------|------|-------------|----------|
| **instant-distance** | crates.io | Pure Rust HNSW | In-memory only | Ephemeral indexes (Nika) |
| **hnswlib-rs** | GitHub | Rust binding | In-memory | High-perf, more features |
| **hnsw_rs** | GitHub | Pure Rust | In-memory | Production-ready |
| **hannoy** | GitHub | LMDB-backed | Disk (LMDB) | Large-scale persistent |
| **usearch** | GitHub | C++ + Rust bindings | Both | GPU-optimized |

**Best fit for Nika: `instant-distance`** -- Pure Rust, zero deps, in-memory, simple API. Perfect for ephemeral indexes in RunContext.

### 4.4 Architecture

```
WORKFLOW RUN
    |
    +-- Task 1: fetch article_1 ----> auto-embed ----> add to HNSW index
    +-- Task 2: fetch article_2 ----> auto-embed ----> add to HNSW index
    +-- Task 3: fetch article_3 ----> auto-embed ----> add to HNSW index
    |                                                       |
    |   RunContext.vector_index: {                           |
    |     "article_1": [0.12, -0.34, 0.56, ...],            |
    |     "article_2": [0.89, -0.12, 0.34, ...],            |
    |     "article_3": [0.45, 0.67, -0.23, ...],            |
    |   }                                                    |
    |                                                       |
    +-- Task 4: nika:search("QR code accessibility")        |
    |   |                                                   |
    |   +-- embed query -----> HNSW nearest neighbors       |
    |   |                                                   |
    |   +-- top-3 results:                                  |
    |       [article_2 (score: 0.94),                       |
    |        article_1 (score: 0.88),                       |
    |        article_3 (score: 0.72)]                       |
    |                                                       |
    +-- Task 5: infer: "Synthesize these findings..."       |
        context: search results from task 4
```

### 4.5 Tool APIs

#### `nika:search`

```json
{
  "name": "nika:search",
  "input": {
    "query": "QR code security best practices",
    "top_k": 5,
    "namespace": "research",
    "threshold": 0.7,
    "include_metadata": true
  },
  "output": {
    "results": [
      {
        "text": "QR codes should implement...",
        "score": 0.94,
        "source": "fetch_arxiv_1",
        "metadata": { "url": "https://...", "title": "..." }
      }
    ],
    "total_indexed": 15,
    "search_time_ms": 2
  }
}
```

#### `nika:index`

```json
{
  "name": "nika:index",
  "input": {
    "text": "Content to index for later retrieval...",
    "namespace": "research",
    "metadata": {
      "source": "arxiv",
      "date": "2026-03",
      "url": "https://arxiv.org/..."
    }
  },
  "output": {
    "indexed": true,
    "vector_id": "idx_abc123",
    "namespace_size": 16
  }
}
```

### 4.6 Auto-Index Mode

```yaml
# In workflow YAML
tasks:
  - id: research
    agent: "Research QR code accessibility standards"
    tools: [nika:fetch, nika:search, nika:index]
    auto_index: true  # Every fetch/invoke result auto-embedded + indexed

    # The agent lifecycle:
    # 1. fetch article_1 -> auto-indexed
    # 2. fetch article_2 -> auto-indexed
    # 3. ...10 articles fetched and indexed...
    # 4. nika:search("accessibility guidelines")
    #    -> returns top-5 from its OWN fetched data
    # 5. infer: synthesize with relevant chunks
    # = RAG pipeline WITHIN a single agent turn, zero infra
```

### 4.7 Killer Use Case: Agent-Driven Research with Native RAG

```yaml
schema: nika/workflow@0.12

tasks:
  - id: deep_research
    agent: "Research and synthesize QR code accessibility standards"
    tools:
      - nika:fetch
      - nika:search
      - nika:index
      - novanet_search
    auto_index: true
    prompt: |
      You have access to web fetching and semantic search tools.
      1. Fetch 10-15 relevant articles about QR accessibility
      2. As you fetch, content is automatically indexed
      3. Use nika:search to find specific patterns across all fetched content
      4. Also search NovaNet for existing entity knowledge
      5. Synthesize your findings into a structured report
      6. Include citations to the specific sources you found most relevant
```

---

## 5. Audio Architecture: Two Paths

### 5.1 Current State (March 2026)

| Capability | mistral.rs v0.7 | Status |
|-----------|-----------------|--------|
| Audio input (Gemma 3n audio encoder) | INFRA exists, model not production-ready | PR #1841 in progress |
| Speech-to-Text (Whisper) | NOT supported | Use whisper-rs |
| Text-to-Speech (Piper) | NOT supported | Use rten + Piper |
| Audio generation (music) | NOT supported | MCP ComfyUI (generate_song) |

### 5.2 Path 1: Explicit Pipeline (v0.30, ships now)

```
Audio file (.wav/.mp3)
    |
    v
nika:transcribe (whisper-rs, Whisper v3 Large)
    |
    v
Text output
    |
    v
infer: "Summarize this transcript..."
    |
    v
Text result
    |
    v (optionally)
nika:speak (rten + Piper, 50+ voices)
    |
    v
Audio file (.wav)
```

This is the **safe, proven path**. whisper-rs is mature (936 stars), Piper is mature (10.7k stars). Ships in v0.30.

### 5.3 Path 2: Native Audio Input in `infer:` (future, when mistral.rs ready)

```yaml
# Future: when mistral.rs supports Gemma 3n audio natively
tasks:
  - id: analyze_meeting
    infer:
      provider: native
      model: gemma-3n
      audio: ./meeting-recording.wav    # <-- new field
      prompt: "Summarize this meeting and extract action items"
```

Symmetry with vision:

| Media Field | Model | Status |
|-------------|-------|--------|
| `image:` | Qwen 3-VL via mistral.rs | Works today |
| `audio:` | Gemma 3n via mistral.rs | Future (when PR #1841 production-ready) |
| `video:` | TBD | Far future |

### 5.4 Decision

- **Ship Path 1** (`nika:transcribe` + `nika:speak`) in v0.30
- **Prepare AST** for Path 2 (`audio:` field in InferAction struct)
- **Activate Path 2** when mistral.rs merges audio support to stable

### 5.5 Audio Landscape (MCP Gap Confirmed)

```
Audio capability        Builtin?     MCP?        Verdict
---                     ---          ---         ---
Speech-to-Text (STT)   whisper-rs   NONE        BUILTIN ONLY (no MCP exists)
Text-to-Speech (TTS)   rten+piper   NONE        BUILTIN ONLY (no MCP exists)
Music generation        NO           comfyui     MCP ONLY (generate_song)
Sound effects           NO           comfyui     MCP ONLY (if nodes installed)
Audio input (LLM)       FUTURE       NONE        FUTURE (mistral.rs Gemma 3n)
```

---

## 6. Complete Tool Inventory (v0.30)

### 6.1 Existing Builtin Tools (11, already shipped)

| Tool | Purpose | Since |
|------|---------|-------|
| `nika:read` | Read file contents | v0.15.0 |
| `nika:write` | Write file contents | v0.15.0 |
| `nika:edit` | Edit file (find/replace) | v0.15.0 |
| `nika:glob` | Find files by pattern | v0.15.0 |
| `nika:grep` | Search file contents | v0.15.0 |
| `nika:sleep` | Wait (max 5 min) | v0.15.0 |
| `nika:records` | Query Punk Records | v0.30.0 (renamed from nika:episodes) |
| `nika:orchestrate` | Orchestration state | v0.30.0 (renamed from nika:strategy_state) |
| `nika:dag_state` | DAG introspection | v0.30.0 |
| `nika:budget` | Token budget tracking | v0.30.0 |
| `nika:task_status` | Task status query | v0.30.0 |

### 6.2 New Multi-Modal Builtin Tools (v0.30)

| Tool | Tier | Engine | Feature Flag | Default | Memory |
|------|------|--------|--------------|---------|--------|
| **`nika:vision`** | 1 | mistral.rs (Qwen 3-VL) | `vision` | ON | Shared with LLM |
| **`nika:embed`** | 1 | mistral.rs (EmbeddingGemma) | `embed` | ON | Shared with LLM |
| **`nika:transcribe`** | 2 | whisper-rs (Whisper v3) | `transcribe` | OFF | ~1.5 GB |
| **`nika:speak`** | 2 | rten + Piper | `speak` | OFF | ~0.1 GB |
| **`nika:ocr`** | 2 | ocr-rs (Tesseract) | `ocr` | OFF | ~0.4 GB |
| **`nika:translate`** | 2 | candle + NLLB-200 | `translate` | OFF | ~0.6 GB |
| **`nika:imagine`** | 3 | Smart Router (MCP) | Always available | -- | 0 (MCP) |
| **`nika:search`** | Meta | nika:embed + instant-distance | `embed` | ON* | ~0.1 GB |
| **`nika:index`** | Meta | nika:embed + instant-distance | `embed` | ON* | ~0.1 GB |

*nika:search and nika:index require `embed` feature flag (shared with nika:embed).

### 6.3 Total: 20 Builtin Tools

```
Existing (v0.27):     11 tools (file ops + introspection)
New multi-modal:       9 tools (vision, embed, transcribe, speak, ocr, translate, imagine, search, index)
                      --------
TOTAL v0.30:          20 builtin tools
```

---

## 7. Memory Budget Analysis

### 7.1 16GB Mac (Development Machine)

```
Component                          Memory      Cumulative
---                                ---         ---
macOS + apps                       ~4 GB       4 GB
LLM text (mistral.rs, Q4 7B)      ~4 GB       8 GB
+ nika:vision (Qwen 3-VL)         shared      8 GB (shared with LLM runtime)
+ nika:embed (EmbeddingGemma)      shared      8 GB (shared with LLM runtime)
+ nika:search (HNSW index)         ~0.1 GB     8.1 GB
---
MINIMAL (text + vision + embed + search): ~8 GB

+ nika:translate (NLLB-600M)       ~0.6 GB     8.7 GB
+ nika:ocr (Tesseract)             ~0.4 GB     9.1 GB
+ nika:transcribe (Whisper Large)  ~1.5 GB     10.6 GB
+ nika:speak (Piper)               ~0.1 GB     10.7 GB
---
FULL MULTIMODAL: ~11 GB

Headroom on 16GB Mac:              ~5 GB       COMFORTABLE
```

### 7.2 Progressive Activation

```
Level          Features ON                              Memory
---            ---                                      ---
Minimal        text only                                ~4 GB
Standard       text + vision + embed + search           ~8 GB (default)
Extended       + translate + ocr                        ~9 GB
Full           + transcribe + speak                     ~11 GB
```

### 7.3 Feature Flag Configuration

```toml
# .nika/config.toml
[features]
vision = true       # Tier 1: default ON
embed = true        # Tier 1: default ON
transcribe = false  # Tier 2: opt-in
speak = false       # Tier 2: opt-in
ocr = false         # Tier 2: opt-in
translate = false   # Tier 2: opt-in
# nika:imagine = always available (MCP, no local memory)
# nika:search/index = available when embed = true
```

---

## 8. MCP Aliases Additions (v0.30)

### 8.1 Proposed New Aliases

| Alias | Command | Description | Use Case |
|-------|---------|-------------|----------|
| `comfyui` | `uvx comfyui-mcp-server` | ComfyUI image/audio gen (13+ tools) | nika:imagine backend |
| `replicate` | `npx @replicate/mcp` | Replicate cloud models | nika:imagine fallback |
| `fal` | `npx @fal-ai/mcp` | FAL cloud generation | nika:imagine fallback |
| `deepl` | `npx deepl-mcp` | DeepL translation API | nika:translate upgrade |
| `elevenlabs` | `npx elevenlabs-mcp` | ElevenLabs TTS | nika:speak upgrade |

### 8.2 Updated Count

```
Current MCP_ALIASES (v0.27):  48
New aliases (v0.30):          +5
Total MCP_ALIASES (v0.30):    53
```

### 8.3 User Experience

```bash
# Add image generation
nika mcp add comfyui
# Now nika:imagine routes to ComfyUI automatically

# Add premium translation
nika mcp add deepl
nika provider set deepl
# Now nika:translate prefers DeepL when key available

# Add premium TTS
nika mcp add elevenlabs
nika provider set elevenlabs
# Now nika:speak prefers ElevenLabs when key available
```

---

## 9. Smart Router Applied to All Tools

### 9.1 Tool Routing Matrix

| Tool | Builtin | MCP Options | LLM Fallback | Error Action |
|------|---------|-------------|--------------|--------------|
| `nika:translate` | NLLB-200 | DeepL, Google Translate | infer: "Translate..." | Never (LLM always available) |
| `nika:imagine` | None | ComfyUI, Replicate, FAL, Aether | None | "nika mcp add comfyui" |
| `nika:speak` | Piper | ElevenLabs | None | "nika config set features.speak true" |
| `nika:transcribe` | Whisper | None (gap!) | None | "nika config set features.transcribe true" |
| `nika:ocr` | Tesseract | None (gap!) | nika:vision + prompt | "nika config set features.ocr true" |
| `nika:vision` | mistral.rs VLM | OpenAI (GPT-4o), Anthropic | None | "Enable native feature" |
| `nika:embed` | mistral.rs | OpenAI embeddings | None | "Enable native feature" |
| `nika:search` | instant-distance | None | None | "Enable embed feature" |

### 9.2 Interesting Fallback: `nika:ocr` -> `nika:vision`

OCR has a unique fallback path: if the OCR engine isn't available, we can use `nika:vision` with a prompt like "Extract all text from this image". Quality is lower than dedicated OCR but it works.

```
nika:ocr("invoice.pdf")
    |
    +-- 1. Tesseract enabled? --YES--> ocr-rs extraction (fast, accurate)
    |
    +-- 2. nika:vision available? --YES--> VLM: "Extract all text from this image"
    |                                       (slower, less accurate, but works)
    |
    +-- 3. Error: "No OCR backend. Enable: nika config set features.ocr true"
```

---

## 10. Cross-Reference: How This Connects to Other Docs

| Doc | Connection to This Document |
|-----|-----------------------------|
| **Doc 12** (Vegapunk Naming) | Smart Router = extension of Orchestrator routing. RunContext gets `vector_index` field for nika:search |
| **Doc 13** (Multi-Modal Worker) | Smart Router IS the evolved Orchestrator routing from Approach C. Capability matching now includes tool backends |
| **Doc 14** (DataStore Naming) | RunContext confirmed. Now extended with vector_index for HNSW |
| **Doc 15** (Ecosystem Coherence) | Tool Layer updated: 20 tools (was 11). New MCP aliases. Package types unchanged |
| **Doc 16** (Builtin Research) | Research validated. All crate selections confirmed. candle for NLLB is the new addition |

---

## 11. Implementation Roadmap (Feature Flags)

```
v0.28: P-MODEL + P-RECORD (Wave 1)
  |
v0.29: P-ORCHESTRATE + P-CONTEXT (Wave 2)
  |
v0.30: P-MEMORY + P-INTROSPECT + MULTIMODAL (Wave 3)
  |
  +-- Milestone 1: nika:vision + nika:embed (Tier 1, mistral.rs)
  +-- Milestone 2: nika:search + nika:index (Meta tools, instant-distance)
  +-- Milestone 3: nika:imagine (Tier 3, Smart Router + ComfyUI MCP alias)
  +-- Milestone 4: nika:translate (Tier 2, candle + NLLB-200)
  +-- Milestone 5: nika:transcribe + nika:speak (Tier 2, whisper-rs + rten/Piper)
  +-- Milestone 6: nika:ocr (Tier 2, ocr-rs)
  +-- Milestone 7: audio: field in infer: (future, when mistral.rs ready)
```

---

## 12. Research Sources

| Source | What It Provided |
|--------|------------------|
| Perplexity: NLLB-200 Rust inference | Confirmed candle + candle-transformers supports NLLB directly |
| Perplexity: Rust HNSW crates | Mapped instant-distance, hnswlib-rs, hnsw_rs, usearch, hannoy, DistX |
| Perplexity: mistral.rs Gemma 3n audio | Confirmed audio NOT yet production-ready in mistral.rs |
| Perplexity: ComfyUI MCP routing | Confirmed NO multi-backend routing exists in MCP ecosystem |
| Perplexity: rust-bert NLLB | No direct crate; candle is the path |
| Perplexity: Audio/STT/TTS MCP servers | Confirmed ZERO MCP servers for audio (gap validated) |
| Firecrawl: joenorton/comfyui-mcp-server | 219 stars, Python, 13+ tools, generate_song (audio!) |
| Firecrawl: alecc08/comfyui-mcp | 14 stars, TypeScript, 7 tools, img2img, upscale, bg removal |

---

## Confidence Level

**High** -- Based on:
- 8 Perplexity searches + 2 Firecrawl scrapes (March 2026 data)
- Direct crates.io verification for instant-distance, candle, whisper-rs
- ComfyUI MCP server source code analysis (both repos)
- Validated interactively with Thibaut during brainstorm session
