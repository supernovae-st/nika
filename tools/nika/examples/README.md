# Nika Workflow Examples

519+ example workflows organized by purpose.

## Quick Start

```bash
# Run a workflow
nika run examples/gates/feature/exec-basic.nika.yaml

# Validate without executing
nika check examples/use-cases/blog-pipeline.nika.yaml

# Run with provider override
nika run examples/gates/feature/infer-basic.nika.yaml -p openai
```

## Prerequisites

- **Tier 1 (exec/fetch):** No API keys needed
- **Tier 2+ (infer/agent):** Set `OPENAI_API_KEY` or `ANTHROPIC_API_KEY`
- **MCP workflows:** Firecrawl (`FIRECRAWL_API_KEY`) or Perplexity (`PERPLEXITY_API_KEY`)

## Directory Structure

```
examples/
  gates/
    feature/     168 feature gate workflows (1 feature each)
    combo/        50 cross-feature combinations
    error/        50 error path tests (expected failures)
    complex/     100 ultra-complex stress tests
  use-cases/      30 production-quality real-world workflows
  advanced/       Advanced patterns
  lib/            Reusable workflow fragments
```

## Feature Gates (`gates/feature/`)

One workflow per feature. All pass `nika check`. Categories:

| Category | Count | Examples |
|----------|-------|---------|
| Verbs | 20+ | `infer-basic`, `exec-pipe`, `fetch-post-json`, `agent-basic` |
| Bindings | 15+ | `with-binding-basic`, `with-transform-upper`, `with-fallback` |
| Transforms | 25+ | `transform-sort`, `transform-join`, `transform-parse-json` |
| for_each | 10+ | `for-each-basic`, `for-each-parallel`, `for-each-binding` |
| Artifacts | 10+ | `artifact-json`, `artifact-template`, `artifact-append` |
| Structured | 5+ | `structured-inline`, `structured-properties` |
| Agent | 10+ | `agent-spawn`, `agent-tools-multiple`, `agent-from-def` |

## Use Cases (`use-cases/`)

Production-quality workflows:

- `blog-pipeline` - Research -> outline -> draft -> review
- `translation` - Translate to 3 languages with for_each
- `changelog-generator` - Git log -> categorize -> format
- `bug-triager` - Bug report -> severity + priority (structured)
- `data-etl` - Extract -> transform -> load (artifact)
- `seo-keywords` - Generate SEO keywords (structured output)
- `code-review` - Read code -> analyze -> generate review
- `sentiment-analyzer` - Analyze reviews with for_each
- ... and 22 more

## Syntax Reference

```yaml
schema: "nika/workflow@0.12"
workflow: my-workflow

tasks:
  - id: step1
    exec: "echo hello"

  - id: step2
    depends_on: [step1]
    with:
      data: $step1
    infer: "Process: {{with.data}}"
    provider: openai
    model: gpt-4.1-mini
    max_tokens: 100
```

## Learn More

- [Language Reference](../../docs/reference/language-reference.md)
- [Nika CLI](../CLAUDE.md)
