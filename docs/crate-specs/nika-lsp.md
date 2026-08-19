# Crate spec — `nika-lsp`

| | |
|---|---|
| Status | **WIP** (Phase B announce-ladder · `nika lsp` v0.1 IN-BINARY · D-2026-06-10-N6 launch floor · ADR-003 12 gates) |
| Layer | L4 — interface (language server) · gated on L0 only (pure analysis over `nika-schema`) · sync · stdio |
| Sub-tier | L4-surface — the editor surface. Pure analysis modules (`analysis::*`) over the L0 `nika-schema` parse + check ladder, driven by a sync `lsp-server` JSON-RPC loop. Feeds the `nika-vscode` extension (and any LSP client) the day `nika --help` lists `lsp` (the extension auto-detects via `caps.lsp`). |
| Design | ONE crate (collapse · per `nika-invariants.md` « nika-lsp-core → merged into nika-lsp » + the collapse-vs-publish default · reconciles D-2026-06-10-N6 steps 19.6/19.7 which carried the brouillon two-crate shape — the LSP has zero external crates.io value, so one crate with internal modules wins). Stack = `lsp-server` 0.7 (rust-analyzer's sync stdio loop · MIT) + `lsp-types` 0.97 (LSP 3.17 types · MIT) — NOT `tower-lsp`/tokio (the v0.1 scope is parse + position-map + full-reparse-on-change · no async needed · Rams « less but better » + minimal deps). Diagnostics reuse the SAME `nika_check` ladder that powers `nika check` (one source of truth · no second checker · descended from nika-schema 2026-07-21). |
| LOC budget | ≤4,000 src (server loop + 8 analysis modules) |
| File cap | ≤1,500 LOC each · Function cap ≤100 lines |
| Crate version | tracks workspace · License `AGPL-3.0-or-later` · Edition 2024 · Publish `false` |
| ADRs | ADR-003 (12-gate admission) · ADR-092 (the check ladder the diagnostics surface) · D-2026-06-10-N6 (launch-surface-complete · LSP at announce) |
| Error range | **none user-facing** — the LSP surfaces `nika-schema`'s existing `NIKA-*` codes verbatim (diagnostic `code` field). Its own transport/protocol failures are an internal `thiserror` enum (`LspError`), not a `NIKA-XXXX` range (transport errors never reach the workflow author). |
| Reference | `git show brouillon:tools/nika-engine/src/lsp/*` (CRAFT reference only · the brouillon used `tower-lsp` + a 2-phase AST + a 1508-LOC model catalog — all OUT of v0.1 scope · we rewrite clean per ADR-001) · `crates/nika-cli/src/verbs/check.rs` (the check ladder render this mirrors) |

---

## 1. Purpose

`nika-lsp` is the **editor brain** for `.nika.yaml`. It turns the engine's
static guarantees into live, in-editor feedback: red squiggles from the
ADR-092 check ladder, hover docs for the 4 verbs and the language keywords,
completion for the locked vocabulary and the workflow's own task ids, an
outline of the tasks (document symbols), and go-to-definition that jumps
from a `depends_on` / `${{ tasks.X }}` reference to the task that defines it.

It is the same intelligence the `nika-vscode` extension ships client-side
today, promoted into the binary so **every** LSP client (VS Code, Cursor,
Zed, Neovim, Helix) gets it uniformly. The extension already auto-detects
the server: `nikaService` parses `nika --help`, sets `caps.lsp =
commands.has('lsp')`, and starts the LSP client the day the subcommand
ships — zero extension change.

## 2. Scope (LOCKED v0.1 · D-2026-06-10-N6)

| Feature | In v0.1 | Source |
|---|:--:|---|
| **Diagnostics** (publishDiagnostics) | ✅ | `nika_check` ladder → `lsp_types::Diagnostic` (code + message + range + severity) |
| **Hover** | ✅ | verb/keyword docs · task refs → target id+verb · task DECLARATION → the DAG card (wave k/N from the engine's own `analyze` · waits/feeds · downstream reach) · `tool:` → builtin card (category·args·required) · `model:` → catalog windows, provider-card fallback for hand-typed models · member refs → declaration cards |
| **Completion** | ✅ | top-level keys · task fields · the 4 verbs · `model:` providers → per-provider models · `tool:` + agent `tools: [` builtins · builtin `args:` keys (required-first) · `mode:` extract vocabulary (contextual to `nika:fetch`) · closed enums (`nika:` · `type:` · `capture:` · `backoff_strategy:`) · island members (`${{ inputs./secrets./env./tasks.<id>. }}`) · task refs by the law that judges each context (DAG-003 declared edges · recover carve-out · outputs freedom · depends_on anti-cycle) · JSON-Schema keyset inside `schema:` · auto-trigger on `.` `/` `[` and ` ` (value-colon pause · prose-empty pinned) |
| **Document symbols** | ✅ | every declaration: vars (typed detail) · env · secrets · tasks(verb) — the navigation twin of member go-to-definition |
| **Go-to-definition** | ✅ | a task ref (`depends_on:` item or `${{ tasks.X }}`) → the task's `id` span · a member ref (`${{ inputs.X / secrets.X / env.X }}`) → its declaration |
| **`$/cancelRequest`** (base protocol) | ✅ | the serve loop drains everything already queued into one batch and answers a request cancelled BEFORE it was computed with `-32800 RequestCancelled` — a fast-typing burst no longer computes stale results the client already discarded |
| Expression intelligence inside `${{ }}` | 🟡 | CEL completion inside `${{ }}` islands shipped (PR #170) · expression hover stays client-side meanwhile |
| Code actions / quick fixes | ✅ v0.2 (shipped 2026-07-12) | quickfix-only — the `check --fix` typed-rename engine (`offending`/`suggestion`) projected; unique-token + did-you-mean-only discipline mirrored |
| Inlay hints · semantic tokens · code lens | ❌ v0.8X | |
| Model catalog hover/compat (`model_intel`) | ❌ v0.8X | |
| Multi-file / includes · incremental reparse | ❌ v0.8X | (v0.1 is single-file, full-reparse-on-change) |

## 2bis. Custom requests (the oracle surface · vendor-prefixed)

Convention: permanent extensions live under the `nika/` prefix
(the rust-analyzer `lsp_ext` discipline) and are capability-gated via
the `experimental` field of `ServerCapabilities` — a client (or agent)
reads the advertisement instead of probing blind.

| Method | Params | Result | Capability |
|---|---|---|---|
| `nika/semanticDocument` | `{ "uri": … }` (a `TextDocumentIdentifier`) | `{ graph, reason?, spans }` (typed: `SemanticDocument`) — `graph` is the canonical `graph_format: 3` projection VERBATIM (`nika-graph::project` · the same bytes `nika inspect --format json` prints · format 3 since cleanup units became nodes) · `reason` appears ONLY when `graph` is null (`"parse"` · `"findings"` — one word, the diagnostics lane carries the details) · `spans` maps task ids to their declaring token ranges | `experimental.nika.semanticDocument.graphFormat` — derived from `nika_graph::GRAPH_FORMAT` (3 today · the advertisement and the payload share the one constant since #980) |

Measured (debug build · 500-task document · hot): hover 16 ms ·
completion 15 ms · semanticDocument 33 ms — the <50 ms budget holds
WITHOUT a cache (decision: no memoization until a release-build bench
breaks the budget; the v0.1 full-reparse contract stays).

The payload versions ITSELF (`graph.graph_format`) — evolution is
additive and spec-first (spec 03 §graph-projection moves, then the
projector, then consumers). This section is doc-synced-to-source: the
capability pin test (`experimental_advertises_the_semantic_document`)
and the byte-parity law test (`graph_half_is_the_canonical_projection_
verbatim`) fail before this table may lie.

## 3. Public API

```rust
//! crates/nika-lsp/src/lib.rs
pub mod analysis;          // pure, sync, testable without a server
pub mod capabilities;      // the v0.1 ServerCapabilities
pub mod error;             // LspError (transport/protocol)
mod server;                // the sync lsp-server loop

/// Run the language server over stdio until the client disconnects.
/// This is what the `nika lsp` subcommand calls.
pub fn run_stdio() -> Result<(), error::LspError>;
```

```rust
//! analysis/ — the pure brain (no I/O, no server state)
pub mod position;    // LineIndex: byte offset ↔ lsp Position (UTF-16)
pub mod document;    // Document: text + LineIndex + incremental apply_change
pub mod diagnostics; // CheckReport (+ parse error) → Vec<lsp Diagnostic>
pub mod symbols;     // RawWorkflow → Vec<DocumentSymbol>
pub mod definition;  // (text, offset) → Option<Location> (task ref → def)
pub mod completion;  // (text, offset) → Vec<CompletionItem>
pub mod hover;       // (text, offset) → Option<Hover>
pub mod vocab;       // the static language vocabulary (verbs, keys, docs)
```

### Analysis contract (the seam to `nika-schema`)

```rust
// parse: nika_schema::parser::parse(yaml, FileId, ParseMode) -> Result<RawWorkflow, SchemaError>
// check: nika_check::check(&RawWorkflow) -> CheckReport
//        CheckReport.{conformance, schema_findings, gate_findings, unknown_tools,
//                     unknown_args, missing_args, schema_lints, secret_leaks,
//                     secret_egresses, capability_escapes, hints}
//        ConformanceViolation { code: String, message: String, span: Option<ByteSpan{start,end:u32}> }
// spans: RawWorkflow.tasks: Vec<Spanned<RawTask>>; RawTask.id: Spanned<String>; Spanned{value,span:Span{start,end}}
```

The parse error (when `parse` returns `Err`) carries its own span
(`SchemaError::span() -> Option<Span>`) and is mapped to a single ERROR
diagnostic. When `parse` succeeds, `check` is INFALLIBLE and the report's
finding arrays map to diagnostics (severity ERROR for violations/leaks/
escapes/findings, WARNING/HINT for `hints`).

## 4. Module budget (LOC estimates)

```
analysis/position.rs    ~120   LineIndex + UTF-16 conversion        (proptest: round-trip)
analysis/document.rs    ~120   text store + incremental apply_change (proptest: apply≡full-replace)
analysis/diagnostics.rs ~260   the 11 finding arrays → diagnostics
analysis/symbols.rs     ~120   tasks → DocumentSymbol tree
analysis/definition.rs  ~160   task-ref-at-offset → def Location
analysis/completion.rs  ~320   context-aware items (keys/verbs/providers/task ids)
analysis/hover.rs       ~200   verb + keyword docs
analysis/vocab.rs       ~200   the static vocabulary tables (1 source)
capabilities.rs          ~60   the v0.1 ServerCapabilities
error.rs                 ~40   LspError (#[non_exhaustive])
server.rs               ~360   the sync lsp-server dispatch loop
─────                   ─────
TOTAL                  ~1,960  (well under the 4,000 budget)
```

## 5. 12-gate plan

| Gate | Plan |
|---|---|
| 1 SPEC | this file ✅ |
| 2 TDD | tests-first per analysis module (position round-trip · diagnostics mapping · symbols · definition · completion context · hover) RED→GREEN |
| 3 IMPL | the modules above · 0 `.unwrap()`/`.expect()` in `src/` · `?` + `unwrap_or` |
| 4 CLIPPY | `cargo clippy -p nika-lsp --all-targets -- -D warnings` = 0 |
| 5 MUTATION | `cargo mutants -p nika-lsp` ≥90% killed (the analysis modules are pure → high kill; the stdio loop in `server.rs` is the documented Rule-2 exemption — its body is I/O-bound JSON-RPC dispatch with no return value to mutate meaningfully) |
| 6 PROPERTY | proptest on `position` (offset↔Position round-trip on arbitrary UTF-8) + `document` (sequence of `apply_change` ≡ full text replace) — the encoding/position class ADR-092 cares about |
| 7 BENCHMARKS | N/A — interactive per-keystroke analysis over single files is not a throughput hot path; full-reparse on a 1k-task cap is bounded. Justified exemption. |
| 8 DOCS | `cargo doc -p nika-lsp --no-deps` 0 warnings · all pub items documented |
| 9 CANARY E2E | a test drives `run_stdio` over an in-memory pipe: initialize → didOpen(a broken workflow) → assert a `NIKA-*` diagnostic is published; didOpen(`01-hello`) → hover/completion/symbols/definition return the expected shapes |
| 10 PARITY LEGACY | N/A by design — the brouillon LSP was a different stack (`tower-lsp`) over a different parser (2-phase AST vs Diamond `nika-schema`); there is no byte-identical legacy surface to golden-test. The diagnostics are golden-tested against `nika check` output instead (same ladder, same codes). Justified. |
| 11 REVIEW | 3-agent swarm: spn-nika:code-reviewer + spn-rust:rust-pro + feature-dev:code-reviewer · P0/P1 fixed same session |
| 12 ATOMIC | `feat(nika-lsp): admit to workspace — all 12 gates passed` · co-authored Nika 🦋 |

## 6. Invariants honoured

- **One source of truth for diagnostics** — the LSP calls the SAME
  `nika_check` the CLI does. No second parser, no drift (the
  brouillon's separate tree-sitter recovery path is deliberately NOT ported
  for v0.1; full-reparse on the real parser keeps codes/spans identical to
  `nika check`).
- **Pure analysis** — every `analysis::*` function is `(text[, offset]) →
  value`, no I/O, no server state → unit + property testable without a
  client. The server is a thin transport shell.
- **Locked vocabulary** — completion/hover read the 4 verbs (`infer · exec ·
  invoke · agent`, LOCKED forever · D-N18) and the spec's top-level/task-field
  keys from `vocab.rs` (one table · the `nika-pack` embedded schema is the
  upstream truth · v0.1 mirrors the stable subset).
- **UTF-16 positions** — LSP default `PositionEncodingKind`; `LineIndex`
  converts byte spans to UTF-16 line/character (é = 1 unit, 🦋 = 2 units).

## 7. Gate-5 mutation exemptions (audited surviving mutants)

`cargo mutants -p nika-lsp` reports **315 caught · 10 missed · 15 unviable ·
12 timeouts** (352 mutants). Caught-rate over the killable surface =
**315 / (315 + 10 surviving) ≈ 96.9 %**, and **315 / (315 + 10 + 12) ≈ 93.5 %**
when the non-terminating timeouts count against. Both clear the ≥90 % Gate-5
floor, and all 10 surviving mutants are documented exemptions below — the
genuinely-killable kill rate is **100 %**.

Every surviving mutant below was hand-audited and falls into one of three
classes — none is a real, killable test gap.

### (a) Rule-2 I/O exemption — the stdio transport (3 mutants)

The `server.rs` / `lib.rs` stdio loop is the documented Rule-2 exemption: its
dispatch is exercised end-to-end by the `canary` E2E + the direct
`handle_request` / `handle_notification` tests, but the OUTER lifecycle has no
observable return value to mutate meaningfully without a real OS pipe.

| Mutant | Why exempt |
|---|---|
| `lib.rs:66 run_stdio → Ok(())` | the public stdio entry; only observable over a real stdin/stdout pipe (Rule-2). |
| `server.rs:85 run_stdio → Ok(())` | same — drives `Connection::stdio()` + thread join. |
| `server.rs:54 delete - (INTERNAL_ERROR = -32603)` | the `-32603` constant is used ONLY in `respond`'s `ExtractError::MethodMismatch` arm, which is unreachable in practice (the handler always extracts with the request's own method). Defensive dead code. The sibling `INVALID_PARAMS = -32602` IS tested (`malformed_request_params_yield_a_json_rpc_invalid_params_error`). |

### (b) Equivalent mutants — loop-bound / guard short-circuits (7 mutants)

Each changes a comparison/operator at a point where the mutated and original
expressions are provably identical for every reachable input. Contorting a
test to "kill" these would assert nothing real.

| Mutant | Why equivalent |
|---|---|
| `position.rs:126 > with >=` (`idx > 0`) | only differs at `idx == 0`, where `str::is_char_boundary(0)` is always true → `!is_char_boundary(0)` is false → the `&&` exits the loop identically. (The `==`/`<` variants on the SAME line ARE killed by `offset_inside_second_multibyte_floors_to_prior_boundary`.) |
| `definition.rs:86 > with >=` (`kw_at > 0`) | `kw_at = island_start + …` and `island_start ≥ 3` (a `${{` always precedes), so `kw_at` is never 0 → `>` and `>=` never differ; even at the unreachable 0 the `&&` short-circuits identically. |
| `definition.rs:112 < with <=` (islands `i < len`) | at `i == len` the body sees an empty slice, fails `starts_with("${{")`, increments, and the next check exits — one harmless no-op iteration, same island list. |
| `definition.rs:127 + with -` (`i = close + 2`) | the bytes at `close`/`close+1` are always `}}`, so `close - 2` re-scans forward and can never re-match `${{` (no `$` at `close-2..close`); it always makes net progress and terminates with the identical island list. |
| `diagnostics.rs:202 delete field code in hint_diag` | the explicit `code: None` equals the `..Diagnostic::default()` value (also `None`) — deleting the field is a no-op. (The `range`/`source`/`message` field deletions on the SAME struct ARE killed by `hint_diagnostic_carries_exact_fields`.) |
| `hover.rs:83 + with *` (`end = probe + 1`) | `probe * 1 == probe`; the right-expansion loop immediately re-checks `get(probe)` (always an ident byte — it just passed the line-76 guard) and converges to the identical `end`. |
| `hover.rs:84 < with <=` (`end < len`) | at `end == len`, `bytes.get(len)` is `None` → `is_ident_byte(None)` is false → the loop exits; the extra `<=` check short-circuits to the same result. |

### (c) Non-terminating timeouts — counted as detected (12 mutants)

All 12 timeouts are loop-counter mutations (`+= → *=`, `-= → /=`, or a
`find_close → Some(0/1)` that never advances) in `islands` / `find_close` /
`ident_end` / `backslash_run_is_odd` / `word_at` / `floor_char_boundary`.
Each turns a monotone loop counter into one that never makes progress → an
**infinite loop**. A hung test IS a detection (the behaviour is provably
broken — the program never returns), so these are effectively caught; they
surface as `TIMEOUT` rather than `MISSED` only because cargo-mutants caps the
wall-clock instead of letting the test spin forever. Not real gaps.
