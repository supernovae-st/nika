# Crate spec — `nika-tui`

| | |
|---|---|
| Status | **WIP · in the workspace since 2026-09-21** (the `nika-tui-core` precedent) · Gate 1 (this document) authored 2026-08-12, amended 2026-09-21 by ADR-139 (the renderer architecture: inline-first, one owner of the terminal) · D-2026-08-11-N6 (T27 after T28 · the renderer is to be the first native consumer of `nika-tui-core`) |
| Layer | L4 — interfaces (the native terminal surface) |
| Design | The session's Ratatui renderer (ADR-139) · ONE owner of the terminal (raw mode · bracketed paste · focus · probed keyboard protocol · alternate screen, enabled in a fixed order and restored in reverse from one place; the panic hook restores BEFORE the message) · INLINE presentation first (`Viewport::Inline` + `insert_before` with scrolling regions: finished blocks live in the terminal's scrollback) · FOCUS presentation on demand (alternate screen, scrollable transcript, draft kept) · ONE event broker, paused around each cursor-position query · one composer (`ratatui-textarea` behind a wrapper: Enter sends, Alt+Enter inserts a line break, a paste is data, history at the edges of the buffer). It decides no product law: it paints and it listens. `nika-session` stays the truth and supplies what it shows as typed turn outcomes; ADR-139 assigns `nika-tui-core` to derive what a screen may claim, which is not wired yet (§1). |
| LOC budget | ≤6,000 src prod · ≤15,000 hard cap |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `ratatui` 0.30 (features `scrolling-regions` · `unstable-rendered-line-info`) · `crossterm` 0.29 (`event-stream` · `bracketed-paste`) · `ratatui-textarea` 0.9 · `tokio` · `unicode-width` · lateral L4 `nika-session` (the live conversation), `nika-cli-host` (the one-use Run child, no admission authority) and `nika-display` (the theme seam: roles, verb and state glyphs, motion frames; already under the other two) · dev: `expectrl` (the PTY proof), `sha2` (the logomark provenance proof). `tachyonfx` and `nika-tui-core` are not dependencies yet; ADR-139 §Consequences leaves tachyonfx and the web-studio port out of the first product. |
| NIKA codes | none owed — the renderer refuses nothing · it displays the refusal the engine rendered |
| Depends on | **T28 admitted** (`nika-tui-core` out of wip, done 2026-08-14) · ADR-139 proposed (confirmed or overturned by the two UX-1 prototypes on the same fixtures) |

---

## 1. Purpose

The native terminal is the surface that cannot lie: a grid of cells, one
character and one style per cell, nothing else. The web studio was written
to be ported (the same buffer model, effects after writing). This crate is
that port, and the map exists (`PORTING.md`, the studio's single source of
truth).

What sets it apart from a rewrite: **it invents no law**. In the target
design, the session model, the derivations (waves · bottleneck · totals),
the board's cell law and the executable claims come from `nika-tui-core`,
compiled natively. This crate then holds exactly what the browser cannot
provide: the event loop (`crossterm::event::read`), the terminal geometry
(the real columns — none of the studio's four measurement errors carries
over), the ratatui widgets, and the two tachyonfx effects.

Today the crate renders the live Session: `nika-session` turn outcomes
become typed beats, and the CLI door injects the runners. The
`nika-tui-core` board law and the tachyonfx effects are not wired yet.

## 2. The semantic layer becomes enforceable here

The known hole in the porting map (§4) closes in this crate, without a second
palette: the roles are the engine's closed set, `nika_display::theme::Role`
(the accent, the three verdicts, dim, strong and the four verb chips), and
`visual::role::style` resolves each at paint time to the Ratatui colour of the
same ANSI-16 slot the CLI frames paint (a test pins the ten slots to the
theme's own SGR codes). Hues stay the user's terminal theme's; without colour
no role carries a hue, and dim and strong remain weights. The renderer's block
faces, status marker and prompt marker ask for a role, never a colour: the busy
marker wears the accent (cyan), a gate or a proposal the warning slot, a
refusal the failure slot. The studio's palette-extent gate and the board roles
(`BarWork`, `BarIdle`, `BarCritical`) arrive with the board.

The rest of the visual vocabulary (`visual`, task T-nika-tui-assets) is the
same kind of borrowing:

- `visual::icon` names the workspace objects a screen shows (project,
  workflow, conversation, run, activation, file, memory, connection,
  settings, pinned, search, choose) with a label that is always drawn, a
  Unicode glyph drawn only when it takes one cell in both the narrow and the
  CJK width tables (three proposals, the run, file and memory glyphs, fall
  back for that reason), and an ASCII twin. Verbs and task states are not
  icons: their glyphs are the theme seam's `◇ ▷ ◆ ✦` and state column.
- `visual::logomark` holds the Supernovae butterfly, the only brand mark:
  five renditions (12×6 to 48×20) sampled from `media/brand/nika-logomark.svg`
  (a test pins its sha256, so a changed mark flags stale renditions), chosen
  whole by `Size::largest_within`, revealed once through five ordered-dither
  frames between 0 and 600 ms and final at 760 ms, shown final at once under
  reduced motion. It never loops and never stands for work in progress.

Nothing in `visual` reads the clock, the environment or a file; the caller
passes the elapsed time, the colour and ASCII choices and reduced motion. Where
the layout places the mark and the icons is UI-LAYOUT's work.

### The workspace screen (T-nika-tui-layout · in progress)

`workspace` builds the full-terminal screen on fixtures; nothing opens it yet,
and the inline presentation and the plain loop are unchanged.

- `workspace::geometry::Geometry::of` places the header, the project aside,
  the object in view, the conversation with its composer and the pinned
  activity row. The composer and the object come first: below 100 columns the
  conversation sits under the object and keeps at least half the rows; from 100
  columns it stands beside the object (36 to 56 columns); from 120 columns the
  project aside appears (20 to 32 columns); from 30 rows the header takes a
  second row. Below 60×16 there is no workspace and the caller keeps the inline
  presentation. The regions cover the screen exactly without overlap at 80×24,
  100×32, 120×40 and 160×48, with and without a pinned row.
- `workspace::header` paints where the human stands from a `Place` the Session
  projects: the active project (icon, name, chevron), its location, the host,
  then the observed facts (git or no git, `nika.yaml` or no `nika.yaml`); an
  unobserved fact is not written, a missing project reads `no project`. A narrow
  row cuts the location from its start, never the project name; the ASCII column
  replaces glyphs, separators and the ellipsis.
- `workspace::aside` lists what the project holds in two projections, Nika and
  Files (the chosen one underlined), with the object in view marked; an overflow
  ends on a `+N more` row and a listing the Session marks partial says so on its
  last row instead of pretending to show the whole disk.
- `workspace::pinned` paints the pinned run: its owning project, workflow and
  run, its state as the theme's glyph and role with the Session's words, and the
  one useful action offered. A narrow row drops the action, then cuts the
  workflow's end; the run and its state words stay.
- `visual::state` re-reads the theme's task-state column (glyph and role, both
  glyph columns) as data for Ratatui; a test pins every state to what
  `nika_display::theme::Theme::glyph` paints.
- `workspace::object` paints the centre. An open object is named by its kind's
  icon and its name, and its given lines are cut at the edge, never wrapped
  (the viewers of T-nika-tui-viewers will paint graphs, sources, diffs, checks,
  results and proofs there). With nothing open it welcomes: the largest
  butterfly that fits whole above the Session's first words (16×8 in the 80×24
  object rows, 48×20 from 120×40), revealed once from the caller's clock, final
  at once under reduced motion.
- `workspace::conversation` names who the next message goes to: the title row
  gives the thread and its project, the composer's placeholder the full
  recipient (`Message to studio / release checklist`), and the context row
  keeps apart what is only on screen and what is attached. What the next
  message carries keeps priority on a narrow panel; the on-screen part is cut
  first, then dropped.
- `workspace::screen::draw` composes one frame from a `Screen` (place, aside,
  object, thread, pinned run): the transcript, status, composer and hint are
  painted by the same functions as the focus presentation. Beside the object
  a rule column and a blank column separate the panel; under it, the panel's
  title is a rule across. Below 60×16 it draws nothing and returns `false`, so
  the caller keeps the inline presentation. The live-area rows (status, hint,
  block glyphs) have no ASCII twins yet: the chrome is ASCII in the ASCII
  column, the transcript and the live area are not.

## 3. What is ported as is (the map, §5 · planned)

- `sweepOver` is NOT `fx::sweep_in`. Zero opacity ahead of the front is
  right for something that arrives and wrong for something being watched;
  the variant moves only a live head.
- It walks the INK, not the columns: a head advancing in `x` falls into
  blank space halfway (measured · the gesture flickers).
- The studio's 9 goldens are the RENDERING proof. The crate reproduces them
  character for character (the goldens harness moves here).

None of these is implemented yet: ADR-139 leaves tachyonfx and the web-studio
port out of the first product.

## 4. Implementation order (ADR-139 · the product waves)

The product waves replace the porting map's order (generated contract ·
buffer · wire · cascade · tachyonfx). Each wave is finished when its complete
scenario is qualified on the REAL binary, never when the code exists:

1. **UX-1 · the renderer proof** (2026-09-21) · the Ratatui shell, both
   presentations (inline · focus) on the same fixture (`Script::demo`), the
   composer spike, and the terminal lifecycle proven from a PTY
   (`tests/pty_restore.rs`: normal close · two Ctrl+C · a panic in the loop ·
   SIGTERM · a pasted `yes`/`/quit` inert across a focus switch · a pipe
   refused with code 2 and zero escape sequences).
2. **UX-2 · the first five seconds** · the real `SessionRuntime` wired through
   the same typed beats · the first screen, local help, latency. The explicit
   switch is retired: bare `nika` on a real terminal opens the renderer, and
   `nika --plain` or `NIKA_TUI=0` keeps the plain loop. The plain loop is also
   the automatic fallback when the renderer cannot take the terminal
   (`TERM=dumb`, a terminal that never answers the cursor-position report),
   said once on stderr.
3. **UX-3 · contextual cognition and recovery** · the intelligence picker,
   typed recovery.
4. **UX-4 · the living workflow object** · typed clarification, review in
   the mandated order, inspector, exact save, Check.
5. **UX-5 · execution** · run, gate, resume, result, proof.
6. **UX-6 · hardening** · the terminal matrix, sizes, tmux, SSH, `TERM=dumb`,
   monochrome. Recorded PTY evidence so far: the renderer opens, helps and
   closes at 60×20, 80×24 and 120×40; a resize while a proposal waits
   re-anchors the viewport and redraws the consent prompt; `TERM=dumb` writes
   no cursor query and no CSI sequence; a mute cursor report falls back within
   the bounded wait.
7. **UX-7 · human qualification** · goldens A to O, dogfood, the moderated
   study.

The conversational delivery A qualification
([docs/qa/delivery-a-2026-09.md](../qa/delivery-a-2026-09.md)) started bare
`nika` on one macOS installation. It is evidence for those journeys, not the
UX-7 human qualification.

## 5. Determinism contract

- The same session state gives the same buffer. Painting is pure; the clock
  enters only through effects (in the target design tachyonfx carries time,
  and widgets never read it).
- The crate's own code performs no I/O beyond the event loop and the
  terminal. Engine reads and writes belong to the session runtime
  (`nika-session`); runs go through the runners the CLI door injects.

## 6. Related

- `docs/crate-specs/nika-tui-core.md` · the law (T28 · wip `c5c8f96cc`)
- the studio's porting map (its single source of truth · the correspondence
  table · the two accepted divergences with tachyonfx · the order)
- the studio's 9 goldens · the rendering proof to reproduce
- D-2026-08-11-N6 · the ordering decision
- [docs/usage/conversational-session.md](../usage/conversational-session.md) ·
  the user guide to the Session this renderer shows

## Fresh local Run cost decision

`session::Live::with_run_review` accepts the existing CLI host's typed child
runner through an acyclic L4 dependency (`nika-tui` → `nika-cli-host`, never the
reverse). A pending Run question is separate from Session authoring and Save
consent, survives only while that child is alive, and is never persisted.
The broker discards input queued before the question is painted. A new `yes`
answers only this question; `no`, cancellation, revision and leaving drop the
child. Ctrl+C invalidates a pending decision immediately. Native catalog
currency evidence and unknown USD remain distinct; the renderer invents no
price, policy exception, endpoint, grant or reusable admission authority.

The Session's one-time unknown-cost choice (`SessionRuntime::waiting_cost_choice`)
is the second fresh spending question and keeps the same broker contract:
typeahead from before it was painted is discarded, Ctrl+C cancels it through the
Session's own answer path (nothing sent), and `details` reads
`cost_choice_details` without a turn. Its first screen is headed as an authoring
decision that never approves a Save or a Run; the Run question approves one Run
that no authoring or Save approval does. Both first screens close on
`yes / no / details`, and the hint row names the same choices in words.
