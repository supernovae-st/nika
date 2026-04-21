# Example Brand Guide

This file is a **placeholder example** for Nika's `context:` YAML feature. It shows how a Nika workflow can reference a markdown file as structured context.

## Usage in a workflow

```yaml
context:
  brand: ./context/brand.md      # loaded as string and passed to LLM context
  style: ./context/style-guide.md
  schema: ./context/schema.json  # parsed as JSON
```

## What YOUR brand.md should contain

Replace this example with your own brand voice, tone, and guidelines. A good brand.md typically covers:

- **Voice axioms** — 5-10 principles describing how your product sounds
- **Anti-patterns** — phrases or tones to avoid
- **Example artifacts** — 2-3 canonical pieces of copy for tone calibration

Keep it under ~800 tokens for efficient LLM context usage.

## About this placeholder

This file ships with Nika as a structural reference for users writing their first workflow. It contains no real brand content from any product.
