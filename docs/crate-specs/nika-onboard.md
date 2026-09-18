# nika-onboard — the onboarding surface (project bootstrap + stateless Compile)

> L4 · descended from the CLI authoring/bootstrap surface at the 15k
> prod-LOC wall (2026-07-12 · the `nika-display`/`nika-dap`/`nika-tmpl`
> precedents) — per D-2026-07-09-N1 this is the cli UNIT in a second
> member, named by parentage.

## Contract

The onboarding surfaces share one law (questions before writes · the human
keeps the hand · the proof inside the first minute):

- **`founding`** — `nika init`'s body: the briefs table (`briefs` — the
  scaffold bytes: AGENTS.md contract · per-client thin briefs · schema
  wiring), the recipe register (`recipes` — SETS over the embedded
  templates through the bootstrap-only `stamp`, ids explicit under the kebab
  law), the scripted path (`scripted_run` — file receipts and next commands),
  the canvas stamp (`nika.dag.theme` parsed-and-re-emitted into a
  CREATED settings.json, never string-spliced), and the trace cover
  (`gitignore` — adds-only: create when absent · one marked section
  appended when the human's file lacks it · never a duplicate, so a
  founded repo cannot commit its own `.nika/traces/` journals).
- **`wizard`** — the founding conversation on the clack rail (recipe ·
  model · canvas · agents), over any `BufRead`/`Write` pair.
- **`bootstrap`** — private init recipe/model prompts and stamping; no authoring
  intent router, creation command or first-workflow wizard.
- **`routing`** — read-only gallery discovery shared by MCP; it cannot author.

## Compile foundation

`compile` is a stateless in-memory authoring core behind the CLI creation door. CREATE accepts exact embedded skeleton names
and explicit request-local JSON answers. `hello` (also `01-hello`) takes the
embedded hello lesson through the same assembler with explicit `mock/echo`.
`with_workflow_id` names CREATE source explicitly; EDIT refuses this option. Unsupported natural language returns
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

Every answer door (CREATE answers and the text, answer and structured EDIT
inputs) shares one guarded literal path. An integer answer outside the canonical
reader's exact `i64` range is refused from the answer's own text, at any depth,
before a decoder can round it; a whole f64 would otherwise satisfy even a
`type: integer` constant. Fraction and exponent answers are floats under the
f64 contract and quoted digits stay text. This is a refusal, not arbitrary
precision.

Emission must preserve the intended canonical literal projection; decoder
ambiguity or precision loss refuses the edit and retains the original source.
All unrelated semantic values survive accepted edits. EDIT replaces only the
target literal's source range for single-line scalars and flow collections;
comments, formatting, line endings and all bytes outside that range survive.
Typed constants retain the declaration around their `value`. Semantic no-ops
retain the exact original source. Block collections and multi-line scalars are
refused without changes; this is a bounded editor, not a general YAML CST.
The emitted candidate must agree with both literal readers, so an accepted
edit cannot introduce decoder drift that blocks a later unrelated edit.
The replacement token is compact JSON. DEL, the C1 controls U+0080 to U+009F
(NEL included) and the noncharacters U+FFFE and U+FFFF are written as `\uXXXX`
escapes in strings and object keys, because a YAML 1.1 reader rejects or folds
them when raw. The escape only proposes a token: the two-reader comparison
still decides every candidate, so such literals stay editable and an
unreadable one is refused. A value written by omission (`key:` with nothing
after it) is refused explicitly, since the parser marks it at the next token
and never at the target; a written `~` or `null` remains editable.

Scope limitations of this bounded editor, not a verified general
source-preserving edit contract:

- The replaced range is the whole literal. Comments INSIDE a replaced
  multi-line flow collection belong to that range and disappear; the outcome
  is still Ready and carries no diagnostic for them. Bytes outside the range
  survive by construction (prefix, token, suffix); no literal reader sees
  comments, so that property is demonstrated by tests, not checked at run time.
- CREATE's slot assembler retains its existing guarded re-emission behavior
  and emits block forms for multi-line strings and collections. EDIT can
  therefore refuse a constant that CREATE itself just wrote. There is no
  fallback whole-document re-emission: the guarantee is untouched bytes
  outside the literal, also for a comment-free source. Block editing is out
  of this slice.

SLOT values remain mandatory questions. Source-only Check is not environment
resolution or Run admission.

Compile performs no file access, credential probes, provider calls or execution,
and keeps no session state. The application owns base revision selection, CAS
and materialization. This API supplies a constant-edit seam, not a Graph
implementation, general patch language or full natural-language authoring.

Private pattern-facet derivation (#1666) lives beside Compile. It parses with
the same schema door, reads Check `needed` as the membrane, and never text-scans
comments. Task ids come from the `tasks:` map only. It is not a public YAML key,
a fifth verb, or an SDK noun, and it does not change CREATE/EDIT.

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
