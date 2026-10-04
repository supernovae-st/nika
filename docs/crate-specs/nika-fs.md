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
| NIKA codes | kernel `FsError` taxonomy — no crate-owned error enum; `OwnedDir` returns `std::io::Result` |

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
impl FsWriteDyn for TokioFs {
  write (temp+rename, replaces) · write_new (complete temp+exclusive hard link)
  create_dir_all · remove_file · remove_regular_file (held parent · no-follow · unlinkat)
}
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

`OwnedDir::append_line` tolerates an initial-create ENOENT race by reopening the
existing child once without O_CREAT, before any write. It frames the line and its newline in one append write,
then synchronizes the file. A short write is reported as an uncertain partial
effect; it is never completed by a second write that could interleave another
writer's row. This mechanism does not replace a caller's transaction lease,
and arbitrary filesystems still need their own append/locking guarantees.

### Caller-selected files and contained descendants

`open_owned(path)` resolves the caller-selected parent once, holds it and opens
only the final regular file without following a link or waiting for a FIFO.
`read_owned(path, cap)` reads UTF-8 on that descriptor with a cap+1 probe, refusing
oversize input. Missing parent/file is `None`; other failures remain errors.
These helpers are not containment for a root joined to an untrusted relative
path: use `OwnedDir::open_relative` below the held root for those descendants.
The existing `OwnedDir::open` component rules are unchanged.

### Exclusive publication and backend migration

`write_new(path, contents)` publishes a complete file only if the destination
name is unoccupied. TokioFs writes arbitrary bytes to an exclusively created
temporary sibling, then publishes with `std::fs::hard_link`. An existing file,
directory, or symlink is preserved and returns `FsError::AlreadyExists`.
Unsupported filesystem operations fail without falling back to replacement.
The ordinary `write` operation continues to create or replace by rename;
other writers using that operation retain their replacement semantics.

The exclusive operation runs in `spawn_blocking`. Dropping its future does
not stop that worker: publication can complete after the caller stops waiting.
Complete, exclusive publication is not crash durability; this operation adds
no `fsync`. Once the link succeeds, the destination is committed. Removing
the temporary name is best-effort cleanup, and a cleanup failure can leave a
temporary alias while the operation still reports successful publication.
An interrupted observer must establish the final state independently rather
than treating cancellation as proof that no file was written.

The kernel method has a provided default that returns `FsError::Io` without
calling any filesystem operation. Existing backend implementations still
compile with their original three methods, but `nika:write` with
`overwrite:false` refuses until the backend overrides `write_new` with the
exclusive contract. Wrappers must forward that method explicitly, including
wrappers whose inner backend already supports it. An `exists` check followed
by ordinary `write` is not a valid implementation. This migration changes no
workflow arguments or result shape: the builtin still returns its path.

The lib tests in `src/write_new_tests.rs` use real filesystem readers and
directory inventories to check one winner, exact bytes, destination absence
before publication, occupied destinations, normal cleanup, and preservation
of ordinary replacement. These checks do
not establish crash durability, worker quiescence, or every filesystem's
behavior. The earlier admission results below do not cover this new method.

### Regular-file removal

`remove_regular_file(path)` removes only a regular file. The private helper
`remove::remove_regular_at(parent, name, shown)` classifies the final name
with `fstatat(AT_SYMLINK_NOFOLLOW)` and unlinks it with
`unlinkat(NoRemoveDir)` relative to the same held parent descriptor. It never
opens or reads the file, creates nothing, retries nothing and touches no ledger
state. Absent is `NotFound`, a symlink `SymlinkRefused`, a directory or a
special node `InvalidData`; other failures keep their translated errno. The
raw spelling is checked before any `Path` decomposition, so empty, root, `.`,
`..`, `file/` and `file/.` are `InvalidData` and never become `file`.

TokioFs holds the existing parent with `OwnedDir::open` (no symlink at any of
its components, nothing created; a bare name's parent is `.`) and runs the
helper in `spawn_blocking`. An ancestor refused by that open keeps its
translated I/O error. The host backend is not a permit sandbox: it acts on the
path its caller chose. RootedFs keeps its path law (absolute, `..` and
non-UTF-8 refused), opens the parents with its existing walk, and registers
the removal as a write of the current phase: read-back and closed rooms refuse
it, no budget is refunded and `written()` keeps its history. The raw
`remove_file` of both backends, which unlinks a symlink name without touching
its target, is unchanged.

The check and the unlink are two steps, not an atomic compare-and-remove of
one inode: a name substituted between them may be removed in its place, a
substituted link being unlinked, never followed. A held parent stops a renamed
or replaced ancestor name from redirecting the removal; it does not re-check
where the held directory now sits. A dropped future may let the removal finish
in the background.

The lib tests in `src/remove_tests.rs` use a private parent per test (the room
harness for RootedFs) and check removal of a binary and an unreadable file
with neighbours unchanged, refusals for absent names and missing parents (none
created), internal, external and dangling links, directories, FIFOs, raw
spellings and escapes with every name and target unchanged, the raw removal
still unlinking a link the new operation refuses, a symlinked ancestor, the
removal staying in a held parent after its visible name was replaced, phase
refusals, and no budget refund. They do not drive a substitution between classification and unlink, or
completion of a dropped removal. The separate `tests/remove_relative.rs`
exercises a bare relative host name in an isolated process. The workflow
callable `nika:remove_file` delegates to this method through `JudgedFs`; the
backend itself grants no permits.

### Diamond upgrades vs brouillon (CRAFT · ADR-001)

1. **Atomic write** — brouillon used bare `tokio::fs::write` (partial
   writes observable). Diamond writes to `.nika-tmp.{pid}.{counter}` (or `..nika-tmp.{pid}.{counter}` for a single-dot-prefixed destination, keeping staging distinct under case folding)
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
| 9 CANARY | N/A | L1 effect, no `.nika` surface until L2 verbs land (justified — same class as clock/screen/ocr) |
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
