# Nika v1.0 Master Plan

**Date**: 2026-03-28
**Codename**: Drums of Liberation
**Current**: v0.49.3, schema @0.12, 8457 tests
**Goal**: From solid engine to intelligent platform with ecosystem

---

## Architecture Overview

```
Phase 0: Stabilize        Phase 1: Intelligence       Phase 2: Ecosystem
(v0.50 — 2 weeks)        (v0.51-0.55 — 10 weeks)     (v0.56-0.60 — 6 weeks)

Fix LSP blocker           P-MODEL (agent presets)      Registry server deploy
Fix registry server       P-RECORD (compression)       nika pkg publish
Fix VS Code extension     P-ORCHESTRATE (goal:)        Community seed content
Update vision docs        P-CONTEXT (budgets)          Showcase CLI commands
Wire agents: → tasks      P-MEMORY-LOCAL (NDJSON)      Course CLI commands
                          Inference routing             Fine-tuning pipeline
                          Self-improvement (Hermes)     Telegram trigger
                          Introspection tools           MCP server expansion
```

---

## Phase 0: Stabilize (v0.50 — 2 weeks)

**Rule**: Zero new features. Fix what's broken. Update what's stale.

### 0.1 Blockers (Day 1-2)

| # | Task | File | Effort | Blocks |
|---|------|------|--------|--------|
| B1 | Fix LSP borrow-after-move | nika-lsp/src/backend.rs:90 | 5 min | All LSP work |
| B2 | Fix VS Code extension marketplace | CI/VSCE_PAT renewal | 1h | User adoption |
| B3 | Deploy registry.supernovae.studio | Infra (Phase 1 = GitHub static) | 4h | All pkg remote |
| B4 | Fix error code table in CLAUDE.md | tools/nika/CLAUDE.md | 15 min | Dev confusion |

### 0.2 Wire `agents:` to tasks (Day 3-4)

The `agents:` block EXISTS in the AST but tasks don't use it as a preset system.
This is the #1 prerequisite for P-MODEL.

| # | Task | Detail |
|---|------|--------|
| A1 | Document `agents:` + `from:` in rules/nika.md | Existing feature, zero docs |
| A2 | Add `agent:` shorthand on infer/fetch/exec tasks | `agent: lite` inherits provider+model+temperature |
| A3 | Test preset inheritance | Agent def → task override → defaults chain |
| A4 | Update vision docs to reference existing `agents:` | Replace model_slots with agents |

### 0.3 Vision docs coherence (Day 5-6)

| # | Task | Detail |
|---|------|--------|
| D1 | Add deprecation banner to 03 + 05 vision docs | "Written for v0.27. Current: v0.49. See master plan." |
| D2 | Remove edison/atlas/york/pythagoras naming | Replace with default/lite/think/search/vision/judge/coder/summary |
| D3 | Update competitive matrix in 03 | Add current features (TUI, media, structured output, custom endpoints) |
| D4 | Reconcile schema strategy | Decide: stay @0.12 with additive fields OR bump to @0.13 for orchestrate |
| D5 | Create "Current vs Vision" status matrix | What shipped vs what's planned |

### 0.4 Quick wins from handoff (Day 7-8)

| # | Task | Source | Effort |
|---|------|--------|--------|
| Q1 | Onboarding wizard on MissingApiKey | v049-fixes-handoff R1 | 30 LOC |
| Q2 | Jobs exit code bug | v049-fixes-handoff R4 | 2 LOC |
| Q3 | Dry-run cost estimation in summary | v049-fixes-handoff R6 | 20 LOC |
| Q4 | LSP task-level unknown key detection | lsp-overhaul Layer 1 | 2h |

### 0.5 Showcase + Course CLI (Day 9-10)

| # | Task | Detail |
|---|------|--------|
| S1 | `nika showcase list` | List all 115 workflows with category filter |
| S2 | `nika showcase extract <name>` | Extract to current dir |
| S3 | `nika course status` | Show constellation progress |
| S4 | `nika course next` | Open next exercise |

**Exit criteria Phase 0:**
- [ ] `cargo check --workspace` = zero errors (incl. nika-lsp)
- [ ] `cargo test --workspace --lib` = 8500+ tests
- [ ] `nika showcase list` shows 115 workflows
- [ ] `nika pkg search` reaches registry (even if empty)
- [ ] VS Code marketplace at v0.50
- [ ] Vision docs have deprecation banners
- [ ] `agents:` documented with examples

---

## Phase 1: Intelligence (v0.51-0.55 — 10 weeks)

**Rule**: Ship incrementally. Each sub-version adds one P-priority.

### 1.1 P-MODEL Complete (v0.51 — 2 weeks)

Builds on Phase 0's `agents:` wiring. Add routing + fallback.

| # | Task | Detail | Effort |
|---|------|--------|--------|
| M1 | Agent preset resolution in executor | `agent: think` → resolve provider+model+system+temperature | M |
| M2 | Preset inheritance chain | agent def → task override → workflow default | M |
| M3 | Inference routing with fallback | `provider: [gemini, deepseek, claude]` | M |
| M4 | Cost-aware routing hints | Task metadata shows model cost estimate | L |
| M5 | `nika:cost` introspection tool | Builtin tool returning tokens/cost | L |
| M6 | Events: `AgentPresetUsed`, `ProviderFallback` | 2 new EventKind variants | L |

**YAML after v0.51:**
```yaml
agents:
  think: { provider: anthropic, model: claude-sonnet-4-6, extended_thinking: true }
  lite: { provider: groq, model: llama-3.3-70b-versatile }
  search: { provider: deepseek, model: deepseek-chat }

tasks:
  - id: plan
    agent: think           # Inherits provider, model, thinking
    infer: "Plan the landing page structure"

  - id: research
    agent: search          # Cheap + fast
    fetch:
      url: "https://api.example.com/trends"
      extract: jsonpath
      selector: "$.data[*].keyword"

  - id: generate
    agent: lite            # Fast generation
    with: { plan: $plan, data: $research }
    infer: "Generate content using: {{with.plan}}"
    provider: [groq, deepseek, anthropic]  # Fallback chain
```

### 1.2 P-RECORD (v0.52 — 3 weeks)

The critical primitive that enables orchestration.

| # | Task | Detail | Effort |
|---|------|--------|--------|
| R1 | `Record` struct in `runtime/record.rs` | summary, key_findings, confidence, tokens, cost, model | M |
| R2 | `RecordCompressor` in `runtime/record_compress.rs` | Uses agent: lite to compress | H |
| R3 | `record:` field in Task AST | compress, retain, max_tokens, confidence_threshold | L |
| R4 | Record-aware bindings | `with: { data: $task }` returns Record when available | M |
| R5 | Backward compat | No `record:` block → raw output (current behavior) | L |
| R6 | Events: `RecordCreated`, `ConfidenceScore` | New EventKind variants | L |
| R7 | `nika:records` introspection tool | Query accumulated records | M |

**YAML after v0.52:**
```yaml
tasks:
  - id: research
    agent: search
    infer: "Research QR code trends 2026"
    record:
      compress: true
      retain: [key_findings, statistics]
      max_tokens: 500

  - id: write
    agent: think
    with: { findings: $research }   # Gets compressed Record, not raw 10K tokens
    infer: "Write article using: {{with.findings}}"
```

### 1.3 P-ORCHESTRATE (v0.53 — 4 weeks)

The hardest piece. Dynamic DAG + orchestrator loop.

| # | Task | Detail | Effort |
|---|------|--------|--------|
| O1 | `goal:` field in Workflow AST | String field, auto-detects orchestrate mode | L |
| O2 | `Orchestrator` struct | Loop: review records → dispatch → synthesize → repeat | H |
| O3 | `DynamicDag` in `dag/dynamic.rs` | Runtime task creation (mutable DAG) | H |
| O4 | Orchestrator plans in YAML | Generates .nika.yaml, runs via nika:run | H |
| O5 | Round tracking | max_rounds, record_budget, cost limit | M |
| O6 | `nika:orchestrate` introspection tool | Round, budget, progress | L |
| O7 | Schema decision: @0.12 additive OR @0.13 bump | Decision required | L |

**YAML after v0.53:**
```yaml
schema: "nika/workflow@0.12"

goal: |
  Generate a complete French landing page for QR Code AI.
  Research current trends, write 4 sections, review quality.
  Target confidence: 0.85

agents:
  think: { provider: anthropic, model: claude-sonnet-4-6, extended_thinking: true }
  lite: { provider: groq, model: llama-3.3-70b-versatile }
  search: { provider: deepseek, model: deepseek-chat }

tasks:
  - id: research
    agent: search
    infer: "Research: {{goal.topic}}"
    record: { compress: true, max_tokens: 300 }

  - id: write_section
    agent: lite
    infer: "Write: {{goal.section}} using {{with.context}}"
    record: { compress: true, retain: [content], max_tokens: 800 }

  - id: review
    agent: think
    infer: "Review and critique: {{with.draft}}"
    record: { compress: true, retain: [issues, score] }
```

### 1.4 P-CONTEXT + P-INTROSPECT (v0.54 — 2 weeks, parallel)

| # | Task | Detail | Effort |
|---|------|--------|--------|
| C1 | `context_budget:` field on Task | Max tokens in context | L |
| C2 | Budget enforcement in executor | Truncate/warn if exceeded | M |
| C3 | Token counting utilities | Approximate tokenizer | M |
| C4 | 4 more introspection tools | nika:dag_info, nika:task_status, nika:threads, nika:orchestrate | M |

### 1.5 P-MEMORY-LOCAL + Self-Improvement (v0.55 — 2 weeks)

NovaNet-free memory layer.

| # | Task | Detail | Effort |
|---|------|--------|--------|
| ME1 | `.nika/records/` NDJSON persistence | Write records to disk after workflow | M |
| ME2 | SQLite FTS5 index | Full-text search across sessions | M |
| ME3 | `nika trace search <query>` | CLI for cross-session recall | L |
| ME4 | Frozen snapshot pattern | Context files loaded 1x, never re-read | L |
| ME5 | File locking (fcntl) | Concurrent write safety for daemon | L |
| ME6 | Background nudge (optional) | Post-workflow review agent | H |
| ME7 | Security scanning | Injection detection on outputs | M |

**Exit criteria Phase 1:**
- [ ] Agent presets route to different models per task
- [ ] Records compress outputs and pass summaries downstream
- [ ] `goal:` field triggers orchestrator loop
- [ ] Context budgets prevent zone-morte degradation
- [ ] Cross-session memory via NDJSON + FTS5
- [ ] 6 introspection builtin tools
- [ ] Inference routing with fallback chains

---

## Phase 2: Ecosystem (v0.56-0.60 — 6 weeks)

### 2.1 Registry & Publishing (v0.56 — 2 weeks)

| # | Task | Detail | Effort |
|---|------|--------|--------|
| E1 | GitHub-based registry (Phase 1 plan) | supernovae/nika-registry repo | M |
| E2 | `nika pkg publish` command | Create tarball + PR to registry | M |
| E3 | Seed registry with 20 packages | Extract from 115 showcases | M |
| E4 | Security scanning on install | Injection, SSRF, command patterns | M |
| E5 | Trust levels | builtin / trusted / community | L |

### 2.2 Community & Content (v0.57 — 2 weeks)

| # | Task | Detail | Effort |
|---|------|--------|--------|
| C1 | `nika showcase extract --all` working | Extract all 115 to directory | L |
| C2 | Workflow metadata (WORKFLOW.md) | agentskills.io-compatible frontmatter | M |
| C3 | `nika new --ai "description"` | NL → YAML generation | M |
| C4 | Course gamification | Constellation map, badges | M |

### 2.3 Integration & Distribution (v0.58-0.60 — 2 weeks)

| # | Task | Detail | Effort |
|---|------|--------|--------|
| I1 | Telegram webhook trigger | Daemon receives Telegram → runs workflow | H |
| I2 | MCP server expansion | Add nika_run, nika_list_packages tools | M |
| I3 | Fine-tuning data pipeline | 5K synthetic workflows, nika check as reward | H |
| I4 | Homebrew tap + GitHub releases | Distribution channels | M |

---

## What Changed from Original Vision

| Original (v0.27 roadmap) | New (v0.50+ plan) | Why |
|--------------------------|-------------------|-----|
| `model_slots:` (edison/atlas/york) | `agents:` presets (already exists!) | agents: is implemented, model_slots never was |
| Schema @0.13 for orchestrate | Stay @0.12, additive fields | Avoid breaking change for zero users |
| P-MEMORY needs NovaNet | P-MEMORY-LOCAL with NDJSON + FTS5 | NovaNet not ready, local memory works fine |
| Wave 1-3 sequential | Phase 0→1→2 with parallel tracks | More realistic, ships incrementally |
| Satellite templates | Reuse existing `agents:` + `from:` | No need for new concept |
| Punk Records 3-tier | 2-tier first: HOT (RAM) + WARM (NDJSON) | COLD (NovaNet) = future upgrade |

---

## Priority Matrix

```
                    HIGH IMPACT
                        │
          ┌─────────────┼─────────────┐
          │  P-MODEL     │  P-RECORD   │
          │  (presets)   │  (compress) │
          │              │             │
LOW ──────┼──────────────┼─────────────┼────── HIGH
EFFORT    │  Stabilize   │ P-ORCHESTR  │      EFFORT
          │  (Phase 0)   │  (goal:)    │
          │              │             │
          │  Registry    │  Fine-tune  │
          │  (seed)      │  (Nika-Brain│
          └─────────────┼─────────────┘
                        │
                    LOW IMPACT
```

**Critical path**: Stabilize → P-MODEL → P-RECORD → P-ORCHESTRATE
Everything else parallelizes.

---

## Dependencies

```
Phase 0: Stabilize
    ├── B1: LSP fix ──────────────────────────────┐
    ├── A1-A4: agents: wiring ────────────────────┤
    └── D1-D5: Vision docs ───────────────────────┤
                                                   ▼
Phase 1.1: P-MODEL ──► Phase 1.2: P-RECORD ──► Phase 1.3: P-ORCHESTRATE
    │                      │                        │
    │                      ├── Phase 1.4: CONTEXT   │
    │                      └── Phase 1.4: INTROSPECT│
    │                                               │
    │                      Phase 1.5: MEMORY-LOCAL ◄┘
    │
    └──► Phase 2.1: Registry (parallel)
         Phase 2.2: Community (parallel)
         Phase 2.3: Integration (parallel)
```

---

## Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|-----------|
| DynamicDag complexity | HIGH | Blocks P-ORCHESTRATE | Start with nika:run (existing) as fallback |
| Schema break @0.13 | MEDIUM | Confuses zero users | Stay @0.12, additive fields only |
| Registry never deployed | MEDIUM | Blocks ecosystem | GitHub-based Phase 1 = zero infra |
| Fine-tuning data quality | MEDIUM | Bad Nika-Brain model | nika check = automatic reward, low risk |
| NovaNet never ready | LOW | No COLD memory tier | WARM (NDJSON) is fully functional |
| LSP complexity spiral | MEDIUM | Delays everything | Timebox to 2 days, defer Layer 4+ |

---

## Success Metrics

| Metric | Phase 0 | Phase 1 | Phase 2 |
|--------|---------|---------|---------|
| Tests | 8,500+ | 9,000+ | 9,500+ |
| CLI commands | All working | +6 introspection | +publish, +new --ai |
| Schema | @0.12 stable | +goal, +record, +context_budget | same |
| Packages | 0 (registry up) | 0 (local only) | 20+ seeded |
| Agent presets | Documented | Routed per-task | Community-shared |
| Memory | None | NDJSON local | FTS5 searchable |
| Orchestration | None | goal: + dynamic DAG | Self-improving |

---

## Timeline

```
Week 1-2      Phase 0: Stabilize (LSP, registry, agents:, docs)
Week 3-4      Phase 1.1: P-MODEL (presets, routing, fallback)
Week 5-7      Phase 1.2: P-RECORD (Record struct, compression, bindings)
Week 8-11     Phase 1.3: P-ORCHESTRATE (goal:, DynamicDag, YAML planning)
Week 9-10     Phase 1.4: P-CONTEXT + P-INTROSPECT (parallel)
Week 11-12    Phase 1.5: P-MEMORY-LOCAL + self-improvement
Week 11-14    Phase 2.1: Registry + seed content (parallel)
Week 13-16    Phase 2.2-2.3: Community + integration (parallel)
```

**Total: ~16 weeks to v1.0 platform with ecosystem.**
