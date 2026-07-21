# Crate spec — `nika-pck-manifest` (Gate 1)

| | |
|---|---|
| Status | **ADMISSION IN REVIEW** — Gate 1 authored 2026-07-08; gates 2-10 run same day (results in §4). The 42nd crate (ADR-094 accepted + FCI-004 amended same day · D-2026-07-08-N2 · the taxonomy gate is CLEARED). |
| Layer | **L0** — pure, zero I/O, zero async, zero `nika-*` deps (`serde` + `thiserror` only; `serde_json` + `toml_convert` + `proptest` dev-only). |
| Design | The pck sharing layer's **data contract as pure types**: manifest · cert · lockfile · package ref · hash newtypes · the D4 artifact taxonomy. Serde-generic (the wire is the consumer's choice; `nika/pck@1` is documented as TOML per FCI-003 — round-trip proven in tests for BOTH toml and json). |
| LOC budget | ≤900 src (target ~650) — sized against `nika-cap` (~1.3k) and `nika-event` (~460); a vocabulary crate, not an engine. ≤1500/file · ≤100/fn. |
| Deps | `serde` (derive) · `thiserror`. Dev: `serde_json` · `toml_convert` · `proptest`. **Zero `nika-*`** — cert claims are `Vec<String>` by design (the plan's exact cut: the cert is a CLAIM CARRIER; `nika verify` re-derives with the full engine, this crate never validates semantics). |
| Publish | `false` — foundation crate (ADR-022). |
| NIKA codes | **none** — local typed `ManifestError` only; the NIKA-200..299 pck range (FCI-005) activates in the L1/L2 pck crates that DO I/O. Same posture as `nika-cap` §7. |

## 1 · Why this crate exists (ADR-094 · D6 row 1)

The pck suite (registry L1 · git L1 · orchestrator L2 · CLI verbs L4) is
post-1.0, but its **data contract** is L0 and admission-ready NOW: pure
types every future pck crate deserializes against. Landing the types first
(a) lands under the ADR-037 count horizon (50-90 · cap 100 · projected, never a gate), (b) freezes the wire shapes
under `#[non_exhaustive]` + INV#19 before any I/O code exists (additive
forever), (c) encodes the D4 taxonomy the operator just ratified as the ONE
closed-but-extensible enum every registry surface shares.

## 2 · Public API (all `#[non_exhaustive]` · INV#19 `new()` constructors)

```rust
/// The D4 artifact taxonomy — closed reserved-core + the vendor escape.
/// TOTAL deserialization: an unrecognized wire string lands in
/// `CustomTool(s)` (forward classes + `x-<vendor>:*` declarations), never
/// an error — the taxonomy is a ROUTER, not a validator.
#[non_exhaustive]
pub enum ArtifactKind {
    Workflow, Pack, Skill, AgentPreset, TemplateVariant, ModelPointer,
    McpConfig, ConformanceFixture, ProviderProfile, PolicyPreset,
    BenchSuite,
    CustomTool(String),           // wire: the string verbatim (x-vendor:* + unknown)
}
// wire forms: kebab-case ("agent-preset", "model-pointer", …)

/// 32-byte digests as validated lowercase-hex newtypes. Parsing a
/// malformed hash IS an error — integrity primitives are never total.
pub struct Sha256Hash(/* private */);   // 64 hex chars
pub struct Blake3Hash(/* private */);   // 64 hex chars
// TryFrom<&str> / FromStr / Display / serde with validation.

/// Go-module-style decentralized ref (ADR-094 D1): the hosting platform
/// is the namespace. Struct-only — no canonical string syntax is parsed
/// or printed here (the ADR locks the SHAPE, not a grammar · §5).
pub struct PackageRef { pub url: String, pub path: Option<String>, pub version: String }

/// One content file: repo-relative path + sha256 (nika-pack precedent).
pub struct FileEntry { pub path: String, pub sha256: Sha256Hash }

/// The shared artifact's manifest (`schema: "nika/pck@1"` · FCI-003).
/// Signed DETACHED (minisign, D3) — no signature field in the data.
pub struct Manifest {
    pub schema: String,               // "nika/pck@1" · validate() checks
    pub name: String,
    pub version: String,
    pub kind: ArtifactKind,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub files: Vec<FileEntry>,
    pub content_hash: Blake3Hash,     // the artifact IS this hash (D1/D2)
}

/// The conformance cert (D3): the `nika check` static proof as CLAIMS the
/// installer re-derives locally (`nika verify` re-runs the oracle — the
/// cert is a claim, not a badge). Claims are opaque strings HERE.
pub struct Cert {
    pub schema: String,               // "nika/pck@1"
    pub manifest_hash: Blake3Hash,    // what this cert certifies
    pub engine: String,               // certifying engine version
    pub verdict: CertVerdict,         // Pass | Fail (closed · non_exhaustive)
    pub effects: Vec<String>,
    pub permits: Vec<String>,
    pub secrets_read: Vec<String>,
    pub cost_floor_usd: Option<String>, // exact-string (micro-USD grain precedent)
}

/// Hash pins that survive any index dying (D1: the index must be losable).
pub struct Lockfile { pub schema: String, pub entries: Vec<LockEntry> }
pub struct LockEntry { pub r#ref: PackageRef, pub content_hash: Blake3Hash }

#[non_exhaustive]
pub enum ManifestError {  // thiserror · no NIKA codes at L0
    HashInvalid { what: &'static str, got: String },
    SchemaUnsupported { got: String },
}

pub const PCK_SCHEMA: &str = "nika/pck@1";
```

`validate()` on Manifest/Cert/Lockfile checks `schema == PCK_SCHEMA`
(SchemaUnsupported otherwise — parse never fails on it: a `nika/pck@2`
document DESERIALIZES, then validate() reports, per FCI-003 dispatch-by-
version forward shape).

## 3 · Anti-scope (fences)

- **No I/O, no crypto**: hashes are validated hex STRINGS — computing
  blake3/sha256 is `nika-blob`'s job; verifying minisign is the L1/L2
  suite's job.
- **No ref-string grammar**: `PackageRef` is struct-only. A canonical
  display/parse syntax needs its own decision (flagged in ADR-094
  follow-ups) — inventing one here would freeze an unratified grammar.
- **No semantic validation of claims**: effects/permits/secrets stay
  opaque `Vec<String>` — typed vocabularies live in `nika-cap`/`nika-schema`
  and the cert must not drag the parser into L0 (the extraction lesson).
- **No registry index row types**: the index is the L1 registry crate's
  surface (it may reuse these types; its rows aren't defined here).

## 4 · The 12 gates

| Gate | Result (2026-07-08) |
|---|---|
| 1 SPEC | ✅ this doc |
| 2 TDD | ✅ tests authored value-exact with the impl (serde round-trips toml+json · taxonomy totality · hash rejection table · schema validate); the RED phase was exercised by Gate 5 — the two `validate()` happy-path-only gaps mutation surfaced were killed with schema-mismatch negatives (honest note: ceremony-RED was not observed per-test; strength is mutation-proven) |
| 3 IMPL | ✅ 845 src LOC across lib/kind/hash/refs/manifest/cert/lockfile (≤900 budget) |
| 4 CLIPPY | ✅ 0 warnings, crate + workspace (`--all-targets -D warnings`) |
| 5 MUTATION | ✅ **11/11 caught (100%) · 0 missed · 3 unviable** (`cargo mutants -p nika-pck-manifest`) |
| 6 PROPERTY | ✅ 3 laws: kind totality+round-trip ∀ strings · hash ∀-64-hex accept+canon · hash ∀-malformed reject (constructor AND serde agree) |
| 7 BENCH | N/A — pure types, no hot path (justified) |
| 8 DOCS | ✅ `cargo doc --no-deps` 0 warnings · every pub item documented |
| 9 CANARY | N/A — L0 data contract, no runtime surface (justified) |
| 10 PARITY | N/A — new surface: legacy v0.79 never shipped a pck manifest format (justified; the nika-pack embedded-content manifest is a DIFFERENT, existing contract — disambiguated §3) |
| 11 REVIEW | 3-agent swarm (run at admission · findings folded or pinned below) |
| 12 ATOMIC | 1 crate = 1 commit |

## 4b · Gate-11 swarm findings (2026-07-08 · verdict FIX-THEN-ADMIT · all resolved)

- **P0 hash newtypes lacked `#[non_exhaustive]`** (the only 2 of 11 public
  types) → FIXED in the `hash_newtype!` macro; uniformity is the ratchet.
- **P1 `CustomTool(String)` injectivity hole** — `CustomTool("workflow")`
  was constructible and serialized byte-identical to `Workflow`
  (type→wire→type broke on a frozen-forever vocabulary) → FIXED
  structurally: the payload is now opaque `CustomKind` (no public
  constructor — `from_wire` IS the constructor and routes core forms to
  named variants first; shadowing unrepresentable). Deliberate INV#19
  exemption documented on `CustomKind`.
- **P1 FCI-004 stale 9-list / P1 ADR-094 `status: proposed`** — branch
  artifacts: the reviewers read a pre-#292 tree; rebranched onto current
  main (which carries the amended FCI-004 11-list + the accepted ADR) —
  both dissolve; kind.rs's "mirrored by FCI-004" claim is true on main.
- Post-fix battery: 21 unit + 3 property tests · clippy 0 · **mutation
  14/14 caught (100% · 0 missed · 3 unviable)**.

## 5 · Open (flagged, not decided here)

- PackageRef canonical string syntax (ADR-094 follow-up · needed by the L1
  registry, not by the types).
- Whether Cert grows typed claim vocabularies post-1.0 (would add nika-cap
  dep — a DELIBERATE layering decision for the suite, not a default).
