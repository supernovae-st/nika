# Crate spec — `nika-tui-view`

| | |
|---|---|
| Status | **WIP · MEMBER** (size-cap split of `nika-tui`, a WIP crate itself · ADR-143 · D-2026-07-09-N1 · 2026-09-30) · it joins the workspace as WIP with its unit, the `nika-tui` precedent |
| Layer | L4 — a library surface; lateral L4→L4 edge `nika-tui → nika-tui-view`, never back (a production edge for workspace workflow inspection) |
| Design | the viewers of the object in view: pure functions from what a caller already holds (bytes or text, the facts it knows, the cells offered) to a `Rendered` value; the four faces of a workflow from the typed facts its owner hands over, and every artifact a workflow produces |
| IMPL | measured by `scripts/crate-metrics.sh nika-tui-view` at each freeze; the crate carries what `nika-tui` held under `src/view` on 2026-09-30 (the gate's own counter: 6,185 prod LOC at the split, 6,143 of them moved · 85 unit tests: the 83 that moved with their files and the two parity tests of the copied role mapping) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` — member of the `nika-tui` unit |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `ratatui` 0.30 (the styled lines) · `unicode-width` · lateral L4 `nika-session` (the review's run order and gates, for the plan face) and `nika-display` (the theme roles, the wires of the graph face, the check layers, the JSON shape) · dev: `nika-cli-host` (the faces tests stand in for the owner of the check facts) |
| NIKA codes | none owed — a viewer refuses nothing; it says what it cut, what fell back and what disagreed |

## 1. Purpose

`nika-tui` stood at 13,879 prod LOC on 2026-09-30, against the 15,000 wall, with the workspace
door, the keys, the transcript and the runs projection still to land. Its `view` module held the
viewers of the object in view: IO-free, not wired yet, and named by no other module of the
renderer. Per D-2026-07-09-N1 a size-cap split is ONE architectural unit in several workspace
members: the viewers descend here, below the renderer, which keeps 7,735 prod LOC (ADR-143).

The paths change from `nika_tui::view::…` to `nika_tui_view::…`; nothing is re-exported by
`nika-tui`. The renderer uses this member as a normal dependency for the workspace workflow
faces. The `view_gallery` example of the terminal UI branches was not recovered; no example
currently shows the artifact viewers.

## 2. The viewers of the object in view

The native workspace wires the four workflow faces and calls `artifact` for a
run's settlement outputs and bounded current reads of files it reported writing.
The Live host owns acquisition and provenance. A viewer is a pure function: bytes or text, the facts the caller knows (`Meta`:
name, declared type, format word, size, dimensions, duration, digest, producer, provenance,
availability, protected) and the cells offered (`Canvas`: width, glyph column, colour, bounds)
become a `Rendered` (title, facts, styled lines, notes). It reads no file, clock or
environment; every decoder is bounded (256 KiB read, 1,000 lines, 4 KiB examined per line by
default) and says what it cut.

- `workflow` draws one of four faces of the workflow in view from typed facts its owner hands
  over (`Workflow`): the source with the verbs as syntax, the plan in run order
  (`nika_session::review`), the graph drawn by `nika_display::wires` or listed by wave when a
  drawing would lie, and the check (the four layers, VALID alone for a proposal judged on its
  source, every finding by its code and place, the hints as advisory). It never audits, judges
  or reads a permit; the static plan and graph keep the verbs in the surrounding ink.
- `artifact` classifies by declared type, then extension, then signature (`classify`; a
  disagreement is a note; a plain JSON declaration from `json_diff` reads as a JSON Patch) and
  shows JSON (a long document summarised by `nika_display::shape`), JSON Patch and merge patch
  as edits, YAML, TOML, CSV/TSV as an aligned table, Markdown, diffs, text and code; an image
  or a sound by the facts its header states, never drawn or played; PDF, office files, video
  and opaque bytes by their facts and a short hex head. Null, empty, missing, changed, not
  loaded and unknown read differently.
- A protected object (`Meta::protected`) masks every value under a key naming a credential,
  however deep and however many lines it spans, and every value that wears or holds a
  credential shape: the JSON document, the values of a JSON Patch and of a merge patch, and the
  lines of YAML, TOML, Markdown, diffs, tables, text, code and workflow sources all walk one
  masking walker. Keys, brackets, quotes, block indicators and fences stay visible; a template
  reference stays readable; the hex head and the shape fact are withheld; every fact and note
  is cleaned before it is drawn.
- A control character or a bidirectional override reaches the screen as a visible mark, widths
  are measured by grapheme, facts and notes agree in number, and every view keeps its meaning
  under `NO_COLOR` and in the ASCII column (the engine's words included).
- Workspace PTYs exercise settlement output and Markdown file display through
  the Live owner. This does not qualify every supported artifact format or a
  stamped integrated build; the gallery example remains unrecovered.

## 3. Boundary

- Nothing here reads a file, a clock or the environment, spawns, blocks or stores. The faces
  tests are the only callers of `nika_cli_host::oracle::audit_source`, as the owner's stand-in,
  once per fixture.
- A viewer takes its colour from a semantic role, through the crate's private `role::style`:
  a copy of the renderer's own mapping, pinned by its test to the slot the CLI theme paints
  (`nika_display::theme::Theme::paint`), as the renderer's copy is. This crate never depends on
  `nika-tui`.
- The renderer paints a `Rendered` in the workspace object region, preparing it when
  the observation, face or width changes, never on a scroll key. The Live inspection
  owner supplies one parent-only audit and byte witness: unobserved imports, skills
  and registry remain UNKNOWN, and this view never claims RUN READY.

## 4. Related

- ADR-143 (this split) · ADR-139 (the terminal renderer) · D-2026-07-09-N1
- `docs/crate-specs/nika-tui.md` · the renderer, the owner of the terminal
