# Nika MCP Integration Brainstorm

**Date:** 2026-02-20
**Context:** Research on Claude Code SDK, installed MCP servers, and skills

---

## 1. New MCP Servers to Integrate

### High Priority (Direct Value)

| MCP Server | Use Case in Nika | Integration Complexity |
|------------|------------------|----------------------|
| **sequential-thinking** | Complex reasoning chains, multi-step planning | Low - npm package |
| **playwright** | Browser automation, UI testing, screenshots | Medium - requires browser |
| **neo4j** | Direct graph queries (for advanced users) | Low - already configured |
| **ahrefs** | SEO research workflows | Low - HTTP MCP |

### Medium Priority (Specialized)

| MCP Server | Use Case | Notes |
|------------|----------|-------|
| **21st.dev** | UI component generation | Via frontend-design plugin |
| **IDE diagnostics** | Code linting in workflows | Via mcp__ide__getDiagnostics |
| **Jupyter** | Code execution in notebooks | Via mcp__ide__executeCode |

---

## 2. Workflow Templates to Create

### Research & Analysis Workflows

```yaml
# SEO Research Pipeline
workflow: seo-research-pipeline
description: "Full SEO analysis using ahrefs + perplexity + firecrawl"
mcp: [ahrefs, perplexity, firecrawl]
tasks:
  - id: keyword_research
    agent: "Research keywords using ahrefs data"
  - id: competitor_analysis
    agent: "Scrape competitor pages with firecrawl"
  - id: content_gaps
    infer: "Identify content opportunities"
```

```yaml
# Documentation Generator
workflow: docs-from-code
description: "Generate docs from codebase using context7"
mcp: [context7]
tasks:
  - id: gather_patterns
    agent: "Use context7 to find library patterns"
  - id: generate_docs
    infer: "Create documentation from patterns"
```

### Development Workflows

```yaml
# UI Component Generator
workflow: ui-component-pipeline
description: "Generate React components with 21st.dev"
mcp: [21st-magic]
tasks:
  - id: design_spec
    infer: "Create component specification"
  - id: generate_ui
    invoke: 21st_magic_component_builder
  - id: accessibility_check
    invoke: getDiagnostics
```

```yaml
# Browser Testing Pipeline
workflow: e2e-test-generator
description: "Generate and run E2E tests"
mcp: [playwright]
tasks:
  - id: analyze_ui
    agent: "Analyze UI structure"
  - id: generate_tests
    infer: "Generate Playwright test code"
  - id: run_tests
    exec: "npx playwright test"
```

---

## 3. Claude Code SDK Patterns for Nika

### Hooks Integration

**Concept:** Add hook-like capabilities to Nika workflows

```yaml
# Proposed syntax
workflow: secure-deployment
hooks:
  pre_task:
    - matcher: "exec:*deploy*"
      command: "./security-check.sh"
  post_task:
    - matcher: "infer:*"
      log: true

tasks:
  - id: deploy
    exec: "kubectl apply -f deployment.yaml"
```

### Permission System

**Concept:** Fine-grained tool permissions per workflow

```yaml
workflow: readonly-analysis
permissions:
  allow: [invoke:novanet_*, fetch:*]
  deny: [exec:*, Write]

tasks:
  - id: analyze
    invoke: novanet_describe  # Allowed
  # - exec: "rm -rf /"       # Would be blocked
```

### Session Persistence

**Concept:** Resume long-running workflows

```yaml
workflow: multi-day-research
session:
  persist: true
  checkpoint_interval: 10  # tasks

tasks:
  - id: day1_research
    agent: "..."
  - id: day2_synthesis
    agent: "..."
    depends_on: [day1_research]
```

---

## 4. New Verbs to Consider

### `think:` - Sequential Reasoning

```yaml
- id: complex_decision
  think:
    prompt: "Analyze trade-offs between approaches"
    steps: 5
    mcp: sequential-thinking
```

### `browse:` - Browser Automation

```yaml
- id: capture_screenshot
  browse:
    url: "https://example.com"
    actions:
      - wait: 2000
      - screenshot: "output.png"
    mcp: playwright
```

### `test:` - Automated Testing

```yaml
- id: run_tests
  test:
    type: playwright  # or jest, cargo
    pattern: "tests/*.spec.ts"
    parallel: true
```

---

## 5. Skills as Workflow Templates

**Concept:** Convert Claude Code skills into Nika workflows

| Skill | Workflow Equivalent |
|-------|---------------------|
| `brainstorming` | `brainstorm-agent.nika.yaml` |
| `test-driven-development` | `tdd-workflow.nika.yaml` |
| `code-review` | `review-pipeline.nika.yaml` |
| `systematic-debugging` | `debug-workflow.nika.yaml` |

### Skill → Workflow Adapter

```yaml
# skills/brainstorming.nika.yaml
schema: nika/workflow@0.5
workflow: skill-brainstorming
description: "Socratic brainstorming (from Claude Code skill)"

# Converted from skills/spn-powers/brainstorming.md
tasks:
  - id: clarify
    infer: "Ask 3 clarifying questions about: {{use.topic}}"
  - id: alternatives
    infer: "Generate 5 alternative approaches"
  - id: critique
    infer: "Play devil's advocate for each"
  - id: synthesize
    infer: "Synthesize into final recommendation"
```

---

## 6. Integration Priorities

### Phase 1: Quick Wins (This Week)

1. ✅ Add `sequential-thinking` MCP to config
2. ✅ Add `playwright` MCP to config
3. Create `seo-research.nika.yaml` using ahrefs
4. Create `docs-generator.nika.yaml` using context7

### Phase 2: SDK Patterns (Next Sprint)

1. Implement workflow hooks (pre_task, post_task)
2. Add permission system to workflow schema
3. Add session persistence for long workflows

### Phase 3: New Verbs (Future)

1. `think:` verb with sequential-thinking MCP
2. `browse:` verb with playwright MCP
3. `test:` verb for automated testing

---

## 7. Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  NIKA WORKFLOW ENGINE (Enhanced)                                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │   infer:    │  │   exec:     │  │   fetch:    │  │   invoke:   │        │
│  │  (LLM)      │  │  (Shell)    │  │  (HTTP)     │  │  (MCP)      │        │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘        │
│         │                │                │                │               │
│  ┌──────┴────────────────┴────────────────┴────────────────┴──────┐        │
│  │                        MCP CLIENT LAYER                         │        │
│  └─────────────────────────────────────────────────────────────────┘        │
│         │                │                │                │                │
│  ┌──────▼──────┐  ┌──────▼──────┐  ┌──────▼──────┐  ┌──────▼──────┐        │
│  │  NovaNet    │  │  Perplexity │  │  Firecrawl  │  │  Context7   │        │
│  │  (Graph)    │  │  (Search)   │  │  (Scrape)   │  │  (Docs)     │        │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘        │
│                                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                      │
│  │  Playwright  │  │  Sequential  │  │  Ahrefs     │                       │
│  │  (Browser)   │  │  Thinking    │  │  (SEO)      │                       │
│  └──────────────┘  └──────────────┘  └──────────────┘                      │
│                                                                             │
│  NEW FEATURES:                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                         │
│  │   Hooks     │  │ Permissions │  │  Sessions   │                         │
│  │ (pre/post)  │  │ (allow/deny)│  │ (persist)   │                         │
│  └─────────────┘  └─────────────┘  └─────────────┘                         │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 8. Concrete Next Steps

1. **Update `config/mcp-servers.yaml`** with all discovered servers
2. **Create example workflows** for each new MCP
3. **Document MCP tool schemas** for workflow authors
4. **Consider new verbs** (`think:`, `browse:`, `test:`)
5. **Port popular skills** to workflow templates

---

## References

- Claude Code SDK: `/Users/thibaut/.claude-code-docs/docs/`
- Installed plugins: `~/.claude/plugins/installed_plugins.json`
- MCP configs: `~/.claude/plugins/cache/*/.mcp.json`
- Our skills: `.claude/skills/`
