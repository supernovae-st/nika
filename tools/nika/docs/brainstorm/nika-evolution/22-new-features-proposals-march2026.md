# 22 -- New Features Proposals: March 2026

> Comprehensive feature proposals synthesized from Jungo migration analysis, competitive
> research (5 parallel agents), and cross-referencing with Nika's existing 20-document
> evolution corpus. Includes current state comparison, 10 new proposals, YAGNI decisions,
> and wave mapping.

**Date**: 2026-03-16 | **Nika version**: v0.27.0 | **Schema**: @0.12

---

## Table of Contents

1. [Research Methodology](#1-research-methodology)
2. [Nika v0.27 Current State](#2-nika-v027-current-state)
3. [Jungo Migration Context](#3-jungo-migration-context)
4. [Research Findings Summary](#4-research-findings-summary)
5. [10 New Feature Proposals](#5-10-new-feature-proposals)
6. [YAGNI -- Deliberately Skipped](#6-yagni----deliberately-skipped)
7. [Proposed Wave Mapping](#7-proposed-wave-mapping)
8. [Current vs Proposed Gap Analysis](#8-current-vs-proposed-gap-analysis)
9. [Sources & References](#9-sources--references)

---

## 1. Research Methodology

### 1.1 Agents Deployed

Five parallel research agents were launched on 2026-03-16 to gather current industry data:

| Agent | Focus | Sources | Key Findings |
|-------|-------|---------|--------------|
| **Workflow Trends** | AI workflow engines, agentic frameworks, 2025-2026 trends | Dify, AutoGen, Amp, LangSmith, CrewAI | Dify HITL, Amp Checks, LangSmith memory-as-files |
| **MCP Ecosystem** | MCP servers, registry state, March 2026 | MCP registry, GitHub, npm | 100+ image gen servers, SEO MCP servers, MCP spec v4 |
| **Durable Execution** | Checkpoint/resume, journaling, recovery patterns | Restate, Temporal, CrewAI, Anthropic | Restate 1.5 journaling, context engineering paper |
| **Eval & Observability** | Testing frameworks, LLM observability, evals-as-code | Langfuse, Braintrust, OpenTelemetry, promptfoo | OTel GenAI conventions, trace-based testing |
| **SEO AI Tools** | SEO automation, GEO, DataForSEO | DataForSEO, Ahrefs, fetchSERP | DataForSEO official MCP, GEO emerging |

### 1.2 Cross-Reference Strategy

Each finding was validated against:
- Nika's 20 existing evolution documents (01-20)
- The 6 planned priorities (P-MODEL through P-INTROSPECT)
- Nika's current source code (220K lines, 373 files)
- The 14 Jungo migration proposals from the other brainstorm session

**Filter criterion**: Only features NOT already in the roadmap and aligned with Nika's
DNA (YAML-first, CLI-first, Rust, declarative, MCP-only) were retained.

---

## 2. Nika v0.27 Current State

### 2.1 Scale

| Metric | Value | Verification |
|--------|-------|--------------|
| Lines of Rust code | ~220,000 | `find src -name '*.rs' -exec cat {} + \| wc -l` |
| Source files | 373 | `find src -name '*.rs' \| wc -l` |
| Tests passing | 6,610 | `cargo test -- --list \| grep "test$" \| wc -l` |
| Source modules | 11 | `ls -d src/*/` |
| EventKind variants | 34 | `src/event/log.rs` |
| Provider definitions | 20+ | `src/core/providers.rs` |
| Model definitions | 36 | `src/core/models.rs` |
| MCP aliases | 48+ | `src/core/mcp_aliases.rs` |
| Error codes (NIKA-XXX) | 000-429 | 14 ranges across `src/error.rs` |

### 2.2 The 5 Sacred Verbs

| Verb | Purpose | Shorthand | Full Form |
|------|---------|-----------|-----------|
| `infer:` | One-shot LLM generation | `infer: "prompt"` | `infer: { prompt: ..., provider: ..., model: ... }` |
| `exec:` | Shell command execution | `exec: "command"` | `exec: { run: ..., shell: true, timeout: ... }` |
| `fetch:` | HTTP request | `fetch: { url: ... }` | Methods: GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS |
| `invoke:` | MCP tool call or resource read | `invoke: { tool: ..., mcp: ... }` | Resource read: `invoke: { resource: ..., mcp: ... }` |
| `agent:` | Multi-turn agentic loop | n/a | `agent: { prompt: ..., mcp: [...], max_turns: ... }` |

### 2.3 CLI Commands

#### Core Execution

| Command | What It Does |
|---------|-------------|
| `nika <workflow.nika.yaml>` | Direct execution (shortcut for `nika run`) |
| `nika run <file>` | Headless workflow execution |
| `nika check <file>` | Validate workflow (schema, AST, DAG, bindings) |
| `nika check --strict <file>` | Strict validation (adds MCP connection + invoke param validation) |
| `nika ui` | Launch TUI (4 views: Studio/Runner/Chat/Settings) |
| `nika chat` | TUI shortcut: open directly in Chat view |
| `nika studio` | TUI shortcut: open directly in Studio view |

#### Management

| Command | Purpose |
|---------|---------|
| `nika init` | Initialize project structure |
| `nika new` | Create workflow from template/wizard |
| `nika provider` | API key management (`list`, `set`, `remove`) |
| `nika mcp` | MCP server management |
| `nika model` | Native inference model management (GGUF) |
| `nika pkg` | Package management (workflows/skills/schemas) |
| `nika config` | Configuration management |
| `nika schema` | Schema version management |
| `nika doctor` | System health diagnostics |
| `nika workflow` | Workflow file operations |
| `nika trace` | Execution trace management |
| `nika lsp` | Language Server Protocol (feature-gated) |
| `nika completion` | Shell completion generation |

#### `nika check` in Detail

**Non-strict mode** validates:
1. YAML schema against JSON Schema
2. Three-phase AST parsing (Raw -> Analyzed -> Lowered)
3. DAG structure + cycle detection
4. Data bindings (`with:` blocks) resolution
5. JSON schema file references (`output.schema`, `structured.schema`)
6. Reports: provider, model, task count, edge count, schema count

**Strict mode** (`--strict`) adds:
7. MCP server connection test
8. `invoke:` parameter validation against actual MCP tool schemas
9. Parameter type verification

### 2.4 Builtin Tools (12 nika:* tools)

#### Core (7)

| Tool | Description |
|------|-------------|
| `nika:sleep` | Pause execution for N milliseconds |
| `nika:log` | Emit structured log event to trace |
| `nika:emit` | Emit custom event to NDJSON trace |
| `nika:assert` | Validate condition, fail task on mismatch |
| `nika:prompt` | HITL -- request user input during execution |
| `nika:run` | Execute nested workflow (composition) |
| `nika:complete` | Signal agent loop completion |

#### File Tools (5)

| Tool | Description |
|------|-------------|
| `nika:read` | Read file contents with line numbers |
| `nika:write` | Create or overwrite file |
| `nika:edit` | Modify file (old_string -> new_string replacement) |
| `nika:glob` | Find files by glob pattern |
| `nika:grep` | Search file contents with regex |

### 2.5 Event System (34 EventKind Variants)

Nika emits NDJSON trace events across 11 categories:

| Category | Events | Metadata |
|----------|--------|----------|
| **Workflow** | Started, Completed, Failed, Aborted, Paused, Resumed | workflow_id, duration |
| **Task** | Scheduled, Started, Completed, Failed | task_id, verb |
| **Provider** | ProviderCalled, ProviderResponded | tokens (input/output/cache), cost_usd, ttft_ms, request_id |
| **Context** | ContextAssembled | token_budget, truncation info |
| **MCP** | Invoke, Response, Connected, Error, Retry | tool, server, params |
| **Agent** | Start, Turn, Complete, Spawned | turn_count, thinking_content |
| **Guardrail** | Passed, Failed, Escalation | rule, confidence |
| **Builtin** | Log, Custom | level, message |
| **Artifact** | Written, Failed | path, format |
| **Structured** | Attempt, Success | layer (0-4), retry count |
| **Limits** | LimitReached | limit_type, current_value |

### 2.6 Structured Output (5-Layer Defense)

~99.99% JSON compliance via progressive fallback:

| Layer | Strategy | Provider |
|-------|----------|----------|
| **0** | DynamicSubmitTool injection | Provider-native (tool_choice) |
| **1** | rig Extractor | Rust type system |
| **2** | Extract + Validate | JSON Schema validation |
| **3** | Retry with feedback | max_retries (default: 2) |
| **4** | LLM repair | Separate repair_model call |

Configuration: `enable_extractor`, `enable_tool_injection`, `enable_retry`, `enable_repair`,
`max_retries`, `repair_model`.

### 2.7 Data Binding System

The `with:` block supports:

| Feature | Example | Description |
|---------|---------|-------------|
| Simple path | `forecast: weather.summary` | Reference task output |
| Default values | `temp: weather.data ?? 20` | Fallback on null |
| Lazy bindings | `lazy: true` | Deferred resolution |
| JSONPath | RFC 9535 compliant | Via serde_json_path |
| Template | `{{with.alias}}` | String interpolation |
| Mention | `@task-id` | Chat-style references |
| **27 transforms** | `{{ value \| upper \| trim }}` | Pipe chains |

Available transforms: `upper`, `lower`, `trim`, `capitalize`, `reverse`, `length`,
`default`, `split`, `join`, `replace`, `substring`, `starts_with`, `ends_with`,
`contains`, `to_json`, `from_json`, `base64_encode`, `base64_decode`, `url_encode`,
`url_decode`, `md5`, `sha256`, `truncate`, `pad_left`, `pad_right`, `format`, `count`.

### 2.8 Advanced Capabilities Already Present

| Capability | Status | Details |
|------------|--------|---------|
| **HITL** | Exists (basic) | `nika:prompt` builtin + `DefaultHitlHandler` trait |
| **Checkpointing** | Exists (partial) | `PartialCheckpoint` saves state, needs manual restore |
| **Cost tracking** | Exists (events) | `ProviderResponded` emits `cost_usd`, `tokens.input/output/cache` |
| **Limits** | Exists | `max_turns`, `max_tokens`, `max_cost_usd`, `max_duration_secs` |
| **On-limit actions** | Exists | `complete_partial`, `fail`, `escalate` |
| **Workflow composition** | Exists | `include:` (DAG fusion), `nika:run` (nested), `import:` (reusable) |
| **Extended thinking** | Exists | `extended_thinking`, `thinking_budget` (Claude only) |
| **Completion detection** | Exists | Modes: `explicit`, `natural`, `pattern` (regex) |
| **MCP resource reads** | Exists | `invoke: { resource: "uri", mcp: "server" }` |
| **Package system** | Exists | `nika pkg` + `@workflows/name` references |
| **Conditional branching** | **MISSING** | No `if/then/else`, no `when:` clause |
| **Eval/testing** | **MISSING** | No `nika eval`, no `.eval.yaml` format |
| **Durable execution** | **PARTIAL** | Checkpoint exists but no automatic `nika resume` |
| **Model override per task** | **MISSING** | Only workflow-level or verb-level provider/model |
| **Binary artifacts** | **MISSING** | ArtifactFormat is text-only (Text/Json/Yaml) |
| **OTel export** | **MISSING** | NDJSON only, no OpenTelemetry span export |
| **Budget per task** | **MISSING** | Budget exists at agent/workflow level, not per task |
| **Agent skills** | **MISSING** | No `.skill.yaml` format for reusable agent behaviors |

### 2.9 Provider Landscape

**Cloud providers (via rig-core v0.32):**

| Provider | Models Available | Notes |
|----------|-----------------|-------|
| **Anthropic** (claude) | Claude Sonnet 4, Opus 4, Haiku 3.5 | Extended thinking support |
| **OpenAI** (openai) | GPT-4.1, o3, o4-mini | Structured output native |
| **Mistral** (mistral) | Mistral Large 2, Small 3, Codestral | EU provider |
| **Groq** (groq) | Llama 3.3, Mixtral | Speed-optimized |
| **DeepSeek** (deepseek) | DeepSeek-V3, R1 | Cost-optimized |

**Native inference (via mistral.rs):**
- 15 curated GGUF models (Qwen3, Llama3.x, Phi-4, Mistral, Gemma2)
- Auto-quantization selection based on available RAM
- Types: Text, Vision, Embedding, Audio, Diffusion

### 2.10 AST Pipeline

Always three phases, never skip:

```
Raw YAML -> RawWorkflow (schema validation)
         -> AnalyzedWorkflow (binding resolution, DAG validation, MCP alias resolution)
         -> LoweredWorkflow (template expansion, provider resolution, ready for execution)
```

### 2.11 9-Point Competitive Moat

1. **NovaNet knowledge graph** -- 59 NodeClasses, 159 ArcClasses, entity-linked memory
2. **YAML-first declarative** -- Not Python, not notebooks, not visual builders
3. **Knowledge atoms** -- 200+ locales, multi-language content at entity level
4. **5-layer structured output** -- 99.99% JSON compliance
5. **34+ event observability** -- Complete execution trace in NDJSON
6. **7 cloud + 1 native provider** -- Multi-provider with auto-detect
7. **Rust performance** -- Single binary, no runtime, ~10x faster than Python frameworks
8. **Security hardening** -- Shell-free exec by default, MCP-only NovaNet access
9. **NDJSON reproducibility** -- Full trace replay, cost attribution, debugging

---

## 3. Jungo Migration Context

### 3.1 What Is Jungo?

Jungo is the QR Code AI production system -- 8 TypeScript agents that handle SEO content
generation, site auditing, knowledge extraction, and multilingual translation. The Jungo
migration project converted these 8 TS agents into 8 Nika YAML workflows.

### 3.2 Migration Results

| Metric | TypeScript (Before) | Nika YAML (After) | Reduction |
|--------|--------------------|--------------------|-----------|
| Total lines | 7,200 | 2,806 | **-61%** |
| Number of agents | 8 | 8 | Same |
| Dependencies | node_modules | Single binary | Eliminated |
| Type safety | Runtime (TS) | Schema + AST validation | Stronger |

### 3.3 The 8 Workflows

| # | Workflow | Purpose | Key Verbs Used |
|---|----------|---------|----------------|
| 1 | `translator` | Multi-locale content translation | `infer:`, `invoke:` |
| 2 | `knowledge-extractor` | Extract structured knowledge from text | `infer:`, `invoke:` |
| 3 | `business-description` | Generate SEO business descriptions | `infer:`, `fetch:` |
| 4 | `project-explorer` | Analyze project structure and context | `exec:`, `infer:` |
| 5 | `term-extractor` | Extract technical terms and glossary | `infer:`, `invoke:` |
| 6 | `site-auditor` | Full site SEO audit | `fetch:`, `infer:`, `invoke:` |
| 7 | `page-auditor` | Single page SEO audit | `fetch:`, `infer:` |
| 8 | `email-auditor` | Email content quality check | `fetch:`, `infer:` |

### 3.4 Pain Points Identified During Migration

1. **Image generation gap** -- Jungo TS agents generated images; Nika has no binary artifact support
2. **Conditional logic** -- TS had `if/else`; Nika workflows are linear DAGs
3. **Per-task model selection** -- Different tasks need different models (fast vs smart)
4. **Testing AI outputs** -- No way to regression-test workflow outputs
5. **Cost visibility** -- Events have cost_usd but no budget enforcement per task
6. **Workflow reuse** -- Complex workflows have repeated patterns that could be shared
7. **Production monitoring** -- NDJSON is great for debugging, hard to feed into Grafana/DataDog

### 3.5 14 Original Proposals from Jungo Brainstorm

The other Claude Code session proposed these improvements. Checked against existing roadmap:

| # | Proposal | Status | Notes |
|---|----------|--------|-------|
| 1 | Binary artifact format | **NEW** -- Not in roadmap | ArtifactFormat needs Binary/Base64 |
| 2 | Image generation via MCP | Partially covered (doc 18) | MCP servers exist, need Nika-side support |
| 3 | Multi-provider model routing | **PLANNED** (P-MODEL) | 4-slot system in Wave 1 |
| 4 | Cost tracking dashboard | **PARTIAL** -- Events exist | Need aggregation + budget enforcement |
| 5 | Conditional branching | **NEW** -- Not in roadmap | `when:` clause proposed |
| 6 | Workflow composition | **EXISTS** -- `include:` + `nika:run` | Could be improved with `for_each` |
| 7 | Agent skills | **NEW** -- Not in roadmap | `.skill.yaml` for reusable behaviors |
| 8 | HITL approval gates | **PARTIAL** -- `nika:prompt` exists | Need structured `gate:` primitive |
| 9 | Durable execution | **PARTIAL** -- Checkpoint exists | Need `nika resume <run-id>` |
| 10 | Eval framework | **NEW** -- Not in roadmap | `nika eval` + `.eval.yaml` |
| 11 | OTel export | **NEW** -- Not in roadmap | Feature-gated OpenTelemetry |
| 12 | A2A protocol | YAGNI | Spec too early, MCP sufficient |
| 13 | Visual builder | YAGNI | Dify's lane, not ours |
| 14 | Marketplace | YAGNI | `nika pkg` is enough for now |

---

## 4. Research Findings Summary

### 4.1 AI Workflow Landscape (Agent: Workflow Trends)

**Dify (Feb 2026)** shipped two major features:
- **Human Input Node**: Pause workflow, present form to user, resume on response.
  Uses Celery workers for pause/resume. Action-based routing (approve/reject/escalate).
- **Skill Editor + Agent Mode**: Sandboxed code execution within workflows. `@tool` syntax
  for referencing skills. Skill = atomic, reusable, testable unit.

**Amp (Feb 2026)** published "The Coding Agent Is Dead":
- Killed VS Code extension entirely. CLI-first only.
- Introduced **Checks system**: `.agents/checks/` directory with user-defined invariants.
  Each check is a script that runs before completion. Agent must satisfy all checks.
- Skills replace commands. Agent discovers and uses skills autonomously.

**LangSmith Agent Builder** introduced memory-as-virtual-filesystem:
- Based on COALA paper (Cognitive Architectures for Language Agents).
- Procedural memory = `AGENTS.md` file (instructions).
- Semantic memory = skill files.
- Episodic memory = execution traces.
- Memory stored as files, not database rows. Agent reads/writes like a filesystem.

**AutoGen** integrated MCP natively:
- `McpWorkbench` adapter wraps any MCP server as AutoGen tools.
- Agents can use MCP tools alongside native tools seamlessly.

### 4.2 MCP Ecosystem (Agent: MCP Ecosystem)

**MCP spec** reached v4 (2025-11-25):
- Official registry with hundreds of servers
- SDKs in 10 languages
- Native integration in Claude Code, VS Code, Cursor, Windsurf

**Image generation servers** (15+ found):

| Server | Backend | Key Feature |
|--------|---------|-------------|
| Fal.ai MCP | FLUX, SD, MusicGen | Multi-model, fast inference |
| Replicate MCP | Any Replicate model | Largest model catalog |
| OpenAI GPT Image | DALL-E, GPT Image | Native OpenAI integration |
| Pixelle MCP | ComfyUI | Omnimodal (text/image/video/audio) |
| Cloudinary MCP | Cloudinary | Full media pipeline (upload/transform/deliver) |
| WaveSpeed MCP | WaveSpeed AI | Image + video generation |

**SEO MCP servers** (5+ found):

| Server | Data Source | Tools |
|--------|------------|-------|
| DataForSEO MCP | DataForSEO API | SERP, keywords, backlinks, rank tracking |
| fetchSERP MCP | fetchSERP API | All-in-one SEO toolkit |
| Keywords Everywhere MCP | KE API | Keyword research, trends |
| kwrds.ai MCP | kwrds.ai | Keywords, People Also Ask, SERP |
| SEO MCP | Ahrefs (free) | Backlinks, keyword ideas |

**Implication for Nika**: Image generation and SEO are Nika's two primary use cases
(QR Code AI). Both domains have rich MCP server ecosystems. Nika just needs to support
binary artifact output -- the MCP servers handle the actual generation.

### 4.3 Durable Execution (Agent: Durable Execution)

**Restate 1.5** (Oct 2025) introduced the "durable execution for AI" pattern:
- **Journal-based recovery**: Every side effect (LLM call, tool invocation) is recorded.
  On crash, replay the journal to restore exact state.
- **Virtual objects**: Named entities with exclusive-writer semantics. An "agent session"
  is a virtual object -- only one executor at a time, state persists across crashes.
- **Workflow keys**: Idempotency keys for LLM calls. Same key = same cached response.
  Prevents duplicate API charges on retry.
- **Deterministic replay**: Re-execute workflow from journal without making real API calls.

**Anthropic's context engineering paper** (2025):
- **Compaction**: When context window fills, compress older messages while preserving
  key information. Not truncation -- intelligent summarization.
- **Structured note-taking**: Agent maintains structured state (JSON/YAML) alongside
  conversation. State is more compact than conversation history.
- **Sub-agent architectures**: Delegate sub-tasks to fresh agents with focused context.
  Parent maintains summary, not full child conversation.

**CrewAI persistence**:
- SQLite-backed agent memory across sessions.
- Three memory types: short-term (conversation), long-term (patterns), entity (facts).
- Automatic memory consolidation between runs.

**Implication for Nika**: Nika already has NDJSON events that record every side effect.
The journal IS the event trace. Adding `nika resume <run-id>` is straightforward:
replay events up to the failure point, skip completed tasks, resume from the failed one.

### 4.4 Eval & Observability (Agent: Eval & Observability)

**OpenTelemetry GenAI semantic conventions** (stable 2025):
- Standard attributes: `gen_ai.system`, `gen_ai.request.model`, `gen_ai.usage.input_tokens`,
  `gen_ai.usage.output_tokens`, `gen_ai.response.finish_reason`.
- Adopted by Langfuse, Arize Phoenix, Traceloop OpenLLMetry.
- Nika's 34 event types map cleanly to OTel spans.

**LLM observability landscape**:

| Tool | Stars | Model | Key Feature |
|------|-------|-------|-------------|
| **Langfuse** | 19k+ | Open-source | Traces, scores, datasets, prompt management |
| **Arize Phoenix** | 15k+ | Open-source | AI observability, evals, tracing |
| **LangSmith** | n/a | SaaS | Agent builder, memory, evals |
| **Helicone** | 4k+ | Open-source | Gateway proxy, cost tracking, caching |
| **Portkey** | 6k+ | Open-source | AI gateway, guardrails, observability |
| **LiteLLM** | 18k+ | Open-source | Proxy for 100+ providers, cost tracking |

**Evals-as-code patterns** (industry standard by 2026):

```yaml
# promptfoo style
prompts:
  - "Summarize: {{text}}"
providers:
  - openai:gpt-4o
  - anthropic:claude-sonnet-4-20250514
tests:
  - vars: { text: "Long article..." }
    assert:
      - type: llm-rubric
        value: "Summary captures main points"
      - type: javascript
        value: "output.length < 500"
```

**Agent testing patterns**:
- **Trace-based testing**: Assert on execution traces, not just final output.
- **LLM-as-judge**: Use a strong model to evaluate a weaker model's output.
- **Metamorphic testing**: Same input with paraphrasing should produce equivalent output.
- **Deterministic graders**: Regex, JSON schema, code execution checks.
- **Braintrust experiments**: Compare model A vs model B on same dataset with scores.

**Implication for Nika**: The eval ecosystem has converged on YAML test suites with
mixed graders (deterministic + LLM-as-judge). This maps perfectly to `.eval.yaml`
files alongside `.nika.yaml` workflows. Nika's structured output already validates
JSON schema -- extend the same pattern to output quality.

### 4.5 SEO & GEO (Agent: SEO AI Tools)

**GEO (Generative Engine Optimization)** is emerging:
- Optimizing content for AI answer citations, not just SERP rankings.
- Requires: structured data, authoritative sourcing, concise answers, entity clarity.
- NovaNet's knowledge graph is a natural fit for GEO (entities + relationships + citations).

**DataForSEO** launched an official MCP server:
- Full API access via MCP: SERP, keywords, backlinks, rank tracking, on-page analysis.
- 60+ tools available as MCP calls.
- Natural fit for `invoke:` verb in Nika workflows.

**Multi-language SEO automation** patterns:
- Keyword research in source language -> translate -> localize -> generate content.
- NovaNet entities have locale-specific content (200+ locales).
- Jungo's `translator` workflow already handles this partially.

**Implication for Nika**: SEO workflows are Nika's bread and butter (QR Code AI).
The MCP server ecosystem covers all tooling needs. Nika's role is orchestration:
chain `invoke:` (SEO data) -> `infer:` (content generation) -> `invoke:` (NovaNet write).

---

## 5. 10 New Feature Proposals

### Legend

- **Effort**: S (small, ~1-3 days), M (medium, ~1-2 weeks), L (large, ~2-4 weeks)
- **Impact**: How much this moves the competitive needle
- **Dependencies**: What must exist first

---

### TIER S -- Game Changers

---

### S1: `nika eval` -- Evals-as-YAML

**The gap**: Nika can run workflows but cannot systematically test their outputs.
No regression testing for AI quality. No way to compare model A vs model B on the
same inputs. When you change a prompt, you can't know if quality improved or degraded.

**What exists today**: `nika check` validates structure (schema, DAG, bindings).
It does NOT validate output quality. No `nika eval`, no `.eval.yaml` format.

**The proposal**: A new `.eval.yaml` file format and `nika eval` command:

```yaml
# seo-description.eval.yaml
nika: "@0.12"

eval:
  workflow: ./seo-description.nika.yaml
  runs: 3                          # Statistical significance
  parallel: true                   # Run cases concurrently

cases:
  - name: "french-bakery"
    with:
      business_name: "Boulangerie Petit"
      locale: "fr-FR"
      sector: "food"
    assert:
      - type: schema                # Deterministic: JSON schema validation
        schema: ./schemas/seo-description.json
      - type: contains              # Deterministic: must contain keyword
        value: "boulangerie"
      - type: max_length            # Deterministic: SEO limit
        value: 160
      - type: llm-judge             # LLM-as-judge: quality assessment
        prompt: |
          Rate this SEO description for a French bakery.
          Must be: compelling, include location, under 160 chars.
        model: claude-sonnet-4-6
        threshold: 0.8              # Minimum score (0-1)
      - type: no_hallucination      # LLM-as-judge: factual grounding
        context: "{{with.business_name}} in {{with.locale}}"

  - name: "japanese-restaurant"
    with:
      business_name: "Sakura Sushi"
      locale: "ja-JP"
      sector: "restaurant"
    assert:
      - type: contains
        value: "寿司"
      - type: llm-judge
        prompt: "Is this natural Japanese? No awkward machine translation?"
        model: claude-sonnet-4-6
        threshold: 0.9

report:
  format: table                     # table | json | markdown
  compare_models: true              # Side-by-side if multiple providers
```

**CLI usage**:

```bash
nika eval seo-description.eval.yaml              # Run all cases
nika eval seo-description.eval.yaml --case french-bakery  # Single case
nika eval seo-description.eval.yaml --compare claude,openai  # Model comparison
nika eval --ci --threshold 0.8                    # CI mode: exit 1 if below threshold
```

**Assert types** (planned):

| Type | Category | Description |
|------|----------|-------------|
| `schema` | Deterministic | JSON Schema validation |
| `contains` | Deterministic | Output contains string |
| `not_contains` | Deterministic | Output does not contain string |
| `regex` | Deterministic | Output matches regex pattern |
| `max_length` | Deterministic | Character count limit |
| `min_length` | Deterministic | Minimum character count |
| `json_path` | Deterministic | JSONPath expression returns truthy |
| `llm-judge` | LLM-as-judge | Model evaluates with prompt + threshold |
| `no_hallucination` | LLM-as-judge | Factual grounding against context |
| `similarity` | Embedding | Semantic similarity to reference (cosine) |
| `custom` | Code | User-defined assertion script |

**Integration with existing features**:
- Reuses `nika check` validation (schema, AST, DAG) before running eval
- Reuses NDJSON events for cost attribution per eval case
- Reuses `with:` binding system for test case variables
- Reuses structured output validation for JSON schema checks
- Eval reports integrate with `nika trace` for drill-down

**Effort**: M | **Impact**: Very High | **Dependencies**: None (can build on v0.27)

---

### S2: Human-in-the-Loop Gate Primitive

**The gap**: `nika:prompt` requests free-form text input. There's no structured
approval gate where a human reviews an AI output and approves/rejects/modifies
before the workflow continues. Dify shipped this in Feb 2026.

**What exists today**: `nika:prompt` builtin is a basic text input during agent loops.
`DefaultHitlHandler` trait exists. `HitlRequest`/`HitlResponse` are typed. But there's
no structured gate in the DAG with approve/reject routing.

**The proposal**: A `gate:` primitive at the task level:

```yaml
# content-approval.nika.yaml
nika: "@0.13"

tasks:
  draft:
    infer:
      prompt: "Write a blog post about {{with.topic}}"
      model: claude-sonnet-4-6

  human-review:
    gate:
      message: "Review the blog post draft"
      show: "{{with.draft}}"          # What to display to the reviewer
      actions:
        approve: "Looks good, publish"
        reject: "Rewrite needed"
        edit: "I'll modify it"        # Returns edited version
      timeout: 24h                    # Max wait time
      escalate_to: "manager@co.com"   # Notify if timeout
      metadata:
        reviewer_role: "editor"
    with:
      draft: draft

  publish:
    invoke:
      tool: novanet_write
      mcp: novanet
      params:
        content: "{{with.approved}}"
    when: "human-review.action == 'approve'"  # Conditional (see B3)
    with:
      approved: human-review.result

  rewrite:
    infer:
      prompt: "Rewrite based on feedback: {{with.feedback}}"
    when: "human-review.action == 'reject'"
    with:
      feedback: human-review.result
```

**Gate response schema**:

```json
{
  "action": "approve" | "reject" | "edit" | "escalate",
  "result": "string (edited content or feedback)",
  "reviewer": "user identifier",
  "timestamp": "ISO 8601",
  "duration_ms": 12345
}
```

**Implementation in TUI**: The Runner view shows a gate as a highlighted task.
Chat view allows inline review. Gate events: `GatePresented`, `GateResolved`.

**Implementation in CLI (headless)**: `nika run` pauses, writes checkpoint,
prints gate request to stdout. Resume with `nika resume <run-id> --approve`
or `nika resume <run-id> --reject "feedback here"`.

**Effort**: L | **Impact**: Very High | **Dependencies**: B3 (conditional branching)

---

### S3: Durable Execution with Journal Recovery

**The gap**: Long-running workflows (multi-hour SEO audits, batch translations)
can fail mid-execution. Currently, you restart from scratch. Checkpoint exists
(`PartialCheckpoint`) but there's no `nika resume` command.

**What exists today**:
- NDJSON event trace records every side effect (LLM call, MCP invoke, etc.)
- `PartialCheckpoint` struct saves conversation history, context, progress
- `save_progress: true` on limit config creates checkpoints
- But: no `nika resume` CLI command, no automatic replay, no journal-based recovery

**The proposal**: Full durable execution using NDJSON events as the journal:

```bash
# Normal run
nika run audit.nika.yaml
# → Runs tasks A, B, C, D
# → C fails (API timeout)
# → Event trace: A(completed), B(completed), C(failed), D(not_started)
# → Trace saved: ~/.nika/traces/run-abc123.ndjson

# Resume from failure point
nika resume run-abc123
# → Reads trace, skips A and B (already completed)
# → Retries C from scratch
# → Continues to D

# Resume with modified params
nika resume run-abc123 --set "tasks.C.fetch.timeout=60s"
```

**How it works**:

1. **On normal run**: Every `TaskCompleted` event in the NDJSON trace includes the
   full task output. This is already the case in v0.27.

2. **On resume**: The executor reads the trace file. For each task marked `Completed`,
   it injects the cached output into the DAG context instead of re-executing.
   For the first `Failed` or `NotStarted` task, it resumes normal execution.

3. **Idempotency keys**: Each task execution gets a deterministic key from
   `workflow_id + task_id + input_hash`. On resume, if the inputs haven't changed,
   the cached result is reused. If inputs changed (e.g., upstream task was re-run),
   the task is re-executed.

4. **Agent loops**: For `agent:` verb, checkpoints save after each turn.
   Resume restores conversation history and continues from the last turn.

**New NDJSON events**:

| Event | When | Payload |
|-------|------|---------|
| `RunResumed` | On `nika resume` | `{ run_id, original_run_id, skip_count }` |
| `TaskSkipped` | When replaying cached result | `{ task_id, reason: "cached" }` |
| `TaskRetried` | When re-executing failed task | `{ task_id, attempt, previous_error }` |

**New CLI commands**:

```bash
nika resume <run-id>                       # Resume from failure
nika resume <run-id> --from <task-id>      # Resume from specific task
nika resume <run-id> --set "key=value"     # Override params
nika trace list                            # List all traces (already exists)
nika trace show <run-id>                   # Show trace details
nika trace replay <run-id> --dry-run       # Simulate resume without executing
```

**Cost savings**: For a 50-task workflow where task #45 fails, resume saves ~90%
of the cost by reusing cached LLM responses for tasks 1-44.

**Effort**: L | **Impact**: Very High | **Dependencies**: None (NDJSON already exists)

---

### TIER A -- Strong Additions

---

### A1: Cost Tracking & Budget Limits

**The gap**: `ProviderResponded` events already emit `cost_usd` and token counts.
But there's no per-task budget enforcement, no workflow-level cost ceiling
(beyond agent limits), and no cost aggregation in the CLI output.

**What exists today**:
- `ProviderResponded` event: `tokens.input`, `tokens.output`, `tokens.cache`, `cost_usd`
- Agent-level limits: `max_tokens`, `max_cost_usd`, `max_duration_secs`
- `LimitReached` event when limits are hit
- No per-task budget, no workflow-level cost summary, no historical cost tracking

**The proposal**: `budget:` field at workflow and task level:

```yaml
# budget-aware-workflow.nika.yaml
nika: "@0.12"

budget:
  max_cost_usd: 5.00                  # Workflow-level ceiling
  warn_at_usd: 3.00                   # Emit warning event at this threshold
  on_exceeded: fail                    # fail | warn | complete_partial

tasks:
  expensive-analysis:
    infer:
      prompt: "Deep analysis of {{with.data}}"
      model: claude-sonnet-4-6
    budget:
      max_cost_usd: 2.00              # Per-task ceiling
      max_tokens: 50000               # Per-task token limit

  cheap-summary:
    infer:
      prompt: "Summarize in one paragraph: {{with.analysis}}"
      model: claude-haiku-3-5
    budget:
      max_cost_usd: 0.10
    with:
      analysis: expensive-analysis
```

**CLI output enhancement**:

```
$ nika run workflow.nika.yaml

  ✓ task-1 (claude-sonnet-4-6)    $0.42  │ 12,340 tokens
  ✓ task-2 (claude-haiku-3-5)     $0.01  │    890 tokens
  ✓ task-3 (claude-sonnet-4-6)    $0.38  │ 10,200 tokens
  ─────────────────────────────────────────
  Total: $0.81 / $5.00 budget  │ 23,430 tokens │ 3.2s
```

**Historical tracking**:

```bash
nika trace costs                          # Cost summary across runs
nika trace costs --last 7d                # Last 7 days
nika trace costs --by workflow            # Group by workflow
nika trace costs --by provider            # Group by provider
nika trace costs --by model               # Group by model
```

**Effort**: S | **Impact**: High | **Dependencies**: None (events already emit cost)

---

### A2: Workflow Composition (DAG-of-DAGs)

**The gap**: `include:` merges a sub-workflow's DAG into the parent (fusion).
`nika:run` executes a nested workflow as a builtin tool call within an agent loop.
But there's no way to invoke a workflow as a task in the DAG with typed inputs/outputs,
or to run a workflow across a collection with `for_each`.

**What exists today**:
- `include:` -- DAG fusion (flat merge, loses encapsulation)
- `nika:run` -- Nested workflow execution within agent tool calls
- `import:` -- Reusable definitions (agents, skills)
- Package system (`nika pkg`) for sharing workflows

**The proposal**: Enhanced `invoke:` with `workflow:` target and `for_each` support:

```yaml
# batch-translate.nika.yaml
nika: "@0.12"

tasks:
  get-entities:
    invoke:
      tool: novanet_query
      mcp: novanet
      params:
        query: "MATCH (e:Entity) RETURN e.slug LIMIT 100"

  translate-all:
    invoke:
      workflow: ./translate-one.nika.yaml
      for_each: "{{with.entities}}"       # Iterate over collection
      with_item: entity                    # Bind current item as {{with.entity}}
      concurrency: 5                      # Max parallel executions
    with:
      entities: get-entities.results

  # Or as a simple workflow call (no for_each):
  generate-report:
    invoke:
      workflow: ./generate-report.nika.yaml
      params:
        entities: "{{with.translated}}"
        format: "pdf"
    with:
      translated: translate-all.results
```

**The child workflow** (`translate-one.nika.yaml`) declares its interface:

```yaml
# translate-one.nika.yaml
nika: "@0.12"

# Typed interface (optional, validates at check time)
interface:
  inputs:
    entity:
      type: string
      required: true
  outputs:
    translation:
      type: object
      schema: ./schemas/translation.json

tasks:
  fetch-content:
    invoke:
      tool: novanet_read
      mcp: novanet
      params:
        slug: "{{with.entity}}"

  translate:
    infer:
      prompt: "Translate to French: {{with.content}}"
    with:
      content: fetch-content.result
```

**`for_each` semantics**:
- Input: array or object (iterates values)
- Each iteration runs the child workflow as an isolated DAG
- Results collected as array in same order
- `concurrency:` controls parallelism (default: 1 = sequential)
- Failure modes: `fail_fast` (default), `continue` (collect errors), `retry`
- Individual iteration results accessible via `translate-all.results[0]`, etc.

**Effort**: M | **Impact**: High | **Dependencies**: None

---

### A3: Agent Skills

**The gap**: Agent loops (`agent:` verb) define behavior inline. Common patterns
(web research, code review, SEO analysis) get copy-pasted across workflows.
There's no way to define reusable agent behaviors that can be loaded and combined.

**What exists today**:
- `import:` for reusable definitions
- Package system for sharing workflows
- Agent `scope:` configuration
- Builtin tools (12 nika:* tools)
- No `.skill.yaml` format, no skill composition

**The proposal**: `.skill.yaml` files that define reusable agent capabilities:

```yaml
# skills/web-researcher.skill.yaml
nika: "@0.12"

skill:
  name: web-researcher
  description: "Research a topic by searching the web and synthesizing findings"
  version: "1.0.0"

  # What the agent can do
  tools:
    - nika:read
    - nika:write
    - nika:grep
  mcp:
    - firecrawl                        # Web scraping
    - perplexity                       # Web search

  # System prompt fragment injected into agent
  instructions: |
    You are a web researcher. When asked to research a topic:
    1. Search with Perplexity for recent sources
    2. Scrape the top 3 results with Firecrawl
    3. Synthesize findings into a structured report
    4. Save the report using nika:write

  # Completion criteria
  completion:
    mode: explicit                     # Must call nika:complete
    required_fields:
      - report_path
      - source_count
```

**Usage in workflows**:

```yaml
# research-workflow.nika.yaml
nika: "@0.12"

import:
  skills:
    - ./skills/web-researcher.skill.yaml
    - ./skills/seo-auditor.skill.yaml

tasks:
  research:
    agent:
      prompt: "Research the latest trends in {{with.topic}}"
      skills: [web-researcher]          # Load skill capabilities
      model: claude-sonnet-4-6
      max_turns: 15

  audit:
    agent:
      prompt: "Audit SEO for {{with.url}}"
      skills: [seo-auditor]
      model: claude-sonnet-4-6
      max_turns: 20
    with:
      url: research.result.url
```

**Skill composition**: An agent can load multiple skills. Their tools and MCP
servers are merged. Instructions are concatenated (in order) into the system prompt.

**Package distribution**: Skills are publishable via `nika pkg`:
```bash
nika pkg publish ./skills/web-researcher.skill.yaml
nika pkg install @supernovae/web-researcher
```

**Effort**: M | **Impact**: High | **Dependencies**: None

---

### A4: OpenTelemetry Export

**The gap**: Nika emits 34 event types as NDJSON traces. These are excellent
for debugging but cannot be fed into production monitoring systems (Grafana,
DataDog, Honeycomb, Jaeger) without custom parsing. The industry has converged
on OpenTelemetry GenAI semantic conventions.

**What exists today**:
- 34 NDJSON event types with rich metadata
- `nika trace` command for viewing traces
- No OTel export, no OTLP endpoint configuration
- `ProviderResponded` already has the fields that map to OTel GenAI attributes

**The proposal**: Feature-gated OpenTelemetry span export:

```toml
# Cargo.toml (feature gate)
[features]
otel = ["opentelemetry", "opentelemetry-otlp", "opentelemetry-semantic-conventions"]
```

```yaml
# .nika/config.yaml
telemetry:
  otel:
    enabled: true
    endpoint: "http://localhost:4317"    # OTLP gRPC endpoint
    service_name: "nika"
    environment: "production"
    sampling_rate: 1.0                  # 0.0-1.0
    export_traces: true
    export_metrics: true
    attributes:                         # Custom resource attributes
      team: "seo-engineering"
      app: "qrcode-ai"
```

**Event-to-span mapping**:

| Nika Event | OTel Span | Attributes |
|------------|-----------|------------|
| `WorkflowStarted/Completed` | Root span | `workflow.name`, `workflow.schema` |
| `TaskStarted/Completed` | Child span | `task.id`, `task.verb`, `task.duration_ms` |
| `ProviderCalled/Responded` | LLM span | `gen_ai.system`, `gen_ai.request.model`, `gen_ai.usage.*` |
| `McpInvoke/McpResponse` | Tool span | `mcp.server`, `mcp.tool`, `mcp.duration_ms` |
| `AgentStart/Turn/Complete` | Agent span | `agent.turn_count`, `agent.token_usage` |
| `GatePresented/Resolved` | Gate span | `gate.action`, `gate.reviewer`, `gate.duration_ms` |

**OTel metrics emitted**:

| Metric | Type | Unit |
|--------|------|------|
| `nika.workflow.duration` | Histogram | ms |
| `nika.task.duration` | Histogram | ms |
| `nika.llm.tokens.input` | Counter | tokens |
| `nika.llm.tokens.output` | Counter | tokens |
| `nika.llm.cost` | Counter | USD |
| `nika.mcp.calls` | Counter | calls |
| `nika.agent.turns` | Histogram | turns |

**Effort**: M | **Impact**: Medium-High | **Dependencies**: None

---

### TIER B -- Quick Wins

---

### B1: Per-Task Model Override

**The gap**: Model selection happens at workflow level (`provider:`, `model:`) or
verb level (`infer: { model: ... }`). But there's no clean way to say "this task
uses a fast model, that task uses a smart model" at the task level. The planned
P-MODEL 4-slot system (edison/atlas/york/pythagoras) is the full solution, but
a simple per-task `model:` field is a quick win for v0.28.

**What exists today**:
- Workflow-level: `provider: claude`, `model: claude-sonnet-4-6`
- Verb-level: `infer: { provider: openai, model: gpt-4.1 }`
- Agent-level: `agent: { provider: ..., model: ... }`
- No task-level override that applies to any verb

**The proposal**: `model:` and `provider:` at task level:

```yaml
tasks:
  fast-classification:
    model: claude-haiku-3-5              # Quick, cheap
    infer:
      prompt: "Classify this text: {{with.text}}"

  deep-analysis:
    model: claude-sonnet-4-6             # Smart, thorough
    infer:
      prompt: "Deep analysis of: {{with.text}}"

  reasoning:
    model: o3                            # Reasoning model
    provider: openai
    infer:
      prompt: "Solve this logic puzzle: {{with.puzzle}}"
```

**Resolution order** (specific overrides general):
1. Verb-level (`infer: { model: ... }`) -- highest priority
2. Task-level (`model:`) -- new
3. Workflow-level (`model:`) -- existing
4. Provider auto-detect (`RigProvider::auto()`) -- default

**This is a stepping stone to P-MODEL**: When the 4-slot system arrives in v0.28,
per-task `model:` becomes syntactic sugar for slot selection:

```yaml
# v0.28 with P-MODEL
tasks:
  fast-task:
    model: atlas         # Maps to atlas slot (fast/tactical)
  smart-task:
    model: edison        # Maps to edison slot (main/quality)
```

**Effort**: S | **Impact**: Medium | **Dependencies**: None

---

### B2: Binary Artifacts

**The gap**: `ArtifactFormat` enum is text-only: `Text`, `Json`, `Yaml`.
Image generation via MCP servers returns binary data (base64-encoded images).
Nika cannot store or pass binary data between tasks.

**What exists today**:
- `ArtifactFormat`: `Text`, `Json`, `Yaml` only
- `ArtifactWritten` event with `path` and `format`
- No binary support, no base64 handling
- MCP image servers return base64 strings that get treated as text

**The proposal**: Extend `ArtifactFormat` with `Binary` and `Base64`:

```rust
// src/ast/artifact.rs
pub enum ArtifactFormat {
    Text,
    Json,
    Yaml,
    Binary,   // Raw bytes, written to file
    Base64,   // Base64-encoded string, decoded to bytes on write
}
```

**Usage in workflows**:

```yaml
tasks:
  generate-image:
    invoke:
      tool: generate_image
      mcp: fal-ai
      params:
        prompt: "A logo for {{with.brand}}"
        model: "fal-ai/flux/schnell"
    output:
      artifact:
        path: "./output/logo.png"
        format: base64                   # Decode base64 -> binary file
        mime_type: "image/png"           # For validation

  resize-image:
    invoke:
      tool: transform
      mcp: cloudinary
      params:
        image: "{{with.logo_path}}"
        width: 512
        height: 512
    with:
      logo_path: generate-image.artifact.path
    output:
      artifact:
        path: "./output/logo-512.png"
        format: binary
```

**New event metadata**:

```json
{
  "event": "ArtifactWritten",
  "path": "./output/logo.png",
  "format": "base64",
  "mime_type": "image/png",
  "size_bytes": 245760
}
```

**Binary data in `with:` bindings**: Binary artifacts are referenced by path,
not by content. `{{with.image}}` resolves to the file path, not the raw bytes.
This keeps the binding system text-based and avoids memory bloat.

**Effort**: S | **Impact**: Medium | **Dependencies**: None

---

### B3: Conditional Branching (`when:` clause)

**The gap**: Nika workflows are linear DAGs. Every task runs if its dependencies
are met. There's no way to conditionally skip a task based on a previous task's
output. This forces users to create separate workflows for different paths or
use workarounds with default values.

**What exists today**:
- DAG with `depends_on:` edges
- `with:` bindings with `??` default operator
- No `if/then/else`, no `switch/case`, no `when:` clause
- Workaround: split into multiple workflows, select at runtime

**The proposal**: `when:` clause on any task:

```yaml
tasks:
  classify:
    infer:
      prompt: "Classify sentiment: {{with.text}}"
      structured:
        schema:
          type: object
          properties:
            sentiment: { type: string, enum: [positive, negative, neutral] }
            confidence: { type: number }

  respond-positive:
    when: "classify.sentiment == 'positive'"
    infer:
      prompt: "Write a thank-you response"
    with:
      text: classify

  respond-negative:
    when: "classify.sentiment == 'negative' && classify.confidence > 0.8"
    infer:
      prompt: "Write an empathetic response addressing concerns"
    with:
      text: classify

  respond-neutral:
    when: "classify.sentiment == 'neutral'"
    infer:
      prompt: "Write a neutral acknowledgment"
    with:
      text: classify

  escalate:
    when: "classify.sentiment == 'negative' && classify.confidence <= 0.8"
    gate:
      message: "Low confidence negative sentiment - human review needed"
      show: "{{with.original}}"
    with:
      original: classify
```

**Expression syntax** (subset of JSONPath + comparisons):

| Operator | Example | Description |
|----------|---------|-------------|
| `==` | `task.field == 'value'` | Equality |
| `!=` | `task.field != 'value'` | Inequality |
| `>`, `>=`, `<`, `<=` | `task.score > 0.8` | Numeric comparison |
| `&&` | `a == 'x' && b > 0.5` | Logical AND |
| `\|\|` | `a == 'x' \|\| a == 'y'` | Logical OR |
| `!` | `!task.is_spam` | Logical NOT |
| `in` | `task.status in ['approved', 'pending']` | Set membership |
| `exists` | `exists(task.optional_field)` | Field presence |

**DAG implications**:
- A `when:` clause does NOT create a dependency. The dependency must be declared
  via `with:` or `depends_on:` as usual.
- Tasks with unsatisfied `when:` are marked `Skipped` (new `TaskSkipped` event).
- Downstream tasks of a skipped task: if all their dependencies are met (some
  via other paths), they still run. If a required dependency was skipped, they
  are also skipped.

**Validation by `nika check`**:
- Parse `when:` expressions at analysis phase
- Validate referenced task IDs and field paths
- Warn on unreachable tasks (when all paths to a task are conditional and
  mutually exclusive is not guaranteed)

**Effort**: M | **Impact**: Medium-High | **Dependencies**: None

---

## 6. YAGNI -- Deliberately Skipped

These features were considered and deliberately rejected. Each has a reason
rooted in Nika's DNA and current strategic position.

### 6.1 A2A Protocol Integration

**What it is**: Google's Agent-to-Agent protocol (v1.0.0, March 12, 2026).
Defines how AI agents discover, communicate, and delegate to each other.

**Why skip**:
- A2A is agent-to-agent; Nika is agent-to-tool (MCP). Different layer.
- MCP covers all our inter-agent needs via `invoke:`.
- A2A adoption is still nascent. Only 3 production implementations exist.
- If needed later, A2A can be exposed as an MCP server wrapping the protocol.
- The spec is changing rapidly (3 revisions in 6 months).

**Revisit when**: A2A reaches v2.0 with stable adoption and clear Nika use case.

### 6.2 Visual Workflow Builder

**What it is**: A drag-and-drop GUI for building workflows (like Dify, n8n, Retool).

**Why skip**:
- This is Dify's lane. They have $100M+ funding and a dedicated UI team.
- Nika is CLI-first by design. Our users are developers who prefer YAML + LSP.
- The LSP (already feature-gated) provides IDE support (autocomplete, validation).
- Building a visual builder would take 3-6 months and distract from core engine work.
- Amp killed their VS Code extension (Feb 2026) to focus on CLI. The trend is CLI-first.

**Revisit when**: Never. This is a strategic decision, not a timing one.

### 6.3 Real-Time Collaboration

**What it is**: Google Docs-style real-time collaborative editing of workflows.

**Why skip**:
- YAML files + git = collaboration is already solved.
- Real-time collab requires: conflict resolution, operational transforms, WebSocket
  infrastructure, presence indicators. Massive engineering effort.
- Our users work solo or in small teams with git branches.
- No competing workflow engine has real-time collab (even Dify doesn't).

**Revisit when**: Never. Git is the collaboration layer.

### 6.4 Marketplace with Ratings & Reviews

**What it is**: A marketplace for buying/selling workflows, skills, and schemas.

**Why skip**:
- `nika pkg` already handles package distribution (install, publish, search).
- Adding ratings, reviews, payments = building an e-commerce platform.
- The ecosystem isn't large enough to justify marketplace infrastructure.
- npm, crates.io, pip don't have ratings/reviews. A registry is enough.

**Revisit when**: When `nika pkg` has 1000+ packages and community requests it.

### 6.5 Prompt Caching Integration

**What it is**: Explicit prompt caching directives (Anthropic's cache_control,
OpenAI's prompt caching) in the workflow schema.

**Why skip**:
- Provider-specific. Each provider handles caching differently.
- rig-core (our provider abstraction) handles caching transparently.
- Anthropic's prompt caching is automatic for long system prompts (>1024 tokens).
- Adding explicit `cache:` directives would couple the schema to specific providers.
- The cost savings are real but the abstraction leak isn't worth it.

**Revisit when**: A provider-agnostic caching standard emerges.

### 6.6 Batch API Support

**What it is**: Using Anthropic's Batch API or OpenAI's Batch API for
non-real-time workloads at 50% cost reduction.

**Why skip**:
- Batch APIs are asynchronous (hours of delay). Not suitable for interactive workflows.
- Nika workflows are designed to complete in seconds-to-minutes.
- The latency tradeoff (50% cheaper but 24h wait) doesn't fit our use cases.
- For truly batch workloads, users can wrap `nika run` in a shell loop.

**Revisit when**: We have a use case for truly asynchronous, non-interactive workloads
where 24h latency is acceptable.

---

## 7. Proposed Wave Mapping

### 7.1 Original 3-Wave Roadmap (from doc 05)

| Wave | Version | Schema | Priorities |
|------|---------|--------|------------|
| Wave 1 | v0.28 | @0.12 | P-MODEL (4-slot routing) + P-RECORD (LLM compression) |
| Wave 2 | v0.29 | @0.13 | P-SHAKA (orchestration) + P-CONTEXT (token budgets) |
| Wave 3 | v0.30 | | P-MEMORY (3-tier Punk Records) + P-INTROSPECT (6 runtime tools) |

### 7.2 Proposed Enriched Waves

The 10 new proposals are distributed across waves based on dependencies and
synergies with the existing 6 priorities:

```
Wave 1 — v0.28 "Measure & Test"
├── P-MODEL (4-slot model routing)        ← existing
├── P-RECORD (LLM compression)           ← existing
├── B1: Per-task model override           ← NEW (stepping stone for P-MODEL)
├── S1: nika eval                         ← NEW (independent, high impact)
└── A1: Cost tracking & budget            ← NEW (leverages existing events)

Wave 2 — v0.29 "Production-Ready"
├── P-SHAKA (dynamic orchestration)       ← existing
├── P-CONTEXT (token budget per task)     ← existing
├── S2: HITL gate primitive               ← NEW (needs B3)
├── S3: Durable execution                 ← NEW (leverages NDJSON)
├── A2: Workflow composition              ← NEW (synergy with P-SHAKA)
└── B3: Conditional branching (when:)     ← NEW (needed by S2)

Wave 3 — v0.30 "Intelligence"
├── P-MEMORY (3-tier Punk Records)        ← existing
├── P-INTROSPECT (6 runtime tools)        ← existing
├── A3: Agent skills                      ← NEW (synergy with P-MEMORY)
├── B2: Binary artifacts                  ← NEW (enables image gen workflows)
└── A4: OpenTelemetry export              ← NEW (production monitoring)
```

### 7.3 Wave Rationale

**Wave 1 "Measure & Test"**: Focus on measurement and quality feedback loops.
- B1 (per-task model) enables model experimentation needed for P-MODEL design.
- S1 (nika eval) provides the testing framework to validate all subsequent changes.
- A1 (cost tracking) makes costs visible, informing P-RECORD compression strategy.
- **Theme**: You can't optimize what you can't measure.

**Wave 2 "Production-Ready"**: Focus on production robustness.
- B3 (when:) is a prerequisite for S2 (HITL gates with approve/reject routing).
- S3 (durable execution) makes long workflows reliable in production.
- A2 (composition) + P-SHAKA enables complex multi-workflow orchestrations.
- **Theme**: Make Nika trustworthy enough for unattended production runs.

**Wave 3 "Intelligence"**: Focus on intelligence and ecosystem.
- A3 (agent skills) composes naturally with P-MEMORY (skills remember across runs).
- B2 (binary artifacts) unlocks image generation workflows.
- A4 (OTel) connects Nika to the production observability stack.
- **Theme**: Make Nika smart and observable.

### 7.4 Schema Version Impact

| Version | Schema | New Syntax Elements |
|---------|--------|---------------------|
| v0.28 | @0.12 (unchanged) | `budget:`, `model:` (task-level), `.eval.yaml` format |
| v0.29 | @0.13 (bump) | `gate:`, `when:`, `invoke: { workflow: ... }`, `for_each:` |
| v0.30 | @0.13 (unchanged) | `.skill.yaml` format, `artifact.format: binary/base64` |

---

## 8. Current vs Proposed Gap Analysis

### 8.1 Feature Matrix

| Feature | v0.27 (Current) | v0.28 (Wave 1) | v0.29 (Wave 2) | v0.30 (Wave 3) |
|---------|-----------------|-----------------|-----------------|-----------------|
| **Verbs** | 5 (infer/exec/fetch/invoke/agent) | 5 (same) | 5 + gate: | 5 + gate: |
| **Builtins** | 12 (7 core + 5 file) | 12 (same) | 12 (same) | 18 (+6 introspect) |
| **Model config** | Workflow/verb level | + Task level + 4-slot | + Shaka routing | Same |
| **Testing** | `nika check` (structural) | + `nika eval` (quality) | Same | Same |
| **Cost tracking** | Events only | + Budget enforcement | Same | Same |
| **HITL** | `nika:prompt` (text) | Same | + `gate:` (structured) | Same |
| **Durability** | Checkpoint (manual) | Same | + `nika resume` (auto) | Same |
| **Composition** | include/nika:run | Same | + DAG-of-DAGs + for_each | Same |
| **Branching** | None | None | + `when:` clause | Same |
| **Artifacts** | Text/Json/Yaml | Same | Same | + Binary/Base64 |
| **Observability** | NDJSON (34 events) | Same | Same | + OTel export |
| **Memory** | None | + P-RECORD (compression) | + P-CONTEXT (budgets) | + 3-tier memory |
| **Skills** | None | None | None | + `.skill.yaml` |

### 8.2 CLI Command Growth

```
v0.27:  nika {run, check, ui, chat, studio, init, new, provider, mcp, model,
              pkg, completion, config, schema, doctor, workflow, trace, lsp}
        = 18 commands

v0.28:  + nika eval
        = 19 commands

v0.29:  + nika resume
        = 20 commands

v0.30:  (no new top-level commands, OTel is config-based)
        = 20 commands
```

### 8.3 Event System Growth

```
v0.27:  34 EventKind variants across 11 categories

v0.28:  + BudgetWarning, BudgetExceeded, EvalCaseStarted, EvalCaseCompleted,
          EvalAssertPassed, EvalAssertFailed
        = 40 events

v0.29:  + GatePresented, GateResolved, RunResumed, TaskSkipped, TaskRetried,
          WorkflowInvoked, WorkflowReturned, ConditionEvaluated
        = 48 events

v0.30:  + SkillLoaded, ArtifactBinaryWritten, OTelSpanExported
        = 51 events
```

### 8.4 Error Code Growth

```
v0.27:  14 ranges (NIKA-000 through NIKA-429)

v0.28:  + NIKA-500-509 (Eval errors)
        + NIKA-510-519 (Budget errors)

v0.29:  + NIKA-520-529 (Gate errors)
        + NIKA-530-539 (Resume errors)
        + NIKA-540-549 (Condition errors)
        + NIKA-550-559 (Composition errors)

v0.30:  + NIKA-560-569 (Skill errors)
        + NIKA-570-579 (Binary artifact errors)
        + NIKA-580-589 (OTel errors)
```

---

## 9. Sources & References

### 9.1 Research Agents (deployed 2026-03-16)

| Agent | Key Sources Scraped |
|-------|---------------------|
| Workflow Trends | Dify blog (Feb 2026), Amp blog "Coding Agent Is Dead" (Feb 2026), LangSmith docs, AutoGen MCP docs, CrewAI memory docs |
| MCP Ecosystem | MCP registry (mcp.so), GitHub MCP server repos, npm @modelcontextprotocol packages |
| Durable Execution | Restate blog (Oct 2025), Temporal docs, Anthropic context engineering paper (2025), CrewAI persistence docs |
| Eval & Observability | Langfuse docs (v3), Arize Phoenix docs, OpenTelemetry GenAI specs, promptfoo docs, Braintrust docs, DeepEval docs |
| SEO AI Tools | DataForSEO MCP docs, Ahrefs API docs, fetchSERP docs, GEO research papers |

### 9.2 Existing Evolution Documents Referenced

| Doc | Title | Key Inputs |
|-----|-------|------------|
| [01](./01-current-features.md) | Current Features | Complete v0.27 inventory |
| [03](./03-competitive-landscape.md) | Competitive Landscape | Positioning matrix |
| [05](./05-evolution-roadmap.md) | Evolution Roadmap | 6 priorities, 3 waves |
| [08](./08-nika-030-complete-guide.md) | v0.30 Complete Guide | User-facing feature guide |
| [11](./11-nika-030-technical-reference.md) | Technical Reference | Struct/trait specs |
| [12](./12-vegapunk-naming.md) | Vegapunk Naming | Naming conventions |
| [15](./15-ecosystem-coherence.md) | Ecosystem Coherence | System topology |
| [18](./18-mcp-multimodal-ecosystem-march2026.md) | MCP Multimodal | MCP server catalog |
| [19](./19-package-registry-design.md) | Package Registry | `nika pkg` design |
| [20](./20-agent-memory-architectures.md) | Agent Memory | Memory patterns |

### 9.3 Raw Research Dumps

| File | Content |
|------|---------|
| [21-ai-workflow-landscape-march2026.md](./21-ai-workflow-landscape-march2026.md) | Full workflow trends research |
| [21-ai-eval-testing-observability.md](./21-ai-eval-testing-observability.md) | Full eval/observability research |
| [21-durable-execution-checkpoint-patterns.md](./21-durable-execution-checkpoint-patterns.md) | Full durable execution research |
| [mcp-ecosystem-march-2026.md](./mcp-ecosystem-march-2026.md) | Full MCP ecosystem catalog |

### 9.4 Key External References

| Source | URL | Relevance |
|--------|-----|-----------|
| Dify Human Input Node | https://dify.ai/blog | HITL pattern reference |
| Amp "Coding Agent Is Dead" | https://amp.dev/blog | CLI-first validation, Checks pattern |
| LangSmith COALA paper | https://arxiv.org/abs/2309.02427 | Memory-as-files architecture |
| Restate durable execution | https://restate.dev/blog | Journal-based recovery |
| OpenTelemetry GenAI | https://opentelemetry.io/docs/specs/semconv/gen-ai/ | Standard LLM tracing attributes |
| Langfuse | https://langfuse.com | Open-source LLM observability (19k+ stars) |
| promptfoo | https://promptfoo.dev | Evals-as-code patterns |
| Braintrust | https://braintrust.dev | Experiment comparison framework |
| DataForSEO MCP | https://dataforseo.com | Official SEO MCP server |
| A2A Protocol | https://google.github.io/a2a | Agent-to-agent (YAGNI for now) |

---

## Appendix A: Competitive Positioning After Proposals

```
                    Expressivity
                         ↑
                         │        ★ Nika v0.30 [0.85, 0.95]
                         │          (with all proposals)
                         │
                         │     · Slate [0.75, 0.85]
                         │
          Nika v0.27     │
           [0.35, 0.80]  │
                         │
                         │  · Claude Code [0.30, 0.60]
                         │
                         │         · LangGraph [0.45, 0.50]
                         │              · CrewAI [0.55, 0.40]
                         │  · Codex [0.20, 0.55]
                         ├──────────────────────────────────→
                                  Memory Sophistication
```

The 10 proposals move Nika from [0.35, 0.80] to [0.85, 0.95]:
- **Memory axis** (+0.50): Durable execution, P-MEMORY, agent skills with persistence
- **Expressivity axis** (+0.15): Conditional branching, HITL gates, workflow composition, evals

---

## Appendix B: One Piece Naming Alignment

All new features follow the Vegapunk naming system (doc 12):

| Feature | Vegapunk Component | Rationale |
|---------|--------------------|-----------|
| `nika eval` | Independent (test framework) | Eval is external to the runtime |
| `gate:` | Shaka (orchestrator decides routing) | Gates are orchestration decisions |
| `nika resume` | Punk Records (journal = NDJSON trace) | Resume reads from WARM tier |
| `budget:` | York (resource allocation) | York handles cost/resource constraints |
| `when:` | Shaka (conditional dispatch) | Conditions are routing decisions |
| `for_each:` | Atlas (parallel tactical execution) | Batch execution is tactical |
| `.skill.yaml` | Edison (main model capabilities) | Skills extend agent knowledge |
| Binary artifacts | Independent (IO format) | Format is orthogonal to architecture |
| OTel export | Independent (observability) | External export, not core engine |
| Per-task model | Edison/Atlas/York/Pythagoras | Direct slot selection |

---

<div align="center">

[← 20 Agent Memory](./20-agent-memory-architectures.md) · [00 Index →](./00-README.md)

</div>
