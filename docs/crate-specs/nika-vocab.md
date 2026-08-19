# Crate spec — `nika-vocab` (Gate 1)

| | |
|---|---|
| Status | **ADMITTED 2026-07-15** — the second size-cap split of the `nika-schema` unit (W-COMP « the composition » needs parser + check headroom while the crate sat at exactly 15 000 prod LOC; per the unit-target discipline, D-2026-07-09-N1, one architectural unit may span N workspace members — `nika-schema` stays the unit's front door). |
| Layer | **L0** — pure, zero I/O, zero async. |
| Design | The `.nika.yaml` configuration vocabulary: capture/decode modes, retry + backoff, Go-style durations, secret sources + egress rules, `when:` gates, `after:` predicates, var/output declarations, the schema-version marker. The closed value-type set both the raw AST (parser output) and the analyzed AST consume. |
| Name | `nika-vocab` (honest: the *vocabulary* of the language, not its grammar). |
| LOC budget | ≤1500 src (admitted at ~1160). ≤1500/file, ≤100/fn. |
| Deps | `nika-source` (Spanned/Span carriers) · `nika-cap` (permits/policy vocab re-exported at the original module path) · `serde` · `serde_json`. |
| Publish | `false` — foundation crate (ADR-017/022 class). |

## 1 · Why this crate exists (the split argument, not DRY)

`nika-schema` is the workflow AST/parser/analyzer hub and grows with every
language wave. W-COMP (spec 14 · `invoke: workflow:` · the NIKA-COMP-001..004
lane) lands while the crate sits at the constitutional 15 000 hard cap. The
registry says what happens next: **a size-cap split is one unit in N members**
(never a watered-down cap, never a waiver). The `nika-source` descent
(2026-07-14) is the exact precedent.

`src/types/` was the widest leaf with one-directional coupling (the config
value types — the parser/analyzer/check all depend on them; they depend only
on `nika-source` spans and the `nika-cap` permits vocab), so it descends as
this crate. `nika-schema::types` re-exports it wholesale (`pub use
nika_vocab::*`): every consumer path (`nika_schema::types::Permits` ·
`VarDecl` · `RetryConfig` · `CaptureMode` · …) is byte-for-byte unchanged,
and the schema crate remains the unit's only front door. No consumer names
`nika-vocab` directly today.

## 2 · Public API (module-per-field)

```rust
pub mod after;           // AfterPredicate — the closed after: set + the R5 dead-spelling teachings
pub mod capture;         // CaptureMode — stdout/stderr/combined/structured
pub mod decode;          // DecodeMode — text/json/jsonl/bytes
pub mod duration;        // parse_go_duration + GoDurationError
pub mod extract;         // ExtractMode + ResponseMode (fetch shapes)
pub mod on_error;        // OnError + OnErrorAction (retry/recover/fail)
pub mod output_decl;     // OutputDecl — untyped | typed outputs: entry
pub mod permits;         // re-export of nika-cap (the original module path)
pub mod retry;           // RetryConfig + BackoffStrategy + is_valid_error_code
pub mod schema_version;  // SchemaVersion — the `nika: v1` marker · stale-ok: dead value type · engine cleanup owed (ADR-113)
pub mod secret;          // SecretSource + SecretRef + EgressRule (+ the refusal teachings)
pub mod type_expr;       // the io-decl TypeExpr helpers — display · coerce · DEFAULT-001 teaching
pub mod var_decl;        // VarDecl — the inputs:/config:/const: declaration forms
pub mod when_gate;       // WhenGate — literal | expression when:
```

(`keys` — the parser's closed key vocabularies · `dead_form` — the C2
dead `vars:`/`env:` forms + their teachings · `assert` — the spec-15
obligation vocabulary — joined at the C2 flag-day; `type_expr` joins at
R3b: the declaration `type:` speaks the full TypeExpr of 09-types, the
flat 6-enum `VarType` is DEAD — `bool` the one boolean spelling.)

`#[non_exhaustive]` note: the CLOSED spec vocabularies the unit matches
exhaustively (`AfterPredicate` · `DecodeMode` · `VarDecl` ·
`OutputDecl` · `SecretSource` · `WhenGate` · `EgressRule`) dropped the
attribute at the split — a new variant in a closed vocabulary IS a spec
change and MUST force every match site to update (the checker's
forced-update property). Open shapes (`RetryConfig` · `SecretRef` ·
`SchemaVersion` · capture/extract modes) keep it.

## 3 · Gates

Inherits the unit's proofs: the split moved files verbatim (`git mv` —
history preserved), the workspace re-proved green after the move (lib +
clippy `-D warnings`), and the public-api baseline lands via the CI artifact
lane (Ubuntu runner · never hand-authored).
