# nika-onboard — the onboarding surface (founding wizard + guided first workflow)

> L4 · descended from `nika-cli/src/verbs/{new.rs, init/}` at the 15k
> prod-LOC wall (2026-07-12 · the `nika-display`/`nika-dap`/`nika-tmpl`
> precedents) — per D-2026-07-09-N1 this is the cli UNIT in a second
> member, named by parentage.

## Contract

The onboarding surfaces share one law (questions before writes · the human
keeps the hand · the proof inside the first minute):

- **`founding`** — `nika init`'s body: the briefs table (`briefs` — the
  scaffold bytes: AGENTS.md contract · per-client thin briefs · schema
  wiring), the recipe register (`recipes` — SETS over the embedded
  templates through the guided `stamp`, ids explicit under the kebab
  law), the scripted path (`scripted_run` — file receipts and next commands),
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

## Compile foundation

`compile` is a stateless in-memory authoring foundation, separate from the
existing `new` routing surface. CREATE accepts exact embedded skeleton names
and explicit request-local JSON answers. Unsupported natural language returns
Incomplete with an Unknown diagnostic and no substitute workflow.

EDIT requires the caller's explicit base source and offers two inputs:

- `CompileRequest::edit(base, "Set const.NAME to JSON_LITERAL")`, also allowing
  `Set const.NAME` followed by an answer to `const.NAME`.
- `CompileRequest::set_constant(base, name, literal_json)`, a structured operation
  for adapters. The bare name and JSON literal are separate arguments; no
  natural-language prompt is synthesized.

Both lower to the same bounded constant-edit operation, existing-node mutation,
assembler, canonical parser and pure Check preview. Names contain only ASCII
letters, digits or underscores; empty, invalid or absent targets and malformed
JSON preserve the original source and remain Incomplete. The operation cannot
insert nodes or edit permits, tasks or nested paths. Payloads resembling policy
or instructions remain literal data; expression islands and root objects with
both `type` and `value` are refused. Existing typed declarations retain their
type, and a type-incompatible candidate remains Incomplete under Check.

Emission must preserve the intended canonical literal projection; decoder
ambiguity or precision loss refuses the edit and retains the original source.
All unrelated semantic values survive accepted edits. Formatting and comments
are not preserved by successful re-emission. SLOT values remain mandatory
questions. Source-only Check is not environment resolution or Run admission.

Compile performs no file access, credential probes, provider calls or execution,
and keeps no session state. The application owns base revision selection, CAS
and materialization. This API supplies a constant-edit seam, not a Graph
implementation, general patch language or full natural-language authoring.

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
  per-recipe ratchet parses AND checks every scaffold clean (validated through
  `nika-schema`; Compile also uses that parser in production).
- **Questions before writes**: cancel at any wizard beat = « nothing
  written », honestly (PTY-pinned at the root).
- **Readable sober registers**: file rows keep the `✔ created …` / `· skipped …`
  prefixes. Created briefs explain their purpose; the team block teaches Git
  and offers `nika init --project-file --yes` before the next commands.
  `NIKA.md` is the human guide, leaving an existing `README.md` untouched.
- **Wire results are outcomes**: both doors preserve each client's receipt
  and propagate a failed wiring code. A failed wizard never emits the ready
  panel. Client names round-trip through the root's live registry without
  substituting a broader target.
- **Project hooks**: Cursor and Claude project settings point at the same
  canonical kit scripts, copied into each client's `hooks-nika/` directory.
  Claude commands anchor at the quoted `CLAUDE_PROJECT_DIR`; existing settings
  are skipped under the normal law. A declared hook is not proof a client
  has loaded it.
- **No CLI framework below the root**: `CanvasTheme` stays a plain enum
  here; the root mirrors it as its clap `ValueEnum`.

## Metrics

Live numbers come from the projector — `scripts/crate-metrics.sh
nika-onboard` (no hardcoded LOC anchor in this spec; nothing to drift).
