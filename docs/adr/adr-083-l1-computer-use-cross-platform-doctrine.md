---
id: ADR-083
title: "L1 computer-use effect crates · cross-platform doctrine (macOS + Linux prio · Windows + all)"
status: accepted
date: 2026-05-26
phase: "Phase 2 M2"
deciders: ["@ThibautMelen"]
tags: ["l1", "computer-use", "cross-platform", "macos", "linux", "windows", "backend", "doctrine"]
affects_crates: ["nika-screen", "nika-ocr", "nika-a11y", "nika-input", "nika-browser", "nika-vision-local"]
affects_layers: ["L1"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-016", "ADR-081"]
requires: ["ADR-003"]
enables: []
amends: []
fci: ["FCI-003"]
inv: ["INV-019"]
shadow_zones: ["synthetic-input", "accessibility-tree"]
---

# ADR-083 · L1 computer-use effect crates · cross-platform doctrine

## Context

The Phase 2 M2 computer-use L1 effect crates (`nika-screen` · `nika-ocr` ·
`nika-a11y` · `nika-input` · `nika-browser` · `nika-vision-local`) back the
future `invoke:`-tool surface that lets a workflow *see + act on* a desktop.

An empirical audit (2026-05-26) of the three already-admitted crates surfaced
an **inconsistent platform posture** that was never an explicit decision:

| Crate | Backend | Platform reality |
|---|---|---|
| `nika-screen` | `xcap` 0.9 | **cross-platform** (macOS / Linux / Windows) |
| `nika-ocr` | `ocrs` + `rten` | **cross-platform** (pure-Rust, no target gate) |
| `nika-a11y` | `accessibility` 0.2 | **macOS-only** (`[target.'cfg(target_os="macos")']`) |

`nika-screen` and `nika-ocr` are cross-platform-by-default; `nika-a11y` shipped
macOS-only — **forced**, because no single cross-platform pure-Rust *accessibility-
query* crate exists (`atspi` = Linux AT-SPI2 · `uiautomation` = Windows UIA are
per-OS). The macOS-only posture was a *necessity*, not a chosen strategy — but it
was never written down as such, so the next crate (`nika-input`, M2.4) nearly
inherited "macOS-first" as if it were the pattern. It is not: `nika-input` has a
mature cross-platform option (`enigo`).

Operator directive (2026-05-26, verbatim): *« je veux cette philosophie de cross
plateform et macos, linux prio sous unix et aussi windows et tout les systeme …
on peut revoir et refactor a11y si besoin »*.

## Decision

**L1 computer-use effect crates are CROSS-PLATFORM.** Priority order ·
**macOS + Linux (Unix family) first**, then **Windows**, then all systems.

**Backend-selection rule (the doctrine):**

1. Prefer **one mature, permissively-licensed cross-platform Rust crate** that
   covers macOS + Linux (+ Windows) behind the L0.5 trait. The guards (ADR-081)
   stay pure + at *our* layer, so a third-party backend never touches them.
2. **Only when no cross-platform crate exists**, fall back to **per-OS backends
   behind the same L0.5 trait** — and even then, macOS-only is a *temporary*
   state: the Linux (then Windows) backends are a **planned refactor**, not a
   permanent gate. Prioritize the **macOS + Linux** backends first.
3. The cross-platform DTOs + the mandatory guards always ship **all-OS, headless**
   (a denied/unavailable OS returns `BackendUnavailable`, never a compile gap in
   the shared logic).

### Per-crate disposition

| Crate | Disposition under this doctrine |
|---|---|
| `nika-screen` | ✅ cross-platform already (`xcap`) · no action |
| `nika-ocr` | ✅ cross-platform already (`ocrs`/`rten`) · no action |
| `nika-a11y` | 🔧 **REFACTOR** · keep `accessibility` (macOS) · **add `atspi`** (Linux AT-SPI2 · the crate-spec §4 already vetted it · Apache/MIT) · `uiautomation` (Windows) later. Trait + `AxNode` DTO + Guard 3 are already backend-agnostic → additive per-OS backend behind `#[cfg]`. Amends the crate-spec's "RESOLVED macOS-first" disposition. |
| `nika-input` | 🆕 **cross-platform from day 1** via `enigo` (macOS + Linux + Windows · MIT). Resolves the M2.4 backend DECISION POINT. Guards 1+2 are pure + at our layer · `enigo` only performs the final OS event post. (No `enigo` is rejected in favour of macOS-only CGEvent — that would re-introduce the inconsistency this ADR removes.) |
| `nika-browser` (M2.5) | cross-platform backend mandatory (the chosen automation lib must cover macOS + Linux). |
| `nika-vision-local` (M2.6) | cross-platform by construction (local model inference · pure compute · like `nika-ocr`). |

Per-crate exact crate + symbol surface is **primary-source verified at Gate 3**
(`.crate` tarball extraction · the rigor that confirmed `accessibility` for
`nika-a11y` · no phantom symbols written before verification).

## Consequences

### Positive
- One coherent platform posture across the computer-use cohort · no accidental
  macOS lock-in.
- macOS + Linux ship together (the operator's stated priority · Unix-family first).
- The guards stay backend-agnostic · adding an OS backend never touches security
  logic.

### Negative (acknowledged · mitigated)
- `nika-a11y` gains a Linux `atspi` backend (D-Bus/zbus) → a refactor + its own
  Gate 3 primary-source verification + tri-platform consideration. Mitigated ·
  additive behind the existing trait · the DTO + Guard 3 already all-OS.
- `nika-input` takes a dependency on `enigo` (a backend we don't fully own) vs
  hand-written per-OS FFI. Mitigated · `enigo` is mature + permissive · the
  guards + consent type-state are *ours* (the dep only posts the final event).
- CI eventually wants macOS + Linux runners for the per-OS backends. Deferred ·
  headless logic + guards are CI-covered all-OS today; real-OS smoke stays
  `#[ignore]` per the M2 precedent.

### Sequencing note (NOT this ADR's call · gate-review)
The computer-use cohort is **not** in the v0.81 first-publish scope (v0.81 =
binary engine + language · the 4 verbs + DAG + providers + MCP server). This ADR
governs **how** computer-use is built (cross-platform); **when** (relative to the
v0.81 spine: providers → verbs → L3 runtime) is an operator gate-review decision
tracked separately (private roadmap).

## Alternatives considered

- **macOS-first, port later (rejected)** · the prior implicit posture. Rejected
  per operator directive — cross-platform is the *philosophy*, day-1 where a
  crate exists. macOS-only survives *only* where no cross-platform crate exists
  (a11y), as a temporary refactor target.
- **`enigo` rejected for "control" (rejected)** · the M2.4 spec's first draft
  leaned macOS-CGEvent for sovereignty/control. Re-examined · the guards are pure
  + at our layer, so `enigo` does not weaken them · it only removes hand-written
  per-OS FFI. Cross-platform day-1 > marginal FFI ownership.

## References
- `docs/crate-specs/nika-input.md` §4 (backend DECISION POINT · resolved here)
- `docs/crate-specs/nika-a11y.md` §4 (macOS-first disposition · amended here)
- `docs/adr/adr-081-l1-effect-crate-guard-contract.md` (guard contract · backend-agnostic)
- `crates/nika-kernel/src/io/input.rs` (the trait names macOS CGEvent · Windows SendInput · Linux uinput — superseded for `nika-input` by the cross-platform `enigo` choice; the per-OS APIs remain the fallback model the trait doc describes)
- v0.81 first-publish scope (binary engine + language · the 4 verbs + DAG + providers + MCP server) · sequencing context for the §Sequencing note (private roadmap · not cited from this public repo)

## Update log

```
2026-05-26  ACCEPTED — initial · operator directive locks the cross-platform
              doctrine for the L1 computer-use cohort (macOS + Linux prio ·
              Windows + all). Resolves nika-input backend = enigo (cross-platform
              day-1). Flags nika-a11y for a Linux atspi backend refactor. screen +
              ocr already cross-platform (no action). Guards stay backend-agnostic.
              Sequencing (now vs v0.81 spine) is a separate operator gate-review.
```
