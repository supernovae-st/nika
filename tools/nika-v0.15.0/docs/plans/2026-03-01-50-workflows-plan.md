# 50 Workflows Plan - Nika v0.15.0 Showcase

## Overview

This document defines 50 diverse, functional workflows demonstrating 100% of Nika's features.

**Goal**: Create production-ready example workflows covering all business sectors plus creative/fun use cases.

**Feature Coverage Target**: Every Nika feature must be used at least once across the 50 workflows.

---

## Feature Coverage Matrix

### 5 Verbs (MUST use all)
| Verb | Count | Workflows |
|------|-------|-----------|
| `infer:` | 40+ | Most workflows |
| `exec:` | 25+ | DevOps, automation |
| `fetch:` | 15+ | API integrations |
| `invoke:` | 10+ | MCP tool calls |
| `agent:` | 15+ | Complex reasoning |

### 7 Providers (MUST use all)
| Provider | Count | Workflows |
|----------|-------|-----------|
| `claude` | 15 | Default for complex reasoning |
| `openai` | 10 | Vision, embeddings |
| `mistral` | 5 | Code generation |
| `groq` | 5 | Speed-critical |
| `deepseek` | 5 | Cost-effective |
| `gemini` | 5 | NEW v0.15.0 |
| `ollama` | 5 | Local/private |

### Composition Features
| Feature | Count | Description |
|---------|-------|-------------|
| `context:` | 20+ | File loading |
| `include:` | 10+ | DAG fusion |
| `flows:` | 30+ | Explicit dependencies |
| `for_each` | 15+ | Parallel iteration |
| `lazy:` | 10+ | Deferred bindings |

### v0.15.0 New Features (MUST showcase)
| Feature | Count | Description |
|---------|-------|-------------|
| `shell: false` | 10+ | Secure exec |
| `shell: true` | 5+ | Pipeline exec |
| `temperature:` | 10+ | Creativity control |
| `system:` | 15+ | System prompts |
| `max_tokens:` | 10+ | Output limits |
| `gemini` provider | 5 | Google's Gemini |

### 11 Builtin Tools (agent: blocks)
| Tool | Workflows |
|------|-----------|
| `nika:sleep` | Rate limiting demos |
| `nika:log` | Progress tracking |
| `nika:emit` | Custom events |
| `nika:assert` | Validation |
| `nika:prompt` | HITL |
| `nika:run` | Sub-workflows |
| `nika:read` | File reading |
| `nika:write` | File writing |
| `nika:edit` | File editing |
| `nika:glob` | File patterns |
| `nika:grep` | Content search |

---

## Workflow Categories

### Batch 1: Business (10 workflows)

| # | Name | Features | Description |
|---|------|----------|-------------|
| 01 | `invoice-generator` | infer, exec, context, gemini | Generate PDF invoices from templates |
| 02 | `email-campaign` | infer, for_each, groq, temperature | Multi-language email campaigns |
| 03 | `crm-sync` | fetch, invoke, agent, mcp | Sync CRM data across platforms |
| 04 | `meeting-summarizer` | infer, context, claude, system | Summarize meeting transcripts |
| 05 | `contract-analyzer` | agent, nika:read, nika:assert | Legal contract analysis |
| 06 | `expense-report` | fetch, infer, for_each, openai | Auto-categorize expenses |
| 07 | `customer-support` | agent, nika:prompt, mistral | Interactive support bot |
| 08 | `sales-forecast` | fetch, infer, deepseek | Predict sales trends |
| 09 | `hr-onboarding` | include, context, for_each | Employee onboarding workflow |
| 10 | `inventory-alert` | fetch, infer, nika:emit, ollama | Low stock notifications |

### Batch 2: DevOps (10 workflows)

| # | Name | Features | Description |
|---|------|----------|-------------|
| 11 | `ci-pipeline` | exec, shell:false, for_each | Secure CI/CD pipeline |
| 12 | `docker-builder` | exec, shell:true, context | Docker image automation |
| 13 | `log-analyzer` | agent, nika:grep, nika:glob | Parse and analyze logs |
| 14 | `security-scan` | exec, fetch, infer, claude | Security vulnerability scan |
| 15 | `infra-provisioner` | agent, exec, nika:write | Infrastructure as code |
| 16 | `backup-manager` | exec, for_each, nika:assert | Automated backups |
| 17 | `deploy-orchestrator` | include, flows, agent | Multi-stage deployment |
| 18 | `metrics-collector` | fetch, for_each, groq | Collect system metrics |
| 19 | `incident-responder` | agent, nika:prompt, nika:log | Incident management |
| 20 | `changelog-generator` | exec, infer, mistral | Auto-generate changelogs |

### Batch 3: Content (10 workflows)

| # | Name | Features | Description |
|---|------|----------|-------------|
| 21 | `blog-writer` | agent, infer, temperature:0.9 | Generate blog posts |
| 22 | `seo-optimizer` | fetch, infer, for_each | SEO content optimization |
| 23 | `social-scheduler` | infer, for_each, gemini | Multi-platform social posts |
| 24 | `video-script` | agent, context, claude | Video script generation |
| 25 | `newsletter-curator` | fetch, infer, for_each | Curate newsletter content |
| 26 | `podcast-notes` | infer, context, system | Podcast show notes |
| 27 | `translation-hub` | for_each, infer, deepseek | Multi-language translation |
| 28 | `image-captioner` | infer, openai, context | AI image captions |
| 29 | `content-calendar` | agent, nika:write, flows | Plan content calendar |
| 30 | `brand-voice` | infer, context, max_tokens | Maintain brand consistency |

### Batch 4: Data & Research (10 workflows)

| # | Name | Features | Description |
|---|------|----------|-------------|
| 31 | `web-scraper` | fetch, for_each, nika:write | Web data extraction |
| 32 | `data-cleaner` | agent, nika:read, nika:edit | Clean and normalize data |
| 33 | `research-assistant` | agent, fetch, claude | Academic research helper |
| 34 | `survey-analyzer` | infer, context, for_each | Analyze survey responses |
| 35 | `competitor-monitor` | fetch, infer, nika:emit | Track competitor changes |
| 36 | `trend-spotter` | fetch, agent, gemini | Identify emerging trends |
| 37 | `data-visualizer` | exec, infer, context | Generate data visualizations |
| 38 | `report-generator` | include, context, flows | Automated reports |
| 39 | `citation-finder` | fetch, infer, for_each | Find academic citations |
| 40 | `knowledge-base` | agent, nika:write, nika:glob | Build knowledge bases |

### Batch 5: Fun & Creative (10 workflows)

| # | Name | Features | Description |
|---|------|----------|-------------|
| 41 | `story-generator` | agent, temperature:1.0, claude | Interactive fiction |
| 42 | `recipe-creator` | infer, context, for_each | Generate recipes |
| 43 | `game-master` | agent, nika:prompt, ollama | D&D game master |
| 44 | `music-playlist` | fetch, infer, groq | Mood-based playlists |
| 45 | `travel-planner` | agent, fetch, gemini | Plan dream vacations |
| 46 | `meme-generator` | infer, exec, temperature:0.95 | Create meme content |
| 47 | `workout-coach` | agent, nika:prompt, mistral | Personalized workouts |
| 48 | `gift-finder` | fetch, infer, for_each | Find perfect gifts |
| 49 | `daily-digest` | fetch, infer, nika:log | Personal news digest |
| 50 | `dream-interpreter` | agent, claude, system | Analyze dreams |

---

## Implementation Order

### Phase 1: Foundation (Workflows 1-10)
- Focus: Basic verb usage, provider diversity
- Duration: ~2 hours
- Priority: 100% feature coverage start

### Phase 2: DevOps (Workflows 11-20)
- Focus: exec: verb, shell security, automation
- Duration: ~2 hours
- Priority: v0.15.0 shell features

### Phase 3: Content (Workflows 21-30)
- Focus: Creative generation, temperature control
- Duration: ~2 hours
- Priority: LLM control features

### Phase 4: Data (Workflows 31-40)
- Focus: Complex DAGs, agent reasoning
- Duration: ~2 hours
- Priority: Advanced composition

### Phase 5: Fun (Workflows 41-50)
- Focus: Interactive, creative, engaging
- Duration: ~2 hours
- Priority: Showcase capabilities

---

## Validation Checklist

After all 50 workflows created:

- [ ] All 5 verbs used
- [ ] All 7 providers used
- [ ] All 11 builtin tools used
- [ ] context: used 20+ times
- [ ] include: used 10+ times
- [ ] for_each used 15+ times
- [ ] lazy: used 10+ times
- [ ] shell:false used 10+ times
- [ ] shell:true used 5+ times
- [ ] temperature: used 10+ times
- [ ] system: used 15+ times
- [ ] max_tokens: used 10+ times
- [ ] All workflows validate with `nika check`
- [ ] All workflows have meaningful descriptions
- [ ] All workflows are self-contained or have clear dependencies

---

## File Structure

```
examples/
├── README.md                    # Index and usage guide
├── _partials/                   # Shared include files
│   ├── setup-common.nika.yaml
│   ├── logging.nika.yaml
│   └── error-handling.nika.yaml
├── _context/                    # Shared context files
│   ├── brand-guidelines.md
│   ├── api-keys.template.yaml
│   └── common-prompts.md
├── business/                    # Batch 1: 01-10
│   ├── 01-invoice-generator.nika.yaml
│   ├── 02-email-campaign.nika.yaml
│   └── ...
├── devops/                      # Batch 2: 11-20
│   ├── 11-ci-pipeline.nika.yaml
│   ├── 12-docker-builder.nika.yaml
│   └── ...
├── content/                     # Batch 3: 21-30
│   ├── 21-blog-writer.nika.yaml
│   ├── 22-seo-optimizer.nika.yaml
│   └── ...
├── data/                        # Batch 4: 31-40
│   ├── 31-web-scraper.nika.yaml
│   ├── 32-data-cleaner.nika.yaml
│   └── ...
└── fun/                         # Batch 5: 41-50
    ├── 41-story-generator.nika.yaml
    ├── 42-recipe-creator.nika.yaml
    └── ...
```

---

## Next Steps

1. Create directory structure
2. Create shared partials and context files
3. Implement workflows batch by batch
4. Test each workflow with `nika check`
5. Fix any validation errors
6. Update README with examples
7. Commit, push, create PR
