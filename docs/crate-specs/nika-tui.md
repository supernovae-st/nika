# Crate spec — `nika-tui`

| | |
|---|---|
| Status | **WIP · in the workspace since 2026-09-21** (the `nika-tui-core` precedent) · Gate 1 (this document) authored 2026-08-12, amended by ADR-139 (2026-09-21 inline milestone; 2026-10-03 workspace default, one owner of the terminal) · D-2026-08-11-N6 (T27 after T28 · the renderer is to be the first native consumer of `nika-tui-core`) |
| Layer | L4 — interfaces (the native terminal surface) |
| Design | The session's Ratatui renderer (ADR-139) · ONE owner of the terminal (raw mode · bracketed paste · focus · probed keyboard protocol · alternate screen, enabled in a fixed order and restored in reverse from one place; the panic hook restores BEFORE the message) · WORKSPACE presentation first on interactive terminals (project, workflow inspection, conversation); explicit INLINE (`NIKA_TUI=inline`) keeps finished blocks in terminal scrollback · FOCUS presentation on demand (alternate screen, scrollable transcript, draft kept) · ONE event broker, paused around each cursor-position query · one composer (`ratatui-textarea` behind a wrapper: Enter sends, Alt+Enter inserts a line break, a paste is data, history at the edges of the buffer). It decides no product law: it paints and it listens. `nika-session` stays the truth and supplies what it shows as typed turn outcomes; ADR-139 assigns `nika-tui-core` to derive what a screen may claim, which is not wired yet (§1). |
| LOC budget | ≤6,000 src prod · ≤15,000 hard cap |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `ratatui` 0.30 (features `scrolling-regions` · `unstable-rendered-line-info`) · `crossterm` 0.29 (`event-stream` · `bracketed-paste`) · `ratatui-textarea` 0.9 · `tokio` · `unicode-width` · `nika-fs` (bounded Live inspection) · lateral L4 `nika-session` (the live conversation), `nika-cli-host` (the one-use Run child, no admission authority), `nika-tui-view` (pure workflow and artifact faces), `nika-trace` (the canonical verifier and captured-journal fold, never back), and `nika-display` (the theme seam: roles, verb and state glyphs, motion frames; already under the other two) · dev: `expectrl` (the PTY proof), `sha2` (the logomark provenance proof). `tachyonfx` and `nika-tui-core` are not dependencies yet; ADR-139 §Consequences leaves tachyonfx and the web-studio port out of the first product. |
| NIKA codes | none owed — the renderer refuses nothing · it displays the refusal the engine rendered |
| Depends on | **T28 admitted** (`nika-tui-core` out of wip, done 2026-08-14) · ADR-139 records the original renderer milestone and the 2026-10-03 workspace amendment |

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

The roles remain the engine's closed set, `nika_display::theme::Role`
(the accent, the three verdicts, dim, strong and the four verb chips).
`visual::role::style` resolves them to the workspace's RGB product palette:
blue activity, green success, amber attention, red failure, and readable
secondary text. The viewer member pins the same RGB values. The CLI retains
its terminal-theme palette. Under `NO_COLOR` no role carries a hue; dim and
strong remain weights. Roles and words still carry meaning without colour.
The existing 100ms busy tick drives the native orbit and its blue/cyan/purple
accent only while work is active. Reduced motion keeps a still marker; idle
views do not animate. A working phase may occupy up to three wrapped rows so
its model and completed phase remain visible without an invented percentage.

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

### The workspace screen (native entry and parent workflow inspection)

Bare `nika` on an interactive terminal opens the workspace. `NIKA_TUI=inline`
selects the earlier inline presentation; plain and pipe behavior stay available.
Below 60×16, focus presentation preserves the conversation. The Live host adapter
lists the Session project, opens only a listed workflow below its owned root,
and resolves that selected project root once. Below the held root it reads at
most 1 MiB of UTF-8, refusing child symlinks. Source, Plan, Graph
and Check share the same byte witness and one `audit_source` result. Inspection
runs before drawing and opens no consent, Save or Run authority. For a workflow with
`infer:`/`agent:` tasks, the readiness judgement may observe provider key presence and,
in a harness build, run the installed agent CLIs' authentication status probes; it never
calls a model.

The check is explicitly `ParentOnly`: imports, skills and registry closure are
unobserved, and RUN READY stays UNKNOWN. Rendering is cached by observation,
face and width; every new observation, including an unread result, invalidates
it. Resize updates geometry before preparing the next frame. Opening an object
changes neither the conversation nor its attached context.

The real CLI PTY suite `workspace_pty` covers startup, the four faces and witness,
re-read after edits, resize, ASCII/no-color/reduced motion, focus and typeahead,
terminal restoration, inline/plain/pipe, inspection without effects, and Save
without Run. These proofs cover workflow inspection; the run faces below add a
separate result and evidence slice, not complete workspace qualification.

The run object offers Run, Outputs, Files and Proof. Outputs come from the
resolved map recorded beside that leg's terminal settlement. Files show at most eight reported writes, read now at up to
1 MiB each; without a digest of the bytes written, the view claims neither
unchanged nor changed since the run. The host acquires files and Proof on its
worker, outside drawing, and applies a result only to the same execution and
reading generation. Typing and drawing remain available during acquisition.

Proof captures at most 8 MiB of the named journal once. `RunFacts::of` and
`trace_verify::verify_captured` consume those same bytes. Binding requires exactly
the observed execution, one start naming its source hash, and the receipt head
and length; missing or conflicting identities remain unbound. The journal
witness does not cover the verifier's separately acquired custody keys, anchor
sidecar or writer lease. Run status, a declared seal and a verified verdict are
separate observations; none proves the requested business result correct.

The Run face lets the user select a task, open its detail and return to the
list. Selection is bound to the execution and task id, and stays visible after
a height-only resize. Detail distinguishes observed state, failures, measured
usage and output from missing observations. Graph facts are added only when
the run names the exact source shown; a declared task without an event stays
not observed. Inspection adds no file access, execution or consent.

From a task whose admitted settle frame names a child, Enter opens that child's
journal one level down. The host reads at most 8 MiB through the held project
root; the target's displayed words are never a fallback path. Verification and
the child view consume the same captured bytes. Head, source and outcome are
compared only with the parent's recorded commitments; absent or contradictory
facts stay explicit. The child's execution identity and length stay not
compared. Each opening asks its own read; a late answer from an earlier opening
is discarded. Keys remain available during acquisition, and Backspace returns
to the parent with its selection and scroll. This view starts no child work and
adds no child usage to the parent's measurements. Live child frames, a produced
child execution identity and a failed-child summary remain outside this slice.

Reopening repaints retained turns as history and exposes the last observed run.
Opening Run, Outputs or Files first captures and verifies its journal once.
Only a bound, verified reading accepted by the Desk and adopted by the host
lends its task rows, terminal outputs, reported write names and child relations.
These observations come from the captured bytes, never from today's workflow.
An older terminal without an outputs map stays absent, not an empty map. A
refused capture revokes the earlier lending even for the same journal bytes.
If the host declines adoption, the historical projection is withdrawn while
Proof keeps its verdict, witness and reason; it does not trigger a refresh
loop. Two admitted captures of identical bytes share a witness. Revocation
bounds later reads; a read already in flight is not cancelled. It calls no
model, starts no run and restores no consent. Full child
hierarchy, project/conversation switching, concurrent
revision during a run and the complete paid journey remain outside this slice's
qualification. The workspace PTYs use cargo-test binaries; they do not qualify
a stamped integrated build or a paid model route.

- `workspace::geometry::Geometry::of` places the header, the project aside,
  the object in view, the conversation with its composer and the pinned
  activity row. The composer and the object come first: below 100 columns the
  conversation sits under the object and keeps at least half the rows; from 100
  columns it stands beside the object (36 to 56 columns); from 120 columns the
  project aside appears (20 to 32 columns); from 30 rows the header takes a
  second row. Below 60×16 there is no workspace and the caller keeps the focus
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
  (workflow faces use `nika-tui-view`; observed run faces use `workspace::live`). With nothing open it welcomes: the largest
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
  the caller keeps the focus presentation.
- `workspace::focus` says which region holds the keyboard. The composer has
  it by default, so typing never needs a first move; `F6` moves to the next
  region and `Shift+F6` back (a folded aside is skipped), `Esc` returns to the
  composer, and `Tab` stays the composer's completion key. In the aside the
  arrows move a reversed selection (a weight, readable without colour) that
  the listing always shows, and `Enter` opens the entry: the object in view
  changes, the conversation does not, and nothing is attached to the next
  message. In the object the arrows and page keys scroll its lines under a
  title row that stays. On the Run face, Up/Down select a task and Enter opens
  its detail; Enter there opens a recorded child relation, and Backspace
  returns one level. `screen::extent` gives the key handler what the regions
  hold at the current size.
- The ASCII glyph column is the theme's decision (`--ascii`, CI logs, a legacy
  console), passed by the CLI door as `app::Options::ascii` and held in
  `UiState::ascii`: bare `nika --ascii` keeps the renderer, `--plain` and
  `NIKA_TUI=0` keep the plain loop. Under it the renderer's own glyphs take
  their twin in all three presentations: the block faces (`>`, `||`, `x`), the
  loader (`| / - \`, `*` when still), the live prompt marker, the focus rule,
  the separators of its own status and hints, and the door's title
  (`nika - <project>`). Two renderer texts keep their `·` and `›` so far: the
  « action required » title suffix, and the echo of a sent line, which repeats
  the waiting prompt as written. The Session's words (the banner, replies, the
  status line, the lifecycle rail) are shown as written, never rewritten, so
  an ASCII frame still carries their `·` and `○`.

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
   (`TERM=dumb`, or an explicitly inline terminal that never answers the cursor-position report),
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
- Painting and the viewers perform no I/O. The Live host adapter additionally
  reads a listed parent workflow through `nika-fs` before preparing inspection
  (bounded, no symlink; see above). Session mutations remain in `nika-session`;
  runs go through the runners the CLI door injects.

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
