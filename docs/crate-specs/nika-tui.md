# Crate spec — `nika-tui`

| | |
|---|---|
| Status | **WIP · in the workspace since 2026-09-21** (the `nika-tui-core` precedent) · Gate 1 (this document) authored 2026-08-12, amended by ADR-139 (2026-09-21 inline milestone; 2026-10-03 workspace default, one owner of the terminal) · D-2026-08-11-N6 (T27 after T28 · the renderer is to be the first native consumer of `nika-tui-core`) |
| Layer | L4 — interfaces (the native terminal surface) |
| Design | The session's Ratatui renderer (ADR-139) · ONE owner of the terminal (raw mode · bracketed paste · focus · probed keyboard protocol · alternate screen, enabled in a fixed order and restored in reverse from one place; the panic hook restores BEFORE the message) · WORKSPACE presentation first on interactive terminals (project, workflow inspection, conversation); explicit INLINE (`NIKA_TUI=inline`) keeps finished blocks in terminal scrollback · FOCUS presentation on demand (alternate screen, scrollable transcript, draft kept) · ONE event broker, paused around each cursor-position query · one composer (`ratatui-textarea` behind a wrapper: Enter sends, Alt+Enter (or Shift/Ctrl+Enter and Ctrl+J where the terminal reports them) inserts a line break, a paste is data, history at the edges of the buffer). It decides no product law: it paints and it listens. `nika-session` stays the truth and supplies what it shows as typed turn outcomes; ADR-139 assigns `nika-tui-core` to derive what a screen may claim, which is not wired yet (§1). |
| LOC budget | ≤19,000 src prod for `crates/nika-tui` only (ADR-143, 9 October 2026 amendment); other members retain their own ceilings |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited) |
| Publish | `false` |
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `ratatui` 0.30 (features `scrolling-regions` · `unstable-rendered-line-info`) · `crossterm` 0.29 (`event-stream` · `bracketed-paste` · `use-dev-tty`) · `ratatui-textarea` 0.9 · `tokio` · `unicode-width` · `serde_json` (host parsing of bytes it already holds) · `nika-fs` (bounded Live inspection) · lateral L4 `nika-session` (the live conversation), `nika-cli-host` (the one-use Run child, no admission authority), `nika-tui-view` (pure workflow and artifact faces), `nika-trace` (the canonical verifier and captured-journal fold, never back), and `nika-display` (the theme seam: roles, verb and state glyphs, motion frames; already under the other two) · dev: `expectrl` (the PTY proof), `sha2` (the logomark provenance proof). `tachyonfx` and `nika-tui-core` are not dependencies yet; ADR-139 §Consequences leaves tachyonfx and the web-studio port out of the first product. |
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
an electric-blue accent and selection fill on dark navy surfaces, blue for infer, cyan
for invoke, lavender for agent, amber for exec and attention, green success,
red failure, and readable secondary text. The renderer and viewer share the
viewer's pure visual owner through compatibility paths (ADR-143). The CLI retains
its terminal-theme palette. Under `NO_COLOR` no role carries a hue; dim and
strong remain weights. Roles and words still carry meaning without colour.
The existing 100ms busy tick turns the native orbit only while work is active;
every frame wears the one accent hue. Reduced motion keeps a still marker with
measured seconds, phase and armed-exit facts retained; idle views stay silent.
A working phase may occupy up to three wrapped rows so
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
  frames between 0 and 1,400 ms, holding the final mark without another wake, shown final at once under
  reduced motion. It never loops and never stands for work in progress.

Nothing in `visual` reads the clock, the environment or a file; the caller
passes the elapsed time, the colour and ASCII choices and reduced motion. Where
the layout places the mark and the icons is UI-LAYOUT's work.

The workspace keeps a bounded activity card from the progress updates actually
reported by Session. Consecutive repeats collapse; the latest twelve updates
remain with an explicit omission count. This is a presentation projection, not
an invented completion percentage or a replacement for the canonical receipts.
When a run's own task lines are said, the card that showed them live gives way
(unless the inline view already printed it), so each step reads once, after the
run's check and announcement; any other block leaves the card in place, settled.
Run, result, questions and the brand use the existing semantic color roles.

### The workspace screen (native entry and parent workflow inspection)

The faces of the object in view, the header's composition and the project's
data are the viewer member's (`nika-tui-view`, ADR-143 amendment of
9 October 2026); `workspace::{candidate, inspect, project}`, the review card
model and `render::own` re-export them item by item. The renderer keeps the
check facade's audit fold (`workspace::inspect::read`), the aside routing over
the live run and the review's place in the transcript.

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

The selected object's local Expand/Restore control and `F4` arrange the same
Desk, preserving input, focus, selection and reading. The header offers concrete
Project/Conversation/Object access only while the project region is folded.
Bounded separators have pointer and keyboard routes;
resize clamps the rendered geometry without changing the chosen proportions.
The Live host keeps only settled display choices in a versioned HOME preference
file through `OwnedDir`. Painting performs no I/O; restoring an arrangement
restores no Session authority. The command chooser names the conversation's
supported commands and their effect/scope. `Enter` runs the selected command
once (the palette's without touching the draft; while a turn works it waits
in the box instead) and `Tab` inserts it without submitting; a repeated
`Enter` after a surface's activation counts once (taken while that turn works,
and for half a second after it returns unless another key comes first).
`Ctrl+O` (or `Tab` in an empty composer) opens the palette. The palette, the slash list and a typed choice's offers open as one
opaque surface above the composer (`render::surface`, its plan in
`nika_tui_view::workspace::surface`): a band from the transcript's first row
down to the composer, joined to its box, sized from facts at rest, so
opening, searching or closing it moves no row and the transcript keeps its
rows and reading position under it. The band takes every pointer event over
it (a press selects, the wheel moves the selection), nothing under it takes a
key, and only an inline frame grows for it. `F2` opens a read-only reader of
the Session's complete words for
the typed question at the answer line, the current proposal under review or the
latest summarized refusal. The [workspace guide](../terminal-workspace.md) owns
every gesture and describes the distinction between inspection, Save and Run.

The check is explicitly `ParentOnly`: imports, skills and registry closure are
unobserved, and RUN READY stays UNKNOWN. Rendering is cached by observation,
face and width; every new observation, including an unread result, invalidates
it. Resize updates geometry before preparing the next frame. Opening an object
changes neither the conversation nor its attached context.

The real CLI PTY suite `workspace_pty` covers startup, the four faces and witness,
re-read after edits, resize, ASCII/no-color/reduced motion, focus and typeahead,
terminal restoration, inline/plain/pipe, inspection without effects, and Save
without Run. Its typed-question cases cover the question's words across sizes,
with the rule of how a reply is taken beside the input and no `F2` row in its
place, the complete question and proposal readers, a reply that was not taken
with newer words kept, `Answer taken` with the exact value and how it was read,
and the question's colour, `NO_COLOR` and ASCII appearance. Its compact
decision case looks for the proposal's standing, change and `when it runs`
facts beside the unsent input at 80×24, 80×30, 99×30, 80×40 and 120×40.
These proofs cover workflow inspection; the run faces below add a
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
Until this session observes a run or a gate, and while only a choice waits, the
footer tells that kept run apart from this session's facts: the rail's Run field
adds its stage (`Run ○ (earlier ✓)`) and the status row opens with it
(`last run ✓ exit 0 in an earlier session`, then the Session's own words), first
so a narrow row keeps it. Checked and Run stay this session's facts; nothing is
replayed, and a record without an exit adds nothing.
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
  columns it stands to the left of the preview (48% of the available work area,
  bounded to 42–92 columns); from 120 columns the
  project aside appears (20 to 32 columns); from 30 rows the header takes a
  second row. Below 60×16 there is no workspace and the caller keeps the focus
  presentation. The regions cover the screen exactly without overlap at 80×24,
  100×32, 120×40 and 160×48, with and without a pinned row.
  Contextual expansion keeps the conversation beside the object from 100 columns,
  at its minimum usable width; narrower views give the object more height.
  Only a strict increase in object space offers Expand. Restore stays reachable.
  Moving the wide expanded separator toward a larger conversation restores the
  regular arrangement at that requested width, preserving the same draft and object.
  On the restored stacked workspace, while the current typed decision needs
  more rows than the restored conversation offers at rest, `screen::folded`
  keeps the object to three rows (title and faces, one row, the continuation
  cue) and gives the rest to the conversation. The decision is the proposal
  the conversation reviews, or the typed question tied to the latest question
  block (`question::asked_block`); a homed question also asks three rows of the
  exchange that led to it (`QUESTION_CONTEXT`). `Desk::prepare_for` decides the
  fold once per frame from the same row plan at rest (`screen::rest_transcript`,
  `cards::rows_from`), independent of typing, the chooser and busy rows, and it
  holds only for the size, arrangement and pin it was decided for. It never
  applies outside the workspace, side by side, expanded, below the minimum, for
  a gate, a choice, a legacy question, a draft or set-aside candidate or another
  identity, or while the folded aside holds the keys. The arrangement and its
  shares are untouched and nothing about the fold is kept; another candidate
  drops it. Drawing, the extent, `F4` and the palette, the object action, the
  pointer and scrolling read the same folded geometry. `screen::object_action`
  places the action at the right end of the title row; with no room there, at
  the end of the continuation cue's row or, with no cue, of a last row the body
  leaves strictly free (fewer body lines than rows under the title), so it never
  covers a face, a line or the cue.
- `workspace::screen::masthead` composes the Session's `Place` and selected
  preparation intelligence in one header. The brand and project stand on the
  left, the intelligence on the right. A second row is a quiet rule, or holds
  the intelligence when it cannot fit beside the project. The selected name
  shortens at whole words beside `/status` when necessary; `/status` alone is
  the fallback when its labeled name cannot fit. It is a displayed fact, not a
  selector, and keeps a blank cell before the right edge or folded region names.
  Routine location, Git and governing file facts are available there. A refused `nika.yaml` remains visible.
  A missing project reads `no project`; unknown facts stay unknown. The ASCII
  column replaces renderer-owned glyphs, separators and the ellipsis.
- `workspace::aside` lists what the project holds in two projections, Nika and
  Files (the chosen one underlined), under headings derived from the entries'
  typed kinds. The selected row and object in view have distinct marks. Pointer
  routing reads the same row plan as painting; headings and notes are inert. An overflow
  ends on a `+N more` row and a listing the Session marks partial says so on its
  last row instead of pretending to show the whole disk.
- `workspace::pinned` paints the pinned run: its owning project, workflow and
  run, its state as the theme's glyph and role with the Session's words, and the
  one useful action offered. A narrow row drops the action, then cuts the
  workflow's end; the run and its state words stay.
- `visual::state` re-reads the theme's task-state column (glyph and role, both
  glyph columns) as data for Ratatui; a test pins every state to what
  `nika_display::theme::Theme::glyph` paints.
- `workspace::object` paints the preview on the right. An open object is named by its kind's
  icon and its name, and its given lines are cut at the edge, never wrapped
  (workflow faces use `nika-tui-view`; observed run faces use `workspace::live`).
  A short inspected or proposed graph uses bounded complete dependency rows for
  plain value flow; predicates, material notes or unsafe/shortened identities
  retain the detailed renderer. The same content budget drives scroll extent,
  painting and the More above/below cue; every retained line remains reachable.
  With nothing open it welcomes: the largest
  butterfly that fits whole above the onboarding words, up to 48×20 when both
  the mark and the instructions fit. The instructions explain describing an
  outcome, answering questions, reviewing, saving and then running; they give
  a concrete example and navigation keys. The mark reveals once from the
  caller's clock and appears final at once under reduced motion.
- `workspace::conversation` names who the next message goes to: the title row
  gives the thread alone, the empty composer invites the work while nothing
  waits (`Ask, change, or run… / commands`, ASCII `...`) and stays blank while
  a decision waits, and the context row
  names actual attachments separately from the viewed object. An empty attachment
  context is silent. What the next
  message carries keeps priority on a narrow panel; the on-screen part is cut
  first, then dropped. The workspace header identifies the Session's selection as
  `Prepare with:`; it does not attribute a local action or a reply to that model.
  It names the explicitly configured model, or the authoring seat's resolved
  provider model when no model was named; an unresolved default stays explicit.
  During intelligence selection the fixed composer hint names all four numbered
  routes (account, API, local, no AI), even when the menu is above the viewport.
  The selection has no second copy in the conversation and does not change its
  scroll bounds. An unknown selection stays explicit; the renderer makes no
  provider call. Beside the object, a sufficiently tall panel gives the composer
  a quiet frame and a `Your message` or `Your answer` caption (none while the
  typed question's card stands right above the line, or while a proposal or a
  gate waits: its `Save? ›` or `answer ›` prompt names the line); stacked
  and short panels retain the compact composer. Input wrapping, cursor placement and
  painting use the same inner cells. Its software cursor disappears while
  another panel has the keys and returns at the retained insertion position;
  buffer replacement preserves that focus style.
  The idle hint names sending, a new line and the command palette; panel
  navigation stays on the status row at a free prompt and is left out while a
  decision waits in the fitting workspace (below the minimum the row keeps its
  recovery note). While scrolled back, it asks the user to click the
  conversation, then press End for the latest messages: the wheel does not move
  keyboard focus. The hint fits one row at the available width; Stop, Save,
  cost questions and completion keep their own instructions.
  A lifecycle whose fields are all still pending takes no workspace row; reached
  states and earlier results remain visible. Inline and Focus keep their rail.
- `workspace::screen::draw` composes one frame from a `Screen` (place, aside,
  object, thread, pinned run): the transcript, status, composer and hint are
  painted by the same functions as the focus presentation. Beside the object
  a rule column and a blank column separate the panel; under it, the panel's
  title is a rule across. Below 60×16 it draws nothing and returns `false`, so
  the caller keeps the focus presentation.
- `workspace::focus` says which region holds the keyboard. The composer has
  it by default, so typing never needs a first move; `F6` moves to the next
  region and `Shift+F6` back (a folded aside stands over the object while it
  holds the keys), `Esc` in the aside or the object returns to the composer,
  `Esc` in the composer leaves the workspace for inline with the draft intact
  (while a turn works, it waits for the turn's end), and `Tab` stays the
  composer's completion key. In the aside the
  arrows move an underlined selection with its own marker, readable without
  colour, that the listing always shows, and `Enter` opens the entry: the object in view
  changes, the conversation does not, and nothing is attached to the next
  message. In the object the arrows and page keys scroll its lines under a
  title row that stays. On the Run face, Up/Down select a task and Enter opens
  its detail; Enter there opens a recorded child relation, and Backspace
  returns one level. `screen::extent` gives the key handler what the regions
  hold at the current size. The [workspace guide](../terminal-workspace.md)
  lists every gesture.
- Full-screen mouse capture belongs to the same terminal Owner as raw mode,
  paste and the alternate screen, and is restored on inline, exit and panic.
  The broker forwards pointer events. The wheel scrolls the actual pane under
  the pointer without taking keyboard focus; a click focuses that pane or
  opens its listed object/tab through the existing Desk route. It never submits
  text or answers consent. Shift-drag remains terminal selection where the
  terminal supports it; the complete keyboard route remains available.
  Observed activity and finished replies preserve a scrolled reading position;
  offset zero follows new content and End returns to the latest messages.
- The ASCII glyph column is the theme's decision (`--ascii`, CI logs, a legacy
  console), passed by the CLI door as `app::Options::ascii` and held in
  `UiState::ascii`: bare `nika --ascii` keeps the renderer, `--plain` and
  `NIKA_TUI=0` keep the plain loop. Under it the renderer's own glyphs take
  their twin in all three presentations: the block faces (`>`, `||`, `x`), the
  loader (`| / - \`, `*` when still), the live prompt marker, the focus rule,
  the separators of its own status and hints, and the door's title
  (`nika - <project>`). One renderer text keeps its `·` so far: the
  « action required » title suffix. The echo of a sent line repeats the waiting
  prompt in its ASCII twin (`nika > `). The CLI proof checks a listed set of
  renderer glyphs; it does not claim an all-ASCII frame. The Session's words
  (the banner, replies, the
  status line, the lifecycle rail) are shown as written, never rewritten, so
  an ASCII frame still carries their `·` and `○`.

### Typed clarification and conversation groups

The native host captures the compiler's question document from one `Work`
snapshot. Its label, reason, required flag, ordered offers and exact offer keys
remain compiler facts. A painted question routes both a selected offer and the
human's own words through `answer_question_for` with the captured strong
`QuestionId`, never the general submission door. The native painted token has a
runtime-local presentation scope; matching serialized words from another live
Session cannot substitute for the strong identity, whose equality also includes
its incarnation. Untyped Run/cost questions keep their existing owners and doors.

A typed choice opens the continuous surface above the line (choice mode):
`Question`, `required` and the page of offers when they do not all show, then
what the Session keeps of the request the question serves, from the same
`Work` snapshot (`model::Retained`: the goal as kept, then the other questions
still open; it grants nothing), then the Session's exact words under one
frame-aware cap (`question::word_cap`: six rows, or a quarter of the frame's
rows when that is more, past it a `… the whole question: F2` row), then the
offers on whole pages. The ordinary draft waits out of view, untouched with
its caret and selection, and the line shows the choice's own reply field. The
offers take keys whatever the draft holds (`Up`, `Down`, `Home`, `End`);
`Enter` sends the own reply when it holds words, else the selected offer's
exact key, once, and the offers are inert while that answer is in flight; a
press selects without sending. An own reply not taken comes back to its
field. The palette opened over a typed choice suspends it and `Esc` restores
it, selection and own reply kept; no slash list opens over it. Keys other than
the shell's own (`Ctrl+C`, `Ctrl+O`, `Ctrl+T`, `Ctrl+L`, `Shift+Tab`, the
function keys) are the choice's, so none reaches the hidden transcript or
leaves the workspace.

A line the Session holds because the knowledge the configuration names was
refused (`Waiting::Knowledge`, from `work::Waiting::KnowledgeChoice`) opens
the same surface: `Knowledge · your message waits`, the held line as the
request kept, the refusal in the Session's words (its source, layer, code and
cause), then its two acts as offers, `/knowledge embedded` (resumes the line
once with the release built into Nika) and `cancel` (drops it). Each act is
sent as the exact ordinary line the Session reads; no identity is bound.

A typed question that offers nothing (a text, an exact value) keeps its card
above the answer line. In the fitting workspace, while the latest question
block carries the witness of the typed question waiting, that card is the
question's home: `Question` in the accent, its shape and `required`, then the
Session's words under the same cap. `Share` gives the words every row while
they pass the cap by at most one row and the card has the room; otherwise it
shows the rows that fit, at most the cap, then a `… the whole question: F2`
row, so that row always stands for at least two unread rows; a card with room
for one word row ends it with `… F2`. Painting, measuring and a decision's
rest demand read that one cap and share. The transcript reads one quiet row
(`↓ the question waits below`) where that block stands, the block untouched.
While a turn works, in inline and focus, with too few rows, for an untagged,
stale or other-identity block, or when a summarized block follows it, the
transcript keeps every word; a surface over the card hides it and moves none
of its rows. Every line sent while a typed question is painted, a command or
an empty line included, takes the identity door; the Session refuses an empty
line unless the question offers a default.

While that card is painted, it takes over only the live rows that repeat it
(`render::Lent`): the lifecycle rail when it is exactly a first question's
`Draft ● · Saved ○ · Checked ○ · Active ○ · Run ○`, and the status row when it
is exactly the Session's `Needs one answer · <label>` for that question and no
exit is armed. Any other rail or status, including a reached state, an earlier
run, a cost, a gate or words unknown here, keeps its row, and nothing is parsed
out of either; demand, painting and the rest rows read the one rule. A current
proposal keeps its rail.

The public `Waiting::Question { key }` constructor retains its legacy shape.
Typed documents use the additive, non-exhaustive `QuestionDocument` variant,
constructed with `Waiting::asked(key, asked)`. A legacy question never acquires
identity-bound offer routing. The default `Conversation::answer_bound` sends
nothing and returns the exact input as `Beat::NotTaken`; a host must implement
its own identity-bound door to take it.

The reply returned as `Beat::NotTaken` preserves its exact bytes ahead of any
newer unsent draft, or leaves its offered choice selected. It is not replayed.
The host's retention rule compares the full pending strong identity;
question closure and a final refusal do not prove binding, since a compilation
refusal can follow an accepted answer. The shell displays no Applied claim from
that inference. The shared Session remains the owner of definitive answer facts.
When the work snapshot carries an answer act bound to the same strong identity
and key, the shell shows `Answer taken`, the question's label, the exact value
and its source, when known, before the turn's other blocks; it claims no Save or
Run, and retention still follows the strong identity alone.

While the Session waits for consent on the proposal whose identity is the
candidate's own, that proposal's card reads as the candidate's typed review
(`workspace::cards::review`): what a `yes` answers first, then every change,
the count of changes whose bytes no face shows, the revision rows, one
`when it runs` group, the declared reach and the rehearsal, each once with its
role, then a quiet footnote with the exact proposal identity and the pending
bytes' witness prefix, and where `F2` reads the Session's whole words. The
object's face then leaves those facts to the review. Recognition is identity
alone: an untagged, older or other proposal, a draft or a set-aside candidate
keeps the Session's words. While a proposal waits, the status row reads
`Not saved yet · yes means Save only`; it adds no control and changes no request or effect
authority.

Contiguous blocks from one speaker share a bubble, using the existing semantic
roles. You stands to the right on a raised surface; Nika stands to the left.
Roomy transcript regions use quiet outlines and short regions use slabs. Every
piece is measured and painted from the same row plan and rectangle, so wrapping,
scrolling, cell widths and speaker grouping agree. Questions, proposals, gates
and refusals retain a distinct boundary. The transcript keeps all original words;
visual grouping changes no identity, dispatch or effect authority. A decision
or refusal card entered part-way names itself on its first visible row with
the count of its rows above (`↑ N rows`, ASCII `^ N rows`), when that row has
room for both; heights and scroll bounds are unchanged. At its live position a plan shorter than the transcript
stands on the transcript's last row, its empty rows above it (`cards::lift`),
so the latest piece touches the decision under it; scrolled back, or longer
than the area, nothing moves, and manual reading, the scroll bounds and every
offset stay as measured.

### Stop and corrections

The conversation arms one preparation stop per turn from the Session. While a
preparation works, the first `Ctrl+C` asks it to stop and arms the second
press, which leaves with the terminal restored; `Enter` with words in the box
asks the same stop and queues those words as a correction. A command the
conversation knows is never a correction: it waits in the box. A queued
correction is sent as the next line only at the free prompt; when a decision is
on screen it returns to the box unsent, and under a spending question it is kept
whole in the conversation instead. Once a turn hands a Run to its runner, this
key stops nothing: the hint says so and a second press leaves. At rest,
`Ctrl+C` declines a pending Run review or unknown-cost choice, and otherwise
arms the press that leaves. The [workspace guide](../terminal-workspace.md)
states the user-facing contract.

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

Other question hints ask for an answer or cancellation without promising a
blank-line default: the question's key alone cannot establish one. Before a
proposal response is interpreted, its busy label remains neutral (`reviewing
your reply`); cancellation, inspection and revision are not announced as Save.
Only the existing observed progress reports describe the work actually begun.

## Opening refusal and terminal restoration

A conversation that refuses and quits while opening (for example, another
instance owns the history lease) is an error, not a normal user quit. The shell
stops its input broker and returns that refusal through the existing error path.
The terminal owner restores all modes before the CLI prints the diagnostic on
stderr and returns its environment-error exit code. The message therefore stays
visible after a fullscreen launch closes. This path never steals a lease, clears
history, submits a draft or signals the existing instance. An ordinary refusal
inside an open conversation remains a card; a normal user quit remains successful.

## Reported harness images

The run fold counts every `agent_image_observed` frame of a task's current leg
beside its unchanged text output and keeps at most four detail rows; the
terminal's `harness_media_count` closes the sequence. A new attempt or cache
hit clears the leg's media. Task details distinguish locally stored received
bytes, bytes with unconfirmed storage, and reported-path-only observations,
naming MIME, received size, the blob locator when present and the harness
source; the peer's file remains unverified. When more frames exist than rows
shown, the detail says `Showing N of TOTAL` and points to the trace; a count
mismatch, a missing count or a malformed frame is reported as incomplete
evidence, never as a complete result. Reading these details performs no file
read, copy, fetch or image generation. Inline raster display is not implied.

### Turn worker stack

The shell's `nika-tui-turn` worker explicitly reserves 8 MiB of stack for the complete synchronous Session entry point and the nested Compiler/provider future it polls. The UI input owner stays separate. This addresses ordinary nested authoring frames overflowing the smaller platform worker stack; it does not add a preparation limit, alter Stop semantics, grant authority, or change the provider. The hermetic regression exercises the same worker constructor with more than 3 MiB live and checks return of its owned conversation across consecutive turns. A real ACP replay remains a separate integration check.
