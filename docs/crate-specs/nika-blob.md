# Crate spec — `nika-blob`

| | |
|---|---|
| Status | **ADMITTED 2026-06-10** (`e91adcef2`) · was **L1 admission target** (Phase-B slice step 6 · announce ladder per D-2026-06-10-N6 cascade) |
| Layer | L1 — effect crate · the only production site touching the blob filesystem |
| Design | `DiskBlobStore` impl of the L0.5 `nika_kernel::blob::BlobStore` trait via the `BlobStoreDyn` (`Send`) companion · blake3 CAS |
| LOC budget | well under the ≤1500/file + ≤15k/crate caps (enforced live by vectors 12+24) · live count · `scripts/crate-metrics.sh nika-blob` |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L1 effect crate |
| NIKA codes | none — speaks the kernel `BlobError` (`NotFound` · `Io` · `TooLarge`) |

---

## 1. Purpose

`nika-blob` is the **production content-addressed blob store**. It
provides `DiskBlobStore`, backed by `tokio::fs` with **blake3** hashes
as keys over a sharded layout (`{root}/ab/cdef…`). Media + artifacts
(thumbnails, fetched pages, generated files) land here; the hash is the
identity, so identical content dedups for free.

It is the only production site touching the blob filesystem — tests
inject `nika-kernel-mock::MockBlob`. Effect-crate discipline (Invariant
#27): one effect per crate.

## 2. Public API

```rust
pub struct DiskBlobStore { /* root + max_size */ }

impl DiskBlobStore {
    pub fn new(root) -> Self;                  // default 500 MiB cap
    pub fn with_max_size(root, max) -> Self;
    pub fn root(&self) -> &Path;
    pub fn max_size(&self) -> u64;             // configured cap (default 500 MiB)
}
impl BlobStoreDyn for DiskBlobStore { put · get · exists · stat · delete }
```

Implementation targets the `BlobStoreDyn` trait-variant companion (the
base `BlobStore` arrives via the `trait_variant` blanket impl · same
pattern as `nika-fs`/`nika-http`/`nika-clock`).

## 3. Design (the Diamond upgrades vs brouillon · CRAFT · ADR-001)

1. **Sidecar mime** — brouillon's `put(mime)` discarded the mime and
   `stat` re-sniffed it from magic bytes, so `put("x","text/plain")`
   then `stat` returned `application/octet-stream` (a silent round-trip
   divergence). Diamond records the declared mime in a `.mime` sidecar
   written atomically beside the blob → `put`/`stat` agree. `stat`
   falls back to `application/octet-stream` when a sidecar is absent
   (foreign/older writer). The store records; it never guesses (content
   sniffing is a higher-layer concern, and dropping the `infer` crate
   keeps the dep surface minimal + sovereign).
2. **Unique temp name** — brouillon used `path.with_extension("tmp")`,
   which COLLIDES under concurrent puts of identical content (same hash
   → same temp path). Diamond uses `.nika-tmp.{pid}.{counter}` (the
   nika-fs pattern), so concurrent dedup races never clobber.
3. **Atomic both files** — bytes AND sidecar each land via temp+rename.
4. **Dedup** — `try_exists` short-circuits the byte write; the sidecar
   is refreshed so a re-put with a new declared mime updates the record.
5. **Hash validation up-front** — every caller-supplied hash passes
   `canonical_raw` (`blake3:` prefix optional + case-insensitive · 64
   lowercase hex) BEFORE touching the filesystem. A malformed hash is
   `NotFound` (or `false` for `exists`), never a slice panic — this
   closes the non-ASCII byte-slice panic vector on the attacker-reachable
   read/delete paths, and makes the path helpers branch-free (no
   degenerate `<2`-char case). Uppercase hex now resolves (normalized).
6. **Blank inputs rejected** — empty blob AND blank `mime_type` raise
   `BlobError::Io` (a blank mime is as meaningless as blank content); a
   blank/absent sidecar reads back as `application/octet-stream`, never
   an empty mime.
7. **`delete` contract** — returns `NotFound` when the blob is already
   absent (the codebase-wide missing-blob contract · NOT a silent
   no-op); the sidecar unlink is best-effort.
8. **Zero `.unwrap()`** in src; the kernel trait's CAS cancel-safety is
   explicit per method.

CAS cancel-safety (kernel contract): a dropped `put` leaves at most a
temp file nobody addresses — existing blobs are never corrupted because
partial content hashes differently.

## 4. The 12 gates

| Gate | Status | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this file |
| 2 TDD | ✅ | `tests/blob_contract.rs` authored first · RED (todo! skeleton) → GREEN |
| 3 IMPL | ✅ | ~393 LOC src (live · `scripts/crate-metrics.sh nika-blob`) · zero unwrap/expect in src |
| 4 CLIPPY 0 | ✅ | `cargo clippy --workspace --all-targets -- -D warnings` GREEN |
| 5 MUTATION ≥90% | ✅ | `cargo mutants -p nika-blob` · 38 mutants · **33 caught / 33 viable = 100%** (5 unviable). Survivors killed across the arc: the `500*1024*1024` cap arithmetic (→ `max_size()` accessor + exact-value test) · the two stat sidecar guards `!s.trim().is_empty()` + `e.kind()==NotFound` (→ empty-sidecar + dir-sidecar boundary tests) |
| 6 PROPERTY | ✅ | put→get roundtrip on arbitrary 1..2048-byte payloads · cross-store hash determinism (48 cases) |
| 7 BENCH | N/A | thin tokio::fs + blake3 wrapper, no algorithmic hot path (justified — clock/fs precedent) |
| 8 DOCS | ✅ | `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` 0 warnings · private-item rustdoc clean |
| 9 CANARY | N/A | L1 effect, no `.nika.yaml` surface until L2 (justified) |
| 10 PARITY | ✅ | brouillon CAS behaviours re-asserted (blake3 prefix · sharding · dedup · NotFound) · blake3("hello") known-vector pinned · Diamond ADDS sidecar mime + unique temp + size cap as a ctor knob |
| 11 REVIEW | ✅ | 3-agent swarm 2026-06-10 · 0 P0 · 1 P1 (README « idempotent » claim · false vs the NotFound-on-missing contract · fixed) · P2s fixed same-session: non-ASCII hash slice-panic → `canonical_raw` up-front validation (also kills the degenerate branches + normalizes uppercase) · stat error-swallow narrowed to NotFound-only · blank-mime rejected · empty-sidecar → octet-stream · spec API + delete contract corrected · prefixless/uppercase/malformed tests added |
| 12 ATOMIC | ✅ | 1 commit · Nika 🦋 trailer |

## 5. Consumers (downstream)

`nika-builtin` (s16 · media builtins · the `nika:fetch` tool caches
fetched bytes), `nika-extract` (artifacts), future `nika-media`
(thumbnails/pdf/convert · feature-gated stdlib). The CAS hash is the
stable handle workflows pass between tasks.

## 6. Dependencies

| dep | why | layer-legal |
|---|---|---|
| `nika-kernel` (path) | trait + type + error contracts | L0.5 ← L1 ✓ |
| `blake3` (workspace pin 1) | content-addressed hashing · CC0-1.0 OR Apache-2.0 (allowlisted) | L1 ✓ |
| `bytes` | kernel put/get payloads | ✓ |
| `tokio` (`fs`) | the storage backend | L1+ ✓ |
| dev: `proptest` · `tempfile` | Gate 6 + tempdir fixtures | dev-only |

deny.toml `tokio` wrapper extended with nika-blob. blake3 added to
`[workspace.dependencies]` (RUST_ENFORCEMENT §2 pin-once). No `infer`
dep — the sidecar records the declared mime rather than sniffing.
