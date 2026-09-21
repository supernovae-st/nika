---
id: ADR-139
title: "The terminal renderer: Ratatui, inline-first, one owner of the terminal"
status: proposed
date: "2026-09-21"
phase: "pre-1.0 · product convergence · terminal experience UX-1"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "tui", "session", "terminal"]
affects_crates: ["nika-tui", "nika-tui-core", "nika-session", "nika-cli"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-125", "ADR-133"]
requires: []
enables: []
amends: []
---

# ADR-139 · The terminal renderer: Ratatui, inline-first, one owner of the terminal

## Context

Bare `nika` on an interactive terminal opens the native session (ADR-125):
a sentence becomes a workflow the human clarifies, reviews, accepts as
exact bytes, checks and runs, with a gate answered inline. Today that
session prints plain lines through the CLI's line loop
(`nika-cli/src/verbs/session.rs`): no viewport, no focus mode, no owner of
the terminal, nothing restored after a panic. The product direction
(« the chat fades, the workflow remains ») asks for a terminal experience
where five components are made excellent before any sixth surface:
Composer · Contextual Intelligence Picker · Clarification · Workflow Review
· Run/Gate/Result. Every one of them rides the renderer, and a broken
terminal after a crash is P0.

`nika-tui-core` (admitted 2026-08-14) already owns the presentation LAW:
the session model, the derivations, the plan-board seating and the
executable claims, compiled native and to WASM. Its first native consumer,
`nika-tui`, has a candidate spec (2026-08-12) written for a port of the
web studio (a cell buffer, effects after writing, a `Block` enum, tachyonfx)
that predates the product convergence. This decision replaces that spec's
implementation order with the product's waves and fixes the renderer's
architecture; it does not change the law crate.

Research (the renderer ledger, 2026-09-21): Codex's TUI (inline-first, its
scrollback IS the transcript, one `EventBroker`, `set_modes`/`restore_common`
in a fixed order, a panic hook tested from a real PTY), OpenCode and Crush
(composer semantics: `Enter` sends, a modifier breaks a line, history at the
buffer's edges, a paste normalised as data), ratatui 0.30 (`Viewport::Inline`
anchored on a cursor-position report, `Terminal::insert_before`, the
`scrolling-regions` feature that spares the repaint, `try_init_with_options`
without the alternate screen), crossterm 0.29 (one input reader behind one
lock, bracketed paste, focus events, the kitty keyboard protocol
probed), `ratatui-textarea` 0.9.2 (the maintained fork with word wrap; the
original `tui-textarea` is frozen at ratatui 0.29).

## Decision

1. **Ratatui 0.30 + Crossterm 0.29, Rust all the way down.** One binary,
   no second runtime. `nika-session` stays the truth, `nika-tui-core`
   derives what a screen may claim, `nika-tui` paints and listens. The
   renderer consumes typed beats (`Say` a block · `Wait` for a named kind
   of line · `Busy` under a seat · `Quit`), whose shapes mirror
   `nika_session::runtime::TurnOutcome` one to one, and emits lines the
   session runtime already reads. Nothing parses stdout or invents a state.

2. **Inline first.** The default presentation is `Viewport::Inline`: a
   finished block (a proposal accepted, a run's result) is pushed into the
   terminal's own scrollback through `insert_before` with the
   `scrolling-regions` feature, so history stays copyable and tmux, SSH
   and the shell's own scrolling stay ordinary; the viewport holds only the
   live area (status · the prompt that names what waits · the composer ·
   one hint). The focus presentation (the alternate screen with the
   transcript scrollable) is entered on request (`Ctrl+T`) and left
   (`Esc`) with the draft intact and the blocks finished meanwhile pushed
   to the scrollback at that moment. One state, one component set, two
   presentations.

3. **One owner of the terminal.** Raw mode, bracketed paste, focus change
   (unix), the keyboard-enhancement flags (probed) and the alternate screen
   are enabled in exactly one place in a fixed order and restored in the
   reverse order from exactly one place, remembering what was enabled so a
   blind restore never sends a pop to a terminal that saw no push. The
   panic hook restores BEFORE the panic prints; the normal close, two
   `Ctrl+C` while idle, a panic inside the loop and `SIGTERM` all reach the
   same restore. The renderer refuses anything that is not an interactive
   terminal (a pipe, a redirect, `TERM=dumb`): the CLI keeps its plain line
   loop there, byte for byte, with the existing PTY goldens.

4. **One event broker, parked around every cursor-position query.** One
   thread reads the terminal in short `poll` slices into one channel of
   typed events; `SIGTERM` and a signalled `SIGINT` arrive on the same
   channel. Because crossterm keeps one input reader behind a lock, and
   because every inline viewport computation (entry, resize, the switch
   back from focus) asks the terminal where the cursor is within two
   seconds, the reader parks before such a query and reads again after;
   `pause` returns only once the thread has acknowledged. Not crossterm's
   `EventStream`: its reader thread holds the lock while it blocks and
   releases it some time after the stream is dropped, a window that made
   the same PTY suite pass on macOS and fail on Linux. A spinner means work
   is active and nothing else; the busy label is the session's own line,
   never a percentage.

5. **The composer: `ratatui-textarea` behind a wrapper.** The wrapper owns
   the meaning: `Enter` sends, `Alt+Enter` (or `Shift+Enter` · `Ctrl+J`)
   breaks a line, a bracketed paste is data (a pasted `yes`, `run it` or
   `/quit` acts on nothing until `Enter`), `Up`/`Down` recall history only
   at the buffer's first or last line and give the draft back. The editor
   never leaves the UI task (the fork's open `Send` regression). Exit
   criterion: the day the wrapper re-implements cursor movement or
   wrapping, the crate is replaced by an owned editor.

6. **Keys the loop owns, never the composer.** `Ctrl+C` is state-aware
   (with work active it interrupts and says so; idle, the first press arms
   and the status line says the next one leaves); `Ctrl+T` toggles the
   presentation; `Esc`, `PageUp`, `PageDown` belong to the focus view. A
   gate is answered by a typed word under the `answer ›` prompt, never by a
   stray key.

7. **Colour is the theme's decision, never the renderer's.** The renderer
   receives one bit (colour allowed) from the CLI's existing chain
   (`--color` > `CLICOLOR_FORCE` > `NO_COLOR` > `CLICOLOR=0` > TTY and
   `TERM≠dumb`) and paints with the ANSI-16 roles only: blue for the prompt
   marker when computation is active, yellow for a gate, a permission, a
   cost or a boundary, red for a refusal; chrome dimmer than the workflow
   text; no gradients, no glow, no giant marks, no truecolor chrome.

## Consequences

- `nika-tui` joins the workspace as a WIP L4 member (the `nika-tui-core`
  precedent), `publish = false`, one binary `nika-tui-proto` that drives
  the shell over a canned conversation in both presentations for the UX-1
  proof; the real session is wired in the next wave through the same beat
  vocabulary, behind an explicit switch until the PTY goldens of the CLI
  are re-cut for the new face.
- The workspace gains `ratatui`, `crossterm` and `ratatui-textarea` (all
  MIT), pinned in the workspace manifest.
- Qualification of the wave = the PTY suite `crates/nika-tui/tests/pty_restore.rs`
  on the real binary: the normal close (inline · focus), two `Ctrl+C`, a
  panic inside the loop (restore BEFORE the message), `SIGTERM` (exit 143),
  a bracketed paste holding `yes` and `/quit` that acts on nothing across a
  focus switch and back, a pipe refused with exit 2 and no escape
  sequence. The frames are judged by `TestBackend` tests in the crate.
- The candidate spec `docs/crate-specs/nika-tui.md` is amended: the
  implementation order becomes the product's waves (UX-1 renderer proof ·
  UX-2 the first five seconds · UX-3 contextual cognition and recovery ·
  UX-4 the living workflow object · UX-5 run, gate, result · UX-6
  hardening · UX-7 human qualification); tachyonfx and the web-studio port
  are not part of the first product; the law crate is unchanged.
- Known limits carried into UX-2: the inline anchoring depends on a
  cursor-position report some PTYs never answer (the renderer then refuses
  with its reason; the plain loop stays available); a resize reflows only
  the live area, the rows already in scrollback are the terminal's;
  `Viewport::Inline` spans the full width; without `scrolling-regions`
  every insert would repaint the viewport (the feature is on).

## What overturns it

- Inline mode failing the resize or the copy/select scenarios of UX-2 in a
  way `scrolling-regions` does not repair: the focus view becomes the
  default and inline the reduced mode.
- `ratatui-textarea` failing paste, Unicode width or wrap on the fixtures:
  an owned editor from the start.
- Input latency above 50 ms p95 on the reference environment: the loop is
  restructured before any component work.

## Not decided here

Exact key bindings beyond the six above, the logo's terminal projection,
the command palette's contents, the living workflow object's glyphs, the
contextual intelligence picker. Those are UX-2 to UX-5 questions, answered
on the real binary.
