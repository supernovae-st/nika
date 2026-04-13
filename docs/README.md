# Nika docs

> Diátaxis-shaped documentation for the Nika workflow engine.
> AI-native: each folder has one purpose, optimized for both LLM context windows and human navigation.

## Structure

| Folder | Purpose | Diátaxis quadrant |
|--------|---------|-------------------|
| [`guides/`](guides/) | How-to guides + tutorials (task-oriented) | Tutorials + How-to |
| [`reference/`](reference/) | Verb syntax, transforms, error codes, architecture refs | Reference |
| [`concepts/`](concepts/) | Architecture explanations + design system | Explanation |
| [`adr/`](adr/) | Architecture Decision Records (MADR format) | — |
| [`roadmap/`](roadmap/) | Active planning (Constellation sessions) | — |
| [`releases/`](releases/) | Per-version release notes | — |
| [`research/`](research/) | Active research artifacts (memory, retrieval, etc.) | — |
| [`marketing/`](marketing/) | Launch material (blog posts, press, social) | — |
| [`audit/`](audit/) | Audit reports | — |
| [`reports/`](reports/) | Performance / state reports | — |

## AI entry points (root)

| File | Purpose |
|------|---------|
| [`llms.txt`](llms.txt) | Curated index for LLM scrapers (per [llmstxt.org](https://llmstxt.org)) |
| [`llms-full.txt`](llms-full.txt) | Full text dump for AI consumption |
| [`llms-syntax.txt`](llms-syntax.txt) | Workflow syntax reference for AI |

The repo root also has [`AGENTS.md`](../AGENTS.md) (with [`CLAUDE.md`](../CLAUDE.md) symlink) as the canonical AI agent entry point.

## What lives outside this repo

Strategic content lives in the private parent monorepo at `/Users/thibaut/dev/supernovae/docs/`:

- **Brand, vision, competitive intelligence** → `04-research/`
- **Launch plans, press kits, podcasts** → `03-launch/`
- **Visual launch assets** (artwork, generated images) → `launch-art/`
- **Memory feature design hub** → `nika-memory/` (ADR-001 through ADR-004)

This separation is enforced by the `supernovae-hq/CLAUDE.md` rule: research, strategy, brand positioning, and launch plans NEVER live in `nika/docs/`.

## Cleanup discipline

Any folder that grows past 50 files needs splitting. Any document older than 90 days without a follow-up commit should be archived or deleted. Untracked drafts that accumulate in `docs/` are accumulated AI-engineering debt — review and prune at the end of every Constellation session.
