# Nika Diamond — Architecture Decision Records

ADRs document **why** we chose a specific architectural path. They are the
public, permanent memory of non-obvious decisions — so that six months from
now, a contributor (or a future version of you) can reconstruct the reasoning
without having to re-derive it.

## When to write an ADR

Write an ADR for any decision that matches ≥2 of:

- Crosses a **crate or layer boundary** (affects >1 crate, or defines a layer contract)
- Introduces a **new invariant** that downstream code must respect
- **Locks a public API** (trait, struct, enum that other crates depend on)
- Makes a **non-reversible** choice (can't be undone without breaking users)
- Trades off a **quality attribute** against another (perf vs correctness, simplicity vs flexibility)
- Replaces or supersedes a prior ADR

Do **not** write an ADR for:

- Single-file refactors
- Bug fixes (commit message suffices)
- Dependency bumps
- Renames without behavior change
- Style / formatting decisions

## How to write one

1. Pick the next sequential number (`ls docs/adr/ | grep -E '^adr-[0-9]{3}'`)
2. Copy `TEMPLATE.md` to `adr-NNN-<short-kebab-title>.md`
3. Fill the sections. Be specific. Prefer grep-verified evidence over narrative.
4. Set Status to `Proposed` while discussing, `Accepted` once committed
5. Commit with message: `docs(adr): ADR-NNN add <title>` (co-author Nika 🦋)
6. If the ADR supersedes a prior one, update the prior ADR's status to
   `Superseded by ADR-NNN` in the same commit

## Enforcement

- `scripts/ci/check-adr-coverage.sh` — hygiene check: every admitted workspace
  member should be mentioned in at least one ADR (warn-only for now).
- Future: integrate into the `crate-admit` skill as a soft-gate.

## Index

The index is a PROJECTION, never hand-maintained (a hand copy froze at
ADR-015 while the corpus grew to 67 — the drift this section replaces):

- **machine**: [`index.json`](index.json) · [`index.toml`](index.toml) —
  id · title · status · date · the full relation graph (supersedes ·
  requires · enables · amends), regenerated from frontmatter via
  `scripts/adr/generate-index.sh` on every ADR change.
- **human**: `ls docs/adr/adr-*.md` — the filenames ARE the titles; each
  file's frontmatter is the authoritative status.

## Pre-Diamond ADRs (legacy reference)

Nika v0.1 → v0.27 had 8 ADRs that were superseded by the Diamond big-bang
rewrite. They are archived at:

```
supernovae-hq (private monorepo) → archive/nika-v0.79/adr/
```

Read them for historical rationale on the 5-semantic-verb DSL, YAML-first,
MCP-only principles — concepts retained conceptually but re-implemented
from scratch in Diamond.

## Further reading

- Michael Nygard, [Documenting Architecture Decisions](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) (2011)
- ThoughtWorks [Lightweight Architecture Decision Records](https://www.thoughtworks.com/insights/blog/architecture-decision-records)
- [adr.github.io](https://adr.github.io/) — community ADR patterns
