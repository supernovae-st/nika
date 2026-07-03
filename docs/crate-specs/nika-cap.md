# Crate spec — `nika-cap`

| | |
|---|---|
| Status | **PLANNED** — Gate 1 (this document) authored 2026-07-03, prior to TDD. Target tag `v0.93.0` per `ROADMAP.md` (40/42). |
| Layer | L0 — pure, zero I/O, zero async |
| Design | A pure vocabulary + set-algebra crate: the declared capability boundary (`permits:`, spec `01-envelope.md` §permits) as data + the "fits" predicate over it. No AST types, no workflow coupling. |
| LOC budget | ≤600 src (target ~480), ≤800 hard cap — sized against the closest L0 sibling, `nika-event` (~462 LOC), not against `nika-schema` (a monolith by different design constraints, see §2) |
| File cap | ≤1,500 LOC each (max file here ~150 — well under) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.93.0` at admission) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — foundation crate, never on crates.io (ADR-017), same as `nika-schema`, `nika-event` |
| Extraction source | `crates/nika-schema/src/types/permits.rs` (171 LOC, moved) + the pure fragment of `crates/nika-schema/src/check/permits_fit.rs` (~120 LOC of the 828-LOC file, moved) |
| NIKA codes | **none** — see §7 |

---

## 1. Purpose

`nika-cap` is the canonical home for the **declared capability boundary**
vocabulary — the `permits:` block (spec `01-envelope.md` §permits): the
workflow author's entire blast radius, declared in-file, as data (`Permits`,
`FsPermits`, `NetPermits`, `ExecPermit`), plus the **pure "fits" predicate**
over that data (`allows_exec` / `allows_program` / `allows_tool` /
`allows_host` / `allows_path`, and the set-algebra `union`/`intersect`
lattice operations over two boundaries).

It does **not** own:

- **The "escapes" diagnostic surface** (walking a `RawWorkflow`'s tasks,
  classifying each builtin's literal effect, producing `CapabilityEscape`
  findings with task ids / spans / machine-applicable fixes). That stays in
  `nika-schema::check::permits_fit` — it is inherently AST-coupled (needs
  `RawAction`, `RawInvokeAction`, `RawCommand`) and workflow-specific
  (diagnostic context), not a generic capability-token concept.
- **Runtime enforcement** (`nika-builtin::FsBoundary`, `nika-http::NetBoundary`).
  Both are I/O-resolved (symlink + `..` canonicalization against the real
  filesystem / DNS), a fundamentally different algorithm from the static
  lexical-only check `nika-cap` performs, and both are correctly already
  decoupled from the `Permits` type (they consume raw `Vec<String>` glob
  lists). **Nothing about the runtime path changes.**
- **Capability *inference*** (`nika-schema::check::infer_permits` — synthesizing
  the tightest boundary from a workflow's observed effects). That module
  becomes a *consumer* of `nika-cap`'s types (it constructs a `Permits`
  value), not a thing `nika-cap` needs to know about.

### Why this crate exists now (not before)

Three call sites today independently reimplement fragments of the same
glob-matching algebra: `permits_fit.rs` (`host_allowed`/`path_allowed`),
`declass.rs` (its own private `host_glob_matches` + `host_within_permits`,
doc-commented "*mirrors the permits-fit host matcher — same semantics on
both sides*" — an unenforced parity claim), and the type definitions
themselves living inside the 15k-LOC `nika-schema` monolith with no
independent test surface. Per ROADMAP.md this is also explicitly the next
L0-completion crate (`0.93.0` — 40/42) ahead of `nika-pck-contracts` and
`nika-binding-types`, and the natural landing spot for the **future**
`nika-policy` (L2, design-locked per `crate-layer-registry.md`) to consume
the boundary vocabulary **without pulling in the whole parser** the way
`nika-runtime` currently must (it already depends on `nika-schema` for
`RawAction`/`RawCommand`, so this crate does not reduce *its* transitive
footprint — but `nika-policy` and any future capability-aware crate get a
genuinely tiny, dependency-light L0 leaf instead).

---

## 2. Layer + LOC budget + cap strategy

**Layer:** L0 — zero I/O, zero async, zero tokio. Verified identically to
`nika-schema`'s own L0 constraint (no `std::fs`, no `std::net`, no `tokio`).

**LOC budget:** ≤600 src (hard cap 800). This is **not** sized against
`nika-schema` (a 15k-LOC monolith with a different design mandate — see
`nika-schema.md` §2 "why 1 crate, not split"). It is sized against the
closest-shaped sibling, `nika-event` (~462 LOC: pure vocabulary + a handful
of predicate methods, zero AST coupling, zero I/O). `nika-cap` is smaller in
kind: 4 DTOs + ~9 predicate/algebra methods + 2 free glob helpers.

**Cap strategy if approaching 600:** there is no plausible growth path
inside this crate's scope (the vocabulary is closed by the spec's
`permits:` shape — `fs`/`net`/`exec`/`tools`, 4 categories, frozen). If a
5th category is ever added to the spec, it lands additively (new
`Option<T>` field on `#[non_exhaustive] Permits`, new `allows_*` method) —
no escape-hatch table needed, unlike `nika-schema`'s.

---

## 3. Public API surface

### 3.1 The declared boundary (`permits.rs` — moved verbatim from `nika-schema/src/types/permits.rs`)

```rust
/// The declared capability boundary (spec `01-envelope.md` §permits).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Permits {
    pub fs: Option<FsPermits>,
    pub net: Option<NetPermits>,
    pub exec: Option<ExecPermit>,
    pub tools: Option<Vec<String>>,
}

impl Permits {
    #[must_use] pub fn new() -> Self;                 // permits: {} — pure compute
    #[must_use] pub fn allows_exec(&self) -> bool;
    #[must_use] pub fn allows_program(&self, program: &str) -> bool;
    #[must_use] pub fn allows_tool(&self, tool: &str) -> bool;
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FsPermits {
    #[serde(default)] pub read: Vec<String>,
    #[serde(default)] pub write: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct NetPermits {
    #[serde(default)] pub http: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ExecPermit { No, Any, Programs(Vec<String>) }

/// Gitignore-style glob match — exact, or a single trailing `*` matching
/// any (possibly empty) tail. Promoted `pub(crate)` → `pub` (nika-schema
/// is now an external consumer).
#[must_use]
pub fn glob_matches(glob: &str, value: &str) -> bool;
```

**Every derive is preserved byte-for-byte** from the current
`nika-schema` definition. This is load-bearing for §6 (the non-breaking
migration) — any drift in derives changes the `#[non_exhaustive]` /
`Serialize` surface and would show as a real `cargo public-api` diff
instead of a no-op re-export.

### 3.2 The "fits" predicate over literal atoms (`fit.rs` — NEW pub surface, logic moved from `nika-schema/src/check/permits_fit.rs` private fns)

```rust
impl Permits {
    /// Whether `host` matches the declared `permits.net.http` allowlist.
    /// Default-deny: an omitted `net` block forbids all hosts.
    #[must_use] pub fn allows_host(&self, host: &str) -> bool;

    /// Whether `path` matches the declared `permits.fs` allowlist for the
    /// direction (`write` selects `fs.write`, else `fs.read`). Default-deny:
    /// an omitted `fs` block forbids all paths. Traversal-safe: BOTH sides
    /// are lexically normalized (`.`/`..` folded) before comparison, so a
    /// `..` that climbs OUT of the glob's literal prefix does not
    /// string-match it. (This is the STATIC half only — the runtime
    /// canonicalize-then-confine check remains required for symlinks and
    /// `${{ }}`-built paths.)
    #[must_use] pub fn allows_path(&self, path: &str, write: bool) -> bool;
}

/// Gitignore-style path glob match — supports a trailing `/**` (any
/// descendant) and a single `*` (any tail within a segment). Conservative:
/// when in doubt it does NOT match.
#[must_use] pub fn path_glob_matches(glob: &str, path: &str) -> bool;

/// Fold `.`/`..` segments textually, preserving a leading `/` or `./`.
#[must_use] pub fn lexically_normalize(path: &str) -> String;
```

`allows_host` depends on `nika_types::net::host_glob_matches` — the **same**
matcher `nika-http` enforces at runtime (already a shared dependency
today; zero new coupling, preserves the existing "check-time and run-time
verdicts can't drift" guarantee documented in `permits_fit.rs`).

### 3.3 The lattice operations (`algebra.rs` — NEW)

```rust
impl Permits {
    /// The loosest boundary that admits everything either operand admits
    /// (join). Component-wise list union for `fs`/`net`/`tools`;
    /// `exec` takes the more-permissive of the two tri-states
    /// (`No < Programs(A∪B) < Any`).
    #[must_use] pub fn union(&self, other: &Self) -> Self;

    /// The tightest boundary that admits only what BOTH operands admit
    /// (meet). Component-wise list intersection for `fs`/`net`/`tools`;
    /// `exec` takes the less-permissive of the two (`No` absorbs;
    /// `Programs(A) ∩ Programs(B)`; `Any ∩ X = X`).
    #[must_use] pub fn intersect(&self, other: &Self) -> Self;
}
```

**Why ship these now with no wired caller** (FCI-001 "kernel traits
upfront, implementations deferred" spirit — already this codebase's own
pattern): `intersect` is exactly the ceiling-composition primitive a future
`nika-policy` (L2, design-locked) needs ("workflow declares X, operator
policy allows Y, effective = X∩Y") and `union` is the natural dual for
combining declared boundaries across included sub-workflows. Shipping
the pure algebra today, unwired, costs ~110 LOC and mirrors the
reservations this codebase already carries for `InferRequest`/`CatalogEntry`.
Wiring either into `nika-policy` is explicitly **out of scope** for this
admission.

---

## 4. Module structure with LOC estimates

```
crates/nika-cap/
  Cargo.toml
  src/
    lib.rs          (~40 LOC  — crate docs, re-exports, no logic)
    permits.rs       (~200 LOC — Permits/FsPermits/NetPermits/ExecPermit
                                 + allows_exec/allows_program/allows_tool
                                 + glob_matches; moved from nika-schema)
    fit.rs            (~140 LOC — allows_host/allows_path
                                 + path_glob_matches/lexically_normalize;
                                 moved+generalized from permits_fit.rs)
    algebra.rs         (~110 LOC — union/intersect; NEW)
  tests/
    algebra_properties.rs   (~150 LOC — the proptest suite, §8)
```

LOC summary: ~490 src (within budget), tests separate per the Gate 2/6
convention already used by `nika-schema`.

---

## 5. Dependencies

```toml
[dependencies]
nika-types = { path = "../nika-types", version = "0.93.0" }  # host_glob_matches — shared with the nika-http runtime matcher, zero drift
serde      = { workspace = true, features = ["derive"] }

[dev-dependencies]
proptest   = { workspace = true }
serde_json = { workspace = true }   # serde round-trip regression (Permits ⇄ JSON)

[lints]
workspace = true
```

### L0 constraint verification

- Zero I/O: no `std::fs`, no `std::net`, no `tokio`.
- Zero async.
- `nika-types` is the foundation leaf (no deps) — a safe L0→L0(leaf)
  downward dependency, identical to `nika-schema`'s existing pattern.
- No `url` crate: `url_host()` (extracting a host from a builtin's literal
  URL arg) stays in `nika-schema` — it is inherently coupled to
  `RawInvokeAction`/`literal_arg` extraction, not a boundary-fits concept.

---

## 6. The migration (additive, zero breaking change)

**Constraint**: every existing caller must keep compiling and passing,
unmodified, unless explicitly migrated as a follow-up. The mechanism is a
**re-export at the identical module path**.

### Step 1 — scaffold `nika-cap`, move `permits.rs` verbatim

Add `"crates/nika-cap"` to workspace `members`, add `layers.nika-cap =
"L0"` to `Cargo.toml` (`docs/architecture/crate-layer-registry.md` L0
bracket gets a new entry). Move
`Permits`/`FsPermits`/`NetPermits`/`ExecPermit`/`glob_matches` into
`nika-cap/src/permits.rs` **byte-identical** (same derives, same doc
comments, only visibility of `glob_matches` widens `pub(crate)` → `pub`).
Port the 3 existing inline tests unmodified.

### Step 2 — add `fit.rs`

Move `host_allowed`/`path_allowed`/`path_glob_matches`/`lexically_normalize`
out of `nika-schema/src/check/permits_fit.rs`, generalize the two
`Permits`-taking private fns into public methods (`allows_host`,
`allows_path`), write **new** direct unit tests for them (today they are
only exercised indirectly through `scan_escapes(parse(yaml))` — this
extraction is the first time these functions get tested in isolation:
a net testing improvement, not just a move).

### Step 3 — add `algebra.rs` + the proptest suite (§8, NEW code)

### Step 4 — wire `nika-schema` as a thin re-exporter

```rust
// crates/nika-schema/src/types/permits.rs — AFTER
//! Re-exported from `nika-cap` (extracted — the canonical home for the
//! `permits:` capability-boundary vocabulary). Kept at this module path
//! so every existing `nika_schema::types::permits::*` / `crate::types::
//! Permits` import continues to resolve unchanged.
pub use nika_cap::{ExecPermit, FsPermits, NetPermits, Permits, glob_matches};
```

Add `nika-cap` to `nika-schema`'s `[dependencies]`. In
`check/permits_fit.rs`: delete the now-duplicated private
`host_allowed`/`path_allowed`/`path_glob_matches`/`lexically_normalize`,
replace their 2 call sites with `permits.allows_host(&host)` /
`permits.allows_path(&path, writes)`. **Every existing test in
`permits_fit.rs` (the `tests`, `fs_net_regression`, `argv_program_check`
modules) must pass unmodified** — the behavior-parity bar for the whole
migration. `check/infer_permits.rs` needs **no changes** (it already
imports via `crate::types::{…}`, which resolves through the re-export).

### Step 4b (optional, same-arc if time permits — else a follow-up commit)

`check/declass.rs` carries a **third**, doc-commented-as-parity-risk copy
of the host-glob algebra (`host_glob_matches` + `host_within_permits`).
Once `Permits::allows_host` exists, `host_within_permits` collapses to
`permits.is_none_or(|p| p.allows_host(host))` and the local fn is
deleted. **Not required** for this admission — if skipped, update
`declass.rs`'s "mirrors the permits-fit host matcher" comment to point at
`nika_cap::Permits::allows_host` so the parity claim stays truthful.

### Step 5 — `nika-runtime` (optional, deferred by default)

`task.rs`/`expr.rs`/`dispatch.rs` currently import via
`nika_schema::types` — this **continues to compile unchanged** after
Step 4. Migrating them to a direct `nika_cap` import is a pure
import-path cleanup, recommended as a **separate, later commit** to keep
this admission's diff scope-locked (1 crate = 1 commit).

### Step 6 — `nika-cli` (zero changes)

`crates/nika-cli/src/verbs/run/compose.rs` never explicitly imports the
`Permits` type name — field access only through `RawWorkflow.permits`.

### Step 7 — registry + status updates

`Cargo.toml` workspace `members` + `layers.nika-cap = "L0"`;
`docs/architecture/crate-layer-registry.md` L0 row; `.claude/CLAUDE.md`
auto-block via `scripts/refresh-status.sh` (crate count 39→40 — never
hand-edit the numbers).

### Verification (the acceptance bar)

```bash
cargo test --workspace --lib          # ALL prior nika-schema tests GREEN, unmodified
cargo clippy --workspace --all-targets -- -D warnings
cargo public-api --diff-git-checkouts main HEAD   # Permits surface shows as re-export, NOT removal
cargo semver-checks check-release      # clean — field/derive shapes byte-identical
```

---

## 7. Error codes — none needed

`Permits` and its full algebra (`allows_*`, `union`, `intersect`) are
**100% infallible**: every method returns `bool` or `Self`, never
`Result`. Verified against `nika-schema/src/error.rs`'s complete
`SchemaError` enum (`NIKA-280..329`) — there is no `BadPermits` variant
today, because malformed `permits:` YAML falls through the existing
generic parse-level paths, not inside the `Permits` type itself.

**Do not** reserve `Category::Sandbox` (`NIKA-750..799`, FCI-005) for
this crate — that range is earmarked for the *unrelated* future
WASM/Landlock/seccomp sandbox subsystem (`crate-layer-registry.md` L3
`nika-sandbox`). Conflating the two "capability" concepts (declared
workflow blast-radius vs OS-level sandbox enforcement) would be a real
doctrine violation, not a naming nicety.

If a future fallible constructor is ever added (e.g. `parse_strict` with
cross-field validation), request a **fresh** `Category` variant at that
time — explicitly deferred.

---

## 8. Proptest plan — the set-algebra laws (Gate 6)

### 8.1 Flagship law — the lattice ordering

```
∀ p1 p2: Permits, ∀ atom a:
    intersect(p1, p2).allows(a)  ⟹  p1.allows(a)  ⟹  union(p1, p2).allows(a)
                                  ⟹  p2.allows(a)  ⟹  union(p1, p2).allows(a)
```

tested once per category (`allows_tool`, `allows_host`, `allows_path`
read, `allows_path` write, `allows_program`) via a shared generic helper
parameterized over the 5 predicate closures.

### 8.2 Component lemmas

| Property fn | Law |
|---|---|
| `prop_empty_boundary_denies_everything` | `Permits::new()` (`permits: {}`) denies arbitrary tool/program/host/path atoms — the bottom element ⊥ |
| `prop_any_exec_allows_every_program` | `ExecPermit::Any` ⇒ `allows_program(p)` true ∀ arbitrary strings — the top element for the exec axis |
| `prop_no_exec_denies_every_program` | `None` / `ExecPermit::No` ⇒ `allows_program` false ∀ strings, and `allows_exec() == false` |
| `prop_exact_glob_matches_iff_equal` | a no-`*` glob matches iff byte-identical to the candidate |
| `prop_trailing_star_matches_prefix_only` | `"<prefix>*"` matches iff `candidate.starts_with(prefix)` |
| `prop_widening_is_monotone` | appending an arbitrary extra entry to any glob list never revokes a previously-allowed atom — ⊆-monotonicity |
| `prop_union_is_the_join` | `union(p1,p2).allows(a) == p1.allows(a) OR p2.allows(a)`, across all 5 predicates |
| `prop_intersect_is_the_meet` | `intersect(p1,p2).allows(a) == p1.allows(a) AND p2.allows(a)`, dual law |
| `prop_union_intersect_algebraic_sanity` | commutativity + idempotence for both operations |

### 8.3 Explicitly deferred from property-testing (kept as example tests only)

Path-traversal lexical-normalization edge cases (`./out/../escape.txt`)
are **ported as example tests** (from `permits_fit.rs`'s
`fs_net_regression` module) rather than generalized into a path-algebra
generator — the existing example coverage is rich and precise, and a
sound arbitrary-path-with-traversal generator is a bigger investment than
this crate's scope justifies today (stretch goal, not a Gate-6 blocker).

---

## 9. Gate exemptions (justified)

| Gate | Status | Justification |
|---|---|---|
| 7 BENCHMARKS | **N/A** | pure value types, no hot path — identical justification to `nika-event`. The static-check surface this feeds is already benchmarked holistically at the `nika-schema` level (`benches/parse_bench.rs`). |
| 9 CANARY E2E | **N/A** | L0 types, no `.nika.yaml` runtime surface — identical justification to `nika-event`. |
| 10 PARITY LEGACY | **N/A (stronger than usual)** | No brouillon `nika-cap` equivalent — confirmed empirically. The `permits:` concept is **post-brouillon, CRAFT-fresh** (ADR-001): the legacy v0.79 engine had no capability-boundary block at all. Nothing to round-trip against. |

Gates 1-6, 8, 11, 12 apply in full, no exemptions. Gate 5 (mutation ≥90%)
should be a single pass given the crate's size (~490 LOC).

---

## 10. Consumers (downstream)

- **`nika-schema`** (L0) — re-exports the types at the same module path
  (§6 Step 4); `check/permits_fit.rs` and `check/infer_permits.rs` become
  callers of the pure predicate instead of owning it.
- **`nika-runtime`** (L3) — exec-sink `NIKA-SEC-004` enforcement;
  direct-import migration optional/deferred (§6 Step 5).
- **`nika-cli`** (L4) — field access only, zero changes.
- **Future `nika-policy`** (L2, design-locked) — the motivating consumer
  for `union`/`intersect`: composing a workflow's declared boundary with
  an operator-level ceiling policy without depending on the whole parser.

---

## 11. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-07-03 | Gate 1 (architect pass) | Initial spec. Extraction scope locked: the 4 permits DTOs + the pure fits predicate + the NEW union/intersect lattice. AST-coupled escapes diagnostics stay in `nika-schema`. Runtime enforcement untouched. Zero NIKA-CAP codes (infallible algebra). Non-breaking migration via same-path re-export. |

🦋


## ADR-027 fan-out consequence (extraction cost)

Extracting `nika-cap` adds a **4th** L0 sibling-dep to `nika-schema` (it now
depends on `nika-cap` for the permits vocabulary it used to define inline),
crossing the ADR-027 3-sibling-dep cap for that one crate. This is exempted at
`crates/nika-schema/Cargo.toml` (`# L0-DEP-FANOUT-EXEMPT`) because `nika-schema`
is the schema/parser/analyzer **assembler** — a legitimate high-fan-in point for
the workflow vocabulary leaves (types · error · catalog · cap). The exemption is
the conscious DAG decision ADR-027 requires; the alternative (keeping permits
inline) would deny `nika-policy`/runtime the lean, parser-free reuse that is the
whole reason to extract `nika-cap`.
