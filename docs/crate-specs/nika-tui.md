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
| Dependencies | **read from `Cargo.toml`, which is authoritative** · `ratatui` 0.30 (features `scrolling-regions` · `unstable-rendered-line-info`) · `crossterm` 0.29 (`event-stream` · `bracketed-paste`) · `ratatui-textarea` 0.9 · `tokio` · `unicode-width` · lateral L4 `nika-session` (the live conversation) and `nika-cli-host` (the one-use Run child, no admission authority) · dev: `expectrl` (the PTY proof). `tachyonfx` and `nika-tui-core` are not dependencies yet; ADR-139 §Consequences leaves tachyonfx and the web-studio port out of the first product. |
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

## 2. The semantic layer becomes enforceable here (planned)

The known hole in the porting map (§4) closes in this crate:

```rust
pub enum Role { BarWork, BarIdle, BarCritical, /* … */ }
impl Role { pub fn color(self, theme: &Theme) -> Color { /* the table */ } }
```

A `Role` resolved at paint time makes the semantic layer enforceable:
citing a palette primitive in a widget becomes a type error. The studio's
palette-extent gate then measures what it claims to measure. `Role` is not
implemented yet.

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
