# nika-onboard — the onboarding surface (founding wizard + guided first workflow)

> L4 · descended from `nika-cli/src/verbs/{new.rs, init/}` at the 15k
> prod-LOC wall (2026-07-12 · the `nika-display`/`nika-dap`/`nika-tmpl`
> precedents) — per D-2026-07-09-N1 this is the cli UNIT in a second
> member, named by parentage.

## Contract

Two surfaces, one law (questions before writes · the human keeps the
hand · the proof inside the first minute):

- **`founding`** — `nika init`'s body: the briefs table (`briefs` — the
  scaffold bytes: AGENTS.md contract · per-client thin briefs · schema
  wiring), the recipe register (`recipes` — SETS over the embedded
  templates through the guided `stamp`, ids explicit under the kebab
  law), the scripted path (`scripted_run` — historical report bytes),
  the canvas stamp (`nika.dag.theme` parsed-and-re-emitted into a
  CREATED settings.json, never string-spliced), and the trace cover
  (`gitignore` — adds-only: create when absent · one marked section
  appended when the human's file lacks it · never a duplicate, so a
  founded repo cannot commit its own `.nika/traces/` journals).
- **`wizard`** — the founding conversation on the clack rail (recipe ·
  model · canvas · agents), over any `BufRead`/`Write` pair.
- **`guided`** — `nika new`'s body: exact-name → BM25 intent routing →
  the chain default; the three-question wizard; `stamp` (id ·
  description · model, YAML-safe scalars); the discovery listing with
  its `embedded set:` wire-contract line.

## The injected seams

The composition root (`nika-cli`) owns what proving and wiring MEAN;
this crate converses and scaffolds:

- `Audit` — `&dyn Fn(&str) -> Outcome` · the check ladder
  (`nika check <path>` at the root · a stub in tests).
- `Wire` — `&dyn Fn(&str, &str) -> Outcome` · the MCP wiring
  (`nika wire <client>` at the root); the wizard speaks client WORDS,
  the root resolves them on its `WireTarget` register.

`Outcome { text, code }` mirrors the root's `VerbOutput` (kept local so
the descent adds zero reverse dependency); `codes` mirrors the spec §4
exit vocabulary.

## Invariants

- **Own-corpus law (#261), inherited**: every workflow any recipe can
  scaffold is an embedded template VERBATIM through `stamp` — the
  per-recipe ratchet parses AND checks every scaffold clean (dev-dep on
  `nika-schema`, test-side only).
- **Questions before writes**: cancel at any wizard beat = « nothing
  written », honestly (PTY-pinned at the root).
- **Byte-stable sober registers**: the scripted report keeps the exact
  historical shape (`✔ created …` rows + the classic next block).
- **No CLI framework below the root**: `CanvasTheme` stays a plain enum
  here; the root mirrors it as its clap `ValueEnum`.

## Metrics

Live numbers come from the projector — `scripts/crate-metrics.sh
nika-onboard` (no hardcoded LOC anchor in this spec; nothing to drift).
