# Crate spec — `nika-fs`

| | |
|---|---|
| Status | **ADMITTED 2026-06-10** (`47825df4a`) · was **L1 admission target** (Phase-B slice step 4 · announce ladder per D-2026-06-10-N6 cascade) |
| Layer | L1 — filesystem effect mechanisms |
| Design | `TokioFs` ZST impl of the L0.5 `nika_kernel::fs` family via the `*Dyn` (`Send`) companions · `OwnedDir` for descriptor-rooted synchronous ownership |
| LOC budget | well under the ≤1500/file + ≤15k/crate caps (enforced live by vectors 12+24) · live count · `scripts/crate-metrics.sh nika-fs` |
| Function cap | ≤100 lines each (largest: `write` ~40) |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L1 effect crate |
| NIKA codes | none — the kernel fs contract speaks `std::io::Result` (no crate error enum, like nika-clock) |

---

## 1. Purpose

`nika-fs` is the **production filesystem effect**. It provides `TokioFs`,
the real-I/O implementation of the four L0.5 kernel traits (`FsRead` ·
`FsWrite` · `FsMeta` · `FsList`, ISP split — and therefore the blanket
`Fs` umbrella) using `tokio::fs` + `globset`, and `OwnedDir`, the
descriptor-rooted mechanism for crash-durable sidecars whose visible path
may be replaced by another process.

It is the **only** place `tokio::fs` is touched on the production path —
pure crates (L0) and the kernel (L0.5) stay filesystem-free; tests inject
`nika-kernel-mock::MockFs`. Effect-crate discipline (Invariant #27): one
effect family per crate.

**Mechanism, not policy**: path capability gating (sandbox roots,
allow-lists, traversal policy) belongs to `nika-policy` (L1.5 · ladder
step 8). Keeping the effect crate policy-free lets the policy layer
reason about ALL filesystem access in one place.

## 2. Public API

```rust
/// Zero-size production filesystem. Copy + Default.
pub struct TokioFs;
pub struct OwnedDir; // held dirfd · contained components · nofollow children

impl FsReadDyn  for TokioFs { read · read_to_string · exists · canonicalize }
impl FsWriteDyn for TokioFs { write (ATOMIC temp+rename) · create_dir_all · remove_file }
impl FsMetaDyn  for TokioFs { metadata }
impl FsListDyn  for TokioFs { list_dir (sorted) · glob (literal_separator · sorted) }

impl OwnedDir {
  create · try_clone · open_lock · read[_optional] · append_line
  write_atomic · names · exists · hard_link · remove
}
```

Implementation targets the `*Dyn` trait-variant companions (the
`Send`-future forms): the base traits + `Fs` umbrella arrive via the
`trait_variant` blanket impls, and every future is `Send` —
consumers can `tokio::spawn` filesystem work. (The `*Dyn` forms are
generic bounds, NOT dyn-dispatch surfaces — RPITIT is not object-safe;
per the kernel doc, L1 impls fan out via `Arc<T>`, not `Arc<dyn _>`.)

### Diamond upgrades vs brouillon (CRAFT · ADR-001)

1. **Atomic write** — brouillon used bare `tokio::fs::write` (partial
   writes observable). Diamond writes to `.nika-tmp.{pid}.{counter}`
   beside the destination then `rename`s (POSIX atomicity) · parents
   auto-created (parity) · error path cleans the temp best-effort ·
   no `fsync` at this layer (documented · durability is a policy/engine
   concern).
2. **4-trait ISP split** — brouillon implemented 2 fat traits
   (`FsRead`+`FsWrite` with metadata/glob mixed in); Diamond follows the
   kernel split (read/write/meta/list).
3. **Iterative glob walk** — brouillon recursed with `Box::pin`; Diamond
   walks a `Vec` stack (no per-dir alloc, no recursion-depth concern).
4. **`list_dir`** — new surface (kernel `FsList`), sorted deterministic.
5. **Native async** — `trait_variant` companions (brouillon: `#[async_trait]`).
6. **Descriptor-rooted ownership** — every `OwnedDir` operation resolves a
   single child beneath a held directory descriptor. Directory and file
   opens use `O_NOFOLLOW`; replacing a visible sidecar between a claim and
   its receipt cannot redirect later bytes.

### Glob semantics (brouillon parity · locked by tests)

`literal_separator(true)` — `*` never crosses `/`, `**` matches zero or
more components. Matching runs against the root-RELATIVE path. Hidden
directories (`.name`) are not traversed. Symlinked directories are not
followed (`file_type` is lstat-like) — cycles terminate. Results sorted.

## 3. The 12 gates

| Gate | Status | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this file |
| 2 TDD | ✅ | `tests/fs_contract.rs` + focused `OwnedDir` containment, symlink, and path-replacement tests |
| 3 IMPL | ✅ | live count · `scripts/crate-metrics.sh nika-fs` · zero unwrap/expect in src |
| 4 CLIPPY 0 | ✅ | `cargo clippy --workspace --all-targets -- -D warnings` GREEN |
| 5 MUTATION ≥90% | ✅ | Existing async effect surface: 19/19 viable caught. `OwnedDir` focused serial run: 70 mutants · 41 caught · 6 unviable · 19 algebraically equivalent OR→XOR flag mutations · **41/45 non-equivalent viable = 91.11%**. |
| 6 PROPERTY | ✅ | 2 proptest invariants · arbitrary-bytes write→read roundtrip · glob returns exactly the created suffix set (32 cases each) |
| 7 BENCH | N/A | thin `tokio::fs` wrappers, no algorithmic hot path (justified — same class as nika-clock) |
| 8 DOCS | ✅ | `RUSTDOCFLAGS=-D warnings cargo doc --no-deps -p nika-fs` 0 warnings · every pub item + per-method CANCEL SAFETY |
| 9 CANARY | N/A | L1 effect, no `.nika.yaml` surface until L2 verbs land (justified — same class as clock/screen/ocr) |
| 10 PARITY | ✅ | brouillon `tools/nika-fs` read via `git show brouillon:` · all 12 brouillon test behaviours re-asserted (roundtrips · parent auto-create · hidden-dir skip · `**` recursion · sorted) · Diamond ADDS atomic write + `list_dir` + 4-trait split — CRAFT-fresh per ADR-001 |
| 11 REVIEW | ✅ | 3-agent swarm 2026-06-10 (spn-nika:code-reviewer + spn-rust:rust-pro + feature-dev:code-reviewer) · verdicts 3× approve-with-P2 · **0 P0/P1** · 8 P2 ALL fixed same session: glob strip_prefix explicit-skip · discriminator-only temp name (ENAMETOOLONG + lossy-collision) · rename-over-dir error-kind assert · dup zero-size test removed · byte-level hidden check (non-UTF-8 fails-closed, was fails-open) · replace-semantics documented (perms/hardlinks/symlink) + pinned by cfg(unix) test · `tmp_sibling` pure helper (empty-parent arm unit-tested) · detach-not-abort cancel doc |
| 12 ATOMIC | ✅ | 1 commit · Nika 🦋 trailer |

## 4. Consumers (downstream)

Every crate needing filesystem access injects the kernel fs traits and
receives `TokioFs` in production, `MockFs` in tests. `nika-cli` also
consumes `OwnedDir` for the `.nika/arm/<label>` evidence sidecar: the L4
adapter chooses policy and names while L1 owns the reusable kernel mechanism.
First consumers on
the announce ladder: `nika-policy` (step 8 · path capability gating wraps
these primitives), `nika-builtin` (step 16 · `nika:file.*` builtins),
`nika-engine` (step 17 · workflow/source loading), `nika-cli` (step 19 ·
`nika run`/`check` file resolution + the embedded-spec extraction path).

## 5. Dependencies

| dep | why | layer-legal |
|---|---|---|
| `nika-kernel` (path) | the trait contracts | L0.5 ← L1 ✓ |
| `tokio` (`fs` feature) | the I/O backend | L1+ effect ✓ |
| `bytes` | `FsRead::read` zero-copy payload (kernel surface) | ✓ |
| `globset` | `FsList::glob` matcher · MIT OR Unlicense · cargo-deny GREEN | ✓ |
| `nix` (`fs`, `dir`) | `openat`/`mkdirat`/`renameat` + `O_NOFOLLOW` ownership | L1 effect ✓ |
| dev: `proptest` · `tempfile` | Gate 6 + tempdir fixtures | dev-only |
