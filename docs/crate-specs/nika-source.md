# Crate spec — `nika-source` (Gate 1)

| | |
|---|---|
| Status | **ADMITTED 2026-07-14** — the size-cap split of the `nika-schema` unit (W3 « the contract » pushed the crate past the 15k prod-LOC budget; per the unit-target discipline, D-2026-07-09-N1, one architectural unit may span two workspace members — `nika-schema` stays the unit's front door). |
| Layer | **L0** — pure, zero I/O, zero async, zero `nika-*` deps. |
| Design | Source tracking for diagnostics: `FileId` interning (`SourceRegistry`), byte-offset `Span`/`Spanned<T>` carriers, and `LineCol` conversion. The substrate every ladder finding anchors to. |
| Name | `nika-source` (honest: the *source* bookkeeping, not the parser). |
| LOC budget | ≤400 src (admitted at ~310). ≤1500/file, ≤100/fn. |
| Deps | `serde` only. |
| Publish | `false` — foundation crate (ADR-017/022 class). |

## 1 · Why this crate exists (the split argument, not DRY)

`nika-schema` is the workflow AST/parser/analyzer hub and grows with every
language wave. W3 (the `types:`/`returns:`/`decode:` contract) carried it to
15 231 prod LOC — past the constitutional 15 000 hard cap. The registry says
what happens next: **a size-cap split is one unit in two members** (never a
watered-down cap, never a waiver).

`src/source/` was the one subtree with zero inbound coupling (spans + file
registry — every OTHER module depends on it, it depends on nothing), so it
descends as this leaf crate. `nika-schema::source` re-exports it wholesale:
every consumer path (`nika_schema::source::Span` · `FileId` · `Spanned`) is
byte-for-byte unchanged, and the schema crate remains the unit's only
front door. No consumer names `nika-source` directly today; the LSP/DAP MAY
once they need spans without the parser.

## 2 · Public API

```rust
pub struct FileId(u32);                  // interned file identity
pub struct ByteOffset(u32);              // absolute byte position
pub struct Span { file, start, end }     // #[non_exhaustive] · Span::new
pub struct Spanned<T> { value, span }    // #[non_exhaustive] · Spanned::new
pub struct LineCol { line, col }         // 1-based · for humans
pub struct SourceRegistry;               // add() → FileId · get() → SourceFile
pub struct SourceFile { path, content }  // Arc<str> shared content
```

## 3 · Gates

Inherits the unit's proofs: the split moved files verbatim (`git mv` — history
preserved), the whole workspace re-proved green after the move (lib 4277/0 ·
integration 4844/0 · clippy 0 with `-D warnings`), and the public-api baseline
lands via the CI artifact lane (Ubuntu runner · never hand-authored).
