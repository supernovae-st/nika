# Crate spec — `nika-kernel-core`

| | |
|---|---|
| Status | Admitted (kernel 4-way split · census 2026-06-10) |
| Layer | L0.5 (TRAITS ONLY · zero I/O · zero impl) |
| Design | Base sibling of the split kernel — `io/` + `infra/` + root primitives |
| LOC budget | ≤6,000 src |
| File cap | ≤1,500 LOC each |
| License | `AGPL-3.0-or-later` |

## 1. Purpose

The BASE sibling of the 4-way kernel split
(`docs/architecture/kernel-split-census-2026-06-10.md`). Carries the
27-trait foundation every other sibling depends on ·

- `io/` — the 10 effect-domain contracts (a11y · blob · browser · clock ·
  fs · http · input · ocr · process · screen · 19 traits)
- `infra/` — the 7 observability/identity sinks (audit · billing ·
  event_sink · id_gen · metrics · secret · trace)
- root primitives — `cancel::CancelCtx` · `sealed::Sealed` (ADR-014 soft
  seal · the ONE seal shared by all siblings) · `types`

Code moved VERBATIM from `nika-kernel` (already 12-gate admitted there ·
the move is mechanical · in-module tests moved with the code). Consumers
keep importing through the `nika-kernel` facade.

## 2. Gate exemptions (documented per Rule 2)

- Gate 5 MUTATION · inherited — the moved code's mutation posture is the
  kernel's (traits-only · the testable surface is small companion impls ·
  covered by the moved in-module tests).
- Gate 7 BENCHMARKS · N/A (trait definitions · no hot path).
- Gate 9 CANARY · N/A (no executable surface).
- Gate 10 PARITY · N/A (no legacy cognate — the split is Diamond-internal).

## 3. Invariants

- NO dependency on any other kernel sibling (the base of the DAG).
- `#[non_exhaustive]` ratchet + sealed-trait contract unchanged.
- New io/infra traits land HERE (never in the hub).

## 4. Filesystem backend migration

`FsWrite::write_new(path, contents)` is a provided method, also present on
the generated `FsWriteDyn` Send companion. It requests publication of complete
bytes at an unoccupied destination. It is distinct from `write`, which keeps
its existing create-or-replace contract. No workflow argument or result type
is introduced by this Rust interface addition.

The default returns `FsError::Io` without invoking `write`, `create_dir_all`,
or `remove_file`. Backends implementing only those original three methods
remain source-compatible. They refuse `nika:write` with `overwrite:false`
until they implement the exclusive operation; they must never simulate it
with a separate `exists` check and a replacing write. Backend wrappers must
forward the new method, or they retain the unsupported default even when
their inner backend implements it.

Implementations report `FsError::AlreadyExists` for an occupied destination
without changing that entry. A successful exclusive publication does not by
itself promise `fsync`, cancellation quiescence, or a multi-file transaction.
A caller abandoning the future cannot infer absence of effects from that
action alone. Each implementation documents its commit and cleanup behavior.

`io/fs/legacy_write_tests.rs` defines separate base and Send backends with
only the original three methods. Immediate polling checks that both refuse
without touching operation counters, including the Send backend's generated
base implementation. These are interface/default checks, not real IO or
runtime cancellation tests.

`FsWrite::remove_regular_file(path)` is a second provided method on the same
terms, also present on `FsWriteDyn`. It removes only the regular file the path
names: an absent name is `FsError::NotFound`, a symlink as the final name is
`SymlinkRefused` (neither the link nor its target is touched), and a
directory, a special node or a path that names no final file (empty, root,
`.`, `..`, a trailing separator) is `InvalidData`. The file's content is never
opened or read and no parent is created. It is distinct from `remove_file`,
whose raw unlink of any non-directory name, symlinks included, is unchanged.

The default returns `FsError::Io` immediately without invoking any other
method. Backends implementing only the three required methods remain
source-compatible and refuse regular-only removals until they override it;
they must never approximate it with `metadata` (which follows links) followed
by `remove_file`. Backend wrappers must forward the method, or they keep the
refusing default even when their inner backend implements it. The trait adds
no new error variant, type or code, and no workflow builtin is attached to it
by this addition.

Implementations classify the final name and then unlink it: two steps, not
an atomic compare-and-remove of one inode. A name substituted between them may
be removed in its place, a substituted symlink being unlinked, never followed.
Dropping the future does not prove the removal stopped. No transaction,
rollback, durability or retry is implied.

`io/fs/remove_default_tests.rs` defines separate base and Send backends with
only the three required methods and checks, by immediate polling, that the new
method refuses on both (and on the Send backend's generated base
implementation) with every operation counter still zero, then that the three
required operations remain callable.

## HTTP attempt capability

The additive default-false `HttpPost::supports_single_attempt()` attests that
one `post` makes at most one transport attempt and honors disabled redirects.
A custom effect must opt in explicitly; old implementations remain compatible
and are refused by consumers that require bounded inference accounting.
