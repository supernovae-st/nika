# Crate spec — `nika-tmpl` (Gate 1)

| | |
|---|---|
| Status | **ADMITTED 2026-07-08** — the 41st crate. Gate 1 authored 2026-07-03; the 12-gate run completed on branch `feat/nika-tmpl` (Gate 5 = 46/51 · 90.2% · §8 budget; Gate 10 pre-proven ~337k inputs). |
| Layer | **L0** — pure, zero I/O, zero async, **zero `nika-*` deps** (serde-free; pure `&str` lexing). |
| Design | The canonical `${{ … }}` **template-island LEXER** — the AST-free lexical layer that finds island boundaries (quote- + escape-aware) and classifies single-island vs interpolation. It does NOT parse bodies: it returns byte-spans + the raw body `&str`; the *body* language stays with each consumer (the checker's static-subset parser · the runtime's `nika-cel`). |
| Name | `nika-tmpl` (honest: the template *lexer*, distinct from the future `nika-binding` template *engine* — the 65 transforms + resolver). **Confirmable at PR** (naming discipline · nika-invariants). Alt considered: `nika-island`, `nika-binding-scan`. |
| LOC budget | ≤400 src (est. ~150). ≤1500/file, ≤100/fn. |
| Deps | none (not even `nika-types`). dev: `proptest`. |
| Publish | `false` — foundation crate (ADR-022). |

## 1 · Why this crate exists (the TCB argument, not DRY)

The `${{ }}` island scanner is currently **hand-duplicated**: canonical in
`nika-schema::expression::template` (private `mod`) and byte-copied into
`nika-runtime::expr` (8 "Mirrors the static analyzer" comment sites + duplicate
tests). This drift class **already shipped a bug** (c15, 2026-06-18: the checker
skipped the `\${{` literal-escape the runtime resolved → check-passed / run-broke).

Recent IFC research (arXiv:2606.26479, 2026-06-25 · systematization of CaMeL,
Fides, Progent) is explicit: for a deterministic out-of-band security gate, the
**trusted computing base is provenance / label assignment** — and *identical
`${{ }}` semantics between the static checker and the runtime resolver IS that
TCB*. If the two scan islands differently, the taint source is misidentified and
the Denning lattice is wrong at the root. Industry precedent: CEL ships one AST
for parse→check→eval (cel-rust spent 2025 retrofitting toward it); GitHub Actions'
two-implementation model (runner vs actionlint) is the named permanent-drift
anti-pattern. Nika starts at rung 1 (one shared lexer).

**So the goal is parity by construction, not by comment.**

## 2 · Public API (AST-free · spans only)

```rust
/// A located `${{ … }}` island: byte offsets into the source + the raw
/// (untrimmed) body slice. AST-free — the body language is the consumer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct IslandSpan<'a> {
    pub start: usize,      // byte offset of the leading `$`
    pub body: &'a str,     // slice BETWEEN `${{` and `}}` (untrimmed)
    pub body_start: usize, // byte offset of the first body byte (== start + 3)
    pub end: usize,        // byte offset ONE PAST the closing `}}`
}

/// Error: an opener with no quote-aware closer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScanError { Unterminated { offset: usize } }

/// Scan every REAL (non-`\`-escaped) `${{ … }}` island, left to right.
/// Quote-aware close (a `}}` inside a `'…'`/`"…"` body literal does not
/// close) · `\${{` is a literal escape, not an island.
pub fn scan_islands(s: &str) -> Result<Vec<IslandSpan<'_>>, ScanError>;

/// The lower-level close-finder (offset of the `}}` opening brace, scanning
/// from `from`), exposed for consumers that stream islands.
pub fn find_island_close(s: &str, from: usize) -> Option<usize>;

/// If `s` is EXACTLY one island (optional surrounding whitespace only),
/// return its trimmed body — the type-preserving single-island case
/// (`"${{ ref }}"` resolves to the referenced VALUE, not its string form).
pub fn single_island(s: &str) -> Option<&str>;
```

Design notes:
- **AST-free is the correct cut.** `scan_templates` in schema couples lexing with
  `parse_expression` → `TemplateIsland{ expr: Expr }` (schema's AST). Runtime parses
  bodies with `nika_cel`. The ONLY thing both share + duplicate is the *lexing*.
  Extracting spans (not ASTs) is exactly the shared surface — it does NOT force the
  (harder, deferred) expression-parser unification.
- `#[non_exhaustive]` + INV#19 constructors on the public structs.
- No error codes in the NIKA-xxxx sense — `ScanError` is a local typed enum; the
  consumers map it to their own domain error (schema `ExprError::UnterminatedTemplate`,
  runtime `RuntimeError::UnresolvedTemplate`) preserving today's messages.

## 3 · Consumers (rewire · delete the copies)

- **`nika-schema::expression::template::scan_templates`** → becomes a thin wrapper:
  `nika_tmpl::scan_islands(s)?` then per span `parse_expression(span.body.trim())` →
  `TemplateIsland`. Deletes schema's private `find_island_close`. All existing
  checker consumers (arg_injection · preference_rules · flow.rs taint) unchanged —
  they call `scan_templates` which keeps its signature.
- **`nika-runtime::expr`** → `render` / `render_json` / `eval_when` call
  `nika_tmpl::{scan_islands|find_island_close|single_island}` instead of the
  hand-copies. Deletes runtime's `find_island_close` + `single_island`.

## 4 · The 12 gates

| Gate | Plan |
|---|---|
| 1 SPEC | this doc |
| 2 TDD | RED-first: port both copies' unit tests; add the escape/quote battery |
| 3 IMPL | ~150 LOC, the proven scanner (verbatim logic) + span wrapper |
| 4 CLIPPY | 0 workspace |
| 5 MUTATION | 46/51 caught (90.2% ≥ floor) · BUDGET belt-and-braces <!-- GATE5-EXEMPT: 5 --> — the 5 survivors are all in two certified-unkillable classes (see §8); every OTHER class is killed by the unit battery (loop-advance pins · display · quote-machine polarity/direction · lone-brace close · escape skips) |
| 6 PROPERTY | **the differential harness IS the property test** — `scan_islands` results must be quote/escape-correct on the hostile alphabet (`} { ' " \ $ a ␠`), exhaustive to len 6 |
| 7 BENCH | N/A — pure lexer, no hot allocation (justified) |
| 8 DOCS | 0 warnings |
| 9 CANARY | N/A — L0 lexer, no runtime surface of its own (justified) |
| 10 PARITY | **PRE-PROVEN**: schema vs runtime scanners are byte-identical across ~337k exhaustive inputs (37,449 close-finder + 299,593 open-scan+escape · 0 diffs · harness 2026-07-03). Golden-test both old copies' behavior against the extracted one to lock it. |
| 11 REVIEW | 3-agent swarm |
| 12 ATOMIC | 1 crate = 1 commit |

## 5 · Anti-scope (what stays out)

- **No expression AST / grammar** — `Expr`, `parse_expression`, `nika-cel` all stay
  where they are. This crate is *below* the body language.
- **No taint / label rules** — those are checker-internal (`flow.rs`); the runtime
  does no taint. Sharing them requires the (deferred) expression-frontend unification
  — a separate ADR (the real `nika-binding` engine · see the 42/42 master plan §3.5).
- **No transforms / resolver** — that is the `nika-binding` engine, later.

## 6 · Coherence

Parity-by-construction for the IFC TCB (research-grounded) · one lexer, zero drift
(canon-ssot-discipline applied to the check⇄run seam) · additive, dependency-light
L0 leaf (nika-cap precedent) · publishing the paired conformance-fixtures (already a
taxonomy class) gives third-party reimplementers rung-3 parity.

## 7 · Review-swarm findings (Gate 11 · resolved / tracked)

- **Error precedence (P1 · rust-pro · ACCEPTED as intentional, pinned):** on a
  doubly-malformed template with BOTH a resolvable-but-erroring island AND a
  later *unterminated* opener, the old streaming render surfaced the island's
  resolution/grammar error first (left-to-right); the new render pre-scans via
  `scan_islands`, so the *structural* `Unterminated` error wins before any
  resolution. Both are hard errors either way (no happy-path change · injection
  safety unchanged). Surfacing a structural fault before a semantic one is
  standard parser discipline, and a passed `nika check` has no unterminated
  islands so runtime never hits it. Pinned by a runtime test; not a regression.
- **`\\${{` (double-backslash) is spec-UNDEFINED (feature-dev · TRACKED follow-up):**
  the spec (`04-variables.md` §Escaping) defines only `\${{` (single backslash →
  literal). `nika-tmpl` (matching the extracted checker+runtime) treats an opener
  with ANY immediately-preceding `\` as escaped (single-byte). `nika-lsp` has a
  THIRD, independent scanner (`analysis/definition.rs`) using backslash-RUN parity
  (even run → live island) — deliberately tested. On the spec-DEFINED case (`\${{`)
  all agree; they diverge only on the undefined `\\${{` edge. **nika-tmpl achieves
  the check⇄run parity (the IFC TCB · the security-critical pair) — its primary
  goal.** Consolidating `nika-lsp` onto `nika-tmpl` REQUIRES first an operator
  spec decision defining `\\${{` (run-parity is the standard + what the LSP tests).
  Tracked; out of this crate's scope (spec §3 consumers = schema + runtime).

## 8 · Gate-5 exemption budget (survivors ≤ 5 · the two unkillable classes)

Admission run (2026-07-08): **46/51 caught · 90.2% ≥ the 90% floor**, with
the 5 survivors all in exactly two classes no lib-test can kill — the
budget documents them and NOTHING else (a survivor outside these classes
is a real test gap: add the killer test, do not grow the budget):

1. **Equivalent mutant** (1) —
   `lib.rs:111 replace < with <= in scan_islands`
   (`while i < bytes.len()` → `<=`): the one extra iteration probes
   `bytes[len..]` (an empty slice — no opener match, no index past the
   guard) and exits. Behaviorally identical on every input.
2. **Hang-only arithmetic no-ops** (4) —
   `lib.rs:115 += → *=` (scan advance) ·
   `lib.rs:158 += → -=` and `+= → *=` (in-quote advance) ·
   `lib.rs:167 += → *=` (close-finder advance):
   the loop stops advancing (`i *= 1` no-ops at any `i`; `-=` walks
   backward into a cycle) and the ONLY observable effect is
   non-termination. cargo-mutants reports these as `timeout` — which
   this repo's floor script counts as survivors by design (stricter
   than upstream). A unit test cannot assert termination; the timeout
   IS the detection.

Every DIVERGENT (behavior-changing) mutant on the same machinery IS
killed by the battery, proven by `cargo mutants --iterate` on the
admission tree: the loop-advance pins (`exact_spans_pin_the_scan_advances`
incl. the `xy\${{${{z}}` escape-adjacency case), the quote-machine
direction + polarity pins (`"x\q}}rest"` backward-slide ·
`'}}}a'` close-polarity), the two-byte escape skip, the lone-brace
close, and the Display rendering.
