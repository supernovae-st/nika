# 15 — Nika Ecosystem Coherence

> How everything fits together in Nika v0.30+.
> Master reference document capturing the unified view of all ecosystem pieces.

**Status:** BRAINSTORM | **Date:** 2026-03-15

---

## 1. Validated Decisions Summary

Decisions prises au cours des sessions de brainstorming (docs 12-17).

| Decision | Status | Source | Details |
|----------|--------|--------|---------|
| DataStore → **RunContext** | VALIDATED | Doc 14 | Industry standard name, 205 refs to rename |
| Approach C + **Progressive Disclosure** | VALIDATED | Doc 13 | 3 levels: minimal/medium/full |
| **Egghead** removed from naming | VALIDATED | Doc 12 + 14 | Was proposed, rejected — RunContext preferred |
| spn **deprecated**, all via `nika` | VALIDATED | v0.27.0 | Already implemented |
| **Vegapunk names** (Shaka, Edison, Atlas, Punk Records, Satellite) | VALIDATED | Doc 12 | One Piece-inspired naming |
| **AgentRecord → Record** | VALIDATED | Doc 12 | Simpler name for NDJSON entries |
| **3-Tier Memory** (HOT/WARM/COLD) | VALIDATED | Doc 12 | RunContext → Punk Records → NovaNet |
| Image gen via **MCP tools** | VALIDATED | Doc 13-17 | Recommended over dedicated satellite type |
| **Smart Router Pattern** | VALIDATED | Doc 17 | Unified dispatch: builtin > MCP > LLM fallback > error |
| **`nika:imagine`** (image gen) | VALIDATED | Doc 17 | MCP-only Smart Router (ComfyUI/Replicate/FAL). Name: One Piece Gear 5 |
| **`nika:translate`** builtin | VALIDATED | Doc 17 | candle + NLLB-200 (600MB, 200 langues, offline) |
| **`nika:search`** native RAG | VALIDATED | Doc 17 | nika:embed + instant-distance HNSW, in-memory |
| **`nika:index`** companion | VALIDATED | Doc 17 | Manual + auto-index into RunContext vector index |
| **Audio input in infer:** | VALIDATED (future) | Doc 17 | audio: field when mistral.rs supports Gemma 3n |
| **Tool prefix nika:** neutral | VALIDATED | Doc 17 | Descriptive names, NOT One Piece refs for tools |
| **5 new MCP aliases** | PROPOSED | Doc 17 | comfyui, replicate, fal, deepl, elevenlabs |
| **SatelliteCard** in NovaNet | PROPOSED | Brainstorm | Node class for satellite metadata |
| **5 package types** | PROPOSED | Brainstorm | skill, workflow, satellite, model, mcp |
| **Capability inference** automatic | PROPOSED | Doc 13 | Model modalities + tool schemas |
| **Local-first + cloud fallback** | PROPOSED | Brainstorm | `fallback:` in satellite def |

---

## 2. The Big Picture Architecture

L'architecture complete de Nika v0.30+ avec tous les layers interconnectes.

```
╔═══════════════════════════════════════════════════════════════════════════════════════════╗
║                           NIKA v0.30+ ECOSYSTEM ARCHITECTURE                              ║
╠═══════════════════════════════════════════════════════════════════════════════════════════╣
║                                                                                           ║
║  ┌──────────────────────────────────────────────────────────────────────────────────────┐ ║
║  │                            ORCHESTRATION LAYER                                       │ ║
║  ├──────────────────────────────────────────────────────────────────────────────────────┤ ║
║  │                                                                                      │ ║
║  │   🎯 ORCHESTRATOR (PUNK-01)                                                                 │ ║
║  │   ├── Dynamic Router LLM (claude-haiku / gpt-4o-mini)                               │ ║
║  │   ├── Capability Matching (accepts/produces MIME types)                              │ ║
║  │   ├── Satellite Dispatch (parallel or sequential based on dependencies)            │ ║
║  │   └── Fallback Handling (when no satellite matches)                                 │ ║
║  │                                                                                      │ ║
║  │   Alternative: orchestration: dag (static DAG, explicit dependencies)               │ ║
║  │                                                                                      │ ║
║  └───────────────────────────────────────┬──────────────────────────────────────────────┘ ║
║                                          │                                                ║
║                                          ▼                                                ║
║  ┌──────────────────────────────────────────────────────────────────────────────────────┐ ║
║  │                              SATELLITE LAYER                                          │ ║
║  ├──────────────────────────────────────────────────────────────────────────────────────┤ ║
║  │                                                                                      │ ║
║  │   🛰️ SATELLITES (Workers specialises)                                                │ ║
║  │                                                                                      │ ║
║  │   ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐                   │ ║
║  │   │  vision-analyst  │  │  text-analyst    │  │ creative-director│                   │ ║
║  │   │  ──────────────  │  │  ──────────────  │  │  ──────────────  │                   │ ║
║  │   │  model: gpt-4o   │  │  model: sonnet   │  │  model: sonnet   │                   │ ║
║  │   │  slot: —         │  │  slot: edison    │  │  slot: edison    │                   │ ║
║  │   │  accepts: image  │  │  accepts: text   │  │  accepts: text   │                   │ ║
║  │   │  produces: json  │  │  produces: md    │  │  produces: image │                   │ ║
║  │   │  tools: [read]   │  │  tools: [novanet]│  │  tools: [img-gen]│                   │ ║
║  │   └──────────────────┘  └──────────────────┘  └──────────────────┘                   │ ║
║  │                                                                                      │ ║
║  │   ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐                   │ ║
║  │   │  local-embedder  │  │  code-generator  │  │  seo-optimizer   │                   │ ║
║  │   │  ──────────────  │  │  ──────────────  │  │  ──────────────  │                   │ ║
║  │   │  provider: native│  │  slot: pythagoras│  │  slot: york      │                   │ ║
║  │   │  model: gguf     │  │  model: sonnet   │  │  model: sonar-pro│                   │ ║
║  │   │  accepts: text   │  │  accepts: md     │  │  accepts: text   │                   │ ║
║  │   │  produces: embed │  │  produces: code  │  │  produces: json  │                   │ ║
║  │   └──────────────────┘  └──────────────────┘  └──────────────────┘                   │ ║
║  │                                                                                      │ ║
║  │   Model Slots:                                                                       │ ║
║  │   💡 edison (PUNK-03) — Main creative work                                           │ ║
║  │   🧮 pythagoras (PUNK-04) — Deep reasoning                                           │ ║
║  │   💪 atlas (PUNK-05) — Fast tactical tasks                                           │ ║
║  │   🔍 york (PUNK-06) — Search & retrieval                                             │ ║
║  │                                                                                      │ ║
║  └───────────────────────────────────────┬──────────────────────────────────────────────┘ ║
║                                          │                                                ║
║                                          ▼                                                ║
║  ┌──────────────────────────────────────────────────────────────────────────────────────┐ ║
║  │                                TOOL LAYER                                             │ ║
║  ├──────────────────────────────────────────────────────────────────────────────────────┤ ║
║  │                                                                                      │ ║
║  │   🔧 BUILTIN TOOLS (20)                      🔌 MCP TOOLS (via servers)              │ ║
║  │   ├── nika:read                               ├── novanet_search                     │ ║
║  │   ├── nika:write                              ├── novanet_context                    │ ║
║  │   ├── nika:edit                               ├── novanet_write                      │ ║
║  │   ├── nika:glob                               ├── comfyui:generate                   │ ║
║  │   ├── nika:grep                               ├── replicate:run                      │ ║
║  │   ├── nika:sleep                              ├── browser:navigate                   │ ║
║  │   ├── nika:records                            ├── github:create_pr                   │ ║
║  │   ├── nika:orchestrate                              ├── slack:send_message                 │ ║
║  │   ├── nika:dag_state                          └── ... (100 MCP aliases)               │ ║
║  │   ├── nika:budget                                                                    │ ║
║  │   ├── nika:task_status                                                               │ ║
║  │   ├── nika:vision (Tier 1)                                                           │ ║
║  │   ├── nika:embed (Tier 1)                                                            │ ║
║  │   ├── nika:transcribe (Tier 2)                                                       │ ║
║  │   ├── nika:speak (Tier 2)                                                            │ ║
║  │   ├── nika:ocr (Tier 2)                                                              │ ║
║  │   ├── nika:translate (Tier 2)                                                        │ ║
║  │   ├── nika:imagine (Tier 3, MCP)                                                     │ ║
║  │   ├── nika:search (Meta)                                                             │ ║
║  │   └── nika:index (Meta)                                                              │ ║
║  │                                                                                      │ ║
║  └───────────────────────────────────────┬──────────────────────────────────────────────┘ ║
║                                          │                                                ║
║                                          ▼                                                ║
║  ┌──────────────────────────────────────────────────────────────────────────────────────┐ ║
║  │                                DATA LAYER                                             │ ║
║  ├──────────────────────────────────────────────────────────────────────────────────────┤ ║
║  │                                                                                      │ ║
║  │   3-TIER MEMORY ARCHITECTURE                                                         │ ║
║  │                                                                                      │ ║
║  │   ┌────────────────────────────────────────────────────────────────────────────────┐ │ ║
║  │   │ HOT: RunContext (DashMap in RAM)                                               │ │ ║
║  │   │ Lifetime: un workflow run │ Contenu: TaskResults, bindings, inputs, context   │ │ ║
║  │   │ Struct Rust: RunContext (anciennement DataStore) │ 205 refs to migrate        │ │ ║
║  │   └───────────────────────────────────────────────────┬────────────────────────────┘ │ ║
║  │                                                       │ compress + save              │ ║
║  │   ┌───────────────────────────────────────────────────▼────────────────────────────┐ │ ║
║  │   │ WARM: Punk Records (NDJSON on disk)                                            │ │ ║
║  │   │ Lifetime: configurable TTL (7d-90d-∞) │ Contenu: Record summaries compresses  │ │ ║
║  │   │ Struct Rust: RecordLog │ Files: .nika/records/{date}_{run_id}.ndjson          │ │ ║
║  │   └───────────────────────────────────────────────────┬────────────────────────────┘ │ ║
║  │                                                       │ promote (if confidence>seuil)│ ║
║  │   ┌───────────────────────────────────────────────────▼────────────────────────────┐ │ ║
║  │   │ COLD: NovaNet (Neo4j via MCP)                                                  │ │ ║
║  │   │ Lifetime: permanent │ Contenu: Records promus, Entity, Page, SEO...           │ │ ║
║  │   │ Node class: Record (layer agent) │ Arcs: HAS_RECORD, RELATES_TO, FOR_LOCALE   │ │ ║
║  │   └────────────────────────────────────────────────────────────────────────────────┘ │ ║
║  │                                                                                      │ ║
║  └──────────────────────────────────────────────────────────────────────────────────────┘ ║
║                                                                                           ║
║  ┌──────────────────────────────────────────────────────────────────────────────────────┐ ║
║  │                              PACKAGE LAYER                                            │ ║
║  ├──────────────────────────────────────────────────────────────────────────────────────┤ ║
║  │                                                                                      │ ║
║  │   📦 5 PACKAGE TYPES                                                                 │ ║
║  │                                                                                      │ ║
║  │   ┌──────────┐  ┌──────────┐  ┌───────────┐  ┌──────────┐  ┌──────────┐             │ ║
║  │   │  skill   │  │ workflow │  │ satellite │  │  model   │  │   mcp    │             │ ║
║  │   │  ──────  │  │  ──────  │  │  ───────  │  │  ─────   │  │  ─────   │             │ ║
║  │   │ .md      │  │.nika.yaml│  │.yaml defs │  │ GGUF refs│  │ configs  │             │ ║
║  │   │ prompts  │  │ DAGs     │  │ workers   │  │ HuggingF │  │ servers  │             │ ║
║  │   └──────────┘  └──────────┘  └───────────┘  └──────────┘  └──────────┘             │ ║
║  │                                                                                      │ ║
║  │   Install: nika add @scope/name │ Manifest: nika.yaml │ Registry: skills.sh        │ ║
║  │                                                                                      │ ║
║  └──────────────────────────────────────────────────────────────────────────────────────┘ ║
║                                                                                           ║
╚═══════════════════════════════════════════════════════════════════════════════════════════╝
```

---

## 3. The End-to-End Flow

Exemple concret d'une requete traversant tout l'ecosysteme.

### User Request

```
"Analyse ce QR code et genere une landing page avec hero image"
```

### Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│  END-TO-END FLOW                                                                            │
├─────────────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                             │
│  1. USER INPUT                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────────────────┐   │
│  │  Prompt: "Analyse ce QR code et genere une landing page avec hero image"            │   │
│  │  Attachments: ./data/qr-sample.png (image/png)                                      │   │
│  └────────────────────────────────────────────────┬────────────────────────────────────┘   │
│                                                   │                                         │
│  2. ORCHESTRATOR ROUTING                                 ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────────────────────┐   │
│  │  Orchestrator detecte:                                                                      │   │
│  │  • Input MIME: image/png → needs vision capability                                   │   │
│  │  • Output demande: "landing page" → needs text generation                            │   │
│  │  • Output demande: "hero image" → needs image generation                             │   │
│  │                                                                                      │   │
│  │  Routing decision:                                                                   │   │
│  │  1. vision-analyst (accepts image → produces json)                                   │   │
│  │  2. text-analyst (accepts json → produces markdown) [depends on 1]                   │   │
│  │  3. creative-director (accepts markdown → produces image) [depends on 2]             │   │
│  └────────────────────────────────────────────────┬────────────────────────────────────┘   │
│                                                   │                                         │
│  3. SATELLITE EXECUTION                           ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────────────────────┐   │
│  │                                                                                      │   │
│  │  ┌──────────────────────────────────────────────────────────────────────────────┐   │   │
│  │  │  vision-analyst (gpt-4o)                                                      │   │   │
│  │  │  ────────────────────────                                                     │   │   │
│  │  │  Input: qr-sample.png                                                         │   │   │
│  │  │  Tools: [nika:read]                                                           │   │   │
│  │  │  Output: {                                                                    │   │   │
│  │  │    "qr_content": "https://qrcode-ai.com/demo",                               │   │   │
│  │  │    "visual_analysis": "High contrast black/white, embedded logo...",          │   │   │
│  │  │    "decoded_url": "https://qrcode-ai.com/demo"                                │   │   │
│  │  │  }                                                                            │   │   │
│  │  └───────────────────────────────────────────────────────┬──────────────────────┘   │   │
│  │                                                          │                          │   │
│  │  ┌───────────────────────────────────────────────────────▼──────────────────────┐   │   │
│  │  │  text-analyst (claude-sonnet + NovaNet)                                       │   │   │
│  │  │  ─────────────────────────────────────                                        │   │   │
│  │  │  Input: vision-analyst.output (json)                                          │   │   │
│  │  │  Tools: [novanet_context, novanet_search]                                     │   │   │
│  │  │  Context loaded via MCP:                                                      │   │   │
│  │  │    - Entity: "qr-code" (fr-FR)                                                │   │   │
│  │  │    - SEOKeyword: "generateur qr code", "qr code gratuit"                      │   │   │
│  │  │  Output: landing_page.md (1200 words, SEO H1/H2)                              │   │   │
│  │  └───────────────────────────────────────────────────────┬──────────────────────┘   │   │
│  │                                                          │                          │   │
│  │  ┌───────────────────────────────────────────────────────▼──────────────────────┐   │   │
│  │  │  creative-director (claude-sonnet + image-gen MCP)                            │   │   │
│  │  │  ────────────────────────────────────────────────                             │   │   │
│  │  │  Input: text-analyst.output (markdown)                                        │   │   │
│  │  │  Tools: [image-gen:generate_image, image-gen:edit_image]                      │   │   │
│  │  │  Prompt: "Create hero image based on landing page theme"                      │   │   │
│  │  │  Output: hero-image.png (1920x1080, on-brand)                                 │   │   │
│  │  └──────────────────────────────────────────────────────────────────────────────┘   │   │
│  │                                                                                      │   │
│  └────────────────────────────────────────────────┬────────────────────────────────────┘   │
│                                                   │                                         │
│  4. RUNCONTEXT ACCUMULATION                       ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────────────────────┐   │
│  │  RunContext (DashMap) accumule:                                                      │   │
│  │  • results["vision-analyst"] → JSON analysis                                         │   │
│  │  • results["text-analyst"] → markdown landing page                                   │   │
│  │  • results["creative-director"] → path to hero-image.png                             │   │
│  │  • context["entity_context"] → NovaNet data loaded via MCP                           │   │
│  │  • bindings["qr_image"] → ./data/qr-sample.png                                       │   │
│  └────────────────────────────────────────────────┬────────────────────────────────────┘   │
│                                                   │                                         │
│  5. PUNK RECORDS LOGGING                          ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────────────────────┐   │
│  │  .nika/records/2026-03-15_abc123.ndjson:                                             │   │
│  │  {"id":"rec-001","task_id":"vision-analyst","summary":"QR analysis...","conf":0.95}  │   │
│  │  {"id":"rec-002","task_id":"text-analyst","summary":"Landing page...","conf":0.89}   │   │
│  │  {"id":"rec-003","task_id":"creative-director","summary":"Hero image...","conf":0.82}│   │
│  └────────────────────────────────────────────────┬────────────────────────────────────┘   │
│                                                   │                                         │
│  6. FINAL RESULT                                  ▼                                         │
│  ┌─────────────────────────────────────────────────────────────────────────────────────┐   │
│  │  Assembled from RunContext:                                                          │   │
│  │  • landing_page.md — Full markdown with SEO structure                                │   │
│  │  • hero-image.png — Generated hero image                                             │   │
│  │  • metadata.json — QR analysis + workflow trace reference                            │   │
│  └─────────────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                             │
└─────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. One CLI: `nika`

Depuis v0.27.0, TOUT passe par un seul CLI unifie. `spn` est deprecated.

```bash
# ═══════════════════════════════════════════════════════════════════════════════
#  PROVIDER MANAGEMENT (API keys)
# ═══════════════════════════════════════════════════════════════════════════════

nika provider list                   # Show all providers with status
nika keys set anthropic          # Store API key in OS keychain
nika provider get openai             # Retrieve key (masked)
nika provider test claude            # Validate key with provider
nika provider migrate                # Migrate env vars to keychain

# ═══════════════════════════════════════════════════════════════════════════════
#  MODEL MANAGEMENT (Local GGUF models)
# ═══════════════════════════════════════════════════════════════════════════════

nika model list                      # List available local models
nika model pull qwen2-vl-2b-q4       # Download model from HuggingFace
nika model info llama3.2-3b          # Show model details
nika model search vision             # Search models by capability

# ═══════════════════════════════════════════════════════════════════════════════
#  PACKAGE MANAGEMENT
# ═══════════════════════════════════════════════════════════════════════════════

nika add @supernovae/vision-analyst  # Add satellite package
nika add @supernovae/seo-expert      # Add skill package
nika add @supernovae/page-generator  # Add workflow package
nika remove @supernovae/old-skill    # Remove package
nika list                            # List installed packages
nika update                          # Update all packages

# ═══════════════════════════════════════════════════════════════════════════════
#  MCP SERVER MANAGEMENT
# ═══════════════════════════════════════════════════════════════════════════════

nika mcp add image-gen               # Add MCP server (100 aliases)
nika mcp add novanet                 # Add NovaNet MCP
nika mcp list                        # List configured servers
nika mcp test neo4j                  # Test server connection
nika mcp tools novanet               # List available tools

# ═══════════════════════════════════════════════════════════════════════════════
#  RUN WORKFLOWS
# ═══════════════════════════════════════════════════════════════════════════════

nika workflow.nika.yaml              # Run workflow (positional)
nika run workflow.nika.yaml          # Run workflow (explicit)
nika check workflow.nika.yaml        # Validate workflow

# ═══════════════════════════════════════════════════════════════════════════════
#  TUI MODES
# ═══════════════════════════════════════════════════════════════════════════════

nika                                 # TUI Home view
nika studio                          # Studio view (YAML editor)
nika studio workflow.yaml            # Studio with file
nika chat                            # Chat view (conversational agent)

# ═══════════════════════════════════════════════════════════════════════════════
#  RECORDS (Punk Records)
# ═══════════════════════════════════════════════════════════════════════════════

nika records list                    # List recent runs with records
nika records show <run-id>           # Show records from a run
nika records search "keyword"        # Full-text search
nika records promote <record-id>     # Promote to NovaNet
nika records prune                   # Garbage collection
nika records stats                   # Usage statistics

# ═══════════════════════════════════════════════════════════════════════════════
#  SETUP & SYSTEM
# ═══════════════════════════════════════════════════════════════════════════════

nika setup                           # Interactive onboarding wizard
nika setup nika                      # Install Nika + LSP + Daemon
nika setup novanet                   # Configure NovaNet + Neo4j
nika sync                            # Sync to enabled editors
nika sync --status                   # Show sync status
nika daemon start                    # Start background daemon
nika daemon status                   # Show daemon status
nika doctor                          # Verify installation
```

---

## 5. Progressive Disclosure Everywhere

Le meme principe applique a chaque niveau de l'ecosysteme.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                          PROGRESSIVE DISCLOSURE                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  LEVEL      │  SATELLITE              │  WORKFLOW               │  PACKAGE   ║
║  ───────────┼─────────────────────────┼─────────────────────────┼────────────║
║             │                         │                         │            ║
║  MINIMAL    │  id + model             │  schema + tasks         │  nika add  ║
║  (2 fields) │  Just works with        │  Minimal valid          │  @scope/   ║
║             │  default accepts/       │  workflow that          │  name      ║
║             │  produces inferred      │  runs                   │            ║
║             │                         │                         │            ║
║  ───────────┼─────────────────────────┼─────────────────────────┼────────────║
║             │                         │                         │            ║
║  MEDIUM     │  + accepts/produces     │  + mcp servers          │  + version ║
║  (5 fields) │  + slot (edison/atlas)  │  + context: files       │  constraints║
║             │  + tools[] list         │  + skills[]             │  @1.0.0    ║
║             │                         │                         │            ║
║  ───────────┼─────────────────────────┼─────────────────────────┼────────────║
║             │                         │                         │            ║
║  FULL       │  + capabilities[]       │  + goal: │  + nika.yaml║
║  (10 fields)│  + fallback model       │  + satellites[]         │  manifest  ║
║             │  + record: config       │  + record: config       │  with deps ║
║             │  + cost/latency hints   │  + goal: config        │            ║
║             │                         │                         │            ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Satellite Examples

```yaml
# ═══════════════════════════════════════════════════════════════════════════════
# MINIMAL (2 champs) — Just works
# ═══════════════════════════════════════════════════════════════════════════════
satellites:
  - id: analyzer
    model: anthropic/claude-sonnet-4-20250514

# ═══════════════════════════════════════════════════════════════════════════════
# MEDIUM (5 champs) — Explicit capabilities
# ═══════════════════════════════════════════════════════════════════════════════
satellites:
  - id: vision-analyst
    model: openai/gpt-4o
    slot: edison
    accepts: [image/png, image/jpeg]
    produces: [application/json, text/markdown]
    tools: [nika:read]

# ═══════════════════════════════════════════════════════════════════════════════
# FULL (10 champs) — Production-grade
# ═══════════════════════════════════════════════════════════════════════════════
satellites:
  - id: vision-analyst
    model:
      provider: openai
      name: gpt-4o
      fallback:
        provider: native
        name: qwen2-vl-2b-q4.gguf
    slot: edison
    accepts: [image/png, image/jpeg, image/webp]
    produces: [application/json, text/markdown]
    capabilities: [vision, image-analysis, ocr]
    tools: [nika:read, nika:glob]
    record:
      compress: true
      max_tokens: 500
      promote: auto
    hints:
      cost: low
      latency: fast
```

---

## 6. Local-First Architecture

Le modele local → cloud fallback pour optimiser cout et latence.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                         LOCAL-FIRST ARCHITECTURE                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  DEVELOPMENT MODE (zero API calls, zero cost)                                 ║
║  ─────────────────────────────────────────────                                ║
║  • provider: native partout                                                   ║
║  • Models GGUF locaux (qwen2-vl, llama3.2, nomic-embed)                       ║
║  • Iteration rapide sans rate limits                                          ║
║  • GPU local: Metal (macOS) ou CUDA (Linux)                                   ║
║                                                                               ║
║  PRODUCTION MODE (maximum quality)                                            ║
║  ──────────────────────────────────                                           ║
║  • provider: anthropic/openai/mistral                                         ║
║  • Models cloud (claude-sonnet-4, gpt-4o, mistral-large)                              ║
║  • Best quality pour les outputs client-facing                                ║
║  • Fallback vers native si API down                                           ║
║                                                                               ║
║  HYBRID MODE (cost optimization)                                              ║
║  ────────────────────────────────                                             ║
║  • Native pour embeddings, small tasks, preprocessing                         ║
║  • Cloud pour complex reasoning, final generation                             ║
║  • Best cost/quality tradeoff                                                 ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Fallback Configuration

```yaml
satellites:
  # Vision with fallback
  - id: vision-local
    model:
      provider: native
      name: qwen2-vl-2b-q4.gguf
      fallback:
        provider: openai
        name: gpt-4o
    accepts: [image/png, image/jpeg]
    produces: [application/json]

  # Text with fallback
  - id: text-analyst
    model:
      provider: anthropic
      name: claude-sonnet-4-20250514
      fallback:
        provider: native
        name: llama3.2-3b-q4.gguf
    slot: edison
    accepts: [text/markdown, application/json]
    produces: [text/markdown]

  # Embeddings always local (fast, cheap)
  - id: embedder
    model:
      provider: native
      name: nomic-embed-text-v1.5.gguf
      # No fallback — embeddings should always be local
    accepts: [text/plain]
    produces: [application/x-embedding]
```

### Resolution Priority

```
1. Si provider: native → utiliser NativeRuntime avec model GGUF local
2. Si provider: anthropic/openai/... → utiliser API cloud
3. Si fallback: defini ET primary echoue → utiliser fallback
4. Si fallback: non defini ET primary echoue → erreur avec message clair
```

---

## 7. MCP as Universal Capability Extender

Les satellites utilisent les MCP tools comme "mains" pour executer des actions specialisees.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                    MCP: THE HANDS OF THE SATELLITES                           ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Le LLM du satellite est le "cerveau" — il decide quoi faire                  ║
║  Les MCP tools sont les "mains" — ils executent les actions                   ║
║                                                                               ║
║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
║  │                         SATELLITE                                        │  ║
║  │                                                                          │  ║
║  │   🧠 BRAIN (LLM)                                                         │  ║
║  │   ├── Reasoning about the task                                           │  ║
║  │   ├── Deciding which tools to call                                       │  ║
║  │   ├── Interpreting tool results                                          │  ║
║  │   └── Generating final output                                            │  ║
║  │                                                                          │  ║
║  │   🤲 HANDS (MCP Tools)                                                    │  ║
║  │   ├── novanet_search → Query knowledge graph                             │  ║
║  │   ├── novanet_context → Load entity context                              │  ║
║  │   ├── novanet_write → Write results back                                 │  ║
║  │   ├── image-gen:generate → Create images                                 │  ║
║  │   ├── browser:navigate → Browse web pages                                │  ║
║  │   ├── github:create_pr → Create pull requests                            │  ║
║  │   └── ... (specialized operations)                                       │  ║
║  │                                                                          │  ║
║  └─────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  EXAMPLE: text-analyst satellite                                              ║
║  ──────────────────────────────────                                           ║
║  1. Brain receives: "Generate SEO landing page for QR code"                   ║
║  2. Brain calls: novanet_search(query="qr code", kinds=["Entity"])           ║
║  3. Brain calls: novanet_context(focus_key="qr-code", locale="fr-FR")        ║
║  4. Brain generates: Landing page using context                               ║
║  5. Brain calls: novanet_write(class="PageNative", ...) to persist            ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Available MCP Servers (100 aliases via `nika mcp add`)

| Category | Server | Key Tools |
|----------|--------|-----------|
| **Knowledge** | novanet | search, context, introspect, write, audit |
| **Image** | image-gen | generate_image, edit_image, upscale |
| **Browser** | browser | navigate, screenshot, extract |
| **Code** | github | create_pr, create_issue, get_file |
| **Communication** | slack | send_message, read_channel |
| **Search** | perplexity | search, research |
| **Database** | neo4j | read_cypher, write_cypher |
| **Files** | filesystem | read, write, list |

---

## 8. NovaNet as Shared Brain

NovaNet sert de memoire partagee pour tous les satellites via MCP.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                        NOVANET: THE SHARED BRAIN                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  NovaNet est le "cerveau" partage — une knowledge graph accessible via MCP    ║
║  Chaque satellite peut lire/ecrire via les 7 tools MCP                        ║
║                                                                               ║
║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
║  │                         NOVANET (Neo4j)                                  │  ║
║  ├─────────────────────────────────────────────────────────────────────────┤  ║
║  │                                                                          │  ║
║  │  SHARED REALM (36 nodes, READ-ONLY)                                      │  ║
║  │  ├── Locale (en-US, fr-FR, de-DE, ...)                                   │  ║
║  │  ├── Country, Region, City                                               │  ║
║  │  ├── Term, Expression, Pattern (knowledge atoms)                         │  ║
║  │  └── CultureRef, Taboo, AudienceTrait                                    │  ║
║  │                                                                          │  ║
║  │  ORG REALM (23 nodes, READ-WRITE)                                        │  ║
║  │  ├── Entity, EntityNative (semantic concepts)                            │  ║
║  │  ├── Page, PageNative (URL-owning structure)                             │  ║
║  │  ├── Block, BlockNative (content units)                                  │  ║
║  │  ├── Project, Brand, Product                                             │  ║
║  │  ├── SEOKeyword, SEOCluster, SEOPillar                                   │  ║
║  │  └── Record (PROPOSED — promoted from Punk Records)                      │  ║
║  │                                                                          │  ║
║  │  PROPOSED: SatelliteCard (satellite metadata/discovery)                  │  ║
║  │  ├── key: "vision-analyst"                                               │  ║
║  │  ├── description: "Analyse images and extract structured data"           │  ║
║  │  ├── accepts: ["image/png", "image/jpeg"]                                │  ║
║  │  ├── produces: ["application/json", "text/markdown"]                     │  ║
║  │  ├── capabilities: ["vision", "ocr", "image-analysis"]                   │  ║
║  │  └── model_requirements: { vision: true, reasoning: false }              │  ║
║  │                                                                          │  ║
║  └─────────────────────────────────────────────────────────────────────────┘  ║
║                                                                               ║
║  MCP TOOLS (8 — The Great Cleanup v0.20.0)                                    ║
║  ─────────────────────────────────────────                                    ║
║  • novanet_describe — Bootstrap schema understanding                          ║
║  • novanet_introspect — Query NodeClasses/ArcClasses                          ║
║  • novanet_search — Find nodes (5 modes: fulltext, property, hybrid, walk, triggers)   ║
║  • novanet_context — Assemble LLM context (4 modes: page, block, knowledge, assemble)  ║
║  • novanet_write — Create/update data (dry_run to validate)                   ║
║  • novanet_audit — Check data quality + CSR metrics                           ║
║  • novanet_batch — Multiple operations in parallel                            ║
║  • novanet_query — Custom Cypher (LAST RESORT)                                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Entity/Page/Record Flow

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  CONTENT GENERATION FLOW                                                     │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. Entity (defined)                                                         │
│     │   key: "qr-code"                                                       │
│     │   description: "Quick Response code for data encoding"                 │
│     │                                                                        │
│     ├──[:HAS_NATIVE]→ EntityNative (authored)                                │
│     │                   locale: fr-FR                                        │
│     │                   text: "Code QR"                                      │
│     │                   title: "Code QR — Generateur Gratuit"                │
│     │                                                                        │
│     └──[:REPRESENTS]→ Page (defined)                                         │
│                         slug: "generateur-qr-code"                           │
│                         │                                                    │
│                         ├──[:HAS_NATIVE]→ PageNative (generated)             │
│                         │                  locale: fr-FR                     │
│                         │                  html: "<html>..."                 │
│                         │                                                    │
│                         └──[:HAS_BLOCK]→ Block                               │
│                                           ├──[:HAS_NATIVE]→ BlockNative      │
│                                           └── ...                            │
│                                                                              │
│  2. Record (promoted from Punk Records) [PROPOSED]                           │
│     │   key: "record-abc123"                                                 │
│     │   summary: "SEO analysis revealed 15 high-value keywords..."          │
│     │   confidence: 0.92                                                     │
│     │   verb: "infer"                                                        │
│     │   model: "claude-sonnet-4"                                                     │
│     │                                                                        │
│     ├──[:RELATES_TO]→ Entity (qr-code)                                       │
│     └──[:FOR_LOCALE]→ Locale (fr-FR)                                         │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 9. Package System

Les 5 types de packages et le manifest `nika.yaml`.

### Package Types

| Type | Contents | Example | Command |
|------|----------|---------|---------|
| **skill** | .md system prompts | `@supernovae/seo-expert` | `nika add @supernovae/seo-expert` |
| **workflow** | .nika.yaml DAGs | `@supernovae/page-gen` | `nika add @supernovae/page-gen` |
| **satellite** | satellite.yaml worker defs | `@supernovae/vision-analyst` | `nika add @supernovae/vision-analyst` |
| **model** | GGUF refs for native | `qwen2-vl-2b-q4` | `nika model pull qwen2-vl-2b-q4` |
| **mcp** | Server configs | `novanet`, `image-gen` | `nika mcp add novanet` |

### Manifest: `nika.yaml`

```yaml
name: qrcode-ai-seo
version: 0.1.0
description: QR Code AI SEO generation pipeline
author: SuperNovae Studio
license: MIT

# ═══════════════════════════════════════════════════════════════════════════════
# DEPENDENCIES
# ═══════════════════════════════════════════════════════════════════════════════

dependencies:
  # Satellites for specialized workers
  satellites:
    - "@supernovae/vision-analyst@^1.0"
    - "@supernovae/text-analyst@^2.0"
    - "@supernovae/creative-director@^1.0"

  # Skills for system prompts
  skills:
    - "@supernovae/seo-expert@^1.0"
    - "@supernovae/brand-voice@^1.0"

  # Workflows to compose
  workflows:
    - "@supernovae/page-generator@^3.0"
    - "@supernovae/image-pipeline@^2.0"

  # Local models for native inference
  models:
    - "qwen2-vl-2b-q4"
    - "nomic-embed-text-v1.5"
    - "llama3.2-3b-q4"

  # MCP servers for external capabilities
  mcp:
    - "novanet"
    - "image-gen"
    - "browser"

# ═══════════════════════════════════════════════════════════════════════════════
# CONFIGURATION
# ═══════════════════════════════════════════════════════════════════════════════

config:
  # Default provider for cloud inference
  provider: anthropic

  # Model slots assignment
  models:
    edison: claude-sonnet-4-20250514
    atlas: claude-haiku-35
    pythagoras: claude-sonnet-4-20250514
    york: perplexity/sonar-pro

  # Punk Records settings
  records:
    ttl: "30d"
    auto_promote: false
    confidence_threshold: 0.85
```

### Package Resolution

```
1. nika add @supernovae/vision-analyst
   │
   ├── Check registry (skills.sh) for package metadata
   │
   ├── Download satellite.yaml to .nika/satellites/vision-analyst.yaml
   │
   ├── Parse accepts/produces/capabilities
   │
   └── Register in nika.yaml dependencies
```

---

## 10. Coherence Table

Table maitre montrant comment chaque piece s'integre.

| Piece | Role | Integrated via | Example |
|-------|------|----------------|---------|
| **Orchestrator** | Orchestre les satellites | `goal:` | Routes image→vision-analyst |
| **Satellite** | Worker specialise | YAML def ou package | `@supernovae/vision-analyst` |
| **Model cloud** | LLM distant | `provider: anthropic/openai/...` | gpt-4o, claude-sonnet |
| **Model native** | LLM local | `provider: native` + GGUF | qwen2-vl, llama3.2 |
| **Builtin tools** | File I/O, multimodal, RAG, introspection | Auto-available | nika:read, nika:vision, nika:embed, nika:translate, nika:search |
| **MCP tools** | Capabilities externes | `mcp.servers` | image-gen, novanet, browser |
| **NovaNet** | Knowledge graph | MCP server | Entity, Page, SEO, Record |
| **RunContext** | State ephemere HOT | DashMap RAM | Task outputs, bindings |
| **Punk Records** | Trace WARM | NDJSON files | Replay, debug, audit |
| **Package** | Distribution | `nika add` | Satellites, skills, workflows |
| **Skills** | System prompts | .md files | SEO expert, coding style |
| **Model slots** | Routing LLM | edison/atlas/pythagoras/york | Assigne models aux roles |
| **Progressive Disclosure** | UX principle | 3 complexity levels | Minimal→Medium→Full |
| **Local-first** | Cost optimization | `fallback:` in satellite def | Native first, cloud backup |

---

## 11. Open Questions

Questions restantes pour les prochaines sessions de brainstorming.

### NovaNet Schema

- **SatelliteCard node class**: Schema exact? Quels properties? Quels arcs?
- **Record promotion**: Auto ou manuel par defaut? Seuil de confidence?
- **Entity-Satellite linking**: Un satellite peut-il "specialiser" sur certaines Entities?

### Package System

- **Registry**: Self-hosted (skills.sh) ou public (npm-like)?
- **Versioning**: Comment gerer les breaking changes dans les satellites?
- **Discovery**: Comment un utilisateur trouve les satellites disponibles?

### Capability Matching

- **Negotiation**: Que se passe-t-il quand aucun satellite ne matche les capabilities demandees?
- **Fallback chain**: Peut-on definir une chaine de fallback de satellites?
- **Capability inference**: Automatique via model metadata ou explicite?

### Native Builtins

- **nika:imagine** (was image_generate): ✅ VALIDATED (Doc 17) — MCP-only Smart Router (ComfyUI/Replicate/FAL)
- **nika:embed**: ✅ VALIDATED (Doc 17) — Tier 1 builtin (nomic-embed via candle)
- **nika:transcribe**: ✅ VALIDATED (Doc 17) — Tier 2 builtin (whisper via candle)
- **nika:speak** (was tts): ✅ VALIDATED (Doc 17) — Tier 2 builtin (text-to-speech)
- **nika:translate**: ✅ VALIDATED (Doc 17) — Tier 2 builtin (NLLB-200, 600MB, offline)
- **nika:vision**: ✅ VALIDATED (Doc 17) — Tier 1 builtin (vision capability)
- **nika:ocr**: ✅ VALIDATED (Doc 17) — Tier 2 builtin (OCR capability)
- **nika:search**: ✅ VALIDATED (Doc 17) — Meta builtin (native RAG with HNSW)
- **nika:index**: ✅ VALIDATED (Doc 17) — Meta builtin (companion to search)

### Verbs

- **New verbs needed?**: Probablement NON — les 5 verbes restent, les builtins etendent les modalites
- **agent: streaming**: Comment streamer les agent turns vers le TUI?

---

## 12. Full Workflow Example

Exemple complet et realiste montrant tout l'ecosysteme ensemble.

```yaml
# ═══════════════════════════════════════════════════════════════════════════════
#  COMPLETE ECOSYSTEM WORKFLOW EXAMPLE
#  File: qrcode-pipeline.nika.yaml
# ═══════════════════════════════════════════════════════════════════════════════

schema: nika/workflow@0.10
goal:

# ───────────────────────────────────────────────────────────────────────────────
#  ORCHESTRATOR CONFIGURATION (PUNK-01 — The Strategist)
# ───────────────────────────────────────────────────────────────────────────────

goal:
  model: anthropic/claude-haiku
  routing: capability-match
  fallback: text-analyst
  max_rounds: 10
  record_budget: 15000

# ───────────────────────────────────────────────────────────────────────────────
#  MCP SERVERS
# ───────────────────────────────────────────────────────────────────────────────

mcp:
  servers:
    novanet:
      command: "novanet-mcp"
    image-gen:
      command: "image-gen-mcp"
      env:
        STABILITY_API_KEY: "${spn:stability}"

# ───────────────────────────────────────────────────────────────────────────────
#  SKILLS (System Prompts)
# ───────────────────────────────────────────────────────────────────────────────

skills:
  - path: pkg:@supernovae/seo-expert@1.0/skill.md
    alias: seo
  - path: ./skills/brand-voice.md
    alias: brand

# ───────────────────────────────────────────────────────────────────────────────
#  SATELLITES (Specialized Workers)
# ───────────────────────────────────────────────────────────────────────────────

satellites:
  # Vision satellite — analyses images
  - id: vision-analyst
    model: openai/gpt-4o
    accepts: [image/png, image/jpeg, image/webp]
    produces: [text/markdown, application/json]
    tools: [nika:read]
    record:
      compress: true
      max_tokens: 500

  # Text satellite — generates content with NovaNet context
  - id: text-analyst
    slot: edison
    model: anthropic/claude-sonnet-4-20250514
    accepts: [text/plain, text/markdown, application/json]
    produces: [text/markdown, text/html]
    tools: [novanet_context, novanet_search]
    skills: [seo, brand]
    record:
      compress: true
      retain: [content, seo_score]

  # Creative satellite — generates images via MCP
  - id: creative-director
    slot: edison
    model: anthropic/claude-sonnet-4-20250514
    accepts: [text/plain, text/markdown]
    produces: [image/png, text/markdown]
    tools: [image-gen:generate_image, image-gen:edit_image]
    record:
      compress: true
      promote: true

  # Local embedder — always native, no cloud
  - id: local-embedder
    model:
      provider: native
      name: nomic-embed-text-v1.5.gguf
    accepts: [text/plain]
    produces: [application/x-embedding]

  # SEO optimizer — search-focused
  - id: seo-optimizer
    slot: york
    model: perplexity/sonar-pro
    accepts: [text/markdown]
    produces: [application/json]
    tools: [novanet_search]
    skills: [seo]

# ───────────────────────────────────────────────────────────────────────────────
#  GOAL (Natural language instruction for the orchestrator)
# ───────────────────────────────────────────────────────────────────────────────

goal: |
  1. Analyse l'image QR code fournie
  2. Recupere le contexte SEO depuis NovaNet pour le domaine qrcode-ai.com
  3. Genere une landing page SEO-optimisee en francais
  4. Cree une hero image correspondant au contenu
  5. Genere les embeddings pour le moteur de recherche

# ───────────────────────────────────────────────────────────────────────────────
#  INPUTS
# ───────────────────────────────────────────────────────────────────────────────

inputs:
  qr_image: ./data/qr-sample.png
  target_locale: fr-FR
  target_entity: qr-code
  output_dir: ./output/

# ───────────────────────────────────────────────────────────────────────────────
#  CONTEXT (Loaded at workflow start)
# ───────────────────────────────────────────────────────────────────────────────

context:
  files:
    brand_guidelines: ./context/brand.md
    seo_config: ./context/seo-config.json
```

### What Happens When This Runs

```
1. nika qrcode-pipeline.nika.yaml
   │
   ├── Parse YAML, validate schema @0.10
   │
   ├── Load context files (brand_guidelines, seo_config)
   │
   ├── Connect to MCP servers (novanet, image-gen)
   │
   ├── Initialize RunContext (DashMap)
   │
   ├── Orchestrator analyzes goal + inputs:
   │   • Input: qr_image (image/png) → needs vision
   │   • Goal mentions: "hero image" → needs image generation
   │   • Goal mentions: "SEO" → needs search/novanet
   │   • Goal mentions: "embeddings" → needs embedding model
   │
   ├── Orchestrator creates execution plan:
   │   1. vision-analyst (qr_image → analysis.json)
   │   2. text-analyst (analysis + novanet context → landing_page.md)
   │   3. creative-director (landing_page → hero.png) [parallel with 4]
   │   4. seo-optimizer (landing_page → seo_report.json) [parallel with 3]
   │   5. local-embedder (landing_page → embeddings.json)
   │
   ├── Execute satellites:
   │   │
   │   ├── vision-analyst runs:
   │   │   • Calls nika:read to load qr_image
   │   │   • GPT-4o analyzes image
   │   │   • Output: { qr_content, visual_analysis, decoded_url }
   │   │   • Record compressed and saved
   │   │
   │   ├── text-analyst runs:
   │   │   • Calls novanet_context(focus_key="qr-code", locale="fr-FR")
   │   │   • Calls novanet_search for SEO keywords
   │   │   • Claude generates landing page with SEO + brand voice
   │   │   • Output: landing_page.md (1200 words)
   │   │   • Record compressed and saved
   │   │
   │   ├── [PARALLEL] creative-director + seo-optimizer:
   │   │   │
   │   │   ├── creative-director:
   │   │   │   • Calls image-gen:generate_image
   │   │   │   • Creates hero image matching landing page theme
   │   │   │   • Output: hero.png
   │   │   │   • Record promoted to NovaNet (promote: true)
   │   │   │
   │   │   └── seo-optimizer:
   │   │       • Analyzes landing page with Perplexity
   │   │       • Generates SEO score + recommendations
   │   │       • Output: seo_report.json
   │   │
   │   └── local-embedder runs:
   │       • Local GGUF model (zero API calls)
   │       • Generates embeddings for search
   │       • Output: embeddings.json
   │
   ├── RunContext contains all results:
   │   • results["vision-analyst"] → JSON
   │   • results["text-analyst"] → markdown
   │   • results["creative-director"] → image path
   │   • results["seo-optimizer"] → JSON
   │   • results["local-embedder"] → embedding vectors
   │
   ├── Punk Records saves run:
   │   • .nika/records/2026-03-15_xyz789.ndjson
   │   • 5 Records with summaries, confidence scores
   │
   └── Output written to ./output/:
       • landing_page.md
       • hero.png
       • seo_report.json
       • embeddings.json
       • metadata.json (workflow trace reference)
```

---

## Summary

Ce document capture l'ecosysteme coherent de Nika v0.30+ :

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                         NIKA v0.30+ ECOSYSTEM SUMMARY                         ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  🎯 ORCHESTRATION                                                             ║
║  └── Orchestrator (historical: Shaka/PUNK-01) route dynamiquement vers les satellites                  ║
║                                                                               ║
║  🛰️ SATELLITES                                                                 ║
║  └── Workers specialises avec model + accepts/produces + tools                ║
║                                                                               ║
║  🔧 TOOLS                                                                      ║
║  └── 20 builtins + MCP tools (100 aliases) comme "mains"                       ║
║                                                                               ║
║  💾 DATA (3-Tier Memory)                                                       ║
║  ├── HOT: RunContext (RAM, un run)                                            ║
║  ├── WARM: Punk Records (NDJSON, configurable TTL)                            ║
║  └── COLD: NovaNet (Neo4j, permanent)                                         ║
║                                                                               ║
║  📦 PACKAGES (5 types)                                                         ║
║  └── skill + workflow + satellite + model + mcp via `nika add`                ║
║                                                                               ║
║  🖥️ ONE CLI                                                                    ║
║  └── `nika` pour TOUT (providers, models, mcp, packages, workflows, TUI)      ║
║                                                                               ║
║  📐 PROGRESSIVE DISCLOSURE                                                     ║
║  └── Minimal (2 champs) → Medium (5) → Full (10) partout                      ║
║                                                                               ║
║  🏠 LOCAL-FIRST                                                                ║
║  └── Native par defaut, cloud en fallback                                     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

**Next Steps:**
1. Valider les decisions PROPOSED avec l'equipe
2. Implementer RunContext rename (doc 14)
3. Designer le schema SatelliteCard pour NovaNet
4. Prototyper orchestrator (v0.29)
5. Definir le package registry (skills.sh ou autre)
