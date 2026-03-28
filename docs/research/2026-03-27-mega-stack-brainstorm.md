# Mega Stack Brainstorm - Nika SuperNovae Infrastructure

**Date**: 2026-03-27
**Session**: VPS setup + GPU strategy + model stack + competitive analysis
**Participants**: Thibaut + Claude (17 research agents deployed)

---

## 1. Infrastructure Live

| Instance | Type | IP | Zone | Role | Cost |
|----------|------|----|------|------|------|
| nika-vps | PLAY2-NANO | 51.15.136.200 | fr-par-1 | Daemon, always-on | ~EUR14/mo |
| Selfhost (Nicolas) | H100-1-80G | 51.159.167.12 | fr-par-2 | vLLM: Qwen3.5-27B-GPTQ-Int4 :8000 | ~EUR1993/mo |
| Generator (Nicolas) | L40S-1-48G | 51.159.159.245 | fr-par-2 | img_samples (8 months) | ~EUR1022/mo |
| nika-h100 (STOPPED) | H100-1-80G | 51.159.153.241 | fr-par-2 | vLLM installed, not configured | EUR2.73/h when on |

### VPS Config
- Nika 0.48.0, 4 providers (Anthropic, OpenAI, Gemini, xAI)
- Daemon systemd + linger, env vars in ~/.nika/.env
- Security group: SSH + vLLM (8000)

---

## 2. Recommended Model Stack (H100 80GB)

### Option A: Single Powerhouse (RECOMMENDED)

```
Qwen3.5-35B-A3B (MoE, BF16)    ~70 GB
  - Only 3B active per token = ultra fast
  - 201 languages, multimodal (vision + text)
  - 262K context, Apache 2.0
  - Handles: orchestration, translation, code, SEO

+ NLLB-200-3.3B (CTranslate2, INT8)  ~4 GB
  - 200 languages including low-resource
  - Encoder-decoder, NOT vLLM compatible
  - Separate FastAPI process
  - Covers: Yoruba, Wolof, Guarani, Quechua, etc.

= 74 GB / 80 GB
```

### Option B: Specialist Stack (alternative)

```
Nemotron-Orchestrator-8B    ~8 GB FP8   orchestration routing
Hermes-4-14B               ~14 GB FP8   tool use + structured output
TranslateGemma-12B         ~12 GB FP8   translation 55 languages
Qwen3-Coder-30B-A3B       ~15 GB FP8   code generation
+ NLLB-200 CTranslate2     ~4 GB INT8   200 languages
= ~53 GB / 80 GB (more headroom, but bandwidth competition)
```

### Provider Strategy by Nika Verb

| Verb | Self-hosted | Cloud primary | Cloud fallback |
|------|-------------|---------------|----------------|
| `infer:` simple | Qwen3.5-35B-A3B | Gemini Flash-Lite EUR0.10 | DeepSeek EUR0.14 |
| `infer:` complex | Qwen3.5-35B-A3B | Claude Sonnet 4 | GPT-4.1 |
| `infer:` + `structured:` | Qwen3.5-35B-A3B | GPT-4.1 (best JSON) | Claude Haiku |
| `agent:` loops | Qwen3.5-35B-A3B | Claude Sonnet 4 | Gemini 2.5 Pro |
| `infer:` translation | NLLB-200 + Qwen | DeepSeek V3.2 | Google Translate |
| `infer:` code | Qwen3.5-35B-A3B | Devstral 2 EUR0.40 | Mistral Large 3 |
| `infer:` SEO rewrite | Qwen3.5-35B-A3B | Gemini Flash-Lite | Claude Haiku |

---

## 3. Translation Pipeline (201 locales from locales.ts)

### Language Tiers

- **Tier 1** (30 locales, ~90% traffic): EN, FR, DE, ES, JA, KO, ZH, PT, IT, AR, etc.
  - LLM direct translation with SEO keyword injection
- **Tier 2** (70 locales, ~9% traffic): TH, VI, UK, RO, BG, HR, SK, etc.
  - NLLB/MT + LLM post-edit on P1 segments (titles, metas)
- **Tier 3** (100 locales, ~1% traffic): low-resource languages
  - NLLB-200 only (page existence + hreflang = SEO value)

### Cost: 1000 pages x 200 locales

| Approach | Cost |
|----------|------|
| All Gemini Flash | ~$45 |
| Hybrid (recommended) | $200-500 |
| NLLB self-hosted + Gemini post-edit | $50-100 + GPU |
| Google Cloud MT | ~$4,800 (100x more expensive!) |

### Key Finding
LLM translation is now CHEAPER than traditional MT APIs AND produces SEO-optimized output.

### CRITICAL: License Issue
- **NLLB-200 = CC-BY-NC 4.0** (non-commercial). Cannot use directly in commercial SaaS without Meta license.
- **MADLAD-400 = Apache 2.0** (safe). 450 languages but lower quality.
- **TranslateGemma = Gemma license** (permissive). Only 55 languages.
- **Qwen3.5 = Apache 2.0** (safe). 201 languages, general-purpose.
- **Recommended**: Start with Gemini Flash ($45 total), migrate to MADLAD-400 self-hosted when volume justifies.
- **NLLB secret weapon**: 9 distinct Arabic dialect codes (Egyptian, Moroccan, etc.) — unique advantage if license cleared.

---

## 4. Competitive Landscape

### Positioning: "Terraform for AI pipelines"

| | OpenClaw | Hermes Agent | Nika |
|-|----------|-------------|------|
| Stars | 338k | 14.5k | - |
| Paradigm | Conversational | Self-improving | Declarative workflows |
| Language | TypeScript | Python | Rust |
| DAG | No | No | Yes |
| Structured output | No | No | 5-layer defense |
| Media pipeline | Basic | Basic | 24 CAS tools |
| Multi-provider | 30+ (Pi agent) | 200+ (OpenRouter) | 7 cloud + native + custom |
| Channels | 24+ platforms | 9 gateways | CLI + TUI (Telegram planned) |

### Nika's Unique Advantages (no competitor has these)
1. Declarative YAML with 5 semantic verbs
2. DAG-validated execution with `nika check --strict`
3. 9 HTTP extraction modes
4. 24 builtin media tools (CAS, C2PA, QR)
5. 31 pipe transforms
6. 5-layer structured output defense
7. Native GGUF inference
8. Multi-phase AST (errors before execution)
9. Rust binary, zero dependencies, 8400+ tests

### Features to Steal

| Feature | Source | Priority |
|---------|--------|----------|
| Telegram webhook trigger | OpenClaw/n8n | P0 |
| Inference routing (fallback chains) | Roadmap Level 3 | P0 |
| Workflow auto-generation (NL -> YAML) | New | P1 |
| Self-improving skills (traces -> templates) | Hermes | P1 |
| MCP Server mode (expose Nika as MCP) | Ecosystem | P1 |
| Persistent memory for `agent:` | Hermes | P2 |
| Streaming API (SSE) | OpenClaw Gateway | P2 |
| Fine-tune Nika-Brain model | Research | P3 |
| Skills registry / marketplace | ClawHub/agentskills.io | P3 |

---

## 5. Fine-Tuning Roadmap

### "Nika-Brain" — Fine-tuned model for workflow generation

**Cost: ~$300-600, 5-6 weeks**

1. **Data generation** (2 weeks): 500 descriptions -> Claude generates .nika.yaml -> 10 variants each -> validate with `nika check` -> ~4000 valid examples. Cost: ~$80-200.
2. **SFT** (1 week): QLoRA rank 64 on Qwen3.5-27B. 1x H100, 4-8 hours.
3. **SimPO alignment** (1 week): Preference pairs auto-generated via `nika check` (valid = chosen, invalid = rejected). 2000 pairs.
4. **Evaluation** (1 week): Custom eval suite on Nika schema compliance.

### Key Insight from Hermes Training
Tool use was only 4.3% of Hermes training data (~17M tokens) yet it's one of the best for function calling. Small, high-quality dataset > large noisy one.

### Complete Synthetic Data Pipeline ($175 API cost)

```
498 existing workflows → 200 curated seeds
  → 500 taxonomy skeletons (GLAN: 5 verbs x N domains x 5 tiers)
  → 3000 raw pairs (Llama 405B / Qwen 72B teacher — legally safe)
  → 1300 validated (nika check 4-stage filter)
  → 5300 Evol-Instruct (4 epochs: constraints → deepen → concretize → complicate)
  → 6500 + reverse generation
  → 9000 + diversity augmentation
  → 8000 final (dedup + quality sweep)
```

### Legal: Teacher Model Selection
- DO NOT use Claude/GPT outputs for training (ToS prohibit distillation)
- USE: Llama 3.1 405B (Meta license OK), Qwen 2.5 72B (Apache 2.0), DeepSeek V3 (MIT)

### Nika's Killer Advantage
`nika check` = automatic reward signal for training data validation.
No human labeling needed. Schema + DAG + template validation = machine-verifiable quality.

### Meta-Opportunity
The synthetic data pipeline itself can be a .nika.yaml workflow:
infer: generate workflows, exec: validate via nika check, for_each: parallelize.

---

## 6. Budget Summary (EUR2000/month)

```
Infrastructure
  VPS nika-vps (PLAY2-NANO)         EUR14/mo
  H100 smart scheduling (220h)      EUR600/mo
  Cloud APIs                        ~EUR400/mo
  ──────────────────────────────────
  Total                             ~EUR1,014/mo
  Remaining                         ~EUR986/mo (margin for scale)
```

---

## 7. Next Steps

### Immediate (this week)
- [ ] Configure H100 with Qwen3.5-35B-A3B + NLLB-200
- [ ] Wire vLLM endpoints into Nika VPS config.toml
- [ ] Test translation pipeline on 10 sample locales

### Short-term (next 2 weeks)
- [ ] Telegram webhook trigger for nika-daemon
- [ ] Inference routing Level 3 (fallback chains)
- [ ] Translation workflow for locales.ts

### Medium-term (next month)
- [ ] Workflow auto-generation (NL -> YAML)
- [ ] Self-improving skills from traces
- [ ] SEO translation pipeline for qrcode-ai.com

### Long-term (next quarter)
- [ ] Fine-tune Nika-Brain model
- [ ] MCP Server mode
- [ ] Skills registry

---

## 8. Hermes Self-Improvement Loop (for Nika adaptation)

### 4 Layers

1. **MEMORY.md + USER.md**: Frozen snapshots loaded at workflow start (2200 + 1375 chars). Never re-read mid-execution. Scanned for injection on every write.
2. **Skills**: SKILL.md (YAML frontmatter + markdown) + optional scripts/templates. Agent can create/edit/delete via tool. Security scan + rollback on failure.
3. **Background Review**: After every Nth turn, a fork agent reviews conversation and decides what to save as memory or skill. Runs AFTER user sees response, best-effort.
4. **Atropos RL**: Execution traces feed into training pipeline for next model generation.

### Adaptation for Nika

- **Post-workflow review**: Optional `infer:` task after workflow completion reviews execution trace and proposes YAML improvements
- **Workflow templates from traces**: Successful complex workflows auto-saved as reusable partials
- **`nika trace search`**: FTS5 across past execution traces
- **agentskills.io compatibility**: Each Nika workflow package includes SKILL.md for agent discovery

---

## 9. OpenClaw Gateway Architecture (for Nika daemon)

### Key Pattern
- Single-process WebSocket server (ws://127.0.0.1:18789)
- Typed JSON-RPC protocol with TypeBox schemas
- State sync: snapshot on connect, incremental events after
- CLI, TUI, WebChat, mobile apps = thin clients to Gateway
- Channel plugins with manifest + SDK boundary

### Adaptation for Nika Daemon
- `nika-daemon` already has IPC + Unix socket + cron + secrets
- Add WebSocket protocol for external triggers (Telegram, Discord, webhook)
- DM pairing security model for inbound triggers
- Channel plugin system for trigger sources
- Daemon IS the control plane, not just a helper service

---

## 10. SEO Translation Pipeline Detail

### Cost Breakthrough
LLM translation (Gemini Flash) = $45 for 1000 pages x 200 locales
Google Cloud MT = $4,800 for same volume
**LLMs are 100x cheaper AND produce SEO-optimized output**

### Tier Strategy for locales.ts (201 locales)
- **Tier 1** (30 locales, ~90% traffic): LLM direct with keyword injection
- **Tier 2** (70 locales, ~9% traffic): NLLB/MT + LLM post-edit on titles/metas
- **Tier 3** (100 locales, ~1% traffic): NLLB-200 only

### Template Decomposition
- 60% of page content is shared template (nav, footer, CTAs)
- Translate shared segments ONCE per locale
- Only translate unique 40% per page
- Reduces total volume by 60%

### Hreflang at Scale
- 201 locales = XML sitemap hreflang (NOT in-page link tags)
- Subdirectory pattern: /fr-FR/, /ja/, /zh-CN/
- Self-referencing canonicals per locale
- Sitemap index with per-locale sitemaps

### Nika Workflow Example
```yaml
schema: "nika/workflow@0.12"
workflow: translate-page
provider: gemini
model: gemini-2.5-flash

inputs:
  page_id: "qr-generator"

tasks:
  - id: load_source
    exec: "cat content/en/{{inputs.page_id}}.json"

  - id: tier1_translate
    depends_on: [load_source]
    with:
      source: $load_source
    for_each: ["fr-FR", "de-DE", "ja-JP", "ko-KR", "zh-CN", "pt-BR", "es-MX"]
    as: locale
    concurrency: 5
    infer:
      prompt: |
        Translate for {{with.locale}}. SEO-optimize titles and meta.
        Source: {{with.source}}
      temperature: 0.3
    structured:
      schema:
        type: object
        properties:
          title: { type: string }
          meta_description: { type: string }
          h1: { type: string }
          body: { type: string }
        required: [title, meta_description, h1, body]

  - id: tier3_nllb
    depends_on: [load_source]
    with:
      source: $load_source
    for_each: ["yo-NG", "wo-SN", "gn-PY", "qu-PE", "ceb-PH"]
    as: locale
    concurrency: 10
    fetch:
      url: "http://localhost:8002/translate"
      method: POST
      json:
        text: "{{with.source}}"
        target: "{{with.locale}}"
      extract: jsonpath
      selector: "$.translation"
```

---

## 11. Research Documents Index

All detailed research saved in this directory:

| File | Topic |
|------|-------|
| `2026-03-27-mega-stack-brainstorm.md` | This file - consolidated synthesis |
| `2026-03-27-ai-orchestrator-landscape.md` | Competitor analysis (Claude Code, Hermes, CrewAI, etc.) |
| `2026-03-27-agent-framework-landscape.md` | GitHub repos deep analysis (7 repos) |
| `2026-03-27-hermes-agent-deep-dive.md` | Hermes self-improvement loop, Atropos RL |
| `2026-03-27-ai-agent-ecosystem-deep-dive.md` | MCP adoption, Telegram bots, vLLM multi-model |
| `fine-tuning-for-workflow-orchestration.md` | Datasets, SimPO, synthetic data, Hermes recipe |
