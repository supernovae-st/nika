# Crate spec — `nika-migrate` (Gate 1)

| | |
|---|---|
| Status | **ADMITTED 2026-07-15** — a size-cap descent of `nika-cli`. W-COMP « the composition » added CLI surface that pushed `nika-cli` past the 15 000 prod-LOC hard cap; per the unit-target discipline (D-2026-07-09-N1, one architectural unit may span N workspace members — `nika-cli` stays the operator front door) the widest self-contained leaf descends. The `nika-source` / `nika-vocab` descents are the exact precedent. |
| Layer | **L0** — pure `std`, zero I/O, zero async, zero deps. |
| Design | The machine-applicable envelope migrations for `.nika.yaml` files, in five rungs: `r1-identity` (the nine-key envelope — `nika: v1` + `workflow: {id, description}` (block · scalar · flow) → `nika: <kebab-id>`, the description prose demoted to `#` comment lines above it, never dropped · since 0.109 · ADR-113), `w1` (the map — `tasks:` sequence → task map · its former `workflow:`-object and `description:`-hoist halves are RETIRED, their target died with the envelope), `w2` (equivalence-or-stop flow migration — `depends_on` + body `tasks.*` reads → `with:` bindings + `after:` predicates, stopping honestly with named diagnostics when the shape is ambiguous), `esplit` (the C2 four-authority flag-day — `vars:` classified into `inputs:`/`const:`) and `predicates` (the R5 outcome-class respelling — `succeeded`→`success` · `failed`→`failure` in `after:` blocks, 1:1 mechanical). These are the repairs `nika check --fix` applies when the parser refuses a dead-form document; the old form is repairable, never executable (there is no legacy parser path). Line-based and structure-aware: comments, blank lines and source order are preserved byte-for-byte outside the transformed shapes, and every wave is idempotent by contract (a migrated document returns `None`/`Clean`). | <!-- stale-ok: the codemod names its SOURCE form -->
| Name | `nika-migrate` (honest: the *migrations* the fix verb applies, not the fix verb itself). |
| LOC budget | ≤15000 src (admitted at ~900 prod LOC). ≤1500/file, ≤100/fn. |
| Deps | none — pure `std` string transforms (`std::collections::BTreeSet` for the W2 binding-name allocator). |
| Publish | `false` — foundation crate (ADR-017 class). |

## 1 · Why this crate exists (the split argument, not DRY)

`nika-cli` is the operator surface and grows with every verb wave. W-COMP
(spec 14 · `invoke: workflow:` · the composition lane) lands while the crate
sits at the constitutional 15 000 hard cap (15 228 prod LOC). The registry
says what happens next: **a size-cap split is one unit in N members** (never a
watered-down cap, never a waiver). The `nika-source` (2026-07-14) and
`nika-vocab` (2026-07-15) descents are the exact precedent.

`src/migrate.rs` was the widest self-contained leaf: a pure string→string
transform with **zero internal coupling** (no `crate::` / `super::` import in
the prod region) and a single consumer — the `fix` verb (`verbs/fix.rs`),
which calls `w1` / `w2` in its repair loop. It descends here unchanged; the
only call-site edit is the path (`crate::migrate::w1` → `nika_migrate::w1`),
behaviour byte-for-byte identical (the conformance suite pins every transform
and every refusal).

## 2 · Public API

```rust
/// Apply the W1 (map) migration. `Some(new)` when the document changed,
/// `None` when it is already in the new form (idempotence by contract).
pub fn w1(source: &str) -> Option<String>;

/// The W2 migration verdict.
pub enum W2Outcome {
    /// Mechanically migrated (equivalence preserved by rule).
    Changed(String),
    /// Ambiguous — each diagnostic names the case and its candidates.
    Stop(Vec<String>),
}

/// Apply the W2 (equivalence-or-stop) migration.
pub fn w2(source: &str) -> W2Outcome;

/// The E-split verdict (changed-or-stop, plus the idempotence class).
pub enum EsplitOutcome { /* Changed(String, Vec<String>) · Clean · Stop(Vec<String>) */ }

/// Apply the C2 E-split codemod (`vars:` → `inputs:`/`const:`).
pub fn esplit(source: &str) -> EsplitOutcome;

/// Apply the R5 predicate codemod (`succeeded`→`success` ·
/// `failed`→`failure` in `after:` blocks). `None` = idempotent-clean.
pub fn predicates(source: &str) -> Option<String>;
```

Everything else in the crate (the line scanners, the surgery planner, the
island/deps/with analysis, the flow/block value flippers) is private — the
functions and the two verdict enums are the whole surface the `fix` verb
names.

## 3 · Gates

Inherits the descent proofs: the split moved the file verbatim (`git mv` —
history preserved), the workspace re-proved green after the move (lib +
clippy `-D warnings`), and the public-api baseline lands via the CI artifact
lane (Ubuntu runner · never hand-authored). The migration conformance is
pinned by the W1/W2 fixtures the `check --fix` suite already carries.
