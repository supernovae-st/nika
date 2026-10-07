---
id: ADR-143
title: "nika-tui size-cap member split: the viewers of the object in view descend to nika-tui-view"
status: accepted
date: "2026-09-30"
phase: "pre-1.0 · terminal UI workstream · delivery 2"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "tui"]
affects_crates: ["nika-tui", "nika-tui-view"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-137", "ADR-138", "ADR-139", "ADR-140", "ADR-141"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.121"
follow_ups: ["the admission evidence of the member, pending with its WIP unit", "one owner for the role mapping once nika-tui depends on the member (the renderer may then re-export the member's copy)", "the object region of the workspace paints a Rendered value (lane ws), which makes the member a normal dependency of nika-tui (done 2026-10-03, TUI-01)"]
---

# ADR-143: nika-tui size-cap member split — nika-tui-view

## Context

`nika-tui` (ADR-139) measures **13,879 prod LOC** at the terminal UI head `887fb8459`, against
the 15,000 Diamond wall (`scripts/ci/check-crate-size.sh`, the gate's own counter). The work
queued behind it does not fit: the keys work of the cards lane measures 15,011 on its branch,
and the workspace door, the transcript and the runs projection still have to land in the
crate. The cap is a locked maintainability budget, not advisory.

One module stands apart. `src/view` (task T-nika-tui-viewers, 6,143 prod LOC) holds the viewers
of the object in view: pure functions from what a caller already holds to styled lines. It
reads no file, clock or environment, nothing opens it yet, and no other module of `nika-tui`
names it; only the `view_gallery` example and the public API snapshot do.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in several workspace
members, the ADR-137, ADR-138, ADR-140 and ADR-141 precedents), the viewers descend from
`nika-tui` to a new L4 member crate `nika-tui-view`, placed BELOW the renderer:
`nika-tui → nika-tui-view → {nika-session, nika-display}`, never back.

- `crates/nika-tui/src/view/**` moves to `crates/nika-tui-view/src/**` (`mod.rs` becomes
  `lib.rs`, whose doc becomes the crate doc). The files keep their bytes except the paths the
  move changes: the test modules name `crate::` for `crate::view::`, and the viewers take
  their colour from the member's own `role` module.
- The public paths change from `nika_tui::view::…` to `nika_tui_view::…`. Nothing is
  re-exported by `nika-tui`: no consumer outside the unit names the module (`nika-cli` does
  not), and the example is migrated in the same change.
- `nika-tui` keeps the `view_gallery` example: it shows the viewers through the renderer's
  terminal owner (`terminal::probe`, `enter`, `restore`, the panic hook), so the renderer takes
  the member as a dev-dependency. The library of `nika-tui` does not depend on the member until
  the workspace paints the object in view.

### Why this boundary, measured

Edges on the tree of `887fb8459`, from `use` trees and inline paths:

| direction | edges |
|---|---:|
| `nika-tui` modules → the viewers | **0** (only `examples/view_gallery.rs`) |
| the viewers → `nika-tui` | `visual::role::style` (one production import, five test calls) |
| the viewers → `nika-session` | `review::{plan_lines_in_order, gate_tasks}` (tests: `guard::builtin_names`) |
| the viewers → `nika-display` | `theme::{Role, Theme}` · `check_render::VerdictLayers` · `dag_art::{GraphDoc, wire_graph}` · `wires::render` · `shape::summarize` |
| outside the unit → the viewers | **0** |

The one edge back is a 20-line mapping from the engine's theme roles to Ratatui styles. The
member carries a private copy of `style` (the one function it uses), with the parity test that
pins the renderer's copy to the slot the CLI theme paints (`nika_display::theme::Theme::paint`):
each copy answers to the same owner, so the two cannot drift apart unseen.

## Consequences

- `nika-tui` measures **7,735** prod LOC (headroom 7,265) and `nika-tui-view` **6,185** (the
  6,143 lines that moved, the crate doc and the copied mapping), the gate's own counter.
- The 239 library tests of `nika-tui` split into 156 that stay and 83 that move with their
  files under their new paths; the member adds the two parity tests of the copied mapping.
  The integration suites of `nika-tui` do not name the viewers and are unchanged.
- `Cargo.lock` gains the member's package and the renderer's dev edge to it; no version moves.
- The public API snapshot of `nika-tui` loses the viewers' section, which becomes the
  member's snapshot under `nika_tui_view::`, with the rows a Linux render adds for its three
  `Default` types (zvariant reaches the member through `nika-display`); the Linux job confirms.
- The member joins the workspace as WIP with its unit, as `nika-tui` did; its admission
  evidence stays pending with the unit, tracked, never claimed.
- The workspace registries name it: the members, the WIP list, the layer table, the public
  API coverage floor and the generated status blocks.

## Alternatives considered

### Alt A -- the gallery with the member

It would need `nika-tui` as a dev-dependency of the member, an edge back that becomes a cycle
as soon as the renderer depends on the member, or a second owner of the terminal inside an
example, against ADR-139.

### Alt B -- the role mapping in nika-display

`nika-display` renders ANSI text for the CLI, the Session and the renderer and has no Ratatui
dependency; giving it one to serve a single style function widens a shared crate.

### Alt C -- the mapping moved to the member, used by nika-tui

`nika-tui` would depend on the viewer library for every colour it paints, before anything
wires the viewers, and the renderer's colour law would live in a library of viewers.

### Alt D -- another module (card, workspace, session)

Each is wired: `model`, `render` and `session` read the cards, and the workspace screen composes
the renderer's own transcript, status and composer. Cutting one would move edges both ways.

### Alt E -- trim the crate in place

The wall counts comments and blank lines by design; the work queued behind it needs thousands
of lines, not tens.

## Amendment 2026-10-03 · recovered from the terminal UI branches, a production edge

The member was recovered from the terminal UI branches (TUI-01), where its bytes were
identical, onto an integration carrier based on `main` `c15cf94da`; it reaches `main`
through the normal integration. The third follow-up above is now done: the workspace's object region
paints one face of the look the Live host adapter takes of an opened workflow, so `nika-tui`
takes the member as a normal dependency (still `nika-tui → nika-tui-view`, never back). The
member gains one verdict, `Verdict::ParentOnly`: layers computed over one file read alone, with
RUN READY unknown and what the look did not capture named, never the readiness of a composed
workflow. With it `nika-tui` measures 7,183 prod LOC and the member 6,210 (the gate's counter at
`0695db637`, base `nika-tui` 5,399); the split keeps the unit's
room for the queued work rather than answering a breach today.

The `view_gallery` example and the dev-dependency it required were not recovered; the
decision text and measurements above describe the 2026-09-30 split. Artifact viewers
remain library surfaces awaiting a Live owner and an on-terminal witness.

## Amendment 2026-10-03 · run outputs and current files

The later run-result slice gives the artifact viewers a Live owner:
`workspace::live::faces` calls `nika_tui_view::artifact` for settlement outputs
and bounded current reads of reported writes. Acquisition remains in the host;
the member stays pure and the dependency direction is unchanged. The earlier
measurements and unrecovered gallery remain historical facts. Workspace PTYs
cover an output and a Markdown file, not every viewer format, the full workspace
journey or a stamped integrated build.

## Related

- ADR-139 (the terminal renderer), ADR-137, ADR-138, ADR-140 and ADR-141 (the size-cap member
  precedents), D-2026-07-09-N1.
