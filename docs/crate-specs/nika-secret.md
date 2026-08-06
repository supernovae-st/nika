# Crate spec — `nika-secret`

| | |
|---|---|
| Status | **ADMITTED** 2026-08-06 — a descent, not a new design: the code shipped and was tested inside `nika-runtime` before it moved. |
| Layer | L1 — pure resolution vocabulary over `nika-schema` declarations; zero I/O of its own (the *store* lives behind the caller's trait impl) |
| Design | The **resolution half** of the secret story: who can be asked for a value, what an ask can fail with, and which declared sources the runtime may resolve at all. |
| LOC budget | ≤600 src (actual ~160), ≤800 hard cap |
| File cap | ≤1,500 LOC (single file, ~160) |
| Function cap | ≤100 lines |
| License | `AGPL-3.0-or-later` · Edition 2024 · `publish = false` |
| Extraction source | `crates/nika-runtime/src/secret.rs` (the head of the file, above the `RedactingSink` doc block) |
| NIKA codes | **none** — `SecretResolveError` is a store-supplied message the composer wraps; it never allocates its own range |

---

## 1. Purpose

`nika-secret` owns the vocabulary a workflow's `secrets:` block resolves
through:

- `WorkflowSecretResolver` — the trait a host implements to answer "what is
  the value behind this reference?" The engine never reads a store itself;
  it asks.
- `NoSecrets` — the refusing default. A composer that was handed no store
  resolves nothing, rather than silently falling back to the process
  environment.
- `resolve_secrets` — the declaration walk: for each declared secret whose
  source the runtime may resolve, ask once, collect `name → value`.
- `source_is_runtime_resolvable` — the closed judgment over `SecretSource`.
  A source the runtime cannot resolve is skipped here and refused upstream,
  never guessed at.
- `REDACTED` — the single masking token every lane agrees on.

## 2. What it does NOT own

The **scrub half** stays in `nika-runtime::secret`: `RedactingSink`,
`scrub_outputs`, `scrub_value`, and the two constants that tune them
(`WIDE_SCRUB_MIN`, `PAYLOAD_FIELDS`). Those are event-shaped — they wrap the
runtime's `EventSink` and know which frame fields can carry a value — so
they belong to the crate that owns the event lane, not to the vocabulary.

The split point is exactly that seam: everything above it answers *what is
the value*, everything below answers *where must it never appear*.

## 3. Why the descent happened

`nika-runtime` reached its 15,000 prod-LOC wall (the L3 orchestrator is the
crate every feature lands in, so it is the crate that hits the wall first).
The resolution half was the cleanest candidate: it needs nothing from the
runtime — no `EventSink`, no compose ladder, no session state — and it had
exactly one consumer, `lib.rs`, which now re-exports it so every historical
`nika_runtime::WorkflowSecretResolver` path still resolves.

This is the fourth descent of the same shape (`nika-proof`, `nika-dap`,
`nika-cap` precede it). The pattern: find the module the wall-crate merely
*hosts* rather than *owns*, give it its own crate, re-export for compat.

## 4. Gates

| Gate | Verdict |
|---|---|
| 1 SPEC | ✅ this document |
| 2 TDD | ✅ inherited — the tests were written RED before the code, in `nika-runtime`, and moved with the re-export intact |
| 3 IMPL | ✅ ~160 prod LOC, compiles, workspace green |
| 4 CLIPPY | ✅ 0 warnings, both feature states |
| 5 MUTATION | ✅ inherited from the source module's admission |
| 6 PROPERTY | ✅ the resolution proptests run against the re-export |
| 7 BENCHMARKS | N/A — a `BTreeMap` walk over a declaration list, never a hot path |
| 8 DOCS | ✅ every public item documented |
| 9 CANARY | N/A — exercised by every workflow carrying a `secrets:` block |
| 10 PARITY | N/A — a move, byte-identical behavior; the workspace suite is the parity proof |
| 11 REVIEW | ✅ the descent carries the reviews the source module already passed |
| 12 ATOMIC | ✅ rides with the B4.5 seat commit that the wall blocked |

## 5. Related

- `crates/nika-runtime/src/secret.rs` — the scrub half, the other side of the seam
- `docs/crate-specs/nika-cap.md` — the closest descent precedent
- spec `01-envelope.md` §secrets — the declaration this crate resolves
